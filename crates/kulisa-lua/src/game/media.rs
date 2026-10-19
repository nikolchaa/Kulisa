//! Screen captures: frozen frames for transitions, captured images (save thumbnails) read
//! back to the CPU, and the thumbnails of saved games.

use kulisa_core::{Ease, Material, Node, NodeId, NodeKind, Params, ShaderId, TextureId, UVec2, Vec2};
use kulisa_render::{Image, Readback};

use super::{ImageInfo, State};

/// A running screen transition: the frozen frame drawn over the live scene.
pub(crate) struct Transition {
    pub id: u64,
    /// A screen-fixed layer holding the frozen frame's sprite.
    pub layer: NodeId,
    pub sprite: NodeId,
    pub time: f32,
    pub elapsed: f32,
    pub ease: Ease,
}

/// What `kulisa.screen.transition` was asked for.
pub(crate) struct TransitionSpec {
    pub shader: ShaderId,
    pub time: f32,
    pub ease: Ease,
    pub mask: Option<TextureId>,
    /// `params[0]` is the progress; the rest are the caller's.
    pub params: Params,
}

/// A captured image on its way to the CPU.
#[derive(Default)]
pub(crate) struct Capture {
    pub readback: Option<Readback>,
    pub pixels: Option<Image>,
}

#[derive(Default)]
pub(crate) struct Media {
    /// Two frames for freezes, used in turn: a running transition reads one while the
    /// next freeze writes the other.
    pub freezes: [Option<ImageInfo>; 2],
    pub next_freeze: usize,
    /// The freeze the next transition starts from.
    pub frozen: Option<ImageInfo>,
    pub transition: Option<Transition>,
    pub captures: std::collections::HashMap<TextureId, Capture>,
    /// Saves written before their thumbnail's pixels arrived (web): rewritten once they do.
    pub pending_saves: Vec<(String, TextureId)>,
    /// Decoded save thumbnails per slot, with the save's timestamp.
    pub thumbs: std::collections::HashMap<String, (u64, ImageInfo)>,
}

/// The window as a rectangle in design units (a transition covers all of it).
fn window_rect(st: &State) -> (Vec2, Vec2) {
    let size = st.renderer.size();
    let inv = st.viewport.layout(size).to_pixels.inverse();
    let a = inv.transform_point2(Vec2::ZERO);
    let b = inv.transform_point2(size.as_vec2());
    (a, b - a)
}

/// Captures the scene as it is now into the next freeze frame. With `hide_transition`, a
/// running transition's overlay is left out (the frame after it, not during it).
pub(crate) fn freeze(st: &mut State, hide_transition: bool) -> Result<ImageInfo, String> {
    let size = st.renderer.size();
    let k = st.media.next_freeze;
    let info = match st.media.freezes[k] {
        Some(f) if f.size == size => f,
        old => {
            if let Some(old) = old {
                st.renderer.free_texture(old.id);
            }
            let f = ImageInfo { id: st.renderer.create_target(size.x, size.y), size };
            st.media.freezes[k] = Some(f);
            f
        }
    };
    let overlay = st.media.transition.as_ref().map(|t| t.layer).filter(|_| hide_transition);
    if let Some(l) = overlay.and_then(|l| st.scene.get_mut(l)) {
        l.visible = false;
    }
    let result = st.renderer.capture(&st.scene, &st.viewport, st.time as f32, info.id);
    if let Some(l) = overlay.and_then(|l| st.scene.get_mut(l)) {
        l.visible = true;
    }
    result?;
    st.media.next_freeze = 1 - k;
    st.media.frozen = Some(info);
    Ok(info)
}

/// Ends the running transition at once.
pub(crate) fn finish_transition(st: &mut State) {
    if let Some(t) = st.media.transition.take() {
        st.scene.remove(t.layer);
    }
}

