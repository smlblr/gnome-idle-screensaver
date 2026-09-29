//! Endless checkered tunnel with a slow twist and a colour drift along its length.

use super::fullscreen::{FullscreenScene, SpeedOptions};
use super::{AnimationInfo, GlScene};

pub const INFO: AnimationInfo = AnimationInfo {
    id: "tunnel",
    title: "Tünel",
    create,
};

fn create(options: Option<&toml::Value>) -> Box<dyn GlScene> {
    let o: SpeedOptions = super::parse_options(INFO.id, options);
    Box::new(FullscreenScene::new(FRAGMENT, o.speed))
}

// Sixteen columns keep the checker parity consistent across atan()'s seam.
const FRAGMENT: &str = r"
void main() {
    vec2 uv = (gl_FragCoord.xy - 0.5 * u_resolution) / u_resolution.y;
    float r = length(uv);
    float angle = atan(uv.y, uv.x);
    float depth = 0.7 / max(r, 0.001);
    vec2 tc = vec2(angle / 3.14159265 * 8.0 + u_time * 0.3 + depth * 0.1, depth + u_time * 2.4);
    float checker = mod(floor(tc.x) + floor(tc.y), 2.0);
    vec3 tint = 0.5 + 0.5 * cos(6.28318 * (vec3(0.0, 0.33, 0.67) + depth * 0.02 + u_time * 0.02));
    vec3 color = mix(tint * 0.12, tint, checker);
    // Fade toward the centre, where the cells shrink below a pixel and alias.
    color *= smoothstep(0.0, 0.25, r) * exp(-depth * 0.03);
    frag_color = vec4(color, 1.0);
}
";
