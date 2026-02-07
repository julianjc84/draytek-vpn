/* SPDX-License-Identifier: GPL-2.0-or-later */
/*
 * DrayTek SSL VPN — NetworkManager auth-dialog (Component C)
 *
 * Prompts for VPN password when NM connects.
 * Reads vpn data/secrets from stdin (NM protocol), shows GTK4 password
 * dialog if needed, writes secrets to stdout.
 *
 * Supports --external-ui-mode for GNOME Shell integration.
 */

#include <gtk/gtk.h>
#include <glib.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

/* Key names — same as nm-draytek-editor.h */
#define NM_DRAYTEK_KEY_PASSWORD  "password"
#define NM_DRAYTEK_KEY_GATEWAY   "gateway"
#define NM_DRAYTEK_VPN_SERVICE   "org.freedesktop.NetworkManager.draytek"

/* ------------------------------------------------------------------ */
/* Read NM vpn details from stdin                                     */
/* ------------------------------------------------------------------ */

/*
 * NM sends vpn data and secrets on stdin in this format:
 *   DATA_KEY=key1\n DATA_VAL=val1\n ... DONE\n QUIT\n
 *   SECRET_KEY=key1\n SECRET_VAL=val1\n ... DONE\n QUIT\n
 * We parse these into two hash tables.
 */
static void
read_vpn_details(GHashTable **out_data, GHashTable **out_secrets)
{
    GHashTable *data    = g_hash_table_new_full(g_str_hash, g_str_equal, g_free, g_free);
    GHashTable *secrets = g_hash_table_new_full(g_str_hash, g_str_equal, g_free, g_free);
    char        line[4096];
    GHashTable *current = data;
    char       *pending_key = NULL;

    while (fgets(line, sizeof(line), stdin)) {
        /* Strip trailing newline */
        size_t len = strlen(line);
        if (len > 0 && line[len - 1] == '\n')
            line[--len] = '\0';

        if (strcmp(line, "DONE") == 0) {
            /* Switch from data to secrets, or we're done */
            g_free(pending_key);
            pending_key = NULL;
            if (current == data)
                current = secrets;
            else
                break;
        } else if (strcmp(line, "QUIT") == 0) {
            break;
        } else if (g_str_has_prefix(line, "DATA_KEY=")) {
            g_free(pending_key);
            pending_key = g_strdup(line + 9);
        } else if (g_str_has_prefix(line, "DATA_VAL=") && pending_key) {
            g_hash_table_insert(data, pending_key, g_strdup(line + 9));
            pending_key = NULL;
        } else if (g_str_has_prefix(line, "SECRET_KEY=")) {
            g_free(pending_key);
            pending_key = g_strdup(line + 11);
        } else if (g_str_has_prefix(line, "SECRET_VAL=") && pending_key) {
            g_hash_table_insert(secrets, pending_key, g_strdup(line + 11));
            pending_key = NULL;
        }
    }

    g_free(pending_key);
    *out_data    = data;
    *out_secrets = secrets;
}

/* ------------------------------------------------------------------ */
/* Write secrets to stdout (NM protocol)                              */
/* ------------------------------------------------------------------ */

static void
write_secret(const char *key, const char *value)
{
    printf("%s\n%s\n", key, value);
}

static void
write_done(void)
{
    printf("\n\n");
    fflush(stdout);
}

/* ------------------------------------------------------------------ */
/* External UI mode — output GKeyFile describing the prompt           */
/* ------------------------------------------------------------------ */

static void
do_external_ui(const char *vpn_name, const char *gateway)
{
    GKeyFile *keyfile = g_key_file_new();
    g_autofree char *title = NULL;
    g_autofree char *msg   = NULL;

    title = g_strdup_printf("Authenticate VPN %s", vpn_name ? vpn_name : "");
    msg   = g_strdup_printf("Password required to connect to DrayTek VPN '%s' (%s)",
                            vpn_name ? vpn_name : "",
                            gateway  ? gateway  : "");

    g_key_file_set_string(keyfile, "VPN Plugin UI", "Version", "2");
    g_key_file_set_string(keyfile, "VPN Plugin UI", "Description", title);

    g_key_file_set_string(keyfile, NM_DRAYTEK_KEY_PASSWORD, "Value", "");
    g_key_file_set_string(keyfile, NM_DRAYTEK_KEY_PASSWORD, "Label", "Password:");
    g_key_file_set_boolean(keyfile, NM_DRAYTEK_KEY_PASSWORD, "IsSecret", TRUE);
    g_key_file_set_boolean(keyfile, NM_DRAYTEK_KEY_PASSWORD, "ShouldAsk", TRUE);

    g_autofree char *data = g_key_file_to_data(keyfile, NULL, NULL);
    fputs(data, stdout);
    fflush(stdout);

    g_key_file_unref(keyfile);
}

