# AppBlocker

A KDE Linux app for blocking, scheduling, and managing application access — built in Rust with egui.

Block apps by process name, schedule them on/off by time, trigger on resource usage, and enforce rules automatically via a background daemon.

## Features

- **Three blocking methods** — Kill (SIGTERM), Force Kill (SIGKILL / pkill -9), PATH Wrapper, or Network Block (nftables)
- **Flexible scheduling** — Always, Time Range (HH:MM – HH:MM), or Rest of Day (until midnight)
- **Grace period** — warn N minutes before a block kicks in
- **Resource trigger** — block automatically when CPU% or RAM exceeds a threshold for a set duration
- **Lazy / fuzzy matching** — type `steam` to catch `steam`, `steamwebhelper`, `steam_osx` all at once
- **Startup actions** — launch an app at login or have it blocked from the moment the session starts
- **Real-time Monitor tab** — live process list sorted by CPU/RAM, right-click to create rules instantly
- **System tray** — runs silently in the background, toggleable
- **Session-only rules** — rules that vanish when AppBlocker exits (no permanent config change)
- **Persistent config** — TOML file at `~/.config/appblocker/config.toml`

## Requirements

| Dependency | Purpose |
|---|---|
| Linux (any distro) | — |
| KDE Plasma (or any D-Bus desktop) | System tray via StatusNotifierItem |
| `libnotify` / `notify-send` | Block/warning desktop notifications |
| `pkexec` (polkit) | Network blocking only — prompts for root |
| Rust 1.75+ | Building from source only |

## Installation

### From a GitHub release (recommended)

1. Download the latest release tarball from [Releases](https://github.com/Segually/AppBlocker/releases)
2. Extract and run the install script:

```bash
tar xzf appblocker-*.tar.gz
cd appblocker-*/
./install.sh
```

The script will:
- Copy the binary to `~/.local/bin/` (or `/usr/local/bin/` if run as root)
- Create a `.desktop` entry so AppBlocker appears in your app launcher under **Utilities**
- Install and enable a systemd user service so the daemon starts automatically on login

### From source

```bash
git clone https://github.com/Segually/AppBlocker
cd AppBlocker
./install.sh        # builds with cargo then installs
```

### Manual

```bash
cargo build --release
cp target/release/appblocker ~/.local/bin/
```

## Usage

Launch from your app menu (under **Utilities**) or run:

```bash
appblocker
```

### Creating a rule

1. Open the **Rules** tab → click **➕ Add Rule**
2. Enter a display name and the process name or full path
   - `steam` — matches the `steam` process (case-insensitive)
   - `/usr/bin/firefox` — matches by exact path
   - Enable **Lazy match** to catch all processes whose name *contains* your input (e.g. `steam` also blocks `steamwebhelper`)
3. Choose a blocking method (**Kill** is recommended for most apps)
4. Set the schedule and save

### Quick-block from the Monitor tab

Open **Monitor**, right-click any running process → **Create block rule…** or **Block for rest of day**.

### Blocking Steam (example)

| Field | Value |
|---|---|
| Display name | Steam |
| Executable / name | `steam` |
| Lazy match | ✓ (catches steamwebhelper too) |
| Method | Force Kill |
| Schedule | Always (or a time range) |

### Daemon mode

The daemon enforces rules in the background. Check its status:

```bash
systemctl --user status appblocker.service
systemctl --user start  appblocker.service   # start now
systemctl --user stop   appblocker.service   # stop
```

### Debugging

Run with logging to see exactly what the daemon is matching and killing:

```bash
RUST_LOG=info appblocker
# or for verbose detail:
RUST_LOG=debug appblocker
```

## Uninstall

```bash
./uninstall.sh
```

Removes the binary, desktop entry, and systemd service. Optionally removes your config.

## Building a release tarball (for maintainers)

```bash
cargo build --release
VERSION=$(grep '^version' Cargo.toml | head -1 | grep -oP '[\d.]+')
DIST="appblocker-${VERSION}"
mkdir -p "${DIST}/assets"
cp target/release/appblocker install.sh uninstall.sh README.md "${DIST}/"
cp assets/appblocker.desktop "${DIST}/assets/"
tar czf "${DIST}.tar.gz" "${DIST}/"
```

## License

MIT
