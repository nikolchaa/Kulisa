//! Smoke scene: exercises layers, parallax, a render target, a custom sprite shader,
//! clipping, a screen effect chain (M1), a typewriter dialogue box (M2) and a choice menu
//! driven by a `.kulisa` script (M3). Built in Rust until the Luau API (M4) can build it.

use kulisa_core::{
    BlendMode, Color, Material, Node, NodeId, NodeKind, Quad, Rect, Scene, TextureId, UVec2, Vec2,
};
use kulisa_render::{Image, Renderer};
use kulisa_text::{TextBox, TextStyle, TextSystem, Typewriter};

use crate::logo;

pub const DESIGN: Vec2 = Vec2::new(1920.0, 1080.0);

const TYPEWRITER: Typewriter = Typewriter { cps: 38.0, fade: 0.08 };
const INSTANT: Typewriter = Typewriter { cps: 0.0, fade: 0.0 };

/// A menu choice card: where it is, and its nodes.
struct Choice {
    index: usize,
    rect: Rect,
    backing: NodeId,
    label: NodeId,
    markup: String,
    text: TextBox,
}

pub struct Smoke {
    pub scene: Scene,
    white: TextureId,
    sky: NodeId,
    logo: NodeId,
    logo_spin: bool,
    mini: NodeId,
    name: NodeId,
    dialogue: NodeId,
    menu: NodeId,
    name_markup: String,
    name_color: Color,
    line_markup: String,
    name_box: TextBox,
    line_box: TextBox,
    choices: Vec<Choice>,
    scale: f32,
    quads: Vec<Quad>,
}

/// Escapes text so it shows literally inside markup.
fn literal(s: &str) -> String {
    s.replace('{', "{{").replace('}', "}}")
}

