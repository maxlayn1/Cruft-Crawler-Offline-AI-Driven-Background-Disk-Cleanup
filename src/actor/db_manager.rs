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
           // println!("User confirmed deletion: {:?}", path);
            match fs::remove_file(&path) {
                Ok(_) => (),
                //Ok(_) => println!("Deleted from disk: {:?}", path),
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use crate::actor::crawler::FileMeta;

    // ── helpers ───────────────────────────────────────────────────────────────

    fn make_meta(file_name: &str, size: u64) -> FileMeta {
        FileMeta {
            rel_path: PathBuf::from(file_name),
            abs_path: PathBuf::from(format!("/tmp/{}", file_name)),
            file_name: file_name.to_string(),
            hash: "deadbeefcafe".to_string(),
            is_file: true,
            size,
            modified: 1700000000,
            created: 1600000000,
            readonly: false,
        }
    }

    fn open_temp_db(test_name: &str) -> sled::Db {
        let path = std::env::temp_dir().join(format!("cruft_test_db_{}", test_name));
        // Always start fresh
        let _ = std::fs::remove_dir_all(&path);
        sled::open(&path).expect("failed to open temp sled db")
    }

    // ── db_add ────────────────────────────────────────────────────────────────

    #[test]
    fn test_db_add_inserts_entry() {
        let db = open_temp_db("add_inserts");
        let meta = make_meta("file1.txt", 1024);
        db_add(1, meta.clone(), &db).expect("db_add should succeed");

        let key = 1i32.to_be_bytes();
        let raw = db.get(key).unwrap().expect("entry should exist");
        let restored = FileMeta::from_bytes(&raw).unwrap();
        assert_eq!(restored.file_name, "file1.txt");
        assert_eq!(restored.size, 1024);
    }

    #[test]
    fn test_db_add_multiple_entries_distinct_keys() {
        let db = open_temp_db("add_multiple");
        let meta1 = make_meta("alpha.rs", 100);
        let meta2 = make_meta("beta.rs", 200);

        db_add(1, meta1, &db).unwrap();
        db_add(2, meta2, &db).unwrap();

        let r1 = FileMeta::from_bytes(&db.get(1i32.to_be_bytes()).unwrap().unwrap()).unwrap();
        let r2 = FileMeta::from_bytes(&db.get(2i32.to_be_bytes()).unwrap().unwrap()).unwrap();

        assert_eq!(r1.file_name, "alpha.rs");
        assert_eq!(r2.file_name, "beta.rs");
    }

    #[test]
    fn test_db_add_overwrites_same_key() {
        let db = open_temp_db("add_overwrite");
        let meta1 = make_meta("old.txt", 50);
        let meta2 = make_meta("new.txt", 999);

        db_add(42, meta1, &db).unwrap();
        db_add(42, meta2, &db).unwrap();

        let raw = db.get(42i32.to_be_bytes()).unwrap().unwrap();
        let restored = FileMeta::from_bytes(&raw).unwrap();
        // sled insert overwrites — should have the second value
        assert_eq!(restored.file_name, "new.txt");
        assert_eq!(restored.size, 999);
    }

    // ── db_remove ─────────────────────────────────────────────────────────────

    #[test]
    fn test_db_remove_deletes_entry() {
        let db = open_temp_db("remove_deletes");
        let meta = make_meta("to_remove.log", 256);
        db_add(10, meta, &db).unwrap();

        db_remove(10, &db).expect("db_remove should succeed");

        let result = db.get(10i32.to_be_bytes()).unwrap();
        assert!(result.is_none(), "entry should no longer exist after remove");
    }

    #[test]
    fn test_db_remove_nonexistent_key_is_ok() {
        let db = open_temp_db("remove_nonexistent");
        // Removing a key that was never inserted should not error
        assert!(db_remove(999, &db).is_ok());
    }

    // ── db_edit ───────────────────────────────────────────────────────────────

    #[test]
    fn test_db_edit_updates_value() {
        let db = open_temp_db("edit_updates");
        let original = make_meta("original.txt", 100);
        let updated = make_meta("updated.txt", 9999);

        db_add(5, original, &db).unwrap();
        db_edit(5, updated, &db).expect("db_edit should succeed");

        let raw = db.get(5i32.to_be_bytes()).unwrap().unwrap();
        let restored = FileMeta::from_bytes(&raw).unwrap();
        assert_eq!(restored.file_name, "updated.txt");
        assert_eq!(restored.size, 9999);
    }

    #[test]
    fn test_db_edit_on_nonexistent_key_inserts() {
        let db = open_temp_db("edit_inserts_new");
        let meta = make_meta("brand_new.txt", 512);

        // db_edit on a key that doesn't exist: removes (no-op) then adds
        db_edit(77, meta, &db).expect("db_edit on new key should succeed");

        let raw = db.get(77i32.to_be_bytes()).unwrap();
        assert!(raw.is_some(), "should have been inserted by db_edit");
    }

    // ── key serialization ─────────────────────────────────────────────────────

    #[test]
    fn test_key_zero_works() {
        let db = open_temp_db("key_zero");
        let meta = make_meta("zero_key.txt", 1);
        db_add(0, meta, &db).unwrap();
        assert!(db.get(0i32.to_be_bytes()).unwrap().is_some());
    }

    #[test]
    fn test_key_negative_works() {
        let db = open_temp_db("key_negative");
        let meta = make_meta("neg_key.txt", 1);
        db_add(-1, meta, &db).unwrap();
        assert!(db.get((-1i32).to_be_bytes()).unwrap().is_some());
    }

    #[test]
    fn test_key_max_i32_works() {
        let db = open_temp_db("key_max");
        let meta = make_meta("max_key.txt", 1);
        db_add(i32::MAX, meta, &db).unwrap();
        assert!(db.get(i32::MAX.to_be_bytes()).unwrap().is_some());
    }
}