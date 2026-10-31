//! `kulisa.video`: playing videos. A video's picture is an image, which any sprite can show.

use kulisa_core::UVec2;
use kulisa_render::Image;
use kulisa_video::{Options, Video};
use mlua::{Lua, MetaMethod, Table, UserData, UserDataMethods, Value as LuaValue};

use super::api::{ImageRef, number, rt, with};
use super::{ImageInfo, State, report};

pub(crate) struct Playing {
    id: u64,
    video: Video,
    image: ImageInfo,
    /// Paused because the app went to the background.
    suspended: bool,
    reported: bool,
}

#[derive(Clone, Copy)]
struct VideoRef(u64);

fn playing<'a>(st: &'a mut State, id: u64) -> Option<&'a mut Playing> {
    st.videos.iter_mut().find(|p| p.id == id)
}

/// Advances every video and shows the pictures that are due.
pub(crate) fn step(shared: &super::Shared, dt: f32) {
    let mut failed = Vec::new();
    {
        let mut guard = shared.borrow_mut();
        let st = &mut *guard;
        for p in &mut st.videos {
            #[cfg(not(target_arch = "wasm32"))]
            if let Some((rgba, info)) = p.video.poll(f64::from(dt), st.audio.as_mut()) {
                st.renderer.update_texture(p.image.id, info.width, info.height, rgba);
                p.image.size = UVec2::new(info.width, info.height);
            }
            #[cfg(target_arch = "wasm32")]
            {
                let _ = dt;
                if p.video.poll(st.audio.as_mut())
                    && let Some(info) = p.video.info()
                {
                    st.renderer.update_texture_from_video(
                        p.image.id,
                        p.video.element(),
                        info.width,
                        info.height,
                    );
                    p.image.size = UVec2::new(info.width, info.height);
                }
            }
            if let Some(e) = p.video.error()
                && !p.reported
            {
                p.reported = true;
                failed.push(format!("video: {e}"));
            }
        }
    }
    for e in failed {
        report(shared, &e);
    }
}

/// Pauses videos while the app is away, and carries on when it is back.
pub(crate) fn set_suspended(st: &mut State, suspended: bool) {
    for p in &mut st.videos {
        if suspended && !p.video.paused() {
            p.suspended = true;
            p.video.set_paused(true);
        } else if !suspended && p.suspended {
            p.suspended = false;
            p.video.set_paused(false);
        }
    }
}

impl UserData for VideoRef {
    fn add_methods<M: UserDataMethods<Self>>(m: &mut M) {
        m.add_meta_method(MetaMethod::Index, |lua, this, key: String| {
            with(lua, |st| {
                let Some(p) = playing(st, this.0) else {
                    return match key.as_str() {
                        "done" => Ok(LuaValue::Boolean(true)),
                        "ready" | "paused" => Ok(LuaValue::Boolean(false)),
                        "time" | "duration" | "width" | "height" => Ok(LuaValue::Number(0.0)),
                        "image" | "error" => Ok(LuaValue::Nil),
                        _ => Err(mlua::Error::runtime(format!("videos have no property `{key}`"))),
                    };
                };
                let info = p.video.info();
                let number = |v: f64| Ok(LuaValue::Number(v));
                match key.as_str() {
                    "ready" => Ok(LuaValue::Boolean(info.is_some())),
                    "done" => Ok(LuaValue::Boolean(p.video.done())),
                    "paused" => Ok(LuaValue::Boolean(p.video.paused())),
                    "time" => number(p.video.time()),
                    "duration" => number(info.map_or(0.0, |i| i.duration)),
                    "width" => number(info.map_or(0.0, |i| f64::from(i.width))),
                    "height" => number(info.map_or(0.0, |i| f64::from(i.height))),
                    "image" => {
                        let image = p.image;
                        lua.create_userdata(ImageRef(image)).map(LuaValue::UserData)
                    }
                    "error" => match p.video.error() {
                        Some(e) => lua.create_string(e).map(LuaValue::String),
                        None => Ok(LuaValue::Nil),
                    },
                    _ => Err(mlua::Error::runtime(format!("videos have no property `{key}`"))),
                }
            })
        });
        m.add_method("pause", |lua, this, ()| {
            with(lua, |st| {
                if let Some(p) = playing(st, this.0) {
                    p.suspended = false;
                    p.video.set_paused(true);
                }
                Ok(())
            })
        });
        m.add_method("resume", |lua, this, ()| {
            with(lua, |st| {
                if let Some(p) = playing(st, this.0) {
                    p.video.set_paused(false);
                }
                Ok(())
            })
        });
        m.add_method("seek", |lua, this, time: f64| {
            with(lua, |st| {
                let State { videos, audio, .. } = st;
                if let Some(p) = videos.iter_mut().find(|p| p.id == this.0) {
                    p.video.seek(time, audio.as_mut());
                }
                Ok(())
            })
        });
        m.add_method("set_volume", |lua, this, volume: f32| {
            with(lua, |st| {
                if let Some(p) = playing(st, this.0) {
                    p.video.set_volume(volume.clamp(0.0, 1.0));
                }
                Ok(())
            })
        });
        // Ends it for good and frees its image, so sprites showing it should go first.
        m.add_method("stop", |lua, this, ()| {
            with(lua, |st| {
                if let Some(i) = st.videos.iter().position(|p| p.id == this.0) {
                    let p = st.videos.remove(i);
                    st.renderer.free_texture(p.image.id);
                }
                Ok(())
            })
        });
    }
}

fn options(opts: Option<&Table>) -> mlua::Result<Options> {
    let mut o = Options::default();
    let Some(t) = opts else { return Ok(o) };
    if let Some(v) = t.get::<Option<bool>>("loop")? {
        o.looping = v;
    }
    if let Some(v) = t.get::<Option<bool>>("paused")? {
        o.paused = v;
    }
    match t.get::<LuaValue>("volume")? {
        LuaValue::Nil => {}
        v => o.volume = number(&v, "volume")?.clamp(0.0, 1.0),
    }
    match t.get::<LuaValue>("channel")? {
        LuaValue::Nil => {}
        LuaValue::Boolean(false) => o.channel = None,
        LuaValue::String(s) => o.channel = Some(s.to_string_lossy()),
        _ => return Err(mlua::Error::runtime("`channel` must be a channel name, or false for no sound")),
    }
    Ok(o)
}

pub(super) fn install(lua: &Lua, k: &Table) -> mlua::Result<()> {
    let v = lua.create_table()?;
    v.set(
        "play",
        lua.create_function(|lua, (path, opts): (String, Option<Table>)| {
            let options = options(opts.as_ref())?;
            with(lua, |st| {
                let location = rt(st.assets.video(&path))?;
                if let Some(channel) = &options.channel
                    && let Some(audio) = &st.audio
                    && !audio.channels().any(|c| c == channel)
                {
                    return Err(mlua::Error::runtime(format!("no audio channel named `{channel}`")));
                }
                let id = st.next_id();
                // Black until its first picture arrives, at the video's real size then.
                let black = Image::solid(2, 2, [0, 0, 0, 255]);
                let image =
                    ImageInfo { id: st.renderer.create_texture_unpacked(&black), size: UVec2::splat(2) };
                let video = Video::open(location, options);
                st.videos.push(Playing { id, video, image, suspended: false, reported: false });
                Ok(VideoRef(id))
            })
        })?,
    )?;
    k.set("video", v)
}
