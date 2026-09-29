//! Northern lights: layered curtains with a sharp lower edge and a soft fade
//! upward, over a dim sky with a few stars.

use super::fullscreen::{FullscreenScene, SpeedOptions};
use super::{AnimationInfo, GlScene};

pub const INFO: AnimationInfo = AnimationInfo {
    id: "aurora",
    title_en: "Aurora",
    title_tr: "Kuzey ışıkları",
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
    vec2 uv = gl_FragCoord.xy / u_resolution;
    float aspect = u_resolution.x / u_resolution.y;
    float t = u_time * 0.12;
    vec3 color = mix(vec3(0.0, 0.01, 0.04), vec3(0.0, 0.0, 0.01), uv.y);

    vec2 grid = vec2(uv.x * aspect, uv.y) * 90.0;
    vec2 cell = floor(grid);
    vec2 jitter = (vec2(hash(cell + 3.1), hash(cell + 8.7)) - 0.5) * 0.6;
    float twinkle = 0.6 + 0.4 * sin(u_time * 2.0 + hash(cell) * 30.0);
    float star = step(0.97, hash(cell)) * smoothstep(0.2, 0.0, length(fract(grid) - 0.5 - jitter));
    color += vec3(star * twinkle * smoothstep(0.3, 0.7, uv.y));

    for (int i = 0; i < 4; i++) {
        float fi = float(i);
        float base = 0.5 + 0.06 * fi;
        float wave = sin(uv.x * 3.0 + t * (1.0 + 0.3 * fi) + fi * 1.7) * 0.07
                   + sin(uv.x * 7.0 - t * 1.7 + fi) * 0.03
                   + sin(uv.x * 13.0 + t * 2.3 + fi * 3.1) * 0.012;
        float d = uv.y - (base + wave);
        float body = exp(-max(d, 0.0) * 8.0) * smoothstep(-0.02, 0.0, d);
        float rays = 0.65 + 0.35 * sin(uv.x * 110.0 + wave * 60.0 + t * 4.0 + fi * 2.0);
        vec3 hue = mix(vec3(0.1, 1.0, 0.45), vec3(0.6, 0.3, 1.0), smoothstep(0.0, 0.35, max(d, 0.0)) * 0.9 + fi * 0.08);
        color += hue * body * rays * 0.42;
    }
    frag_color = vec4(min(color, vec3(1.0)), 1.0);
}
";
