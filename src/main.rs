use llama_cpp_2::context::params::LlamaContextParams;
use llama_cpp_2::llama_backend::LlamaBackend;
use llama_cpp_2::llama_batch::LlamaBatch;
use llama_cpp_2::model::params::LlamaModelParams;
use llama_cpp_2::model::{AddBos, Special};
use llama_cpp_2::model::LlamaModel;
use llama_cpp_2::sampling::LlamaSampler;

use serde::Deserialize;
use std::fs::File;
use std::fs::OpenOptions;
use std::io::{BufRead, BufReader, Write};
use std::num::NonZeroU32;
use std::thread::{sleep, yield_now};
use std::time::{Duration, Instant};

#[derive(Debug, Deserialize)]
struct FileMeta {
    id: String,
    path: String,
    size_bytes: u64,
    owner: String,
    group: String,
    permissions: String,
    is_dir: bool,
    extension: String,
    created_utc: String,
    modified_utc: String,
    accessed_utc: String,
    location_hint: String,
    category_hint: String,
    origin_hint: String,
    source: String,
}

// -----------------------------------------------------------
// Windows: pin process to CPU 0 + set process priority to IDLE
// (keeps the agent "background" from the OS scheduler's POV)
// -----------------------------------------------------------
#[cfg(target_os = "windows")]
fn set_windows_background_mode() {
    // Requires windows-sys in Cargo.toml:
    // windows-sys = { version = "0.52", features = ["Win32_System_Threading"] }
    use windows_sys::Win32::System::Threading::{
        GetCurrentProcess, SetPriorityClass, SetProcessAffinityMask, IDLE_PRIORITY_CLASS,
    };

    unsafe {
        let proc = GetCurrentProcess();
        // Affinity mask bit 0 => CPU 0 only
        let _ = SetProcessAffinityMask(proc, 1);
        let _ = SetPriorityClass(proc, IDLE_PRIORITY_CLASS);
    }
}

// -----------------------------------------------------------
// CPU noise-floor throttle: enforce a target duty cycle.
// Example: target = 0.05 means "work 5% of the time, sleep 95%".
// -----------------------------------------------------------
fn throttle_after_active(active: Duration, target_duty: f32) {
    let target = target_duty.clamp(0.01, 0.50);

    // sleep = active * (1-target)/target
    let active_ns = active.as_nanos() as f64;
    let mut sleep_ns = active_ns * ((1.0 - target as f64) / target as f64);

    // Windows sleep granularity can be coarse; enforce a minimum to smooth spikes
    // (prevents bursty "run hard, sleep tiny" behavior)
    sleep_ns = sleep_ns.max(20_000_000.0); // >= 20ms

    yield_now();
    sleep(Duration::from_nanos(sleep_ns as u64));
}

// -------------------------------------------
// Helper: run one prompt and return keep/delete
// -------------------------------------------
fn run_one_prompt(
    model: &LlamaModel,
    backend: &LlamaBackend,
    ctx_params: &LlamaContextParams,
    prompt: &str,
) -> anyhow::Result<String> {
    // ---- tunable knobs (background-safe) ----
    const TARGET_DUTY: f32 = 0.05; // 5% duty cycle target
    const MAX_TOKENS: usize = 3;   // minimal output for keep/delete
    // ----------------------------------------

    // NOTE: Creating a new context per record can spike CPU.
    // We throttle context creation too to reduce bursts.
    let t0 = Instant::now();
    let mut ctx = model.new_context(backend, ctx_params.clone())?;
    throttle_after_active(t0.elapsed(), TARGET_DUTY);

    // Tokenization can also spike; throttle it too.
    let t1 = Instant::now();
    let tokens = model.str_to_token(prompt, AddBos::Always)?;
    throttle_after_active(t1.elapsed(), TARGET_DUTY);

    // Small batch since we feed 1 token at a time
    let mut batch = LlamaBatch::new(32, 1);

    // -------------------------
    // Prefill prompt (1 token per decode to avoid spikes)
    // -------------------------
    for (pos, &token) in tokens.iter().enumerate() {
        let pos = pos as i32;
        let needs_logits = pos == (tokens.len() as i32 - 1);

        batch.clear();
        batch.add(token, pos, &[0], needs_logits)?;

        let td = Instant::now();
        ctx.decode(&mut batch)?;
        throttle_after_active(td.elapsed(), TARGET_DUTY);
    }

    let mut n_cur = tokens.len() as i32;

    let mut sampler = LlamaSampler::greedy();
    let mut decoder = encoding_rs::UTF_8.new_decoder();

    // Single-token decode => logits index is always 0
    let mut logits_idx = 0;

    let mut out = String::new();

    // -------------------------
    // Generate minimal output (1 token per decode)
    // -------------------------
    for _ in 0..MAX_TOKENS {
        let token = sampler.sample(&ctx, logits_idx);
        sampler.accept(token);

        if model.is_eog_token(token) {
            break;
        }

        let bytes = model.token_to_bytes(token, Special::Tokenize)?;
        let mut s = String::with_capacity(32);
        let _ = decoder.decode_to_string(&bytes, &mut s, false);
        out.push_str(&s);

        // stop early once we clearly have keep/delete
        let normalized = out.trim().to_ascii_lowercase();
        if normalized.starts_with("keep") {
            return Ok("keep".to_string());
        }
        if normalized.starts_with("delete") {
            return Ok("delete".to_string());
        }

        // next token
        batch.clear();
        batch.add(token, n_cur, &[0], true)?;
        n_cur += 1;

        let td = Instant::now();
        ctx.decode(&mut batch)?;
        throttle_after_active(td.elapsed(), TARGET_DUTY);

        logits_idx = 0;
    }

    // fallback if model outputs something weird (safe default)
    Ok("keep".to_string())
}

