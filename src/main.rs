use steady_state::*;
use std::time::Duration;
// use crate::actor::crawler::FileMeta;

// crate that adds in both the actors from the actor/ directory
pub(crate) mod actor {  
    pub(crate) mod crawler;
    pub(crate) mod db_manager;
}

//TODO: Add functionality for priority setting using screensaver api

fn main() -> Result<(), Box<dyn Error>> {

    init_logging(LogLevel::Info)?;   

    // pass unit value into .build() to ignore cli_args for now
    let mut graph = GraphBuilder::default().build(());

    build_graph(&mut graph); 

    graph.start();  

    // note: need to use Duration Crate to countdown seconds, rust will not take other data-types
    graph.block_until_stopped(Duration::from_secs(1)) 
}

const NAME_CRAWLER: &str = "CRAWLER";
const NAME_DB: &str = "DB_MANAGER";

fn build_graph(graph: &mut Graph) {

    // build channels and configure colors on graph if they fill up too much
    let channel_builder = graph.channel_builder()
        .with_filled_trigger(Trigger::AvgAbove(Filled::p90()), AlertColor::Red) //#!#//
        .with_filled_trigger(Trigger::AvgAbove(Filled::p60()), AlertColor::Orange)
        .with_filled_percentile(Percentile::p80());

    // Build Channels for Sender and Reciever Tx and Rx for communication between actors
    let (crawler_tx, crawler_rx) = channel_builder.build();

    // build actor interface
    let actor_builder = graph.actor_builder()
        .with_load_avg()
        .with_mcpu_avg();

    // sender actor
    actor_builder.with_name(NAME_CRAWLER)
        .build(move |actor| actor::crawler::run(actor, crawler_tx.clone()) 
               , SoloAct);

    // receiver actor
    actor_builder.with_name(NAME_DB)
        .build(move |actor| actor::db_manager::run(actor, crawler_rx.clone()) 
               , SoloAct);

}
