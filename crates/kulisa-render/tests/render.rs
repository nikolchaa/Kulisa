//! Headless GPU tests: render small scenes offscreen and check pixels.
//!
//! Without a GPU (or software adapter) the tests skip, unless `KULISA_REQUIRE_GPU` is set,
//! which CI does so they cannot silently pass.

use kulisa_core::{
    BlendMode, Color, Material, Node, NodeKind, Rect, ScaleMode, Scene, Slice, UVec2, Vec2, Viewport,
};
use kulisa_render::{Image, Renderer};

const SIZE: u32 = 64;

fn renderer() -> Option<Renderer> {
    match pollster::block_on(Renderer::headless(SIZE, SIZE)) {
        Ok(r) => Some(r),
        Err(e) if std::env::var_os("KULISA_REQUIRE_GPU").is_some() => panic!("no GPU adapter: {e}"),
        Err(e) => {
            eprintln!("skipping: {e}");
            None
        }
    }
}

fn scene() -> (Scene, Viewport) {
    let mut s = Scene::new(Vec2::splat(SIZE as f32));
    s.clear = Color::BLACK;
    (s, Viewport::new(SIZE as f32, SIZE as f32, ScaleMode::Expand))
}

fn pixel(img: &Image, x: u32, y: u32) -> [u8; 4] {
    let i = ((y * img.width + x) * 4) as usize;
    img.pixels[i..i + 4].try_into().unwrap()
}

#[track_caller]
fn assert_px(img: &Image, x: u32, y: u32, want: [u8; 3]) {
    let got = pixel(img, x, y);
    let close = got.iter().zip(want).all(|(&g, w)| (g as i32 - w as i32).abs() <= 2);
    assert!(close, "pixel ({x},{y}) is {got:?}, want {want:?}");
}

fn solid(r: &mut Renderer, rgba: [u8; 4]) -> kulisa_core::TextureId {
    r.create_texture(&Image::solid(4, 4, rgba))
}

#[test]
fn clears_and_places_sprites() {
    let Some(mut r) = renderer() else { return };
    let (mut s, v) = scene();
    let red = solid(&mut r, [255, 0, 0, 255]);
    s.add(s.root(), Node::sprite(red, Vec2::new(16.0, 8.0)).at(8.0, 8.0));
    r.render(&s, &v, 0.0);
    let img = r.read_pixels().unwrap();
    assert_px(&img, 10, 10, [255, 0, 0]);
    assert_px(&img, 23, 15, [255, 0, 0]);
    assert_px(&img, 30, 10, [0, 0, 0]);
    assert_px(&img, 10, 20, [0, 0, 0]);
}

#[test]
fn atlas_sprites_batch_into_one_draw_call() {
    let Some(mut r) = renderer() else { return };
    let (mut s, v) = scene();
    let textures =
        [solid(&mut r, [255, 0, 0, 255]), solid(&mut r, [0, 255, 0, 255]), solid(&mut r, [0, 0, 255, 255])];
    for i in 0..60 {
        let t = textures[i % 3];
        s.add(s.root(), Node::sprite(t, Vec2::splat(2.0)).at((i % 8) as f32 * 8.0, (i / 8) as f32 * 8.0));
    }
    r.render(&s, &v, 0.0);
    assert_eq!(r.stats().draw_calls, 1, "three textures on one atlas page batch together");
    let img = r.read_pixels().unwrap();
    assert_px(&img, 0, 0, [255, 0, 0]);
    assert_px(&img, 8, 0, [0, 255, 0]);
    assert_px(&img, 16, 0, [0, 0, 255]);
}

#[test]
fn blends_premultiplied_alpha_and_add() {
    let Some(mut r) = renderer() else { return };
    let (mut s, v) = scene();
    let white = solid(&mut r, [255, 255, 255, 255]);
    let mut half = Node::sprite(white, Vec2::splat(16.0));
    half.opacity = 0.5;
    s.add(s.root(), half);
    let red = solid(&mut r, [255, 0, 0, 255]);
    let green = solid(&mut r, [0, 255, 0, 255]);
    s.add(s.root(), Node::sprite(red, Vec2::splat(16.0)).at(32.0, 0.0));
    let mut add = Node::sprite(green, Vec2::splat(16.0)).at(32.0, 0.0);
    add.blend = BlendMode::Add;
    s.add(s.root(), add);
    r.render(&s, &v, 0.0);
    let img = r.read_pixels().unwrap();
    // 50% white over black is linear 0.5, which is sRGB 188.
    assert_px(&img, 8, 8, [188, 188, 188]);
    assert_px(&img, 40, 8, [255, 255, 0]);
}

