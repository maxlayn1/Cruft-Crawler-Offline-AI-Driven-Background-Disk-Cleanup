#![allow(unused)]

use steady_state::*;

use crate::actor::crawler;

// run function 
pub async fn run(actor: SteadyActorShadow, crawler_to_model_rx: SteadyRx<String>, ai_model_to_ui_tx: SteadyTx<String>) -> Result<(),Box<dyn Error>> {

    let actor = actor.into_spotlight([&crawler_to_model_rx], [&ai_model_to_ui_tx]);

	if actor.use_internal_behavior {
	    internal_behavior(actor, crawler_to_model_rx, ai_model_to_ui_tx).await
	} else {
	    actor.simulated_behavior(vec!(&crawler_to_model_rx)).await
	}
}


// Internal behaviour for the actor
// @TO-DO:
// - Change data types of channels other than string
async fn internal_behavior<A: SteadyActor>(mut actor: A, crawler_to_ai_model_rx: SteadyRx<String>, ai_model_to_ui_tx: SteadyTx<String>) -> Result<(),Box<dyn Error>> {
	
    let mut crawler_to_ai_model_rx = crawler_to_ai_model_rx.lock().await;
	let mut ai_model_to_ui_tx = ai_model_to_ui_tx.lock().await;

    
    while actor.is_running(|| crawler_to_ai_model_rx.is_closed_and_empty() || ai_model_to_ui_tx.mark_closed()) {

	    //Reciecing data from crawler actor
	    actor.wait_avail(&mut crawler_to_ai_model_rx, 1).await;
        let recieved = actor.try_take(&mut crawler_to_ai_model_rx);
	    let message = recieved.expect("Expected a string");
		
		//Sending data to ui actor
		actor.wait_vacant(&mut ai_model_to_ui_tx, 1);
		let ai_to_ui_message = "Hello";
		actor.try_send(&mut ai_model_to_ui_tx , ai_to_ui_message.to_string());
    } 
	return Ok(());
}