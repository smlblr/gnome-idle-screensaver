//! The full-screen window an animation runs in: one per monitor, each with a
//! GLArea driven by the frame clock, rendering through a multisampled framebuffer.
//! Also decides when the screensaver closes.

use std::cell::{Cell, RefCell};
use std::ffi::c_void;
use std::rc::Rc;
use std::time::{Duration, Instant};

use glow::HasContext;
use gtk::prelude::*;
use gtk::{gdk, gio, glib};

use crate::animations::{self, Frame, GlScene, GlslDialect};
use crate::config::Config;
use crate::dbus::IdleMonitor;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    /// Opened by the daemon after the idle timeout: the first real input closes it.
    Idle,
    /// Opened on request: input is ignored briefly so the click that asked for it
    /// (and the hand still on the mouse) does not dismiss it at once.
    Preview,
}

#[derive(Clone, Copy, Debug)]
pub enum CloseReason {
    Key,
    Button,
    Scroll,
    Motion,
    /// Mutter saw input that never reached our windows (keyboard focus elsewhere).
    InputElsewhere,
    Timeout,
    Requested,
}

/// Pointer travel in logical px, measured from where a motion burst began,
/// that counts as deliberate. Smaller movement is treated as desk or sensor jitter.
const MOTION_THRESHOLD: f64 = 24.0;
/// Stillness that ends a motion burst, so slow drift never accumulates to the threshold.
const MOTION_QUIET: Duration = Duration::from_millis(800);
const PREVIEW_GRACE: Duration = Duration::from_millis(800);
/// After Mutter reports activity, how long to wait for the same input to reach GTK,
/// and how far back an input we already saw may count as that same input.
const ELSEWHERE_WAIT: Duration = Duration::from_millis(150);
const ELSEWHERE_LOOKBACK: Duration = Duration::from_millis(300);
/// Activity Mutter reports while our windows are still mapping is not trusted to
/// mean input went elsewhere; direct input to our windows still closes at once.
const ELSEWHERE_SETTLE: Duration = Duration::from_millis(300);
const STATS_INTERVAL_S: f64 = 5.0;

type CloseCallback = Box<dyn FnOnce(CloseReason)>;

pub struct Stage {
    inner: Rc<Inner>,
}

struct Inner {
    mode: Mode,
    opened_at: Instant,
    windows: RefCell<Vec<gtk::Window>>,
    closed: Cell<bool>,
    on_closed: RefCell<Option<CloseCallback>>,
    motion: Cell<Option<MotionBurst>>,
    pointer: Cell<Option<(f64, f64)>>,
    last_input: Cell<Option<Instant>>,
    idle_monitor: Option<IdleMonitor>,
    active_watch: Cell<Option<u32>>,
    watch_subscription: RefCell<Option<gio::SignalSubscription>>,
}

#[derive(Clone, Copy)]
struct MotionBurst {
    x: f64,
    y: f64,
    last: Instant,
}

impl Stage {
    /// Opens the animation from `config` on every monitor. `on_closed` runs from
    /// the main loop once all windows are gone.
    pub fn open(
        app: Option<&gtk::Application>,
        config: &Config,
        mode: Mode,
        idle_monitor: Option<IdleMonitor>,
        on_closed: impl FnOnce(CloseReason) + 'static,
    ) -> Stage {
        let inner = Rc::new(Inner {
            mode,
            opened_at: Instant::now(),
            windows: RefCell::default(),
            closed: Cell::new(false),
            on_closed: RefCell::new(Some(Box::new(on_closed))),
            motion: Cell::new(None),
            pointer: Cell::new(None),
            last_input: Cell::new(None),
            idle_monitor,
            active_watch: Cell::new(None),
            watch_subscription: RefCell::default(),
        });

        let display = gdk::Display::default().expect("GTK is initialised before a stage opens");
        install_css(&display);
        let info = animations::find(&config.animation);
        let monitors = display.monitors();
        for i in 0..monitors.n_items() {
            let Some(monitor) = monitors.item(i).and_downcast::<gdk::Monitor>() else {
                continue;
            };
            let scene = (info.create)(config.animation_options(info.id));
            let window = build_window(&inner, app, &monitor, scene);
            inner.windows.borrow_mut().push(window);
        }
        for window in inner.windows.borrow().iter() {
            match mode {
                // present() would also send an xdg-activation request; with no recent
                // input serial Mutter answers it with an "is ready" notification.
                Mode::Idle => window.set_visible(true),
                Mode::Preview => window.present(),
            }
        }
        debug!(
            "stage opened: {} window(s), mode {mode:?}, animation {}",
            monitors.n_items(),
            info.id
        );
        inner.watch_input_elsewhere();
        Stage { inner }
    }

