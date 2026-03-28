// src/actor/user_interface.rs
// Receives FileDecision from AI_MODEL, stores them in shared state,
// and serves them over a local HTTP API for the frontend.
//
// Endpoints:
//   GET  http://localhost:3000/api/files       — returns all FileDecisions as JSON
//   POST http://localhost:3000/api/decision    — receives user keep/delete override
//   GET  http://localhost:3000/api/status      — returns current scan status
//
// Add to Cargo.toml:
//   axum            = "0.7"
//   tokio           = { version = "1", features = ["full"] }
//   tower-http      = { version = "0.5", features = ["cors"] }
//   serde_json      = "1"
//   serde           = { version = "1", features = ["derive"] }

#![allow(unused)]

use steady_state::*;
<<<<<<< Updated upstream
use std::path::PathBuf;
use std::sync::mpsc;

use ratatui::{
    DefaultTerminal, Frame,
    crossterm::event::{self, Event, KeyCode, KeyEventKind},
    layout::{Constraint, Layout},
    style::{Color, Modifier, Style, Stylize},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph},
};

pub async fn run(
    actor: SteadyActorShadow,
    ai_model_to_ui_rx: SteadyRx<String>,
    ui_to_db_tx: SteadyTx<PathBuf>,
) -> Result<(), Box<dyn std::error::Error>> {
    let actor = actor.into_spotlight([&ai_model_to_ui_rx], [&ui_to_db_tx]);
    if actor.use_internal_behavior {
        internal_behavior(actor, ai_model_to_ui_rx, ui_to_db_tx).await
=======
use std::sync::{Arc, Mutex};
use std::collections::HashMap;

use axum::{
    Router,
    routing::{get, post},
    extract::State,
    http::{Method, HeaderValue},
    Json,
};
use tower_http::cors::{CorsLayer, Any};
use serde::{Serialize, Deserialize};
use serde_json::json;

use crate::actor::crawler::FileMeta;
use crate::file_decision::FileDecision;

// ── Shared state between the actor loop and the HTTP server ──────────────────

#[derive(Clone)]
struct AppState {
    // key = file name, value = latest decision
    // Using HashMap so user overrides replace AI decisions cleanly
    files:  Arc<Mutex<HashMap<String, FileDecision>>>,
    status: Arc<Mutex<String>>,
}

// ── Request / Response types ─────────────────────────────────────────────────

#[derive(Deserialize)]
struct UserDecisionRequest {
    name:     String,
    decision: String,  // "keep" | "delete"
}

// ── Actor entry point ─────────────────────────────────────────────────────────

pub async fn run(
    actor: SteadyActorShadow,
    ai_model_to_ui_rx:      SteadyRx<FileDecision>,
    ui_to_file_handler_tx:  SteadyTx<String>,
) -> Result<(), Box<dyn Error>> {

    let actor = actor.into_spotlight([&ai_model_to_ui_rx], [&ui_to_file_handler_tx]);

    if actor.use_internal_behavior {
        internal_behavior(actor, ai_model_to_ui_rx, ui_to_file_handler_tx).await
>>>>>>> Stashed changes
    } else {
        actor.simulated_behavior(vec![&ai_model_to_ui_rx]).await
    }
}

async fn internal_behavior<A: SteadyActor>(
    mut actor: A,
<<<<<<< Updated upstream
    ai_model_to_ui_rx: SteadyRx<String>,
    ui_to_db_tx: SteadyTx<PathBuf>,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut ai_model_to_ui_rx = ai_model_to_ui_rx.lock().await;
    let mut ui_to_db_tx = ui_to_db_tx.lock().await;

    // actor → TUI thread: send new suggested files
    let (suggest_tx, suggest_rx) = mpsc::channel::<PathBuf>();
    // TUI thread → actor: send confirmed deletions
    let (delete_tx, delete_rx) = mpsc::channel::<PathBuf>();

    // Spawn TUI on a plain OS thread (no Tokio reactor needed)
	std::thread::spawn(move || {
		let mut terminal = ratatui::init();
		let result = run_tui(&mut terminal, suggest_rx, delete_tx);
		ratatui::restore();
		if let Err(e) = result {
			eprintln!("TUI error: {}", e);
		}
	});

    while actor.is_running(|| ai_model_to_ui_rx.is_closed_and_empty()) {
        // Forward AI verdicts to the TUI thread
        while let Some(verdict) = actor.try_take(&mut ai_model_to_ui_rx) {
            // TODO: replace with real FileMeta path once channel type is updated
            if verdict.trim().to_lowercase() == "delete" {
                let _ = suggest_tx.send(PathBuf::from(&verdict));
            }
        }

        // Forward confirmed deletions to DB actor
        while let Ok(path) = delete_rx.try_recv() {
            actor.wait_vacant(&mut ui_to_db_tx, 1).await;
            actor.try_send(&mut ui_to_db_tx, path);
        }

        actor.wait_avail(&mut ai_model_to_ui_rx, 1).await;
=======
    ai_model_to_ui_rx:     SteadyRx<FileDecision>,
    ui_to_file_handler_tx: SteadyTx<String>,
) -> Result<(), Box<dyn Error>> {

    let mut rx = ai_model_to_ui_rx.lock().await;
    let mut tx = ui_to_file_handler_tx.lock().await;

    // Shared state — the HTTP server and this actor loop both access it
    let state = AppState {
        files:  Arc::new(Mutex::new(HashMap::new())),
        status: Arc::new(Mutex::new("scanning".to_string())),
    };

    // Spawn the HTTP server on a separate task so it runs concurrently
    let server_state = state.clone();
    std::thread::spawn(move || {
    tokio::runtime::Runtime::new()
        .expect("failed to create Tokio runtime for HTTP server")
        .block_on(start_http_server(server_state));
});


    println!("UI_ACTOR: HTTP server started at http://localhost:3000");

    while actor.is_running(|| rx.is_closed_and_empty()) {

        await_for_all!(actor.wait_avail(&mut rx, 1), actor.wait_vacant(&mut tx, 1));

        let decision = match actor.try_take(&mut rx) {
            Some(d) => d,
            None    => continue,
        };

        let file_name = decision.meta.file_name.clone();
        let ai_dec    = decision.ai_decision.clone();

        // Store in shared state so the HTTP server can serve it
        {
            let mut files = state.files.lock().unwrap();
            files.insert(file_name.clone(), decision);
        }

        println!("UI_ACTOR: received '{}' → {}", file_name, ai_dec);

        // If AI said delete, forward the file name to the file handler
        if ai_dec == "delete" {
            match actor.try_send(&mut tx, file_name.clone()) {
    SendOutcome::Success    => {}
    SendOutcome::Blocked(_) => {
        eprintln!("UI_ACTOR: file handler channel blocked for '{}'", file_name);
    }
    SendOutcome::Timeout(_) => {
        eprintln!("UI_ACTOR: file handler channel timeout for '{}'", file_name);
    }
    SendOutcome::Closed(_)  => { break; }
}
        }
    }

    // Mark scan as complete when actor loop ends
    {
        let mut status = state.status.lock().unwrap();
        *status = "complete".to_string();
>>>>>>> Stashed changes
    }

    Ok(())
}

<<<<<<< Updated upstream
// ── TUI App State ────────────────────────────────────────────────────────────

struct App {
    suggested_files: Vec<PathBuf>,
    list_state: ListState,
    status: String,
    suggest_rx: mpsc::Receiver<PathBuf>,
    delete_tx: mpsc::Sender<PathBuf>,
}

impl App {
    fn new(suggest_rx: mpsc::Receiver<PathBuf>, delete_tx: mpsc::Sender<PathBuf>) -> Self {
        let mut list_state = ListState::default();
        list_state.select(Some(0));
        Self {
            suggested_files: Vec::new(),
            list_state,
            status: String::from("Waiting for AI suggestions..."),
            suggest_rx,
            delete_tx,
        }
    }

    fn selected_path(&self) -> Option<PathBuf> {
        self.list_state
            .selected()
            .and_then(|i| self.suggested_files.get(i))
            .cloned()
    }

    fn clamp_selection(&mut self) {
        let len = self.suggested_files.len();
        if len == 0 {
            self.list_state.select(None);
        } else if let Some(i) = self.list_state.selected() {
            if i >= len {
                self.list_state.select(Some(len - 1));
            }
        } else {
            self.list_state.select(Some(0));
        }
    }

    fn move_up(&mut self) {
        if let Some(i) = self.list_state.selected() {
            if i > 0 {
                self.list_state.select(Some(i - 1));
            }
        }
    }

    fn move_down(&mut self) {
        if let Some(i) = self.list_state.selected() {
            if i + 1 < self.suggested_files.len() {
                self.list_state.select(Some(i + 1));
            }
        }
    }

    fn delete_selected(&mut self) {
        if let Some(path) = self.selected_path() {
            self.suggested_files.retain(|p| *p != path);
            let _ = self.delete_tx.send(path.clone());
            self.status = format!("Deleted: {:?}", path);
            self.clamp_selection();
        } else {
            self.status = String::from("No file selected.");
        }
    }

    fn keep_selected(&mut self) {
        if let Some(path) = self.selected_path() {
            self.suggested_files.retain(|p| *p != path);
            self.status = format!("Kept: {:?}", path);
            self.clamp_selection();
        } else {
            self.status = String::from("No file selected.");
        }
    }

    fn never_delete_selected(&mut self) {
        if let Some(path) = self.selected_path() {
            self.suggested_files.retain(|p| *p != path);
            self.status = format!("Marked never-delete: {:?}", path);
            // TODO: persist to sled DB
            self.clamp_selection();
        } else {
            self.status = String::from("No file selected.");
        }
    }

    // Pull any new suggestions from the actor
    fn poll_suggestions(&mut self) {
        while let Ok(path) = self.suggest_rx.try_recv() {
            self.suggested_files.push(path);
            if self.list_state.selected().is_none() {
                self.list_state.select(Some(0));
            }
        }
    }
}

// ── TUI Render + Event Loop ──────────────────────────────────────────────────

fn run_tui(
    terminal: &mut DefaultTerminal,
    suggest_rx: mpsc::Receiver<PathBuf>,
    delete_tx: mpsc::Sender<PathBuf>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let mut app = App::new(suggest_rx, delete_tx);

    loop {
        app.poll_suggestions();
        terminal.draw(|frame| render(frame, &mut app))?;

        // Poll for key events with a short timeout so we keep polling suggestions
        if event::poll(std::time::Duration::from_millis(100))? {
            if let Event::Key(key) = event::read()? {
                if key.kind != KeyEventKind::Press {
                    continue;
                }
                match key.code {
                    KeyCode::Char('q') => break,
                    KeyCode::Up       => app.move_up(),
                    KeyCode::Down     => app.move_down(),
                    KeyCode::Char('d') => app.delete_selected(),
                    KeyCode::Char('k') => app.keep_selected(),
                    KeyCode::Char('n') => app.never_delete_selected(),
                    _ => {}
                }
            }
        }
    }

    Ok(())
}

fn render(frame: &mut Frame, app: &mut App) {
    let vertical = Layout::vertical([
        Constraint::Min(0),    // file list
        Constraint::Length(1), // key hints
        Constraint::Length(1), // status bar
    ]);
    let [list_area, hints_area, status_area] = vertical.areas(frame.area());

    // ── File list ────────────────────────────────────────────────────────────
    let items: Vec<ListItem> = app
        .suggested_files
        .iter()
        .enumerate()
        .map(|(i, path)| {
            let label = format!("[{}] {}", i + 1, path.display());
            ListItem::new(label)
        })
        .collect();

    let list = List::new(items)
        .block(Block::bordered().title(" CruftCrawler — Suggested Files "))
        .highlight_style(
            Style::default()
                .fg(Color::Black)
                .bg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol("▶ ");

    frame.render_stateful_widget(list, list_area, &mut app.list_state);

    // ── Key hints ────────────────────────────────────────────────────────────
    let hints = Line::from(vec![
        " (↑↓) navigate ".into(),
        " (d) delete ".bold().fg(Color::Red),
        " (k) keep ".bold().fg(Color::Green),
        " (n) never-delete ".bold().fg(Color::Cyan),
        " (q) quit ".bold().fg(Color::Gray),
    ]);
    frame.render_widget(hints, hints_area);

    // ── Status bar ───────────────────────────────────────────────────────────
    let status = Paragraph::new(app.status.as_str()).fg(Color::DarkGray);
    frame.render_widget(status, status_area);
}
=======
// ── HTTP server ───────────────────────────────────────────────────────────────

async fn start_http_server(state: AppState) {

    // Allow the frontend HTML file (opened from file://) to call localhost
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods([Method::GET, Method::POST])
        .allow_headers(Any);

    let app = Router::new()
        .route("/api/files",    get(handle_get_files))
        .route("/api/decision", post(handle_post_decision))
        .route("/api/status",   get(handle_get_status))
        .layer(cors)
        .with_state(state);

    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000")
        .await
        .expect("UI_ACTOR: failed to bind port 3000");

    axum::serve(listener, app)
        .await
        .expect("UI_ACTOR: server error");
}

// GET /api/files — return all file decisions as a JSON array
async fn handle_get_files(State(state): State<AppState>) -> Json<serde_json::Value> {
    let files = state.files.lock().unwrap();
    let list: Vec<&FileDecision> = files.values().collect();
    Json(json!(list))
}

// POST /api/decision — user overrides a single file's decision
// Body: { "name": "foo.txt", "decision": "keep" }
async fn handle_post_decision(
    State(state): State<AppState>,
    Json(req): Json<UserDecisionRequest>,
) -> Json<serde_json::Value> {

    let mut files = state.files.lock().unwrap();

    if let Some(entry) = files.get_mut(&req.name) {
        entry.ai_decision = req.decision.clone();
        entry.ai_reason   = format!("[User override] Manually marked as {}.", req.decision);
        Json(json!({ "ok": true, "name": req.name, "decision": req.decision }))
    } else {
        Json(json!({ "ok": false, "error": "file not found" }))
    }
}

// GET /api/status — let the frontend poll whether the scan is still running
async fn handle_get_status(State(state): State<AppState>) -> Json<serde_json::Value> {
    let status = state.status.lock().unwrap().clone();
    let count  = state.files.lock().unwrap().len();
    Json(json!({ "status": status, "files_processed": count }))
}
>>>>>>> Stashed changes
