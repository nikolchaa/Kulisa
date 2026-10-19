//! Retained scene graph: nodes in a generational arena, addressed by [`NodeId`].

use glam::{Affine2, UVec2, Vec2};

use crate::{Camera, Color, Mesh};

/// A texture owned by the renderer (images and render targets).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct TextureId(pub u32);

/// A WGSL shader registered with the renderer.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ShaderId(pub u32);

/// Shader parameters: four `vec4<f32>`, visible to WGSL as `params.p[0..4]`.
pub type Params = [f32; 16];

/// Axis-aligned rectangle.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rect {
    pub min: Vec2,
    pub max: Vec2,
}

impl Rect {
    pub const UNIT: Rect = Rect { min: Vec2::ZERO, max: Vec2::ONE };

    pub fn new(x: f32, y: f32, w: f32, h: f32) -> Self {
        Self { min: Vec2::new(x, y), max: Vec2::new(x + w, y + h) }
    }

    pub fn size(&self) -> Vec2 {
        self.max - self.min
    }

    pub fn is_empty(&self) -> bool {
        self.max.x <= self.min.x || self.max.y <= self.min.y
    }

    pub fn intersect(&self, o: &Rect) -> Rect {
        Rect { min: self.min.max(o.min), max: self.max.min(o.max) }
    }

    pub fn contains(&self, p: Vec2) -> bool {
        p.x >= self.min.x && p.y >= self.min.y && p.x < self.max.x && p.y < self.max.y
    }

    /// Bounding box of `points`.
    pub fn bounding(points: &[Vec2]) -> Rect {
        let mut r = Rect { min: Vec2::splat(f32::INFINITY), max: Vec2::splat(f32::NEG_INFINITY) };
        for &p in points {
            r.min = r.min.min(p);
            r.max = r.max.max(p);
        }
        r
    }
}

/// How a node's pixels combine with what is already drawn. Colors are premultiplied.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum BlendMode {
    #[default]
    Alpha,
    Add,
    Multiply,
    Screen,
}

/// A custom shader with its parameters, used for sprites and full-screen effects.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Material {
    pub shader: ShaderId,
    pub params: Params,
    /// A second texture for the shader (`mask_tex` in WGSL): wipe masks, lookup tables.
    pub mask: Option<TextureId>,
}

impl Material {
    pub fn new(shader: ShaderId, params: Params) -> Self {
        Self { shader, params, mask: None }
    }
}

/// A full-screen pass in a post-processing chain. Same shader contract as [`Material`].
pub type Effect = Material;

/// Local transform. `origin` is the pivot in local units (e.g. half the size to rotate a
/// sprite around its center).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Transform {
    pub position: Vec2,
    pub rotation: f32,
    pub scale: Vec2,
    pub origin: Vec2,
}

impl Default for Transform {
    fn default() -> Self {
        Self { position: Vec2::ZERO, rotation: 0.0, scale: Vec2::ONE, origin: Vec2::ZERO }
    }
}

impl Transform {
    pub fn at(x: f32, y: f32) -> Self {
        Self { position: Vec2::new(x, y), ..Default::default() }
    }

    pub fn to_affine(&self) -> Affine2 {
        Affine2::from_scale_angle_translation(self.scale, self.rotation, self.position)
            * Affine2::from_translation(-self.origin)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Sprite {
    pub texture: TextureId,
    /// Size in local units.
    pub size: Vec2,
    /// Normalized texture region; flip by swapping min and max.
    pub uv: Rect,
    /// Nine-slice borders: the corners keep their size and the edges and middle stretch.
    pub slice: Option<Slice>,
}

/// Nine-slice borders of a sprite, in texture pixels (`source` is the region's size in
/// texture pixels). Borders are drawn `scale` times their pixel size.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Slice {
    pub left: f32,
    pub top: f32,
    pub right: f32,
    pub bottom: f32,
    pub source: Vec2,
    pub scale: f32,
}

/// Groups content that moves with the camera by `parallax` (1 = world, 0 = fixed to the
/// screen, between = farther away) and optionally post-processes it as a whole.
#[derive(Clone, Debug, PartialEq)]
pub struct Layer {
    pub parallax: f32,
    pub effects: Vec<Effect>,
}

/// Renders its children into `texture` (render-to-texture). Children use target pixel
/// coordinates and ignore the camera. Draws nothing itself; show it with a sprite.
#[derive(Clone, Debug, PartialEq)]
pub struct Target {
    pub texture: TextureId,
    pub size: UVec2,
    pub clear: Option<Color>,
    pub effects: Vec<Effect>,
}

/// One textured quad of a [`NodeKind::Quads`] node, in the node's local space.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Quad {
    pub texture: TextureId,
    pub rect: Rect,
    pub uv: Rect,
    /// Multiplied with the node's inherited tint and opacity.
    pub color: Color,
}

