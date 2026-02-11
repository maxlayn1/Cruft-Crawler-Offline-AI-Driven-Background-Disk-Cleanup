use anyhow::{anyhow, Result};

slint::include_modules!();

use crate::report;

pub fn run_ui() -> Result<()> {
    let ui = AppWindow::new()?;

    // Load Report button
    {
        let ui_weak = ui.as_weak();

        ui.on_load_report(move || {
            if let Some(ui) = ui_weak.upgrade() {
                // Open file picker
                let picked = rfd::FileDialog::new()
                    .add_filter("TSV report", &["tsv"])
                    .pick_file();

                let Some(path) = picked else {
                    ui.set_status_text("Load canceled.".into());
                    return;
                };

                let path_str = path.to_string_lossy().to_string();
                ui.set_status_text(format!("Loading {path_str} ...").into());

                match report::load_llm_report_tsv(&path_str) {
                    Ok(rows) => {
                        let slint_rows: Vec<ReportRow> = rows
                            .into_iter()
                            .map(|r| ReportRow {
                                id: r.id.into(),
                                path: r.path.into(),
                                decision: r.decision.into(),
                            })
                            .collect();

                        let count = slint_rows.len(); // compute BEFORE move

                        ui.set_rows(slint::ModelRc::new(
                            slint::VecModel::from(slint_rows),
                        ));

                        ui.set_status_text(
                            format!("Loaded {count} rows ✅").into(),
                        );
                    }
                    Err(e) => {
                        ui.set_status_text(
                            format!("Failed to load report: {e}").into(),
                        );
                    }
                }
            }
        });
    } // important: block ends here

    // Start the UI event loop
    ui.run().map_err(|e| anyhow!(e))?;

    Ok(())
}
