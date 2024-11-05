#[cfg(target_os = "linux")]
mod linux {
    /// Daemonize the process on Linux
    pub fn daemonize() {

    }
}

#[cfg(target_os = "windows")]
pub(crate) mod windows {
    #[macro_use]
    use windows_service::{
        define_windows_service,
        service::{
            ServiceControl, ServiceControlAccept, ServiceExitCode, ServiceState, ServiceStatus, ServiceType
        },
        service_control_handler::{self, ServiceControlHandlerResult},
        service_dispatcher,
    };
    use std::{ffi::OsString, sync::mpsc, time::Duration};

    // Define the Windows service entry point
    define_windows_service!(ffi_service_main, service_main);

    // Main logic for the service
    fn service_main(arguments: Vec<OsString>) {
        if let Err(e) = run_service(arguments) {
            // Log the error or handle it as required
        }
    }

    fn run_service(arguments: Vec<OsString>) -> windows_service::Result<()> {
        let (shutdown_tx, shutdown_rx) = mpsc::channel();

        let event_handler = move |control_event| -> ServiceControlHandlerResult {
            match control_event {
                ServiceControl::Stop => {
                    // Handle stop event
                    shutdown_tx.send(()).unwrap();
                    ServiceControlHandlerResult::NoError
                }
                _ => ServiceControlHandlerResult::NotImplemented,
            }
        };

        // Register the service control handler
        let status_handle = service_control_handler::register("rtget", event_handler)?;

        // Set the service status to running
        status_handle.set_service_status(ServiceStatus {
            service_type: ServiceType::OWN_PROCESS,
            current_state: ServiceState::Running,
            controls_accepted: ServiceControlAccept::STOP,
            exit_code: ServiceExitCode::Win32(0),
            checkpoint: 0,
            wait_hint: Duration::default(),
            process_id: None,
        })?;

        // Main service loop
        shutdown_rx.recv().unwrap();

        // Service shutdown
        status_handle.set_service_status(ServiceStatus {
            service_type: ServiceType::OWN_PROCESS,
            current_state: ServiceState::Stopped,
            controls_accepted: ServiceControlAccept::empty(),
            exit_code: ServiceExitCode::Win32(0),
            checkpoint: 0,
            wait_hint: Duration::default(),
            process_id: None,
        })?;

        Ok(())
    }

    /// Function to daemonize the process on Windows.
    pub fn daemonize() {
        // Run the service dispatcher
        // This will block until the service is stopped
        if let Err(_e) = service_dispatcher::start("rtget", ffi_service_main) {

        }
    }
}

/// Cross-platform daemonization function.
pub fn daemonize() {
    #[cfg(target_os = "linux")]
    linux::daemonize();

    #[cfg(target_os = "windows")]
    windows::daemonize();
}