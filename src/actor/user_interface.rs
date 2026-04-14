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
    ui_to_db_tx: SteadyTx<PathBuf>,
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
        while let Some(message) = actor.try_take(&mut ai_model_to_ui_rx) {
            //eprintln!("UI received message: {:?}", message);  // ← add this
            if let Some((verdict, path_str)) = message.split_once('|') {
               // eprintln!("Verdict: {:?}, Path: {:?}", verdict, path_str);  // ← add this
                if verdict.trim() == "delete" {
                    //eprintln!("Sending to TUI: {:?}", path_str);  // ← add this
                    let _ = suggest_tx.send(PathBuf::from(path_str.trim()));
                }
            } else {
                eprintln!("Message did not split on '|': {:?}", message);  // ← add this
            }
        }

        // Forward confirmed deletions to DB actor
        while let Ok(path) = delete_rx.try_recv() {
            actor.wait_vacant(&mut ui_to_db_tx, 1).await;
            actor.try_send(&mut ui_to_db_tx, path);
        }

        actor.wait_avail(&mut ai_model_to_ui_rx, 1).await; 
    }

    Ok(())
}

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

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use std::sync::mpsc;

    // ── helpers ───────────────────────────────────────────────────────────────

    fn make_app() -> App {
        let (suggest_tx, suggest_rx) = mpsc::channel::<PathBuf>();
        let (delete_tx, delete_rx) = mpsc::channel::<PathBuf>();
        // We keep suggest_tx and delete_rx alive in the returned app;
        // leak them so they don't close the channels mid-test.
        std::mem::forget(suggest_tx);
        std::mem::forget(delete_rx);
        App::new(suggest_rx, delete_tx)
    }

    fn make_app_with_channels() -> (App, mpsc::Sender<PathBuf>, mpsc::Receiver<PathBuf>) {
        let (suggest_tx, suggest_rx) = mpsc::channel::<PathBuf>();
        let (delete_tx, delete_rx) = mpsc::channel::<PathBuf>();
        let app = App::new(suggest_rx, delete_tx);
        (app, suggest_tx, delete_rx)
    }

    fn path(s: &str) -> PathBuf {
        PathBuf::from(s)
    }

    // ── App::new ──────────────────────────────────────────────────────────────

    #[test]
    fn test_new_starts_empty() {
        let app = make_app();
        assert!(app.suggested_files.is_empty());
        assert_eq!(app.status, "Waiting for AI suggestions...");
    }

    #[test]
    fn test_new_selection_is_zero() {
        let app = make_app();
        // ListState starts at Some(0) per the constructor
        assert_eq!(app.list_state.selected(), Some(0));
    }

    // ── selected_path ─────────────────────────────────────────────────────────

    #[test]
    fn test_selected_path_empty_list_returns_none() {
        let app = make_app();
        assert_eq!(app.selected_path(), None);
    }

    #[test]
    fn test_selected_path_returns_correct_item() {
        let mut app = make_app();
        app.suggested_files.push(path("/tmp/a.txt"));
        app.suggested_files.push(path("/tmp/b.txt"));
        app.list_state.select(Some(1));
        assert_eq!(app.selected_path(), Some(path("/tmp/b.txt")));
    }

    // ── clamp_selection ───────────────────────────────────────────────────────

    #[test]
    fn test_clamp_selection_empty_sets_none() {
        let mut app = make_app();
        app.list_state.select(Some(5));
        app.clamp_selection();
        assert_eq!(app.list_state.selected(), None);
    }

    #[test]
    fn test_clamp_selection_out_of_bounds_clamps_to_last() {
        let mut app = make_app();
        app.suggested_files.push(path("/tmp/a.txt"));
        app.suggested_files.push(path("/tmp/b.txt"));
        app.list_state.select(Some(99));
        app.clamp_selection();
        assert_eq!(app.list_state.selected(), Some(1));
    }

    #[test]
    fn test_clamp_selection_none_selects_first() {
        let mut app = make_app();
        app.suggested_files.push(path("/tmp/a.txt"));
        app.list_state.select(None);
        app.clamp_selection();
        assert_eq!(app.list_state.selected(), Some(0));
    }

    #[test]
    fn test_clamp_selection_in_bounds_unchanged() {
        let mut app = make_app();
        app.suggested_files.push(path("/a.txt"));
        app.suggested_files.push(path("/b.txt"));
        app.suggested_files.push(path("/c.txt"));
        app.list_state.select(Some(1));
        app.clamp_selection();
        assert_eq!(app.list_state.selected(), Some(1));
    }

    // ── move_up / move_down ───────────────────────────────────────────────────

    #[test]
    fn test_move_up_decrements_selection() {
        let mut app = make_app();
        app.suggested_files.push(path("/a.txt"));
        app.suggested_files.push(path("/b.txt"));
        app.list_state.select(Some(1));
        app.move_up();
        assert_eq!(app.list_state.selected(), Some(0));
    }

    #[test]
    fn test_move_up_at_zero_stays_zero() {
        let mut app = make_app();
        app.suggested_files.push(path("/a.txt"));
        app.list_state.select(Some(0));
        app.move_up();
        assert_eq!(app.list_state.selected(), Some(0));
    }

    #[test]
    fn test_move_down_increments_selection() {
        let mut app = make_app();
        app.suggested_files.push(path("/a.txt"));
        app.suggested_files.push(path("/b.txt"));
        app.list_state.select(Some(0));
        app.move_down();
        assert_eq!(app.list_state.selected(), Some(1));
    }

    #[test]
    fn test_move_down_at_last_stays_at_last() {
        let mut app = make_app();
        app.suggested_files.push(path("/a.txt"));
        app.suggested_files.push(path("/b.txt"));
        app.list_state.select(Some(1));
        app.move_down();
        assert_eq!(app.list_state.selected(), Some(1));
    }

    // ── delete_selected ───────────────────────────────────────────────────────

    #[test]
    fn test_delete_selected_removes_file_and_sends_to_channel() {
        let (mut app, _, delete_rx) = make_app_with_channels();
        app.suggested_files.push(path("/tmp/del.txt"));
        app.list_state.select(Some(0));

        app.delete_selected();

        assert!(app.suggested_files.is_empty());
        assert!(app.status.contains("Deleted"));
        // Confirm the path was forwarded to the delete channel
        let received = delete_rx.try_recv().expect("path should have been sent");
        assert_eq!(received, path("/tmp/del.txt"));
    }

    #[test]
    fn test_delete_selected_nothing_selected_updates_status() {
        let mut app = make_app();
        app.list_state.select(None);
        app.delete_selected();
        assert_eq!(app.status, "No file selected.");
    }

    #[test]
    fn test_delete_selected_clamps_after_removal() {
        let (mut app, _, _delete_rx) = make_app_with_channels();
        app.suggested_files.push(path("/a.txt"));
        app.suggested_files.push(path("/b.txt"));
        app.suggested_files.push(path("/c.txt"));
        app.list_state.select(Some(2)); // last item

        app.delete_selected();

        // After removing the last item, selection should clamp to new last (index 1)
        assert_eq!(app.list_state.selected(), Some(1));
        assert_eq!(app.suggested_files.len(), 2);
    }

    // ── keep_selected ─────────────────────────────────────────────────────────

    #[test]
    fn test_keep_selected_removes_file_without_sending_to_delete() {
        let (mut app, _, delete_rx) = make_app_with_channels();
        app.suggested_files.push(path("/tmp/keep.txt"));
        app.list_state.select(Some(0));

        app.keep_selected();

        assert!(app.suggested_files.is_empty());
        assert!(app.status.contains("Kept"));
        // Nothing should have been sent to the delete channel
        assert!(delete_rx.try_recv().is_err());
    }

    #[test]
    fn test_keep_selected_nothing_selected_updates_status() {
        let mut app = make_app();
        app.list_state.select(None);
        app.keep_selected();
        assert_eq!(app.status, "No file selected.");
    }

    // ── never_delete_selected ─────────────────────────────────────────────────

    #[test]
    fn test_never_delete_removes_file_from_list() {
        let (mut app, _, _) = make_app_with_channels();
        app.suggested_files.push(path("/tmp/never.txt"));
        app.list_state.select(Some(0));

        app.never_delete_selected();

        assert!(app.suggested_files.is_empty());
        assert!(app.status.contains("never-delete"));
    }

    #[test]
    fn test_never_delete_nothing_selected_updates_status() {
        let mut app = make_app();
        app.list_state.select(None);
        app.never_delete_selected();
        assert_eq!(app.status, "No file selected.");
    }

    // ── poll_suggestions ──────────────────────────────────────────────────────

    #[test]
    fn test_poll_suggestions_adds_paths_to_list() {
        let (mut app, suggest_tx, _) = make_app_with_channels();
        suggest_tx.send(path("/tmp/new1.txt")).unwrap();
        suggest_tx.send(path("/tmp/new2.txt")).unwrap();

        app.poll_suggestions();

        assert_eq!(app.suggested_files.len(), 2);
        assert_eq!(app.suggested_files[0], path("/tmp/new1.txt"));
        assert_eq!(app.suggested_files[1], path("/tmp/new2.txt"));
    }

    #[test]
    fn test_poll_suggestions_sets_selection_when_first_item_arrives() {
        let (mut app, suggest_tx, _) = make_app_with_channels();
        app.list_state.select(None);  // start with no selection
        suggest_tx.send(path("/tmp/first.txt")).unwrap();

        app.poll_suggestions();

        assert_eq!(app.list_state.selected(), Some(0));
    }

    #[test]
    fn test_poll_suggestions_empty_channel_does_nothing() {
        let (mut app, _suggest_tx, _) = make_app_with_channels();
        app.poll_suggestions();
        assert!(app.suggested_files.is_empty());
    }

    #[test]
    fn test_poll_suggestions_does_not_reset_existing_selection() {
        let (mut app, suggest_tx, _) = make_app_with_channels();
        app.suggested_files.push(path("/existing.txt"));
        app.list_state.select(Some(0));

        suggest_tx.send(path("/new.txt")).unwrap();
        app.poll_suggestions();

        // Selection should still be 0 (unchanged) since it was already set
        assert_eq!(app.list_state.selected(), Some(0));
        assert_eq!(app.suggested_files.len(), 2);
    }
}