    pub fn close_after(&self, after: Duration) {
        let weak = Rc::downgrade(&self.inner);
        glib::timeout_add_local_once(after, move || {
            if let Some(inner) = weak.upgrade() {
                inner.close(CloseReason::Timeout, None);
            }
        });
    }
}

impl Drop for Stage {
    fn drop(&mut self) {
        self.inner.close(CloseReason::Requested, None);
    }
}

impl Inner {
    fn in_grace(&self) -> bool {
        self.mode == Mode::Preview && self.opened_at.elapsed() < PREVIEW_GRACE
    }

    fn on_input(self: &Rc<Self>, reason: CloseReason, event_time: u32) {
        self.last_input.set(Some(Instant::now()));
        if !self.in_grace() {
            self.close(reason, Some(event_time));
        }
    }

    fn on_motion(self: &Rc<Self>, x: f64, y: f64, event_time: u32) {
        let now = Instant::now();
        self.last_input.set(Some(now));
        // A new burst starts where the pointer last rested, so a single large jump counts.
        let (rest_x, rest_y) = self.pointer.replace(Some((x, y))).unwrap_or((x, y));
        let burst = match self.motion.get() {
            Some(b) if now - b.last < MOTION_QUIET && !self.in_grace() => b,
            _ => MotionBurst {
                x: rest_x,
                y: rest_y,
                last: now,
            },
        };
        if (x - burst.x).hypot(y - burst.y) >= MOTION_THRESHOLD {
            self.close(CloseReason::Motion, Some(event_time));
        } else {
            self.motion.set(Some(MotionBurst { last: now, ..burst }));
        }
    }

    /// Keyboard focus may stay on another window (Mutter never focuses a new window
    /// over a terminal), so keys would not reach us. Mutter's user-active watch sees
    /// all input; if none of it arrived here, it went elsewhere and we close.
    fn watch_input_elsewhere(self: &Rc<Self>) {
        let Some(monitor) = &self.idle_monitor else {
            return;
        };
        let weak = Rc::downgrade(self);
        let subscription = monitor.subscribe_watch_fired(move |id| {
            let Some(inner) = weak.upgrade() else { return };
            if inner.active_watch.get() == Some(id) {
                inner.active_watch.set(None);
                inner.on_user_active();
            }
        });
        *self.watch_subscription.borrow_mut() = Some(subscription);
        self.arm_active_watch();
    }

    fn arm_active_watch(&self) {
        let (Some(monitor), false) = (&self.idle_monitor, self.closed.get()) else {
            return;
        };
        match monitor.add_user_active_watch() {
            Ok(id) => self.active_watch.set(Some(id)),
            Err(e) => info!("AddUserActiveWatch failed: {e}"),
        }
    }

    fn on_user_active(self: &Rc<Self>) {
        let fired = Instant::now();
        let weak = Rc::downgrade(self);
        glib::timeout_add_local_once(ELSEWHERE_WAIT, move || {
            let Some(inner) = weak.upgrade() else { return };
            if inner.closed.get() {
                return;
            }
            let seen_here = inner
                .last_input
                .get()
                .is_some_and(|t| t + ELSEWHERE_LOOKBACK >= fired);
            let settling = fired.duration_since(inner.opened_at) < ELSEWHERE_SETTLE;
            if seen_here || settling || inner.in_grace() {
                inner.arm_active_watch();
            } else {
                inner.close(CloseReason::InputElsewhere, None);
            }
        });
    }

