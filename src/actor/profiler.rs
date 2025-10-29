use std::error::Error;
use steady_state::*;
use std::time::Duration;

pub async fn run(actor: SteadyActorShadow) -> Result<(),Box<dyn Error>> {

    internal_behavior(actor.into_spotlight([], [])).await
}



async fn internal_behavior<A: SteadyActor>(mut actor: A) -> Result<(),Box<dyn Error>> {

    let mut count = 100;

    while actor.is_running(|| true) { 
	let mut _count = 100;
	let rate = Duration::from_millis(1000);
	let _clean = await_for_all!(actor.wait_periodic(rate));  //#!#//
        
	count -= 1;

        if count==0   {
            actor.request_shutdown().await;
        }
    }
	return Ok(());
}
