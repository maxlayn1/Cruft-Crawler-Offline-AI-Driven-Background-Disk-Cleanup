use steady_state::*;
use std::time::Duration;
use std::path::PathBuf;
use std::sync::{mpsc, Arc, Mutex};
use std::sync::atomic::{AtomicBool, Ordering};
use tauri::{Manager, Emitter, AppHandle};
use walkdir::WalkDir;

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

// ── Tauri shared state ───────────────────────────────────────────────────────

struct ScanTrigger {
    tx: Mutex<mpsc::Sender<()>>,
}

struct StopFlag(Arc<AtomicBool>);

/// Tracks the last fully-processed file path so a stopped scan can resume.
struct ResumeFrom(Arc<Mutex<String>>);

// ── Directory exclusion list (same as crawler) ───────────────────────────────

const SKIP_DIRS: &[&str] = &[
    ".git", ".svn", ".hg",
    "node_modules", "target",
    "Library", "Temp", "Artifacts", "PackageCache", "ShaderCache",
    ".vs", ".idea", "__pycache__",
    "obj", "bin",
    ".cache", ".next", "dist", "build",
];

// ── Commands ─────────────────────────────────────────────────────────────────

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

#[derive(serde::Serialize)]
struct FolderInfo {
    file_count: u64,
    total_bytes: u64,
    capped: bool,
}

/// Walk ALL of `path` (no exclusions) and return total file count + size,
/// matching what Windows Explorer reports.
#[tauri::command]
fn get_folder_info(path: String) -> FolderInfo {
    let mut file_count = 0u64;
    let mut total_bytes = 0u64;
    let mut capped = false;
    const MAX_FILES: u64 = 500_000;

    for entry in WalkDir::new(&path).into_iter().flatten() {
        if entry.file_type().is_file() {
            file_count += 1;
            total_bytes += entry.metadata().map(|m| m.len()).unwrap_or(0);
            if file_count >= MAX_FILES {
                capped = true;
                break;
            }
        }
    }
    FolderInfo { file_count, total_bytes, capped }
}

/// Returns a sorted list of immediate children in `path`.
#[tauri::command]
fn list_folder(path: String) -> Vec<String> {
    let mut entries = Vec::new();
    if let Ok(dir) = std::fs::read_dir(&path) {
        for entry in dir.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            let is_dir = entry.file_type().map(|t| t.is_dir()).unwrap_or(false);
            entries.push(if is_dir { format!("[DIR] {}", name) } else { name });
        }
    }
    entries.sort_by(|a, b| {
        let a_dir = a.starts_with("[DIR]");
        let b_dir = b.starts_with("[DIR]");
        b_dir.cmp(&a_dir).then(a.cmp(b))
    });
    entries
}

/// Signals the graph thread to start (or resume) scanning.
#[tauri::command]
fn start_scan(state: tauri::State<ScanTrigger>, stop: tauri::State<StopFlag>) -> Result<(), String> {
    // Clear any previous stop so the crawler runs
    stop.0.store(false, Ordering::Relaxed);
    state.tx.lock()
        .map_err(|e| e.to_string())?
        .send(())
        .map_err(|e| e.to_string())
}

/// Stops the in-progress scan. The resume position is preserved.
/// Emits "scan-stopped" immediately so the UI updates without waiting.
#[tauri::command]
fn stop_scan(stop: tauri::State<StopFlag>, app: AppHandle) {
    stop.0.store(true, Ordering::Relaxed);
    let _ = app.emit("scan-stopped", ());
}

/// Restarts the Tauri process.
#[tauri::command]
fn restart_app(app: AppHandle) {
    app.restart();
}

// ── Entry point ───────────────────────────────────────────────────────────────

fn main() {
    let (event_tx, event_rx) = mpsc::channel::<actor::web_ui::DuplicateEvent>();
    let (scan_tx, scan_rx)   = mpsc::channel::<()>();

    let stop_flag   = Arc::new(AtomicBool::new(false));
    let resume_from = Arc::new(Mutex::new(String::new()));

    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(ScanTrigger { tx: Mutex::new(scan_tx) })
        .manage(StopFlag(stop_flag.clone()))
        .manage(ResumeFrom(resume_from.clone()))
        .setup(move |app| {
            let handle = app.handle().clone();

            // Bridge: forward actor events to Tauri window events (runs forever)
            let _event_tx_keep = event_tx.clone(); // keep channel open across graph rebuilds
            std::thread::spawn(move || {
                while let Ok(ev) = event_rx.recv() {
                    let _ = handle.emit("duplicate-found", serde_json::json!({
                        "dup":     ev.dup,
                        "orig":    ev.orig,
                        "size":    ev.size,
                        "verdict": ev.verdict,
                    }));
                }
            });

            // Graph control thread — loops so the user can stop and resume
            let handle2      = app.handle().clone();
            let stop_flag2   = stop_flag.clone();
            let resume_from2 = resume_from.clone();
            std::thread::spawn(move || {
                while let Ok(()) = scan_rx.recv() {
                    // Reset stop flag (start_scan already does this, belt-and-suspenders)
                    stop_flag2.store(false, Ordering::Relaxed);

                    // Tell the frontend which path we're scanning
                    let config_str = std::fs::read_to_string("./config.toml").unwrap_or_default();
                    let config: toml::Value = toml::from_str(&config_str)
                        .unwrap_or(toml::Value::Table(Default::default()));
                    let scan_path = config.get("directory")
                        .and_then(|d| d.get("path"))
                        .and_then(|p| p.as_str())
                        .unwrap_or(".")
                        .to_string();
                    let _ = handle2.emit("scan-path", &scan_path);

                    // Build and run the graph
                    let mut graph = GraphBuilder::default().build(());
                    build_graph(
                        &mut graph,
                        event_tx.clone(),
                        stop_flag2.clone(),
                        resume_from2.clone(),
                    );
                    graph.start();
                    let _ = graph.block_until_stopped(Duration::from_secs(1));

                    // Only emit scan-complete on natural finish (stop already emitted by stop_scan command)
                    if !stop_flag2.load(Ordering::Relaxed) {
                        *resume_from2.lock().unwrap() = String::new();
                        let _ = handle2.emit("scan-complete", ());
                    }
                }
            });

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            delete_file,
            never_delete_file,
            get_scan_path,
            set_scan_path,
            get_folder_info,
            list_folder,
            start_scan,
            stop_scan,
            restart_app,
        ])
        .run(tauri::generate_context!())
        .expect("Tauri error");
}

fn build_graph(
    graph: &mut Graph,
    event_tx: mpsc::Sender<actor::web_ui::DuplicateEvent>,
    stop_flag: Arc<AtomicBool>,
    resume_from: Arc<Mutex<String>>,
) {
    let channel_builder = graph.channel_builder()
        .with_filled_trigger(Trigger::AvgAbove(Filled::p90()), AlertColor::Red)
        .with_filled_trigger(Trigger::AvgAbove(Filled::p60()), AlertColor::Orange)
        .with_filled_percentile(Percentile::p80());

    let (crawler_to_db_tx, crawler_to_db_rx)           = channel_builder.build();
    let (crawler_to_ai_model_tx, crawler_to_ai_model_rx) = channel_builder.build();
    let (ai_model_to_ui_tx, ai_model_to_ui_rx)         = channel_builder.build();
    let (ui_to_db_tx, ui_to_db_rx)                     = channel_builder.build();

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
            stop_flag.clone(),
            resume_from.clone(),
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