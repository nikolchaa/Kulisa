//! Extraction: flattens a [`Scene`] into passes of batched draw items.
//!
//! All vertices are in the pixel space of their pass target, so the renderer only needs one
//! projection per pass. Consecutive draws with identical state are merged here, which is
//! what keeps draw calls low.

use std::ops::Range;

use bytemuck::{Pod, Zeroable};
use glam::{Affine2, UVec2, Vec2};

use crate::{
    BlendMode, Color, Effect, Material, Mesh, NodeId, NodeKind, Rect, Scene, Sprite, TextureId, ViewLayout,
};

/// Vertex in pass pixel space. `color` is linear, premultiplied, 8-bit.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Pod, Zeroable)]
pub struct Vertex {
    pub pos: [f32; 2],
    pub uv: [f32; 2],
    pub color: [u8; 4],
}

/// What a draw samples.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum TexRef {
    /// A 1x1 white texel (solid colors).
    White,
    Texture(TextureId),
    /// A screen-sized scratch texture produced by an earlier pass in the same frame.
    Transient(u32),
}

#[derive(Clone, Debug, PartialEq)]
pub struct DrawItem {
    pub texture: TexRef,
    pub blend: BlendMode,
    pub material: Option<Material>,
    /// Scissor rectangle in target pixels: x, y, width, height.
    pub clip: Option<[u32; 4]>,
    pub vertices: Range<u32>,
    pub indices: Range<u32>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PassTarget {
    /// The window (or the headless output).
    Surface,
    Texture(TextureId),
    Transient(u32),
}

#[derive(Clone, Debug, PartialEq)]
pub struct Pass {
    pub target: PassTarget,
    pub size: UVec2,
    pub clear: Option<Color>,
    pub items: Vec<DrawItem>,
    /// Applied in order after the items are drawn; the last one writes into `target`.
    pub effects: Vec<Effect>,
}

/// One frame's worth of geometry and passes, in execution order. Reused across frames.
#[derive(Default)]
pub struct Frame {
    pub vertices: Vec<Vertex>,
    pub indices: Vec<u32>,
    pub passes: Vec<Pass>,
    /// Number of transient textures the passes use.
    pub transients: u32,
}

impl Frame {
    pub fn clear(&mut self) {
        self.vertices.clear();
        self.indices.clear();
        self.passes.clear();
        self.transients = 0;
    }

    /// Draw items across all passes, i.e. an upper bound on draw calls (effects excluded).
    pub fn draw_items(&self) -> usize {
        self.passes.iter().map(|p| p.items.len()).sum()
    }
}

#[derive(Clone, Copy)]
struct Ctx {
    model: Affine2,
    base: Affine2,
    camera: bool,
    parallax: f32,
    opacity: f32,
    tint: Color,
    clip: Option<Rect>,
    bounds: Rect,
}

struct Walker<'a> {
    scene: &'a Scene,
    frame: &'a mut Frame,
    window: UVec2,
}

/// Flattens `scene` into `frame` for a window of `window` pixels laid out by `layout`.
pub fn extract(scene: &Scene, layout: &ViewLayout, window: UVec2, frame: &mut Frame) {
    frame.clear();
    let window = window.max(UVec2::ONE);
    let ctx = Ctx {
        model: Affine2::IDENTITY,
        base: layout.to_pixels,
        camera: true,
        parallax: 1.0,
        opacity: 1.0,
        tint: Color::WHITE,
        clip: layout.letterbox,
        bounds: Rect { min: Vec2::ZERO, max: window.as_vec2() },
    };
    let mut items = Vec::new();
    let mut w = Walker { scene, frame, window };
    w.children(scene.root(), ctx, &mut items);
    w.frame.passes.push(Pass {
        target: PassTarget::Surface,
        size: window,
        clear: Some(scene.clear),
        items,
        effects: scene.effects.clone(),
    });
}