    fn close(self: &Rc<Self>, reason: CloseReason, event_time: Option<u32>) {
        if self.closed.replace(true) {
            return;
        }
        let handled_ms = crate::log::now_ms();
        if let (Some(monitor), Some(id)) = (&self.idle_monitor, self.active_watch.take()) {
            monitor.remove_watch(id);
        }
        self.watch_subscription.borrow_mut().take();
        let windows = std::mem::take(&mut *self.windows.borrow_mut());
        let opened_at = self.opened_at;
        let on_closed = self.on_closed.borrow_mut().take();
        // Teardown waits for the next main-loop turn: destroying the window whose event is
        // still being dispatched, then a Wayland round trip, segfaulted in libgtk (2026-09-28).
        glib::idle_add_local_once(move || {
            let display = windows.first().map(WidgetExt::display);
            for window in windows {
                window.destroy();
            }
            // Once the round trip returns Mutter has processed the unmap; it repaints on the next vblank.
            if let Some(display) = display {
                display.sync();
            }
            let acked_ms = crate::log::now_ms();
            match event_time {
                Some(t) => {
                    let event_ms = wayland_time_to_monotonic_ms(t, handled_ms);
                    info!(
                        "stage closed ({reason:?}): input→handler {:.1} ms, input→compositor ack {:.1} ms",
                        handled_ms - event_ms,
                        acked_ms - event_ms
                    );
                }
                None => info!(
                    "stage closed ({reason:?}) after {:.1} s",
                    opened_at.elapsed().as_secs_f64()
                ),
            }
            if let Some(on_closed) = on_closed {
                on_closed(reason);
            }
        });
    }
}

/// Wayland event times are CLOCK_MONOTONIC milliseconds truncated to 32 bits.
fn wayland_time_to_monotonic_ms(event: u32, now_ms: f64) -> f64 {
    let now32 = (now_ms as u64 & 0xffff_ffff) as u32;
    now_ms.floor() - f64::from(now32.wrapping_sub(event))
}

fn install_css(display: &gdk::Display) {
    thread_local!(static INSTALLED: Cell<bool> = const { Cell::new(false) });
    if INSTALLED.replace(true) {
        return;
    }
    let provider = gtk::CssProvider::new();
    provider.load_from_string("window.gis-stage { background-color: black; }");
    gtk::style_context_add_provider_for_display(
        display,
        &provider,
        gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
    );
}

fn build_window(
    inner: &Rc<Inner>,
    app: Option<&gtk::Application>,
    monitor: &gdk::Monitor,
    scene: Box<dyn GlScene>,
) -> gtk::Window {
    let window = gtk::Window::builder()
        .title("Ekran koruyucu")
        .decorated(false)
        .css_classes(["gis-stage"])
        .build();
    if let Some(app) = app {
        window.set_application(Some(app));
    }

    let area = gtk::GLArea::new();
    area.set_has_depth_buffer(false);
    area.set_auto_render(false);
    area.set_cursor_from_name(Some("none"));
    attach_renderer(&area, scene);
    window.set_child(Some(&area));

    attach_input(inner, &window);
    let weak = Rc::downgrade(inner);
    window.connect_close_request(move |_| {
        if let Some(inner) = weak.upgrade() {
            inner.close(CloseReason::Requested, None);
        }
        glib::Propagation::Proceed
    });
    let opened_at = inner.opened_at;
    window.connect_is_active_notify(move |w| {
        debug!(
            "stage window active={} at +{} ms",
            w.is_active(),
            opened_at.elapsed().as_millis()
        );
    });
    let weak = Rc::downgrade(inner);
    window.connect_map(move |w| {
        debug!(
            "stage window mapped at +{} ms",
            opened_at.elapsed().as_millis()
        );
        let w = w.downgrade();
        let weak = weak.clone();
        glib::timeout_add_local_once(Duration::from_millis(1000), move || {
            if let (Some(w), Some(_)) = (w.upgrade(), weak.upgrade()) {
                info!("stage window focus 1 s after map: active={}", w.is_active());
            }
        });
    });

    window.fullscreen_on_monitor(monitor);
    window
}

