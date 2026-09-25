/*
 * Copyright (C) 1999-2001  Brian Paul   All Rights Reserved.
 *
 * SPDX-License-Identifier: MIT
 */

//! The three gears of glxgears (Mesa demos, src/xdemos/glxgears.c), ported from
//! display lists and fixed-function lighting to VBOs and GLSL. `gear()` keeps the
//! original's glBegin/glEnd structure so it can be diffed against the C source;
//! the full MIT text is in LICENSES/MIT-mesa-demos.txt.
//!
//! Additions over the original: MSAA (done by the stage), a projection that keeps
//! the whole scene on any aspect ratio, and a slowly orbiting camera.

use std::f32::consts::PI;

use glow::HasContext;
use serde::Deserialize;

use super::math::Mat4;
use super::{AnimationInfo, Frame, GlScene, GlslDialect};

pub const INFO: AnimationInfo = AnimationInfo {
    id: "gears",
    title: "Çarklar",
    create,
};

fn create(options: Option<&toml::Value>) -> Box<dyn GlScene> {
    Box::new(Gears::new(super::parse_options(INFO.id, options)))
}

/// `[animations.gears]` in config.toml. Not in the settings window yet.
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct GearsOptions {
    /// Degrees per second of the big gear; glxgears uses 70.
    pub rotation_speed: f32,
    /// Degrees per second the camera orbits the scene's vertical axis.
    pub camera_speed: f32,
    /// Red, green, blue gear colours from glxgears, linear RGB 0–1.
    pub colors: [[f32; 3]; 3],
    pub msaa_samples: i32,
}

impl Default for GearsOptions {
    fn default() -> Self {
        Self {
            rotation_speed: 70.0,
            camera_speed: 5.0,
            colors: [[0.8, 0.1, 0.0], [0.0, 0.8, 0.2], [0.2, 0.2, 1.0]],
            msaa_samples: 4,
        }
    }
}

/// gear(inner_radius, outer_radius, width, teeth, tooth_depth) arguments.
#[derive(Clone, Copy)]
struct GearSpec {
    inner_radius: f32,
    outer_radius: f32,
    width: f32,
    teeth: i32,
    tooth_depth: f32,
}

/// Where draw() puts a gear and how its angle follows the big gear's.
struct Placement {
    spec: GearSpec,
    x: f32,
    y: f32,
    angle_factor: f32,
    angle_offset: f32,
}

const PLACEMENTS: [Placement; 3] = [
    Placement {
        spec: GearSpec { inner_radius: 1.0, outer_radius: 4.0, width: 1.0, teeth: 20, tooth_depth: 0.7 },
        x: -3.0,
        y: -2.0,
        angle_factor: 1.0,
        angle_offset: 0.0,
    },
    Placement {
        spec: GearSpec { inner_radius: 0.5, outer_radius: 2.0, width: 2.0, teeth: 10, tooth_depth: 0.7 },
        x: 3.1,
        y: -2.0,
        angle_factor: -2.0,
        angle_offset: -9.0,
    },
    Placement {
        spec: GearSpec { inner_radius: 1.3, outer_radius: 2.0, width: 0.5, teeth: 10, tooth_depth: 0.7 },
        x: -3.1,
        y: 4.2,
        angle_factor: -2.0,
        angle_offset: -25.0,
    },
];

/// glxgears' view: eye 40 units away, glFrustum near 5 / far 60, view_rotx 20, view_roty 30.
const EYE_DISTANCE: f32 = 40.0;
const NEAR: f32 = 5.0;
const FAR: f32 = 60.0;
const VIEW_ROT_X: f32 = 20.0;
const VIEW_ROT_Y: f32 = 30.0;
/// Light 0 at (5, 5, 10, 0), set while the modelview was identity: an eye-space direction.
const LIGHT_DIR: [f32; 3] = [5.0, 5.0, 10.0];
/// Empty space kept around the scene's bounding sphere.
const FIT_MARGIN: f32 = 1.06;
const PITCH_SWING_DEG: f32 = 12.0;
const PITCH_PERIOD_S: f64 = 53.0;

