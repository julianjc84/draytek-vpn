use std::process::{Command, Stdio};
use std::time::Instant;

use ksni::menu::{MenuItem, StandardItem};
use ksni::{Status, ToolTip};
use tokio::sync::mpsc;
use zbus::zvariant::OwnedObjectPath;

use crate::format::{format_bytes, format_duration, format_packets};
use crate::nm_monitor::{SavedVpn, VpnState};
use crate::stats::NetStats;

pub struct VpnTray {
    pub vpn_state: VpnState,
    pub stats: Option<NetStats>,
    pub connected_since: Option<Instant>,
    pub saved_vpns: Vec<SavedVpn>,
    pub disconnect_tx: mpsc::UnboundedSender<OwnedObjectPath>,
    pub connect_tx: mpsc::UnboundedSender<OwnedObjectPath>,
}

impl ksni::Tray for VpnTray {
    const MENU_ON_ACTIVATE: bool = true;

    fn id(&self) -> String {
        "draytek-vpn-tray".to_string()
    }

    fn category(&self) -> ksni::Category {
        ksni::Category::Communications
    }

    fn title(&self) -> String {
        "DrayTek VPN".to_string()
    }

    fn icon_name(&self) -> String {
        String::new()
    }

    fn icon_pixmap(&self) -> Vec<ksni::Icon> {
        let icon = match &self.vpn_state {
            VpnState::Disconnected => &*crate::icons::DISCONNECTED,
            VpnState::Connecting { .. } | VpnState::Disconnecting => &*crate::icons::CONNECTING,
            VpnState::Connected { .. } => &*crate::icons::CONNECTED,
        };
        vec![icon.clone()]
    }

    fn status(&self) -> Status {
        // Always visible in the tray
        Status::Active
    }

    fn tool_tip(&self) -> ToolTip {
        let title = "DrayTek VPN".to_string();
        let description = match &self.vpn_state {
            VpnState::Disconnected => "Disconnected".to_string(),
            VpnState::Connecting { name } => format!("Connecting: {name}"),
            VpnState::Connected { name, ip, .. } => format!("Connected: {name} ({ip})"),
            VpnState::Disconnecting => "Disconnecting...".to_string(),
        };
        ToolTip {
            icon_name: String::new(),
            icon_pixmap: Vec::new(),
            title,
            description,
        }
    }

    fn menu(&self) -> Vec<MenuItem<Self>> {
        match &self.vpn_state {
            VpnState::Disconnected => {
                let mut items: Vec<MenuItem<Self>> = Vec::new();
                items.push(label("Disconnected"));
                items.push(MenuItem::Separator);

                if self.saved_vpns.is_empty() {
                    items.push(label("No saved VPN connections"));
                } else {
                    for vpn in &self.saved_vpns {
                        let path = vpn.path.clone();
                        items.push(
                            StandardItem {
                                label: format!("Connect: {}", vpn.name),
                                enabled: true,
                                visible: true,
                                icon_name: String::new(),
                                icon_data: Vec::new(),
                                shortcut: Vec::new(),
                                disposition: ksni::menu::Disposition::Normal,
                                activate: Box::new(move |tray: &mut VpnTray| {
                                    let _ = tray.connect_tx.send(path.clone());
                                }),
                            }
                            .into(),
                        );
                    }
                }

                items.push(MenuItem::Separator);
                items.push(
                    StandardItem {
                        label: "Network Connections...".to_string(),
                        enabled: true,
                        visible: true,
                        icon_name: String::new(),
                        icon_data: Vec::new(),
                        shortcut: Vec::new(),
                        disposition: ksni::menu::Disposition::Normal,
                        activate: Box::new(|_: &mut VpnTray| {
                            let _ = Command::new("nm-connection-editor")
                                .stdout(Stdio::null()).stderr(Stdio::null())
                                .spawn();
                        }),
                    }
                    .into(),
                );
                items.push(
                    StandardItem {
                        label: "Network Settings...".to_string(),
                        enabled: true,
                        visible: true,
                        icon_name: String::new(),
                        icon_data: Vec::new(),
                        shortcut: Vec::new(),
                        disposition: ksni::menu::Disposition::Normal,
                        activate: Box::new(|_: &mut VpnTray| {
                            let _ = Command::new("cinnamon-settings").arg("network")
                                .stdout(Stdio::null()).stderr(Stdio::null())
                                .spawn()
                                .or_else(|_| Command::new("gnome-control-center").arg("network")
                                    .stdout(Stdio::null()).stderr(Stdio::null())
                                    .spawn());
                        }),
                    }
                    .into(),
                );

                items
            }
            VpnState::Connecting { name } => {
                vec![label(&format!("Connecting: {name}..."))]
            }
            VpnState::Disconnecting => {
                vec![label("Disconnecting...")]
            }
            VpnState::Connected { name, ip, routes, path } => {
                let mut items: Vec<MenuItem<Self>> = Vec::new();

                items.push(label(&format!("Connected: {name}")));
                items.push(label(&format!("IP: {ip}")));

                if let Some(since) = self.connected_since {
                    let elapsed = since.elapsed().as_secs();
                    items.push(label(&format!("Time: {}", format_duration(elapsed))));
                }

                if !routes.is_empty() {
                    items.push(MenuItem::Separator);
                    for route in routes {
                        items.push(label(&format!("Route: {route}")));
                    }
                }

                items.push(MenuItem::Separator);

                if let Some(stats) = &self.stats {
                    items.push(label(&format!(
                        "TX: {} ({} pkts)",
                        format_bytes(stats.tx_bytes),
                        format_packets(stats.tx_packets)
                    )));
                    items.push(label(&format!(
                        "RX: {} ({} pkts)",
                        format_bytes(stats.rx_bytes),
                        format_packets(stats.rx_packets)
                    )));
                } else {
                    items.push(label("TX: --"));
                    items.push(label("RX: --"));
                }

                items.push(MenuItem::Separator);

                let path = path.clone();
                items.push(
                    StandardItem {
                        label: "Disconnect".to_string(),
                        enabled: true,
                        visible: true,
                        icon_name: String::new(),
                        icon_data: Vec::new(),
                        shortcut: Vec::new(),
                        disposition: ksni::menu::Disposition::Normal,
                        activate: Box::new(move |tray: &mut VpnTray| {
                            let _ = tray.disconnect_tx.send(path.clone());
                        }),
                    }
                    .into(),
                );

                items
            }
        }
    }
}

/// Create a disabled label menu item.
fn label(text: &str) -> MenuItem<VpnTray> {
    StandardItem {
        label: text.to_string(),
        enabled: false,
        visible: true,
        icon_name: String::new(),
        icon_data: Vec::new(),
        shortcut: Vec::new(),
        disposition: ksni::menu::Disposition::Normal,
        activate: Box::new(|_| {}),
    }
    .into()
}
