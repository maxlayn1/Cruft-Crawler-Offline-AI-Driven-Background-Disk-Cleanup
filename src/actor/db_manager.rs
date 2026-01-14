use steady_state::*;
use std::error::Error;
use crate::actor::crawler::FileMeta;


const BATCH_SIZE: usize = 1;

pub async fn run(actor: SteadyActorShadow, 
                 crawler_rx: SteadyRx<FileMeta> ) -> Result<(),Box<dyn Error>> {

    internal_behavior(actor.into_spotlight([&crawler_rx], []), crawler_rx).await
}


async fn internal_behavior<A: SteadyActor>(mut actor: A,
                                           crawler_rx: SteadyRx<FileMeta>) -> Result<(),Box<dyn Error>> {

    let mut crawler_rx = crawler_rx.lock().await;


    // TODO: example code that I need to change
    let db: sled::Db = sled::open("../db").unwrap();
    let ctr: i32 = 0;

    while actor.is_running(|| crawler_rx.is_closed_and_empty()) {

	actor.wait_avail(&mut crawler_rx, BATCH_SIZE).await;
	let recieved = actor.try_take(&mut crawler_rx);

	let msg = recieved.expect("expected FileMeta Struct (db_actor)");
	let _ = db_add(ctr, msg.clone(), db.clone());
	msg.meta_print();
	}

  Ok(())
}


// add db entry given key and value pair
fn db_add(key: i32, value: FileMeta, db: sled::Db) -> Result<(), Box<dyn Error>> {

    // serialise struct into u8
    let value_s = value.to_bytes()?;

    // serialize i32 to bytes
    let key_s = key.to_be_bytes();

    // insert into db
    let _ = db.insert(key_s, value_s)?;

Ok(())
}


// edit db entry given key
fn db_edit(key: i32, value: FileMeta, db: sled::Db) -> Result<(), Box<dyn Error>> {

    // sled has immutable db, so we need to delete old key then insert new
    let _ = db_remove(key, db.clone());
    let _ = db_add(key, value, db.clone());

Ok(())
}


// remove db entry given key
fn db_remove(key: i32, db: sled::Db) -> Result<(), Box<dyn Error>> {

    let key_s = key.to_be_bytes();
    
    // remove entry based on key
    let _ = db.remove(key_s);

    Ok(())
}