const VERTEX_SHADER: &str = r"
in vec3 a_position;
in vec3 a_normal;
uniform mat4 u_projection;
uniform mat4 u_model_view;
uniform mat3 u_normal_matrix;
out vec3 v_normal;
void main() {
    v_normal = u_normal_matrix * a_normal;
    gl_Position = u_projection * u_model_view * vec4(a_position, 1.0);
}
";

/// GL_LIGHT0 defaults with glMaterial AMBIENT_AND_DIFFUSE: 0.2 global ambient plus
/// white diffuse; the material has no specular, so neither does this.
const FRAGMENT_SHADER: &str = r"
in vec3 v_normal;
uniform vec3 u_color;
uniform vec3 u_light_dir;
out vec4 frag_color;
void main() {
    float diffuse = max(dot(normalize(v_normal), u_light_dir), 0.0);
    frag_color = vec4(min(u_color * (0.2 + diffuse), 1.0), 1.0);
}
";

const ATTR_POSITION: u32 = 0;
const ATTR_NORMAL: u32 = 1;
const FLOATS_PER_VERTEX: usize = 6;

struct GpuMesh {
    vao: glow::VertexArray,
    vbo: glow::Buffer,
    vertices: i32,
}

struct Uniforms {
    projection: Option<glow::UniformLocation>,
    model_view: Option<glow::UniformLocation>,
    normal_matrix: Option<glow::UniformLocation>,
    color: Option<glow::UniformLocation>,
    light_dir: Option<glow::UniformLocation>,
}

pub struct Gears {
    options: GearsOptions,
    angle: f32,
    gpu: Option<(glow::Program, Uniforms, Vec<GpuMesh>)>,
    bounds_center: [f32; 3],
    bounds_radius: f32,
}

impl Gears {
    fn new(options: GearsOptions) -> Self {
        let (bounds_center, bounds_radius) = bounding_sphere();
        Self { options, angle: 0.0, gpu: None, bounds_center, bounds_radius }
    }

    /// A symmetric frustum whose narrower axis just contains the bounding sphere,
    /// so the camera can orbit freely without the scene ever leaving the screen.
    fn projection(&self, width: i32, height: i32) -> Mat4 {
        let r = self.bounds_radius * FIT_MARGIN;
        let tan_half = r / (EYE_DISTANCE * EYE_DISTANCE - r * r).sqrt();
        let aspect = width.max(1) as f32 / height.max(1) as f32;
        let (half_w, half_h) = if aspect >= 1.0 {
            (NEAR * tan_half * aspect, NEAR * tan_half)
        } else {
            (NEAR * tan_half, NEAR * tan_half / aspect)
        };
        Mat4::frustum(-half_w, half_w, -half_h, half_h, NEAR, FAR)
    }

    fn view(&self, elapsed: f64) -> Mat4 {
        let yaw = VIEW_ROT_Y + (self.options.camera_speed as f64 * elapsed % 360.0) as f32;
        let pitch = VIEW_ROT_X
            + PITCH_SWING_DEG * (elapsed * std::f64::consts::TAU / PITCH_PERIOD_S).sin() as f32;
        let [cx, cy, cz] = self.bounds_center;
        Mat4::translation(0.0, 0.0, -EYE_DISTANCE)
            * Mat4::rotation(pitch, 1.0, 0.0, 0.0)
            * Mat4::rotation(yaw, 0.0, 1.0, 0.0)
            * Mat4::translation(-cx, -cy, -cz)
    }
}