fn attach_input(inner: &Rc<Inner>, window: &gtk::Window) {
    let key = gtk::EventControllerKey::new();
    key.set_propagation_phase(gtk::PropagationPhase::Capture);
    let weak = Rc::downgrade(inner);
    key.connect_key_pressed(move |c, _, _, _| {
        if let Some(inner) = weak.upgrade() {
            inner.on_input(CloseReason::Key, c.current_event_time());
        }
        glib::Propagation::Stop
    });
    window.add_controller(key);

    let click = gtk::GestureClick::new();
    click.set_button(0);
    click.set_propagation_phase(gtk::PropagationPhase::Capture);
    let weak = Rc::downgrade(inner);
    click.connect_pressed(move |g, _, _, _| {
        if let Some(inner) = weak.upgrade() {
            inner.on_input(CloseReason::Button, g.current_event_time());
        }
    });
    window.add_controller(click);

    let scroll = gtk::EventControllerScroll::new(gtk::EventControllerScrollFlags::BOTH_AXES);
    let weak = Rc::downgrade(inner);
    scroll.connect_scroll(move |c, _, _| {
        if let Some(inner) = weak.upgrade() {
            inner.on_input(CloseReason::Scroll, c.current_event_time());
        }
        glib::Propagation::Stop
    });
    window.add_controller(scroll);

    let motion = gtk::EventControllerMotion::new();
    let weak = Rc::downgrade(inner);
    // The enter position is not trusted as a resting position: under a window that just
    // mapped it can be far from where the pointer really is.
    motion.connect_enter(move |_, _, _| {
        if let Some(inner) = weak.upgrade() {
            inner.last_input.set(Some(Instant::now()));
        }
    });
    let weak = Rc::downgrade(inner);
    motion.connect_motion(move |c, x, y| {
        if let Some(inner) = weak.upgrade() {
            inner.on_motion(x, y, c.current_event_time());
        }
    });
    window.add_controller(motion);
}

struct Target {
    fbo: glow::Framebuffer,
    color: glow::Renderbuffer,
    depth: glow::Renderbuffer,
    size: (i32, i32),
}

#[derive(Default)]
struct Clock {
    last_us: Option<i64>,
    elapsed: f64,
    delta: f64,
}

#[derive(Default)]
struct Stats {
    since_us: Option<i64>,
    frames: u32,
    logged_refresh: bool,
}

struct Renderer {
    scene: Box<dyn GlScene>,
    gl: Option<glow::Context>,
    samples: i32,
    target: Option<Target>,
    size: (i32, i32),
    clock: Clock,
    stats: Stats,
    checked_errors: bool,
    rendered: u32,
}

