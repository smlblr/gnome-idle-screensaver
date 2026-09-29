//! Flight through a star field: six layers of stars zoom outward from the centre,
//! each fading in far away and out as it passes.

use super::fullscreen::{FullscreenScene, SpeedOptions};
use super::{AnimationInfo, GlScene};

pub const INFO: AnimationInfo = AnimationInfo {
    id: "starfield",
    title_en: "Starfield",
    title_tr: "Yıldızlar",
    create,
};

fn create(options: Option<&toml::Value>) -> Box<dyn GlScene> {
    let o: SpeedOptions = super::parse_options(INFO.id, options);
    Box::new(FullscreenScene::new(FRAGMENT, o.speed))
}

const FRAGMENT: &str = r"
float hash(vec2 p) {
    p = fract(p * vec2(123.34, 456.21));
    p += dot(p, p + 45.32);
    return fract(p.x * p.y);
}

void main() {
    vec2 uv = (gl_FragCoord.xy - 0.5 * u_resolution) / u_resolution.y;
    vec3 color = vec3(0.0);
    for (int i = 0; i < 6; i++) {
        float phase = float(i) / 6.0 + u_time * 0.07;
        float depth = fract(phase);
        // A new seed each time a layer wraps to the far end, so it never repeats.
        float seed = floor(phase) * 13.1 + float(i) * 7.7;
        vec2 p = uv * mix(26.0, 3.0, depth * depth);
        vec2 cell = floor(p);
        vec2 local = fract(p) - 0.5;
        // Most cells are empty, so test for a star before doing the rest.
        if (hash(cell + seed + 9.1) < 0.62) continue;
        vec2 jitter = (vec2(hash(cell + seed), hash(cell + seed + 4.3)) - 0.5) * 0.7;
        float radius = mix(0.03, 0.11, depth);
        float star = smoothstep(radius, radius * 0.25, length(local - jitter));
        float fade = smoothstep(0.0, 0.2, depth) * smoothstep(1.0, 0.8, depth);
        vec3 tint = mix(vec3(0.7, 0.8, 1.0), vec3(1.0, 0.9, 0.7), hash(cell + seed + 2.2));
        color += tint * star * fade;
    }
    frag_color = vec4(color, 1.0);
}
";