impl GlScene for Gears {
    fn realize(&mut self, gl: &glow::Context, glsl: GlslDialect) -> Result<(), String> {
        let program = super::compile_program(
            gl,
            glsl,
            VERTEX_SHADER,
            FRAGMENT_SHADER,
            &[(ATTR_POSITION, "a_position"), (ATTR_NORMAL, "a_normal")],
        )?;
        let uniform = |name| unsafe { gl.get_uniform_location(program, name) };
        let uniforms = Uniforms {
            projection: uniform("u_projection"),
            model_view: uniform("u_model_view"),
            normal_matrix: uniform("u_normal_matrix"),
            color: uniform("u_color"),
            light_dir: uniform("u_light_dir"),
        };
        let mut meshes = Vec::new();
        for placement in &PLACEMENTS {
            meshes.push(upload(gl, &gear(placement.spec))?);
        }
        self.gpu = Some((program, uniforms, meshes));
        Ok(())
    }

    fn render(&mut self, gl: &glow::Context, frame: &Frame) {
        let Some((program, u, meshes)) = &self.gpu else { return };
        self.angle = (self.angle + self.options.rotation_speed * frame.delta as f32) % 360.0;
        let projection = self.projection(frame.width, frame.height);
        let view = self.view(frame.elapsed);
        let len = LIGHT_DIR.iter().map(|c| c * c).sum::<f32>().sqrt();
        let light = LIGHT_DIR.map(|c| c / len);
        unsafe {
            gl.clear_color(0.0, 0.0, 0.0, 1.0);
            gl.clear(glow::COLOR_BUFFER_BIT | glow::DEPTH_BUFFER_BIT);
            gl.enable(glow::DEPTH_TEST);
            gl.enable(glow::CULL_FACE);
            gl.use_program(Some(*program));
            gl.uniform_matrix_4_f32_slice(u.projection.as_ref(), false, &projection.0);
            gl.uniform_3_f32_slice(u.light_dir.as_ref(), &light);
            for ((placement, mesh), color) in PLACEMENTS.iter().zip(meshes).zip(&self.options.colors) {
                let angle = placement.angle_factor * self.angle + placement.angle_offset;
                let model_view = view
                    * Mat4::translation(placement.x, placement.y, 0.0)
                    * Mat4::rotation(angle, 0.0, 0.0, 1.0);
                gl.uniform_matrix_4_f32_slice(u.model_view.as_ref(), false, &model_view.0);
                gl.uniform_matrix_3_f32_slice(u.normal_matrix.as_ref(), false, &model_view.rotation_part());
                gl.uniform_3_f32_slice(u.color.as_ref(), color);
                gl.bind_vertex_array(Some(mesh.vao));
                gl.draw_arrays(glow::TRIANGLES, 0, mesh.vertices);
            }
            gl.bind_vertex_array(None);
        }
    }

    fn unrealize(&mut self, gl: &glow::Context) {
        if let Some((program, _, meshes)) = self.gpu.take() {
            unsafe {
                for mesh in meshes {
                    gl.delete_vertex_array(mesh.vao);
                    gl.delete_buffer(mesh.vbo);
                }
                gl.delete_program(program);
            }
        }
    }

    fn msaa_samples(&self) -> i32 {
        self.options.msaa_samples
    }
}

/// Centre and radius of a sphere holding all three gears at any rotation.
fn bounding_sphere() -> ([f32; 3], f32) {
    let reach = |s: &GearSpec| {
        let r2 = s.outer_radius + s.tooth_depth / 2.0;
        (r2 * r2 + s.width * s.width / 4.0).sqrt()
    };
    let (mut lo, mut hi) = ([f32::MAX; 2], [f32::MIN; 2]);
    for p in &PLACEMENTS {
        let r = reach(&p.spec);
        lo = [lo[0].min(p.x - r), lo[1].min(p.y - r)];
        hi = [hi[0].max(p.x + r), hi[1].max(p.y + r)];
    }
    let center = [(lo[0] + hi[0]) / 2.0, (lo[1] + hi[1]) / 2.0, 0.0];
    let radius = PLACEMENTS
        .iter()
        .map(|p| (p.x - center[0]).hypot(p.y - center[1]) + reach(&p.spec))
        .fold(0.0, f32::max);
    (center, radius)
}

