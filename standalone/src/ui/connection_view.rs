/// Connection status display + connect/disconnect controls.
use crate::messages::TunnelStatus;
use gtk4::prelude::*;
use std::cell::Cell;
use std::time::Instant;

/// Build the connection status view.
pub struct ConnectionView {
    pub container: gtk4::Box,
    status_label: gtk4::Label,
    timer_label: gtk4::Label,
    // Status text for non-connected states (e.g. "Establishing TLS connection")
    details_label: gtk4::Label,
    // Individual info labels (visible when connected)
    info_box: gtk4::Box,
    ip_label: gtk4::Label,
    gateway_label: gtk4::Label,
    dns_label: gtk4::Label,
    mtu_label: gtk4::Label,
    routing_label: gtk4::Label,
    // Individual stats labels (visible when connected)
    stats_box: gtk4::Box,
    tx_label: gtk4::Label,
    rx_label: gtk4::Label,
    oversized_label: gtk4::Label,
    max_pkt_label: gtk4::Label,
    pub connect_btn: gtk4::Button,
    pub disconnect_btn: gtk4::Button,
    pub keepalive_btn: gtk4::ToggleButton,
    status_icon: gtk4::Image,
    connected_since: Cell<Option<Instant>>,
}

fn info_label(tooltip: &str) -> gtk4::Label {
    gtk4::Label::builder()
        .css_classes(["dim-label"])
        .xalign(0.0)
        .selectable(true)
        .tooltip_text(tooltip)
        .build()
}

fn stat_label(tooltip: &str) -> gtk4::Label {
    gtk4::Label::builder()
        .css_classes(["monospace", "dim-label"])
        .xalign(0.0)
        .selectable(true)
        .tooltip_text(tooltip)
        .build()
}

impl ConnectionView {
    pub fn new() -> Self {
        let container = gtk4::Box::new(gtk4::Orientation::Vertical, 12);
        container.set_margin_top(24);
        container.set_margin_bottom(24);
        container.set_margin_start(24);
        container.set_margin_end(24);

        // Status icon
        let status_icon = gtk4::Image::builder()
            .icon_name("network-offline-symbolic")
            .pixel_size(64)
            .css_classes(["dim-label"])
            .build();

        // Status row: status label + timer side by side
        let status_row = gtk4::Box::new(gtk4::Orientation::Horizontal, 8);
        status_row.set_halign(gtk4::Align::Center);

        let status_label = gtk4::Label::builder()
            .label("Disconnected")
            .css_classes(["title-2"])
            .build();

        let timer_label = gtk4::Label::builder()
            .label("")
            .css_classes(["title-2", "dim-label"])
            .visible(false)
            .build();

        status_row.append(&status_label);
        status_row.append(&timer_label);

        // Details label for non-connected status text
        let details_label = gtk4::Label::builder()
            .label("Select a profile and connect")
            .css_classes(["dim-label"])
            .xalign(0.0)
            .wrap(true)
            .build();

        // Individual info labels (hidden until connected)
        let info_box = gtk4::Box::new(gtk4::Orientation::Vertical, 4);
        info_box.set_visible(false);

        let ip_label = info_label("Your IP address on the VPN network, assigned by the router");
        let gateway_label = info_label("The VPN gateway — the router's IP on the tunnel network");
        let dns_label = info_label("DNS server provided by the VPN for resolving domain names");
        let mtu_label = info_label(
            "Maximum Transmission Unit — largest packet size allowed.\n\
             'ours' is what we can receive, 'router' is what the router accepts.\n\
             The effective MTU is the smaller of the two.",
        );
        let routing_label = info_label(
            "Routes applied to the VPN tunnel — determines which traffic goes through the VPN",
        );

        info_box.append(&ip_label);
        info_box.append(&gateway_label);
        info_box.append(&dns_label);
        info_box.append(&mtu_label);
        info_box.append(&routing_label);

        // Individual stats labels (hidden until connected)
        let stats_box = gtk4::Box::new(gtk4::Orientation::Vertical, 4);
        stats_box.set_visible(false);

        let tx_label = stat_label("Data sent through the VPN tunnel to the remote network");
        let rx_label = stat_label("Data received through the VPN tunnel from the remote network");
        let oversized_label =
            stat_label("Packets that exceeded the MTU limit — may cause fragmentation or drops");
        let max_pkt_label = stat_label(
            "Largest single packet seen in each direction this session.\n\
             Compare with MTU to see how close traffic gets to the limit.",
        );

        stats_box.append(&tx_label);
        stats_box.append(&rx_label);
        stats_box.append(&oversized_label);
        stats_box.append(&max_pkt_label);

        // Connect button
        let connect_btn = gtk4::Button::builder()
            .label("Connect")
            .css_classes(["suggested-action"])
            .build();

        // Disconnect button (hidden initially)
        let disconnect_btn = gtk4::Button::builder()
            .label("Disconnect")
            .css_classes(["destructive-action"])
            .visible(false)
            .build();

        // Keepalive toggle button (hidden until connected)
        let keepalive_btn = gtk4::ToggleButton::builder()
            .label("Keepalive: OFF")
            .tooltip_text("Send periodic pings to prevent idle timeout")
            .visible(false)
            .build();

        container.append(&status_icon);
        container.append(&status_row);
        container.append(&details_label);
        container.append(&info_box);
        container.append(&stats_box);

        let btn_row = gtk4::Box::new(gtk4::Orientation::Horizontal, 12);
        btn_row.set_homogeneous(true);
        btn_row.append(&connect_btn);
        btn_row.append(&disconnect_btn);
        btn_row.append(&keepalive_btn);
        container.append(&btn_row);

        ConnectionView {
            container,
            status_label,
            timer_label,
            details_label,
            info_box,
            ip_label,
            gateway_label,
            dns_label,
            mtu_label,
            routing_label,
            stats_box,
            tx_label,
            rx_label,
            oversized_label,
            max_pkt_label,
            connect_btn,
            disconnect_btn,
            keepalive_btn,
            status_icon,
            connected_since: Cell::new(None),
        }
    }

