#![allow(unused)]

use libc::printf;
use steady_state::*;
use crate::llm_engine::LlmEngine;
use crate::actor::crawler::FileMeta;
use std::fs;

const MODEL_FILE_PATH: &str = "./src/models/Llama-3.2-3B-Instruct-UD-Q8_K_XL.gguf";

pub async fn run(
    actor: SteadyActorShadow,
    crawler_to_model_rx: SteadyRx<FileMeta>,
    ai_model_to_ui_tx: SteadyTx<String>,
) -> Result<(), Box<dyn std::error::Error>> {
    let actor = actor.into_spotlight([&crawler_to_model_rx], [&ai_model_to_ui_tx]);

    if actor.use_internal_behavior {
        internal_behavior(actor, crawler_to_model_rx, ai_model_to_ui_tx).await
    } else {
        actor.simulated_behavior(vec!(&crawler_to_model_rx)).await
    }
}

async fn internal_behavior<A: SteadyActor>(
    mut actor: A,
    crawler_to_ai_model_rx: SteadyRx<FileMeta>,
    ai_model_to_ui_tx: SteadyTx<String>,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut crawler_to_ai_model_rx = crawler_to_ai_model_rx.lock().await;
    let mut ai_model_to_ui_tx = ai_model_to_ui_tx.lock().await;

    let engine = match LlmEngine::load_new_model(MODEL_FILE_PATH) {
        Ok(e) => e,
        Err(e) => return Err(e.into()),
    };

    while actor.is_running(|| crawler_to_ai_model_rx.is_closed_and_empty() || ai_model_to_ui_tx.mark_closed()) {
        await_for_all!(
            actor.wait_avail(&mut crawler_to_ai_model_rx, 1),
            actor.wait_vacant(&mut ai_model_to_ui_tx, 1)
        );

        let file_meta = match actor.try_take(&mut crawler_to_ai_model_rx) {
            Some(m) => m,
            None => continue,
        };

        let prompt = build_prompt(&file_meta);
       
        let verdict = match engine.infer_model(&prompt) {
            Ok(raw) => {
                eprintln!("AI_MODEL: raw output: {:?}", raw);
                parse_verdict(&raw)
            }
            Err(e) => {
                eprintln!("AI_MODEL: inference FAILED: {}", e);
                continue;
            }
        };

        let message = format!("{}|{}", verdict, file_meta.abs_path.display());
        loop {
            actor.wait_vacant(&mut ai_model_to_ui_tx, 1).await;
            match actor.try_send(&mut ai_model_to_ui_tx, message.clone()) {
                SendOutcome::Success => break,
                SendOutcome::Blocked(_) => continue,
                other => break,
            }
        }
    }

    Ok(())
}

/// Parses the model's response to find "Decision: keep" or "Decision: delete".
/// Falls back to scanning for bare "keep"/"delete" keywords if the structured
/// line is absent. Defaults to "keep" when nothing matches.
fn parse_verdict(raw: &str) -> String {
    let lower = raw.to_lowercase();

    // Primary: look for "decision: <verdict>" anywhere in the output
    for line in lower.lines() {
        let trimmed = line.trim();
        if let Some(rest) = trimmed.strip_prefix("decision:") {
            let word = rest
                .split_whitespace()
                .next()
                .unwrap_or("")
                .chars()
                .filter(|c| c.is_alphabetic())
                .collect::<String>();

            if word == "delete" || word == "keep" {
                return word;
            }
        }
    }

    // Fallback: scan every word for an explicit keep/delete keyword
    for word in lower.split_whitespace() {
        let clean: String = word.chars().filter(|c| c.is_alphabetic()).collect();
        if clean == "delete" || clean == "keep" {
            return clean;
        }
    }

    // Default: when uncertain, always keep
    "keep".to_string()
}

fn build_prompt(meta: &FileMeta) -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;
    let days_since_modified = (now - meta.modified) / 86400;

    format!(
        r#"You are a file management assistant. Your job is to decide whether a file should be kept or deleted based on its metadata.

        ### Decision Rules
        - DELETE if: the file has not been accessed or modified in over 365 days AND the file name suggests it is temporary, a draft, a cache, or a duplicate (e.g., contains "tmp", "temp", "cache", "copy", "backup", "old", "~", or ends in ".log", ".bak", ".swp")
        - DELETE if: the file is very small (under 512 bytes), has not been modified in over 180 days, and the name suggests it is a leftover or auto-generated artifact
        - KEEP if: the file is read-only (system or protected files are rarely safe to delete)
        - KEEP if: the file has been modified recently (within 30 days)
        - KEEP if: uncertain — always prefer keeping over deleting

        ### Examples
        File: "cache_session_1A2B.tmp", 204 bytes, 412 days old, read-only: false
        Reasoning: Name contains "cache" and ".tmp", very old, small, not protected.
        Decision: delete

        File: "project_report_final.pdf", 84200 bytes, 5 days old, read-only: false
        Reasoning: Recently modified, meaningful name, substantial size.
        Decision: keep

        File: "libsystem_kernel.dylib", 512 bytes, 730 days old, read-only: true
        Reasoning: Read-only flag suggests system file. Never delete.
        Decision: keep

        File: "notes_backup_old.txt", 1100 bytes, 200 days old, read-only: false
        Reasoning: Name contains "backup" and "old", moderately old, small, not protected.
        Decision: delete

        ### Now decide for this file
        File: "{}", {} bytes, {} days since last modified, read-only: {}
        Reasoning:"#,
        meta.file_name,
        meta.size,
        days_since_modified,
        meta.readonly,
    )
}