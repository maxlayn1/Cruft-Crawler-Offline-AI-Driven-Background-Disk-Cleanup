// Receives FileMeta from crawler, builds prompt, runs LLM, sends FileDecision to UI.

#![allow(unused)]

use steady_state::*;
use std::error::Error as StdError;

use crate::llm_engine::LlmEngine;
<<<<<<< Updated upstream

use llama_cpp_2::context::params::LlamaContextParams;
use llama_cpp_2::llama_backend::LlamaBackend;
use llama_cpp_2::llama_batch::LlamaBatch;
use llama_cpp_2::model::params::LlamaModelParams;
use llama_cpp_2::model::LlamaModel;
use llama_cpp_2::model::AddBos;
use llama_cpp_2::sampling::LlamaSampler;
use std::io::Write;
use std::num::NonZeroU32;
use std::{any, fs};

use crate::actor::crawler;
=======
use crate::actor::crawler::FileMeta;
use crate::file_decision::FileDecision;
>>>>>>> Stashed changes

const MODEL_FILE_PATH: &str  = "./src/models/Llama-3.2-3B-Instruct-UD-Q4_K_XL.gguf";
const PROMPT_FILE_PATH: &str = "./src/prompt.txt";

// ── Channel types updated:
//   IN:  FileMeta      (from crawler — real file metadata)
//   OUT: FileDecision  (to UI       — metadata + AI verdict)
pub async fn run(
    actor: SteadyActorShadow,
    crawler_to_model_rx: SteadyRx<FileMeta>,
    ai_model_to_ui_tx:   SteadyTx<FileDecision>,
) -> Result<(), Box<dyn StdError>> {

    let actor = actor.into_spotlight([&crawler_to_model_rx], [&ai_model_to_ui_tx]);

    if actor.use_internal_behavior {
        internal_behavior(actor, crawler_to_model_rx, ai_model_to_ui_tx).await
    } else {
        actor.simulated_behavior(vec![&crawler_to_model_rx]).await
    }
}

async fn internal_behavior<A: SteadyActor>(
    mut actor: A,
    crawler_to_ai_model_rx: SteadyRx<FileMeta>,
    ai_model_to_ui_tx:      SteadyTx<FileDecision>,
) -> Result<(), Box<dyn StdError>> {

    let mut rx = crawler_to_ai_model_rx.lock().await;
    let mut tx = ai_model_to_ui_tx.lock().await;

    // --- Windows-safe absolute path for model ---
    println!("AI_MODEL: starting, loading model from {}", MODEL_FILE_PATH);

<<<<<<< Updated upstream
    let prompt1 = fs::read_to_string(PROMPT_FILE_PATH)?;
    let resp1 = engine.infer_model(&prompt1)?;
    //println!("Response 1:\n{}", resp1);
   
    
    while actor.is_running(|| crawler_to_ai_model_rx.is_closed_and_empty() || ai_model_to_ui_tx.mark_closed()) {
		
		await_for_all!(actor.wait_avail(&mut crawler_to_ai_model_rx, 1), actor.wait_vacant(&mut ai_model_to_ui_tx, 1));

		let resp1 = engine.infer_model(&prompt1)?;
		
	    //Reciecing data from crawler actor
        let received = match actor.try_take(&mut crawler_to_ai_model_rx) {
			Some(msg) => msg,
			None => {
				// channel closed or no message despite wait; decide what to do
				// e.g. just skip this loop iteration:
				continue;
			}
		};
=======
    let abs_model_path = std::fs::canonicalize(MODEL_FILE_PATH)
        .expect("AI_MODEL: could not resolve model path");

    // Strip Windows extended-length prefix \\?\ that llama.cpp's C backend can't handle
    let abs_model_path_str = abs_model_path.to_string_lossy().to_string();
    let abs_model_path_str = abs_model_path_str
        .strip_prefix(r"\\?\")
        .unwrap_or(&abs_model_path_str)
        .to_string();
>>>>>>> Stashed changes

    println!("AI_MODEL: resolved model path = {:?}", abs_model_path_str);

    // Optional sanity check: ensure file exists and show size
    match std::fs::metadata(&abs_model_path_str) {
        Ok(md) => println!("AI_MODEL: model file exists, size = {} bytes", md.len()),
        Err(e) => {
            eprintln!("AI_MODEL: model file metadata read failed: {}", e);
            return Err(Box::new(e));
        }
    }

    // Load model once at startup — keep it alive for the whole run
    let engine = match LlmEngine::load_new_model(&abs_model_path_str) {
        Ok(e) => {
            println!("AI_MODEL: model loaded successfully");
            e
        }
        Err(e) => {
            eprintln!("AI_MODEL: FAILED to load model: {}", e);
            return Err(e.into());
        }
    };

    // --- Windows-safe absolute path for prompt ---
    println!("AI_MODEL: reading prompt template from {}", PROMPT_FILE_PATH);

    let abs_prompt_path = std::fs::canonicalize(PROMPT_FILE_PATH)
        .expect("AI_MODEL: could not resolve prompt path");
    println!("AI_MODEL: resolved prompt path = {:?}", abs_prompt_path);

    let prompt_template = match std::fs::read_to_string(&abs_prompt_path) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("AI_MODEL: FAILED to read prompt file: {}", e);
            return Err(Box::new(e));
        }
    };

    println!("AI_MODEL: ready, entering classification loop");

    // IMPORTANT CHANGE:
    // Don’t call tx.mark_closed() in the loop guard (destructive side effect).
    // Loop based only on rx state; close tx once at the end.
    while actor.is_running(|| rx.is_closed_and_empty()) {

        await_for_all!(actor.wait_avail(&mut rx, 1), actor.wait_vacant(&mut tx, 1));

        let meta = match actor.try_take(&mut rx) {
            Some(m) => m,
            None    => continue,
        };

        // Build a prompt by injecting real metadata into the template.
        // The template already has a "Decision:" marker at the end (see prompt.txt).
        let filled_prompt = build_prompt(&prompt_template, &meta);

        // Write filled prompt to metadata.txt for debugging (matches existing code pattern)
        let _ = std::fs::write("metadata.txt", &filled_prompt);

        // Run inference
        let raw = match engine.infer_model(&filled_prompt) {
            Ok(output) => output,
            Err(e) => {
                eprintln!("AI_MODEL: inference failed for '{}': {}", meta.file_name, e);
                "pending".to_string()  // mark as pending instead of crashing
            }
        };

        // Parse LLM output — trim whitespace, lowercase, default to "pending" if unclear
        let (decision, reason) = parse_llm_output(&raw, &meta);

        let file_decision = FileDecision {
            meta,
            ai_decision: decision,
            ai_reason:   reason,
        };

        match actor.try_send(&mut tx, file_decision) {
            SendOutcome::Success    => {}
            SendOutcome::Blocked(_) => { eprintln!("AI_MODEL: channel to UI blocked"); }
            SendOutcome::Timeout(_) => { eprintln!("AI_MODEL: channel to UI timeout"); }
            SendOutcome::Closed(_)  => { break; }
        }
    }

    // Close outgoing channel once, explicitly, at end
    tx.mark_closed();

    Ok(())
}

