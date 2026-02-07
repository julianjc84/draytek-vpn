/* SPDX-License-Identifier: LGPL-2.1-or-later */
/*
 * Vendored from NetworkManager source (nm-vpn-plugin-utils.c).
 * Modified: replaced nm-default.h with direct glib/libnm includes.
 *
 * Loads a VPN editor .so via dlopen and calls its factory function.
 */

#include "nm-vpn-plugin-utils.h"

#include <dlfcn.h>
#include <gmodule.h>

typedef NMVpnEditor *(*NMVpnEditorFactory)(NMVpnEditorPlugin *plugin,
                                            NMConnection      *connection,
                                            gpointer           user_data,
                                            GError           **error);

NMVpnEditor *
nm_vpn_plugin_utils_load_editor(const char        *module_name,
                                const char        *factory_name,
                                NMVpnEditorPlugin *editor_plugin,
                                NMConnection      *connection,
                                gpointer           user_data,
                                GError           **error)
{
    static struct {
        gpointer   dl_module;
        NMVpnEditorFactory factory;
    } cache = { NULL, NULL };

    NMVpnEditorFactory factory;
    NMVpnEditor       *editor;
    gpointer           dl_module;

    g_return_val_if_fail(module_name != NULL, NULL);
    g_return_val_if_fail(factory_name != NULL, NULL);
    g_return_val_if_fail(NM_IS_VPN_EDITOR_PLUGIN(editor_plugin), NULL);
    g_return_val_if_fail(NM_IS_CONNECTION(connection), NULL);
    g_return_val_if_fail(error == NULL || *error == NULL, NULL);

    if (cache.factory) {
        editor = cache.factory(editor_plugin, connection, user_data, error);
        if (!editor) {
            if (error && !*error) {
                g_set_error_literal(error,
                                    NM_CONNECTION_ERROR,
                                    NM_CONNECTION_ERROR_INVALID_PROPERTY,
                                    "Cannot create editor: unknown error");
            }
        }
        return editor;
    }

    dl_module = dlopen(module_name, RTLD_LAZY | RTLD_LOCAL);
    if (!dl_module) {
        /* Try in the NM lib directory */
        g_autofree char *module_path = NULL;
        const char *libdir;

        libdir = getenv("NM_VPN_PLUGIN_DIR");
        if (!libdir)
            libdir = NM_VPN_PLUGIN_DIR;

        module_path = g_build_filename(libdir, module_name, NULL);
        dl_module = dlopen(module_path, RTLD_LAZY | RTLD_LOCAL);

        if (!dl_module) {
            g_set_error(error,
                        NM_CONNECTION_ERROR,
                        NM_CONNECTION_ERROR_INVALID_PROPERTY,
                        "Cannot load editor plugin '%s': %s",
                        module_path, dlerror());
            return NULL;
        }
    }

    factory = dlsym(dl_module, factory_name);
    if (!factory) {
        g_set_error(error,
                    NM_CONNECTION_ERROR,
                    NM_CONNECTION_ERROR_INVALID_PROPERTY,
                    "Cannot find symbol '%s' in '%s': %s",
                    factory_name, module_name, dlerror());
        dlclose(dl_module);
        return NULL;
    }

    editor = factory(editor_plugin, connection, user_data, error);
    if (!editor) {
        if (error && !*error) {
            g_set_error_literal(error,
                                NM_CONNECTION_ERROR,
                                NM_CONNECTION_ERROR_INVALID_PROPERTY,
                                "Cannot create editor: unknown error");
        }
        dlclose(dl_module);
        return NULL;
    }

    /* Cache on success — the .so stays loaded for the process lifetime */
    cache.dl_module = dl_module;
    cache.factory   = factory;

    return editor;
}
