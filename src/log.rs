use std::sync::atomic::{AtomicBool, Ordering};

static VERBOSE: AtomicBool = AtomicBool::new(false);

pub fn set_verbose(on: bool) {
    VERBOSE.store(on, Ordering::Relaxed);
}

pub fn verbose() -> bool {
    VERBOSE.load(Ordering::Relaxed)
}

/// CLOCK_MONOTONIC in ms: the clock Mutter stamps Wayland input events with,
/// so event-to-close latency can be computed from our own log lines.
pub fn now_ms() -> f64 {
    gtk::glib::monotonic_time() as f64 / 1000.0
}

macro_rules! info {
    ($($arg:tt)*) => {
        eprintln!("[{:.3}] {}", $crate::log::now_ms() / 1000.0, format_args!($($arg)*))
    };
}

macro_rules! debug {
    ($($arg:tt)*) => {
        if $crate::log::verbose() {
            info!($($arg)*)
        }
    };
}
