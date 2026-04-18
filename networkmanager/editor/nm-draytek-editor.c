/* SPDX-License-Identifier: GPL-2.0-or-later */
/*
 * DrayTek SSL VPN — NMVpnEditor implementation (Component B)
 *
 * Compiles for GTK3 or GTK4 depending on USE_GTK4 define.
 * Exports nm_vpn_editor_factory_draytek() as the sole public symbol.
 */

#include "nm-draytek-editor.h"

#include <gtk/gtk.h>
#define GETTEXT_PACKAGE "draytek-vpn"
#include <glib/gi18n-lib.h>
#include <gmodule.h>
#include <string.h>
#include <stdlib.h>

/* ── GTK3/GTK4 compat macros ──────────────────────────────────── */

#ifdef USE_GTK4
  #define COMPAT_BOX_APPEND(box, child) gtk_box_append(GTK_BOX(box), child)
  #define COMPAT_FRAME_SET_CHILD(fr, c) gtk_frame_set_child(GTK_FRAME(fr), c)
  #define COMPAT_ENTRY_SET_TEXT(e, t)    gtk_editable_set_text(GTK_EDITABLE(e), t)
  #define COMPAT_ENTRY_GET_TEXT(e)       gtk_editable_get_text(GTK_EDITABLE(e))
#else
  #define COMPAT_BOX_APPEND(box, child) gtk_box_pack_start(GTK_BOX(box), child, TRUE, TRUE, 0)
  #define COMPAT_FRAME_SET_CHILD(fr, c) gtk_container_add(GTK_CONTAINER(fr), c)
  #define COMPAT_ENTRY_SET_TEXT(e, t)    gtk_entry_set_text(GTK_ENTRY(e), t)
  #define COMPAT_ENTRY_GET_TEXT(e)       gtk_entry_get_text(GTK_ENTRY(e))
#endif

/* ── GObject boilerplate ──────────────────────────────────────── */

#define DRAYTEK_TYPE_EDITOR            (draytek_editor_get_type())
#define DRAYTEK_EDITOR(obj)            (G_TYPE_CHECK_INSTANCE_CAST((obj), DRAYTEK_TYPE_EDITOR, DraytekEditor))
#define DRAYTEK_IS_EDITOR(obj)         (G_TYPE_CHECK_INSTANCE_TYPE((obj), DRAYTEK_TYPE_EDITOR))

typedef struct {
    GObject parent;

    GtkWidget *widget;

    /* Connection group */
    GtkWidget *gateway_entry;
    GtkWidget *port_spin;
    GtkWidget *username_entry;
    GtkWidget *password_entry;

    /* Options group */
    GtkWidget *route_remote_switch;
    GtkWidget *routes_entry;
    GtkWidget *default_gw_switch;
    GtkWidget *keepalive_switch;
    GtkWidget *self_signed_switch;
    GtkWidget *mru_spin;

    /* Rows greyed out when default gateway is on */
    GtkWidget *route_remote_row;
    GtkWidget *routes_row;
} DraytekEditor;

typedef struct {
    GObjectClass parent;
} DraytekEditorClass;

static void draytek_editor_interface_init(NMVpnEditorInterface *iface);

G_DEFINE_TYPE_WITH_CODE(DraytekEditor,
                        draytek_editor,
                        G_TYPE_OBJECT,
                        G_IMPLEMENT_INTERFACE(NM_TYPE_VPN_EDITOR,
                                              draytek_editor_interface_init))

/* ── Helpers ──────────────────────────────────────────────────── */

static gboolean
str_to_bool(const char *s, gboolean default_val)
{
    if (!s || !*s)
        return default_val;
    if (g_ascii_strcasecmp(s, "yes") == 0 || g_ascii_strcasecmp(s, "true") == 0 || g_strcmp0(s, "1") == 0)
        return TRUE;
    if (g_ascii_strcasecmp(s, "no") == 0 || g_ascii_strcasecmp(s, "false") == 0 || g_strcmp0(s, "0") == 0)
        return FALSE;
    return default_val;
}

