mod format;
mod icons;
mod nm_monitor;
mod stats;
mod tray_impl;

use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::Result;
use ksni::TrayMethods;
use tokio::sync::{mpsc, watch};
use tracing::{error, info, warn};
use zbus::zvariant::OwnedObjectPath;

use nm_monitor::VpnState;
use tray_impl::VpnTray;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "draytek_vpn_tray=info".into()),
        )
        .init();

    info!("DrayTek VPN tray indicator starting");

    let (state_tx, state_rx) = watch::channel(VpnState::Disconnected);
    let (disconnect_tx, mut disconnect_rx) = mpsc::unbounded_channel::<OwnedObjectPath>();
    let (connect_tx, mut connect_rx) = mpsc::unbounded_channel::<OwnedObjectPath>();

    // Fetch saved DrayTek VPN connections for the menu
    let saved_vpns = nm_monitor::list_saved_vpns().await;
    info!("found {} saved DrayTek VPN connection(s)", saved_vpns.len());

    let tray = VpnTray {
        vpn_state: VpnState::Disconnected,
        stats: None,
        connected_at: None,
        saved_vpns,
        disconnect_tx,
        connect_tx,
    };

    let handle = tray.spawn().await?;
    info!("tray icon registered");

    // Task 1: Monitor NM for DrayTek VPN connections
    let monitor_tx = state_tx.clone();
    tokio::spawn(async move {
        loop {
            if let Err(e) = nm_monitor::monitor_vpn(monitor_tx.clone()).await {
                error!("NM monitor error: {e:#}");
            }
            // Retry after a delay
            tokio::time::sleep(std::time::Duration::from_secs(5)).await;
            warn!("restarting NM monitor");
        }
    });

    // Task 2: Handle disconnect requests
    tokio::spawn(async move {
        while let Some(path) = disconnect_rx.recv().await {
            info!("disconnect requested for {path}");
            if let Err(e) = nm_monitor::disconnect_vpn(&path).await {
                error!("disconnect failed: {e:#}");
            }
        }
    });

    // Task 3: Handle connect requests
    let handle2 = handle.clone();
    tokio::spawn(async move {
        while let Some(path) = connect_rx.recv().await {
            info!("connect requested for {path}");
            if let Err(e) = nm_monitor::connect_vpn(&path).await {
                error!("connect failed: {e:#}");
            }
        }
        drop(handle2); // keep handle alive
    });

    // Main loop: watch state changes + poll stats every 3s, update tray
    let mut connected_at: Option<u64> = None;
    let mut stats_interval = tokio::time::interval(std::time::Duration::from_secs(10));
    let mut state_rx = state_rx;

    loop {
        tokio::select! {
            result = state_rx.changed() => {
                if result.is_err() {
                    break; // channel closed
                }
                let new_state = state_rx.borrow_and_update().clone();

                // Use NM's activation timestamp, fall back to current time
                match &new_state {
                    VpnState::Connected { connected_at: ts, .. } if connected_at.is_none() => {
                        connected_at = Some(if *ts > 0 {
                            *ts
                        } else {
                            SystemTime::now()
                                .duration_since(UNIX_EPOCH)
                                .map(|d| d.as_secs())
                                .unwrap_or(0)
                        });
                    }
                    VpnState::Disconnected => {
                        connected_at = None;
                    }
                    _ => {}
                }

                // Refresh saved VPNs list when transitioning to disconnected
                let saved = if matches!(new_state, VpnState::Disconnected) {
                    Some(nm_monitor::list_saved_vpns().await)
                } else {
                    None
                };

                let at = connected_at;
                let stats = if matches!(new_state, VpnState::Connected { .. }) {
                    stats::read_stats().await
                } else {
                    None
                };

                handle.update(|tray| {
                    tray.vpn_state = new_state;
                    tray.connected_at = at;
                    tray.stats = stats;
                    if let Some(vpns) = saved {
                        tray.saved_vpns = vpns;
                    }
                }).await;
            }
            _ = stats_interval.tick() => {
                // Only poll stats when connected
                if connected_at.is_some() {
                    let net_stats = stats::read_stats().await;
                    let at = connected_at;
                    handle.update(|tray| {
                        tray.stats = net_stats;
                        tray.connected_at = at;
                    }).await;
                }
            }
        }
    }

    Ok(())
}
