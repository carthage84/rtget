mod args;
mod progress;
mod error;
mod concurrency;
mod downloader;
mod url_validator;
mod daemonize;
mod filesystem;

use args::CommandLineArgs;
use std::path::{Path, PathBuf};
use log::{error, info, LevelFilter};
use url::Url;
use crate::filesystem::FileSystem;
use url_validator::validate_url;
use crate::concurrency::{ConcurrentDownloader};
use crate::downloader::{Downloader, FileDownloader};
use crate::error::AppError;
use crate::progress::ProgressManager;

// Main function for the application
// This is the entry point for the application
#[tokio::main]
async fn main() {
    // Parse command line arguments
    let args: CommandLineArgs = argh::from_env();

    init_logging(args.verbose);

    // Run the application and handle errors
    if let Err(e) = run(args.clone()).await {
        if args.verbose {
            error!("Error: {}", e); // Use Debug format
        } else {
            eprintln!("Error: {}", e); // Use Display format
        }
        std::process::exit(1); // Exit with error code
    }
}

async fn run(args: CommandLineArgs) -> Result<(), AppError> {
    // Validate the URL
    let valid_url = validate_url(&args.url)
        .map_err(|e| AppError::UrlParseError(e.to_string()))?;
    println!("Downloading from {}", valid_url);

    // Run the application in the foreground or background
    if args.background {
        run_in_background(args).await?;
    } else {
        run_in_foreground(args).await?;
    }
    Ok(())
}

// Run the application in the background
// This function will fork the current process into a daemon process
// This is required to run the application in the background
async fn run_in_background(args: CommandLineArgs) -> Result<(), AppError> {
    daemonize::daemonize();
    Ok(())
}

// Run the application in the foreground
// This function will run the application in the foreground
async fn run_in_foreground(args: CommandLineArgs) -> Result<(), AppError> {
    let downloader = FileDownloader::new();
    let total_size = downloader.get_total_file_size(&args.url).await?;
    let mut progress_manager = ProgressManager::new();
    let byte_ranges = FileDownloader::calculate_byte_ranges(args.connections as usize, total_size);

    // Derive output path from args.output or URL
    let output_path = match args.output {
        Some(ref output) => PathBuf::from(output),
        None => {
            let url = Url::parse(&args.url)
                .map_err(|e| AppError::UrlParseError(e.to_string()))?;
            let path = url.path();
            let filename = Path::new(path)
                .file_name()
                .ok_or_else(|| AppError::CouldNotConnect("Could not derive filename from URL".to_string()))?;
            PathBuf::from(filename)
        }
    };

    let tasks = downloader.calculate_download_chunks(args.clone()).await?;
    let downloader = ConcurrentDownloader::new(tasks);
    downloader.execute_all(&mut progress_manager, byte_ranges.iter().map(|(start, end)| (*start as u64, *end as u64)).collect()).await?;

    // Merge chunks into final output file
    let fs = FileSystem::new(&output_path, byte_ranges);
    fs.merge_chunks(&output_path, args.connections).await?;

    // Download finished, print final progress
    progress_manager.finish_all(output_path.display());

    Ok(())
}

fn init_logging(verbose: bool) {
    let log_level = if verbose { LevelFilter::Debug } else { LevelFilter::Info };
    env_logger::Builder::from_default_env()
        .filter_level(log_level)
        .init();
}