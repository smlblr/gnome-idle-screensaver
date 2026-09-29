# Architecture

[English](architecture.md) · [Türkçe](architecture.tr.md)

One binary, three subcommands, each its own process:

```
                 config.toml  ◄── writes ── settings (libadwaita window)
                     │                         │ Preview
              FileMonitor                      ▼
                     ▼                      Stage ──► GlScene (an animation)
 Mutter IdleMonitor ─► daemon ──────────────► Stage
 SessionManager ─────►   (when idle time runs out)
```

| File | Job |
|---|---|
| `src/daemon.rs` | Runs for the whole session. Sets an `AddIdleWatch` on Mutter, opens the stage on `WatchFired`. Follows inhibitors and the config file |
| `src/stage.rs` | One full-screen window per monitor: GLArea, frame clock, MSAA framebuffer, closing rules |
| `src/animations/` | The `GlScene` interface, the registry (`ALL`), matrix and shader helpers, one file per animation |
| `src/settings.rs` | The settings window |
| `src/preview.rs` | The `preview` subcommand |
| `src/config.rs` | Reading and writing `config.toml` (atomic), file monitoring |
| `src/dbus.rs` | Wrappers for Mutter's IdleMonitor and GNOME's SessionManager |
| `src/i18n.rs` | Interface languages |

## The watcher

- **No polling.** One persistent `AddIdleWatch(idle_ms)` on Mutter, which fires it once per idle
  period.
- **Inhibitors.** While a session idle inhibitor (flag 8) exists, Mutter does not fire idle
  watches at all. Measured: with the user idle for 589 s, a 3 s watch did not fire within 12 s,
  while `GetIdletime` kept counting. According to Mutter's source the idle counter also resets
  when the inhibitor goes away. So:
  - With **"Don't start while video is playing" on** (the default) Mutter's behaviour is the
    wanted behaviour. The daemon also checks `IsInhibited(8)` once when the watch fires. The
    suspend inhibitor (flag 4) is ignored on purpose.
  - With it **off**, the watch never arrives while the inhibitor lasts. The daemon listens for
    inhibitor changes (`InhibitorAdded/Removed`, `InhibitedActions`); if one exists it reads
    `GetIdletime`, computes the remaining time and arms a single one-shot timer, then checks
    again when it fires. One D-Bus call per threshold, not polling.
- **Config changes:** `gio::FileMonitor` → 150 ms debounce → reload → watch re-armed. A broken
  file keeps the previous settings.
- **GTK is initialised lazily**, on the first show: until then the daemon is a small D-Bus client
  (about 23 MB, 0 % CPU). Wayland, GTK and the GL driver load on the first show.
- **One copy:** the daemon owns `io.github.smlblr.GnomeIdleScreensaver.Daemon` on the session bus;
  a second `daemon` exits without doing anything.
- For scripts and tests, the running daemon accepts an action that shows the stage (argument:
  seconds until it closes by itself, `0` = until input):

  ```bash
  gdbus call --session --dest io.github.smlblr.GnomeIdleScreensaver.Daemon --object-path /io/github/smlblr/GnomeIdleScreensaver/Daemon --method org.freedesktop.Application.ActivateAction show '[<uint32 10>]' '{}'
  ```

## Stage, focus and closing

The stage window opens inside the long-lived daemon with `fullscreen_on_monitor`. On Wayland a
client cannot raise itself; the compositor decides. Mutter's rules, as read from its source:

- A new window without timestamp information gets focus and is placed on top.
- If the focused window is a terminal, it is only placed on top and focus stays in the terminal.
- `present()` also sends an xdg-activation request. Without a recent input serial Mutter refuses
  it, and it can turn into a "… is ready" notification.

So the idle-triggered stage opens with `set_visible(true)`, not `present()`, and the daemon drops
any inherited `XDG_ACTIVATION_TOKEN`. In practice the window took focus (`active=true` one second
after mapping) on the test machine.

**Closing:**

- Key, click, scroll: at once.
- Mouse: when the pointer travels 24 logical pixels from where a burst of movement began. A burst
  begins at the last position the pointer *rested* at, so one large jump counts; it ends after
  0.8 s of stillness, so slow sensor drift never accumulates. The position GTK reports on window
  enter is not used: under a window that has just mapped it can be far from the real pointer.
- Keyboard focus may stay in another window, so keys would never reach the stage. While the stage
  is open the daemon therefore holds a Mutter `AddUserActiveWatch`. If Mutter reports activity
  that does not reach our windows within 150 ms, the stage closes. Activity in the first 300 ms
  after opening is ignored, and so is input in the first 0.8 s of a preview (the click that asked
  for it).

## The animation interface

```rust
pub trait GlScene {
    fn realize(&mut self, gl: &glow::Context, glsl: GlslDialect) -> Result<(), String>;
    fn render(&mut self, gl: &glow::Context, frame: &Frame);
    fn unrealize(&mut self, gl: &glow::Context);
    fn msaa_samples(&self) -> i32 { 4 }
    fn animated(&self) -> bool { true }
}
```

The stage prepares the GL context, binds a multisampled framebuffer, sets the viewport, and hands
over time from the frame clock (vertical sync): `Frame { elapsed, delta, width, height }`. It then
resolves the samples into GtkGLArea's own buffer. A scene only draws. A scene that returns
`animated() == false` is drawn three times and then left alone, which is what makes the black
"screen off" free.

