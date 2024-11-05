mod args;
mod progress;
mod error;
mod concurrency;
mod downloader;
mod url_validator;
mod daemonize;
//mod filesystem;

use args::CommandLineArgs;
use url_validator::validate_url;

// Main function for the application
// This is the entry point for the application
#[tokio::main]
async fn main() {
    // Parse command line arguments
    let args: CommandLineArgs = argh::from_env();

    // Validate the URL
    match validate_url(&args.url) {
        Ok(valid_url) => {
            println!("Downloading from {}", valid_url.to_string());
        }
        Err(error) => {
            eprintln!("Error: {}", error);
            return;
        }
    }

    // Run the application in the foreground or background
    if args.background {
        run_in_background().await;
    } else {
        run_in_foreground().await;
    }
}

// Run the application in the background
// This function will fork the current process into a daemon process
// This is required to run the application in the background
async fn run_in_background() {
    daemonize::daemonize();
    return;
}

// Run the application in the foreground
// This function will run the application in the foreground
async fn run_in_foreground() {
    
}