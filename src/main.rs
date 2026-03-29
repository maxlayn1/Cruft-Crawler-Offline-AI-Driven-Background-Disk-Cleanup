use steady_state::*;
use std::time::Duration;
use std::path::PathBuf;
use std::sync::mpsc;
use tauri::{Manager, Emitter};

pub(crate) mod actor {
    pub(crate) mod crawler;
    pub(crate) mod db_manager;
    pub(crate) mod ai_model;
    pub(crate) mod web_ui;
}
pub(crate) mod llm_engine;

const NAME_CRAWLER:  &str = "CRAWLER";
const NAME_DB:       &str = "DB_MANAGER";
const NAME_AI_MODEL: &str = "AI_MODEL";
const NAME_UI_ACTOR: &str = "UI_ACTOR";

#[tauri::command]
fn delete_file(path: String) -> Result<(), String> {
    std::fs::remove_file(&path).map_err(|e| e.to_string())
}

#[tauri::command]
fn never_delete_file(path: String) -> Result<(), String> {
    use std::io::Write;
    let mut f = std::fs::OpenOptions::new()
        .create(true).append(true)
        .open("./never_delete.txt")
        .map_err(|e| e.to_string())?;
    writeln!(f, "{}", path).map_err(|e| e.to_string())
}

fn main() {
    let (event_tx, event_rx) = mpsc::channel::<actor::web_ui::DuplicateEvent>();
    tauri::Builder::default()
        .setup(move |app| {
            let handle = app.handle().clone();

            // Bridge: forward actor events to Tauri window events
            std::thread::spawn(move || {
                while let Ok(ev) = event_rx.recv() {
                    let _ = handle.emit("duplicate-found", serde_json::json!({
                        "dup":     ev.dup,
                        "orig":    ev.orig,
                        "size":    ev.size,
                        "verdict": ev.verdict,
                    }));
                }
                let _ = handle.emit("scan-complete", ());
            });

            // Give the window 2 seconds to load before starting the graph
            std::thread::spawn(move || {
                std::thread::sleep(std::time::Duration::from_secs(2));
                let mut graph = GraphBuilder::default().build(());
                build_graph(&mut graph, event_tx);
                graph.start();
                graph.block_until_stopped(Duration::from_secs(1));
            });

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![delete_file, never_delete_file])
        .run(tauri::generate_context!())
        .expect("Tauri error");
}

fn build_graph(graph: &mut Graph, event_tx: mpsc::Sender<actor::web_ui::DuplicateEvent>) {
    let channel_builder = graph.channel_builder()
        .with_filled_trigger(Trigger::AvgAbove(Filled::p90()), AlertColor::Red)
        .with_filled_trigger(Trigger::AvgAbove(Filled::p60()), AlertColor::Orange)
        .with_filled_percentile(Percentile::p80());

    let (crawler_to_db_tx, crawler_to_db_rx) = channel_builder.build();
    let (crawler_to_ai_model_tx, crawler_to_ai_model_rx) = channel_builder.build();
    let (ai_model_to_ui_tx, ai_model_to_ui_rx) = channel_builder.build();
    let (ui_to_db_tx, ui_to_db_rx) = channel_builder.build();

    let actor_builder = graph.actor_builder()
        .with_load_avg()
        .with_mcpu_avg();

    let state = new_state();
    actor_builder.with_name(NAME_CRAWLER)
        .build(move |actor| actor::crawler::run(
            actor,
            crawler_to_db_tx.clone(),
            crawler_to_ai_model_tx.clone(),
            state.clone(),
        ), SoloAct);

    actor_builder.with_name(NAME_DB)
        .build(move |actor| actor::db_manager::run(
            actor,
            crawler_to_db_rx.clone(),
            ui_to_db_rx.clone(),
        ), SoloAct);

    actor_builder.with_name(NAME_AI_MODEL)
        .build(move |actor| actor::ai_model::run(
            actor,
            crawler_to_ai_model_rx.clone(),
            ai_model_to_ui_tx.clone(),
        ), SoloAct);

    actor_builder.with_name(NAME_UI_ACTOR)
        .build(move |actor| actor::web_ui::run(
            actor,
            ai_model_to_ui_rx.clone(),
            ui_to_db_tx.clone(),
            event_tx.clone(),
        ), SoloAct);
}
