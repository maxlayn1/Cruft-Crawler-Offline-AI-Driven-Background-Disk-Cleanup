#![allow(unused)]

use steady_state::*;

use crate::actor::crawler;

// run function 
pub async fn run(actor: SteadyActorShadow, crawler_to_model_rx: SteadyRx<String>) -> Result<(),Box<dyn Error>> {

    let actor = actor.into_spotlight([&crawler_to_model_rx], []);

	if actor.use_internal_behavior {
	    internal_behavior(actor, crawler_to_model_rx).await
	} else {
	    actor.simulated_behavior(vec!(&crawler_to_model_rx)).await
	}
}


// Internal behaviour for the actor
async fn internal_behavior<A: SteadyActor>(mut actor: A, crawler_to_model_rx: SteadyRx<String>) -> Result<(),Box<dyn Error>> {

    

    let mut crawler_to_model_rx = crawler_to_model_rx.lock().await;

    
    while actor.is_running(|| crawler_to_model_rx.is_closed_and_empty()) {

	    
	    actor.wait_avail(&mut crawler_to_model_rx, 1).await; 
        let recieved = actor.try_take(&mut crawler_to_model_rx);
	    let message = recieved.expect("Expected a string");
    } 
    actor.request_shutdown().await;
	return Ok(());
}