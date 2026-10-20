//! Engine core without GPU, audio or scripting: scene graph, camera, viewport, the
//! extraction of a scene into a renderer-agnostic [`Frame`], plain-data [`Value`]s and the
//! versioned save format.

mod camera;
mod color;
mod frame;
mod mesh;
mod particles;
mod save;
mod scene;
mod tween;
mod value;
mod viewport;

pub use camera::Camera;
pub use color::Color;
pub use frame::{DrawItem, Frame, ParticleDraw, Pass, PassTarget, TexRef, Vertex, extract, f16_bits};
pub use glam::{Affine2, UVec2, Vec2, Vec4};
pub use mesh::{MAX_VERTICES, Mesh};
pub use particles::{
    Emitter, Look as ParticleLook, Particle, ParticleBufferId, ParticleSystem, ParticleUniform, Shape, Span,
};
pub use save::{FORMAT_VERSION, Migrations, SaveFile, SaveHeader};
pub use scene::{
    AlphaSource, BlendMode, Effect, EffectInput, Layer, Material, Node, NodeId, NodeKind, Params, Quad, Rect,
    Scene, ShaderId, Slice, Sprite, Target, TextureId, Transform,
};
pub use tween::{Ease, Prop, Tween};
pub use value::Value;
pub use viewport::{ScaleMode, ViewLayout, Viewport, parse_aspect};