static void
stuff_changed_cb(GtkWidget *widget, gpointer user_data)
{
    g_signal_emit_by_name(DRAYTEK_EDITOR(user_data), "changed");
}

static void
switch_changed_cb(GObject *gobject, GParamSpec *pspec, gpointer user_data)
{
    g_signal_emit_by_name(DRAYTEK_EDITOR(user_data), "changed");
}

static void
default_gw_toggled(GObject *gobject, GParamSpec *pspec, gpointer user_data)
{
    DraytekEditor *self = DRAYTEK_EDITOR(user_data);
    gboolean active = gtk_switch_get_active(GTK_SWITCH(self->default_gw_switch));

    gtk_widget_set_sensitive(self->route_remote_row, !active);
    gtk_widget_set_sensitive(self->routes_row, !active);

    g_signal_emit_by_name(self, "changed");
}

/* ── Grid row helper ──────────────────────────────────────────── */

static void
grid_attach_row(GtkGrid *grid, int row, const char *label_text,
                const char *tooltip, GtkWidget *widget)
{
    GtkWidget *label = gtk_label_new(label_text);
    gtk_label_set_xalign(GTK_LABEL(label), 1.0);
    gtk_widget_set_hexpand(label, FALSE);
#ifdef USE_GTK4
    gtk_widget_set_margin_end(label, 12);
#else
    g_object_set(label, "margin-end", 12, NULL);
#endif
    if (tooltip)
        gtk_widget_set_tooltip_text(label, tooltip);

    gtk_widget_set_hexpand(widget, TRUE);
    if (tooltip)
        gtk_widget_set_tooltip_text(widget, tooltip);

    gtk_grid_attach(grid, label, 0, row, 1, 1);
    gtk_grid_attach(grid, widget, 1, row, 1, 1);
}

/* ── Password entry compat ────────────────────────────────────── */

static GtkWidget *
create_password_entry(void)
{
#ifdef USE_GTK4
    GtkWidget *entry = gtk_password_entry_new();
    gtk_password_entry_set_show_peek_icon(GTK_PASSWORD_ENTRY(entry), TRUE);
    return entry;
#else
    GtkWidget *entry = gtk_entry_new();
    gtk_entry_set_visibility(GTK_ENTRY(entry), FALSE);
    gtk_entry_set_input_purpose(GTK_ENTRY(entry), GTK_INPUT_PURPOSE_PASSWORD);
    return entry;
#endif
}

static void
password_entry_set_text(GtkWidget *entry, const char *text)
{
#ifdef USE_GTK4
    gtk_editable_set_text(GTK_EDITABLE(entry), text);
#else
    gtk_entry_set_text(GTK_ENTRY(entry), text);
#endif
}

static const char *
password_entry_get_text(GtkWidget *entry)
{
#ifdef USE_GTK4
    return gtk_editable_get_text(GTK_EDITABLE(entry));
#else
    return gtk_entry_get_text(GTK_ENTRY(entry));
#endif
}

/* ── Widget construction ──────────────────────────────────────── */

