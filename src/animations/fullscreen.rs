//! Scenes that are a single fragment shader over the whole screen. A module supplies
//! the shader body and options; this owns the GL objects, uniforms and the draw.
//! No MSAA: a per-pixel shader has no geometry edges to smooth.

use glow::HasContext;

use super::{Frame, GlScene, GlslDialect};

/// Declared for every fragment shader; the body only adds helpers and `main()`.
const FRAGMENT_PRELUDE: &str = "
uniform vec2 u_resolution;
uniform float u_time;
out vec4 frag_color;
";

/// A vertex-less triangle that covers the screen, so no buffers are needed.
const VERTEX_SHADER: &str = "
void main() {
    vec2 corner = vec2(float((gl_VertexID << 1) & 2), float(gl_VertexID & 2));
    gl_Position = vec4(corner * 2.0 - 1.0, 0.0, 1.0);
}
";

struct Gpu {
    program: glow::Program,
    vao: glow::VertexArray,
    time: Option<glow::UniformLocation>,
    resolution: Option<glow::UniformLocation>,
}

pub struct FullscreenScene {
    fragment: &'static str,
    speed: f32,
    gpu: Option<Gpu>,
}

impl FullscreenScene {
    /// `speed` scales `u_time`; 1.0 is the module's design speed.
    pub fn new(fragment: &'static str, speed: f32) -> Self {
        Self { fragment, speed, gpu: None }
    }
}

impl GlScene for FullscreenScene {
    fn realize(&mut self, gl: &glow::Context, glsl: GlslDialect) -> Result<(), String> {
        let fragment = format!("{FRAGMENT_PRELUDE}{}", self.fragment);
        let program = super::compile_program(gl, glsl, VERTEX_SHADER, &fragment, &[])?;
        unsafe {
            let vao = gl.create_vertex_array()?;
            self.gpu = Some(Gpu {
                program,
                vao,
                time: gl.get_uniform_location(program, "u_time"),
                resolution: gl.get_uniform_location(program, "u_resolution"),
            });
        }
        Ok(())
    }

    fn render(&mut self, gl: &glow::Context, frame: &Frame) {
        let Some(gpu) = &self.gpu else { return };
        unsafe {
            gl.disable(glow::DEPTH_TEST);
            gl.disable(glow::CULL_FACE);
            gl.use_program(Some(gpu.program));
            gl.uniform_2_f32(gpu.resolution.as_ref(), frame.width as f32, frame.height as f32);
            gl.uniform_1_f32(gpu.time.as_ref(), (frame.elapsed * f64::from(self.speed)) as f32);
            gl.bind_vertex_array(Some(gpu.vao));
            gl.draw_arrays(glow::TRIANGLES, 0, 3);
            gl.bind_vertex_array(None);
        }
    }

    fn unrealize(&mut self, gl: &glow::Context) {
        if let Some(gpu) = self.gpu.take() {
            unsafe {
                gl.delete_vertex_array(gpu.vao);
                gl.delete_program(gpu.program);
            }
        }
    }

    fn msaa_samples(&self) -> i32 {
        0
    }
}

/// `[animations.<id>]` shared by the shader scenes.
#[derive(Debug, Clone, serde::Deserialize)]
#[serde(default)]
pub struct SpeedOptions {
    /// 1.0 is the design speed.
    pub speed: f32,
}

impl Default for SpeedOptions {
    fn default() -> Self {
        Self { speed: 1.0 }
    }
}
