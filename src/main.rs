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
use std::thread::sleep;
use std::time::Duration;

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

// -------------------------------------------
// Helper: run one prompt and return keep/delete
// -------------------------------------------
fn run_one_prompt(
    model: &LlamaModel,
    backend: &LlamaBackend,
    ctx_params: &LlamaContextParams,
    prompt: &str,
) -> anyhow::Result<String> {
    // fresh context per record (prevents prompt bleed)
    let mut ctx = model.new_context(backend, ctx_params.clone())?;

    let tokens = model.str_to_token(prompt, AddBos::Always)?;

    // ---- tunable knobs (conservative + low CPU) ----
    let chunk_size: usize = 8;
    let chunk_delay = Duration::from_millis(250);
    let gen_delay = Duration::from_millis(0);
    let max_tokens = 6;
    // ------------------------------------------------

    let mut batch = LlamaBatch::new(1024, 1);
    let chunks: Vec<&[llama_cpp_2::token::LlamaToken]> = tokens.chunks(chunk_size).collect();
    let num_chunks = chunks.len();

    let mut last_chunk_len: i32 = 0;

    // prefill prompt
    for (chunk_idx, chunk) in chunks.iter().enumerate() {
        let is_last_chunk = chunk_idx == num_chunks - 1;

        for (i, &token) in chunk.iter().enumerate() {
            let pos = (chunk_idx * chunk_size + i) as i32;
            let needs_logits = is_last_chunk && i == chunk.len() - 1;
            batch.add(token, pos, &[0], needs_logits)?;
        }

        ctx.decode(&mut batch)?;
        batch.clear();
        last_chunk_len = chunk.len() as i32;

        if !is_last_chunk {
            sleep(chunk_delay);
        }
    }

    let mut n_cur = tokens.len() as i32;

    let mut sampler = LlamaSampler::greedy();
    let mut decoder = encoding_rs::UTF_8.new_decoder();

    // after prefill, logits were requested on last token of last chunk
    let mut logits_idx = last_chunk_len - 1;

    let mut out = String::new();

    for _ in 0..max_tokens {
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
        logits_idx = 0; // single-token batch => logits at index 0

        ctx.decode(&mut batch)?;

        if !gen_delay.is_zero() {
            sleep(gen_delay);
        }
    }

    // fallback if model outputs something weird (safe default)
    Ok("keep".to_string())
}

fn main() -> anyhow::Result<()> {
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
        .with_n_ctx(Some(NonZeroU32::new(1024).unwrap()))
        .with_n_threads(1)
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