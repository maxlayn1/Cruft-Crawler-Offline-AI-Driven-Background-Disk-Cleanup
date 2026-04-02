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

// TODO: implement fallback logic
// TODO: implement file cruft_utils.rs for get_file_hash and other non actor utilities to reside in

pub(crate) struct CrawlerState {
    pub(crate) abs_path: PathBuf,
    pub(crate) hash:     String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]  // ← added PartialEq, Eq
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

    pub fn to_bytes(&self) -> Result<Vec<u8>, Box<dyn Error>> {
        Ok(serde_cbor::to_vec(self)?)
    }

    pub fn from_bytes(bytes: &[u8]) -> Result<Self, Box<dyn Error>> {
        Ok(serde_cbor::from_slice(bytes)?)
    }
}

pub async fn run(
    actor: SteadyActorShadow,
    crawler_tx: SteadyTx<FileMeta>,
    crawler_to_model_tx: SteadyTx<FileMeta>,
    state: SteadyState<CrawlerState>,
) -> Result<(), Box<dyn std::error::Error>> {

    let actor = actor.into_spotlight([], [&crawler_tx, &crawler_to_model_tx]);

    if actor.use_internal_behavior {
        internal_behavior(actor, crawler_tx, crawler_to_model_tx, state).await
    } else {
        actor.simulated_behavior(vec!(&crawler_tx)).await
    }
}

async fn internal_behavior<A: SteadyActor>(
    mut actor: A,
    crawler_tx: SteadyTx<FileMeta>,
    crawler_to_ai_model_tx: SteadyTx<FileMeta>,
    state: SteadyState<CrawlerState>,
) -> Result<(), Box<dyn std::error::Error>> {

    let mut state = state.lock(|| CrawlerState {
        abs_path: PathBuf::new(),
        hash: String::new(),
    }).await;

    let mut crawler_tx = crawler_tx.lock().await;
    let mut crawler_to_ai_model_tx = crawler_to_ai_model_tx.lock().await;

    let path1 = Path::new("C:/School/testfolder");
    let metas: Vec<FileMeta> = visit_dir(path1, &state)?;

    // ← one file per iteration instead of dumping all at once
    let mut metas_iter = metas.iter();
    

    while actor.is_running(|| crawler_tx.mark_closed()) {
        // await_for_all!(
        //     actor.wait_vacant(&mut crawler_to_ai_model_tx, 1),
        //     actor.wait_vacant(&mut crawler_tx, 1)
        // );
        

        match metas_iter.next() {
            Some(m) => {
                // Skip directories — only send files to AI model
               // m.meta_print();
                if !m.is_file {
                    // Still send to DB for record keeping — wait and retry until sent
                    loop {
                        actor.wait_vacant(&mut crawler_tx, 1).await;
                        match actor.try_send(&mut crawler_tx, m.clone()) {
                            SendOutcome::Success => break,
                            SendOutcome::Blocked(_) => continue,
                            other => {
                                eprintln!("Send to DB failed: {:?}", other);
                                break;
                            }
                        }
                    }
                    continue;
                }

                // Skip target/ build directory — no point analysing build artifacts
                if m.abs_path.components().any(|c| c.as_os_str() == "target") {
                    continue;
                }

                // Send to AI model — wait for room and retry until sent (don't drop or spam log)
                loop {
                    actor.wait_vacant(&mut crawler_to_ai_model_tx, 1).await;
                    match actor.try_send(&mut crawler_to_ai_model_tx, m.clone()) {
                        SendOutcome::Success => break,
                        SendOutcome::Blocked(_) => continue,
                        other => {
                            eprintln!("Send to AI failed: {:?}", other);
                            break;
                        }
                    }
                }

                // Send to DB — wait and retry until sent
                loop {
                    actor.wait_vacant(&mut crawler_tx, 1).await;
                    match actor.try_send(&mut crawler_tx, m.clone()) {
                        SendOutcome::Success => break,
                        SendOutcome::Blocked(_) => continue,
                        other => {
                            eprintln!("Send to DB failed: {:?}", other);
                            break;
                        }
                    }
                }
            }
            None => {
                // All files sent — shut down cleanly
                //actor.request_shutdown().await;
                break;
            }
        }
    }

    Ok(())
}

pub fn get_file_hash(file_name: PathBuf) -> Result<String, Box<dyn Error>> {
    let mut file = std::fs::File::open(file_name)?;
    let mut buffer = [0u8; 1024];
    let n = file.read(&mut buffer)?;
    let mut hasher = Sha256::new();
    hasher.update(&buffer[..n]);
    let result = hasher.finalize();
    let mut out: [u8; 32] = result.into();
    out.copy_from_slice(&result);
    Ok(hex::encode(out))
}

pub fn visit_dir(
    dir: &Path,
    state: &StateGuard<'_, CrawlerState>,
) -> Result<Vec<FileMeta>, Box<dyn Error>> {

    let mut metas: Vec<FileMeta> = Vec::new();

    for entry_res in WalkDir::new(dir) {
        let entry = entry_res?;
        let rel_path: &Path = entry.path();
        let abs_path: PathBuf = std::path::absolute(&rel_path)?;
        let rel_path: PathBuf = rel_path.to_path_buf();
        let name_os: &OsStr = entry.file_name();

        let file_name: String = match name_os.to_str() {
            Some(s) => s.to_owned(),
            None => name_os.to_string_lossy().into_owned(),
        };

        match entry.metadata() {
            Ok(md) => {
                let is_file:  bool = md.is_file();
                let size:     u64  = md.len();
                let modified: i64  = FileTime::from_last_modification_time(&md).seconds();
                let created:  i64  = FileTime::from_creation_time(&md)
                                        .expect("created file time").seconds();
                let readonly: bool = md.permissions().readonly();
                let mut hash: String = String::new();

                if is_file {
                    match get_file_hash(abs_path.clone()) {
                        Ok(h) => hash = h,
                        Err(_) => continue,
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
            Err(_) => {}
        }
    }
    Ok(metas)
}
