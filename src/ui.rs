use anyhow::{anyhow, Result};

slint::include_modules!();

use crate::report;

pub fn run_ui() -> Result<()> {
    let ui = AppWindow::new()?;
    let ui_weak = ui.as_weak();

    // Load Report button
    ui.on_load_report({
        let ui_weak = ui_weak.clone();
        move || {
            // ✅ Keep file picker on UI thread
            let picked = rfd::FileDialog::new()
                .add_filter("TSV report", &["tsv"])
                .pick_file();

            let Some(path) = picked else {
                slint::invoke_from_event_loop({
                    let ui_weak = ui_weak.clone();
                    move || {
                        if let Some(ui) = ui_weak.upgrade() {
                            ui.set_status_text("Load canceled.".into());
                            ui.set_progress_value(0);
                        }
                    }
                })
                .unwrap();
                return;
            };

            let path_str = path.to_string_lossy().to_string();

            // ✅ Start background thread for parsing + progress updates
            let ui_weak_thread = ui_weak.clone();
            std::thread::spawn(move || {
                // start
                slint::invoke_from_event_loop({
                    let ui_weak = ui_weak_thread.clone();
                    let path_str = path_str.clone();
                    move || {
                        if let Some(ui) = ui_weak.upgrade() {
                            ui.set_status_text(format!("Loading {path_str} ...").into());
                            ui.set_progress_value(0);
                        }
                    }
                })
                .unwrap();

                // fake progress so UI clearly animates
                for p in 0..=30 {
                    std::thread::sleep(std::time::Duration::from_millis(10));
                    slint::invoke_from_event_loop({
                        let ui_weak = ui_weak_thread.clone();
                        move || {
                            if let Some(ui) = ui_weak.upgrade() {
                                ui.set_progress_value(p);
                                ui.set_status_text(format!("Loading report... {}%", p).into());
                            }
                        }
                    })
                    .unwrap();
                }

                // ✅ Real work: parse TSV off the UI thread (metadata-only now)
                let loaded = report::load_llm_report_tsv(&path_str);

                match loaded {
                    Ok(rows) => {
                        // ✅ Step 7: create RowData with Pending status
                        let slint_rows: Vec<RowData> = rows
                            .into_iter()
                            .map(|r| RowData {
                                id: r.id.parse::<i32>().unwrap_or(0),
                                path: r.path.into(),

                                // ✅ Size comes from TSV (or "—" from parser)
                                size: r.size.into(),

                                // ✅ Hardcoded until LLM fills it
                                reason: "Pending...".into(),
                                decision: "PENDING".into(),
                            })
                            .collect();

                        let count = slint_rows.len();

                        // finish progress to 100
                        for p in 31..=100 {
                            std::thread::sleep(std::time::Duration::from_millis(5));
                            slint::invoke_from_event_loop({
                                let ui_weak = ui_weak_thread.clone();
                                move || {
                                    if let Some(ui) = ui_weak.upgrade() {
                                        ui.set_progress_value(p);
                                        ui.set_status_text(format!("Loading report... {}%", p).into());
                                    }
                                }
                            })
                            .unwrap();
                        }

                        // ✅ Apply rows + final status on UI thread
                        slint::invoke_from_event_loop({
                            let ui_weak = ui_weak_thread.clone();
                            move || {
                                if let Some(ui) = ui_weak.upgrade() {
                                    ui.set_rows(slint::ModelRc::new(slint::VecModel::from(slint_rows)));
                                    ui.set_status_text(format!("Loaded {count} rows ✅").into());
                                }
                            }
                        })
                        .unwrap();
                    }
                    Err(e) => {
                        // ✅ Error UI update on UI thread
                        slint::invoke_from_event_loop({
                            let ui_weak = ui_weak_thread.clone();
                            move || {
                                if let Some(ui) = ui_weak.upgrade() {
                                    ui.set_progress_value(0);
                                    ui.set_status_text(format!("Failed to load report: {e}").into());
                                }
                            }
                        })
                        .unwrap();
                    }
                }
            });
        }
    });

    // Start the UI event loop
    ui.run().map_err(|e| anyhow!(e))?;
    Ok(())
}
