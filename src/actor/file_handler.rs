#![allow(unused)]

use steady_state::*;

use crate::actor::crawler;

// run function
pub async fn run(
    actor: SteadyActorShadow,
    ui_to_file_handler_rx: SteadyRx<String>,
    file_handler_to_db_tx: SteadyTx<String>,
) -> Result<(), Box<dyn Error>> {

    let actor = actor.into_spotlight([&ui_to_file_handler_rx], [&file_handler_to_db_tx]);

    if actor.use_internal_behavior {
        internal_behavior(actor, ui_to_file_handler_rx, file_handler_to_db_tx).await
    } else {
        actor.simulated_behavior(vec!(&ui_to_file_handler_rx, &file_handler_to_db_tx)).await
    }
}

// Internal behaviour for the actor
// @TO-DO:
// - Change data types of channels other than string
async fn internal_behavior<A: SteadyActor>(
    mut actor: A,
    ui_to_file_handler_rx: SteadyRx<String>,
    file_handler_to_db_tx: SteadyTx<String>,
) -> Result<(), Box<dyn Error>> {

    let mut ui_to_file_handler_rx = ui_to_file_handler_rx.lock().await;
    let mut file_handler_to_db_tx = file_handler_to_db_tx.lock().await;

    while actor.is_running(|| ui_to_file_handler_rx.is_closed_and_empty()) {
        actor.wait_avail(&mut ui_to_file_handler_rx, 1).await;

        // Use if let instead of .expect() so a None doesn't panic
        if let Some(file_name) = actor.try_take(&mut ui_to_file_handler_rx) {
            println!("FILE_HANDLER: received '{}' — would delete", file_name);

            // Notify DB that this file was handled
            actor.wait_vacant(&mut file_handler_to_db_tx, 1).await;
            match actor.try_send(&mut file_handler_to_db_tx, file_name.clone()) {
                SendOutcome::Success    => {}
                SendOutcome::Blocked(_) => { eprintln!("FILE_HANDLER: DB channel blocked"); }
                SendOutcome::Timeout(_) => { eprintln!("FILE_HANDLER: DB channel timeout"); }
                SendOutcome::Closed(_)  => { break; }
            }
        }
    }

    Ok(())
}