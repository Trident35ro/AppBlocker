#!/usr/bin/env bash
# AppBlocker install script
# Works with a pre-built binary (GitHub release tarball) or builds from source.
set -euo pipefail

BINARY_NAME="appblocker"
DESKTOP_SRC="assets/appblocker.desktop"
SERVICE_NAME="${BINARY_NAME}.service"

# ── Colour helpers ────────────────────────────────────────────────────────────
green()  { printf '\e[32m✓\e[0m %s\n' "$*"; }
yellow() { printf '\e[33m→\e[0m %s\n' "$*"; }
red()    { printf '\e[31m✗\e[0m %s\n' "$*" >&2; }
die()    { red "$*"; exit 1; }

# ── Locate or build the binary ────────────────────────────────────────────────
if [[ -f "./${BINARY_NAME}" ]]; then
    # Pre-built binary sitting next to this script (release tarball layout).
    BINARY="./${BINARY_NAME}"
    yellow "Using pre-built binary: ${BINARY}"
elif command -v cargo &>/dev/null; then
    yellow "Building release binary (this may take a minute)…"
    cargo build --release
    BINARY="./target/release/${BINARY_NAME}"
else
    die "No pre-built binary found and 'cargo' is not installed. \
Install Rust from https://rustup.rs or download a release tarball."
fi

[[ -f "${BINARY}" ]] || die "Binary not found at ${BINARY}"

# ── Determine install paths ───────────────────────────────────────────────────
if [[ "${EUID}" -eq 0 ]]; then
    # Root install — system-wide.
    BIN_DIR="/usr/local/bin"
    DESKTOP_DIR="/usr/share/applications"
    yellow "Running as root — installing system-wide."
else
    # User install — no sudo required.
    BIN_DIR="${HOME}/.local/bin"
    DESKTOP_DIR="${HOME}/.local/share/applications"
    yellow "Installing for current user only (${USER})."
fi

INSTALL_PATH="${BIN_DIR}/${BINARY_NAME}"
DESKTOP_DEST="${DESKTOP_DIR}/${BINARY_NAME}.desktop"
SERVICE_DIR="${HOME}/.config/systemd/user"
SERVICE_PATH="${SERVICE_DIR}/${SERVICE_NAME}"

# ── Install binary ────────────────────────────────────────────────────────────
mkdir -p "${BIN_DIR}"
install -Dm755 "${BINARY}" "${INSTALL_PATH}"
green "Binary installed to ${INSTALL_PATH}"

# Warn if the directory is not in PATH.
if ! echo "${PATH}" | tr ':' '\n' | grep -qx "${BIN_DIR}"; then
    yellow "NOTE: ${BIN_DIR} is not in your PATH."
    yellow "Add this to your shell rc file:"
    yellow "  export PATH=\"\${PATH}:${BIN_DIR}\""
fi

# ── Install .desktop file ─────────────────────────────────────────────────────
mkdir -p "${DESKTOP_DIR}"
if [[ -f "${DESKTOP_SRC}" ]]; then
    # Patch the Exec path to the installed binary.
    sed "s|^Exec=.*|Exec=${INSTALL_PATH}|" "${DESKTOP_SRC}" > "${DESKTOP_DEST}"
else
    # Generate a minimal one in case assets/ wasn't bundled.
    cat > "${DESKTOP_DEST}" <<EOF
[Desktop Entry]
Name=AppBlocker
GenericName=Application Blocker
Comment=Block and schedule applications based on rules, time, and resource usage
Exec=${INSTALL_PATH}
Icon=security-high
Terminal=false
Type=Application
Categories=Utility;System;
Keywords=block;focus;productivity;scheduler;apps;kill;
StartupNotify=true
EOF
fi
green "Desktop entry installed to ${DESKTOP_DEST}"

# Refresh the desktop database so the launcher picks it up.
update-desktop-database "${DESKTOP_DIR}" 2>/dev/null && green "Desktop database updated." || true

# ── Install systemd user service (startup) ────────────────────────────────────
mkdir -p "${SERVICE_DIR}"
cat > "${SERVICE_PATH}" <<EOF
[Unit]
Description=AppBlocker Daemon
After=graphical-session.target

[Service]
Type=simple
ExecStart=${INSTALL_PATH} --daemon
Restart=on-failure
RestartSec=5

[Install]
WantedBy=graphical-session.target
EOF

systemctl --user daemon-reload 2>/dev/null && true
systemctl --user enable "${SERVICE_NAME}" 2>/dev/null \
    && green "Systemd user service enabled — daemon will start on next login." \
    || yellow "Could not enable systemd service (no graphical session?). Run manually: systemctl --user enable ${SERVICE_NAME}"

# ── Done ──────────────────────────────────────────────────────────────────────
echo ""
green "AppBlocker installed successfully!"
echo ""
echo "  Launch:      ${INSTALL_PATH}"
echo "  App menu:    look under Utilities"
echo "  Start daemon now: systemctl --user start ${SERVICE_NAME}"
echo "  Uninstall:   ./uninstall.sh"
