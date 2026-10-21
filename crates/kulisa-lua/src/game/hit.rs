//! Pixel-perfect hit testing: the CPU copy of image alpha that `contains` and `pick` read.

use std::cell::RefCell;
use std::collections::HashMap;

use kulisa_core::{AlphaSource, NodeId, TextureId, Vec2};
use kulisa_platform::Assets;
use kulisa_render::Image;
use mlua::Value as LuaValue;

use super::{ImageInfo, State};

/// The alpha channel of one image.
pub(crate) struct AlphaMask {
    width: u32,
    height: u32,
    alpha: Vec<u8>,
}

/// Alpha masks by texture, decoded on first use from the asset the texture was loaded from.
/// Images that did not come from an asset (captures, render targets, glyphs) have none and
/// count as opaque. Nothing is decoded for games that never ask for alpha tests.
#[derive(Default)]
pub(crate) struct Masks(RefCell<HashMap<TextureId, Option<AlphaMask>>>);

impl Masks {
    /// Drops the cached alpha of a texture whose image changed (live reload).
    pub fn forget(&self, id: TextureId) {
        self.0.borrow_mut().remove(&id);
    }
}

/// Reads alpha for the scene's hit tests.
pub(crate) struct Alpha<'a> {
    assets: &'a Assets,
    images: &'a HashMap<String, ImageInfo>,
    masks: &'a Masks,
}

impl State {
    pub(crate) fn alpha(&self) -> Alpha<'_> {
        Alpha { assets: &self.assets, images: &self.images, masks: &self.alpha_masks }
    }
}

impl Alpha<'_> {
    fn load(&self, texture: TextureId) -> Option<AlphaMask> {
        let (path, _) = self.images.iter().find(|(_, i)| i.id == texture)?;
        let image = Image::decode(&self.assets.read(path).ok()?).ok()?;
        let alpha = image.pixels.as_chunks::<4>().0.iter().map(|p| p[3]).collect();
        Some(AlphaMask { width: image.width, height: image.height, alpha })
    }
}

impl AlphaSource for Alpha<'_> {
    fn alpha(&self, texture: TextureId, uv: Vec2) -> Option<f32> {
        let mut masks = self.masks.0.borrow_mut();
        let mask = masks.entry(texture).or_insert_with(|| self.load(texture)).as_ref()?;
        let x = ((uv.x * mask.width as f32).floor().max(0.0) as u32).min(mask.width - 1);
        let y = ((uv.y * mask.height as f32).floor().max(0.0) as u32).min(mask.height - 1);
        Some(mask.alpha[(y * mask.width + x) as usize] as f32 / 255.0)
    }
}

/// `true` means the default threshold (half opaque); a number is the threshold itself.
pub(crate) fn threshold(v: &LuaValue) -> mlua::Result<Option<f32>> {
    match v {
        LuaValue::Nil | LuaValue::Boolean(false) => Ok(None),
        LuaValue::Boolean(true) => Ok(Some(0.5)),
        LuaValue::Integer(_) | LuaValue::Number(_) => {
            let t = super::api::number(v, "alpha")?;
            if !(0.0..=1.0).contains(&t) {
                return Err(mlua::Error::runtime("`alpha` is an opacity threshold between 0 and 1"));
            }
            Ok(Some(t))
        }
        _ => Err(mlua::Error::runtime("`alpha` must be true or a number between 0 and 1")),
    }
}

pub(crate) fn contains(st: &State, id: NodeId, p: Vec2, alpha: Option<f32>) -> bool {
    let a = st.alpha();
    st.scene.hit_alpha(id, p, alpha.map(|t| (t, &a as &dyn AlphaSource)))
}

/// The nodes under `p`, topmost first.
pub(crate) fn pick_all(st: &State, p: Vec2, root: Option<NodeId>, alpha: Option<f32>) -> Vec<NodeId> {
    let a = st.alpha();
    let mut out = Vec::new();
    st.scene.pick_all(p, root, alpha.map(|t| (t, &a as &dyn AlphaSource)), &mut out);
    out
}
