#![allow(unused)]

use steady_state::*;
use std::error::Error;
use crate::actor::crawler::FileMeta;
use std::path::PathBuf;
use std::fs;


// size of batch we want (# of FileMeta Structs before writing to DB)
const BATCH_SIZE: usize = 1;

pub async fn run(actor: SteadyActorShadow, 
                 crawler_to_db_rx: SteadyRx<FileMeta>,
                 ui_to_db_rx: SteadyRx<PathBuf> ) -> Result<(),Box<dyn Error>> {

    let actor = actor.into_spotlight([&crawler_to_db_rx, &ui_to_db_rx], []);
	internal_behavior(actor, crawler_to_db_rx, ui_to_db_rx).await
}


async fn internal_behavior<A: SteadyActor>(mut actor: A, 
                                                crawler_to_db_rx: SteadyRx<FileMeta>, 
                                                ui_to_db_rx: SteadyRx<PathBuf>) -> Result<(),Box<dyn Error>> {

    let mut crawler_to_db_rx = crawler_to_db_rx.lock().await;

    let mut ui_to_db_rx = ui_to_db_rx.lock().await;

    // TODO: example code that I need to change
    let mut db: sled::Db = sled::open("./src/db").unwrap();
    let ctr: i32 = 0;

    while actor.is_running(|| crawler_to_db_rx.is_closed_and_empty()) {
        // 1) Wait until there is at least one FileMeta from the crawler
        actor.wait_avail(&mut crawler_to_db_rx, BATCH_SIZE).await;
    
        
        // Handle any confirmed user deletions from UI
        if let Some(path) = actor.try_take(&mut ui_to_db_rx) {
            println!("User confirmed deletion: {:?}", path);
            match fs::remove_file(&path) {
                Ok(_) => println!("Deleted from disk: {:?}", path),
                Err(e) => eprintln!("Failed to delete {:?}: {}", path, e),
            }
            // TODO: remove corresponding sled entry by looking up key for this path
        }
    
        // 3) Drain up to BATCH_SIZE items from crawler_to_db_rx
        for _ in 0..BATCH_SIZE {
            match actor.try_take(&mut crawler_to_db_rx) {
                Some(file_meta) => {
                    let _ = db_add(ctr, file_meta.clone(), &db);
                    //file_meta.meta_print();
                }
                None => {
                    // nothing more to read right now
                    break;
                }
            }
        }
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