fn upload(gl: &glow::Context, vertices: &[f32]) -> Result<GpuMesh, String> {
    unsafe {
        let vao = gl.create_vertex_array()?;
        let vbo = gl.create_buffer()?;
        gl.bind_vertex_array(Some(vao));
        gl.bind_buffer(glow::ARRAY_BUFFER, Some(vbo));
        let bytes = std::slice::from_raw_parts(vertices.as_ptr().cast::<u8>(), std::mem::size_of_val(vertices));
        gl.buffer_data_u8_slice(glow::ARRAY_BUFFER, bytes, glow::STATIC_DRAW);
        let stride = (FLOATS_PER_VERTEX * 4) as i32;
        gl.enable_vertex_attrib_array(ATTR_POSITION);
        gl.vertex_attrib_pointer_f32(ATTR_POSITION, 3, glow::FLOAT, false, stride, 0);
        gl.enable_vertex_attrib_array(ATTR_NORMAL);
        gl.vertex_attrib_pointer_f32(ATTR_NORMAL, 3, glow::FLOAT, false, stride, 12);
        gl.bind_vertex_array(None);
        gl.bind_buffer(glow::ARRAY_BUFFER, None);
        Ok(GpuMesh { vao, vbo, vertices: (vertices.len() / FLOATS_PER_VERTEX) as i32 })
    }
}