#[test]
fn clips_to_rectangles() {
    let Some(mut r) = renderer() else { return };
    let (mut s, v) = scene();
    let red = solid(&mut r, [255, 0, 0, 255]);
    let mut g = Node::group();
    g.clip = Some(Rect::new(0.0, 0.0, 10.0, 64.0));
    let g = s.add(s.root(), g);
    s.add(g, Node::sprite(red, Vec2::splat(32.0)));
    r.render(&s, &v, 0.0);
    let img = r.read_pixels().unwrap();
    assert_px(&img, 5, 5, [255, 0, 0]);
    assert_px(&img, 15, 5, [0, 0, 0]);
}

#[test]
fn render_targets_feed_sprites() {
    let Some(mut r) = renderer() else { return };
    let (mut s, v) = scene();
    let target = r.create_target(16, 16);
    let blue = solid(&mut r, [0, 0, 255, 255]);
    let t = s.add(s.root(), Node::target(target, UVec2::new(16, 16)));
    s.add(t, Node::sprite(blue, Vec2::splat(8.0))); // top-left quarter of the target
    s.add(s.root(), Node::sprite(target, Vec2::splat(32.0)).at(32.0, 32.0));
    r.render(&s, &v, 0.0);
    let img = r.read_pixels().unwrap();
    assert_px(&img, 36, 36, [0, 0, 255]);
    assert_px(&img, 60, 60, [0, 0, 0]);
}

#[test]
fn custom_shaders_read_params() {
    let Some(mut r) = renderer() else { return };
    let (mut s, v) = scene();
    let shader = r
        .create_shader(
            "paint.wgsl",
            "fn fragment(in: VertexOut) -> vec4<f32> { return vec4<f32>(params.p[0].rgb, 1.0); }",
        )
        .unwrap();
    let white = solid(&mut r, [255, 255, 255, 255]);
    let mut node = Node::sprite(white, Vec2::splat(16.0));
    let mut params = [0.0; 16];
    params[1] = 1.0; // linear green
    node.material = Some(Material::new(shader, params));
    s.add(s.root(), node);
    r.render(&s, &v, 0.0);
    assert_px(&r.read_pixels().unwrap(), 8, 8, [0, 255, 0]);
}

#[test]
fn screen_and_layer_effects_chain() {
    let Some(mut r) = renderer() else { return };
    let (mut s, v) = scene();
    let invert = r
        .create_shader(
            "invert.wgsl",
            "fn fragment(in: VertexOut) -> vec4<f32> { let c = textureSample(tex, samp, in.uv); return vec4<f32>(c.a - c.rgb, c.a); }",
        )
        .unwrap();
    let swap = r
        .create_shader(
            "swap.wgsl",
            "fn fragment(in: VertexOut) -> vec4<f32> { let c = textureSample(tex, samp, in.uv); return vec4<f32>(c.g, c.r, c.b, c.a); }",
        )
        .unwrap();
    let red = solid(&mut r, [255, 0, 0, 255]);
    let effect = |shader| Material::new(shader, [0.0; 16]);
    // Layer effect: red becomes green inside the layer only.
    let mut layer = Node::layer(1.0);
    if let NodeKind::Layer(l) = &mut layer.kind {
        l.effects.push(effect(swap));
    }
    let l = s.add(s.root(), layer);
    s.add(l, Node::sprite(red, Vec2::splat(16.0)));
    s.add(s.root(), Node::sprite(red, Vec2::splat(16.0)).at(32.0, 0.0));
    // Screen chain: swap then invert (two passes, ping-pong).
    s.effects = vec![effect(swap), effect(invert)];
    r.render(&s, &v, 0.0);
    let img = r.read_pixels().unwrap();
    // Layer: red -> green; screen swap: green -> red; invert: red -> cyan.
    assert_px(&img, 8, 8, [0, 255, 255]);
    // Plain red: screen swap -> green; invert -> magenta.
    assert_px(&img, 40, 8, [255, 0, 255]);
    // Background black -> white after invert.
    assert_px(&img, 8, 40, [255, 255, 255]);
}

#[test]
fn shader_errors_are_readable() {
    let Some(mut r) = renderer() else { return };
    let err = r
        .create_shader("broken.wgsl", "fn fragment(in: VertexOut) -> vec4<f32> {\n  return nope;\n}")
        .unwrap_err();
    assert!(err.contains("broken.wgsl:2"), "{err}");
}