/// Starts a transition from the last freeze (or from now, when nothing was frozen).
pub(crate) fn start_transition(st: &mut State, spec: TransitionSpec) -> Result<u64, String> {
    let frame = match st.media.frozen.take() {
        Some(f) => f,
        None => {
            freeze(st, false)?;
            st.media.frozen.take().unwrap()
        }
    };
    finish_transition(st);
    let id = st.next_id();
    if spec.time <= 0.0 {
        return Ok(id);
    }
    let mut layer = Node::layer(0.0);
    layer.z = i32::MAX;
    let layer = st.scene.add(st.scene.root(), layer);
    let (pos, size) = window_rect(st);
    let mut sprite = Node::sprite(frame.id, size).at(pos.x, pos.y);
    let mut params = spec.params;
    params[0] = 0.0;
    sprite.material = Some(Material { shader: spec.shader, params, mask: spec.mask });
    let sprite = st.scene.add(layer, sprite);
    st.media.transition =
        Some(Transition { id, layer, sprite, time: spec.time, elapsed: 0.0, ease: spec.ease });
    Ok(id)
}

/// Advances the transition, captured images and saves waiting for thumbnails.
pub(crate) fn step(st: &mut State, dt: f32) {
    if let Some(mut t) = st.media.transition.take() {
        t.elapsed += dt;
        let progress = t.ease.apply((t.elapsed / t.time).clamp(0.0, 1.0));
        let (pos, size) = window_rect(st);
        let ended = t.elapsed >= t.time || !st.scene.contains(t.sprite);
        if ended {
            st.scene.remove(t.layer);
        } else {
            let n = &mut st.scene[t.sprite];
            n.transform.position = pos;
            if let NodeKind::Sprite(s) = &mut n.kind {
                s.size = size;
            }
            if let Some(m) = &mut n.material {
                m.params[0] = progress;
            }
            st.media.transition = Some(t);
        }
    }
    let State { media, renderer, .. } = st;
    for c in media.captures.values_mut() {
        if let Some(rb) = &c.readback
            && let Some(result) = renderer.poll_readback(rb, false)
        {
            c.readback = None;
            match result {
                Ok(img) => c.pixels = Some(img),
                Err(e) => log::warn!("capture: {e}"),
            }
        }
    }
}

/// Renders the scene into a `width`x`height` image (reusing `reuse` when it is a capture
/// of that size) and starts reading it back.
pub(crate) fn capture(
    st: &mut State,
    width: u32,
    height: u32,
    reuse: Option<ImageInfo>,
) -> Result<ImageInfo, String> {
    let size = UVec2::new(width.clamp(1, 4096), height.clamp(1, 4096));
    let info = match reuse {
        Some(r) if r.size == size && st.media.captures.contains_key(&r.id) => r,
        _ => ImageInfo { id: st.renderer.create_target(size.x, size.y), size },
    };
    st.renderer.capture(&st.scene, &st.viewport, st.time as f32, info.id)?;
    let readback = st.renderer.read_texture(info.id);
    st.media.captures.insert(info.id, Capture { readback, pixels: None });
    Ok(info)
}

/// The pixels of a captured image. `wait` blocks for them where the platform can.
pub(crate) fn capture_pixels(st: &mut State, id: TextureId, wait: bool) -> Result<Option<Image>, String> {
    let State { media, renderer, .. } = st;
    let c = media.captures.get_mut(&id).ok_or("`thumbnail` must be an image from kulisa.screen.capture")?;
    if let Some(rb) = &c.readback
        && let Some(result) = renderer.poll_readback(rb, wait)
    {
        c.readback = None;
        c.pixels = Some(result?);
    }
    Ok(c.pixels.clone())
}

/// A save's thumbnail as an image, decoded once per save.
pub(crate) fn thumbnail(st: &mut State, slot: &str, timestamp: u64, png: &[u8]) -> Option<ImageInfo> {
    if let Some((t, info)) = st.media.thumbs.get(slot)
        && *t == timestamp
    {
        return Some(*info);
    }
    let img = Image::decode(png).map_err(|e| log::warn!("thumbnail of `{slot}`: {e}")).ok()?;
    let info =
        ImageInfo { id: st.renderer.create_texture_unpacked(&img), size: UVec2::new(img.width, img.height) };
    if let Some((_, old)) = st.media.thumbs.insert(slot.to_string(), (timestamp, info)) {
        st.renderer.free_texture(old.id);
    }
    Some(info)
}

/// Frees a slot's decoded thumbnail (after the save was deleted).
pub(crate) fn forget_thumbnail(st: &mut State, slot: &str) {
    if let Some((_, old)) = st.media.thumbs.remove(slot) {
        st.renderer.free_texture(old.id);
    }
}
