use steady_state::*;
use std::time::Duration;
// use crate::actor::crawler::FileMeta;

// crate that adds in both the actors from the actor/ directory
pub(crate) mod actor {  
    pub(crate) mod crawler;
    pub(crate) mod db_manager;
    pub(crate) mod ai_model;
    pub(crate) mod user_interface;
    pub(crate) mod file_handler;
}

//TODO: Add functionality for priority setting using screensaver api

fn main() -> Result<(), Box<dyn Error>> {

    init_logging(LogLevel::Info)?;   

    // pass unit value into .build() to ignore cli_args for now
    let mut graph = GraphBuilder::default().build(());

    build_graph(&mut graph); 

    graph.start();  

    graph.block_until_stopped(Duration::from_secs(1)) 
}

const NAME_CRAWLER: &str = "CRAWLER";
const NAME_DB: &str = "DB_MANAGER";
const NAME_AI_MODEL: &str = "AI_MODEL";
const NAME_UI_ACTOR: &str = "UI_ACTOR";
const NAME_FILE_HANDLER: &str = "FILE_HANDLER_ACTOR";

fn build_graph(graph: &mut Graph) {

    // build channels and configure colors on graph if they fill up too much
    let channel_builder = graph.channel_builder()
        .with_filled_trigger(Trigger::AvgAbove(Filled::p90()), AlertColor::Red) 
        .with_filled_trigger(Trigger::AvgAbove(Filled::p60()), AlertColor::Orange)
        .with_filled_percentile(Percentile::p80());

    // Build Channels for Sender and Reciever Tx and Rx for communication between actors
    let (crawler_to_db_tx, crawler_to_db_rx) = channel_builder.build();

    let (crawler_to_ai_model_tx, crawler_to_ai_model_rx) = channel_builder.build();

    let (ai_model_to_ui_tx, ai_model_to_ui_rx) = channel_builder.build();

    let (ui_to_file_handler_tx, ui_to_file_handler_rx) = channel_builder.build();

    let (file_handler_to_db_tx, file_handler_to_db_rx) = channel_builder.build();



    // build actor interface
    let actor_builder = graph.actor_builder()
        .with_load_avg()
        .with_mcpu_avg();

    // sender actor
    let state = new_state();
    actor_builder.with_name(NAME_CRAWLER)
        .build(move |actor| actor::crawler::run(actor, crawler_to_db_tx.clone(), crawler_to_ai_model_tx.clone(), state.clone()) 
               , SoloAct);

    // receiver actor
    actor_builder.with_name(NAME_DB)
        .build(move |actor| actor::db_manager::run(actor, crawler_to_db_rx.clone(), file_handler_to_db_rx.clone()) 
               , SoloAct);

    actor_builder.with_name(NAME_AI_MODEL)
        .build(move |actor | actor::ai_model::run(actor, crawler_to_ai_model_rx.clone(), ai_model_to_ui_tx.clone())
                , SoloAct); 
    
     actor_builder.with_name(NAME_UI_ACTOR)
        .build(move |actor | actor::user_interface::run(actor, ai_model_to_ui_rx.clone(), ui_to_file_handler_tx.clone())
                , SoloAct); 

    actor_builder.with_name(NAME_FILE_HANDLER)
        .build(move |actor | actor::file_handler::run(actor, ui_to_file_handler_rx.clone(), file_handler_to_db_tx.clone())
                , SoloAct);
                


}