/// The quads a sprite draws, in local units with their UVs: one, or up to nine for a
/// nine-slice. Borders larger than the sprite shrink proportionally.
fn sprite_parts(s: &Sprite) -> Vec<(Rect, Rect)> {
    let full = Rect { min: Vec2::ZERO, max: s.size };
    let Some(sl) = s.slice else { return vec![(full, s.uv)] };
    let src = sl.source.max(Vec2::ONE);
    let shrink = |a: f32, b: f32, room: f32| {
        let total = (a + b) * sl.scale;
        if total > room && total > 0.0 {
            (a * sl.scale * room / total, b * sl.scale * room / total)
        } else {
            (a * sl.scale, b * sl.scale)
        }
    };
    let (l, r) = shrink(sl.left, sl.right, s.size.x.abs());
    let (t, b) = shrink(sl.top, sl.bottom, s.size.y.abs());
    let xs = [0.0, l, s.size.x - r, s.size.x];
    let ys = [0.0, t, s.size.y - b, s.size.y];
    let uv = s.uv;
    let us =
        [0.0, sl.left / src.x, 1.0 - sl.right / src.x, 1.0].map(|f| uv.min.x + (uv.max.x - uv.min.x) * f);
    let vs =
        [0.0, sl.top / src.y, 1.0 - sl.bottom / src.y, 1.0].map(|f| uv.min.y + (uv.max.y - uv.min.y) * f);
    let mut parts = Vec::with_capacity(9);
    for j in 0..3 {
        for i in 0..3 {
            let rect = Rect { min: Vec2::new(xs[i], ys[j]), max: Vec2::new(xs[i + 1], ys[j + 1]) };
            if rect.is_empty() {
                continue;
            }
            let uv = Rect { min: Vec2::new(us[i], vs[j]), max: Vec2::new(us[i + 1], vs[j + 1]) };
            parts.push((rect, uv));
        }
    }
    parts
}

