//! A black screen that looks like the display is off. It is drawn a few times and
//! then left alone (`animated() == false`), so it costs nothing while it is up.

use glow::HasContext;

use super::{AnimationInfo, Frame, GlScene, GlslDialect};

pub const INFO: AnimationInfo = AnimationInfo {
    id: "blank",
    title: "Kapalı ekran (siyah)",
    create,
};

fn create(_: Option<&toml::Value>) -> Box<dyn GlScene> {
    Box::new(Blank)
}

struct Blank;

impl GlScene for Blank {
    fn realize(&mut self, _: &glow::Context, _: GlslDialect) -> Result<(), String> {
        Ok(())
    }

    fn render(&mut self, gl: &glow::Context, _: &Frame) {
        unsafe {
            gl.clear_color(0.0, 0.0, 0.0, 1.0);
            gl.clear(glow::COLOR_BUFFER_BIT);
        }
    }

    fn unrealize(&mut self, _: &glow::Context) {}

    fn msaa_samples(&self) -> i32 {
        0
    }

    fn animated(&self) -> bool {
        false
    }
}
