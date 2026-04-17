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

const WINDOWS_TO_UNIX_EPOCH_OFFSET: i64 = 11_644_473_600;
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

fn load_scan_dir() -> PathBuf {
    let config_file = PathBuf::from("scan_path.txt");

    if !config_file.exists() {
        eprintln!("\n  ✗ CruftCrawler could not start.");
        eprintln!("  No 'scan_path.txt' file was found in the current directory.");
        eprintln!("  Please create a file named 'scan_path.txt' next to the executable");
        eprintln!("  and put the directory you want to scan on the first line.");
        eprintln!();
        eprintln!("  Example (Windows):  C:\\Users\\YourName\\Documents");
        eprintln!("  Example (Linux):    /home/yourname/documents");
        eprintln!();
        std::process::exit(1);
    }

    let contents = std::fs::read_to_string(&config_file).unwrap_or_else(|e| {
        eprintln!("\n  ✗ CruftCrawler could not start.");
        eprintln!("  Found 'scan_path.txt' but could not read it: {}", e);
        eprintln!();
        std::process::exit(1);
    });

    let trimmed = contents.trim().to_string();

    if trimmed.is_empty() {
        eprintln!("\n  ✗ CruftCrawler could not start.");
        eprintln!("  'scan_path.txt' exists but is empty.");
        eprintln!("  Please put the directory you want to scan on the first line.");
        eprintln!();
        std::process::exit(1);
    }

    let path = PathBuf::from(&trimmed);

    if !path.exists() {
        eprintln!("\n  ✗ CruftCrawler could not start.");
        eprintln!("  The path in 'scan_path.txt' does not exist:");
        eprintln!("    {}", trimmed);
        eprintln!("  Please check the path is spelled correctly.");
        eprintln!();
        std::process::exit(1);
    }

    if !path.is_dir() {
        eprintln!("\n  ✗ CruftCrawler could not start.");
        eprintln!("  The path in 'scan_path.txt' is not a directory:");
        eprintln!("    {}", trimmed);
        eprintln!("  Please provide a folder path, not a file path.");
        eprintln!();
        std::process::exit(1);
    }

    path
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

    

    let path = load_scan_dir();
    let metas: Vec<FileMeta> = visit_dir(&path, &state)?;

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
        actor.wait_periodic(std::time::Duration::from_secs(10)).await;
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
                //windows and unix timestamps are different so you must convert the timestamps to seconds differently
                let modified: i64 = {
                    let raw = FileTime::from_last_modification_time(&md).seconds();
                    #[cfg(target_os = "windows")]
                    let raw = raw - WINDOWS_TO_UNIX_EPOCH_OFFSET;
                    raw
                };
                //windows and unix timestamps are different so you must convert the timestamps to seconds differently
                let created: i64 = FileTime::from_creation_time(&md)
                    .map(|ft| {
                        let raw = ft.seconds();
                        #[cfg(target_os = "windows")]
                        let raw = raw - 11_644_473_600;
                        raw
                    })
                    .unwrap_or(modified);
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::{self, File};
    use std::io::Write;
    use std::path::PathBuf;

    // ── helpers ───────────────────────────────────────────────────────────────

    fn make_meta(file_name: &str, size: u64, modified: i64, readonly: bool) -> FileMeta {
        FileMeta {
            rel_path: PathBuf::from(file_name),
            abs_path: PathBuf::from(format!("/tmp/{}", file_name)),
            file_name: file_name.to_string(),
            hash: String::new(),
            is_file: true,
            size,
            modified,
            created: 0,
            readonly,
        }
    }

    // ── FileMeta::to_bytes / from_bytes roundtrip ────────────────────────────

    #[test]
    fn test_serialize_deserialize_roundtrip() {
        let meta = make_meta("test.txt", 1024, 1700000000, false);
        let bytes = meta.to_bytes().expect("serialization failed");
        let restored = FileMeta::from_bytes(&bytes).expect("deserialization failed");
        assert_eq!(meta, restored);
    }

    #[test]
    fn test_serialize_preserves_all_fields() {
        let meta = FileMeta {
            rel_path: PathBuf::from("some/relative/path.rs"),
            abs_path: PathBuf::from("/absolute/path.rs"),
            file_name: "path.rs".to_string(),
            hash: "abc123deadbeef".to_string(),
            is_file: true,
            size: 99999,
            modified: 1700000000,
            created: 1600000000,
            readonly: true,
        };
        let bytes = meta.to_bytes().unwrap();
        let restored = FileMeta::from_bytes(&bytes).unwrap();
        assert_eq!(restored.file_name, "path.rs");
        assert_eq!(restored.hash, "abc123deadbeef");
        assert_eq!(restored.size, 99999);
        assert_eq!(restored.readonly, true);
        assert_eq!(restored.is_file, true);
        assert_eq!(restored.created, 1600000000);
    }

    #[test]
    fn test_deserialize_invalid_bytes_returns_error() {
        let bad_bytes = vec![0xDE, 0xAD, 0xBE, 0xEF, 0x00, 0x01];
        assert!(FileMeta::from_bytes(&bad_bytes).is_err());
    }

    #[test]
    fn test_serialize_empty_hash_and_paths() {
        let meta = FileMeta {
            rel_path: PathBuf::new(),
            abs_path: PathBuf::new(),
            file_name: String::new(),
            hash: String::new(),
            is_file: false,
            size: 0,
            modified: 0,
            created: 0,
            readonly: false,
        };
        let bytes = meta.to_bytes().unwrap();
        let restored = FileMeta::from_bytes(&bytes).unwrap();
        assert_eq!(meta, restored);
    }

    // ── FileMeta equality / clone ─────────────────────────────────────────────

    #[test]
    fn test_clone_is_equal() {
        let meta = make_meta("clone_test.log", 512, 1700000000, false);
        let cloned = meta.clone();
        assert_eq!(meta, cloned);
    }

    #[test]
    fn test_different_metas_are_not_equal() {
        let a = make_meta("a.txt", 100, 1000, false);
        let b = make_meta("b.txt", 200, 2000, true);
        assert_ne!(a, b);
    }

    // ── get_file_hash ─────────────────────────────────────────────────────────

    #[test]
    fn test_get_file_hash_returns_hex_string() {
        // Create a real temp file to hash
        let path = std::env::temp_dir().join("cruft_hash_test.txt");
        let mut f = File::create(&path).unwrap();
        f.write_all(b"hello cruftcrawler").unwrap();

        let hash = get_file_hash(path.clone()).expect("hash should succeed");
        // SHA-256 hex output is always 64 characters
        assert_eq!(hash.len(), 64);
        // Should only contain hex characters
        assert!(hash.chars().all(|c| c.is_ascii_hexdigit()));

        fs::remove_file(path).ok();
    }

    #[test]
    fn test_get_file_hash_same_content_same_hash() {
        let path1 = std::env::temp_dir().join("cruft_hash_a.txt");
        let path2 = std::env::temp_dir().join("cruft_hash_b.txt");

        for p in [&path1, &path2] {
            let mut f = File::create(p).unwrap();
            f.write_all(b"identical content").unwrap();
        }

        let hash1 = get_file_hash(path1.clone()).unwrap();
        let hash2 = get_file_hash(path2.clone()).unwrap();
        assert_eq!(hash1, hash2);

        fs::remove_file(path1).ok();
        fs::remove_file(path2).ok();
    }

    #[test]
    fn test_get_file_hash_different_content_different_hash() {
        let path1 = std::env::temp_dir().join("cruft_diff_a.txt");
        let path2 = std::env::temp_dir().join("cruft_diff_b.txt");

        let mut f1 = File::create(&path1).unwrap();
        f1.write_all(b"content A").unwrap();

        let mut f2 = File::create(&path2).unwrap();
        f2.write_all(b"content B").unwrap();

        let hash1 = get_file_hash(path1.clone()).unwrap();
        let hash2 = get_file_hash(path2.clone()).unwrap();
        assert_ne!(hash1, hash2);

        fs::remove_file(path1).ok();
        fs::remove_file(path2).ok();
    }

    #[test]
    fn test_get_file_hash_nonexistent_file_returns_error() {
        let result = get_file_hash(PathBuf::from("/tmp/definitely_does_not_exist_xyz.txt"));
        assert!(result.is_err());
    }

    #[test]
    fn test_get_file_hash_empty_file() {
        let path = std::env::temp_dir().join("cruft_empty.txt");
        File::create(&path).unwrap(); // creates empty file

        let hash = get_file_hash(path.clone()).expect("empty file hash should succeed");
        assert_eq!(hash.len(), 64);

        fs::remove_file(path).ok();
    }
}