# DrayTek SSL VPN Client for Linux

Native Linux SSL VPN client for DrayTek routers. Connects to DrayTek's built-in SSL VPN server using the same protocol as the official Windows Smart VPN Client — no special router configuration needed.

## How It Works

The client establishes a TLS connection to the router, performs an HTTP CONNECT handshake, then negotiates a PPP tunnel using SSTP framing. Authentication supports both PAP and MS-CHAPv2. Once the PPP session is up (LCP, authentication, IPCP), a TUN device is created and IPv4 traffic flows through the encrypted tunnel.

```
TLS 1.2 → HTTP CONNECT → SSTP framing → PPP (LCP/Auth/IPCP) → IPv4 tunnel
```

The protocol implementation is a shared Rust library used by all components.

## Components

The project has four components that can be used independently or together:

| Component | Description | Use When |
|-----------|-------------|----------|
| **GUI App** | GTK4/libadwaita desktop application | You want a graphical interface to manage connections |
| **NetworkManager Plugin** | Integrates into NM as a VPN provider | You want VPN in GNOME Settings / `nmcli` / system-managed |
| **System Tray** | Status indicator in the system tray | You want to see VPN status, IP, and routes at a glance |
| **Protocol Library** | Shared crate implementing the full VPN protocol | Used internally by all components above |

### GUI App (`src/`)

A standalone GTK4/libadwaita application for managing VPN connections. Saves connection profiles locally and provides a log view for debugging.

- **Main thread** runs the GTK4/GLib event loop (UI)
- **Background thread** runs a Tokio async runtime for TLS, TUN I/O, and timers
- **Privilege separation**: network operations (TUN device creation, routing, DNS) run in a separate `draytek-vpn-helper` binary elevated via Polkit (`pkexec`), so the GUI itself never runs as root

### NetworkManager Plugin (`networkmanager/`)

Integrates directly with NetworkManager so VPN connections appear alongside Wi-Fi and Ethernet in GNOME Settings, KDE, Cinnamon, or any NM frontend. NM spawns the plugin as root, so no password prompts are needed to connect.

The plugin consists of three parts:

| Part | Language | Purpose |
|------|----------|---------|
| **VPN Service** | Rust | D-Bus daemon that manages the tunnel, communicates with NM via `org.freedesktop.NetworkManager.VPN.Plugin` |
| **Editor Plugin** | C | Settings UI that loads inside `nm-connection-editor` or GNOME Settings. Ships as three `.so` files — a base plugin plus GTK3 and GTK4 editors, with runtime detection to load the right one |
| **Auth Dialog** | C | Provides the VPN password to NM when connecting |

### System Tray (`tray/`)

A lightweight system tray indicator that monitors NetworkManager over D-Bus. Shows VPN connection status with colored icons (green = connected, red = disconnected, amber = connecting), the assigned IP address, active routes, and traffic statistics. Runs in the background and starts automatically on login.

### Protocol Library (`protocol/`)

Shared Rust crate that implements the full protocol stack. Used by both the GUI app and the NM plugin. The PPP finite state machine is a pure state machine — takes events in, returns actions out — making it straightforward to test.

## Prerequisites

### System dependencies

```bash
# Debian/Ubuntu
sudo apt install build-essential libgtk-4-dev libadwaita-1-dev libssl-dev pkg-config

# Additional dependencies for the NetworkManager plugin
sudo apt install libnm-dev libgtk-3-dev gcc
```

### Rust toolchain

