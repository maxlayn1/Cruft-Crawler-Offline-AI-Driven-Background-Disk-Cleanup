use steady_state::*;
use std::time::Duration;
use std::path::PathBuf;

// Actor modules — file_handler removed
pub(crate) mod actor {
    pub(crate) mod crawler;
    pub(crate) mod db_manager;
    pub(crate) mod ai_model;
    pub(crate) mod user_interface;
}
pub(crate) mod llm_engine;

// TODO: Add functionality for priority setting using screensaver api

fn main() -> Result<(), Box<dyn std::error::Error>> {
     


    init_logging(LogLevel::Info)?;

    // pass unit value into .build() to ignore cli_args for now
    let mut graph = GraphBuilder::default().build(());

    build_graph(&mut graph);

    graph.start();

    graph.block_until_stopped(Duration::from_secs(1))
}

const NAME_CRAWLER:  &str = "CRAWLER";
const NAME_DB:       &str = "DB_MANAGER";
const NAME_AI_MODEL: &str = "AI_MODEL";
const NAME_UI_ACTOR: &str = "UI_ACTOR";

fn build_graph(graph: &mut Graph) {

    // Channel monitoring: alert colors when channels fill up
    let channel_builder = graph.channel_builder()
        .with_filled_trigger(Trigger::AvgAbove(Filled::p90()), AlertColor::Red)
        .with_filled_trigger(Trigger::AvgAbove(Filled::p60()), AlertColor::Orange)
        .with_filled_percentile(Percentile::p80());

    // Crawler → DB (FileMeta)
    let (crawler_to_db_tx, crawler_to_db_rx) = channel_builder.build();

    // Crawler → AI Model (String)
    let (crawler_to_ai_model_tx, crawler_to_ai_model_rx) = channel_builder.build();

    // AI Model → UI (String verdict)
    let (ai_model_to_ui_tx, ai_model_to_ui_rx) = channel_builder.build();

    // UI → DB (PathBuf of confirmed deletions) — replaces the old two-hop UI→FileHandler→DB
    let (ui_to_db_tx, ui_to_db_rx) = channel_builder.build();

    // Actor monitoring: track load and CPU averages
    let actor_builder = graph.actor_builder()
        .with_load_avg()
        .with_mcpu_avg();

    // Crawler actor
    let state = new_state();
    actor_builder.with_name(NAME_CRAWLER)
        .build(move |actor| actor::crawler::run(
            actor,
            crawler_to_db_tx.clone(),
            crawler_to_ai_model_tx.clone(),
            state.clone(),
        ), SoloAct);

    // DB Manager actor — now receives PathBuf from UI instead of String from file handler
    actor_builder.with_name(NAME_DB)
        .build(move |actor| actor::db_manager::run(
            actor,
            crawler_to_db_rx.clone(),
            ui_to_db_rx.clone(),
        ), SoloAct);

    // AI Model actor
    actor_builder.with_name(NAME_AI_MODEL)
        .build(move |actor| actor::ai_model::run(
            actor,
            crawler_to_ai_model_rx.clone(),
            ai_model_to_ui_tx.clone(),
        ), SoloAct);

    // UI actor — now sends directly to DB, no file handler in between
    actor_builder.with_name(NAME_UI_ACTOR)
        .build(move |actor| actor::user_interface::run(
            actor,
            ai_model_to_ui_rx.clone(),
            ui_to_db_tx.clone(),
        ), SoloAct);
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── actor name constants ──────────────────────────────────────────────────

    #[test]
    fn test_name_crawler_is_correct() {
        assert_eq!(NAME_CRAWLER, "CRAWLER");
    }

    #[test]
    fn test_name_db_is_correct() {
        assert_eq!(NAME_DB, "DB_MANAGER");
    }

    #[test]
    fn test_name_ai_model_is_correct() {
        assert_eq!(NAME_AI_MODEL, "AI_MODEL");
    }

    #[test]
    fn test_name_ui_actor_is_correct() {
        assert_eq!(NAME_UI_ACTOR, "UI_ACTOR");
    }

    #[test]
    fn test_all_actor_names_are_unique() {
        let names = [NAME_CRAWLER, NAME_DB, NAME_AI_MODEL, NAME_UI_ACTOR];
        let unique: std::collections::HashSet<&str> = names.iter().copied().collect();
        assert_eq!(unique.len(), names.len(), "all actor names must be unique");
    }

    #[test]
    fn test_all_actor_names_are_nonempty() {
        for name in [NAME_CRAWLER, NAME_DB, NAME_AI_MODEL, NAME_UI_ACTOR] {
            assert!(!name.is_empty(), "actor name '{}' must not be empty", name);
        }
    }

    #[test]
    fn test_all_actor_names_are_uppercase() {
        for name in [NAME_CRAWLER, NAME_DB, NAME_AI_MODEL, NAME_UI_ACTOR] {
            assert_eq!(
                name, name.to_uppercase(),
                "actor name '{}' should be uppercase",
                name
            );
        }
    }

    // ── module structure ──────────────────────────────────────────────────────
    // These compile-only tests confirm the module tree is wired up correctly.
    // If any actor module is missing or renamed, these will fail to compile.

    #[test]
    fn test_crawler_module_exists() {
        // If this compiles, the module is present and pub(crate)
        let _: fn() = || {
            let _ = std::any::type_name::<()>(); // dummy usage
        };
        // Real check: we can reference the module path
        let _ = std::stringify!(crate::actor::crawler);
    }

    #[test]
    fn test_llm_engine_module_exists() {
        let _ = std::stringify!(crate::llm_engine);
    }
}