GTK4's GLArea gives either a desktop **core profile** or a **GLES 3** context: no
`glBegin/glEnd`, no display lists, no fixed-function lighting. Shaders are written in the
common subset of both, and `GlslDialect::header()` prepends `#version 150` or
`#version 300 es`. GL function pointers come from libepoxy, which GTK already loaded, through
`glow`.

### Adding an animation

1. Create `src/animations/mine.rs` with a struct implementing `GlScene` and an `INFO`:

   ```rust
   pub const INFO: AnimationInfo = AnimationInfo {
       id: "mine",
       title_en: "My animation",
       title_tr: "Benim animasyonum",
       create,
   };

   fn create(options: Option<&toml::Value>) -> Box<dyn GlScene> {
       Box::new(Mine::new(super::parse_options(INFO.id, options)))
   }
   ```

2. In `src/animations/mod.rs` add `pub mod mine;` and put `mine::INFO` in `ALL`.
3. If it has options, write a `#[derive(Deserialize, Default)]` struct; it is read from
   `[animations.mine]`. `config.rs` needs no change.

The settings window's list is filled from `ALL`; nothing else needs touching.

If all you want is a fragment shader, do not implement `GlScene` by hand:
`fullscreen::FullscreenScene::new(FRAGMENT, speed)` runs a shader with `u_time` and
`u_resolution` uniforms. See `plasma.rs`, about 20 lines. `cargo test` cannot see the result, so
run `tools/gui-tests.sh`: it renders every animation offscreen, checks GL errors and coverage,
times the frames, and `GIS_RENDER_DUMP=/dir` saves them as PNG.

## Language switching

The interface has two languages, English and Turkish, chosen by the `language` setting
(`auto` follows the desktop's language, from `LANGUAGE`, `LC_ALL`, `LC_MESSAGES`, `LANG`). Log lines
are always English.

- Messages live in one `messages!` table in `src/i18n.rs`. `Msg::text` is an exhaustive
  `match`, so a message missing one of the languages does not compile.
- Every user-visible string in the settings window is set in `Ui::retranslate()`, so switching
  the language updates the open window without a restart.
- **Do not call `retranslate()` from inside a notify handler.** Rebuilding a combo model resets
  its selection and raises `notify::selected`. GObject holds back notifications raised while a
  handler for the same property runs and delivers them afterwards, when a "guard" flag has long
  been released, so the handler ran again, rebuilt the model again, and looped forever. The
  language handler defers `retranslate()` to an idle callback, the language row's own model is
  never rebuilt (its labels are fixed and bilingual), and the handler returns at once if the
  setting has not changed. `settings::tests` covers this; run it with `tools/gui-tests.sh`.
- Animation names are per language on `AnimationInfo` (`title_en`, `title_tr`); `.desktop`
  files carry `[tr]` keys.

### Adding a language

1. Add a variant to `Lang` and its locale prefix to `Lang::from_locales`.
2. `pick()` and the `messages!` table then fail to compile until every message has the new text.
3. Add the language to `i18n::SETTINGS` and `LANGUAGE_NAMES`, a `title_xx` to `AnimationInfo`, and
   the `[xx]` keys to the two `.desktop.in` files.

Why not gettext: two languages and about thirty short strings do not justify `.po` files, a
build-time compiler step and a libintl binding, and gettext cannot switch language in a running
window without restarting it.

## Window teardown

Closing the stage does not destroy its windows inside the handler that decided to close. On
2026-09-28, right after a real idle show, the daemon crashed in `libgtk-4` (kernel log:
segfault) two seconds after closing: the window whose motion event was still being dispatched was
destroyed in that handler, followed by a Wayland round trip. `Inner::close` now only records the
decision and defers `destroy()` and the round trip to the next main-loop turn
(`glib::idle_add_local_once`). Seven consecutive show/close cycles and later real use ran without
a crash. An RDP connection was starting at the same moment, which may have been the trigger; the
deferred teardown is correct either way.

## Design decisions

- **TOML, not GSettings.** Options are added module by module. With GSettings each new key needs
  a schema change, `glib-compile-schemas` and installing the compiled schema under
  `~/.local/share/glib-2.0/schemas`. In the TOML file each module parses its own
  `[animations.<id>]` table. The file is editable by hand, and removing the program is deleting
  one directory. `gio::FileMonitor` supplies the change notification GSettings would give for
  free; the settings window writes atomically (temporary file plus `rename`).
- **gio's D-Bus, not zbus.** GTK already runs on the GLib main loop; gio's D-Bus works on the same
  loop and thread, with no separate async runtime.
- **No `epoxy` crate.** It depends on a yanked release of `xml-rs`, so it does not build. Reading the `epoxy_gl*` pointers out of GTK's already-loaded libepoxy with `libloading`
  does the same job.
- **Intel GPU, not NVIDIA.** No PRIME offload variables are set; GTK's context opens on the
  default EGL device.
- **The stage lives in the daemon.** A separate process per show would keep the daemon small,
  but would give the stage window the focus problems described above. The cost is the daemon's
  memory after the first show: see [measurements](measurements.md).