    /// Update the view based on tunnel status.
    pub fn update_status(&self, status: &TunnelStatus) {
        match status {
            TunnelStatus::Connecting => {
                self.status_label.set_label("Connecting...");
                self.details_label.set_label("Establishing TLS connection");
                self.details_label.set_visible(true);
                self.info_box.set_visible(false);
                self.stats_box.set_visible(false);
                self.status_icon
                    .set_icon_name(Some("network-transmit-symbolic"));
                self.connect_btn.set_visible(false);
                self.disconnect_btn.set_visible(true);
            }
            TunnelStatus::Handshaking => {
                self.status_label.set_label("Handshaking...");
                self.details_label.set_label("HTTP CONNECT handshake");
            }
            TunnelStatus::NegotiatingLcp => {
                self.status_label.set_label("Negotiating...");
                self.details_label.set_label("LCP negotiation");
            }
            TunnelStatus::Authenticating => {
                self.status_label.set_label("Authenticating...");
                self.details_label.set_label("Verifying credentials");
            }
            TunnelStatus::NegotiatingIpcp => {
                self.status_label.set_label("Negotiating...");
                self.details_label
                    .set_label("IPCP negotiation (IP assignment)");
            }
            TunnelStatus::Connected {
                local_ip,
                remote_ip,
                dns,
                mtu,
                local_mru,
                remote_mru,
                default_gateway,
                remote_network_route,
                additional_routes,
            } => {
                self.connected_since.set(Some(Instant::now()));
                self.status_label.set_label("Connected");
                self.timer_label.set_label("00:00:00");
                self.timer_label.set_visible(true);
                self.details_label.set_visible(false);

                self.ip_label.set_label(&format!("IP: {local_ip}"));
                self.gateway_label
                    .set_label(&format!("Gateway: {remote_ip}"));
                let dns_str = dns
                    .map(|d| format!("DNS: {d}"))
                    .unwrap_or_else(|| "DNS: none".to_string());
                self.dns_label.set_label(&dns_str);
                self.mtu_label.set_label(&format!(
                    "MTU: {mtu}  (ours: {local_mru} / router: {remote_mru})"
                ));

                let routing_text = if *default_gateway {
                    "Routing: All traffic (default gateway)".to_string()
                } else {
                    let mut parts: Vec<String> = Vec::new();
                    if let Some(subnet) = remote_network_route {
                        parts.push(format!("Remote network ({subnet})"));
                    }
                    parts.extend(additional_routes.iter().cloned());
                    if parts.is_empty() {
                        "Routing: None (no traffic routed through tunnel)".to_string()
                    } else {
                        format!("Routing: {}", parts.join(", "))
                    }
                };
                self.routing_label.set_label(&routing_text);

                self.info_box.set_visible(true);

                self.tx_label.set_label("");
                self.rx_label.set_label("");
                self.oversized_label.set_label("");
                self.max_pkt_label.set_label("");
                self.stats_box.set_visible(true);

                self.status_icon.set_icon_name(Some("network-vpn-symbolic"));
                self.status_icon.remove_css_class("dim-label");
                self.status_icon.add_css_class("success");
                self.connect_btn.set_visible(false);
                self.disconnect_btn.set_visible(true);
                self.keepalive_btn.set_visible(true);
                self.keepalive_btn.set_active(false);
                self.keepalive_btn.set_label("Keepalive: OFF");
                self.keepalive_btn.remove_css_class("suggested-action");
            }
            TunnelStatus::Disconnecting => {
                self.status_label.set_label("Disconnecting...");
                self.details_label.set_label("Tearing down tunnel");
                self.details_label.set_visible(true);
                self.info_box.set_visible(false);
                self.disconnect_btn.set_sensitive(false);
            }
            TunnelStatus::Disconnected => {
                self.connected_since.set(None);
                self.timer_label.set_visible(false);
                self.info_box.set_visible(false);
                self.stats_box.set_visible(false);
                self.keepalive_btn.set_visible(false);
                self.status_label.set_label("Disconnected");
                self.details_label.set_label("Select a profile and connect");
                self.details_label.set_visible(true);
                self.status_icon
                    .set_icon_name(Some("network-offline-symbolic"));
                self.status_icon.remove_css_class("success");
                self.status_icon.add_css_class("dim-label");
                self.connect_btn.set_visible(true);
                self.connect_btn.set_sensitive(true);
                self.disconnect_btn.set_visible(false);
                self.disconnect_btn.set_sensitive(true);
            }
            TunnelStatus::Error(msg) => {
                self.connected_since.set(None);
                self.timer_label.set_visible(false);
                self.info_box.set_visible(false);
                self.stats_box.set_visible(false);
                self.keepalive_btn.set_visible(false);
                self.status_label.set_label("Error");
                self.details_label.set_label(msg);
                self.details_label.set_visible(true);
                self.status_icon
                    .set_icon_name(Some("dialog-error-symbolic"));
                self.status_icon.remove_css_class("success");
                self.status_icon.remove_css_class("dim-label");
                self.status_icon.add_css_class("error");
                self.connect_btn.set_visible(true);
                self.connect_btn.set_sensitive(true);
                self.disconnect_btn.set_visible(false);
            }
            TunnelStatus::AuthFailed => {
                self.connected_since.set(None);
                self.timer_label.set_visible(false);
                self.info_box.set_visible(false);
                self.stats_box.set_visible(false);
                self.keepalive_btn.set_visible(false);
                self.status_label.set_label("Authentication Failed");
                self.details_label
                    .set_label("Check your username and password");
                self.details_label.set_visible(true);
                self.status_icon
                    .set_icon_name(Some("dialog-error-symbolic"));
                self.status_icon.remove_css_class("success");
                self.status_icon.add_css_class("error");
                self.connect_btn.set_visible(true);
                self.connect_btn.set_sensitive(true);
                self.disconnect_btn.set_visible(false);
            }
            TunnelStatus::Stats {
                bytes_tx,
                bytes_rx,
                packets_tx,
                packets_rx,
                oversized_tx,
                oversized_rx,
                max_packet_tx,
                max_packet_rx,
            } => {
                self.tx_label.set_label(&format!(
                    "TX: {} pkts  {}",
                    format_count(*packets_tx),
                    format_bytes(*bytes_tx),
                ));
                self.rx_label.set_label(&format!(
                    "RX: {} pkts  {}",
                    format_count(*packets_rx),
                    format_bytes(*bytes_rx),
                ));
                self.oversized_label.set_label(&format!(
                    "Oversized: {} TX / {} RX",
                    oversized_tx, oversized_rx,
                ));
                self.max_pkt_label.set_label(&format!(
                    "Max pkt: {} TX / {} RX",
                    max_packet_tx, max_packet_rx,
                ));
            }
        }
    }

    /// Update the connection timer label. Called from the 100ms UI poll loop.
    pub fn tick(&self) {
        if let Some(since) = self.connected_since.get() {
            let elapsed = since.elapsed().as_secs();
            let h = elapsed / 3600;
            let m = (elapsed % 3600) / 60;
            let s = elapsed % 60;
            self.timer_label.set_label(&format!("{h:02}:{m:02}:{s:02}"));
        }
    }
}

fn format_bytes(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = 1024 * 1024;
    const GB: u64 = 1024 * 1024 * 1024;
    if bytes >= GB {
        format!("{:.1} GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.1} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.1} KB", bytes as f64 / KB as f64)
    } else {
        format!("{bytes} B")
    }
}

fn format_count(count: u64) -> String {
    if count >= 1_000_000 {
        format!("{:.1}M", count as f64 / 1_000_000.0)
    } else if count >= 1_000 {
        format!("{:.1}K", count as f64 / 1_000.0)
    } else {
        format!("{count}")
    }
}
