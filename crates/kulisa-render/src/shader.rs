//! WGSL composition and validation for built-in and user shaders.

use wgpu::naga;

const PRELUDE: &str = include_str!("prelude.wgsl");
const PARTICLE_PRELUDE: &str = include_str!("particles.wgsl");

/// Textured, tinted sprite. Shader 0.
pub(crate) const SPRITE: &str = "fn fragment(in: VertexOut) -> vec4<f32> {
    return textureSample(tex, samp, in.uv) * in.color;
}
";

/// The default look of particles: the image (or frame) times the color over life.
pub(crate) const PARTICLE: &str = "fn fragment(in: VertexOut) -> vec4<f32> {
    return textureSample(tex, samp, in.uv) * in.color;
}
";

/// Copies an sRGB-decoded texture to a non-sRGB output, encoding by hand. Used when the
/// surface cannot be viewed as sRGB (some GLES drivers).
pub(crate) const PRESENT_ENCODE: &str = "fn fragment(in: VertexOut) -> vec4<f32> {
    let c = textureSample(tex, samp, in.uv);
    let lo = c.rgb * 12.92;
    let hi = 1.055 * pow(c.rgb, vec3<f32>(1.0 / 2.4)) - 0.055;
    return vec4<f32>(select(hi, lo, c.rgb <= vec3<f32>(0.0031308)), 1.0);
}
";

/// User code first (so error line numbers match the user's file), then the prelude.
pub(crate) fn compose(user: &str) -> String {
    let entry = if has_fn(user, "vertex") { "vertex" } else { "kulisa_vertex" };
    format!("{user}\n{}", PRELUDE.replace("KULISA_VERTEX_ENTRY", entry))
}

fn has_fn(src: &str, name: &str) -> bool {
    src.match_indices("fn ").any(|(i, _)| {
        let rest = src[i + 3..].trim_start();
        rest.strip_prefix(name).is_some_and(|r| r.trim_start().starts_with('('))
    })
}

/// Checks a user shader and returns the full WGSL module, or a readable error.
/// `name` (e.g. a file path) prefixes error locations.
pub(crate) fn build(name: &str, user: &str) -> Result<String, String> {
    if !has_fn(user, "fragment") {
        return Err(format!("{name}: shader must define `fn fragment(in: VertexOut) -> vec4<f32>`"));
    }
    validate(name, compose(user))
}

/// The same for drawing particles: the user's `fragment` with the particle vertex stage. A
/// user `vertex` function is not used (particles place themselves).
pub(crate) fn build_particles(name: &str, user: &str) -> Result<String, String> {
    if !has_fn(user, "fragment") {
        return Err(format!("{name}: shader must define `fn fragment(in: VertexOut) -> vec4<f32>`"));
    }
    if has_fn(user, "vertex") {
        return Err(format!("{name}: particle shaders cannot define `vertex` (particles place themselves)"));
    }
    validate(name, format!("{user}\n{PARTICLE_PRELUDE}"))
}

fn validate(name: &str, source: String) -> Result<String, String> {
    let module =
        naga::front::wgsl::parse_str(&source).map_err(|e| e.emit_to_string_with_path(&source, name))?;
    naga::valid::Validator::new(naga::valid::ValidationFlags::all(), naga::valid::Capabilities::empty())
        .validate(&module)
        .map_err(|e| e.emit_to_string_with_path(&source, name))?;
    Ok(source)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtin_shaders_validate() {
        build("sprite", SPRITE).unwrap();
        build("present", PRESENT_ENCODE).unwrap();
        build_particles("particles", PARTICLE).unwrap();
        build_particles("particle sprite", SPRITE).unwrap();
        assert!(build_particles("v", "fn vertex(in: VertexIn) -> VertexOut { return kulisa_vertex(in); }\nfn fragment(in: VertexOut) -> vec4<f32> { return in.color; }").is_err());
    }

    #[test]
    fn user_vertex_function_is_used() {
        let src = "fn vertex(in: VertexIn) -> VertexOut { var o = kulisa_vertex(in); o.uv.x = 1.0 - o.uv.x; return o; }
fn fragment(in: VertexOut) -> vec4<f32> { return in.color * params.p[0].x * globals.time; }";
        let full = build("flip.wgsl", src).unwrap();
        assert!(full.contains("return vertex(in);"));
    }

    #[test]
    fn errors_point_at_user_lines() {
        let err =
            build("bad.wgsl", "fn fragment(in: VertexOut) -> vec4<f32> {\n    return undefined_thing;\n}")
                .unwrap_err();
        assert!(err.contains("bad.wgsl:2"), "{err}");
        let err = build("empty.wgsl", "// nothing").unwrap_err();
        assert!(err.contains("must define"), "{err}");
    }
}