/// glxgears' gear(), statement for statement, against the immediate-mode shim below.
fn gear(s: GearSpec) -> Vec<f32> {
    let GearSpec { inner_radius, outer_radius, width, teeth, tooth_depth } = s;
    let (cos, sin) = (f32::cos, f32::sin);
    let mut gl = Immediate::default();

    let r0 = inner_radius;
    let r1 = outer_radius - tooth_depth / 2.0;
    let r2 = outer_radius + tooth_depth / 2.0;

    let da = 2.0 * PI / teeth as f32 / 4.0;

    gl.shade_model(Shade::Flat);

    gl.normal3f(0.0, 0.0, 1.0);

    // draw front face
    gl.begin(Primitive::QuadStrip);
    for i in 0..=teeth {
        let angle = i as f32 * 2.0 * PI / teeth as f32;
        gl.vertex3f(r0 * cos(angle), r0 * sin(angle), width * 0.5);
        gl.vertex3f(r1 * cos(angle), r1 * sin(angle), width * 0.5);
        if i < teeth {
            gl.vertex3f(r0 * cos(angle), r0 * sin(angle), width * 0.5);
            gl.vertex3f(r1 * cos(angle + 3.0 * da), r1 * sin(angle + 3.0 * da), width * 0.5);
        }
    }
    gl.end();

    // draw front sides of teeth
    gl.begin(Primitive::Quads);
    for i in 0..teeth {
        let angle = i as f32 * 2.0 * PI / teeth as f32;
        gl.vertex3f(r1 * cos(angle), r1 * sin(angle), width * 0.5);
        gl.vertex3f(r2 * cos(angle + da), r2 * sin(angle + da), width * 0.5);
        gl.vertex3f(r2 * cos(angle + 2.0 * da), r2 * sin(angle + 2.0 * da), width * 0.5);
        gl.vertex3f(r1 * cos(angle + 3.0 * da), r1 * sin(angle + 3.0 * da), width * 0.5);
    }
    gl.end();

    gl.normal3f(0.0, 0.0, -1.0);

    // draw back face
    gl.begin(Primitive::QuadStrip);
    for i in 0..=teeth {
        let angle = i as f32 * 2.0 * PI / teeth as f32;
        gl.vertex3f(r1 * cos(angle), r1 * sin(angle), -width * 0.5);
        gl.vertex3f(r0 * cos(angle), r0 * sin(angle), -width * 0.5);
        if i < teeth {
            gl.vertex3f(r1 * cos(angle + 3.0 * da), r1 * sin(angle + 3.0 * da), -width * 0.5);
            gl.vertex3f(r0 * cos(angle), r0 * sin(angle), -width * 0.5);
        }
    }
    gl.end();

    // draw back sides of teeth
    gl.begin(Primitive::Quads);
    for i in 0..teeth {
        let angle = i as f32 * 2.0 * PI / teeth as f32;
        gl.vertex3f(r1 * cos(angle + 3.0 * da), r1 * sin(angle + 3.0 * da), -width * 0.5);
        gl.vertex3f(r2 * cos(angle + 2.0 * da), r2 * sin(angle + 2.0 * da), -width * 0.5);
        gl.vertex3f(r2 * cos(angle + da), r2 * sin(angle + da), -width * 0.5);
        gl.vertex3f(r1 * cos(angle), r1 * sin(angle), -width * 0.5);
    }
    gl.end();

    // draw outward faces of teeth
    gl.begin(Primitive::QuadStrip);
    for i in 0..teeth {
        let angle = i as f32 * 2.0 * PI / teeth as f32;

        gl.vertex3f(r1 * cos(angle), r1 * sin(angle), width * 0.5);
        gl.vertex3f(r1 * cos(angle), r1 * sin(angle), -width * 0.5);
        let mut u = r2 * cos(angle + da) - r1 * cos(angle);
        let mut v = r2 * sin(angle + da) - r1 * sin(angle);
        let len = (u * u + v * v).sqrt();
        u /= len;
        v /= len;
        gl.normal3f(v, -u, 0.0);
        gl.vertex3f(r2 * cos(angle + da), r2 * sin(angle + da), width * 0.5);
        gl.vertex3f(r2 * cos(angle + da), r2 * sin(angle + da), -width * 0.5);
        gl.normal3f(cos(angle), sin(angle), 0.0);
        gl.vertex3f(r2 * cos(angle + 2.0 * da), r2 * sin(angle + 2.0 * da), width * 0.5);
        gl.vertex3f(r2 * cos(angle + 2.0 * da), r2 * sin(angle + 2.0 * da), -width * 0.5);
        let u = r1 * cos(angle + 3.0 * da) - r2 * cos(angle + 2.0 * da);
        let v = r1 * sin(angle + 3.0 * da) - r2 * sin(angle + 2.0 * da);
        gl.normal3f(v, -u, 0.0);
        gl.vertex3f(r1 * cos(angle + 3.0 * da), r1 * sin(angle + 3.0 * da), width * 0.5);
        gl.vertex3f(r1 * cos(angle + 3.0 * da), r1 * sin(angle + 3.0 * da), -width * 0.5);
        gl.normal3f(cos(angle), sin(angle), 0.0);
    }

    gl.vertex3f(r1 * cos(0.0), r1 * sin(0.0), width * 0.5);
    gl.vertex3f(r1 * cos(0.0), r1 * sin(0.0), -width * 0.5);

    gl.end();

    gl.shade_model(Shade::Smooth);

    // draw inside radius cylinder
    gl.begin(Primitive::QuadStrip);
    for i in 0..=teeth {
        let angle = i as f32 * 2.0 * PI / teeth as f32;
        gl.normal3f(-cos(angle), -sin(angle), 0.0);
        gl.vertex3f(r0 * cos(angle), r0 * sin(angle), -width * 0.5);
        gl.vertex3f(r0 * cos(angle), r0 * sin(angle), width * 0.5);
    }
    gl.end();

    gl.out
}

#[derive(Clone, Copy, Default, PartialEq)]
enum Shade {
    #[default]
    Smooth,
    Flat,
}

#[derive(Clone, Copy, Default)]
enum Primitive {
    #[default]
    Quads,
    QuadStrip,
}

type Vertex = ([f32; 3], [f32; 3]);

/// Just enough of GL 1.x immediate mode to replay gear(): quads and quad strips
/// become triangles, and GL_FLAT gives each quad its provoking vertex's normal
/// exactly as the fixed-function pipeline did (GL 2.1 spec, table 2.12).
#[derive(Default)]
struct Immediate {
    shade: Shade,
    normal: [f32; 3],
    primitive: Primitive,
    vertices: Vec<Vertex>,
    out: Vec<f32>,
}

