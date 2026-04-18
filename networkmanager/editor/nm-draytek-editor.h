/* SPDX-License-Identifier: GPL-2.0-or-later */
/*
 * DrayTek SSL VPN plugin for NetworkManager — shared header
 *
 * Defines vpn.data key constants (must match tunnel.rs parse_settings()),
 * service name, factory typedefs, and shared .so filenames.
 */

#ifndef NM_DRAYTEK_EDITOR_H
#define NM_DRAYTEK_EDITOR_H

#include <glib.h>
#include <NetworkManager.h>

/* D-Bus service name — must match nm-draytek-service.name [VPN Connection] */
#define NM_DRAYTEK_VPN_SERVICE "org.freedesktop.NetworkManager.draytek"

/* vpn.data key names — must match tunnel.rs parse_settings() exactly */
#define NM_DRAYTEK_KEY_GATEWAY              "gateway"
#define NM_DRAYTEK_KEY_PORT                 "port"
#define NM_DRAYTEK_KEY_USERNAME             "username"
#define NM_DRAYTEK_KEY_VERIFY_CERT          "verify-cert"
#define NM_DRAYTEK_KEY_MRU                  "mru"
#define NM_DRAYTEK_KEY_ROUTE_REMOTE_NETWORK "route-remote-network"
#define NM_DRAYTEK_KEY_NEVER_DEFAULT        "never-default"
#define NM_DRAYTEK_KEY_KEEPALIVE            "keepalive"
#define NM_DRAYTEK_KEY_ROUTES               "routes"

/* vpn.secrets key names */
#define NM_DRAYTEK_KEY_PASSWORD             "password"

/* Defaults */
#define NM_DRAYTEK_DEFAULT_PORT             443
#define NM_DRAYTEK_DEFAULT_MRU              0

/* Shared library filenames */
#define NM_DRAYTEK_PLUGIN_SO               "libnm-vpn-plugin-draytek.so"
#define NM_DRAYTEK_EDITOR_SO_GTK3          "libnm-vpn-plugin-draytek-editor.so"
#define NM_DRAYTEK_EDITOR_SO_GTK4          "libnm-gtk4-vpn-plugin-draytek-editor.so"

/* Factory function name exported by the editor .so */
#define NM_DRAYTEK_EDITOR_FACTORY_SYMBOL   "nm_vpn_editor_factory_draytek"

/* Factory function typedef */
typedef NMVpnEditor *(*NMDraytekEditorFactory)(NMVpnEditorPlugin *plugin,
                                                NMConnection      *connection,
                                                GError           **error);

#endif /* NM_DRAYTEK_EDITOR_H */