#[derive(Clone, Debug, PartialEq)]
pub enum NodeKind {
    Group,
    Sprite(Sprite),
    /// Many textured quads drawn as one node: text glyphs, nine-slices, tile runs.
    Quads(Vec<Quad>),
    /// A triangle mesh whose vertices scripts edit (deformable sprites, cloth, peels).
    Mesh(Mesh),
    Layer(Layer),
    Target(Target),
}

/// A scene node. Transform, opacity, tint, clip and layer parallax are inherited by children;
/// blend and material apply to the node's own drawing.
#[derive(Clone, Debug)]
pub struct Node {
    pub kind: NodeKind,
    pub transform: Transform,
    /// Draw order among siblings (lower first; ties keep insertion order). Children of a
    /// sprite with negative z draw behind it.
    pub z: i32,
    pub visible: bool,
    pub opacity: f32,
    pub tint: Color,
    pub blend: BlendMode,
    /// Clip rectangle in local units. Rotated clips use their bounding box.
    pub clip: Option<Rect>,
    pub material: Option<Material>,
    parent: Option<NodeId>,
    children: Vec<NodeId>,
}

impl Node {
    pub fn new(kind: NodeKind) -> Self {
        Self {
            kind,
            transform: Transform::default(),
            z: 0,
            visible: true,
            opacity: 1.0,
            tint: Color::WHITE,
            blend: BlendMode::Alpha,
            clip: None,
            material: None,
            parent: None,
            children: Vec::new(),
        }
    }

    pub fn group() -> Self {
        Self::new(NodeKind::Group)
    }

    pub fn sprite(texture: TextureId, size: Vec2) -> Self {
        Self::new(NodeKind::Sprite(Sprite { texture, size, uv: Rect::UNIT, slice: None }))
    }

    pub fn quads(quads: Vec<Quad>) -> Self {
        Self::new(NodeKind::Quads(quads))
    }

    pub fn mesh(mesh: Mesh) -> Self {
        Self::new(NodeKind::Mesh(mesh))
    }

    pub fn layer(parallax: f32) -> Self {
        Self::new(NodeKind::Layer(Layer { parallax, effects: Vec::new() }))
    }

    pub fn target(texture: TextureId, size: UVec2) -> Self {
        Self::new(NodeKind::Target(Target {
            texture,
            size,
            clear: Some(Color::TRANSPARENT),
            effects: Vec::new(),
        }))
    }

    pub fn at(mut self, x: f32, y: f32) -> Self {
        self.transform.position = Vec2::new(x, y);
        self
    }

    pub fn with_z(mut self, z: i32) -> Self {
        self.z = z;
        self
    }

    /// Centers the pivot on a sprite (rotation and scale around its middle).
    pub fn centered(mut self) -> Self {
        if let NodeKind::Sprite(s) = &self.kind {
            self.transform.origin = s.size * 0.5;
        }
        self
    }

    pub fn parent(&self) -> Option<NodeId> {
        self.parent
    }