impl Immediate {
    fn shade_model(&mut self, shade: Shade) {
        self.shade = shade;
    }

    fn normal3f(&mut self, x: f32, y: f32, z: f32) {
        // Also covers GL_NORMALIZE: the falling flank's normal is not unit length in glxgears.
        let len = (x * x + y * y + z * z).sqrt();
        self.normal = [x / len, y / len, z / len];
    }

    fn begin(&mut self, primitive: Primitive) {
        self.primitive = primitive;
        self.vertices.clear();
    }

    fn vertex3f(&mut self, x: f32, y: f32, z: f32) {
        self.vertices.push(([x, y, z], self.normal));
    }

    fn end(&mut self) {
        let v = std::mem::take(&mut self.vertices);
        match self.primitive {
            Primitive::Quads => {
                for q in v.chunks_exact(4) {
                    self.quad([q[0], q[1], q[2], q[3]], q[3].1);
                }
            }
            Primitive::QuadStrip => {
                for k in 0..v.len().saturating_sub(2) / 2 {
                    let (a, b, c, d) = (v[2 * k], v[2 * k + 1], v[2 * k + 3], v[2 * k + 2]);
                    self.quad([a, b, c, d], c.1);
                }
            }
        }
    }

    fn quad(&mut self, q: [Vertex; 4], provoking_normal: [f32; 3]) {
        for tri in [[q[0], q[1], q[2]], [q[0], q[2], q[3]]] {
            if is_degenerate(&tri) {
                continue;
            }
            for (position, normal) in tri {
                let normal = if self.shade == Shade::Flat { provoking_normal } else { normal };
                self.out.extend_from_slice(&position);
                self.out.extend_from_slice(&normal);
            }
        }
    }
}

/// glxgears' front and back strips repeat vertices, producing zero-area triangles.
fn is_degenerate(tri: &[Vertex; 3]) -> bool {
    let [a, b, c] = tri.map(|v| v.0);
    let e1 = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
    let e2 = [c[0] - a[0], c[1] - a[1], c[2] - a[2]];
    let n = [e1[1] * e2[2] - e1[2] * e2[1], e1[2] * e2[0] - e1[0] * e2[2], e1[0] * e2[1] - e1[1] * e2[0]];
    n[0] * n[0] + n[1] * n[1] + n[2] * n[2] < 1e-12
}

#[cfg(test)]
mod tests {
    use super::*;

    fn triangles(mesh: &[f32]) -> Vec<[[f32; 6]; 3]> {
        mesh.chunks_exact(18)
            .map(|t| [0, 1, 2].map(|i| t[i * 6..i * 6 + 6].try_into().unwrap()))
            .collect()
    }

    #[test]
    fn every_triangle_faces_along_its_flat_normal() {
        // Culling keeps only counter-clockwise triangles, so each face's winding
        // must agree with the normal glxgears lit it with.
        let mesh = gear(PLACEMENTS[0].spec);
        for [a, b, c] in triangles(&mesh) {
            let e1 = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
            let e2 = [c[0] - a[0], c[1] - a[1], c[2] - a[2]];
            let n = [e1[1] * e2[2] - e1[2] * e2[1], e1[2] * e2[0] - e1[0] * e2[2], e1[0] * e2[1] - e1[1] * e2[0]];
            let dot = n[0] * a[3] + n[1] * a[4] + n[2] * a[5];
            assert!(dot > 0.0, "triangle wound against its normal: {a:?} {b:?} {c:?}");
        }
    }

    #[test]
    fn bounding_sphere_contains_every_vertex_at_any_angle() {
        let (center, radius) = bounding_sphere();
        for p in &PLACEMENTS {
            for v in gear(p.spec).chunks_exact(6) {
                // Distance from the gear axis is rotation-invariant.
                let r = v[0].hypot(v[1]);
                let d = ((p.x - center[0]).hypot(p.y - center[1]) + r).hypot(v[2]);
                assert!(d <= radius + 1e-4, "vertex outside bounds: {d} > {radius}");
            }
        }
    }
}