static void
init_editor_widget(DraytekEditor *self, NMConnection *connection)
{
    NMSettingVpn *s_vpn = NULL;
    GtkWidget    *vbox;
    GtkWidget    *frame, *grid_widget;
    GtkGrid      *grid;
    const char   *val;
    int           row;

    if (connection)
        s_vpn = nm_connection_get_setting_vpn(connection);

    /* Top-level vertical box */
    vbox = gtk_box_new(GTK_ORIENTATION_VERTICAL, 18);
#ifdef USE_GTK4
    gtk_widget_set_margin_top(vbox, 12);
    gtk_widget_set_margin_bottom(vbox, 12);
    gtk_widget_set_margin_start(vbox, 12);
    gtk_widget_set_margin_end(vbox, 12);
#else
    g_object_set(vbox,
                 "margin-top", 12, "margin-bottom", 12,
                 "margin-start", 12, "margin-end", 12, NULL);
#endif
    self->widget = vbox;

    /* ---- Connection frame ---- */
    frame = gtk_frame_new("Connection");
    COMPAT_BOX_APPEND(vbox, frame);

    grid_widget = gtk_grid_new();
    grid = GTK_GRID(grid_widget);
    gtk_grid_set_row_spacing(grid, 8);
    gtk_grid_set_column_spacing(grid, 12);
#ifdef USE_GTK4
    gtk_widget_set_margin_top(grid_widget, 8);
    gtk_widget_set_margin_bottom(grid_widget, 8);
    gtk_widget_set_margin_start(grid_widget, 12);
    gtk_widget_set_margin_end(grid_widget, 12);
#else
    g_object_set(grid_widget,
                 "margin-top", 8, "margin-bottom", 8,
                 "margin-start", 12, "margin-end", 12, NULL);
#endif
    COMPAT_FRAME_SET_CHILD(frame, grid_widget);

    row = 0;

    /* Server Address */
    self->gateway_entry = gtk_entry_new();
    gtk_entry_set_placeholder_text(GTK_ENTRY(self->gateway_entry), "vpn.example.com");
    grid_attach_row(grid, row++, "Server Address",
                    "Hostname or IP address of the DrayTek router",
                    self->gateway_entry);

    /* Port */
    self->port_spin = gtk_spin_button_new_with_range(1, 65535, 1);
    gtk_spin_button_set_value(GTK_SPIN_BUTTON(self->port_spin), NM_DRAYTEK_DEFAULT_PORT);
    grid_attach_row(grid, row++, "Port",
                    "SSL VPN port on the router (default: 443)",
                    self->port_spin);

    /* Username */
    self->username_entry = gtk_entry_new();
    grid_attach_row(grid, row++, "Username",
                    "VPN account username configured on the router",
                    self->username_entry);

    /* Password */
    self->password_entry = create_password_entry();
    grid_attach_row(grid, row++, "Password",
                    "VPN account password.\n"
                    "Saved passwords are stored securely by NetworkManager\n"
                    "and will not be shown here when reopening the editor.",
                    self->password_entry);

    /* ---- Options frame ---- */
    frame = gtk_frame_new("Options");
    COMPAT_BOX_APPEND(vbox, frame);

    grid_widget = gtk_grid_new();
    grid = GTK_GRID(grid_widget);
    gtk_grid_set_row_spacing(grid, 8);
    gtk_grid_set_column_spacing(grid, 12);
#ifdef USE_GTK4
    gtk_widget_set_margin_top(grid_widget, 8);
    gtk_widget_set_margin_bottom(grid_widget, 8);
    gtk_widget_set_margin_start(grid_widget, 12);
    gtk_widget_set_margin_end(grid_widget, 12);
#else
    g_object_set(grid_widget,
                 "margin-top", 8, "margin-bottom", 8,
                 "margin-start", 12, "margin-end", 12, NULL);
#endif
    COMPAT_FRAME_SET_CHILD(frame, grid_widget);

    row = 0;

    /* Route Remote Network */
    self->route_remote_switch = gtk_switch_new();
    gtk_switch_set_active(GTK_SWITCH(self->route_remote_switch), TRUE);
    self->route_remote_row = gtk_box_new(GTK_ORIENTATION_HORIZONTAL, 0);
    COMPAT_BOX_APPEND(self->route_remote_row, self->route_remote_switch);
    grid_attach_row(grid, row++, "Route Remote Network",
                    "Automatically adds a route for the gateway's subnet when connected.\n"
                    "e.g. gateway 192.168.1.1 -> auto-adds route 192.168.1.0/24,\n"
                    "so all 192.168.1.* traffic goes through the VPN.",
                    self->route_remote_row);

    /* Additional Routes */
    self->routes_entry = gtk_entry_new();
    gtk_entry_set_placeholder_text(GTK_ENTRY(self->routes_entry), "192.168.1.0/24, 10.0.0.0/8");
    self->routes_row = gtk_box_new(GTK_ORIENTATION_HORIZONTAL, 0);
    gtk_widget_set_hexpand(self->routes_entry, TRUE);
    COMPAT_BOX_APPEND(self->routes_row, self->routes_entry);
    grid_attach_row(grid, row++, "Additional Routes",
                    "Subnets to route through the VPN tunnel (CIDR notation).\n"
                    "e.g. 192.168.1.0/24 routes all 192.168.1.* traffic via VPN.\n"
                    "/24 = whole subnet (254 hosts), /32 = single host.\n"
                    "Without routes, no traffic flows through the tunnel.",
                    self->routes_row);

    /* Use as Default Gateway */
    self->default_gw_switch = gtk_switch_new();
    gtk_switch_set_active(GTK_SWITCH(self->default_gw_switch), FALSE);
    {
        GtkWidget *box = gtk_box_new(GTK_ORIENTATION_HORIZONTAL, 0);
        COMPAT_BOX_APPEND(box, self->default_gw_switch);
        grid_attach_row(grid, row++, "Use as Default Gateway",
                        "Route all internet traffic through the VPN tunnel",
                        box);
    }

    /* Keepalive */
    self->keepalive_switch = gtk_switch_new();
    gtk_switch_set_active(GTK_SWITCH(self->keepalive_switch), FALSE);
    {
        GtkWidget *box = gtk_box_new(GTK_ORIENTATION_HORIZONTAL, 0);
        COMPAT_BOX_APPEND(box, self->keepalive_switch);
        grid_attach_row(grid, row++, "Keepalive",
                        "Automatically send periodic pings when connected to prevent\n"
                        "the router's idle timeout from dropping the tunnel",
                        box);
    }

    /* Accept Self-Signed Certificates */
    self->self_signed_switch = gtk_switch_new();
    gtk_switch_set_active(GTK_SWITCH(self->self_signed_switch), TRUE);
    {
        GtkWidget *box = gtk_box_new(GTK_ORIENTATION_HORIZONTAL, 0);
        COMPAT_BOX_APPEND(box, self->self_signed_switch);
        grid_attach_row(grid, row++, "Accept Self-Signed Certs",
                        "Allow connections to routers using self-signed TLS certificates",
                        box);
    }

    /* MRU */
    self->mru_spin = gtk_spin_button_new_with_range(0, 9000, 1);
    gtk_spin_button_set_value(GTK_SPIN_BUTTON(self->mru_spin), NM_DRAYTEK_DEFAULT_MRU);
    grid_attach_row(grid, row++, "MRU (0 = default 1280)",
                    "Maximum Receive Unit -- largest packet size we accept.\n"
                    "0 uses the default (1280). The router may negotiate a different value.",
                    self->mru_spin);

    /* ---- Populate from existing connection ---- */
    if (s_vpn) {
        val = nm_setting_vpn_get_data_item(s_vpn, NM_DRAYTEK_KEY_GATEWAY);
        if (val)
            COMPAT_ENTRY_SET_TEXT(self->gateway_entry, val);

        val = nm_setting_vpn_get_data_item(s_vpn, NM_DRAYTEK_KEY_PORT);
        if (val) {
            int port = atoi(val);
            if (port > 0 && port <= 65535)
                gtk_spin_button_set_value(GTK_SPIN_BUTTON(self->port_spin), port);
        }

        val = nm_setting_vpn_get_data_item(s_vpn, NM_DRAYTEK_KEY_USERNAME);
        if (val)
            COMPAT_ENTRY_SET_TEXT(self->username_entry, val);

        val = nm_setting_vpn_get_secret(s_vpn, NM_DRAYTEK_KEY_PASSWORD);
        if (val)
            password_entry_set_text(self->password_entry, val);

        val = nm_setting_vpn_get_data_item(s_vpn, NM_DRAYTEK_KEY_ROUTE_REMOTE_NETWORK);
        if (val) {
            gboolean active = (g_ascii_strcasecmp(val, "no") != 0);
            gtk_switch_set_active(GTK_SWITCH(self->route_remote_switch), active);
        }

        val = nm_setting_vpn_get_data_item(s_vpn, NM_DRAYTEK_KEY_ROUTES);
        if (val)
            COMPAT_ENTRY_SET_TEXT(self->routes_entry, val);

        val = nm_setting_vpn_get_data_item(s_vpn, NM_DRAYTEK_KEY_NEVER_DEFAULT);
        if (val) {
            gboolean default_gw = !str_to_bool(val, TRUE);
            gtk_switch_set_active(GTK_SWITCH(self->default_gw_switch), default_gw);
        }

        val = nm_setting_vpn_get_data_item(s_vpn, NM_DRAYTEK_KEY_KEEPALIVE);
        if (val)
            gtk_switch_set_active(GTK_SWITCH(self->keepalive_switch), str_to_bool(val, FALSE));

        val = nm_setting_vpn_get_data_item(s_vpn, NM_DRAYTEK_KEY_VERIFY_CERT);
        if (val) {
            gboolean accept_self_signed = !str_to_bool(val, FALSE);
            gtk_switch_set_active(GTK_SWITCH(self->self_signed_switch), accept_self_signed);
        }

        val = nm_setting_vpn_get_data_item(s_vpn, NM_DRAYTEK_KEY_MRU);
        if (val) {
            int mru = atoi(val);
            if (mru >= 0 && mru <= 9000)
                gtk_spin_button_set_value(GTK_SPIN_BUTTON(self->mru_spin), mru);
        }
    }

    /* ---- Apply initial sensitivity ---- */
    {
        gboolean gw_active = gtk_switch_get_active(GTK_SWITCH(self->default_gw_switch));
        gtk_widget_set_sensitive(self->route_remote_row, !gw_active);
        gtk_widget_set_sensitive(self->routes_row, !gw_active);
    }

    /* ---- Connect change signals ---- */
    g_signal_connect(self->gateway_entry,  "changed", G_CALLBACK(stuff_changed_cb), self);
    g_signal_connect(self->port_spin,      "value-changed", G_CALLBACK(stuff_changed_cb), self);
    g_signal_connect(self->username_entry,  "changed", G_CALLBACK(stuff_changed_cb), self);
    g_signal_connect(self->password_entry,  "changed", G_CALLBACK(stuff_changed_cb), self);
    g_signal_connect(self->routes_entry,    "changed", G_CALLBACK(stuff_changed_cb), self);
    g_signal_connect(self->mru_spin,        "value-changed", G_CALLBACK(stuff_changed_cb), self);

    g_signal_connect(self->route_remote_switch,   "notify::active", G_CALLBACK(switch_changed_cb), self);
    g_signal_connect(self->keepalive_switch,      "notify::active", G_CALLBACK(switch_changed_cb), self);
    g_signal_connect(self->self_signed_switch,    "notify::active", G_CALLBACK(switch_changed_cb), self);

    g_signal_connect(self->default_gw_switch, "notify::active", G_CALLBACK(default_gw_toggled), self);

    /* GTK3 needs explicit show */
#ifndef USE_GTK4
    gtk_widget_show_all(self->widget);
#endif
}

