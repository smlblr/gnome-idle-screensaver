#!/usr/bin/env bash
# Builds dist/gnome-idle-screensaver_<version>_amd64.deb from target/release.
# Nothing is installed; install the result with `sudo apt install ./dist/*.deb`.
# Yalnızca paketi üretir; kurmak için `sudo apt install ./dist/*.deb`.
set -euo pipefail
cd "$(dirname "$(readlink -f "$0")")/.."

APP_ID=io.github.smlblr.GnomeIdleScreensaver
PKG=gnome-idle-screensaver
BIN=target/release/$PKG
VERSION=$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)
ARCH=$(dpkg --print-architecture)
OUT=dist/${PKG}_${VERSION}_${ARCH}.deb

[[ -x $BIN ]] || { echo "Build first: cargo build --release" >&2; exit 1; }

root=$(mktemp -d)
trap 'rm -rf "$root"' EXIT

install -Dm755 "$BIN" "$root/usr/bin/$PKG"
render() { sed "s|@BINARY@|/usr/bin/$PKG|g" "$1" >"$2"; chmod 644 "$2"; }
mkdir -p "$root/usr/share/applications" "$root/etc/xdg/autostart"
render "data/$APP_ID.desktop.in" "$root/usr/share/applications/$APP_ID.desktop"
render "data/$APP_ID.Daemon.desktop.in" "$root/etc/xdg/autostart/$APP_ID.Daemon.desktop"
install -Dm644 "data/$APP_ID.svg" "$root/usr/share/icons/hicolor/scalable/apps/$APP_ID.svg"

doc=$root/usr/share/doc/$PKG
install -Dm644 README.md "$doc/README.md"
install -Dm644 README.tr.md "$doc/README.tr.md"
cat >"$doc/copyright" <<'COPYRIGHT'
Format: https://www.debian.org/doc/packaging-manuals/copyright-format/1.0/
Upstream-Name: gnome-idle-screensaver

Files: *
Copyright: 2026 smlblr
License: MIT

Files: src/animations/gears.rs
Copyright: 1999-2001 Brian Paul
License: MIT
Comment: Port of glxgears.c from Mesa demos.

License: MIT
COPYRIGHT
sed -e 's/^$/./' -e 's/^/ /' LICENSE | sed '1,2d' >>"$doc/copyright"

# umask-independent modes: Debian wants directories 755 and plain files 644.
find "$root" -type d -exec chmod 755 {} +
find "$root" -type f ! -perm /111 -exec chmod 644 {} +

size_kb=$(du -sk "$root" | cut -f1)
# The oldest glibc the binary can run on is the newest symbol version it links against.
glibc=$(objdump -T "$BIN" | grep -o 'GLIBC_[0-9.]*' | sort -Vu | tail -1 | cut -d_ -f2)
mkdir -p "$root/DEBIAN"
cat >"$root/DEBIAN/control" <<CONTROL
Package: $PKG
Version: $VERSION
Section: gnome
Priority: optional
Architecture: $ARCH
Installed-Size: $size_kb
Depends: libc6 (>= $glibc), libgtk-4-1 (>= 4.14), libadwaita-1-0 (>= 1.5), libepoxy0
Recommends: gnome-shell (>= 46)
Maintainer: smlblr <67227628+smlblr@users.noreply.github.com>
Description: Idle screensaver for GNOME on Wayland
 Shows a full-screen animation after the computer has been idle for a set
 time and closes on any key, click or deliberate mouse movement. Idle time
 comes from Mutter's IdleMonitor; playing video and other idle inhibitors
 are respected. Never locks the session.
 .
 Animations: gears (glxgears), starfield, plasma, tunnel, aurora, blank.
CONTROL
for script in postinst postrm; do
    cat >"$root/DEBIAN/$script" <<'SH'
#!/bin/sh
set -e
if command -v gtk-update-icon-cache >/dev/null 2>&1; then
    gtk-update-icon-cache -q -t -f /usr/share/icons/hicolor || true
fi
if command -v update-desktop-database >/dev/null 2>&1; then
    update-desktop-database -q /usr/share/applications || true
fi
exit 0
SH
    chmod 755 "$root/DEBIAN/$script"
done

mkdir -p dist
dpkg-deb --root-owner-group --build -Zxz "$root" "$OUT" >/dev/null
echo "$OUT"
dpkg-deb --info "$OUT" | sed -n '/Package:/,$p'
echo "--- içerik"
dpkg-deb --contents "$OUT"
