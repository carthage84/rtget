mod args;
mod config;
mod daemonize;
mod downloader;
mod error;
mod filesystem;
mod html_redirect;
mod progress;
mod proxy;
mod retry;
mod url_validator;

use log::{LevelFilter, error, info};

use crate::args::CommandLineArgs;
use crate::config::DownloadConfig;
use crate::error::AppError;
use crate::progress::ProgressManager;

fn main() {
    let args: CommandLineArgs = argh::from_env();
    init_logging(args.verbose, args.quiet);

    if args.background
        && let Err(e) = daemonize::go_background()
    {
        report_error(&e, args.verbose);
        std::process::exit(1);
    }

    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("failed to start async runtime");

    if let Err(e) = runtime.block_on(run(args.clone())) {
        report_error(&e, args.verbose);
        std::process::exit(1);
    }
}

async fn run(args: CommandLineArgs) -> Result<(), AppError> {
    let config = DownloadConfig::from_args(&args)?;
    if !config.quiet {
        eprintln!("Downloading from {}", config.url);
    } else {
        info!("Downloading from {}", config.url);
    }

    let progress = ProgressManager::new(config.quiet);
    downloader::download(&config, &progress).await
}

fn init_logging(verbose: bool, quiet: bool) {
    let level = if verbose {
        LevelFilter::Debug
    } else if quiet {
        LevelFilter::Warn
    } else {
        LevelFilter::Info
    };
    let _ = env_logger::Builder::from_default_env()
        .filter_level(level)
        .format_timestamp_secs()
        .try_init();
}

fn report_error(err: &AppError, verbose: bool) {
    if verbose {
        error!("{err:?}");
    } else {
        eprintln!("Error: {err}");
    }
}
