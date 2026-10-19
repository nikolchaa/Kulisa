//! Smoke scene: exercises layers, parallax, a render target, a custom sprite shader,
//! clipping, a screen effect chain (M1) and a typewriter dialogue box (M2). Built in Rust
//! until the Luau API (M4) can build it.

use kulisa_core::{BlendMode, Color, Material, Node, NodeId, NodeKind, Quad, Rect, Scene, UVec2, Vec2};
use kulisa_render::{Image, Renderer};
use kulisa_text::{TextBox, TextStyle, TextSystem, Typewriter};

use crate::logo;

pub const DESIGN: Vec2 = Vec2::new(1920.0, 1080.0);

const LINES: &[&str] = &[
    "{b}Kulisa{/b} draws {color=#ff6a3d}styled{/color}, {wave}waving{/wave} and {shake}shaking{/shake} text.{w=0.4} Click to continue.",
    "Ruby sits above its base: {ruby=stage scenery}kulisa{/ruby}.{w=0.3} Pauses,{w=0.6} {speed=0.35}slow parts{/speed} and {size=1.4}sizes{/size} are inline tags.",
    "Glyphs share the sprite atlas, so this whole box batches with the scene. Press {b}M{/b} to toggle the music.",
];

pub struct Smoke {
    pub scene: Scene,
    logo: NodeId,
    mini: NodeId,
    name: NodeId,
    dialogue: NodeId,
    name_box: TextBox,
    line_box: TextBox,
    line: usize,
    quads: Vec<Quad>,
}

fn name_style() -> TextStyle {
    TextStyle {
        font: Some("Inter".into()),
        size: 30.0,
        color: Color::hex("#ff6a3d").unwrap(),
        ..Default::default()
    }
}

fn line_style() -> TextStyle {
    TextStyle {
        font: Some("Inter".into()),
        size: 34.0,
        width: Some(1520.0),
        color: Color::hex("#f4f1ea").unwrap(),
        ..Default::default()
    }
}

fn params(p0: [f32; 4]) -> [f32; 16] {
    let mut p = [0.0; 16];
    p[..4].copy_from_slice(&p0);
    p
}

/// White disc fading to transparent at the edge (smoothstep falloff), for soft glows.
fn radial(size: u32) -> Image {
    let mut px = Vec::with_capacity((size * size * 4) as usize);
    let c = (size as f32 - 1.0) * 0.5;
    for y in 0..size {
        for x in 0..size {
            let d = ((x as f32 - c).powi(2) + (y as f32 - c).powi(2)).sqrt() / c;
            let t = (1.0 - d).clamp(0.0, 1.0);
            px.extend_from_slice(&[255, 255, 255, (t * t * (3.0 - 2.0 * t) * 255.0) as u8]);
        }
    }
    Image::from_straight_rgba(size, size, px)
}

/// Vertical gradient between two sRGB colors (straight alpha).
fn gradient(top: [u8; 3], bottom: [u8; 3]) -> Image {
    let h = 256;
    let mut px = Vec::with_capacity(h * 4);
    for y in 0..h {
        let t = y as f32 / (h - 1) as f32;
        for c in 0..3 {
            px.push((top[c] as f32 * (1.0 - t) + bottom[c] as f32 * t) as u8);
        }
        px.push(255);
    }
    Image::from_straight_rgba(1, h as u32, px)
}

