use steady_state::*;
use std::fs::OpenOptions;
use std::io::Write;
use crate::actors::file_crawler_2::MetadataFields;
use chrono::{DateTime, Local};
use std::time::SystemTime;
use std::fs::FileType;

// use tokio::fs::OpenOptions;
// use tokio::io::AsyncWriteExt;

pub async fn run(actor: SteadyActorShadow, metadata_rx: SteadyRx<MetadataFields>)-> Result<(), Box<dyn Error>>{
    let actor = actor.into_spotlight([&metadata_rx], []);
    
    internal_behavior(actor, metadata_rx).await
}

/*
* This funciton, system_time_to_stirng, converts system time to a string. This is used in write_metadata_line() to convert system time variables in the MetadataFields struct to strings to be written to files.
* In the main project this will be for inserting the metadata fields to the LLM prompt.
*/
fn system_time_to_string(time: SystemTime) -> String {
    let datetime: DateTime<Local> = time.into();
    datetime.format("%Y-%m-%d %H:%M:%S").to_string()
}

/*
* This function, file_type_to_string, converts a file type to a static string, which can be used to create a larger string.
* This function is used in write_metadata_line() so the FileType variable in MetadataFields struct can be put into a string.
 */
fn file_type_to_string(file_type: FileType) -> &'static str{
    if file_type.is_file(){
        return "File"
    } else if file_type.is_dir(){
        return "Directory"
    } else if file_type.is_symlink(){
        return "Symlink"
    } else{
        return "Unknown"
    }
}

/* 
* This function writes all metadata from the fs::std::Metadata struct.
*/
fn write_metadata_line(file_path: &str, metadata: &MetadataFields) -> std::io::Result<()> {
    
    // create a string that has all the fields from the MetadataFields struct. This string is formatted for readability.///////////////
    let metadata_block = format!("File Name: {file_name}\nFile Path:{file_path}\nIs File: {is_file}\nIs dir: {is_dir}\nTime Created: {time_created}\nModified: {last_modified}\nAccessed: {last_accessed}\nFile Size: {file_size}\nFile Type: {file_type}\n\n",
    file_name = metadata.file_name,
    file_path = metadata.full_path,
    is_file = metadata.is_file,
    is_dir = metadata.is_dir,
    time_created = system_time_to_string(metadata.time_created),
    last_modified = system_time_to_string(metadata.last_time_modified),
    last_accessed = system_time_to_string(metadata.last_accessed),
    file_size = metadata.file_size,
    file_type = file_type_to_string(metadata.file_type),
    );
    /////////////////////////////////////////////////////////////////////////

    let mut file = OpenOptions::new()
        .append(true)
        .create(true)
        .open(file_path)?;

    file.write_all(metadata_block.as_bytes())?;
    file.flush()?;
    Ok(())
}

async fn internal_behavior<A: SteadyActor>(mut actor: A, metadata_rx: SteadyRx<MetadataFields>)-> Result<(), Box<dyn Error>>{

    let mut metadata_rx = metadata_rx.lock().await;
   

    while actor.is_running(||metadata_rx.is_closed_and_empty()){

        await_for_all!(actor.wait_avail(&mut metadata_rx,1));
        
        while let Some(msg) = actor.try_take(&mut metadata_rx){
            
            let _ = write_metadata_line("metadata_output.txt", &msg);
            
        }
        
    }
    Ok(())

}