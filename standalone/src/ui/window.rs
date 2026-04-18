/// Main application window.
use crate::config::{self, AppConfig};
use crate::glib_channels::GlibSender;
use crate::logging::LogBuffer;
use crate::messages::{ConnectionProfile, TunnelCommand, TunnelStatus};
use crate::ui::connection_view::ConnectionView;
use crate::ui::log_view::LogView;
use crate::ui::profile_editor;
use gtk4::prelude::*;
use libadwaita as adw;
use libadwaita::prelude::*;
use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::{Arc, Mutex};
use tokio::sync::mpsc;
use tracing::{error, info};

pub struct MainWindow {
    pub window: adw::ApplicationWindow,
}

impl MainWindow {
    pub fn new(
        app: &adw::Application,
        log_buffer: LogBuffer,
        tokio_handle: tokio::runtime::Handle,
    ) -> Self {
        let window = adw::ApplicationWindow::builder()
            .application(app)
            .title("DrayTek SSL VPN")
            .default_width(500)
            .default_height(700)
            .width_request(500)
            .height_request(400)
            .build();

        // State
        let mut loaded_config = config::load_config().unwrap_or_else(|e| {
            error!("Failed to load config: {e:#}");
            AppConfig::default()
        });
        config::migrate_passwords(&mut loaded_config);
        let app_config = Rc::new(RefCell::new(loaded_config));
        let tunnel_cmd_tx: Rc<RefCell<Option<mpsc::UnboundedSender<TunnelCommand>>>> =
            Rc::new(RefCell::new(None));
        let profile_keepalive: Rc<Cell<bool>> = Rc::new(Cell::new(false));

        // Shared status queue: tunnel thread pushes, UI thread polls
        let status_queue: Arc<Mutex<Vec<TunnelStatus>>> = Arc::new(Mutex::new(Vec::new()));

        // Layout
        let toolbar_view = adw::ToolbarView::new();

        let header = adw::HeaderBar::new();
        toolbar_view.add_top_bar(&header);

        let content = gtk4::Box::new(gtk4::Orientation::Vertical, 0);

        // Profile selector
        let dropdown_box = gtk4::Box::new(gtk4::Orientation::Horizontal, 8);
        dropdown_box.set_margin_start(16);
        dropdown_box.set_margin_end(16);
        dropdown_box.set_margin_top(8);

        let profile_dropdown = gtk4::DropDown::builder().hexpand(true).build();

        let add_btn = gtk4::Button::builder()
            .icon_name("list-add-symbolic")
            .tooltip_text("Add Profile")
            .css_classes(["flat"])
            .build();
        let edit_btn = gtk4::Button::builder()
            .icon_name("document-edit-symbolic")
            .tooltip_text("Edit Profile")
            .css_classes(["flat"])
            .build();

        dropdown_box.append(&profile_dropdown);
        dropdown_box.append(&add_btn);
        dropdown_box.append(&edit_btn);

        content.append(&dropdown_box);

        // Connection view
        let connection_view = Rc::new(ConnectionView::new());
        content.append(&connection_view.container);

        // Separator
        let separator = gtk4::Separator::new(gtk4::Orientation::Horizontal);
        content.append(&separator);

        // Log view
        let log_view = Rc::new(LogView::new(log_buffer.clone()));
        content.append(&log_view.container);

        let scrolled = gtk4::ScrolledWindow::builder()
            .child(&content)
            .vexpand(true)
            .build();

        toolbar_view.set_content(Some(&scrolled));
        window.set_content(Some(&toolbar_view));

        // Populate dropdown
        let update_dropdown = {
            let config = app_config.clone();
            let dropdown = profile_dropdown.clone();
            move || {
                let cfg = config.borrow();
                let names: Vec<String> = cfg.profiles.iter().map(|p| p.name.clone()).collect();
                let model =
                    gtk4::StringList::new(&names.iter().map(|s| s.as_str()).collect::<Vec<_>>());
                dropdown.set_model(Some(&model));
                if let Some(idx) = cfg.last_selected {
                    if idx < cfg.profiles.len() {
                        dropdown.set_selected(idx as u32);
                    }
                }
            }
        };
        update_dropdown();

        // Add profile button
        {
            let window_clone = window.clone();
            let config = app_config.clone();
            let update_fn = update_dropdown.clone();
            add_btn.connect_clicked(move |_| {
                let config = config.clone();
                let update_fn = update_fn.clone();
                profile_editor::show_profile_editor(
                    &window_clone,
                    None,
                    move |profile| {
                        if let Err(e) = config::store_password(&profile.name, &profile.password) {
                            error!("Failed to store password in keyring: {e:#}");
                        }
                        let mut cfg = config.borrow_mut();
                        cfg.profiles.push(profile);
                        cfg.last_selected = Some(cfg.profiles.len() - 1);
                        if let Err(e) = config::save_config(&cfg) {
                            error!("Failed to save config: {e:#}");
                        }
                        drop(cfg);
                        update_fn();
                    },
                    None::<fn()>,
                );
            });
        }

        // Edit profile button
        {
            let window_clone = window.clone();
            let config = app_config.clone();
            let dropdown = profile_dropdown.clone();
            let update_fn = update_dropdown.clone();
            edit_btn.connect_clicked(move |_| {
                let idx = dropdown.selected() as usize;
                let cfg = config.borrow();
                if idx >= cfg.profiles.len() {
                    return;
                }
                let existing = cfg.profiles[idx].clone();
                let old_name = existing.name.clone();
                drop(cfg);

                let config = config.clone();
                let update_fn = update_fn.clone();
                let delete_config = config.clone();
                let delete_update_fn = update_fn.clone();
                let delete_name = old_name.clone();
                profile_editor::show_profile_editor(
                    &window_clone,
                    Some(&existing),
                    move |profile| {
                        // Handle rename: delete old keyring entry if name changed
                        if profile.name != old_name {
                            config::delete_password(&old_name);
                        }
                        if let Err(e) = config::store_password(&profile.name, &profile.password) {
                            error!("Failed to store password in keyring: {e:#}");
                        }
                        let mut cfg = config.borrow_mut();
                        if idx < cfg.profiles.len() {
                            cfg.profiles[idx] = profile;
                            if let Err(e) = config::save_config(&cfg) {
                                error!("Failed to save config: {e:#}");
                            }
                        }
                        drop(cfg);
                        update_fn();
                    },
                    Some(move || {
                        config::delete_password(&delete_name);
                        let mut cfg = delete_config.borrow_mut();
                        if idx < cfg.profiles.len() {
                            cfg.profiles.remove(idx);
                            cfg.last_selected = if cfg.profiles.is_empty() {
                                None
                            } else {
                                Some(0)
                            };
                            if let Err(e) = config::save_config(&cfg) {
                                error!("Failed to save config: {e:#}");
                            }
                            drop(cfg);
                            delete_update_fn();
                        } else {
                            drop(cfg);
                            delete_update_fn();
                        }
                    }),
                );
            });
        }

        // Connect button
        {
            let config = app_config.clone();
            let dropdown = profile_dropdown.clone();
            let tunnel_cmd_tx = tunnel_cmd_tx.clone();
            let tokio_handle = tokio_handle.clone();
            let status_queue = status_queue.clone();
            let profile_keepalive = profile_keepalive.clone();

            connection_view.connect_btn.connect_clicked(move |_| {
                let idx = dropdown.selected() as usize;
                let cfg = config.borrow();
                if idx >= cfg.profiles.len() {
                    error!("No profile selected");
                    return;
                }
                let profile_cfg = cfg.profiles[idx].clone();
                drop(cfg);

                let profile: ConnectionProfile = profile_cfg.into();
                profile_keepalive.set(profile.keepalive);
                info!("Connecting to {}", profile.server);

                // Create command channel
                let (cmd_tx, cmd_rx) = mpsc::unbounded_channel();
                *tunnel_cmd_tx.borrow_mut() = Some(cmd_tx);

                // Create status sender — pushes to thread-safe queue,
                // then pokes the GLib main loop to drain it.
                let queue = status_queue.clone();
                let status_tx = GlibSender::new(move |status: TunnelStatus| {
                    queue
                        .lock()
                        .expect("status queue lock poisoned")
                        .push(status);
                });

                // Spawn tunnel task
                tokio_handle.spawn(crate::tunnel::engine::run(profile, status_tx, cmd_rx));
            });
        }

        // Disconnect button
        {
            let tunnel_cmd_tx = tunnel_cmd_tx.clone();
            connection_view.disconnect_btn.connect_clicked(move |_| {
                if let Some(tx) = tunnel_cmd_tx.borrow().as_ref() {
                    if let Err(e) = tx.send(TunnelCommand::Disconnect) {
                        error!("Failed to send disconnect command: {e}");
                    }
                }
            });
        }

        // Keepalive toggle button
        {
            let tunnel_cmd_tx = tunnel_cmd_tx.clone();
            connection_view.keepalive_btn.connect_toggled(move |btn| {
                let enabled = btn.is_active();
                if enabled {
                    btn.set_label("Keepalive: ON");
                    btn.add_css_class("suggested-action");
                } else {
                    btn.set_label("Keepalive: OFF");
                    btn.remove_css_class("suggested-action");
                }
                if let Some(tx) = tunnel_cmd_tx.borrow().as_ref() {
                    if let Err(e) = tx.send(TunnelCommand::ToggleKeepalive(enabled)) {
                        error!("Failed to send keepalive toggle: {e}");
                    }
                }
            });
        }

        // Periodic UI update: drain status queue + refresh log (every 100ms)
        {
            let connection_view = connection_view.clone();
            let log_view = log_view.clone();
            let status_queue = status_queue.clone();
            let profile_keepalive = profile_keepalive.clone();
            gtk4::glib::timeout_add_local(std::time::Duration::from_millis(100), move || {
                // Drain status updates
                let statuses: Vec<TunnelStatus> = {
                    let mut q = status_queue.lock().expect("status queue lock poisoned");
                    q.drain(..).collect()
                };
                for status in &statuses {
                    connection_view.update_status(status);
                    // Auto-enable keepalive if profile setting is on
                    if matches!(status, TunnelStatus::Connected { .. }) && profile_keepalive.get() {
                        connection_view.keepalive_btn.set_active(true);
                    }
                }

                // Update connection timer
                connection_view.tick();

                // Refresh log view
                log_view.refresh();

                gtk4::glib::ControlFlow::Continue
            });
        }

        // Save selected profile on dropdown change
        {
            let config = app_config.clone();
            profile_dropdown.connect_selected_notify(move |dd| {
                // try_borrow_mut: skip save during programmatic updates (e.g. set_model)
                // where the config is already borrowed
                if let Ok(mut cfg) = config.try_borrow_mut() {
                    let idx = dd.selected() as usize;
                    cfg.last_selected = Some(idx);
                    let _ = config::save_config(&cfg);
                }
            });
        }

        // Check for stale tunnel device from a previous crashed session
        {
            use crate::tunnel::privilege;

            if privilege::is_device_present(privilege::TUN_DEVICE_NAME) {
                let restore_dns = privilege::has_dns_backup();
                info!(
                    "Stale tunnel detected: device {} present, DNS backup {}",
                    privilege::TUN_DEVICE_NAME,
                    if restore_dns { "found" } else { "not found" },
                );

                let dialog = adw::AlertDialog::builder()
                    .heading("Stale Tunnel Detected")
                    .body(
                        "A tunnel device (draytek0) from a previous session is still active. \
                         This may affect your network. Clean it up?",
                    )
                    .build();
                dialog.add_response("ignore", "Ignore");
                dialog.add_response("cleanup", "Clean Up");
                dialog.set_response_appearance("cleanup", adw::ResponseAppearance::Suggested);
                dialog.set_default_response(Some("cleanup"));
                dialog.set_close_response("ignore");

                let handle = tokio_handle.clone();
                dialog.connect_response(None, move |_, response| {
                    if response == "cleanup" {
                        info!("User chose to clean up stale tunnel");
                        let device = privilege::TUN_DEVICE_NAME.to_string();
                        handle.spawn(async move {
                            privilege::teardown(&device, restore_dns).await;
                        });
                    } else {
                        info!("User chose to ignore stale tunnel");
                    }
                });
                dialog.present(Some(&window));
            }
        }

        MainWindow { window }
    }
}
