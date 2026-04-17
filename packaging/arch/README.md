# Arch Package

Native `pacman` package for the NetworkManager plugin + tray.

## Build & install

```bash
cd packaging/arch
makepkg -si
```

`makepkg` compiles from the surrounding git checkout (no source tarball
download), so run it from this directory. `-s` pulls build dependencies,
`-i` installs the resulting `.pkg.tar.zst` via pacman.

## What it installs

| File | Path |
|------|------|
| VPN service daemon | `/usr/lib/NetworkManager/nm-draytek-service` |
| Editor plugin (base + GTK3 + GTK4) | `/usr/lib/NetworkManager/libnm-*-draytek*.so` |
| Auth dialog | `/usr/libexec/nm-draytek-auth-dialog` |
| NM service registration | `/usr/lib/NetworkManager/VPN/nm-draytek-service.name` |
| D-Bus system policy | `/usr/share/dbus-1/system.d/nm-draytek-service.conf` |
| Tray binary | `/usr/bin/draytek-vpn-tray` |
| NM dispatcher (auto-launches tray on connect) | `/etc/NetworkManager/dispatcher.d/90-draytek-vpn-tray` |

NetworkManager is restarted automatically on install/upgrade/remove via the
`.install` hook.

## Tray visibility on Wayland

The tray uses the StatusNotifierItem (SNI) D-Bus protocol. Niri, Sway, and
other minimal Wayland compositors have no built-in tray — you need a bar
that implements an SNI host:

- **waybar** — enable the `tray` module
- **yambar**, **i3status-rust**, etc.

Without an SNI host the VPN still works; you just won't see the indicator.

## Uninstall

```bash
sudo pacman -R draytek-vpn-networkmanager
```
