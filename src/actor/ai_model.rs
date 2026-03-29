#![allow(unused)]

use steady_state::*;
use crate::llm_engine::LlmEngine;

use std::path::Path;
use std::error::Error;

const MODEL_FILE_PATH: &str  = "./src/models/Llama-3.2-3B-Instruct-UD-Q4_K_XL.gguf";

// run function 
pub async fn run(actor: SteadyActorShadow, crawler_to_model_rx: SteadyRx<String>, ai_model_to_ui_tx: SteadyTx<String>) -> Result<(),Box<dyn Error>> {

    let actor = actor.into_spotlight([&crawler_to_model_rx], [&ai_model_to_ui_tx]);

    if actor.use_internal_behavior {
        internal_behavior(actor, crawler_to_model_rx, ai_model_to_ui_tx).await
    } else {
        actor.simulated_behavior(vec!(&crawler_to_model_rx)).await
    }
}


async fn internal_behavior<A: SteadyActor>(
    mut actor: A,
    crawler_to_ai_model_rx: SteadyRx<String>,
    ai_model_to_ui_tx: SteadyTx<String>,
) -> Result<(), Box<dyn Error>> {

    let mut crawler_to_ai_model_rx = crawler_to_ai_model_rx.lock().await;
    let mut ai_model_to_ui_tx = ai_model_to_ui_tx.lock().await;

    // Load the model once before the loop
    let engine = match LlmEngine::load_new_model(MODEL_FILE_PATH) {
        Ok(e) => e,
        Err(err) => {
            // If the model file is missing or fails, fall back to hardcoded "delete"
            while actor.is_running(|| crawler_to_ai_model_rx.is_closed_and_empty() || ai_model_to_ui_tx.mark_closed()) {
                await_for_all!(actor.wait_avail(&mut crawler_to_ai_model_rx, 1), actor.wait_vacant(&mut ai_model_to_ui_tx, 1));
                let received = match actor.try_take(&mut crawler_to_ai_model_rx) {
                    Some(msg) => msg,
                    None => continue,
                };
                let verdict = format!("delete::{}", received);
                actor.try_send(&mut ai_model_to_ui_tx, verdict);
            }
            return Ok(());
        }
    };

    // Load never_delete list from file
    let never_delete_list: std::collections::HashSet<String> = std::fs::read_to_string("./never_delete.txt")
        .unwrap_or_default()
        .lines()
        .map(|l| l.trim().to_string())
        .filter(|l| !l.is_empty())
        .collect();

    while actor.is_running(|| crawler_to_ai_model_rx.is_closed_and_empty() || ai_model_to_ui_tx.mark_closed()) {
        await_for_all!(actor.wait_avail(&mut crawler_to_ai_model_rx, 1), actor.wait_vacant(&mut ai_model_to_ui_tx, 1));

        let path_str = match actor.try_take(&mut crawler_to_ai_model_rx) {
            Some(msg) => msg,
            None => continue,
        };

        // path_str is "dup_path|orig_path" - extract just the dup path
        let dup_path = path_str.splitn(2, '|').next().unwrap_or(&path_str).to_string();

        // Skip files the user has marked as never-delete
        if never_delete_list.contains(&dup_path) {
            continue;
        }

        // Build prompt from real file metadata
        let prompt = build_prompt(&dup_path);

        // Run inference (blocking call - acceptable on a dedicated SoloAct thread)
        let response = engine.infer_model(&prompt).unwrap_or_else(|_| "keep".to_string());

        // Parse: if response contains "delete" -> delete, otherwise keep
        let decision = if response.to_lowercase().contains("delete") { "delete" } else { "keep" };
        let verdict = format!("{}::{}", decision, path_str);

        actor.try_send(&mut ai_model_to_ui_tx, verdict);
    }

    Ok(())
}


fn build_prompt(path_str: &str) -> String {
    let path = Path::new(path_str);
    let file_name = path.file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("unknown");

    let (size_str, modified_str) = match std::fs::metadata(path) {
        Ok(md) => {
            let size = md.len();
            let size_str = format_size(size);
            let modified = filetime::FileTime::from_last_modification_time(&md).seconds();
            let modified_str = format_age(modified);
            (size_str, modified_str)
        }
        Err(_) => ("unknown".to_string(), "unknown".to_string()),
    };

    format!(
        "You are a disk cleanup assistant. Respond with exactly one word: delete or keep.

File metadata:
- Name: {}
- Size: {}
- Last modified: {}
- Note: This file is a duplicate (identical content exists elsewhere on disk)

Decision:",
        file_name, size_str, modified_str
    )
}


fn format_size(bytes: u64) -> String {
    if bytes >= 1_073_741_824 {
        format!("{:.1} GB", bytes as f64 / 1_073_741_824.0)
    } else if bytes >= 1_048_576 {
        format!("{:.1} MB", bytes as f64 / 1_048_576.0)
    } else if bytes >= 1024 {
        format!("{:.1} KB", bytes as f64 / 1024.0)
    } else {
        format!("{} bytes", bytes)
    }
}


fn format_age(modified_secs: i64) -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    let age_secs = now - modified_secs;
    if age_secs < 0 {
        return "recently".to_string();
    }
    let days = age_secs / 86400;
    if days == 0 {
        "today".to_string()
    } else if days == 1 {
        "1 day ago".to_string()
    } else if days < 30 {
        format!("{} days ago", days)
    } else if days < 365 {
        format!("{} months ago", days / 30)
    } else {
        format!("{} years ago", days / 365)
    }
}
