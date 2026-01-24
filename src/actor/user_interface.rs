#![allow(unused)]

use steady_state::*;

use crate::actor::crawler;

// run function 
pub async fn run(actor: SteadyActorShadow, ai_model_to_ui_rx: SteadyRx<String>, ui_to_file_handler_tx: SteadyTx<String>) -> Result<(),Box<dyn Error>> {

    let actor = actor.into_spotlight([&ai_model_to_ui_rx], [&ui_to_file_handler_tx]);

	if actor.use_internal_behavior {
	    internal_behavior(actor,  ai_model_to_ui_rx, ui_to_file_handler_tx).await
	} else {
	    actor.simulated_behavior(vec!(&ai_model_to_ui_rx)).await
	}
}


// Internal behaviour for the actor
// @TO-DO:
// - Change data types of channels other than string
async fn internal_behavior<A: SteadyActor>(mut actor: A, ai_model_to_ui_rx: SteadyRx<String>, ui_to_file_handler_tx: SteadyTx<String>) -> Result<(),Box<dyn Error>> {

    
    let mut ai_model_to_ui_rx = ai_model_to_ui_rx.lock().await;
	let mut ui_to_file_handler_tx = ui_to_file_handler_tx.lock().await;

    while actor.is_running(|| ai_model_to_ui_rx.is_closed_and_empty()) {

	    // ADD UI LOGIC HERE

		// Recieving data from ai actor
	    actor.wait_avail(&mut ai_model_to_ui_rx, 1).await;
        let recieved = actor.try_take(&mut ai_model_to_ui_rx);
	    let message = recieved.expect("Expected a string");

		// Sending data to file handler
		actor.wait_vacant(&mut ui_to_file_handler_tx, 1);
		let ui_to_file_handler = "Hello";
		actor.try_send(&mut ui_to_file_handler_tx , ui_to_file_handler.to_string());

    } 
   
	return Ok(());
}