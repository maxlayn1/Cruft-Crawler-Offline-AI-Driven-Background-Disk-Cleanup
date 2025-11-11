use clap::Parser;
use std::path::PathBuf;
use steady_state::LogLevel;

#[derive(Parser, Debug, Clone)]
#[command(name = "cruft_crawler")]
#[command(about = "A Steady State actor that scans directories and files")]

pub struct Args {

    //Directory to scan
    #[arg(short, long, default_value = ".")]
    pub directory: PathBuf,

    // Output file to write paths to
    #[arg(short, long, default_value = "file_paths.txt")]
    pub out_putfile: PathBuf,

    // Log level
    #[arg(short, long, default_value = "info")]
    pub loglevel: LogLevel,

    // Enable recursive directory traversal
    #[arg(short, long, default_value = "true")]
    pub recursive: bool,
    
}