/* ------------------------------------------------------------------ */
/* GTK4 password dialog                                               */
/* ------------------------------------------------------------------ */

typedef struct {
    GtkWidget  *window;
    GtkWidget  *entry;
    char       *result;
    GMainLoop  *loop;
} DialogData;

static void
on_ok_clicked(GtkButton *button, gpointer user_data)
{
    DialogData *dd = user_data;
    dd->result = g_strdup(gtk_editable_get_text(GTK_EDITABLE(dd->entry)));
    g_main_loop_quit(dd->loop);
}

static void
on_cancel_clicked(GtkButton *button, gpointer user_data)
{
    DialogData *dd = user_data;
    dd->result = NULL;
    g_main_loop_quit(dd->loop);
}

static void
on_window_close(GtkWindow *window, gpointer user_data)
{
    DialogData *dd = user_data;
    if (!dd->result)
        dd->result = NULL;
    g_main_loop_quit(dd->loop);
}

static void
on_entry_activate(GtkEntry *entry, gpointer user_data)
{
    DialogData *dd = user_data;
    dd->result = g_strdup(gtk_editable_get_text(GTK_EDITABLE(dd->entry)));
    g_main_loop_quit(dd->loop);
}

static char *
show_password_dialog(const char *vpn_name, const char *gateway)
{
    DialogData dd = { .result = NULL };

    dd.loop = g_main_loop_new(NULL, FALSE);

    dd.window = gtk_window_new();
    gtk_window_set_title(GTK_WINDOW(dd.window), "VPN Authentication");
    gtk_window_set_default_size(GTK_WINDOW(dd.window), 350, -1);
    gtk_window_set_resizable(GTK_WINDOW(dd.window), FALSE);

    GtkWidget *vbox = gtk_box_new(GTK_ORIENTATION_VERTICAL, 12);
    gtk_widget_set_margin_top(vbox, 18);
    gtk_widget_set_margin_bottom(vbox, 18);
    gtk_widget_set_margin_start(vbox, 18);
    gtk_widget_set_margin_end(vbox, 18);
    gtk_window_set_child(GTK_WINDOW(dd.window), vbox);

    /* Info label */
    g_autofree char *msg = g_strdup_printf(
        "Password required for DrayTek VPN\n<b>%s</b> (%s)",
        vpn_name ? vpn_name : "", gateway ? gateway : "");
    GtkWidget *label = gtk_label_new(NULL);
    gtk_label_set_markup(GTK_LABEL(label), msg);
    gtk_label_set_wrap(GTK_LABEL(label), TRUE);
    gtk_box_append(GTK_BOX(vbox), label);

    /* Password entry */
    dd.entry = gtk_password_entry_new();
    gtk_password_entry_set_show_peek_icon(GTK_PASSWORD_ENTRY(dd.entry), TRUE);
    gtk_box_append(GTK_BOX(vbox), dd.entry);
    g_signal_connect(dd.entry, "activate", G_CALLBACK(on_entry_activate), &dd);

    /* Buttons */
    GtkWidget *hbox = gtk_box_new(GTK_ORIENTATION_HORIZONTAL, 8);
    gtk_widget_set_halign(hbox, GTK_ALIGN_END);
    gtk_box_append(GTK_BOX(vbox), hbox);

    GtkWidget *cancel_btn = gtk_button_new_with_label("Cancel");
    g_signal_connect(cancel_btn, "clicked", G_CALLBACK(on_cancel_clicked), &dd);
    gtk_box_append(GTK_BOX(hbox), cancel_btn);

    GtkWidget *ok_btn = gtk_button_new_with_label("Connect");
    gtk_widget_add_css_class(ok_btn, "suggested-action");
    g_signal_connect(ok_btn, "clicked", G_CALLBACK(on_ok_clicked), &dd);
    gtk_box_append(GTK_BOX(hbox), ok_btn);

    g_signal_connect(dd.window, "close-request", G_CALLBACK(on_window_close), &dd);

    gtk_window_present(GTK_WINDOW(dd.window));
    g_main_loop_run(dd.loop);

    gtk_window_destroy(GTK_WINDOW(dd.window));
    g_main_loop_unref(dd.loop);

    return dd.result;
}