fn attach_renderer(area: &gtk::GLArea, scene: Box<dyn GlScene>) {
    let renderer = Rc::new(RefCell::new(Renderer {
        scene,
        gl: None,
        samples: 0,
        target: None,
        size: (0, 0),
        clock: Clock::default(),
        stats: Stats::default(),
        checked_errors: false,
        rendered: 0,
    }));

    let r = renderer.clone();
    area.connect_realize(move |area| {
        area.make_current();
        if let Some(e) = area.error() {
            info!("GL context unavailable, showing black: {e}");
            return;
        }
        let Some(context) = area.context() else {
            return;
        };
        let dialect = if context.uses_es() {
            GlslDialect::Es300
        } else {
            GlslDialect::Core150
        };
        let gl = match load_gl() {
            Ok(gl) => gl,
            Err(e) => {
                info!("cannot load GL entry points: {e}");
                return;
            }
        };
        let mut r = r.borrow_mut();
        let max_samples = unsafe { gl.get_parameter_i32(glow::MAX_SAMPLES) };
        r.samples = r.scene.msaa_samples().clamp(0, max_samples);
        unsafe {
            info!(
                "GL: {} | {} | {:?} | MSAA {}x (max {max_samples})",
                gl.get_parameter_string(glow::RENDERER),
                gl.get_parameter_string(glow::VERSION),
                dialect,
                r.samples
            );
        }
        if let Err(e) = r.scene.realize(&gl, dialect) {
            info!("animation failed to initialise, showing black: {e}");
            return;
        }
        r.gl = Some(gl);
    });

    let r = renderer.clone();
    area.connect_unrealize(move |area| {
        area.make_current();
        let mut r = r.borrow_mut();
        let Some(gl) = r.gl.take() else { return };
        if area.error().is_none() {
            r.scene.unrealize(&gl);
            if let Some(t) = r.target.take() {
                delete_target(&gl, t);
            }
        }
    });

    let r = renderer.clone();
    area.connect_resize(move |_, width, height| {
        r.borrow_mut().size = (width, height);
    });

    let r = renderer.clone();
    area.connect_render(move |_, _| {
        let mut r = r.borrow_mut();
        let Renderer {
            scene,
            gl,
            samples,
            target,
            size,
            clock,
            stats,
            checked_errors,
            rendered,
        } = &mut *r;
        let Some(gl) = gl.as_ref() else {
            return glib::Propagation::Proceed;
        };
        let (width, height) = *size;
        if width <= 0 || height <= 0 {
            return glib::Propagation::Stop;
        }
        unsafe {
            let screen = gl.get_parameter_framebuffer(glow::DRAW_FRAMEBUFFER_BINDING);
            if target.as_ref().is_some_and(|t| t.size != *size) {
                delete_target(gl, target.take().unwrap());
            }
            if target.is_none() {
                match create_target(gl, *size, *samples) {
                    Ok(t) => *target = Some(t),
                    Err(e) => {
                        info!("cannot create {samples}x framebuffer: {e}");
                        return glib::Propagation::Stop;
                    }
                }
            }
            let frame = Frame {
                elapsed: clock.elapsed,
                delta: clock.delta,
                width,
                height,
            };
            draw(gl, scene.as_mut(), target.as_ref().unwrap(), screen, &frame);
            if !*checked_errors {
                *checked_errors = true;
                let error = gl.get_error();
                if error != glow::NO_ERROR {
                    info!("GL error 0x{error:x} after first frame");
                }
            }
        }
        stats.frames += 1;
        *rendered += 1;
        glib::Propagation::Stop
    });

    let r = renderer;
    area.add_tick_callback(move |area, frame_clock| {
        let now_us = frame_clock.frame_time();
        let mut r = r.borrow_mut();
        let clock = &mut r.clock;
        if let Some(last) = clock.last_us {
            clock.delta = ((now_us - last) as f64 / 1e6).clamp(0.0, 0.1);
            clock.elapsed += clock.delta;
        }
        clock.last_us = Some(now_us);
        if crate::log::verbose() {
            log_stats(&mut r, frame_clock, now_us);
        }
        // Three frames so the buffer chain (GTK's plus ours) has been filled at least once.
        if !r.scene.animated() && r.rendered >= 3 {
            return glib::ControlFlow::Break;
        }
        area.queue_render();
        glib::ControlFlow::Continue
    });
}

fn log_stats(r: &mut Renderer, frame_clock: &gdk::FrameClock, now_us: i64) {
    let size = r.size;
    let stats = &mut r.stats;
    if !stats.logged_refresh
        && let Some(t) = frame_clock.current_timings()
    {
        let interval = t.refresh_interval();
        if interval > 0 {
            stats.logged_refresh = true;
            debug!(
                "monitor refresh interval {interval} µs ({:.2} Hz)",
                1e6 / interval as f64
            );
        }
    }
    let since = *stats.since_us.get_or_insert(now_us);
    let seconds = (now_us - since) as f64 / 1e6;
    if seconds >= STATS_INTERVAL_S {
        debug!(
            "stage {}x{}: {:.2} fps rendered, frame clock {:.2} fps",
            size.0,
            size.1,
            f64::from(stats.frames) / seconds,
            frame_clock.fps()
        );
        stats.since_us = Some(now_us);
        stats.frames = 0;
    }
}

/// Renders `frame` into the multisampled `target`, then resolves the samples into
/// `screen` (GtkGLArea's own framebuffer, which cannot be multisampled).
unsafe fn draw(
    gl: &glow::Context,
    scene: &mut dyn GlScene,
    target: &Target,
    screen: Option<glow::Framebuffer>,
    frame: &Frame,
) {
    unsafe {
        let (width, height) = target.size;
        gl.bind_framebuffer(glow::FRAMEBUFFER, Some(target.fbo));
        gl.viewport(0, 0, width, height);
        scene.render(gl, frame);
        gl.bind_framebuffer(glow::READ_FRAMEBUFFER, Some(target.fbo));
        gl.bind_framebuffer(glow::DRAW_FRAMEBUFFER, screen);
        gl.blit_framebuffer(
            0,
            0,
            width,
            height,
            0,
            0,
            width,
            height,
            glow::COLOR_BUFFER_BIT,
            glow::NEAREST,
        );
        gl.bind_framebuffer(glow::FRAMEBUFFER, screen);
    }
}