// Inject real FileMeta values into the prompt template.
// Replaces the placeholder example values with actual file data.
fn build_prompt(template: &str, meta: &FileMeta) -> String {
    // The template has a "Decision:" marker — split there so we insert
    // metadata right before the model is asked to decide.
    let marker = "Decision:";
    let parts: Vec<&str> = template.splitn(2, marker).collect();

    let meta_block = format!(
        "File metadata:\n\
         - Name: {}\n\
         - Size: {} bytes\n\
         - Last modified (unix seconds / 60): {}\n\
         - Read-only: {}\n\
         - Is file: {}\n\
         - SHA-256 (first 1KB): {}\n\
         - Path: {}\n\n",
        meta.file_name,
        meta.size,
        meta.modified,
        meta.readonly,
        meta.is_file,
        meta.hash,
        meta.rel_path.display(), // IMPORTANT CHANGE: no {:?} debug quotes
    );

    // Replace the example metadata block in the template with real data.
    // We keep the system instruction at the top and the Decision: marker at the end.
    let system_instruction = parts[0]
        .lines()
        .take_while(|l| !l.trim_start().starts_with("File metadata:"))
        .collect::<Vec<_>>()
        .join("\n");

    format!(
        "{}\n\n{}{}\n{}",
        system_instruction.trim(),
        meta_block,
        marker,
        parts.get(1).map_or("", |v| v.trim())
    )
}

// Parse the raw LLM output into a (decision, reason) pair.
// The model is instructed to output one word: "keep" or "delete".
fn parse_llm_output(raw: &str, meta: &FileMeta) -> (String, String) {
    let clean = raw.trim().to_lowercase();

    if clean.starts_with("delete") {
        (
            "delete".to_string(),
            format!("LLM classified '{}' for deletion.", meta.file_name),
        )
    } else if clean.starts_with("keep") {
        (
            "keep".to_string(),
            format!("LLM classified '{}' to keep.", meta.file_name),
        )
    } else {
        (
            "pending".to_string(),
            format!(
                "LLM output was unclear ('{}') for '{}'. Manual review recommended.",
                raw.trim(),
                meta.file_name
            ),
        )
    }
}