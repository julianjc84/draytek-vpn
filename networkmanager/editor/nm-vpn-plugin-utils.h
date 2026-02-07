/* SPDX-License-Identifier: LGPL-2.1-or-later */
/*
 * Vendored from NetworkManager source (nm-vpn-plugin-utils.h).
 * Provides nm_vpn_plugin_utils_load_editor() for dlopen-loading
 * the GTK editor .so at runtime.
 */

#ifndef NM_VPN_PLUGIN_UTILS_H
#define NM_VPN_PLUGIN_UTILS_H

#include <glib.h>
#include <NetworkManager.h>

NMVpnEditor *nm_vpn_plugin_utils_load_editor(const char  *module_name,
                                              const char  *factory_name,
                                              NMVpnEditorPlugin *editor_plugin,
                                              NMConnection      *connection,
                                              gpointer           user_data,
                                              GError           **error);

#endif /* NM_VPN_PLUGIN_UTILS_H */