unsafe fn create_target(
    gl: &glow::Context,
    size: (i32, i32),
    samples: i32,
) -> Result<Target, String> {
    unsafe {
        let (width, height) = size;
        let fbo = gl.create_framebuffer()?;
        gl.bind_framebuffer(glow::FRAMEBUFFER, Some(fbo));
        let color = gl.create_renderbuffer()?;
        gl.bind_renderbuffer(glow::RENDERBUFFER, Some(color));
        gl.renderbuffer_storage_multisample(
            glow::RENDERBUFFER,
            samples,
            glow::RGBA8,
            width,
            height,
        );
        gl.framebuffer_renderbuffer(
            glow::FRAMEBUFFER,
            glow::COLOR_ATTACHMENT0,
            glow::RENDERBUFFER,
            Some(color),
        );
        let depth = gl.create_renderbuffer()?;
        gl.bind_renderbuffer(glow::RENDERBUFFER, Some(depth));
        gl.renderbuffer_storage_multisample(
            glow::RENDERBUFFER,
            samples,
            glow::DEPTH_COMPONENT24,
            width,
            height,
        );
        gl.framebuffer_renderbuffer(
            glow::FRAMEBUFFER,
            glow::DEPTH_ATTACHMENT,
            glow::RENDERBUFFER,
            Some(depth),
        );
        gl.bind_renderbuffer(glow::RENDERBUFFER, None);
        let target = Target {
            fbo,
            color,
            depth,
            size,
        };
        let status = gl.check_framebuffer_status(glow::FRAMEBUFFER);
        if status != glow::FRAMEBUFFER_COMPLETE {
            delete_target(gl, target);
            return Err(format!("framebuffer incomplete: 0x{status:x}"));
        }
        debug!("render target {width}x{height}, {samples} samples");
        Ok(target)
    }
}

fn delete_target(gl: &glow::Context, t: Target) {
    unsafe {
        gl.delete_framebuffer(t.fbo);
        gl.delete_renderbuffer(t.color);
        gl.delete_renderbuffer(t.depth);
    }
}

