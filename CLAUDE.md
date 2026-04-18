# DrayTek SSL VPN Client for Linux

Native Linux SSL VPN client for DrayTek routers. Same protocol as the Windows Smart VPN Client (TLS → HTTP CONNECT → SSTP → PPP).

## Workspace Layout

Cargo workspace with four members:

- `protocol/` — `draytek-vpn-protocol` lib: TLS connect, SSTP framing, PPP FSM, LCP/IPCP/auth (PAP + MS-CHAPv2), keepalive. Used by all binaries.
- `standalone/` — `draytek-vpn` GTK4/libadwaita desktop app + `draytek-vpn-helper` (Polkit-elevated for TUN/routes/DNS). `standalone/src/tunnel/engine.rs` is the GUI data loop.
- `networkmanager/` — `draytek-vpn-nm` VPN plugin on the system D-Bus (runs as root under NM). GTK3/GTK4 editor `.so` and auth-dialog are C (`networkmanager/editor/`, `networkmanager/auth-dialog/`). `networkmanager/src/tunnel.rs` is the NM data loop.
- `networkmanagertray/` — `draytek-vpn-tray` ksni StatusNotifier tray. Watches NM over D-Bus (`nm_monitor.rs`), renders via `tray_impl.rs`. Launched/killed by `networkmanager/data/90-draytek-vpn-tray` dispatcher script on vpn-up/vpn-down.

## Build & Test

`build.sh` is the canonical build/install entry point. Do not invoke `cargo build` + `makepkg`/`cp` manually — `build.sh` handles distro detection (Debian multiarch vs Arch/Fedora), staging cleanup, and service restarts.

```bash
./build.sh app                  # standalone GTK4 app (debug)
./build.sh app release          # release build
./build.sh app install          # release + install polkit policy
./build.sh nm release           # NM plugin + editor .so + auth-dialog
./build.sh nm install           # build + install + restart NetworkManager
./build.sh tray install         # tray indicator + autostart
./build.sh arch install         # Arch: makepkg -fCsi (clean staging, force, install)
./build.sh all install          # everything
./build.sh clean                # remove build artifacts
```

Packaging:

- Arch: `packaging/arch/PKGBUILD` → `draytek-vpn-standalone` + `draytek-vpn-networkmanager`
- Debian: `./build.sh app deb` / `./build.sh nm deb`
- AppImage: `./build.sh app appimage`

Release profile is defined at workspace root (`Cargo.toml`), not in any member crate — cargo ignores `[profile.*]` outside the workspace root.

### Running locally during development

```bash
./build.sh all install          # builds + installs everything; NM restarts itself
nmcli connection up <name>      # trigger the NM plugin; tray auto-launches via dispatcher
journalctl -u NetworkManager -f # tail plugin logs
```

The standalone GUI is the fastest path for iterating on protocol changes — `./build.sh app run` builds debug and launches it with stderr logs in-terminal.

## Pre-push Checks

Run all three before committing/pushing any Rust changes — adopted from niri's CI standard:

```bash
cargo fmt --check && cargo clippy --all --all-targets && cargo test --all
```

All three must be clean. No warnings allowed in clippy output.

### No `#[allow(...)]` suppressions — code smell

Allow attributes paper over real design issues. When a lint fires, refactor the code honestly:

- `dead_code` → delete the unused code, or actually wire it up
- `too_many_arguments` → bundle args into a struct (see `PppFsmPair` / `TunnelAddrs` in `protocol/src/engine_common.rs`), or split the function
- `derivable_impls` → use `#[derive(Default)]` with `#[default]` on the variant
- any other lint → fix it, don't suppress

Only acceptable exception: the lint is provably wrong for a narrow specific reason, with a comment explaining why.

## Error Handling

Both the library (`protocol/`) and the binaries use `anyhow::Result` end-to-end. This is a deliberate trade-off: the library leaks `anyhow` to consumers, which is fine here because the only consumers are the two internal binaries. If `protocol/` ever grows third-party consumers, migrate its public surface to `thiserror`-derived error enums at that point. `thiserror` is intentionally not a dependency today — do not add it back without a concrete consumer.

## Known Gotchas

- **Compiled `.so` artifacts are tracked in git** (`networkmanager/editor/libnm-*.so`, `networkmanager/auth-dialog/nm-draytek-auth-dialog`). `build.sh` rewrites them in place on every NM build; they'll show as dirty in `git status` after any `./build.sh nm` run. Don't commit these incidentally unless the C sources changed.
- **C editor keys must match Rust `parse_settings`** — any new `vpn.data` key needs a matching `#define NM_DRAYTEK_KEY_*` in `networkmanager/editor/nm-draytek-editor.h` AND a read/write in the `.c` file AND a parser in `networkmanager/src/tunnel.rs::parse_settings`. Mismatches silently drop data.
- **NM plugin runs as root under NetworkManager**; stdin is closed and stderr goes to journald. Don't expect `println!` — use `tracing::{info,warn,error}!`.
- **`tokio::select!` macro hygiene** brings `std::pin::Pin` into scope inside its branches. Prefer a fully-qualified `std::pin::Pin::new(...)` at the call site so the behaviour doesn't depend on macro internals.

## Key Source Files

- `protocol/src/engine_common.rs` — `PppFsmPair`, `TunnelAddrs`, `PingKeeper`, `TrafficStats`, shared helpers used by both data loops
- `protocol/src/negotiate.rs` — PPP negotiation state machine driver (returns `NegotiationResult` to feed into the data loop)
- `protocol/src/protocol/fsm.rs` — generic PPP finite state machine (LCP/IPCP)
- `protocol/src/keepalive.rs` — `KeepaliveTracker`: 10s idle → REQUEST, 3 missed → disconnect
- `standalone/src/tunnel/engine.rs` — GUI data loop (`data_loop`), Polkit helper invocation via `privilege`
- `networkmanager/src/tunnel.rs` — NM plugin data loop; emits NM D-Bus signals (`state_changed`, `config`, `ip4_config`)
- `networkmanager/src/plugin.rs` — `org.freedesktop.NetworkManager.VPN.Plugin` D-Bus interface
- `networkmanagertray/src/nm_monitor.rs` — NM D-Bus watcher; `VpnState` enum flows to tray via `tokio::sync::watch`
- `networkmanagertray/src/tray_impl.rs` — ksni rendering of `VpnState` (icon, tooltip, menu, keepalive status display)
- `networkmanager/editor/nm-draytek-editor.c` — C editor plugin, GTK3/GTK4 variants built from the same sources

## Remotes

- `origin` — GitHub (public): https://github.com/julianjc84/draytek-ssl-vpn-client-linux
- `omv` — NAS mirror (private): `omv:/srv/dev-disk-by-uuid-bea51c11-5f48-44fb-a728-6acfe6c133bb/ProjectGitSync/DrayTek_Smart_VPN_ParentFolder/draytek-ssl-vpn-client-linux.git`

Push to both.

## Runtime Logs

- NM plugin (as root, spawned by NM): `journalctl -u NetworkManager -f` — the plugin's `tracing` output goes to NM's journal stream.
- Tray (user session, systemd-run scope): `journalctl --user -f` — filter by `_COMM=draytek-vpn-tray` if noisy.
- Standalone app: stderr in the launching terminal, or `~/.local/share/draytek-vpn/` logs if launched detached.
