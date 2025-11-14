use steady_state::*;
// use sahomedb::prelude::*;
use std::error::Error;
use crate::actor::crawler::FileMeta;

//TODO: Remove SahomeDB, use Sled instead
//TODO: push all the metadata into the Sled database 
//TODO: research a way to view the sled database for presentation

pub async fn run(actor: SteadyActorShadow, 
                 crawler_rx: SteadyRx<FileMeta> ) -> Result<(),Box<dyn Error>> {

    internal_behavior(actor.into_spotlight([&crawler_rx], []), crawler_rx).await
}


// for debugging
#[allow(dead_code)]
fn print_type_of<T>(_: &T) {
    println!("{}", std::any::type_name::<T>())
}


async fn internal_behavior<A: SteadyActor>(mut actor: A,
                                           crawler_rx: SteadyRx<FileMeta>) -> Result<(),Box<dyn Error>> {

    let mut crawler_rx = crawler_rx.lock().await;

    // Database code here ------------------------------------
    // Database code here ------------------------------------


    while actor.is_running(|| crawler_rx.is_closed_and_empty()) {

	// condition to wait for sender or reciever channels to not be empty
	actor.wait_avail(&mut crawler_rx, 1).await;
	let recieved = actor.try_take(&mut crawler_rx);




	// print using the function given in crawler.rs for pretty-printing
	recieved.unwrap().printMeta();

	}
    Ok(())
}


