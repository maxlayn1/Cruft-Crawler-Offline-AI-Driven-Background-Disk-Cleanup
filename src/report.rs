use anyhow::{Context, Result};
use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct ReportRow {
    pub id: String,

    // ✅ Read ONLY metadata from TSV
    pub path: String,

    // ✅ If TSV has Size column, read it; if not present, default to "—"
    #[serde(default = "default_size")]
    pub size: String,
}

fn default_size() -> String {
    "—".to_string()
}

pub fn load_llm_report_tsv(path: &str) -> Result<Vec<ReportRow>> {
    let mut rdr = csv::ReaderBuilder::new()
        .delimiter(b'\t')
        .has_headers(true)
        .from_path(path)
        .with_context(|| format!("Failed to open report file: {path}"))?;

    let mut out = Vec::new();

    for rec in rdr.deserialize::<ReportRow>() {
        let mut row = rec.with_context(|| "Failed to parse TSV row")?;

        // ✅ If size column exists but is empty, normalize to "—"
        if row.size.trim().is_empty() {
            row.size = "—".to_string();
        }

        // ✅ Skip empty paths safely
        if row.path.trim().is_empty() {
            continue;
        }

        out.push(row);
    }

    Ok(out)
}
