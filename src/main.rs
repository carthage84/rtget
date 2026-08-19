mod args;
mod checksum;
mod config;
mod cookies;
mod daemonize;
mod downloader;
mod error;
mod filesystem;
mod html_redirect;
mod progress;
mod proxy;
mod rate;
mod retry;
mod url_validator;

use std::sync::Arc;

use log::{LevelFilter, error, info};

use crate::args::CommandLineArgs;
use crate::config::{DownloadConfig, FileConfig};
use crate::cookies::CookieJar;
use crate::downloader::TransferResult;
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

async fn run(mut args: CommandLineArgs) -> Result<(), AppError> {
    let file = FileConfig::load(&args)?;
    let urls = args.collect_urls()?;

    if urls.len() > 1 {
        if let Some(out) = args.output_hint()
            && !filesystem::dir_hint(&out)
        {
            return Err(AppError::InvalidArgument(
                "-o must be a directory when downloading more than one URL (or use -P)".into(),
            ));
        }
        if args.checksum.is_some() {
            return Err(AppError::InvalidArgument(
                "--checksum can only be used with a single URL".into(),
            ));
        }
    }

    if args.ask_password {
        args.password = Some(config::prompt_password()?);
    }

    let jar = Arc::new(CookieJar::new());
    if let Some(path) = &args.load_cookies {
        jar.load_netscape(std::path::Path::new(path))?;
    }

    let mut template = DownloadConfig::from_args_and_file(&args, &file, urls[0].clone())?;
    template.cookie_jar = Some(jar.clone());
    if args.ask_password {
        template.password = args.password.clone();
    }

    let progress = ProgressManager::new(template.quiet);
    let total = urls.len();
    let mut failed = 0usize;

    for (i, url) in urls.into_iter().enumerate() {
        let config = template.with_url(url);
        if !config.quiet {
            if total > 1 {
                eprintln!("[{}/{}] Downloading from {}", i + 1, total, config.url);
            } else if !config.spider {
                eprintln!("Downloading from {}", config.url);
            }
        } else {
            info!("Downloading from {}", config.url);
        }

        match downloader::download(&config, &progress).await {
            Ok(TransferResult::Saved(path)) => {
                if !config.quiet && !config.spider {
                    info!("Saved {}", path.display());
                }
            }
            Ok(TransferResult::Skipped(path)) => {
                if !config.quiet {
                    eprintln!("Skipping existing file {}", path.display());
                }
            }
            Ok(TransferResult::Spider) => {}
            Err(e) if args.fail_fast => return Err(e),
            Err(e) => {
                report_error(&e, args.verbose);
                failed += 1;
            }
        }
    }

    if let Some(path) = &args.save_cookies {
        jar.save_netscape(std::path::Path::new(path))?;
    }

    if failed > 0 {
        Err(AppError::Download(format!(
            "{failed} of {total} downloads failed"
        )))
    } else {
        Ok(())
    }
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
