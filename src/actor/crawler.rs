#![allow(unused)]

use steady_state::*;

use std::path::{Path, PathBuf};
use sha2::{Sha256, Digest};
use std::io::prelude::*;
use walkdir::WalkDir;
use std::ffi::OsStr;
use filetime::FileTime;
use std::error::Error;
use serde::{Serialize, Deserialize};
use hex;

// TODO: change state within visit_dir()
// TODO: implement fallback logic
// TODO: cleanup crate names
// TODO: implement file cruft_utils.rs for get_file_hash and other non actor utilities to reside in

// Internal state that helps return back to last crawled entry
pub(crate) struct CrawlerState {
    pub(crate) abs_path:  PathBuf,
    pub(crate) hash:      String,    
}

// derived fn that allow cloning and printing
#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct FileMeta {
    pub rel_path:  PathBuf,
    pub abs_path:  PathBuf,
    pub file_name: String,
    pub hash:      String,
    pub is_file:   bool,
    pub size:      u64,
    pub modified:  i64,
    pub created:   i64,
    pub readonly:  bool,
} 


impl FileMeta {
// for easy debugging of struct if needed
   pub fn meta_print(&self) {
	println!("Printing Metadata Object -----------");
	println!("Absolute_Path: {:?}", self.abs_path);
	println!("Relative_Path: {:?}", self.rel_path);
	println!("File_Name: {}",       self.file_name);
	println!("hash: {}",            self.hash);
	println!("is_file: {}",         self.is_file);
	println!("size: {}",            self.size);
	println!("modified: {}",        self.modified / 60);
	println!("created: {}",         self.created / 60);
	println!("read-only: {}",       self.readonly);
	println!("Printing Metadata Object -----------\n");
    }

    // serialize into bytes using bincode
    pub fn to_bytes(&self) -> Result<Vec<u8>, Box<dyn Error>> {
	Ok(serde_cbor::to_vec(self)?)
    }

    // deserialize from bytes using bincode
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, Box<dyn Error>> {
	Ok(serde_cbor::from_slice(bytes)?)
    }
}


// run function 
pub async fn run(actor: SteadyActorShadow, crawler_tx: SteadyTx<FileMeta>, crawler_to_model_tx : SteadyTx<String>,
                 state: SteadyState<CrawlerState>) -> Result<(),Box<dyn Error>> {

    let actor = actor.into_spotlight([], [&crawler_tx, &crawler_to_model_tx]);

	if actor.use_internal_behavior {
	    internal_behavior(actor, crawler_tx,crawler_to_model_tx, state).await
	} else {
	    actor.simulated_behavior(vec!(&crawler_tx)).await
	}
}


// Internal behaviour for the actor
async fn internal_behavior<A: SteadyActor>(mut actor: A, crawler_tx: SteadyTx<FileMeta>, crawler_to_ai_model_tx : SteadyTx<String>,
                                           state: SteadyState<CrawlerState>) -> Result<(),Box<dyn Error>> {

    // lock state
    let mut state = state.lock(|| CrawlerState{abs_path: PathBuf::new(),
					       hash: String::new()}).await;

    let mut crawler_tx = crawler_tx.lock().await;
    let mut crawler_to_ai_model_tx = crawler_to_ai_model_tx.lock().await;

    let path1 = Path::new("./src/");

    // TODO: change value of state inside this function before pushing metadata
    // NOTE: state passed in as mutable reference  &StateGuard<'_, CrawlerState>
    let metas: Vec<FileMeta> = visit_dir(path1, &state)?;
    
    while actor.is_running(|| crawler_tx.mark_closed()) {
        await_for_all!(actor.wait_vacant(&mut crawler_to_ai_model_tx, 1), actor.wait_vacant(&mut crawler_tx, 1));

        //Sending data to ai actor
        //actor.wait_vacant(&mut crawler_to_ai_model_tx, 1).await;
        let message2 = "Hello".to_string();
    
        match actor.try_send(&mut crawler_to_ai_model_tx, message2){
            SendOutcome::Success=>{
            }
            SendOutcome::Blocked(_)=>{
                eprintln!("CrawlerChannel is blocked");
            }
            
        }
        
        //sending data to database actor
        for m in &metas {
            //actor.wait_vacant(&mut crawler_tx, 1).await; 
            let message = m.clone();

            match actor.try_send(&mut crawler_tx, message){
                SendOutcome::Success =>{
                    eprintln!("Crawler sent to DB");
                }
                SendOutcome::Blocked(_) => {
                    eprintln!("Channel is blocked");
                }
            }
            //actor.try_send(&mut crawler_tx, message).expect("couldn't send to DB");
	    }
        
        //actor.request_shutdown().await; //comment out this line to make the program have an infinite loop.

    } 
	return Ok(());
}


// Read first 1024 bytes of file then hash, note that this hashes the bytes, not a string from the file
// TODO: double check that hashing bytes is correct
pub fn get_file_hash(file_name: PathBuf) -> Result<String, Box<dyn Error>> {

    let mut file = std::fs::File::open(file_name)?;
    
    // buffer of 1024 bytes to read file
    let mut buffer = [0u8; 1024];

    let n = file.read(&mut buffer)?;

    let mut hasher = Sha256::new();
    hasher.update(&buffer[..n]);
    let result = hasher.finalize();

    let mut out: [u8; 32] = result.into();
    out.copy_from_slice(&result);

    //encodes value as string
    let convert = hex::encode(out);
    
    Ok(convert)
}


// function to visit test directory and return metadata of each file and insert into metadata struct
// also updates state per every entry
pub fn visit_dir(dir: &Path,
                 state: &StateGuard<'_, CrawlerState> ) -> Result<Vec<FileMeta>, Box<dyn Error>> {

    let mut metas: Vec<FileMeta> = Vec::new();

    // Read the directory (non-recursive)
    for entry_res in WalkDir::new(dir) {
        let entry = entry_res?;
        let rel_path: &Path = entry.path();
	let abs_path: PathBuf = std::path::absolute(&rel_path)?;

	// convert relative path to Pathbuf for printing
	let rel_path: PathBuf = rel_path.to_path_buf();

	let name_os: &OsStr = entry.file_name();

	let file_name: String = match name_os.to_str() {
            Some(s) => s.to_owned(),
            None => name_os.to_string_lossy().into_owned(),
        };

	
        // Try to get metadata; if failing for a specific entry, skip it but continue
        match entry.metadata() {
            Ok(md) => {
                let is_file:  bool   = md.is_file();
                let size:     u64    = md.len();
                let modified: i64    = FileTime::from_last_modification_time(&md).seconds();
                let created:  i64    = FileTime::from_creation_time(&md).expect("created file time").seconds();
                let readonly: bool   = md.permissions().readonly();
		let mut hash: String = String::new();

		if is_file {
            match get_file_hash(abs_path.clone()) {
                Ok(h) => {
                    hash = h;
                }
                Err(e) => {
                    eprintln!("Skipping locked/unreadable file {:?}: {}", abs_path, e);
                    continue; // skip this entry and move on
                }
            }
        }

        metas.push(FileMeta {
		        rel_path,
		        abs_path,
                file_name,
		        hash, 
                is_file,
                size,
                modified, 
                created,
                readonly,
            });
        }
        Err(e) => {
		// TODO: log errors here
                eprintln!("warning: cannot stat {}: {}", file_name, e);
            }
        }
    }
    Ok(metas)
}
