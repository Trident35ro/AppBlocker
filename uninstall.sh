#!/usr/bin/env bash
# AppBlocker uninstall script
set -euo pipefail

BINARY_NAME="appblocker"
SERVICE_NAME="${BINARY_NAME}.service"

green() { printf '\e[32m✓\e[0m %s\n' "$*"; }
yellow(){ printf '\e[33m→\e[0m %s\n' "$*"; }

# Stop and disable the systemd service.
systemctl --user stop  "${SERVICE_NAME}" 2>/dev/null && green "Service stopped."   || true
systemctl --user disable "${SERVICE_NAME}" 2>/dev/null && green "Service disabled." || true

SERVICE_PATH="${HOME}/.config/systemd/user/${SERVICE_NAME}"
if [[ -f "${SERVICE_PATH}" ]]; then
    rm "${SERVICE_PATH}"
    systemctl --user daemon-reload 2>/dev/null || true
    green "Service file removed."
fi

# Remove binary.
for dir in "/usr/local/bin" "${HOME}/.local/bin"; do
    target="${dir}/${BINARY_NAME}"
    if [[ -f "${target}" ]]; then
        rm "${target}"
        green "Removed binary: ${target}"
    fi
done

# Remove desktop entry.
for dir in "/usr/share/applications" "${HOME}/.local/share/applications"; do
    desktop="${dir}/${BINARY_NAME}.desktop"
    if [[ -f "${desktop}" ]]; then
        rm "${desktop}"
        update-desktop-database "${dir}" 2>/dev/null || true
        green "Removed desktop entry: ${desktop}"
    fi
done

# Offer to remove config.
CONFIG_DIR="${HOME}/.config/appblocker"
if [[ -d "${CONFIG_DIR}" ]]; then
    read -r -p "Remove config and rules at ${CONFIG_DIR}? [y/N] " answer
    if [[ "${answer,,}" == "y" ]]; then
        rm -rf "${CONFIG_DIR}"
        green "Config removed."
    else
        yellow "Config kept at ${CONFIG_DIR}"
    fi
fi

echo ""
green "AppBlocker uninstalled."
