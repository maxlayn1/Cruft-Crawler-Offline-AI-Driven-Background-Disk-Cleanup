// src/main.rs
// Only change from your original: added file_decision module,
// and updated the two channel types that now carry FileDecision.

use steady_state::*;
use std::time::Duration;
<<<<<<< Updated upstream
use std::path::PathBuf;

// Actor modules — file_handler removed
=======

use crate::llm_engine::LlmEngine;

>>>>>>> Stashed changes
pub(crate) mod actor {
    pub(crate) mod crawler;
    pub(crate) mod db_manager;
    pub(crate) mod ai_model;
    pub(crate) mod user_interface;
}
pub(crate) mod llm_engine;
<<<<<<< Updated upstream

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
=======
pub(crate) mod file_decision;   // ← NEW

fn main() -> Result<(), Box<dyn Error>> {
    init_logging(LogLevel::Info, None)?;
    let mut graph = GraphBuilder::default().build(());
    build_graph(&mut graph);
    graph.start();
    graph.block_until_stopped(Duration::from_secs(1))
}

const NAME_CRAWLER:      &str = "CRAWLER";
const NAME_DB:           &str = "DB_MANAGER";
const NAME_AI_MODEL:     &str = "AI_MODEL";
const NAME_UI_ACTOR:     &str = "UI_ACTOR";
const NAME_FILE_HANDLER: &str = "FILE_HANDLER_ACTOR";

fn build_graph(graph: &mut Graph) {

>>>>>>> Stashed changes
    let channel_builder = graph.channel_builder()
        .with_filled_trigger(Trigger::AvgAbove(Filled::p90()), AlertColor::Red)
        .with_filled_trigger(Trigger::AvgAbove(Filled::p60()), AlertColor::Orange)
        .with_filled_percentile(Percentile::p80());

<<<<<<< Updated upstream
    // Crawler → DB (FileMeta)
    let (crawler_to_db_tx, crawler_to_db_rx) = channel_builder.build();

    // Crawler → AI Model (String)
    let (crawler_to_ai_model_tx, crawler_to_ai_model_rx) = channel_builder.build();

    // AI Model → UI (String verdict)
    let (ai_model_to_ui_tx, ai_model_to_ui_rx) = channel_builder.build();

    // UI → DB (PathBuf of confirmed deletions) — replaces the old two-hop UI→FileHandler→DB
    let (ui_to_db_tx, ui_to_db_rx) = channel_builder.build();

    // Actor monitoring: track load and CPU averages
=======
    // crawler → db_manager            (FileMeta — unchanged)
    let (crawler_to_db_tx, crawler_to_db_rx) = channel_builder.build();

    // crawler → ai_model              (FileMeta — was String)
    let (crawler_to_ai_model_tx, crawler_to_ai_model_rx) = channel_builder.build();

    // ai_model → ui_actor             (FileDecision — was String)
    let (ai_model_to_ui_tx, ai_model_to_ui_rx) = channel_builder.build();

    // ui_actor → file_handler         (String file name — unchanged)
    let (ui_to_file_handler_tx, ui_to_file_handler_rx) = channel_builder.build();

    // file_handler → db_manager       (String — unchanged)
    let (file_handler_to_db_tx, file_handler_to_db_rx) = channel_builder.build();

>>>>>>> Stashed changes
    let actor_builder = graph.actor_builder()
        .with_load_avg()
        .with_mcpu_avg();

<<<<<<< Updated upstream
    // Crawler actor
=======
>>>>>>> Stashed changes
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
<<<<<<< Updated upstream
            ui_to_db_rx.clone(),
=======
            file_handler_to_db_rx.clone(),
>>>>>>> Stashed changes
        ), SoloAct);

    // AI Model actor
    actor_builder.with_name(NAME_AI_MODEL)
        .build(move |actor| actor::ai_model::run(
            actor,
            crawler_to_ai_model_rx.clone(),
            ai_model_to_ui_tx.clone(),
        ), SoloAct);
<<<<<<< Updated upstream

    // UI actor — now sends directly to DB, no file handler in between
    actor_builder.with_name(NAME_UI_ACTOR)
        .build(move |actor| actor::user_interface::run(
            actor,
            ai_model_to_ui_rx.clone(),
            ui_to_db_tx.clone(),
        ), SoloAct);
}
=======

    actor_builder.with_name(NAME_UI_ACTOR)
        .build(move |actor| actor::user_interface::run(
            actor,
            ai_model_to_ui_rx.clone(),
            ui_to_file_handler_tx.clone(),
        ), SoloAct);

    actor_builder.with_name(NAME_FILE_HANDLER)
        .build(move |actor| actor::file_handler::run(
            actor,
            ui_to_file_handler_rx.clone(),
            file_handler_to_db_tx.clone(),
        ), SoloAct);
}
>>>>>>> Stashed changes
