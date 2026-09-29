//! Animation modules. Each one is a `GlScene` plus an `AnimationInfo` entry in `ALL`;
//! the stage (src/stage.rs) owns the window, frame clock, MSAA target and input.
//! README.md, "Yeni bir animasyon eklemek", walks through adding one.

pub mod aurora;
pub mod fullscreen;
pub mod gears;
pub mod math;
pub mod plasma;
pub mod starfield;
pub mod tunnel;

use glow::HasContext;
use serde::de::DeserializeOwned;

/// Every module the settings window offers, in display order. The first is the default.
pub const ALL: &[AnimationInfo] =
    &[gears::INFO, starfield::INFO, plasma::INFO, tunnel::INFO, aurora::INFO];

pub struct AnimationInfo {
    /// Stored in config.toml (`animation = "…"`) and used as the options table name.
    pub id: &'static str,
    /// Shown in the settings window.
    pub title: &'static str,
    /// Receives `[animations.<id>]` from config.toml, if present.
    pub create: fn(Option<&toml::Value>) -> Box<dyn GlScene>,
}

pub fn find(id: &str) -> &'static AnimationInfo {
    ALL.iter().find(|a| a.id == id).unwrap_or_else(|| {
        info!("unknown animation {id:?}, using {:?}", ALL[0].id);
        &ALL[0]
    })
}

/// A module's view of one frame. Times are in seconds; `delta` is clamped so a
/// stalled frame never makes the scene jump.
pub struct Frame {
    pub elapsed: f64,
    pub delta: f64,
    /// Framebuffer size in device pixels.
    pub width: i32,
    pub height: i32,
}

/// GTK hands GLArea either a desktop core context or a GLES one; shaders are
/// written once in the common subset and get the matching header.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GlslDialect {
    Core150,
    Es300,
}

impl GlslDialect {
    pub fn header(self) -> &'static str {
        match self {
            GlslDialect::Core150 => "#version 150\n",
            GlslDialect::Es300 => "#version 300 es\nprecision highp float;\n",
        }
    }
}

/// All methods run with the scene's GL context current. The stage binds a
/// multisampled framebuffer and sets the viewport before `render`.
pub trait GlScene {
    fn realize(&mut self, gl: &glow::Context, glsl: GlslDialect) -> Result<(), String>;
    fn render(&mut self, gl: &glow::Context, frame: &Frame);
    fn unrealize(&mut self, gl: &glow::Context);

    /// Requested MSAA sample count; the stage clamps it to what the driver allows.
    fn msaa_samples(&self) -> i32 {
        4
    }
}

/// Parses `[animations.<id>]`; a bad table logs and falls back to defaults rather
/// than leaving the screen black.
pub fn parse_options<T: DeserializeOwned + Default>(id: &str, value: Option<&toml::Value>) -> T {
    match value {
        None => T::default(),
        Some(v) => v.clone().try_into().unwrap_or_else(|e| {
            info!("[animations.{id}]: {e}; using defaults");
            T::default()
        }),
    }
}

pub fn compile_program(
    gl: &glow::Context,
    glsl: GlslDialect,
    vertex: &str,
    fragment: &str,
    attributes: &[(u32, &str)],
) -> Result<glow::Program, String> {
    unsafe {
        let program = gl.create_program()?;
        let mut shaders = Vec::new();
        for (kind, body) in [(glow::VERTEX_SHADER, vertex), (glow::FRAGMENT_SHADER, fragment)] {
            let shader = gl.create_shader(kind)?;
            gl.shader_source(shader, &format!("{}{body}", glsl.header()));
            gl.compile_shader(shader);
            if !gl.get_shader_compile_status(shader) {
                return Err(format!("shader compile: {}", gl.get_shader_info_log(shader)));
            }
            gl.attach_shader(program, shader);
            shaders.push(shader);
        }
        // GLSL 1.50 has no layout(location), so locations are fixed before linking.
        for &(index, name) in attributes {
            gl.bind_attrib_location(program, index, name);
        }
        gl.link_program(program);
        for shader in shaders {
            gl.detach_shader(program, shader);
            gl.delete_shader(shader);
        }
        if !gl.get_program_link_status(program) {
            return Err(format!("program link: {}", gl.get_program_info_log(program)));
        }
        Ok(program)
    }
}
