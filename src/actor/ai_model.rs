#![allow(unused)]

use libc::printf;
use steady_state::*;
use crate::llm_engine::LlmEngine;
use crate::actor::crawler::FileMeta;
use std::fs;
use std::path::PathBuf;

// Scans `./src/models/` and returns the path to the first `.gguf` file found.
/// Returns an error if the directory doesn't exist or contains no `.gguf` files.
fn find_model_file() -> Result<PathBuf, Box<dyn std::error::Error>> {
    let models_dir = std::path::Path::new("./src/models");

    let entry = fs::read_dir(models_dir)
        .map_err(|e| format!("Could not open models directory './src/models': {}", e))?
        .filter_map(|res| res.ok())
        .map(|e| e.path())
        .find(|p| p.extension().and_then(|ext| ext.to_str()) == Some("gguf"))
        .ok_or_else(|| "No .gguf model file found in './src/models'".to_string())?;

    Ok(entry)
}



pub async fn run(
    actor: SteadyActorShadow,
    crawler_to_model_rx: SteadyRx<FileMeta>,
    ai_model_to_ui_tx: SteadyTx<String>,
) -> Result<(), Box<dyn std::error::Error>> {
    let actor = actor.into_spotlight([&crawler_to_model_rx], [&ai_model_to_ui_tx]);

    if actor.use_internal_behavior {
        internal_behavior(actor, crawler_to_model_rx, ai_model_to_ui_tx).await
    } else {
        actor.simulated_behavior(vec!(&crawler_to_model_rx)).await
    }
}

async fn internal_behavior<A: SteadyActor>(
    mut actor: A,
    crawler_to_ai_model_rx: SteadyRx<FileMeta>,
    ai_model_to_ui_tx: SteadyTx<String>,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut crawler_to_ai_model_rx = crawler_to_ai_model_rx.lock().await;
    let mut ai_model_to_ui_tx = ai_model_to_ui_tx.lock().await;

    let model_path = find_model_file()?;
    let model_path_str = model_path
        .to_str()
        .ok_or("Model path contains invalid UTF-8")?;
    eprintln!("AI_MODEL: loading model from {}", model_path_str);

    let engine = match LlmEngine::load_new_model(model_path_str) {
        Ok(e) => e,
        Err(e) => return Err(e.into()),
    };

    while actor.is_running(|| crawler_to_ai_model_rx.is_closed_and_empty() || ai_model_to_ui_tx.mark_closed()) {
        await_for_all!(
            actor.wait_avail(&mut crawler_to_ai_model_rx, 1),
            actor.wait_vacant(&mut ai_model_to_ui_tx, 1)
        );

        let file_meta = match actor.try_take(&mut crawler_to_ai_model_rx) {
            Some(m) => m,
            None => continue,
        };

        let prompt = build_prompt(&file_meta);
       
        let verdict = match engine.infer_model(&prompt) {
            Ok(raw) => {
                //eprintln!("AI_MODEL: raw output: {:?}", raw);
                parse_verdict(&raw)
            }
            Err(e) => {
                //eprintln!("AI_MODEL: inference FAILED: {}", e);
                continue;
            }
        };

        let message = format!("{}|{}", verdict, file_meta.abs_path.display());
        loop {
            actor.wait_vacant(&mut ai_model_to_ui_tx, 1).await;
            match actor.try_send(&mut ai_model_to_ui_tx, message.clone()) {
                SendOutcome::Success => break,
                SendOutcome::Blocked(_) => continue,
                other => break,
            }
        }
    }

    Ok(())
}

/// Parses the model's response to find "Decision: keep" or "Decision: delete".
/// Falls back to scanning for bare "keep"/"delete" keywords if the structured
/// line is absent. Defaults to "keep" when nothing matches.
fn parse_verdict(raw: &str) -> String {
    let lower = raw.to_lowercase();

    // Primary: look for "decision: <verdict>" anywhere in the output
    for line in lower.lines() {
        let trimmed = line.trim();
        if let Some(rest) = trimmed.strip_prefix("decision:") {
            let word = rest
                .split_whitespace()
                .next()
                .unwrap_or("")
                .chars()
                .filter(|c| c.is_alphabetic())
                .collect::<String>();

            if word == "delete" || word == "keep" {
                return word;
            }
        }
    }

    // Fallback: scan every word for an explicit keep/delete keyword
    for word in lower.split_whitespace() {
        let clean: String = word.chars().filter(|c| c.is_alphabetic()).collect();
        if clean == "delete" || clean == "keep" {
            return clean;
        }
    }

    // Default: when uncertain, always keep
    "keep".to_string()
}