#[test]
fn masks_bind_a_second_texture() {
    let Some(mut r) = renderer() else { return };
    let (mut s, v) = scene();
    // Left half of the mask is black, right half white (an atlas image: copied unpacked).
    let mut px = Vec::new();
    for _y in 0..4 {
        for x in 0..4 {
            px.extend_from_slice(if x < 2 { &[0, 0, 0, 255] } else { &[255, 255, 255, 255] });
        }
    }
    let mask = r.create_texture(&Image::from_straight_rgba(4, 4, px));
    let shader = r
        .create_shader(
            "masked.wgsl",
            "fn fragment(in: VertexOut) -> vec4<f32> { let m = textureSample(mask_tex, mask_samp, in.uv).r; return vec4<f32>(m, 0.0, 1.0 - m, 1.0); }",
        )
        .unwrap();
    let target = r.create_target(32, 32);
    let mut node = Node::sprite(target, Vec2::splat(32.0));
    node.material = Some(Material { shader, params: [0.0; 16], mask: Some(mask) });
    s.add(s.root(), node);
    // Without a mask the shader samples white.
    let mut plain = Node::sprite(target, Vec2::splat(16.0)).at(40.0, 40.0);
    plain.material = Some(Material::new(shader, [0.0; 16]));
    s.add(s.root(), plain);
    r.render(&s, &v, 0.0);
    let img = r.read_pixels().unwrap();
    assert_px(&img, 4, 16, [0, 0, 255]);
    assert_px(&img, 28, 16, [255, 0, 0]);
    assert_px(&img, 48, 48, [255, 0, 0]);
}

#[test]
fn captures_scenes_into_targets_and_reads_them_back() {
    let Some(mut r) = renderer() else { return };
    let (mut s, v) = scene();
    let red = solid(&mut r, [255, 0, 0, 255]);
    s.add(s.root(), Node::sprite(red, Vec2::splat(32.0)));
    // A 32x32 capture of the 64x64 scene: the sprite covers its top-left quarter.
    let shot = r.create_target(32, 32);
    r.capture(&s, &v, 0.0, shot).unwrap();
    let rb = r.read_texture(shot).unwrap();
    let img = r.poll_readback(&rb, true).unwrap().unwrap();
    assert_eq!((img.width, img.height), (32, 32));
    assert_px(&img, 4, 4, [255, 0, 0]);
    assert_px(&img, 24, 24, [0, 0, 0]);
    // The capture shows on screen like any texture.
    let (mut s2, v2) = scene();
    s2.add(s2.root(), Node::sprite(shot, Vec2::splat(64.0)));
    r.render(&s2, &v2, 0.0);
    let img = r.read_pixels().unwrap();
    assert_px(&img, 10, 10, [255, 0, 0]);
    assert_px(&img, 50, 50, [0, 0, 0]);
    assert!(r.capture(&s, &v, 0.0, red).is_err(), "atlas images are not render targets");
    // Freed textures draw white instead of failing.
    r.free_texture(shot);
    r.render(&s2, &v2, 0.0);
    assert_px(&r.read_pixels().unwrap(), 50, 50, [255, 255, 255]);
}

#[test]
fn nine_slices_keep_their_borders() {
    let Some(mut r) = renderer() else { return };
    let (mut s, v) = scene();
    // 8x8 image: a 2 px red border around blue.
    let mut px = Vec::new();
    for y in 0..8 {
        for x in 0..8 {
            let edge = x < 2 || y < 2 || x >= 6 || y >= 6;
            px.extend_from_slice(if edge { &[255, 0, 0, 255] } else { &[0, 0, 255, 255] });
        }
    }
    let img = r.create_texture_unpacked(&Image::from_straight_rgba(8, 8, px));
    let mut node = Node::sprite(img, Vec2::new(48.0, 32.0)).at(8.0, 8.0);
    if let NodeKind::Sprite(sp) = &mut node.kind {
        sp.slice = Some(Slice {
            left: 2.0,
            top: 2.0,
            right: 2.0,
            bottom: 2.0,
            source: Vec2::splat(8.0),
            scale: 1.0,
        });
    }
    s.add(s.root(), node);
    r.render(&s, &v, 0.0);
    let out = r.read_pixels().unwrap();
    assert_px(&out, 8, 20, [255, 0, 0]); // left border stays 2 px wide
    assert_px(&out, 30, 24, [0, 0, 255]); // the middle stretches
    assert_px(&out, 54, 20, [255, 0, 0]);
    assert_px(&out, 30, 39, [255, 0, 0]);
}
