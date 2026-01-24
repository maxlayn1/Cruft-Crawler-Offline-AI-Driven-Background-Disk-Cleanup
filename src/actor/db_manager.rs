#![allow(unused)]

use steady_state::*;
use std::error::Error;
use crate::actor::crawler::FileMeta;


// size of batch we want (# of FileMeta Structs before writing to DB)
const BATCH_SIZE: usize = 1;

pub async fn run(actor: SteadyActorShadow, 
                 crawler_to_db_rx: SteadyRx<FileMeta>,
                 file_handler_to_db_rx: SteadyRx<String> ) -> Result<(),Box<dyn Error>> {

    let actor = actor.into_spotlight([&crawler_to_db_rx, &file_handler_to_db_rx], []);
	internal_behavior(actor, crawler_to_db_rx, file_handler_to_db_rx).await
}


async fn internal_behavior<A: SteadyActor>(mut actor: A, 
                                                crawler_to_db_rx: SteadyRx<FileMeta>, 
                                                file_handler_to_db_rx: SteadyRx<String>) -> Result<(),Box<dyn Error>> {

    let mut crawler_to_db_rx = crawler_to_db_rx.lock().await;

    let mut file_handler_to_db_rx = file_handler_to_db_rx.lock().await;


    // TODO: example code that I need to change
    let mut db: sled::Db = sled::open("./src/db").unwrap();
    let ctr: i32 = 0;

    while actor.is_running(|| crawler_to_db_rx.is_closed_and_empty()) {

        //Recieving data from ui actor
	    actor.wait_avail(&mut file_handler_to_db_rx, 1).await;
        let recieved = actor.try_take(&mut file_handler_to_db_rx);
	    let message = recieved.expect("Expected a string");

        //Recieving from crawler actor
	    actor.wait_avail(&mut crawler_to_db_rx, BATCH_SIZE).await;
	    let recieved = actor.try_take(&mut crawler_to_db_rx);

	    let msg = recieved.expect("expected FileMeta Struct (db_actor)");
	    let _ = db_add(ctr, msg.clone(), &db);
	    msg.meta_print();
	}
    
  Ok(())
}


// add db entry given key and value pair
// TODO: add match to check if db operations are successful or not
fn db_add(key: i32, value: FileMeta, db: &sled::Db) -> Result<(), Box<dyn Error>> {

    // serialise struct into u8
    let value_s = value.to_bytes()?;

    // serialize i32 to bytes
    let key_s = key.to_be_bytes();

    // insert into db
    let _ = db.insert(key_s, value_s)?;

Ok(())
}


// edit db entry given key
// TODO: add match to check if db operations are successful or not
fn db_edit(key: i32, value: FileMeta, db: &sled::Db) -> Result<(), Box<dyn Error>> {

    // sled has immutable db, so we need to delete old key then insert new
    let _ = db_remove(key, &db);
    let _ = db_add(key, value, &db);

Ok(())
}


// remove db entry given key
// TODO: add match to check if db operations are successful or not
fn db_remove(key: i32, db: &sled::Db) -> Result<(), Box<dyn Error>> {

    let key_s = key.to_be_bytes();
    
    // remove entry based on key
    let _ = db.remove(key_s);

    Ok(())
}
