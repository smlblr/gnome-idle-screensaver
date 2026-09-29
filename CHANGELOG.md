# Changelog

All notable changes to this project are listed here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/).

## [0.1.0] - 2026-09-29

First release.

### Added
- Watcher that starts with the session and shows a full-screen animation after a set idle time,
  using Mutter's IdleMonitor without polling. Never locks the session.
- Six animations: gears (glxgears, anti-aliased, orbiting camera), starfield, plasma, tunnel,
  aurora, and a black "screen off".
- Respect for idle inhibitors such as video playback, switchable.
- Settings window (libadwaita) in English and Turkish with live language switching;
  settings are picked up by the running watcher without a restart.
- Closing on key, click, scroll or deliberate mouse movement; small jitter is ignored.
- `preview --animation` to look at one animation without changing settings.
- `install.sh` and `uninstall.sh` (user level) and a `.deb` package.
