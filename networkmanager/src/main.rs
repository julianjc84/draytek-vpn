mod plugin;
mod tun_device;
mod tunnel;

use tracing::info;

fn main() {
    // Initialize logging
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    info!("DrayTek VPN NetworkManager plugin starting");

    let rt = tokio::runtime::Runtime::new().expect("Failed to create tokio runtime");
    rt.block_on(async {
        if let Err(e) = plugin::run().await {
            tracing::error!("Plugin error: {e:#}");
            std::process::exit(1);
        }
    });
}