fn build_prompt(meta: &FileMeta) -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;
    let days_since_modified = (now - meta.modified) / 86400;

    format!(
        r#"You are a file management assistant. Your job is to decide whether a file should be kept or deleted based on its metadata.

        ### Decision Rules
        - DELETE if: the file has not been accessed or modified in over 365 days AND the file name suggests it is temporary, a draft, a cache, or a duplicate (e.g., contains "tmp", "temp", "cache", "copy", "backup", "old", "~", or ends in ".log", ".bak", ".swp")
        - DELETE if: the file is very small (under 512 bytes), has not been modified in over 180 days, and the name suggests it is a leftover or auto-generated artifact
        - KEEP if: the file is read-only (system or protected files are rarely safe to delete)
        - KEEP if: the file has been modified recently (within 30 days)
        - KEEP if: uncertain — always prefer keeping over deleting

        ### Examples
        File: "cache_session_1A2B.tmp", 204 bytes, 412 days old, read-only: false
        Reasoning: Name contains "cache" and ".tmp", very old, small, not protected.
        Decision: delete

        File: "project_report_final.pdf", 84200 bytes, 5 days old, read-only: false
        Reasoning: Recently modified, meaningful name, substantial size.
        Decision: keep

        File: "libsystem_kernel.dylib", 512 bytes, 730 days old, read-only: true
        Reasoning: Read-only flag suggests system file. Never delete.
        Decision: keep

        File: "notes_backup_old.txt", 1100 bytes, 200 days old, read-only: false
        Reasoning: Name contains "backup" and "old", moderately old, small, not protected.
        Decision: delete

        ### Now decide for this file
        File: "{}", {} bytes, {} days since last modified, read-only: {}
        Reasoning:"#,
        meta.file_name,
        meta.size,
        days_since_modified,
        meta.readonly,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use crate::actor::crawler::FileMeta;

    // ── helpers ──────────────────────────────────────────────────────────────

    fn make_meta(
        file_name: &str,
        size: u64,
        modified: i64,
        readonly: bool,
        abs_path: &str,
    ) -> FileMeta {
        FileMeta {
            file_name: file_name.to_string(),
            size,
            modified,
            readonly,
            abs_path: PathBuf::from(abs_path),
            rel_path: PathBuf::from(file_name),   // relative path, just use file_name as a stub
            hash: String::new(),                   // empty hash for testing purposes
            is_file: true,                         // assume it's a file in all test cases
            created: 0,                            // epoch default, same as modified
        }
    }

    // ── parse_verdict: primary "Decision:" path ───────────────────────────────

    #[test]
    fn test_parse_verdict_decision_delete_exact() {
        let raw = "Reasoning: old file.\nDecision: delete";
        assert_eq!(parse_verdict(raw), "delete");
    }

    #[test]
    fn test_parse_verdict_decision_keep_exact() {
        let raw = "Reasoning: recently used.\nDecision: keep";
        assert_eq!(parse_verdict(raw), "keep");
    }

    #[test]
    fn test_parse_verdict_decision_case_insensitive() {
        let raw = "DECISION: DELETE";
        assert_eq!(parse_verdict(raw), "delete");
    }

    #[test]
    fn test_parse_verdict_decision_with_punctuation() {
        // "delete." — the char filter strips the period
        let raw = "Decision: delete.";
        assert_eq!(parse_verdict(raw), "delete");
    }

    #[test]
    fn test_parse_verdict_decision_keep_with_trailing_text() {
        let raw = "Decision: keep this file because it is important";
        assert_eq!(parse_verdict(raw), "keep");
    }

    #[test]
    fn test_parse_verdict_decision_delete_with_trailing_text() {
        let raw = "Decision: delete – no longer needed";
        assert_eq!(parse_verdict(raw), "delete");
    }

    #[test]
    fn test_parse_verdict_decision_mixed_case_keep() {
        let raw = "Decision: Keep";
        assert_eq!(parse_verdict(raw), "keep");
    }

    // ── parse_verdict: fallback keyword scan ─────────────────────────────────

    #[test]
    fn test_parse_verdict_fallback_bare_delete() {
        let raw = "This file is a cache artifact and should be delete";
        assert_eq!(parse_verdict(raw), "delete");
    }

    #[test]
    fn test_parse_verdict_fallback_bare_keep() {
        let raw = "You should keep this important document.";
        assert_eq!(parse_verdict(raw), "keep");
    }

    #[test]
    fn test_parse_verdict_fallback_delete_wins_first_occurrence() {
        // "delete" appears before "keep" in word order → should return "delete"
        let raw = "please delete or keep";
        assert_eq!(parse_verdict(raw), "delete");
    }

    #[test]
    fn test_parse_verdict_fallback_keep_wins_first_occurrence() {
        let raw = "keep or maybe delete later";
        assert_eq!(parse_verdict(raw), "keep");
    }

    #[test]
    fn test_parse_verdict_fallback_word_with_punctuation() {
        // "delete," — punctuation stripped by char filter
        let raw = "verdict: delete, it is old";
        assert_eq!(parse_verdict(raw), "delete");
    }

    // ── parse_verdict: default "keep" path ───────────────────────────────────

    #[test]
    fn test_parse_verdict_default_empty_string() {
        assert_eq!(parse_verdict(""), "keep");
    }

    #[test]
    fn test_parse_verdict_default_no_keywords() {
        let raw = "The analysis is inconclusive. No action recommended.";
        assert_eq!(parse_verdict(raw), "keep");
    }

    #[test]
    fn test_parse_verdict_default_unrelated_decision_line() {
        // "decision: uncertain" — neither keep nor delete, falls to scan
        let raw = "Decision: uncertain about this file";
        // "uncertain", "about", "this", "file" — none match, defaults to keep
        assert_eq!(parse_verdict(raw), "keep");
    }

    #[test]
    fn test_parse_verdict_default_whitespace_only() {
        assert_eq!(parse_verdict("   \n\t  "), "keep");
    }

    // ── parse_verdict: edge cases ─────────────────────────────────────────────

    #[test]
    fn test_parse_verdict_multiline_decision_block() {
        let raw = r#"
            File has not been accessed in 400 days.
            Name contains "tmp" suffix.
            Decision: delete
        "#;
        assert_eq!(parse_verdict(raw), "delete");
    }

    #[test]
    fn test_parse_verdict_decision_colon_no_space() {
        // "decision:delete" — strip_prefix("decision:") gives "delete"
        let raw = "decision:delete";
        assert_eq!(parse_verdict(raw), "delete");
    }

    #[test]
    fn test_parse_verdict_decision_keep_no_space() {
        let raw = "decision:keep";
        assert_eq!(parse_verdict(raw), "keep");
    }

    #[test]
    fn test_parse_verdict_partial_word_does_not_match() {
        // "deleted" and "keeper" should NOT trigger the fallback match
        let raw = "The file was deleted from the system by keeper process.";
        // "deleted" → clean = "deleted" ≠ "delete"; "keeper" → "keeper" ≠ "keep"
        // → default keep
        assert_eq!(parse_verdict(raw), "keep");
    }

    // ── build_prompt ──────────────────────────────────────────────────────────

    #[test]
    fn test_build_prompt_contains_file_name() {
        let meta = make_meta("cache_old.tmp", 200, 0, false, "/tmp/cache_old.tmp");
        let prompt = build_prompt(&meta);
        assert!(prompt.contains("cache_old.tmp"));
    }

    #[test]
    fn test_build_prompt_contains_size() {
        let meta = make_meta("report.pdf", 84200, 0, false, "/home/jace/report.pdf");
        let prompt = build_prompt(&meta);
        assert!(prompt.contains("84200"));
    }

    #[test]
    fn test_build_prompt_contains_readonly_false() {
        let meta = make_meta("draft.txt", 100, 0, false, "/tmp/draft.txt");
        let prompt = build_prompt(&meta);
        assert!(prompt.contains("false"));
    }

    #[test]
    fn test_build_prompt_contains_readonly_true() {
        let meta = make_meta("libkernel.dylib", 512, 0, true, "/usr/lib/libkernel.dylib");
        let prompt = build_prompt(&meta);
        assert!(prompt.contains("true"));
    }

    #[test]
    fn test_build_prompt_days_since_modified_recent() {
        // modified = now → 0 days
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;
        let meta = make_meta("fresh.rs", 5000, now, false, "/src/fresh.rs");
        let prompt = build_prompt(&meta);
        assert!(prompt.contains("0 days since last modified"));
    }

    #[test]
    fn test_build_prompt_days_since_modified_old() {
        // modified 400 days ago
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;
        let old_ts = now - (400 * 86400);
        let meta = make_meta("old_cache.tmp", 100, old_ts, false, "/tmp/old_cache.tmp");
        let prompt = build_prompt(&meta);
        assert!(prompt.contains("400 days since last modified"));
    }

    #[test]
    fn test_build_prompt_contains_decision_rules_header() {
        let meta = make_meta("file.txt", 1000, 0, false, "/tmp/file.txt");
        let prompt = build_prompt(&meta);
        assert!(prompt.contains("Decision Rules"));
    }

    #[test]
    fn test_build_prompt_contains_examples_section() {
        let meta = make_meta("file.txt", 1000, 0, false, "/tmp/file.txt");
        let prompt = build_prompt(&meta);
        assert!(prompt.contains("Examples"));
    }

    #[test]
    fn test_build_prompt_structure_ends_with_reasoning_prompt() {
        let meta = make_meta("file.txt", 1000, 0, false, "/tmp/file.txt");
        let prompt = build_prompt(&meta);
        assert!(prompt.trim_end().ends_with("Reasoning:"));
    }

    #[test]
    fn test_build_prompt_zero_modified_timestamp() {
        // modified = 0 (epoch) → very large days value, just ensure no panic
        let meta = make_meta("ancient.log", 50, 0, false, "/var/log/ancient.log");
        let _prompt = build_prompt(&meta);  // must not panic
    }

    #[test]
    fn test_build_prompt_special_chars_in_filename() {
        let meta = make_meta("my file (copy) ~backup.bak", 300, 0, false, "/tmp/x.bak");
        let prompt = build_prompt(&meta);
        assert!(prompt.contains("my file (copy) ~backup.bak"));
    }
}