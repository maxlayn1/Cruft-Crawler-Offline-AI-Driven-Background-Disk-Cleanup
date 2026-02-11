use anyhow::{Context, Result};
use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct ReportRow {
    pub id: String,
    pub path: String,
    pub decision: String,
}

pub fn load_llm_report_tsv(path: &str) -> Result<Vec<ReportRow>> {
    let mut rdr = csv::ReaderBuilder::new()
        .delimiter(b'\t')
        .has_headers(true)
        .from_path(path)
        .with_context(|| format!("Failed to open report file: {path}"))?;

    let mut out = Vec::new();

    for rec in rdr.deserialize::<ReportRow>() {
        let row = rec.with_context(|| "Failed to parse TSV row")?;
        out.push(row);
    }

    Ok(out)
}