fn name_style(color: Color) -> TextStyle {
    TextStyle { font: Some("Inter".into()), size: 30.0, color, ..Default::default() }
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

fn choice_style() -> TextStyle {
    TextStyle {
        font: Some("Inter".into()),
        size: 30.0,
        width: Some(700.0),
        align: kulisa_text::Align::Center,
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
    pub fn build(r: &mut Renderer, scale: f32) -> Result<Smoke, String> {
        let mut scene = Scene::new(DESIGN);
        scene.clear = Color::hex("#111116").unwrap();
        let white = r.create_texture(&Image::solid(4, 4, [255; 4]));
        let sky_tex = r.create_texture(&gradient([17, 17, 22], [58, 40, 52]));
        let logo_tex = r.create_texture(&logo::image(256));
        let halo = r.create_texture(&radial(128));
        let wave = r.create_shader("wave.wgsl", include_str!("shaders/wave.wgsl"))?;
        let vignette = r.create_shader("vignette.wgsl", include_str!("shaders/vignette.wgsl"))?;
        let root = scene.root();

        // Far backdrop: a gradient wider than any screen, barely moving.
        let back = scene.add(root, Node::layer(0.1));
        let sky = scene.add(back, Node::sprite(sky_tex, Vec2::new(4000.0, 1600.0)).at(-1040.0, -260.0));

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

        // World: the logo (shown by the story, animated by Luau) and a render target shown
        // through a wave shader.
        let world = scene.add(root, Node::layer(1.0).with_z(10));
        let mut logo = Node::sprite(logo_tex, Vec2::splat(320.0)).centered().at(960.0, 520.0);
        logo.visible = false;
        let logo = scene.add(world, logo);
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

        // Dialogue box and menu: a typical visual novel UI, built from the same primitives.
        let ui = scene.add(root, Node::layer(0.0).with_z(30));
        let mut backing = Node::sprite(white, Vec2::new(1600.0, 236.0)).at(160.0, 800.0);
        backing.tint = Color::hex("#111116").unwrap();
        backing.opacity = 0.82;
        scene.add(ui, backing);
        let name = scene.add(ui, Node::quads(Vec::new()).at(200.0, 816.0));
        let dialogue = scene.add(ui, Node::quads(Vec::new()).at(200.0, 866.0));
        let menu = scene.add(ui, Node::group());

        scene.effects.push(Material { shader: vignette, params: params([0.35, 0.85, 0.0, 0.55]) });
        Ok(Smoke {
            scene,
            white,
            sky,
            logo,
            logo_spin: false,
            mini,
            name,
            dialogue,
            menu,
            name_markup: String::new(),
            name_color: Color::WHITE,
            line_markup: String::new(),
            name_box: TextBox::default(),
            line_box: TextBox::default(),
            choices: Vec::new(),
            scale,
            quads: Vec::new(),
        })
    }

    fn lay_out(
        text: &mut TextSystem,
        r: &mut Renderer,
        markup: &str,
        style: &TextStyle,
        scale: f32,
        tw: Typewriter,
    ) -> TextBox {
        // Story text is markup; a mistake in it shows the error instead of the line.
        let rich = kulisa_text::parse(markup).unwrap_or_else(|e| {
            let msg = format!(
                "{{color=#ff6a3d}}Markup error at byte {}:{{/color}} {}",
                e.offset,
                literal(&e.message)
            );
            kulisa_text::parse(&msg).expect("escaped error text is valid markup")
        });
        TextBox::new(text.layout(r, &rich, style, scale), tw)
    }

    /// Shows a line of dialogue, with the speaker's name and color if any.
    pub fn set_line(
        &mut self,
        r: &mut Renderer,
        text: &mut TextSystem,
        who: Option<(String, Color)>,
        markup: &str,
    ) {
        let (name, color) =
            who.map_or((String::new(), Color::WHITE), |(n, c)| (format!("{{b}}{}{{/b}}", literal(&n)), c));
        self.name_markup = name;
        self.name_color = color;
        self.line_markup = markup.to_string();
        self.name_box = Self::lay_out(text, r, &self.name_markup, &name_style(color), self.scale, INSTANT);
        self.line_box = Self::lay_out(text, r, markup, &line_style(), self.scale, TYPEWRITER);
    }

    /// Shows a script error in the dialogue box.
    pub fn set_error(&mut self, r: &mut Renderer, text: &mut TextSystem, error: &str) {
        let first = error.lines().next().unwrap_or(error);
        let who = Some(("Story error".to_string(), Color::hex("#ff6a3d").unwrap()));
        self.set_line(r, text, who, &literal(first));
        self.line_box.skip();
    }

    pub fn line_complete(&self) -> bool {
        self.line_box.is_complete()
    }

    pub fn skip_line(&mut self) {
        self.line_box.skip();
    }

    /// Shows menu choices as cards above the dialogue box.
    pub fn set_menu(&mut self, r: &mut Renderer, text: &mut TextSystem, items: &[(usize, String)]) {
        self.clear_menu();
        let top = 440.0 - items.len() as f32 * 46.0;
        for (i, (index, markup)) in items.iter().enumerate() {
            let rect = Rect::new(580.0, top + i as f32 * 92.0, 760.0, 72.0);
            let mut card = Node::sprite(self.white, rect.size()).at(rect.min.x, rect.min.y);
            card.tint = Color::hex("#111116").unwrap();
            card.opacity = 0.88;
            let backing = self.scene.add(self.menu, card);
            let label =
                self.scene.add(self.menu, Node::quads(Vec::new()).at(rect.min.x + 30.0, rect.min.y + 16.0));
            let text = Self::lay_out(text, r, markup, &choice_style(), self.scale, INSTANT);
            self.choices.push(Choice { index: *index, rect, backing, label, markup: markup.clone(), text });
        }
    }

    pub fn clear_menu(&mut self) {
        for c in self.choices.drain(..) {
            self.scene.remove(c.backing);
            self.scene.remove(c.label);
        }
    }

    /// The menu choice under `p` (design units), if any.
    pub fn menu_hit(&self, p: Vec2) -> Option<usize> {
        self.choices.iter().find(|c| c.rect.contains(p)).map(|c| c.index)
    }

    /// Stage: the scene backdrop. `dusk` (default) and `night` are known.
    pub fn set_scene(&mut self, image: &[String]) {
        let tint = match image.first().map(String::as_str) {
            Some("night") => Color::hex("#6f7fb8").unwrap(),
            Some("dusk") | None => Color::WHITE,
            Some(other) => {
                log::warn!("smoke scene has no backdrop `{other}`");
                Color::WHITE
            }
        };
        self.scene[self.sky].tint = tint;
        self.hide("logo"); // a new scene clears what was shown
    }

    /// Stage: shows an image. The smoke scene knows `logo` (attribute `spin`).
    pub fn show(&mut self, image: &[String], _at: Option<&str>) {
        match image.first().map(String::as_str) {
            Some("logo") => {
                self.scene[self.logo].visible = true;
                self.logo_spin = image.iter().any(|a| a == "spin");
            }
            other => log::warn!("smoke scene has no image `{}`", other.unwrap_or_default()),
        }
    }

    pub fn hide(&mut self, tag: &str) {
        if tag == "logo" {
            self.scene[self.logo].visible = false;
            self.logo_spin = false;
        }
    }

    /// Clears the stage before rebuilding it from saved state.
    pub fn reset_stage(&mut self) {
        self.set_scene(&[]);
    }

    /// Re-rasterizes text when the window's pixel scale changes, keeping reveal progress.
    fn relayout(&mut self, r: &mut Renderer, text: &mut TextSystem, scale: f32) {
        if (self.scale - scale).abs() / scale < 0.01 {
            return;
        }
        self.scale = scale;
        let elapsed = self.line_box.elapsed;
        self.name_box =
            Self::lay_out(text, r, &self.name_markup, &name_style(self.name_color), scale, INSTANT);
        self.line_box = Self::lay_out(text, r, &self.line_markup, &line_style(), scale, TYPEWRITER);
        self.line_box.elapsed = elapsed;
        for c in &mut self.choices {
            c.text = Self::lay_out(text, r, &c.markup, &choice_style(), scale, INSTANT);
        }
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
        let boxes = [(self.name, &self.name_box), (self.dialogue, &self.line_box)];
        let choices = self.choices.iter().map(|c| (c.label, &c.text));
        for (node, b) in boxes.into_iter().chain(choices) {
            b.quads(t, &mut self.quads);
            if let NodeKind::Quads(q) = &mut self.scene[node].kind {
                std::mem::swap(q, &mut self.quads);
            }
        }
        for c in &self.choices {
            let hover = pointer.is_some_and(|p| c.rect.contains(p));
            self.scene[c.backing].tint = Color::hex(if hover { "#3b2a2a" } else { "#111116" }).unwrap();
        }

        let logo = &mut self.scene[self.logo];
        logo.transform.rotation = if self.logo_spin { t * 2.5 } else { angle };
        logo.tint = Color::rgba(tint[0], tint[1], tint[2], tint[3]);
        self.scene[self.mini].transform.rotation = t;

        let center = self.scene.camera.center();
        let aim = pointer.map_or(center, |p| center + (p - center) * 0.25);
        let cam = &mut self.scene.camera;
        cam.position += (aim - cam.position) * (1.0 - (-4.0 * dt).exp());
        cam.zoom = 1.0 + 0.03 * (t * 0.5).sin();
    }
}
