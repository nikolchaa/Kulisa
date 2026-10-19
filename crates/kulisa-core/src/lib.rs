//! Engine core without GPU, audio or scripting: scene graph, camera, viewport and the
//! extraction of a scene into a renderer-agnostic [`Frame`].

mod camera;
mod color;
mod frame;
mod scene;
mod viewport;

pub use camera::Camera;
pub use color::Color;
pub use frame::{DrawItem, Frame, Pass, PassTarget, TexRef, Vertex, extract};
pub use glam::{Affine2, UVec2, Vec2};
pub use scene::{
    BlendMode, Effect, Layer, Material, Node, NodeId, NodeKind, Params, Rect, Scene, ShaderId, Sprite,
    Target, TextureId, Transform,
};
pub use viewport::{ScaleMode, ViewLayout, Viewport};
