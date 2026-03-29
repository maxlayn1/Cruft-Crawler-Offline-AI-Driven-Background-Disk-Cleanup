use steady_state::*;
use std::time::Duration;
use std::path::PathBuf;
use std::sync::mpsc;
use tauri::{Manager, Emitter, AppHandle};

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

// ── Tauri state: channel to trigger scan start ──────────────────────────────
struct ScanTrigger {
    tx: std::sync::Mutex<mpsc::Sender<()>>,
}

// ── Commands ────────────────────────────────────────────────────────────────

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

#[tauri::command]
fn get_scan_path() -> String {
    let config_str = std::fs::read_to_string("./config.toml").unwrap_or_default();
    let config: toml::Value = toml::from_str(&config_str)
        .unwrap_or(toml::Value::Table(Default::default()));
    config.get("directory")
        .and_then(|d| d.get("path"))
        .and_then(|p| p.as_str())
        .unwrap_or(".")
        .to_string()
}

#[tauri::command]
fn set_scan_path(new_path: String) -> Result<(), String> {
    let config_str = std::fs::read_to_string("./config.toml").unwrap_or_default();
    let updated = if config_str.contains("path = ") {
        let lines: Vec<String> = config_str.lines().map(|l| {
            if l.trim().starts_with("path = ") {
                format!("path = '{}'", new_path)
            } else {
                l.to_string()
            }
        }).collect();
        lines.join("\n")
    } else {
        config_str
    };
    std::fs::write("./config.toml", updated).map_err(|e| e.to_string())
}

/// Returns a sorted list of entries in `path`.
/// Directories are prefixed with "[DIR] ", files are plain names.
#[tauri::command]
fn list_folder(path: String) -> Vec<String> {
    let mut entries = Vec::new();
    if let Ok(dir) = std::fs::read_dir(&path) {
        for entry in dir.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            let is_dir = entry.file_type().map(|t| t.is_dir()).unwrap_or(false);
            if is_dir {
                entries.push(format!("[DIR] {}", name));
            } else {
                entries.push(name);
            }
        }
    }
    entries.sort_by(|a, b| {
        let a_dir = a.starts_with("[DIR]");
        let b_dir = b.starts_with("[DIR]");
        b_dir.cmp(&a_dir).then(a.cmp(b))
    });
    entries
}

/// Fires off the steady_state graph (called when user clicks "Start Scan").
#[tauri::command]
fn start_scan(state: tauri::State<ScanTrigger>) -> Result<(), String> {
    state.tx.lock()
        .map_err(|e| e.to_string())?
        .send(())
        .map_err(|e| e.to_string())
}

/// Restarts the Tauri process.
#[tauri::command]
fn restart_app(app: AppHandle) {
    app.restart();
}

// ── Entry point ──────────────────────────────────────────────────────────────

fn main() {
    let (event_tx, event_rx) = mpsc::channel::<actor::web_ui::DuplicateEvent>();
    let (scan_tx, scan_rx)   = mpsc::channel::<()>();

    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(ScanTrigger { tx: std::sync::Mutex::new(scan_tx) })
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

            // Wait for the user to click "Start Scan" before launching the graph
            let handle2 = app.handle().clone();
            std::thread::spawn(move || {
                // Block here until start_scan command fires
                if scan_rx.recv().is_err() { return; }

                // Tell the frontend which path we're scanning
                let config_str = std::fs::read_to_string("./config.toml").unwrap_or_default();
                let config: toml::Value = toml::from_str(&config_str)
                    .unwrap_or(toml::Value::Table(Default::default()));
                let scan_path = config.get("directory")
                    .and_then(|d| d.get("path"))
                    .and_then(|p| p.as_str())
                    .unwrap_or(".")
                    .to_string();
                let _ = handle2.emit("scan-path", scan_path);

                let mut graph = GraphBuilder::default().build(());
                build_graph(&mut graph, event_tx);
                graph.start();
                graph.block_until_stopped(Duration::from_secs(1));
            });

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            delete_file,
            never_delete_file,
            get_scan_path,
            set_scan_path,
            list_folder,
            start_scan,
            restart_app,
        ])
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