/* ------------------------------------------------------------------ */
/* GTK4 application activate (interactive path only)                   */
/* ------------------------------------------------------------------ */

typedef struct {
    const char *vpn_name;
    const char *gateway;
} ActivateData;

static void
on_activate(GtkApplication *app, gpointer user_data)
{
    ActivateData *ad = user_data;

    char *password = show_password_dialog(ad->vpn_name, ad->gateway);
    if (password && *password) {
        write_secret(NM_DRAYTEK_KEY_PASSWORD, password);
    }
    write_done();

    g_free(password);
}

/* ------------------------------------------------------------------ */
/* main                                                               */
/* ------------------------------------------------------------------ */

int
main(int argc, char *argv[])
{
    const char *uuid    = NULL;
    const char *name    = NULL;
    const char *service = NULL;
    gboolean    reprompt         = FALSE;
    gboolean    allow_interaction = FALSE;
    gboolean    external_ui_mode = FALSE;
    int         i;

    /* Parse NM auth-dialog arguments */
    for (i = 1; i < argc; i++) {
        if (g_strcmp0(argv[i], "--uuid") == 0 && i + 1 < argc)
            uuid = argv[++i];
        else if (g_strcmp0(argv[i], "--name") == 0 && i + 1 < argc)
            name = argv[++i];
        else if (g_strcmp0(argv[i], "--service") == 0 && i + 1 < argc)
            service = argv[++i];
        else if (g_strcmp0(argv[i], "-r") == 0)
            reprompt = TRUE;
        else if (g_strcmp0(argv[i], "-i") == 0)
            allow_interaction = TRUE;
        else if (g_strcmp0(argv[i], "--external-ui-mode") == 0)
            external_ui_mode = TRUE;
    }

    (void)uuid;
    (void)service;

    /* ---- External UI mode: respond immediately, no stdin needed ---- */
    /* Cinnamon's agent calls with --external-ui-mode but does NOT send
     * data on stdin, so read_vpn_details() would block until D-Bus
     * timeout (~25s).  Handle this before touching stdin. */
    if (external_ui_mode) {
        do_external_ui(name, NULL);
        return 0;
    }

    /* ---- Read NM data from stdin (only needs glib, not GTK) ---- */
    GHashTable *data    = NULL;
    GHashTable *secrets = NULL;
    read_vpn_details(&data, &secrets);

    const char *gateway     = g_hash_table_lookup(data, NM_DRAYTEK_KEY_GATEWAY);
    const char *existing_pw = g_hash_table_lookup(secrets, NM_DRAYTEK_KEY_PASSWORD);

    /* Already have a password and not asked to reprompt — return it */
    if (existing_pw && *existing_pw && !reprompt) {
        write_secret(NM_DRAYTEK_KEY_PASSWORD, existing_pw);
        write_done();
        g_hash_table_unref(data);
        g_hash_table_unref(secrets);
        return 0;
    }

    /* No interaction allowed and no password — signal failure (exit 1).
     * Matching StrongSwan behavior; avoids Cinnamon agent crash on empty result. */
    if (!allow_interaction) {
        g_hash_table_unref(data);
        g_hash_table_unref(secrets);
        return 1;
    }

    /* ---- Interactive path: need GTK4 for the password dialog ---- */
    ActivateData ad = { .vpn_name = name, .gateway = gateway };

    GtkApplication *app = gtk_application_new(
        "com.draytek.vpn.auth-dialog", G_APPLICATION_NON_UNIQUE);
    g_signal_connect(app, "activate", G_CALLBACK(on_activate), &ad);

    int status = g_application_run(G_APPLICATION(app), 0, NULL);

    g_object_unref(app);
    g_hash_table_unref(data);
    g_hash_table_unref(secrets);

    return status;
}
