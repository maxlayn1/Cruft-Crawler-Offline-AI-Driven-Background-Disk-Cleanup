#![allow(unused)]

use steady_state::*;

use std::path::{Path, PathBuf};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::sync::atomic::{AtomicBool, Ordering};
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
pub async fn run(
    actor: SteadyActorShadow,
    crawler_tx: SteadyTx<FileMeta>,
    crawler_to_model_tx: SteadyTx<String>,
    state: SteadyState<CrawlerState>,
    stop_flag: Arc<AtomicBool>,
    resume_from: Arc<Mutex<String>>,
) -> Result<(), Box<dyn std::error::Error>> {

    let actor = actor.into_spotlight([], [&crawler_tx, &crawler_to_model_tx]);

	if actor.use_internal_behavior {
	    internal_behavior(actor, crawler_tx, crawler_to_model_tx, state, stop_flag, resume_from).await
	} else {
	    actor.simulated_behavior(vec!(&crawler_tx)).await
	}
}


/// Change internal_behavior signature to match:
async fn internal_behavior<A: SteadyActor>(
    mut actor: A,
    crawler_tx: SteadyTx<FileMeta>,
    crawler_to_ai_model_tx: SteadyTx<String>,
    state: SteadyState<CrawlerState>,
    stop_flag: Arc<AtomicBool>,
    resume_from: Arc<Mutex<String>>,
) -> Result<(), Box<dyn std::error::Error>> {

    // lock state
    let mut state = state.lock(|| CrawlerState{abs_path: PathBuf::new(),
                                               hash: String::new()}).await;

    let mut crawler_tx = crawler_tx.lock().await;
    let mut crawler_to_ai_model_tx = crawler_to_ai_model_tx.lock().await;

    let config_str = std::fs::read_to_string("./config.toml").unwrap_or_default();
    let config: toml::Value = toml::from_str(&config_str).unwrap_or(toml::Value::Table(Default::default()));
    let crawl_path_str = config
        .get("directory")
        .and_then(|d| d.get("path"))
        .and_then(|p| p.as_str())
        .unwrap_or(".")
        .to_string();
    let crawl_path_str = if crawl_path_str.trim().is_empty() { ".".to_string() } else { crawl_path_str };
    let crawl_path = PathBuf::from(&crawl_path_str);

    // Directories to skip entirely (build caches, VCS, package managers, etc.)
    const SKIP_DIRS: &[&str] = &[
        ".git", ".svn", ".hg",
        "node_modules", "target",
        "Library", "Temp", "Artifacts", "PackageCache", "ShaderCache",
        ".vs", ".idea", "__pycache__",
        "obj", "bin",
        ".cache", ".next", "dist", "build",
    ];

    // Resume: get the last file path we processed in a previous (stopped) run
    let resume_path = resume_from.lock().unwrap().clone();
    let mut skipping = !resume_path.is_empty();

    let mut seen_hashes: std::collections::HashMap<String, String> = std::collections::HashMap::new();
    while actor.is_running(|| crawler_tx.mark_closed()) {
        'scan: for entry_res in WalkDir::new(&crawl_path)
            .into_iter()
            .filter_entry(|e| {
                if e.file_type().is_dir() {
                    let name = e.file_name().to_str().unwrap_or("");
                    return !SKIP_DIRS.contains(&name);
                }
                true
            })
        {
            // Check stop flag — break out cleanly so channels close properly
            if stop_flag.load(Ordering::Relaxed) {
                break 'scan;
            }

            let entry = match entry_res {
                Ok(e) => e,
                Err(_) => continue,
            };

            let rel_path = entry.path().to_path_buf();
            let abs_path: PathBuf = match std::path::absolute(&rel_path) {
                Ok(p) => p,
                Err(_) => continue,
            };

            let file_name: String = match entry.file_name().to_str() {
                Some(s) => s.to_owned(),
                None => entry.file_name().to_string_lossy().into_owned(),
            };

            let md = match entry.metadata() {
                Ok(m) => m,
                Err(_) => continue,
            };

            let is_file = md.is_file();

            // Resume: skip files until we reach (and pass) the last processed path
            if skipping && is_file {
                let path_str = abs_path.to_string_lossy().to_string();
                if path_str == resume_path {
                    skipping = false; // found our position; next file will be processed
                }
                continue;
            }

            let size = md.len();
            let modified = FileTime::from_last_modification_time(&md).seconds();
            let created = match FileTime::from_creation_time(&md) {
                Some(t) => t.seconds(),
                None => 0,
            };
            let readonly = md.permissions().readonly();

            let hash = if is_file {
                match get_file_hash(abs_path.clone()) {
                    Ok(h) => h,
                    Err(_) => continue,
                }
            } else {
                String::new()
            };

            let file_meta = FileMeta {
                rel_path,
                abs_path: abs_path.clone(),
                file_name,
                hash: hash.clone(),
                is_file,
                size,
                modified,
                created,
                readonly,
            };

            // Send to DB
            actor.wait_vacant(&mut crawler_tx, 1).await;
            match actor.try_send(&mut crawler_tx, file_meta) {
                SendOutcome::Success => {}
                SendOutcome::Blocked(_) => {}
                _ => {}
            }

            // Send path to AI model — only for duplicates
            if is_file && size > 0 && !hash.is_empty() {
                let dup_path = abs_path.to_string_lossy().to_string();

                // Track resume position after successfully processing this file
                *resume_from.lock().unwrap() = dup_path.clone();

                if let Some(orig_path) = seen_hashes.get(&hash) {
                    let msg = format!("{}|{}", dup_path, orig_path);
                    actor.wait_vacant(&mut crawler_to_ai_model_tx, 1).await;
                    match actor.try_send(&mut crawler_to_ai_model_tx, msg) {
                        SendOutcome::Success => {}
                        SendOutcome::Blocked(_) => {}
                        _ => {}
                    }
                } else {
                    seen_hashes.insert(hash.clone(), dup_path);
                }
            }

            // Throttle CPU usage to stay well below 15%
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        // Crawl complete - stop
        break;
    }

    Ok(())
}


// Read first 1024 bytes of file then hash, note that this hashes the bytes, not a string from the file
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
