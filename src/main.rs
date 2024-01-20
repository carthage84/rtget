mod args;
mod progress;
mod error;
mod concurrency;
mod downloader;
mod url_validator;

use args::CommandLineArgs;
use url_validator::validate_url;

fn main() {
    let args: CommandLineArgs = argh::from_env();

    match validate_url(&args.url) {
        Ok(valid_url) => {
            println!("Downloading from {}", valid_url.to_string());
        }
        Err(error) => {
            eprintln!("Error: {}", error);
            return;
        }
    }
}