Install via [rustup](https://rustup.rs/) if you don't have it:

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

### Kernel requirements

- Linux kernel TUN/TAP support (`/dev/net/tun` must exist)

## Building & Installing

Everything is managed through `./build.sh`:

```
./build.sh <target> [action]
```

### Targets

| Target | What it builds |
|--------|----------------|
| `app` | GUI app + privileged helper |
| `nm` | NetworkManager plugin (Rust service + C editor + C auth-dialog) |
| `tray` | System tray indicator |
| `all` | All of the above |

### Actions

| Action | Effect |
|--------|--------|
| *(none)* | Build debug |
| `release` | Build release (optimized) |
| `install` | Build release + install system-wide (requires sudo) |
| `run` | Build debug + launch immediately (app only) |
| `uninstall` | Remove installed files (nm, tray) |
| `clean` | Remove C build artifacts |

### Examples

```bash
# Build and run the GUI app
./build.sh app run

# Build and install the NetworkManager plugin
./build.sh nm install

# Build and install the system tray indicator
./build.sh tray install

# Build everything in release mode
./build.sh all release

# Install everything
./build.sh all install

# Remove the NM plugin
./build.sh nm uninstall
```

## Quick Start

### Option 1: GUI App

```bash
./build.sh app run
```

Create a connection profile in the GUI, enter your router's address and credentials, and click Connect. A desktop password prompt (Polkit) will appear to authorize network operations.

**Optional**: Install the Polkit policy for a nicer auth dialog with credential caching:

```bash
sudo cp data/com.draytek.vpn.policy /usr/share/polkit-1/actions/
```

### Option 2: NetworkManager

```bash
# Install the plugin
./build.sh nm install

# Create a connection
nmcli connection add type vpn \
    vpn-service-type org.freedesktop.NetworkManager.draytek \
    con-name "My DrayTek VPN" \
    vpn.data "gateway=vpn.example.com,port=443,username=myuser,verify-cert=no"

# Set the password
nmcli connection modify "My DrayTek VPN" vpn.secrets "password=mypassword"

# Connect
nmcli connection up "My DrayTek VPN"
```

Or use GNOME Settings / KDE / Cinnamon — "DrayTek SSL VPN" will appear as a VPN type when adding a new connection.

### Option 3: System Tray (alongside NetworkManager)

```bash
./build.sh tray install
```

The tray indicator starts automatically on login. It monitors NM for DrayTek VPN connections and shows status, IP, routes, and traffic stats.

## NetworkManager Configuration Keys

These go in `vpn.data` when creating a connection via `nmcli`:

| Key | Default | Description |
|-----|---------|-------------|
| `gateway` | *(required)* | VPN server hostname or IP |
| `port` | `443` | Server port |
| `username` | *(required)* | VPN username |
| `verify-cert` | `no` | Verify server TLS certificate (`yes`/`no`) |
| `mru` | `0` (auto) | MRU to propose during LCP negotiation |
| `route-remote-network` | `yes` | Auto-route the remote gateway's /24 subnet |
| `never-default` | `yes` | Don't replace the default route with the VPN |
| `keepalive` | `no` | Enable ICMP keepalive pings |
| `auto-reconnect` | `no` | Automatically reconnect on disconnect |
| `routes` | *(empty)* | Additional routes in CIDR notation, comma-separated (e.g. `10.0.0.0/8,172.16.0.0/12`) |

Password is stored in `vpn.secrets` under the key `password`.

## Project Structure

```
draytek-vpn/
├── build.sh                        # Build & install script for all components
├── Cargo.toml                      # Workspace root
│
├── protocol/                       # Shared protocol library (Rust)
│   └── src/
│       ├── connection.rs           #   TLS + HTTP CONNECT handshake
│       ├── negotiate.rs            #   PPP negotiation (LCP/Auth/IPCP)
│       ├── engine_common.rs        #   Data loop, stats, keepalive
│       └── protocol/               #   Wire protocol implementations
│           ├── fsm.rs              #     PPP finite state machine
│           ├── sstp.rs             #     SSTP framing
│           ├── ppp.rs              #     PPP framing
│           ├── lcp.rs / ipcp.rs    #     LCP + IPCP options
│           └── auth/               #     PAP + MS-CHAPv2
│
├── src/                            # GUI application (Rust, GTK4/libadwaita)
│   ├── app.rs                      #   Application entry point
│   ├── ui/                         #   Window, profile editor, connection view
│   ├── tunnel/                     #   Tunnel orchestrator, TUN device
│   ├── config.rs                   #   Profile persistence
│   └── bin/
│       └── draytek-vpn-helper.rs   #   Privileged helper (runs via pkexec)
│
├── networkmanager/                 # NetworkManager VPN plugin
│   ├── src/                        #   Rust VPN service
│   │   ├── plugin.rs               #     D-Bus interface (zbus)
│   │   └── tunnel.rs               #     Tunnel lifecycle
│   ├── editor/                     #   C editor plugin (.so files)
│   ├── auth-dialog/                #   C auth dialog
│   └── data/                       #   D-Bus and NM config files
│
├── tray/                           # System tray indicator (Rust)
│   └── src/
│       ├── nm_monitor.rs           #   NM D-Bus monitor
│       ├── tray_impl.rs           #   Menu and status rendering
│       └── icons.rs                #   Colored status icons
│
└── data/
    ├── com.draytek.vpn.policy      # Polkit policy for GUI app
    └── draytek-vpn-tray.desktop    # Autostart entry for tray
```

## Testing

```bash
cargo test --workspace
```

## License

This project is licensed under the [GNU General Public License v3.0](LICENSE).
