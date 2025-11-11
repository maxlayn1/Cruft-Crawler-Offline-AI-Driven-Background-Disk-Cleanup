pub mod args;
use steady_state::*;
use args::Args;
use std::time::Duration;

pub(crate) mod actors { 
    pub(crate) mod file_crawler_2;
    pub(crate) mod metadata;
}



fn main() {
    
    let args = Args::parse();
    
     if let Err(e) = init_logging(args.loglevel) {
        //do not use logger to report logger could not start
        eprint!("Warning: Logger initialization failed with {:?}. There will be no logging.", e);
    }
    //build and start graph
    let mut graph = GraphBuilder::default()
        .build(args);

    build_graph(&mut graph);


    graph.start();

    let _ = graph.block_until_stopped(Duration::from_secs(4));
}

fn build_graph(graph: &mut Graph) {

    let base_channel_builder = graph.channel_builder()
        .with_filled_trigger(Trigger::AvgAbove(Filled::p90()), AlertColor::Red)
        .with_filled_trigger(Trigger::AvgAbove(Filled::percentage(75.0f32).expect("internal range error")), AlertColor::Orange)
        .with_filled_trigger(Trigger::AvgAbove(Filled::p50()), AlertColor::Yellow)
        .with_line_expansion(0.001f32)
        .with_type();

        let base_actor_builder = graph.actor_builder()
        .with_mcpu_trigger(Trigger::AvgAbove(MCPU::m512()), AlertColor::Yellow)
        .with_mcpu_trigger(Trigger::AvgAbove(MCPU::m768()), AlertColor::Red)
        .with_thread_info()
        .with_mcpu_avg()
        .with_load_avg();

    let (file_paths_tx, file_paths_rx) = base_channel_builder.build();
    //let (metadata_tx, metadata_rx) = base_channel_builder.build();

    // create file crawler actor
    base_actor_builder.with_name("FILE_CRAWLER")
        .build(move |actor| actors::file_crawler_2::run(actor, file_paths_tx.clone()), SoloAct);

    base_actor_builder.with_name("METADATA")
        .build(move |actor| actors::metadata::run(actor, file_paths_rx.clone()), SoloAct);
   // {
    //     let file_paths_tx_clone = file_paths_tx.clone();
    //     base_actor_builder
    //         .with_name("FileCrawler")
    //         .with_explicit_core(1)
    //         .build(move | context| {
    //             let tx = file_paths_tx_clone.clone();
    //             async move {
    //                 actors::file_crawler::run(context, tx).await
    //             }
    //         }, SoloAct);
    // }
}
