use steady_state::*;
use sahomedb::prelude::*;
use std::error::Error;
use crate::actor::crawler::FileMeta;

pub async fn run(actor: SteadyActorShadow, 
                 crawler_rx: SteadyRx<FileMeta> ) -> Result<(),Box<dyn Error>> {

    internal_behavior(actor.into_spotlight([&crawler_rx], []), crawler_rx).await
}


#[allow(dead_code)]
fn print_type_of<T>(_: &T) {
    println!("{}", std::any::type_name::<T>())
}

async fn internal_behavior<A: SteadyActor>(mut actor: A,
                                           crawler_rx: SteadyRx<FileMeta>) -> Result<(),Box<dyn Error>> {

    let mut crawler_rx = crawler_rx.lock().await;

        
    // SahomeDB Code Here ---------------------------------------------
    let dimension = 128;

    // Replace with your own data.
    let records = Record::many_random(dimension, 100);

    let mut config = Config::default();

    // Optionally set the distance function. Default to Euclidean.
    config.distance = Distance::Cosine;

    // Create a vector collection.
    let collection = Collection::build(&config, &records).unwrap();

    // Optionally save the collection to persist it.
    let path = "data/test";
    let mut db = Database::new(path).unwrap();
    db.save_collection("vectors", &collection).unwrap();
    // SahomeDB code example end --------------------------------------


    while actor.is_running(|| crawler_rx.is_closed_and_empty()) {


	// condition to wait for sender or reciever channels to not be empty
	actor.wait_avail(&mut crawler_rx, 1).await;
	let recieved = actor.try_take(&mut crawler_rx);
	println!("{:?}", recieved.unwrap().file_name);
	}
    Ok(())
}


