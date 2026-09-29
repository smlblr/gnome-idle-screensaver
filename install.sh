#!/usr/bin/env bash
# User-level install: writes only under ~/.local and ~/.config, needs no sudo.
#   ./install.sh             install and (re)start the watcher
#   ./install.sh --no-start  install, do not start the watcher
# Kullanıcı düzeyinde kurulum; yalnızca ~/.local ve ~/.config altına yazar, sudo gerekmez.
set -euo pipefail
cd "$(dirname "$(readlink -f "$0")")"

case "${LC_ALL:-${LC_MESSAGES:-${LANG:-}}}" in tr*) TR=1 ;; *) TR=0 ;; esac
say() { if ((TR)); then echo "$2"; else echo "$1"; fi; }

APP_ID=io.github.smlblr.GnomeIdleScreensaver
BIN_DIR="$HOME/.local/bin"
DATA_DIR="${XDG_DATA_HOME:-$HOME/.local/share}"
CONFIG_DIR="${XDG_CONFIG_HOME:-$HOME/.config}"
BIN="$BIN_DIR/gnome-idle-screensaver"
BUILD=target/release/gnome-idle-screensaver
APPS="$DATA_DIR/applications"
ICONS="$DATA_DIR/icons/hicolor"
AUTOSTART="$CONFIG_DIR/autostart"

start=1
[[ ${1:-} == --no-start ]] && start=0

if [[ ! -x $BUILD ]]; then
    say "Build first: cargo build --release  (on a laptop: nice -n 10 cargo build --release -j 4)" \
        "Önce derleyin: cargo build --release  (dizüstünde: nice -n 10 cargo build --release -j 4)" >&2
    exit 1
fi
if [[ $BIN == *[[:space:]]* ]]; then
    say "An install path with spaces is not supported: $BIN" \
        "Boşluk içeren kurulum yolu desteklenmiyor: $BIN" >&2
    exit 1
fi

render() { # template target
    local tmp
    tmp=$(mktemp)
    sed "s|@BINARY@|$BIN|g" "$1" >"$tmp"
    install -Dm644 "$tmp" "$2"
    rm -f "$tmp"
    echo "  $2"
}

say "Installing:" "Kuruluyor:"
install -Dm755 "$BUILD" "$BIN"
echo "  $BIN"
render "data/$APP_ID.desktop.in" "$APPS/$APP_ID.desktop"
render "data/$APP_ID.Daemon.desktop.in" "$AUTOSTART/$APP_ID.Daemon.desktop"
install -Dm644 "data/$APP_ID.svg" "$ICONS/scalable/apps/$APP_ID.svg"
echo "  $ICONS/scalable/apps/$APP_ID.svg"

if command -v desktop-file-validate >/dev/null; then
    desktop-file-validate "$APPS/$APP_ID.desktop" "$AUTOSTART/$APP_ID.Daemon.desktop"
fi
command -v update-desktop-database >/dev/null && update-desktop-database -q "$APPS"
# Only refresh a cache that already exists; creating one would hide icons that
# other programs later add without updating it.
if [[ -f $ICONS/icon-theme.cache ]] && command -v gtk-update-icon-cache >/dev/null; then
    gtk-update-icon-cache -q -f -t "$ICONS"
fi
touch "$ICONS"

pids=$(pgrep -u "$USER" -x -f "$BIN daemon" || true)
if [[ -n $pids ]]; then
    say "Stopping the old watcher (PID $pids)" "Eski izleyici durduruluyor (PID $pids)"
    # shellcheck disable=SC2086 # several PIDs, word splitting intended
    kill $pids
fi
if ((start)); then
    gio launch "$AUTOSTART/$APP_ID.Daemon.desktop" >/dev/null 2>&1 ||
        setsid -f "$BIN" daemon </dev/null >/dev/null 2>&1
    say "Watcher started; it starts by itself in later sessions." \
        "İzleyici başlatıldı; sonraki oturumlarda kendiliğinden başlar."
fi

echo
say "Settings: open \"Screensaver\" in the app grid, or run: $BIN settings" \
    "Ayarlar: uygulama ızgarasında \"Ekran Koruyucu\" ya da: $BIN settings"
say "GNOME Shell usually shows a new icon only after you log out and in." \
    "GNOME Shell yeni simgeyi çoğu zaman ancak oturum kapatılıp açılınca gösterir."
