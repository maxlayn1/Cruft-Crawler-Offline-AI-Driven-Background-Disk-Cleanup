#![allow(unused)]

use steady_state::*;
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
    ui_to_db_tx: SteadyTx<String>,
) -> Result<(), Box<dyn std::error::Error>> {
    let actor = actor.into_spotlight([&ai_model_to_ui_rx], [&ui_to_db_tx]);
    if actor.use_internal_behavior {
        internal_behavior(actor, ai_model_to_ui_rx, ui_to_db_tx).await
    } else {
        actor.simulated_behavior(vec![&ai_model_to_ui_rx]).await
    }
}

async fn internal_behavior<A: SteadyActor>(
    mut actor: A,
    ai_model_to_ui_rx: SteadyRx<String>,
    ui_to_db_tx: SteadyTx<String>,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut ai_model_to_ui_rx = ai_model_to_ui_rx.lock().await;
    let mut ui_to_db_tx = ui_to_db_tx.lock().await;

    // actor → TUI thread: send new suggested files
    let (suggest_tx, suggest_rx) = mpsc::channel::<(PathBuf, PathBuf)>();
    // TUI thread → actor: send delete/never_delete commands
    let (delete_tx, delete_rx) = mpsc::channel::<String>();

    // Spawn TUI on a plain OS thread (no Tokio reactor needed)
	std::thread::spawn(move || {
		let mut terminal = ratatui::init();
		let result = run_tui(&mut terminal, suggest_rx, delete_tx);
		ratatui::restore();
		if let Err(e) = result {
			eprintln!("TUI error: {}", e);
		}
		// q was pressed - shut down the entire process cleanly
		std::process::exit(0);
	});

    while actor.is_running(|| ai_model_to_ui_rx.is_closed_and_empty()) {
        // Forward AI verdicts to the TUI thread
        while let Some(verdict) = actor.try_take(&mut ai_model_to_ui_rx) {
            // Format is "delete::dup_path|orig_path"
            if let Some((decision, paths)) = verdict.split_once("::") {
                if decision.trim().to_lowercase() == "delete" {
                    let (dup, orig) = match paths.split_once('|') {
                        Some((d, o)) => (PathBuf::from(d), PathBuf::from(o)),
                        None => (PathBuf::from(paths), PathBuf::new()),
                    };
                    let _ = suggest_tx.send((dup, orig));
                }
            }
        }

        // Forward delete/never_delete commands to DB actor
        while let Ok(cmd) = delete_rx.try_recv() {
            actor.wait_vacant(&mut ui_to_db_tx, 1).await;
            actor.try_send(&mut ui_to_db_tx, cmd);
        }

        actor.wait_avail(&mut ai_model_to_ui_rx, 1).await;
    }

    Ok(())
}

// ── TUI App State ────────────────────────────────────────────────────────────

struct App {
    suggested_files: Vec<(PathBuf, PathBuf)>,
    list_state: ListState,
    status: String,
    suggest_rx: mpsc::Receiver<(PathBuf, PathBuf)>,
    delete_tx: mpsc::Sender<String>,
}

impl App {
    fn new(suggest_rx: mpsc::Receiver<(PathBuf, PathBuf)>, delete_tx: mpsc::Sender<String>) -> Self {
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

    fn selected_path(&self) -> Option<(PathBuf, PathBuf)> {
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
        if let Some((dup, _orig)) = self.selected_path() {
            self.suggested_files.retain(|(d, _)| *d != dup);
            let cmd = format!("delete::{}", dup.display());
            let _ = self.delete_tx.send(cmd);
            self.status = format!("Deleted: {}", dup.display());
            self.clamp_selection();
        } else {
            self.status = String::from("No file selected.");
        }
    }

    fn keep_selected(&mut self) {
        if let Some((dup, _orig)) = self.selected_path() {
            self.suggested_files.retain(|(d, _)| *d != dup);
            self.status = format!("Kept: {}", dup.display());
            self.clamp_selection();
        } else {
            self.status = String::from("No file selected.");
        }
    }

    fn never_delete_selected(&mut self) {
        if let Some((dup, _orig)) = self.selected_path() {
            self.suggested_files.retain(|(d, _)| *d != dup);
            let _ = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open("./never_delete.txt")
                .and_then(|mut f| {
                    use std::io::Write;
                    writeln!(f, "{}", dup.display())
                });
            self.status = format!("Never-delete: {}", dup.display());
            self.clamp_selection();
        } else {
            self.status = String::from("No file selected.");
        }
    }

    // Pull any new suggestions from the actor
    fn poll_suggestions(&mut self) {
        // Process at most 20 items per frame so the TUI stays responsive
        for _ in 0..20 {
            match self.suggest_rx.try_recv() {
                Ok((dup, orig)) => {
                    if !self.suggested_files.iter().any(|(d, _)| *d == dup) {
                        self.suggested_files.push((dup, orig));
                        if self.list_state.selected().is_none() {
                            self.list_state.select(Some(0));
                        }
                    }
                }
                Err(_) => break,
            }
        }
    }
}

// ── TUI Render + Event Loop ──────────────────────────────────────────────────

fn run_tui(
    terminal: &mut DefaultTerminal,
    suggest_rx: mpsc::Receiver<(PathBuf, PathBuf)>,
    delete_tx: mpsc::Sender<String>,
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
        .map(|(i, (dup, orig))| {
            let line1 = ratatui::text::Line::from(format!("[{}] {}", i + 1, dup.display()));
            let line2 = ratatui::text::Line::from(
                ratatui::text::Span::styled(
                    format!("    copy of: {}", orig.display()),
                    Style::default().fg(Color::DarkGray),
                )
            );
            ListItem::new(ratatui::text::Text::from(vec![line1, line2]))
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