/// GL entry points from libepoxy, which GTK itself renders with: its `epoxy_gl*`
/// pointers dispatch to whichever context is current, desktop GL or GLES alike.
fn load_gl() -> Result<glow::Context, String> {
    let lib = unsafe { libloading::Library::new("libepoxy.so.0") }.map_err(|e| e.to_string())?;
    let gl = unsafe {
        glow::Context::from_loader_function(|name| {
            let symbol = format!("epoxy_{name}\0");
            match lib.get::<*const *const c_void>(symbol.as_bytes()) {
                Ok(slot) => **slot,
                Err(_) => std::ptr::null(),
            }
        })
    };
    // GTK links libepoxy, so dlclose() here only drops our extra reference.
    drop(lib);
    Ok(gl)
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::*;

    /// One frame of every animation through `draw()` at the first monitor's size, in a
    /// GL context GTK creates the same way as for GtkGLArea, without opening a window.
    /// Run via tools/gui-tests.sh; GIS_RENDER_DUMP=<dir> also saves PNGs.
    #[test]
    #[ignore = "needs a graphical session and a GPU"]
    fn renders_every_animation_offscreen() {
        gtk::init().expect("graphical session");
        let display = gdk::Display::default().unwrap();
        let context = display.create_gl_context().expect("GL context");
        context.realize().expect("realize GL context");
        context.make_current();
        let dialect = if context.uses_es() {
            GlslDialect::Es300
        } else {
            GlslDialect::Core150
        };
        let gl = load_gl().unwrap();
        let monitor = display
            .monitors()
            .item(0)
            .and_downcast::<gdk::Monitor>()
            .unwrap();
        let (geometry, scale) = (monitor.geometry(), monitor.scale_factor());
        let size = (geometry.width() * scale, geometry.height() * scale);
        unsafe {
            println!(
                "GL: {} | {} | {dialect:?} | {}x{}",
                gl.get_parameter_string(glow::RENDERER),
                gl.get_parameter_string(glow::VERSION),
                size.0,
                size.1
            );
            // Stands in for GtkGLArea's framebuffer, an RGBA8 texture.
            let texture = gl.create_texture().unwrap();
            gl.bind_texture(glow::TEXTURE_2D, Some(texture));
            gl.tex_storage_2d(glow::TEXTURE_2D, 1, glow::RGBA8, size.0, size.1);
            let screen = gl.create_framebuffer().unwrap();
            gl.bind_framebuffer(glow::FRAMEBUFFER, Some(screen));
            gl.framebuffer_texture_2d(
                glow::FRAMEBUFFER,
                glow::COLOR_ATTACHMENT0,
                glow::TEXTURE_2D,
                Some(texture),
                0,
            );
            assert_eq!(
                gl.check_framebuffer_status(glow::FRAMEBUFFER),
                glow::FRAMEBUFFER_COMPLETE
            );

            for info in animations::ALL {
                let mut scene = (info.create)(None);
                scene.realize(&gl, dialect).expect("scene realize");
                let samples = scene
                    .msaa_samples()
                    .clamp(0, gl.get_parameter_i32(glow::MAX_SAMPLES));
                let target = create_target(&gl, size, samples).expect("MSAA target");
                let mut frame = Frame {
                    elapsed: 0.0,
                    delta: 1.0 / 60.0,
                    width: size.0,
                    height: size.1,
                };
                draw(&gl, scene.as_mut(), &target, Some(screen), &frame);
                gl.finish();
                let frames = 120;
                let start = Instant::now();
                for i in 0..frames {
                    frame.elapsed = 3.0 + f64::from(i) / 60.0;
                    draw(&gl, scene.as_mut(), &target, Some(screen), &frame);
                    gl.finish();
                }
                let per_frame_ms = start.elapsed().as_secs_f64() * 1000.0 / f64::from(frames);
                assert_eq!(gl.get_error(), glow::NO_ERROR, "{}: GL error", info.id);

                let mut pixels = vec![0u8; (size.0 * size.1 * 4) as usize];
                gl.bind_framebuffer(glow::READ_FRAMEBUFFER, Some(screen));
                gl.read_pixels(
                    0,
                    0,
                    size.0,
                    size.1,
                    glow::RGBA,
                    glow::UNSIGNED_BYTE,
                    glow::PixelPackData::Slice(Some(&mut pixels)),
                );
                let lit = pixels
                    .as_chunks::<4>()
                    .0
                    .iter()
                    .filter(|p| p[..3].iter().any(|&c| c > 16))
                    .count();
                let coverage = lit as f64 / f64::from(size.0 * size.1);
                println!(
                    "{}: {samples}x MSAA, {per_frame_ms:.2} ms/frame with the GPU drained each frame, {:.1}% of pixels lit",
                    info.id,
                    coverage * 100.0
                );
                if scene.animated() {
                    assert!(coverage > 0.01, "{}: frame is (almost) black", info.id);
                } else {
                    assert_eq!(lit, 0, "{}: a still black scene drew something", info.id);
                }
                if let Some(dir) = std::env::var_os("GIS_RENDER_DUMP") {
                    save_png(
                        &pixels,
                        size,
                        &Path::new(&dir).join(format!("{}.png", info.id)),
                    );
                }
                delete_target(&gl, target);
                scene.unrealize(&gl);
            }
        }
    }

    fn save_png(rgba: &[u8], (width, height): (i32, i32), path: &Path) {
        // glReadPixels rows run bottom-up.
        let stride = width as usize * 4;
        let flipped: Vec<u8> = rgba.chunks_exact(stride).rev().flatten().copied().collect();
        let bytes = glib::Bytes::from_owned(flipped);
        gdk::MemoryTexture::new(width, height, gdk::MemoryFormat::R8g8b8a8, &bytes, stride)
            .save_to_png(path)
            .unwrap();
    }
}
