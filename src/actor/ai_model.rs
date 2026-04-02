#![allow(unused)]

use steady_state::*;
use crate::llm_engine::LlmEngine;
use crate::actor::crawler::FileMeta;
use std::fs;

const MODEL_FILE_PATH: &str  = "C:/School/Spring_2026/CS_Senior_Project/cruft_crawler/cruftcrawler_0.1/cc_0.1/src/models/Llama-3.2-1B-Instruct-Q4_K_M.gguf";

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

    //eprintln!("AI_MODEL: loading model from {}", MODEL_FILE_PATH);
    let engine = match LlmEngine::load_new_model(MODEL_FILE_PATH) {
        Ok(e) => {
           // eprintln!("AI_MODEL: model loaded successfully");
            e
        }
        Err(e) => {
           // eprintln!("AI_MODEL: FAILED to load model: {}", e);
            return Err(e.into());
        }
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

        // Build prompt from real FileMeta
        let prompt = build_prompt(&file_meta);

        // Run inference
       // eprintln!("AI_MODEL: running inference on {}", file_meta.file_name);
		let verdict = match engine.infer_model(&prompt) {
			Ok(v) => {
				//eprintln!("AI_MODEL: raw output: {:?}", v);  // ← see what the model actually returns
				v.split_whitespace()
				.next()
				.unwrap_or("")
				.chars()
				.filter(|c| c.is_alphabetic())
				.collect::<String>()
				.to_lowercase()
			}
			Err(e) => {
				//eprintln!("AI_MODEL: inference FAILED: {}", e);
				continue;
			}
		};


        //eprintln!("AI verdict for {:?}: {}", file_meta.file_name, verdict);

        // Send "verdict|/absolute/path" to UI actor — wait and retry until sent
        let message = format!("{}|{}", verdict, file_meta.abs_path.display());
        loop {
            actor.wait_vacant(&mut ai_model_to_ui_tx, 1).await;
            match actor.try_send(&mut ai_model_to_ui_tx, message.clone()) {
                SendOutcome::Success => break,
                SendOutcome::Blocked(_) => continue,
                other => {
                    eprintln!("Send to UI failed: {:?}", other);
                    break;
                }
            }
        }
    }

    Ok(())
}

fn build_prompt(meta: &FileMeta) -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;
    let days_since_modified = (now - meta.modified) / 86400;

    format!(
        "You are an automated file management assistant. Decide whether to keep or delete a file. Your response must be exactly one word: either \"keep\" or \"delete\".\n\nFile metadata:\n- Name: {}\n- Size: {} bytes\n- Days since last modified: {}\n- Read-only: {}\n\nDecision:",
        meta.file_name,
        meta.size,
        days_since_modified,
        meta.readonly,
    )
}