impl Walker<'_> {
    fn children(&mut self, id: NodeId, ctx: Ctx, items: &mut Vec<DrawItem>) {
        self.children_in(id, ctx, items, |_| true);
    }

    /// Visits children (sorted by z, stable) for which `keep(z)` holds.
    fn children_in(&mut self, id: NodeId, ctx: Ctx, items: &mut Vec<DrawItem>, keep: impl Fn(i32) -> bool) {
        let mut kids: Vec<NodeId> =
            self.scene[id].children().iter().copied().filter(|&c| keep(self.scene[c].z)).collect();
        kids.sort_by_key(|&c| self.scene[c].z); // stable: ties keep insertion order
        for c in kids {
            self.node(c, ctx, items);
        }
    }

    fn to_pixels(&self, c: &Ctx) -> Affine2 {
        let view = if c.camera { self.scene.camera.view(c.parallax) } else { Affine2::IDENTITY };
        c.base * view * c.model
    }

    fn node(&mut self, id: NodeId, parent: Ctx, items: &mut Vec<DrawItem>) {
        let node = &self.scene[id];
        if !node.visible || node.opacity <= 0.0 {
            return;
        }
        let mut c = Ctx {
            model: parent.model * node.transform.to_affine(),
            opacity: parent.opacity * node.opacity,
            tint: parent.tint * node.tint,
            ..parent
        };
        if let NodeKind::Layer(l) = &node.kind {
            c.parallax = l.parallax;
        }
        if let Some(r) = node.clip {
            let m = self.to_pixels(&c);
            let corners = [r.min, Vec2::new(r.max.x, r.min.y), r.max, Vec2::new(r.min.x, r.max.y)];
            let aabb = Rect::bounding(&corners.map(|p| m.transform_point2(p)));
            let clip = c.clip.map_or(aabb, |old| old.intersect(&aabb));
            if clip.is_empty() {
                return;
            }
            c.clip = Some(clip);
        }

        match &node.kind {
            NodeKind::Group => self.children(id, c, items),
            NodeKind::Layer(l) if l.effects.is_empty() => self.children(id, c, items),
            NodeKind::Layer(l) => {
                let effects = l.effects.clone();
                let slot = self.frame.transients;
                self.frame.transients += 1;
                let mut sub = Vec::new();
                self.children(id, c, &mut sub);
                self.frame.passes.push(Pass {
                    target: PassTarget::Transient(slot),
                    size: self.window,
                    clear: Some(Color::TRANSPARENT),
                    items: sub,
                    effects,
                });
                // Composite the processed layer back, clipped like its parent.
                let full = Rect { min: Vec2::ZERO, max: self.window.as_vec2() };
                let quad = [full.min, Vec2::new(full.max.x, 0.0), full.max, Vec2::new(0.0, full.max.y)];
                let item = self.quad(
                    quad,
                    Rect::UNIT,
                    Color::WHITE,
                    TexRef::Transient(slot),
                    BlendMode::Alpha,
                    None,
                );
                self.push(items, item, parent.clip, parent.bounds);
            }
            NodeKind::Target(t) => {
                let (texture, size, clear, effects) = (t.texture, t.size, t.clear, t.effects.clone());
                let inner = Ctx {
                    model: Affine2::IDENTITY,
                    base: Affine2::IDENTITY,
                    camera: false,
                    parallax: 1.0,
                    opacity: 1.0,
                    tint: Color::WHITE,
                    clip: None,
                    bounds: Rect { min: Vec2::ZERO, max: size.as_vec2() },
                };
                let mut sub = Vec::new();
                self.children(id, inner, &mut sub);
                self.frame.passes.push(Pass {
                    target: PassTarget::Texture(texture),
                    size,
                    clear,
                    items: sub,
                    effects,
                });
            }
            NodeKind::Quads(quads) => {
                let m = self.to_pixels(&c);
                for q in quads {
                    let r = q.rect;
                    let corners = [r.min, Vec2::new(r.max.x, r.min.y), r.max, Vec2::new(r.min.x, r.max.y)];
                    let tint = c.tint * q.color;
                    let tint = Color { a: tint.a * c.opacity, ..tint };
                    if tint.a <= 0.0 {
                        continue;
                    }
                    let item = self.quad(
                        corners.map(|p| m.transform_point2(p)),
                        q.uv,
                        tint,
                        TexRef::Texture(q.texture),
                        node.blend,
                        node.material,
                    );
                    self.push(items, item, c.clip, c.bounds);
                }
                self.children(id, c, items);
            }
            NodeKind::Mesh(mesh) => {
                let m = self.to_pixels(&c);
                if let Some(item) = self.mesh(mesh, m, &c, node.blend, node.material) {
                    self.push(items, item, c.clip, c.bounds);
                }
                self.children(id, c, items);
            }
            NodeKind::Sprite(s) => {
                let m = self.to_pixels(&c);
                let tint = Color { a: c.tint.a * c.opacity, ..c.tint };
                // Children with negative z go behind the sprite (shadows, glows).
                self.children_in(id, c, items, |z| z < 0);
                for (rect, uv) in sprite_parts(s) {
                    let r = rect;
                    let quad = [r.min, Vec2::new(r.max.x, r.min.y), r.max, Vec2::new(r.min.x, r.max.y)]
                        .map(|p| m.transform_point2(p));
                    let item =
                        self.quad(quad, uv, tint, TexRef::Texture(s.texture), node.blend, node.material);
                    self.push(items, item, c.clip, c.bounds);
                }
                self.children_in(id, c, items, |z| z >= 0);
            }
        }
    }

    /// Appends a quad's geometry and returns its (unclipped) draw item.
    fn quad(
        &mut self,
        p: [Vec2; 4],
        uv: Rect,
        tint: Color,
        texture: TexRef,
        blend: BlendMode,
        material: Option<Material>,
    ) -> DrawItem {
        let color = tint.premultiplied().map(|v| (v.clamp(0.0, 1.0) * 255.0 + 0.5) as u8);
        let uvs = [uv.min, Vec2::new(uv.max.x, uv.min.y), uv.max, Vec2::new(uv.min.x, uv.max.y)];
        let v0 = self.frame.vertices.len() as u32;
        let i0 = self.frame.indices.len() as u32;
        for k in 0..4 {
            self.frame.vertices.push(Vertex { pos: p[k].to_array(), uv: uvs[k].to_array(), color });
        }
        self.frame.indices.extend([0, 1, 2, 0, 2, 3].map(|i| v0 + i));
        DrawItem { texture, blend, material, clip: None, vertices: v0..v0 + 4, indices: i0..i0 + 6 }
    }

    /// Appends a mesh's geometry (positions transformed by `m`, vertex colors times the
    /// inherited tint) and returns its draw item, or `None` for an empty or invisible mesh.
    fn mesh(
        &mut self,
        mesh: &Mesh,
        m: Affine2,
        c: &Ctx,
        blend: BlendMode,
        material: Option<Material>,
    ) -> Option<DrawItem> {
        if mesh.indices.is_empty() || mesh.positions.is_empty() || c.tint.a * c.opacity <= 0.0 {
            return None;
        }
        let v0 = self.frame.vertices.len() as u32;
        let i0 = self.frame.indices.len() as u32;
        for ((p, uv), col) in mesh.positions.iter().zip(&mesh.uvs).zip(&mesh.colors) {
            let t = c.tint * *col;
            let t = Color { a: t.a * c.opacity, ..t };
            let color = t.premultiplied().map(|v| (v.clamp(0.0, 1.0) * 255.0 + 0.5) as u8);
            self.frame.vertices.push(Vertex {
                pos: m.transform_point2(*p).to_array(),
                uv: uv.to_array(),
                color,
            });
        }
        let n = mesh.positions.len() as u32;
        // Indices past the vertex count cannot come from a validated mesh; guard anyway.
        self.frame.indices.extend(mesh.indices.iter().map(|&i| v0 + i.min(n - 1)));
        Some(DrawItem {
            texture: TexRef::Texture(mesh.texture),
            blend,
            material,
            clip: None,
            vertices: v0..v0 + n,
            indices: i0..i0 + mesh.indices.len() as u32,
        })
    }

    /// Culls, applies the clip and merges with the previous item when the state matches.
    fn push(&mut self, items: &mut Vec<DrawItem>, mut item: DrawItem, clip: Option<Rect>, bounds: Rect) {
        let pts: Vec<Vec2> = self.frame.vertices[item.vertices.start as usize..item.vertices.end as usize]
            .iter()
            .map(|v| Vec2::from(v.pos))
            .collect();
        let visible = clip.map_or(bounds, |c| c.intersect(&bounds));
        if Rect::bounding(&pts).intersect(&visible).is_empty() {
            // Culled: drop the geometry we just appended.
            self.frame.vertices.truncate(item.vertices.start as usize);
            self.frame.indices.truncate(item.indices.start as usize);
            return;
        }
        item.clip = clip.map(|c| {
            let c = c.intersect(&bounds);
            let (x0, y0) = (c.min.x.floor().max(0.0) as u32, c.min.y.floor().max(0.0) as u32);
            let (x1, y1) = (c.max.x.ceil().max(0.0) as u32, c.max.y.ceil().max(0.0) as u32);
            [x0, y0, x1.saturating_sub(x0), y1.saturating_sub(y0)]
        });
        if let Some(last) = items.last_mut()
            && last.texture == item.texture
            && last.blend == item.blend
            && last.material == item.material
            && last.clip == item.clip
            && last.vertices.end == item.vertices.start
            && last.indices.end == item.indices.start
        {
            last.vertices.end = item.vertices.end;
            last.indices.end = item.indices.end;
            return;
        }
        items.push(item);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Node, ScaleMode, Viewport};

    const DESIGN: Vec2 = Vec2::new(1920.0, 1080.0);
    const WINDOW: UVec2 = UVec2::new(1920, 1080);

    fn run(scene: &Scene) -> Frame {
        let layout = Viewport::new(DESIGN.x, DESIGN.y, ScaleMode::Expand).layout(WINDOW);
        let mut f = Frame::default();
        extract(scene, &layout, WINDOW, &mut f);
        f
    }

    fn tex(n: u32) -> TextureId {
        TextureId(n)
    }

    fn first_vertex(f: &Frame, pass: usize, item: usize) -> Vec2 {
        Vec2::from(f.vertices[f.passes[pass].items[item].vertices.start as usize].pos)
    }

    #[test]
    fn nested_transforms_place_sprites() {
        let mut s = Scene::new(DESIGN);
        let g = s.add(s.root(), Node::group().at(100.0, 50.0));
        s.add(g, Node::sprite(tex(1), Vec2::new(10.0, 10.0)).at(5.0, 5.0));
        let f = run(&s);
        assert_eq!(f.passes.len(), 1);
        assert_eq!(first_vertex(&f, 0, 0), Vec2::new(105.0, 55.0));
    }

    #[test]
    fn same_state_sprites_batch_into_one_item() {
        let mut s = Scene::new(DESIGN);
        for i in 0..100 {
            s.add(s.root(), Node::sprite(tex(1), Vec2::splat(8.0)).at(i as f32 * 10.0, 0.0));
        }
        let f = run(&s);
        assert_eq!(f.draw_items(), 1);
        assert_eq!(f.passes[0].items[0].indices.len(), 600);

        s.add(s.root(), Node::sprite(tex(2), Vec2::splat(8.0)));
        s.add(s.root(), Node::sprite(tex(1), Vec2::splat(8.0)));
        assert_eq!(run(&s).draw_items(), 3, "texture changes break the batch");
    }

    #[test]
    fn z_orders_siblings_stably() {
        let mut s = Scene::new(DESIGN);
        s.add(s.root(), Node::sprite(tex(1), Vec2::ONE).at(1.0, 0.0).with_z(5));
        s.add(s.root(), Node::sprite(tex(2), Vec2::ONE).at(2.0, 0.0));
        s.add(s.root(), Node::sprite(tex(3), Vec2::ONE).at(3.0, 0.0));
        let f = run(&s);
        let order: Vec<_> = f.passes[0].items.iter().map(|i| i.texture).collect();
        assert_eq!(order, vec![TexRef::Texture(tex(2)), TexRef::Texture(tex(3)), TexRef::Texture(tex(1))]);
    }

    #[test]
    fn negative_z_children_draw_behind_their_sprite() {
        let mut s = Scene::new(DESIGN);
        let parent = s.add(s.root(), Node::sprite(tex(1), Vec2::ONE));
        s.add(parent, Node::sprite(tex(2), Vec2::ONE).with_z(-1));
        s.add(parent, Node::sprite(tex(3), Vec2::ONE));
        let f = run(&s);
        let order: Vec<_> = f.passes[0].items.iter().map(|i| i.texture).collect();
        assert_eq!(order, vec![TexRef::Texture(tex(2)), TexRef::Texture(tex(1)), TexRef::Texture(tex(3))]);
    }

    #[test]
    fn quads_place_tint_and_batch() {
        let mut s = Scene::new(DESIGN);
        let q = |x: f32, a: f32| crate::Quad {
            texture: tex(1),
            rect: Rect::new(x, 0.0, 4.0, 4.0),
            uv: Rect::UNIT,
            color: Color::rgba(1.0, 1.0, 1.0, a),
        };
        s.add(s.root(), Node::quads(vec![q(0.0, 1.0), q(10.0, 0.0), q(20.0, 1.0)]).at(100.0, 100.0));
        let f = run(&s);
        assert_eq!(f.draw_items(), 1, "one batch; the transparent quad is skipped");
        assert_eq!(f.vertices.len(), 8);
        assert_eq!(f.vertices[4].pos, [120.0, 100.0]);
    }

    #[test]
    fn layers_follow_camera_by_parallax() {
        let mut s = Scene::new(DESIGN);
        let far = s.add(s.root(), Node::layer(0.5));
        let hud = s.add(s.root(), Node::layer(0.0));
        s.add(far, Node::sprite(tex(1), Vec2::ONE).at(500.0, 500.0));
        s.add(hud, Node::sprite(tex(2), Vec2::ONE).at(500.0, 500.0));
        s.add(s.root(), Node::sprite(tex(3), Vec2::ONE).at(500.0, 500.0));
        s.camera.position.x += 100.0;
        let f = run(&s);
        assert_eq!(first_vertex(&f, 0, 0), Vec2::new(450.0, 500.0));
        assert_eq!(first_vertex(&f, 0, 1), Vec2::new(500.0, 500.0));
        assert_eq!(first_vertex(&f, 0, 2), Vec2::new(400.0, 500.0));
    }

    #[test]
    fn opacity_and_tint_inherit_premultiplied() {
        let mut s = Scene::new(DESIGN);
        let mut g = Node::group();
        g.opacity = 0.5;
        let g = s.add(s.root(), g);
        let mut sp = Node::sprite(tex(1), Vec2::ONE);
        sp.tint = Color::rgba(1.0, 0.0, 0.0, 1.0);
        s.add(g, sp);
        let f = run(&s);
        assert_eq!(f.vertices[0].color, [128, 0, 0, 128]);
    }

    #[test]
    fn invisible_transparent_and_offscreen_nodes_are_skipped() {
        let mut s = Scene::new(DESIGN);
        let mut a = Node::sprite(tex(1), Vec2::ONE);
        a.visible = false;
        s.add(s.root(), a);
        let mut b = Node::sprite(tex(1), Vec2::ONE);
        b.opacity = 0.0;
        s.add(s.root(), b);
        s.add(s.root(), Node::sprite(tex(1), Vec2::ONE).at(-500.0, -500.0));
        let f = run(&s);
        assert_eq!(f.draw_items(), 0);
        assert!(f.vertices.is_empty(), "culled geometry is not kept");
    }

    #[test]
    fn clip_sets_scissor_and_culls() {
        let mut s = Scene::new(DESIGN);
        let mut g = Node::group().at(100.0, 100.0);
        g.clip = Some(Rect::new(0.0, 0.0, 50.0, 40.0));
        let g = s.add(s.root(), g);
        s.add(g, Node::sprite(tex(1), Vec2::splat(30.0)));
        s.add(g, Node::sprite(tex(1), Vec2::splat(10.0)).at(200.0, 0.0)); // outside the clip
        let f = run(&s);
        assert_eq!(f.draw_items(), 1);
        assert_eq!(f.passes[0].items[0].clip, Some([100, 100, 50, 40]));
    }

    #[test]
    fn targets_render_first_in_their_own_space() {
        let mut s = Scene::new(DESIGN);
        let t = s.add(s.root(), Node::target(tex(9), UVec2::new(256, 256)).at(999.0, 999.0));
        s.add(t, Node::sprite(tex(1), Vec2::splat(16.0)).at(10.0, 20.0));
        s.add(s.root(), Node::sprite(tex(9), Vec2::splat(256.0)).at(800.0, 400.0));
        s.camera.position.x += 300.0; // targets ignore the camera
        let f = run(&s);
        assert_eq!(f.passes.len(), 2);
        assert_eq!(f.passes[0].target, PassTarget::Texture(tex(9)));
        assert_eq!(f.passes[0].size, UVec2::new(256, 256));
        assert_eq!(first_vertex(&f, 0, 0), Vec2::new(10.0, 20.0));
        assert_eq!(f.passes[1].target, PassTarget::Surface);
        assert_eq!(f.passes[1].items[0].texture, TexRef::Texture(tex(9)));
    }

    #[test]
    fn layer_effects_render_to_transient_and_composite() {
        let mut s = Scene::new(DESIGN);
        let mut layer = Node::layer(1.0);
        if let NodeKind::Layer(l) = &mut layer.kind {
            l.effects.push(Material::new(crate::ShaderId(1), [0.0; 16]));
        }
        let l = s.add(s.root(), layer);
        s.add(l, Node::sprite(tex(1), Vec2::ONE));
        let f = run(&s);
        assert_eq!(f.transients, 1);
        assert_eq!(f.passes[0].target, PassTarget::Transient(0));
        assert_eq!(f.passes[0].effects.len(), 1);
        assert_eq!(f.passes[1].items[0].texture, TexRef::Transient(0));
    }

    #[test]
    fn meshes_transform_tint_and_batch_with_sprites() {
        let mut s = Scene::new(DESIGN);
        let mut mesh = Mesh::grid(tex(1), 2, 1, Vec2::new(20.0, 10.0), Rect::UNIT);
        mesh.colors[1] = Color::rgba(1.0, 0.0, 0.0, 1.0);
        mesh.positions[0].y -= 5.0; // deformed
        let mut node = Node::mesh(mesh).at(100.0, 50.0);
        node.opacity = 0.5;
        s.add(s.root(), node);
        s.add(s.root(), Node::sprite(tex(1), Vec2::splat(8.0)));
        let f = run(&s);
        assert_eq!(f.draw_items(), 1, "same texture and state: one batch");
        // 6 mesh vertices, then the sprite's 4.
        assert_eq!(f.vertices.len(), 10);
        assert_eq!(f.indices.len(), 12 + 6);
        assert_eq!(f.vertices[0].pos, [100.0, 45.0]);
        assert_eq!(f.vertices[0].color, [128, 128, 128, 128], "opacity premultiplied");
        assert_eq!(f.vertices[1].color, [128, 0, 0, 128], "vertex color multiplies the tint");
        // The sprite's indices point at its own vertices, after the mesh's.
        assert_eq!(f.indices[12], 6);
        assert!(f.indices[..12].iter().all(|&i| i < 6));
    }

    #[test]
    fn empty_and_offscreen_meshes_draw_nothing() {
        let mut s = Scene::new(DESIGN);
        let mut m = Mesh::grid(tex(1), 1, 1, Vec2::splat(10.0), Rect::UNIT);
        m.indices.clear();
        s.add(s.root(), Node::mesh(m));
        s.add(
            s.root(),
            Node::mesh(Mesh::grid(tex(1), 1, 1, Vec2::splat(10.0), Rect::UNIT)).at(-900.0, -900.0),
        );
        let f = run(&s);
        assert_eq!(f.draw_items(), 0);
        assert!(f.vertices.is_empty());
    }

    #[test]
    fn nine_slice_keeps_corners_and_stretches_the_middle() {
        let mut s = Scene::new(DESIGN);
        let mut n = Node::sprite(tex(1), Vec2::new(200.0, 100.0)).at(10.0, 10.0);
        if let NodeKind::Sprite(sp) = &mut n.kind {
            sp.slice = Some(crate::Slice {
                left: 8.0,
                top: 8.0,
                right: 8.0,
                bottom: 8.0,
                source: Vec2::new(32.0, 32.0),
                scale: 2.0,
            });
        }
        s.add(s.root(), n);
        let f = run(&s);
        assert_eq!(f.draw_items(), 1, "the nine parts batch");
        assert_eq!(f.vertices.len(), 36);
        // Top-left corner: 16x16 units, UVs 0..0.25.
        assert_eq!(f.vertices[2].pos, [26.0, 26.0]);
        assert_eq!(f.vertices[2].uv, [0.25, 0.25]);
        // Middle part spans the rest; last corner ends at the sprite's far edge.
        assert_eq!(f.vertices[35 - 1].pos, [210.0, 110.0]);
        assert_eq!(f.vertices[35 - 1].uv, [1.0, 1.0]);

        // Borders larger than the sprite shrink instead of overlapping.
        let mut small = Sprite { texture: tex(1), size: Vec2::new(20.0, 20.0), uv: Rect::UNIT, slice: None };
        small.slice = Some(crate::Slice {
            left: 16.0,
            top: 0.0,
            right: 16.0,
            bottom: 0.0,
            source: Vec2::splat(32.0),
            scale: 1.0,
        });
        let parts = sprite_parts(&small);
        assert_eq!(parts.len(), 2, "the empty middle column is dropped");
        assert_eq!(parts[0].0.max.x, 10.0);
    }
}