fn main() -> anyhow::Result<()> {
    // -------------------------------
    // CPU pinning & priority (Windows)
    // -------------------------------
    #[cfg(target_os = "windows")]
    {
        set_windows_background_mode();
    }

    // -------------------------------
    // CPU pinning & priority (Linux)
    // -------------------------------
    #[cfg(target_os = "linux")]
    unsafe {
        let mut cpu_set: libc::cpu_set_t = std::mem::zeroed();
        libc::CPU_SET(0, &mut cpu_set);
        libc::sched_setaffinity(0, std::mem::size_of::<libc::cpu_set_t>(), &cpu_set);

        let nice_value = 19;
        libc::setpriority(libc::PRIO_PROCESS, 0, nice_value);
    }

    // -------------------------------
    // llama.cpp backend + model
    // -------------------------------
    let backend = LlamaBackend::init()?;

    let model_params = LlamaModelParams::default();

    let model = LlamaModel::load_from_file(
        &backend,
        "models/llama-3.2-3b-instruct-q8_0.gguf",
        &model_params,
    )?;

    // keep ctx small for CPU/noise-floor testing
    let ctx_params = LlamaContextParams::default()
        .with_n_ctx(Some(NonZeroU32::new(512).unwrap())) // smaller ctx = less work
        .with_n_threads(1)                               // HARD cap threads
        .with_n_threads_batch(1);

    // -------------------------------
    // Report output
    // -------------------------------
    let mut report = OpenOptions::new()
        .create(true)
        .truncate(true)
        .write(true)
        .open("data/llm_report.tsv")?;
    writeln!(report, "id\tpath\tdecision")?;

    // -------------------------------
    // Read demo metadata + run LLM
    // -------------------------------
    let file = File::open("data/demo_metadata_clean.jsonl")?;
    let reader = BufReader::new(file);

    println!("Running LLM over demo metadata...\n");

    for line in reader.lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }

        let meta: FileMeta = serde_json::from_str(&line)?;

        let prompt = format!(
            "You are a local, offline file classification engine used for safe disk cleanup.\n\
Your task is to decide whether a file should be KEPT or DELETED based ONLY on the metadata provided.\n\
\n\
HARD RULES:\n\
- Use metadata only. Do NOT assume file contents.\n\
- Be conservative. If uncertain, choose KEEP.\n\
- Do NOT explain your decision.\n\
- Output exactly ONE lowercase word: keep or delete\n\
\n\
FILE METADATA:\n\
Path: {path}\n\
Size_bytes: {size}\n\
Owner: {owner}\n\
Group: {group}\n\
Permissions: {perm}\n\
Is_dir: {is_dir}\n\
Extension: {ext}\n\
Created_utc: {c}\n\
Modified_utc: {m}\n\
Accessed_utc: {a}\n\
Location_hint: {loc}\n\
Category_hint: {cat}\n\
Origin_hint: {orig}\n\
\n\
Decision:",
            path = meta.path,
            size = meta.size_bytes,
            owner = meta.owner,
            group = meta.group,
            perm = meta.permissions,
            is_dir = meta.is_dir,
            ext = meta.extension,
            c = meta.created_utc,
            m = meta.modified_utc,
            a = meta.accessed_utc,
            loc = meta.location_hint,
            cat = meta.category_hint,
            orig = meta.origin_hint,
        );

        let decision = run_one_prompt(&model, &backend, &ctx_params, &prompt)?;

        writeln!(report, "{}\t{}\t{}", meta.id, meta.path, decision)?;
        println!("{} -> {}", meta.id, decision);
    }

    println!("\nDone. Wrote: data/llm_report.tsv");
    Ok(())
}
