use steady_state::*;
use std::path::{Path, PathBuf};
use std::time::SystemTime;
use std::error::Error;

#[allow(unused_imports)]
use std::time::Duration;

// have here for implementing state later
// use crate::db_manager::db_state;

// derived fn that allow cloning and printing
#[derive(Default, Clone, Debug, PartialEq, Eq)]
pub(crate) struct FileMeta {
    pub path: PathBuf,
    pub file_name: String,
    pub is_file: bool,
    pub is_dir: bool,
    pub size: u64,
    pub modified: Option<SystemTime>,
    pub created: Option<SystemTime>,
    pub readonly: bool,
} 

//TODO: replace SahomeDB back with Sled
//TODO: import hashing crate and hash first chunk of files
//TODO: hard-code values for different file-types and how to treat them
//TODO: implement Walkdir to recursively get different directories
//TODO: Implement state or communication to Database to ensure its crawling in correct location on actor failure

// run function 
pub async fn run(actor: SteadyActorShadow,
                 crawler_tx: SteadyTx<FileMeta>) -> Result<(),Box<dyn Error>> {

    internal_behavior(actor.into_spotlight([], [&crawler_tx]), crawler_tx).await
}


// Internal behaviour for the actor
async fn internal_behavior<A: SteadyActor>(mut actor: A,
					   crawler_tx: SteadyTx<FileMeta> ) -> Result<(),Box<dyn Error>> {

    let mut crawler_tx = crawler_tx.lock().await;

    let dir = Path::new("crawl_test/");
    let metas = visit_dir(dir)?;

    while actor.is_running(|| crawler_tx.mark_closed()) {

	// wait before channel is vacant before sending
	// note that depending on the situation you can call the await_for_***() function for different scenarios
	for m in &metas {
	actor.wait_vacant(&mut crawler_tx, 1).await; 

	// awaiting either sleeping the thread or actor.wait_periodic() cause's a return None issue at the end
	// actor.wait_periodic(Duration::from_millis(1000)).await;

	actor.try_send(&mut crawler_tx, m.clone()).expect("couldn't send to DB");
	}

	actor.request_shutdown().await
    }
	return Ok(());
}



// function to visit test directory and return metadata of each file and insert into metadata struct
// then send to the db_manager actor (although this doesnt occur in this function)

pub fn visit_dir(dir: &Path) -> Result<Vec<FileMeta>, Box<dyn Error>> {
    let mut metas = Vec::new();

    // Read the directory (non-recursive)
    for entry_res in std::fs::read_dir(dir)? {
        let entry = entry_res?;
        let path = entry.path();
        let file_name = entry
            .file_name()
            .into_string()
            .unwrap_or_else(|os| os.to_string_lossy().into_owned());

        // Try to get metadata; if failing for a specific entry, skip it but continue
        match entry.metadata() {
            Ok(md) => {
                let is_file = md.is_file();
                let is_dir = md.is_dir();
                let size = md.len();
                let modified = md.modified().ok();
                let created = md.created().ok();
                let readonly = md.permissions().readonly();

                metas.push(FileMeta {
                    path,
                    file_name,
                    is_file,
                    is_dir,
                    size,
                    modified,
                    created,
                    readonly,
                });
            }
            Err(e) => {
                // Option A: skip entries we can't stat. Option B: return Err(e.into()) to fail hard.
                // Here we skip but log to stderr.
                eprintln!("warning: cannot stat {}: {}", file_name, e);
            }
        }
    }
    Ok(metas)
}



