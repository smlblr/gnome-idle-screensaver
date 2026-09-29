#!/usr/bin/env bash
# Removes everything install.sh wrote.
#   ./uninstall.sh          your settings file stays
#   ./uninstall.sh --purge  ~/.config/gnome-idle-screensaver is deleted too
# install.sh'nin yazdığı her şeyi kaldırır.
set -euo pipefail

case "${LC_ALL:-${LC_MESSAGES:-${LANG:-}}}" in tr*) TR=1 ;; *) TR=0 ;; esac
say() { if ((TR)); then echo "$2"; else echo "$1"; fi; }

APP_ID=io.github.smlblr.GnomeIdleScreensaver
DATA_DIR="${XDG_DATA_HOME:-$HOME/.local/share}"
CONFIG_DIR="${XDG_CONFIG_HOME:-$HOME/.config}"
BIN="$HOME/.local/bin/gnome-idle-screensaver"
ICONS="$DATA_DIR/icons/hicolor"

pids=$(pgrep -u "$USER" -x -f "$BIN daemon" || true)
if [[ -n $pids ]]; then
    say "Stopping the watcher (PID $pids)" "İzleyici durduruluyor (PID $pids)"
    # shellcheck disable=SC2086 # several PIDs, word splitting intended
    kill $pids
fi

for f in \
    "$BIN" \
    "$DATA_DIR/applications/$APP_ID.desktop" \
    "$CONFIG_DIR/autostart/$APP_ID.Daemon.desktop" \
    "$ICONS/scalable/apps/$APP_ID.svg"; do
    if [[ -e $f ]]; then
        rm -f "$f"
        say "Removed: $f" "Silindi: $f"
    fi
done

if [[ ${1:-} == --purge && -d $CONFIG_DIR/gnome-idle-screensaver ]]; then
    rm -rf "$CONFIG_DIR/gnome-idle-screensaver"
    say "Removed: $CONFIG_DIR/gnome-idle-screensaver" "Silindi: $CONFIG_DIR/gnome-idle-screensaver"
fi

command -v update-desktop-database >/dev/null && update-desktop-database -q "$DATA_DIR/applications"
if [[ -f $ICONS/icon-theme.cache ]] && command -v gtk-update-icon-cache >/dev/null; then
    gtk-update-icon-cache -q -f -t "$ICONS"
fi
touch "$ICONS" 2>/dev/null || true
say "Removed." "Kaldırıldı."
