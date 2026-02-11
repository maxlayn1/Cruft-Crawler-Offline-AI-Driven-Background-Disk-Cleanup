use anyhow::Result;

slint::include_modules!();

use crate::report;

pub fn run_ui() -> Result<()> {
    let ui = AppWindow::new()?;

    // Load Report button
    {
        let ui_weak = ui.as_weak();
        ui.on_load_report(move || {
            if let Some(ui) = ui_weak.upgrade() {
                ui.set_status_text("Loading data/llm_report.tsv...".into());

                match report::load_llm_report_tsv("data/llm_report.tsv") {
                    Ok(rows) => {
                        let slint_rows: Vec<ReportRow> = rows
                            .into_iter()
                            .map(|r| ReportRow {
                                id: r.id.into(),
                                path: r.path.into(),
                                decision: r.decision.into(),
                            })
                            .collect();

                        ui.set_rows(slint::ModelRc::new(slint::VecModel::from(slint_rows)));
                        ui.set_status_text("Loaded report ✅".into());
                    }
                    Err(e) => {
                        ui.set_status_text(format!("Failed to load report: {e}").into());
                    }
                }
            }
        });
    }

    // Run Scan button (placeholder for now)
    {
        let ui_weak = ui.as_weak();
        ui.on_run_scan(move || {
            if let Some(ui) = ui_weak.upgrade() {
                ui.set_status_text("Run Scan: not wired yet (next step)".into());
            }
        });
    }

    ui.run()?;
    Ok(())
}
