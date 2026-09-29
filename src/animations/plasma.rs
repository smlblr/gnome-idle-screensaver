//! Classic plasma: interfering sine waves through a cycling cosine palette.

use super::fullscreen::{FullscreenScene, SpeedOptions};
use super::{AnimationInfo, GlScene};

pub const INFO: AnimationInfo = AnimationInfo {
    id: "plasma",
    title: "Plazma",
    create,
};

fn create(options: Option<&toml::Value>) -> Box<dyn GlScene> {
    let o: SpeedOptions = super::parse_options(INFO.id, options);
    Box::new(FullscreenScene::new(FRAGMENT, o.speed))
}

const FRAGMENT: &str = r"
void main() {
    vec2 p = gl_FragCoord.xy / u_resolution.y * 4.0;
    float t = u_time * 0.6;
    float v = sin(p.x * 1.3 + t)
            + sin(p.y * 1.7 - t * 1.1)
            + sin((p.x + p.y) * 1.1 + t * 0.7)
            + sin(length(p - vec2(3.5 + sin(t * 0.5), 2.0 + cos(t * 0.4))) * 2.2 - t);
    vec3 color = 0.5 + 0.5 * cos(6.28318 * (v * 0.25 + vec3(0.0, 0.33, 0.67) + t * 0.05));
    frag_color = vec4(color * 0.85, 1.0);
}
";