/* ── NMVpnEditor: get_widget ──────────────────────────────────── */

static GObject *
get_widget(NMVpnEditor *editor)
{
    DraytekEditor *self = DRAYTEK_EDITOR(editor);
    return G_OBJECT(self->widget);
}

/* ── NMVpnEditor: update_connection ───────────────────────────── */

static gboolean
update_connection(NMVpnEditor *editor, NMConnection *connection, GError **error)
{
    DraytekEditor *self = DRAYTEK_EDITOR(editor);
    NMSettingVpn  *s_vpn;
    const char    *gateway, *username, *password, *routes;
    char           buf[32];

    gateway = COMPAT_ENTRY_GET_TEXT(self->gateway_entry);
    if (!gateway || !*gateway) {
        g_set_error(error, NM_CONNECTION_ERROR,
                    NM_CONNECTION_ERROR_INVALID_PROPERTY,
                    "Server address is required");
        return FALSE;
    }

    username = COMPAT_ENTRY_GET_TEXT(self->username_entry);
    if (!username || !*username) {
        g_set_error(error, NM_CONNECTION_ERROR,
                    NM_CONNECTION_ERROR_INVALID_PROPERTY,
                    "Username is required");
        return FALSE;
    }

    s_vpn = nm_connection_get_setting_vpn(connection);
    if (s_vpn)
        nm_connection_remove_setting(connection, NM_TYPE_SETTING_VPN);

    s_vpn = (NMSettingVpn *)nm_setting_vpn_new();
    g_object_set(s_vpn, NM_SETTING_VPN_SERVICE_TYPE, NM_DRAYTEK_VPN_SERVICE, NULL);

    nm_setting_vpn_add_data_item(s_vpn, NM_DRAYTEK_KEY_GATEWAY, gateway);

    g_snprintf(buf, sizeof(buf), "%d",
               gtk_spin_button_get_value_as_int(GTK_SPIN_BUTTON(self->port_spin)));
    nm_setting_vpn_add_data_item(s_vpn, NM_DRAYTEK_KEY_PORT, buf);

    nm_setting_vpn_add_data_item(s_vpn, NM_DRAYTEK_KEY_USERNAME, username);

    password = password_entry_get_text(self->password_entry);
    if (password && *password)
        nm_setting_vpn_add_secret(s_vpn, NM_DRAYTEK_KEY_PASSWORD, password);

    /* Store password as a standard NM secret with flags=NONE (saved by NM).
     * This matches how StrongSwan, OpenVPN, etc. store VPN passwords. */
    nm_setting_set_secret_flags(NM_SETTING(s_vpn), NM_DRAYTEK_KEY_PASSWORD,
                                NM_SETTING_SECRET_FLAG_NONE, NULL);

    nm_setting_vpn_add_data_item(s_vpn, NM_DRAYTEK_KEY_ROUTE_REMOTE_NETWORK,
        gtk_switch_get_active(GTK_SWITCH(self->route_remote_switch)) ? "yes" : "no");

    routes = COMPAT_ENTRY_GET_TEXT(self->routes_entry);
    if (routes && *routes)
        nm_setting_vpn_add_data_item(s_vpn, NM_DRAYTEK_KEY_ROUTES, routes);

    nm_setting_vpn_add_data_item(s_vpn, NM_DRAYTEK_KEY_NEVER_DEFAULT,
        gtk_switch_get_active(GTK_SWITCH(self->default_gw_switch)) ? "no" : "yes");

    nm_setting_vpn_add_data_item(s_vpn, NM_DRAYTEK_KEY_KEEPALIVE,
        gtk_switch_get_active(GTK_SWITCH(self->keepalive_switch)) ? "yes" : "no");

    nm_setting_vpn_add_data_item(s_vpn, NM_DRAYTEK_KEY_VERIFY_CERT,
        gtk_switch_get_active(GTK_SWITCH(self->self_signed_switch)) ? "no" : "yes");

    g_snprintf(buf, sizeof(buf), "%d",
               gtk_spin_button_get_value_as_int(GTK_SPIN_BUTTON(self->mru_spin)));
    nm_setting_vpn_add_data_item(s_vpn, NM_DRAYTEK_KEY_MRU, buf);

    nm_connection_add_setting(connection, NM_SETTING(s_vpn));

    return TRUE;
}

/* ── GObject lifecycle ────────────────────────────────────────── */

static void
dispose(GObject *object)
{
    DraytekEditor *self = DRAYTEK_EDITOR(object);

    /* NM owns the widget — just drop our reference */
    self->widget = NULL;

    G_OBJECT_CLASS(draytek_editor_parent_class)->dispose(object);
}

static void
draytek_editor_interface_init(NMVpnEditorInterface *iface)
{
    iface->get_widget        = get_widget;
    iface->update_connection = update_connection;
}

static void
draytek_editor_class_init(DraytekEditorClass *klass)
{
    GObjectClass *object_class = G_OBJECT_CLASS(klass);
    object_class->dispose = dispose;
}

static void
draytek_editor_init(DraytekEditor *self)
{
}

/* ── Public factory ───────────────────────────────────────────── */

G_MODULE_EXPORT NMVpnEditor *
nm_vpn_editor_factory_draytek(NMVpnEditorPlugin *plugin,
                               NMConnection      *connection,
                               gpointer           user_data,
                               GError           **error)
{
    DraytekEditor *self;

    self = g_object_new(DRAYTEK_TYPE_EDITOR, NULL);
    init_editor_widget(self, connection);

    return NM_VPN_EDITOR(self);
}
