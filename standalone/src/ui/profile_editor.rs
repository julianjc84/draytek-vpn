/// Profile editor dialog for adding/editing VPN connection profiles.
use crate::config::{self, ProfileConfig};
use gtk4::prelude::*;
use libadwaita as adw;
use libadwaita::prelude::*;

/// Show a profile editor dialog.
///
/// `on_save` is called with the edited profile when the user clicks Save.
/// `on_delete` is called (if `Some`) when the user confirms deletion — only shown when editing.
pub fn show_profile_editor(
    parent: &adw::ApplicationWindow,
    existing: Option<&ProfileConfig>,
    on_save: impl Fn(ProfileConfig) + 'static,
    on_delete: Option<impl Fn() + 'static>,
) {
    let dialog = adw::Dialog::builder()
        .title(if existing.is_some() {
            "Edit Profile"
        } else {
            "New Profile"
        })
        .content_width(450)
        .content_height(550)
        .build();

    let toolbar_view = adw::ToolbarView::new();

    let header = adw::HeaderBar::new();
    toolbar_view.add_top_bar(&header);

    let content = gtk4::Box::new(gtk4::Orientation::Vertical, 0);

    let prefs_page = adw::PreferencesPage::new();

    // Connection group
    let conn_group = adw::PreferencesGroup::builder()
        .title("Connection")
        .build();

    let name_row = adw::EntryRow::builder()
        .title("Profile Name")
        .tooltip_text("A friendly name to identify this connection")
        .build();
    let server_row = adw::EntryRow::builder()
        .title("Server Address")
        .tooltip_text("Hostname or IP address of the DrayTek router")
        .build();
    let port_row = adw::SpinRow::builder()
        .title("Port")
        .tooltip_text("SSL VPN port on the router (default: 443)")
        .adjustment(&gtk4::Adjustment::new(443.0, 1.0, 65535.0, 1.0, 10.0, 0.0))
        .build();
    let username_row = adw::EntryRow::builder()
        .title("Username")
        .tooltip_text("VPN account username configured on the router")
        .build();
    let password_row = adw::PasswordEntryRow::builder()
        .title("Password")
        .tooltip_text("VPN account password")
        .build();

    conn_group.add(&name_row);
    conn_group.add(&server_row);
    conn_group.add(&port_row);
    conn_group.add(&username_row);
    conn_group.add(&password_row);
    prefs_page.add(&conn_group);

    // Options group
    let opts_group = adw::PreferencesGroup::builder()
        .title("Options")
        .build();

    let self_signed_row = adw::SwitchRow::builder()
        .title("Accept Self-Signed Certificates")
        .tooltip_text("Allow connections to routers using self-signed TLS certificates")
        .active(true)
        .build();
    let route_remote_row = adw::SwitchRow::builder()
        .title("Route Remote Network")
        .tooltip_text("Automatically adds a route for the gateway's subnet when connected.\n\
             e.g. gateway 192.168.1.1 → auto-adds route 192.168.1.0/24,\n\
             so all 192.168.1.* traffic goes through the VPN.")
        .active(true)
        .build();
    let default_gw_row = adw::SwitchRow::builder()
        .title("Use as Default Gateway")
        .tooltip_text("Route all internet traffic through the VPN tunnel")
        .active(false)
        .build();
    let keepalive_row = adw::SwitchRow::builder()
        .title("Keepalive")
        .tooltip_text("Automatically send periodic pings when connected to prevent the router's idle timeout from dropping the tunnel")
        .active(false)
        .build();
    let auto_reconnect_row = adw::SwitchRow::builder()
        .title("Auto-Reconnect")
        .tooltip_text("Automatically reconnect if the tunnel drops unexpectedly")
        .active(false)
        .build();
    let mru_row = adw::SpinRow::builder()
        .title("MRU (0 = default 1280)")
        .tooltip_text("Maximum Receive Unit — largest packet size we accept. 0 uses the default (1280). The router may negotiate a different value.")
        .adjustment(&gtk4::Adjustment::new(0.0, 0.0, 9000.0, 1.0, 100.0, 0.0))
        .build();
    let routes_row = adw::EntryRow::builder()
        .title("Additional Routes (comma-separated CIDR)")
        .tooltip_text("Subnets to route through the VPN tunnel (CIDR notation).\n\
             e.g. 192.168.1.0/24 routes all 192.168.1.* traffic via VPN.\n\
             /24 = whole subnet (254 hosts), /32 = single host.\n\
             Without routes, no traffic flows through the tunnel.")
        .build();

    // When default gateway is on, routing options are redundant
    let update_route_sensitivity = {
        let route_remote_row = route_remote_row.clone();
        let routes_row = routes_row.clone();
        move |is_default_gw: bool| {
            route_remote_row.set_sensitive(!is_default_gw);
            routes_row.set_sensitive(!is_default_gw);
        }
    };
    update_route_sensitivity(default_gw_row.is_active());
    {
        let update = update_route_sensitivity.clone();
        default_gw_row.connect_active_notify(move |row| {
            update(row.is_active());
        });
    }

    opts_group.add(&route_remote_row);
    opts_group.add(&routes_row);
    opts_group.add(&default_gw_row);
    opts_group.add(&keepalive_row);
    opts_group.add(&auto_reconnect_row);
    opts_group.add(&self_signed_row);
    opts_group.add(&mru_row);
    prefs_page.add(&opts_group);

    // Pre-fill if editing
    if let Some(profile) = existing {
        name_row.set_text(&profile.name);
        server_row.set_text(&profile.server);
        port_row.set_value(profile.port as f64);
        username_row.set_text(&profile.username);
        let pw = config::retrieve_password(&profile.name)
            .unwrap_or_default();
        password_row.set_text(&pw);
        self_signed_row.set_active(profile.accept_self_signed);
        route_remote_row.set_active(profile.route_remote_network);
        default_gw_row.set_active(profile.default_gateway);
        keepalive_row.set_active(profile.keepalive);
        auto_reconnect_row.set_active(profile.auto_reconnect);
        mru_row.set_value(profile.mru as f64);
        routes_row.set_text(&profile.routes.join(", "));
    }

    // Save button
    let save_btn = gtk4::Button::builder()
        .label("Save")
        .css_classes(["suggested-action"])
        .build();

    let btn_box = gtk4::Box::new(gtk4::Orientation::Horizontal, 12);
    btn_box.set_halign(gtk4::Align::Fill);
    btn_box.set_homogeneous(true);
    btn_box.set_margin_top(24);
    btn_box.set_margin_bottom(24);
    btn_box.set_margin_start(24);
    btn_box.set_margin_end(24);
    // Delete button — only when editing an existing profile (left side)
    if on_delete.is_some() {
        let delete_btn = gtk4::Button::builder()
            .label("Delete")
            .css_classes(["destructive-action"])
            .build();

        let dialog_ref = dialog.clone();
        let on_delete = std::rc::Rc::new(std::cell::RefCell::new(Some(on_delete.unwrap())));
        let profile_name = existing
            .map(|p| p.name.clone())
            .unwrap_or_default();

        delete_btn.connect_clicked(move |btn| {
            let confirm = adw::AlertDialog::builder()
                .heading("Delete Profile?")
                .body(format!("Are you sure you want to delete \"{profile_name}\"? This cannot be undone."))
                .build();
            confirm.add_response("cancel", "Cancel");
            confirm.add_response("delete", "Delete");
            confirm.set_response_appearance("delete", adw::ResponseAppearance::Destructive);
            confirm.set_default_response(Some("cancel"));
            confirm.set_close_response("cancel");

            let dialog_ref = dialog_ref.clone();
            let on_delete = on_delete.clone();
            confirm.connect_response(None, move |_, response| {
                if response == "delete" {
                    if let Some(f) = on_delete.borrow_mut().take() {
                        f();
                    }
                    dialog_ref.close();
                }
            });
            confirm.present(Some(btn));
        });

        btn_box.append(&delete_btn);
    }

    // Save always on the right
    btn_box.append(&save_btn);

    content.append(&prefs_page);
    content.append(&btn_box);

    let scrolled = gtk4::ScrolledWindow::builder()
        .child(&content)
        .vexpand(true)
        .build();

    toolbar_view.set_content(Some(&scrolled));
    dialog.set_child(Some(&toolbar_view));

    let dialog_clone = dialog.clone();
    save_btn.connect_clicked(move |_| {
        let routes_text: String = routes_row.text().into();
        let routes: Vec<String> = routes_text
            .split(',')
            .map(|part: &str| part.trim().to_string())
            .filter(|part: &String| !part.is_empty())
            .collect();

        let profile = ProfileConfig {
            name: name_row.text().into(),
            server: server_row.text().into(),
            port: port_row.value() as u16,
            username: username_row.text().into(),
            password: password_row.text().into(),
            accept_self_signed: self_signed_row.is_active(),
            route_remote_network: route_remote_row.is_active(),
            default_gateway: default_gw_row.is_active(),
            keepalive: keepalive_row.is_active(),
            auto_reconnect: auto_reconnect_row.is_active(),
            mru: mru_row.value() as u16,
            routes,
        };
        on_save(profile);
        dialog_clone.close();
    });

    dialog.present(Some(parent));
}