impl Smoke {
    pub fn build(r: &mut Renderer, text: &mut TextSystem, scale: f32) -> Result<Smoke, String> {
        let mut scene = Scene::new(DESIGN);
        scene.clear = Color::hex("#111116").unwrap();
        let white = r.create_texture(&Image::solid(4, 4, [255; 4]));
        let sky = r.create_texture(&gradient([17, 17, 22], [58, 40, 52]));
        let logo_tex = r.create_texture(&logo::image(256));
        let halo = r.create_texture(&radial(128));
        let wave = r.create_shader("wave.wgsl", include_str!("shaders/wave.wgsl"))?;
        let vignette = r.create_shader("vignette.wgsl", include_str!("shaders/vignette.wgsl"))?;
        let root = scene.root();

        // Far backdrop: a gradient wider than any screen, barely moving.
        let back = scene.add(root, Node::layer(0.1));
        scene.add(back, Node::sprite(sky, Vec2::new(4000.0, 1600.0)).at(-1040.0, -260.0));

        // Stage flats at three depths; all share one texture, so they batch.
        for (depth, parallax, color, count) in
            [(0, 0.3, "#24222c", 7), (1, 0.6, "#2f2a36", 6), (2, 0.85, "#3b3340", 5)]
        {
            let layer = scene.add(root, Node::layer(parallax).with_z(depth));
            for i in 0..count {
                let w = 220.0 + 40.0 * depth as f32;
                let h = 380.0 + 120.0 * depth as f32 + ((i * 37) % 5) as f32 * 30.0;
                let x = -400.0 + i as f32 * (2700.0 / count as f32) + depth as f32 * 90.0;
                let mut flat = Node::sprite(white, Vec2::new(w, h)).at(x, 1080.0 - h + 40.0);
                flat.tint = Color::hex(color).unwrap();
                scene.add(layer, flat);
            }
        }

        // World: the logo (driven by Luau) and a render target shown through a wave shader.
        let world = scene.add(root, Node::layer(1.0).with_z(10));
        let logo = scene.add(world, Node::sprite(logo_tex, Vec2::splat(320.0)).centered().at(960.0, 520.0));
        // A soft additive halo behind the logo (a child with negative z draws behind it).
        let mut glow = Node::sprite(halo, Vec2::splat(620.0)).centered().at(160.0, 160.0).with_z(-1);
        glow.blend = BlendMode::Add;
        glow.tint = Color::hex("#ff6a3d").unwrap();
        glow.opacity = 0.22;
        scene.add(logo, glow);

        let target = r.create_target(256, 256);
        let t = scene.add(world, Node::target(target, UVec2::splat(256)));
        if let NodeKind::Target(spec) = &mut scene[t].kind {
            spec.clear = Some(Color::hex("#1e1c24").unwrap());
        }
        let mini = scene.add(t, Node::sprite(logo_tex, Vec2::splat(160.0)).centered().at(128.0, 128.0));
        let mut shown = Node::sprite(target, Vec2::splat(256.0)).at(1400.0, 380.0);
        shown.material = Some(Material { shader: wave, params: params([0.02, 18.0, 3.0, 0.0]) });
        scene.add(world, shown);

        // HUD: fixed to the screen, with a clipped panel.
        let hud = scene.add(root, Node::layer(0.0).with_z(20));
        let mut panel = Node::group().at(40.0, 40.0);
        panel.clip = Some(Rect::new(0.0, 0.0, 260.0, 64.0));
        let panel = scene.add(hud, panel);
        let mut bar = Node::sprite(white, Vec2::new(400.0, 64.0));
        bar.tint = Color::hex("#ff6a3d").unwrap();
        bar.opacity = 0.85;
        scene.add(panel, bar);
        scene.add(panel, Node::sprite(logo_tex, Vec2::splat(48.0)).at(8.0, 8.0));

        // Dialogue box: a typical visual novel textbox, built from the same primitives.
        let ui = scene.add(root, Node::layer(0.0).with_z(30));
        let mut backing = Node::sprite(white, Vec2::new(1600.0, 236.0)).at(160.0, 800.0);
        backing.tint = Color::hex("#111116").unwrap();
        backing.opacity = 0.82;
        scene.add(ui, backing);
        let name = scene.add(ui, Node::quads(Vec::new()).at(200.0, 816.0));
        let dialogue = scene.add(ui, Node::quads(Vec::new()).at(200.0, 866.0));

        scene.effects.push(Material { shader: vignette, params: params([0.35, 0.85, 0.0, 0.55]) });
        let name_box =
            Self::lay_out(text, r, "{b}Kulisa{/b}", &name_style(), scale, Typewriter { cps: 0.0, fade: 0.0 });
        let line_box =
            Self::lay_out(text, r, LINES[0], &line_style(), scale, Typewriter { cps: 38.0, fade: 0.08 });
        Ok(Smoke { scene, logo, mini, name, dialogue, name_box, line_box, line: 0, quads: Vec::new() })
    }

    fn lay_out(
        text: &mut TextSystem,
        r: &mut Renderer,
        markup: &str,
        style: &TextStyle,
        scale: f32,
        tw: Typewriter,
    ) -> TextBox {
        let rich = kulisa_text::parse(markup).unwrap_or_else(|e| panic!("smoke line markup: {e}"));
        TextBox::new(text.layout(r, &rich, style, scale), tw)
    }

    /// Click/tap/space: finish the current line, or go to the next one.
    pub fn advance(&mut self, r: &mut Renderer, text: &mut TextSystem) {
        if !self.line_box.is_complete() {
            self.line_box.skip();
            return;
        }
        self.line = (self.line + 1) % LINES.len();
        let scale = self.line_box.layout.scale;
        self.line_box =
            Self::lay_out(text, r, LINES[self.line], &line_style(), scale, self.line_box.typewriter);
    }

    /// Re-rasterizes text when the window's pixel scale changes, keeping reveal progress.
    fn relayout(&mut self, r: &mut Renderer, text: &mut TextSystem, scale: f32) {
        if (self.line_box.layout.scale - scale).abs() / scale < 0.01 {
            return;
        }
        let elapsed = self.line_box.elapsed;
        self.name_box =
            Self::lay_out(text, r, "{b}Kulisa{/b}", &name_style(), scale, self.name_box.typewriter);
        self.line_box =
            Self::lay_out(text, r, LINES[self.line], &line_style(), scale, self.line_box.typewriter);
        self.line_box.elapsed = elapsed;
    }

    /// `pointer` is in design units; the camera drifts toward it. `scale` is the window's
    /// physical pixels per design unit.
    #[allow(clippy::too_many_arguments)]
    pub fn update(
        &mut self,
        r: &mut Renderer,
        text: &mut TextSystem,
        scale: f32,
        t: f32,
        dt: f32,
        tint: [f32; 4],
        angle: f32,
        pointer: Option<Vec2>,
    ) {
        self.relayout(r, text, scale);
        self.line_box.advance(dt);
        for (node, b) in [(self.name, &self.name_box), (self.dialogue, &self.line_box)] {
            b.quads(t, &mut self.quads);
            if let NodeKind::Quads(q) = &mut self.scene[node].kind {
                std::mem::swap(q, &mut self.quads);
            }
        }

        let logo = &mut self.scene[self.logo];
        logo.transform.rotation = angle;
        logo.tint = Color::rgba(tint[0], tint[1], tint[2], tint[3]);
        self.scene[self.mini].transform.rotation = t;

        let center = self.scene.camera.center();
        let aim = pointer.map_or(center, |p| center + (p - center) * 0.25);
        let cam = &mut self.scene.camera;
        cam.position += (aim - cam.position) * (1.0 - (-4.0 * dt).exp());
        cam.zoom = 1.0 + 0.03 * (t * 0.5).sin();
    }
}
