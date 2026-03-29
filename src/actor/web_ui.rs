#![allow(unused)]

use steady_state::*;
use std::sync::mpsc::Sender;

/// Message sent from the actor to the Tauri event bridge
pub struct DuplicateEvent {
    pub dup:     String,
    pub orig:    String,
    pub size:    u64,
    pub verdict: String,
}

pub async fn run(
    actor: SteadyActorShadow,
    ai_model_to_ui_rx: SteadyRx<String>,
    ui_to_db_tx: SteadyTx<String>,
    event_tx: Sender<DuplicateEvent>,
) -> Result<(), Box<dyn std::error::Error>> {
    let actor = actor.into_spotlight([&ai_model_to_ui_rx], [&ui_to_db_tx]);
    if actor.use_internal_behavior {
        internal_behavior(actor, ai_model_to_ui_rx, ui_to_db_tx, event_tx).await
    } else {
        actor.simulated_behavior(vec![&ai_model_to_ui_rx]).await
    }
}

async fn internal_behavior<A: SteadyActor>(
    mut actor: A,
    ai_model_to_ui_rx: SteadyRx<String>,
    ui_to_db_tx: SteadyTx<String>,
    event_tx: Sender<DuplicateEvent>,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut ai_model_to_ui_rx = ai_model_to_ui_rx.lock().await;
    let mut ui_to_db_tx = ui_to_db_tx.lock().await;

    while actor.is_running(|| ai_model_to_ui_rx.is_closed_and_empty()) {
        actor.wait_avail(&mut ai_model_to_ui_rx, 1).await;

        while let Some(verdict) = actor.try_take(&mut ai_model_to_ui_rx) {
            // verdict format: "delete::dup_path|orig_path" or "keep::dup_path|orig_path"
            if let Some((decision, paths)) = verdict.split_once("::") {
                let (dup, orig) = match paths.split_once('|') {
                    Some((d, o)) => (d.to_string(), o.to_string()),
                    None => (paths.to_string(), String::new()),
                };

                // Get file size
                let size = std::fs::metadata(&dup)
                    .map(|m| m.len())
                    .unwrap_or(0);

                let _ = event_tx.send(DuplicateEvent {
                    dup,
                    orig,
                    size,
                    verdict: decision.to_string(),
                });
            }
        }
    }

    Ok(())
}