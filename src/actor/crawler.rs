use steady_state::*;

use std::path::{Path, PathBuf};
use sha2::{Sha256, Digest};
use std::io::prelude::*;
use walkdir::WalkDir;
use std::ffi::OsStr;
use filetime::FileTime;
use std::error::Error;
// use std::env;
use serde::{Serialize, Deserialize};
use hex;

// TODO: implement state
// TODO: implement file cruft_utils.rs for get_file_hash and other non actor utilities to reside in


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

// for easy debugging of struct if needed
impl FileMeta {

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

    // deserialize from bytes
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, Box<dyn Error>> {
	Ok(serde_cbor::from_slice(bytes)?)
    }
}

// run function 
pub async fn run(actor: SteadyActorShadow,
                 crawler_tx: SteadyTx<FileMeta>) -> Result<(),Box<dyn Error>> {

    internal_behavior(actor.into_spotlight([], [&crawler_tx]), crawler_tx).await
}


// Internal behaviour for the actor
async fn internal_behavior<A: SteadyActor>(mut actor: A,
					   crawler_tx: SteadyTx<FileMeta> ) -> Result<(),Box<dyn Error>> {

    let mut crawler_tx = crawler_tx.lock().await;

    // TODO: make this code work correctly with rel_path, and figure out where to start processing directories
    // returns pathbuf, from current directory of process being run
    // let curr_env: PathBuf = env::current_dir()?;
    // let env_path: &Path = curr_env.as_path();

    let path1 = Path::new("crawl_test/");

    // convert pathbuf to path

    let metas: Vec<FileMeta> = visit_dir(path1)?;

    while actor.is_running(|| crawler_tx.mark_closed()) {

	for m in &metas {
	actor.wait_vacant(&mut crawler_tx, 1).await; 

	let message = m.clone();
	actor.try_send(&mut crawler_tx, message).expect("couldn't send to DB");
	}

	actor.request_shutdown().await
    }
	return Ok(());
}



// Read first 1024 bytes of file then hash, note that this hashes the bytes, not a string from the file
// although we could import another package and hex.encode() to convert
// note that I am only 70% confident in this function, apparently it doesnt always read 1024 bytes exactly?
// research more
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
// then send to the db_manager actor (although this doesnt occur in this function)
pub fn visit_dir(dir: &Path) -> Result<Vec<FileMeta>, Box<dyn Error>> {
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
		hash = get_file_hash(abs_path.clone()).expect("didn't get hash value");
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
                eprintln!("warning: cannot stat {}: {}", file_name, e);
            }
        }
    }
    Ok(metas)
}

