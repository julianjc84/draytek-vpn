# DrayTek SSL VPN — NetworkManager Plugin

## Architecture

Three components, all installed to `/usr/lib/NetworkManager/`:

| Component | Language | File | Purpose |
|-----------|----------|------|---------|
| VPN Service | Rust | `nm-draytek-service` | D-Bus daemon that runs the tunnel (`org.freedesktop.NetworkManager.VPN.Plugin`) |
| Editor Plugin | C | `libnm-vpn-plugin-draytek.so` + GTK3/GTK4 `.so` | Settings UI for `nm-connection-editor` / GNOME Settings |
| Auth Dialog | C (GTK4) | `nm-draytek-auth-dialog` | Prompts for password when NM needs secrets |

The editor plugin ships as three `.so` files: a base plugin (no GTK), a GTK3 editor, and a GTK4 editor. The base plugin detects the host toolkit at runtime via `g_module_symbol()` looking for `gtk_container_add` — if found, loads GTK3; otherwise GTK4. This prevents crashes when GTK3 apps (like Cinnamon's `nm-connection-editor`) load the plugin.

## Password Storage

Passwords are stored as standard NM secrets (`vpn.secrets`) with `password-flags=0` (saved by NM). This matches StrongSwan, OpenVPN, and other NM VPN plugins.

**Implications:**
- `nmcli connection show "NAME"` does not show the password (use `-s` flag)
- NM handles the secret lifecycle (storage, retrieval, agent integration)
- When reopening the editor, the password field will be empty — NM populates secrets via `GetSecrets` / auth-dialog, which returns a keyfile description in `--external-ui-mode`, not the actual value
- On netplan systems (Ubuntu Server), `vpn.secrets` may not persist across reboots — the user re-enters the password on first connect after reboot (standard behavior for many VPN setups)

## The `--external-ui-mode` Gotcha

Cinnamon's secret agent (and some others) call the auth-dialog with `--external-ui-mode` but do **not** send data on stdin. If the auth-dialog calls `fgets(stdin)` before checking this flag, it blocks for ~25 seconds until NM's D-Bus timeout kills it — causing the editor to appear frozen.

**Fix:** Check `--external-ui-mode` *before* `read_vpn_details()` and return the GKeyFile description immediately.

## Build & Install

```bash
# Build and install all NM components
./build.sh nm install

# Build only (no install)
./build.sh nm release

# Uninstall
./build.sh nm uninstall
```

### Build Dependencies

```bash
sudo apt install libnm-dev libgtk-4-dev libgtk-3-dev pkg-config gcc
```

## Testing

1. Delete old VPN profiles, create a new one with password
2. Verify storage: `nmcli connection show "NAME" | grep vpn.data` — no `password` in data, has `password-flags = 0`
3. Open editor (gear icon) — should open fast (< 1 second), password field empty (expected)
4. Connect VPN — NM provides password via secrets to the service
