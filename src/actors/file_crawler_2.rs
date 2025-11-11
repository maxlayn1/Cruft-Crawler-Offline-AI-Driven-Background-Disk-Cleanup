use std::error::Error;
use steady_state::*;
use walkdir::WalkDir;
// use log::*;
//use std::fs::Metadata;
use std::time::SystemTime;
use std::fs::FileType;
// use tokio::fs::OpenOptions;
// use std::io::Write;
// use tokio::io::AsyncWriteExt; // For write_all() and flush()
// //use tokio::time::{sleep, Duration};
// use tokio::runtime::Runtime;

pub struct MetadataFields{
    pub file_name: String,
    pub full_path: String,
    pub is_file: bool,
    pub is_dir: bool,
    pub time_created: SystemTime,
    pub last_time_modified: SystemTime,
    pub last_accessed: SystemTime,
    pub file_size: u64,
    pub file_type: FileType,
}

pub async fn run(actor: SteadyActorShadow
                 , file_path_tx: SteadyTx<MetadataFields>) -> Result<(), Box<dyn Error>> {

    let actor = actor.into_spotlight( [], [&file_path_tx]);

    internal_behavior(actor, file_path_tx).await
}


async fn internal_behavior<A: SteadyActor>(mut actor: A
                                           , file_path_tx: SteadyTx<MetadataFields>) -> Result<(),Box<dyn Error>> { 
    let mut file_path_tx = file_path_tx.lock().await; 
    let root_path = "D:/CS 425/Steady_State_Prototype/testDirectory";

    let mut entries = WalkDir::new(root_path)
        .follow_links(false)
        .max_depth(usize::MAX)
        .into_iter()
        .filter_map(|e| e.ok());

    while actor.is_running(|| i!(file_path_tx.mark_closed() && i!(file_path_tx.is_empty()))) {

        //wait for 2 seconds before transmission?
        let clean = await_for_all!(actor.wait_periodic(Duration::from_secs(2)), actor.wait_vacant(&mut file_path_tx, 1));
      
        
        if clean {
            if let Some(entry) = entries.next() {
                match entry.metadata() {
                    Ok(metadata) => {
                        
                        // Create a MetadataFields struct to be transmitted. This is a custom struct seperate from the std::fs::Metadata struct.
                        // Eventually used for feeding information into LLM prompt.
                        let custom_metadata = MetadataFields {
                            file_name: entry.file_name().to_string_lossy().to_string(),
                            full_path: entry.path().to_string_lossy().to_string(),
                            is_file: metadata.is_file(),
                            is_dir: metadata.is_dir(),
                            time_created: metadata.created()?,
                            last_accessed: metadata.accessed()?,
                            last_time_modified: metadata.modified()?,
                            file_size: metadata.len(),
                            file_type: metadata.file_type(),
                        };

                        actor.try_send(&mut file_path_tx, custom_metadata);
                    }
                    Err(e) => eprintln!("Failed to get metadata: {}", e),
                }
            } else {
                // No more entries - stop the actor
                break;
            }
        }
    }
    actor.request_shutdown().await;
    Ok(())
}