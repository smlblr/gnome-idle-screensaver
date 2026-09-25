#!/usr/bin/env bash
# install.sh'nin yazdığı her şeyi kaldırır.
#   ./uninstall.sh          ayar dosyası kalır
#   ./uninstall.sh --purge  ~/.config/gnome-idle-screensaver de silinir
set -euo pipefail

APP_ID=io.github.smlblr.GnomeIdleScreensaver
DATA_DIR="${XDG_DATA_HOME:-$HOME/.local/share}"
CONFIG_DIR="${XDG_CONFIG_HOME:-$HOME/.config}"
BIN="$HOME/.local/bin/gnome-idle-screensaver"
ICONS="$DATA_DIR/icons/hicolor"

pids=$(pgrep -u "$USER" -x -f "$BIN daemon" || true)
if [[ -n $pids ]]; then
    echo "İzleyici durduruluyor (PID $pids)"
    kill $pids
fi

for f in \
    "$BIN" \
    "$DATA_DIR/applications/$APP_ID.desktop" \
    "$CONFIG_DIR/autostart/$APP_ID.Daemon.desktop" \
    "$ICONS/scalable/apps/$APP_ID.svg"; do
    if [[ -e $f ]]; then
        rm -f "$f"
        echo "Silindi: $f"
    fi
done

if [[ ${1:-} == --purge && -d $CONFIG_DIR/gnome-idle-screensaver ]]; then
    rm -rf "$CONFIG_DIR/gnome-idle-screensaver"
    echo "Silindi: $CONFIG_DIR/gnome-idle-screensaver"
fi

command -v update-desktop-database >/dev/null && update-desktop-database -q "$DATA_DIR/applications"
if [[ -f $ICONS/icon-theme.cache ]] && command -v gtk-update-icon-cache >/dev/null; then
    gtk-update-icon-cache -q -f -t "$ICONS"
fi
touch "$ICONS" 2>/dev/null || true
echo "Kaldırıldı."