    pub fn children(&self) -> &[NodeId] {
        &self.children
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct NodeId {
    index: u32,
    generation: u32,
}

struct Slot {
    generation: u32,
    node: Option<Node>,
}

/// The scene: a node tree plus the camera, screen effects and clear color.
pub struct Scene {
    slots: Vec<Slot>,
    free: Vec<u32>,
    root: NodeId,
    pub camera: Camera,
    /// Post-processing chain applied to the whole screen.
    pub effects: Vec<Effect>,
    pub clear: Color,
}

impl Scene {
    /// An empty scene for a game designed at `design` resolution.
    pub fn new(design: Vec2) -> Self {
        let mut scene = Self {
            slots: Vec::new(),
            free: Vec::new(),
            root: NodeId { index: 0, generation: 0 },
            camera: Camera::new(design * 0.5),
            effects: Vec::new(),
            clear: Color::BLACK,
        };
        scene.root = scene.insert(Node::group());
        scene
    }

    pub fn root(&self) -> NodeId {
        self.root
    }

    fn insert(&mut self, node: Node) -> NodeId {
        if let Some(index) = self.free.pop() {
            let slot = &mut self.slots[index as usize];
            slot.node = Some(node);
            NodeId { index, generation: slot.generation }
        } else {
            self.slots.push(Slot { generation: 0, node: Some(node) });
            NodeId { index: self.slots.len() as u32 - 1, generation: 0 }
        }
    }

    /// Adds `node` as the last child of `parent`.
    pub fn add(&mut self, parent: NodeId, mut node: Node) -> NodeId {
        assert!(self.contains(parent), "parent {parent:?} is not in the scene");
        node.parent = Some(parent);
        node.children.clear();
        let id = self.insert(node);
        self.slots[parent.index as usize].node.as_mut().unwrap().children.push(id);
        id
    }

    pub fn contains(&self, id: NodeId) -> bool {
        self.get(id).is_some()
    }

    pub fn get(&self, id: NodeId) -> Option<&Node> {
        let slot = self.slots.get(id.index as usize)?;
        if slot.generation == id.generation { slot.node.as_ref() } else { None }
    }

    pub fn get_mut(&mut self, id: NodeId) -> Option<&mut Node> {
        let slot = self.slots.get_mut(id.index as usize)?;
        if slot.generation == id.generation { slot.node.as_mut() } else { None }
    }

    /// Removes a node and its whole subtree. The root cannot be removed.
    pub fn remove(&mut self, id: NodeId) {
        if id == self.root || !self.contains(id) {
            return;
        }
        if let Some(parent) = self.get(id).and_then(|n| n.parent) {
            self.get_mut(parent).unwrap().children.retain(|&c| c != id);
        }
        let mut stack = vec![id];
        while let Some(id) = stack.pop() {
            let slot = &mut self.slots[id.index as usize];
            if let Some(node) = slot.node.take() {
                stack.extend(node.children);
                slot.generation = slot.generation.wrapping_add(1);
                self.free.push(id.index);
            }
        }
    }

    /// Moves `id` (with its subtree) under `new_parent`. Refuses to create cycles.
    pub fn reparent(&mut self, id: NodeId, new_parent: NodeId) -> bool {
        if id == self.root || !self.contains(id) || !self.contains(new_parent) {
            return false;
        }
        let mut p = Some(new_parent);
        while let Some(n) = p {
            if n == id {
                return false;
            }
            p = self.get(n).unwrap().parent;
        }
        let old = self.get(id).unwrap().parent.unwrap();
        self.get_mut(old).unwrap().children.retain(|&c| c != id);
        self.get_mut(new_parent).unwrap().children.push(id);
        self.get_mut(id).unwrap().parent = Some(new_parent);
        true
    }

    /// Node-local to design-space transform, including ancestors, camera and the nearest
    /// layer's parallax. `None` for removed nodes and nodes inside a render target (they
    /// are not on screen directly).
    pub fn to_design(&self, id: NodeId) -> Option<Affine2> {
        let mut model = Affine2::IDENTITY;
        let mut parallax = None;
        let mut cur = Some(id);
        while let Some(n) = cur {
            let node = self.get(n)?;
            match &node.kind {
                NodeKind::Target(_) if n != id => return None,
                NodeKind::Layer(l) if parallax.is_none() => parallax = Some(l.parallax),
                _ => {}
            }
            model = node.transform.to_affine() * model;
            cur = node.parent;
        }
        Some(self.camera.view(parallax.unwrap_or(1.0)) * model)
    }

    /// Whether design-space point `p` is inside the node's drawn area (sprite rectangle or
    /// any of its quads). Groups and layers have no area of their own.
    pub fn hit(&self, id: NodeId, p: Vec2) -> bool {
        let Some(m) = self.to_design(id) else { return false };
        let local = m.inverse().transform_point2(p);
        match &self[id].kind {
            NodeKind::Sprite(s) => Rect { min: Vec2::ZERO, max: s.size }.contains(local),
            NodeKind::Quads(q) => q.iter().any(|q| q.rect.contains(local)),
            NodeKind::Mesh(m) => m.hit(local).is_some(),
            _ => false,
        }
    }

    /// Number of live nodes, including the root.
    pub fn len(&self) -> usize {
        self.slots.len() - self.free.len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() <= 1
    }
}

impl std::ops::Index<NodeId> for Scene {
    type Output = Node;
    fn index(&self, id: NodeId) -> &Node {
        self.get(id).expect("stale NodeId")
    }
}

impl std::ops::IndexMut<NodeId> for Scene {
    fn index_mut(&mut self, id: NodeId) -> &mut Node {
        self.get_mut(id).expect("stale NodeId")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scene() -> Scene {
        Scene::new(Vec2::new(1920.0, 1080.0))
    }

    #[test]
    fn add_remove_reuses_slots_with_new_generation() {
        let mut s = scene();
        let a = s.add(s.root(), Node::group());
        let b = s.add(a, Node::group());
        assert_eq!(s.len(), 3);
        s.remove(a);
        assert!(!s.contains(a) && !s.contains(b), "subtree removed");
        assert_eq!(s.len(), 1);
        let c = s.add(s.root(), Node::group());
        assert!(s.contains(c));
        assert!(!s.contains(a), "old id stays stale after slot reuse");
        assert!(s[s.root()].children().contains(&c));
    }

    #[test]
    fn reparent_refuses_cycles() {
        let mut s = scene();
        let a = s.add(s.root(), Node::group());
        let b = s.add(a, Node::group());
        assert!(!s.reparent(a, b));
        assert!(s.reparent(b, s.root()));
        assert_eq!(s[b].parent(), Some(s.root()));
        assert!(s[a].children().is_empty());
    }

    #[test]
    fn hit_tests_through_transforms_parallax_and_targets() {
        let mut s = scene();
        let layer = s.add(s.root(), Node::layer(0.5));
        let g = s.add(layer, Node::group().at(100.0, 100.0));
        let sprite = s.add(g, Node::sprite(TextureId(1), Vec2::new(50.0, 20.0)).at(10.0, 0.0));
        assert!(s.hit(sprite, Vec2::new(115.0, 105.0)));
        assert!(!s.hit(sprite, Vec2::new(105.0, 105.0)));
        assert!(!s.hit(g, Vec2::new(115.0, 105.0)), "groups have no area");
        s.camera.position.x += 100.0; // the layer moves by half: 50 px left
        assert!(s.hit(sprite, Vec2::new(65.0, 105.0)));
        let t = s.add(s.root(), Node::target(TextureId(2), UVec2::new(64, 64)));
        let inside = s.add(t, Node::sprite(TextureId(1), Vec2::splat(10.0)));
        assert!(s.to_design(inside).is_none());
        assert!(!s.hit(inside, Vec2::new(5.0, 5.0)));
    }

    #[test]
    fn transform_rotates_around_origin() {
        let t = Transform {
            position: Vec2::new(100.0, 50.0),
            rotation: std::f32::consts::FRAC_PI_2,
            scale: Vec2::ONE,
            origin: Vec2::new(10.0, 0.0),
        };
        // The pivot lands on `position`.
        let p = t.to_affine().transform_point2(Vec2::new(10.0, 0.0));
        assert!((p - Vec2::new(100.0, 50.0)).length() < 1e-4);
        let q = t.to_affine().transform_point2(Vec2::new(20.0, 0.0));
        assert!((q - Vec2::new(100.0, 60.0)).length() < 1e-4, "{q}");
    }
}
