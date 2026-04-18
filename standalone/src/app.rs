/// Application state and initialization.
use crate::logging::{LogBuffer, LogBufferMakeWriter};
use crate::ui::window::MainWindow;
use libadwaita as adw;
use libadwaita::prelude::*;
use tracing_subscriber::fmt::time::OffsetTime;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::EnvFilter;

pub fn build_app() {
    // Initialize logging with local time in HH:MM:SS format
    let log_buffer = LogBuffer::new(1000);
    let log_writer = LogBufferMakeWriter::new(log_buffer.clone());

    let time_format =
        time::format_description::parse("[hour]:[minute]:[second]").expect("invalid time format");
    let offset = time::UtcOffset::current_local_offset().unwrap_or(time::UtcOffset::UTC);
    let timer = OffsetTime::new(offset, time_format.clone());
    let timer2 = OffsetTime::new(offset, time_format.clone());

    tracing_subscriber::registry()
        .with(EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")))
        .with(
            tracing_subscriber::fmt::layer()
                .with_writer(std::io::stderr)
                .with_target(false)
                .with_timer(timer),
        )
        .with(
            tracing_subscriber::fmt::layer()
                .with_writer(log_writer)
                .with_target(false)
                .with_ansi(false)
                .with_timer(timer2),
        )
        .init();

    // Create tokio runtime for background I/O
    let tokio_rt = tokio::runtime::Runtime::new().expect("Failed to create tokio runtime");
    let tokio_handle = tokio_rt.handle().clone();

    // Keep the runtime alive for the lifetime of the app
    let _rt_guard = tokio_rt.enter();

    let app = adw::Application::builder()
        .application_id("com.draytek.vpn.linux")
        .build();

    app.connect_activate(move |app| {
        let main_window = MainWindow::new(app, log_buffer.clone(), tokio_handle.clone());
        main_window.window.present();
    });

    // Run with empty args (GTK parses argv internally)
    app.run_with_args::<String>(&[]);

    // Ensure tokio runtime shuts down cleanly — give teardown time to finish
    drop(_rt_guard);
    tokio_rt.shutdown_timeout(std::time::Duration::from_secs(5));
}
