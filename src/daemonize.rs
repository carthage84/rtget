use crate::error::AppError;

/// Detach this process so the download continues in the background.
///
/// Unix: daemonize the current process (parent prints a message and exits).
/// Windows: spawn a detached copy of this executable without `-b`.
pub fn go_background() -> Result<(), AppError> {
    #[cfg(unix)]
    {
        unix_daemonize()
    }
    #[cfg(windows)]
    {
        windows_spawn_detached()
    }
    #[cfg(not(any(unix, windows)))]
    {
        Err(AppError::Download(
            "background mode is not supported on this platform".into(),
        ))
    }
}

#[cfg(unix)]
fn unix_daemonize() -> Result<(), AppError> {
    use std::fs::File;

    eprintln!("Continuing in background. Output is written to rtget.log");
    let log = File::create("rtget.log")
        .map_err(|e| AppError::Download(format!("could not create rtget.log: {e}")))?;
    let err_log = log
        .try_clone()
        .map_err(|e| AppError::Download(format!("could not clone log handle: {e}")))?;

    daemonize::Daemonize::new()
        .working_directory(".")
        .stdout(log)
        .stderr(err_log)
        .start()
        .map_err(|e| AppError::Download(format!("failed to daemonize: {e}")))?;
    Ok(())
}

#[cfg(windows)]
fn windows_spawn_detached() -> Result<(), AppError> {
    use std::fs::OpenOptions;
    use std::os::windows::process::CommandExt;
    use std::process::{Command, Stdio};

    const DETACHED_PROCESS: u32 = 0x00000008;
    const CREATE_NEW_PROCESS_GROUP: u32 = 0x00000200;
    const CREATE_NO_WINDOW: u32 = 0x08000000;

    let exe = std::env::current_exe()?;
    let log = OpenOptions::new()
        .create(true)
        .append(true)
        .open("rtget.log")
        .map_err(|e| AppError::Download(format!("could not create rtget.log: {e}")))?;
    let err_log = log.try_clone()?;

    let mut cmd = Command::new(exe);
    for arg in std::env::args().skip(1) {
        if arg == "-b" || arg == "--background" {
            continue;
        }
        cmd.arg(arg);
    }

    cmd.stdin(Stdio::null())
        .stdout(Stdio::from(log))
        .stderr(Stdio::from(err_log))
        .creation_flags(DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP | CREATE_NO_WINDOW)
        .spawn()
        .map_err(|e| AppError::Download(format!("failed to start background process: {e}")))?;

    eprintln!("Continuing in background. Output is written to rtget.log");
    std::process::exit(0);
}
