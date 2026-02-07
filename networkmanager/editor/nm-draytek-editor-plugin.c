/* SPDX-License-Identifier: GPL-2.0-or-later */
/*
 * DrayTek SSL VPN — NMVpnEditorPlugin implementation (Component A)
 *
 * Pure GLib + libnm, no GTK dependency.
 * Exports nm_vpn_editor_plugin_factory() as the sole public symbol.
 * Detects GTK4 at runtime and dlopen-loads the editor .so.
 */

#include "nm-draytek-editor.h"
#include "nm-vpn-plugin-utils.h"

#define GETTEXT_PACKAGE "draytek-vpn"
#include <glib/gi18n-lib.h>
#include <gmodule.h>

/* ------------------------------------------------------------------ */
/* GObject boilerplate                                                */
/* ------------------------------------------------------------------ */

#define DRAYTEK_TYPE_EDITOR_PLUGIN            (draytek_editor_plugin_get_type())
#define DRAYTEK_EDITOR_PLUGIN(obj)            (G_TYPE_CHECK_INSTANCE_CAST((obj), DRAYTEK_TYPE_EDITOR_PLUGIN, DraytekEditorPlugin))
#define DRAYTEK_EDITOR_PLUGIN_CLASS(klass)    (G_TYPE_CHECK_CLASS_CAST((klass), DRAYTEK_TYPE_EDITOR_PLUGIN, DraytekEditorPluginClass))
#define DRAYTEK_IS_EDITOR_PLUGIN(obj)         (G_TYPE_CHECK_INSTANCE_TYPE((obj), DRAYTEK_TYPE_EDITOR_PLUGIN))

typedef struct {
    GObject parent;
} DraytekEditorPlugin;

typedef struct {
    GObjectClass parent;
} DraytekEditorPluginClass;

static void draytek_editor_plugin_interface_init(NMVpnEditorPluginInterface *iface);

G_DEFINE_TYPE_WITH_CODE(DraytekEditorPlugin,
                        draytek_editor_plugin,
                        G_TYPE_OBJECT,
                        G_IMPLEMENT_INTERFACE(NM_TYPE_VPN_EDITOR_PLUGIN,
                                              draytek_editor_plugin_interface_init))

/* ------------------------------------------------------------------ */
/* NMVpnEditorPlugin interface                                        */
/* ------------------------------------------------------------------ */

static NMVpnEditor *
get_editor(NMVpnEditorPlugin *plugin, NMConnection *connection, GError **error)
{
    /*
     * Detect GTK3 vs GTK4 at runtime.
     * If gtk_container_add is resolvable, the host process loaded GTK3 —
     * use the GTK3 editor .so.  Otherwise use GTK4.
     * This is the same pattern used by all NM VPN plugins (openvpn, pptp, etc).
     */
    const char *editor_so;
    GModule    *self_module;
    gpointer    sym;

    self_module = g_module_open(NULL, G_MODULE_BIND_LAZY);
    if (self_module && g_module_symbol(self_module, "gtk_container_add", &sym))
        editor_so = NM_DRAYTEK_EDITOR_SO_GTK3;
    else
        editor_so = NM_DRAYTEK_EDITOR_SO_GTK4;

    return nm_vpn_plugin_utils_load_editor(editor_so,
                                           NM_DRAYTEK_EDITOR_FACTORY_SYMBOL,
                                           plugin,
                                           connection,
                                           NULL,
                                           error);
}

static NMVpnEditorPluginCapability
get_capabilities(NMVpnEditorPlugin *plugin)
{
    return NM_VPN_EDITOR_PLUGIN_CAPABILITY_NONE;
}

/* ------------------------------------------------------------------ */
/* GObject properties                                                 */
/* ------------------------------------------------------------------ */

enum {
    PROP_0,
    PROP_NAME,
    PROP_DESC,
    PROP_SERVICE,
    PROP_LAST
};

static void
get_property(GObject *object, guint prop_id, GValue *value, GParamSpec *pspec)
{
    switch (prop_id) {
    case PROP_NAME:
        g_value_set_string(value, "DrayTek SSL VPN");
        break;
    case PROP_DESC:
        g_value_set_string(value, "SSL VPN client for DrayTek routers");
        break;
    case PROP_SERVICE:
        g_value_set_string(value, NM_DRAYTEK_VPN_SERVICE);
        break;
    default:
        G_OBJECT_WARN_INVALID_PROPERTY_ID(object, prop_id, pspec);
        break;
    }
}

/* ------------------------------------------------------------------ */
/* Interface + class init                                             */
/* ------------------------------------------------------------------ */

static void
draytek_editor_plugin_interface_init(NMVpnEditorPluginInterface *iface)
{
    iface->get_editor       = get_editor;
    iface->get_capabilities = get_capabilities;
}

static void
draytek_editor_plugin_class_init(DraytekEditorPluginClass *klass)
{
    GObjectClass *object_class = G_OBJECT_CLASS(klass);

    object_class->get_property = get_property;

    g_object_class_override_property(object_class, PROP_NAME, NM_VPN_EDITOR_PLUGIN_NAME);
    g_object_class_override_property(object_class, PROP_DESC, NM_VPN_EDITOR_PLUGIN_DESCRIPTION);
    g_object_class_override_property(object_class, PROP_SERVICE, NM_VPN_EDITOR_PLUGIN_SERVICE);
}

static void
draytek_editor_plugin_init(DraytekEditorPlugin *self)
{
}

/* ------------------------------------------------------------------ */
/* Public factory — sole exported symbol                              */
/* ------------------------------------------------------------------ */

G_MODULE_EXPORT NMVpnEditorPlugin *
nm_vpn_editor_plugin_factory(GError **error)
{
    g_return_val_if_fail(error == NULL || *error == NULL, NULL);

    bindtextdomain("draytek-vpn", NULL);
    bind_textdomain_codeset("draytek-vpn", "UTF-8");

    return g_object_new(DRAYTEK_TYPE_EDITOR_PLUGIN, NULL);
}
