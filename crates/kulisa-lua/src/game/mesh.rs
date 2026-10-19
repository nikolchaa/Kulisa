//! Mesh nodes: `kulisa.mesh{}` and the methods that edit vertices every frame.

use kulisa_core::{Color, Mesh, NodeId, NodeKind, Rect, Vec2};
use mlua::{Buffer, Lua, Table, Value as LuaValue};

use super::State;
use super::api::{color, image_arg, number};

fn mesh_mut(st: &mut State, id: NodeId) -> mlua::Result<&mut Mesh> {
    match st.scene.get_mut(id).map(|n| &mut n.kind) {
        Some(NodeKind::Mesh(m)) => Ok(m),
        Some(_) => Err(mlua::Error::runtime("this method needs a mesh node (kulisa.mesh)")),
        None => Err(mlua::Error::runtime("this node was removed")),
    }
}

/// Numbers from a flat table (`{ x1, y1, x2, y2, ... }`) or a Luau buffer of 32-bit floats.
fn floats(v: &LuaValue, what: &str) -> mlua::Result<Vec<f32>> {
    match v {
        LuaValue::Table(t) => t.sequence_values::<LuaValue>().map(|x| number(&x?, what)).collect(),
        LuaValue::Buffer(b) => {
            Ok(b.to_vec().as_chunks::<4>().0.iter().map(|c| f32::from_le_bytes(*c)).collect())
        }
        _ => {
            Err(mlua::Error::runtime(format!("`{what}` must be a flat table of numbers or a buffer of f32")))
        }
    }
}

/// 1-based vertex numbers from a flat table, or a buffer of u32.
fn indices(v: &LuaValue) -> mlua::Result<Vec<u32>> {
    let bad = || mlua::Error::runtime("`indices` are vertex numbers starting at 1");
    match v {
        LuaValue::Table(t) => t
            .sequence_values::<LuaValue>()
            .map(|x| {
                let n = number(&x?, "indices")?;
                if n < 1.0 || n.fract() != 0.0 { Err(bad()) } else { Ok(n as u32 - 1) }
            })
            .collect(),
        LuaValue::Buffer(b) => b
            .to_vec()
            .as_chunks::<4>()
            .0
            .iter()
            .map(|c| u32::from_le_bytes(*c).checked_sub(1).ok_or_else(bad))
            .collect(),
        _ => Err(mlua::Error::runtime("`indices` must be a flat table or a buffer of u32")),
    }
}

fn pairs(v: &LuaValue, what: &str) -> mlua::Result<Vec<Vec2>> {
    let f = floats(v, what)?;
    if !f.len().is_multiple_of(2) {
        return Err(mlua::Error::runtime(format!("`{what}` holds x, y pairs; got {} numbers", f.len())));
    }
    Ok(f.as_chunks::<2>().0.iter().map(|c| Vec2::new(c[0], c[1])).collect())
}

fn colors(v: &LuaValue) -> mlua::Result<Vec<Color>> {
    let f = floats(v, "colors")?;
    if !f.len().is_multiple_of(4) {
        return Err(mlua::Error::runtime(format!(
            "`colors` hold r, g, b, a groups; got {} numbers",
            f.len()
        )));
    }
    Ok(f.as_chunks::<4>().0.iter().map(|c| Color::srgb(c[0], c[1], c[2], c[3])).collect())
}

fn expect(what: &str, got: usize, want: usize) -> mlua::Result<()> {
    if got == want {
        return Ok(());
    }
    Err(mlua::Error::runtime(format!(
        "`{what}` has {got} vertices but the mesh has {want}; use set_geometry to change the vertex count"
    )))
}

/// Builds the mesh a `kulisa.mesh{}` call describes: a grid (`cols`, `rows`, `width`,
/// `height`) or raw arrays (`positions`, `uvs`, `colors`, `indices`).
pub(super) fn make(st: &mut State, opts: &Table) -> mlua::Result<Mesh> {
    let image = match opts.get::<LuaValue>("image")? {
        LuaValue::Nil => st.white,
        v => image_arg(st, &v)?,
    };
    let raw = opts.get::<LuaValue>("positions")?;
    let mesh = if raw.is_nil() {
        let cols = opts.get::<Option<u32>>("cols")?.unwrap_or(1);
        let rows = opts.get::<Option<u32>>("rows")?.unwrap_or(1);
        if cols == 0 || rows == 0 || (cols as usize + 1) * (rows as usize + 1) > kulisa_core::MAX_VERTICES {
            return Err(mlua::Error::runtime(
                "`cols` and `rows` must be at least 1 and make at most 65536 vertices",
            ));
        }
        let natural = if image.id == st.white.id { Vec2::splat(100.0) } else { image.size.as_vec2() };
        let size = Vec2::new(
            opts.get::<Option<f32>>("width")?.unwrap_or(natural.x),
            opts.get::<Option<f32>>("height")?.unwrap_or(natural.y),
        );
        Mesh::grid(image.id, cols, rows, size, Rect::UNIT)
    } else {
        let positions = pairs(&raw, "positions")?;
        let uvs = match opts.get::<LuaValue>("uvs")? {
            LuaValue::Nil => Vec::new(),
            v => pairs(&v, "uvs")?,
        };
        let cols = match opts.get::<LuaValue>("colors")? {
            LuaValue::Nil => Vec::new(),
            v => colors(&v)?,
        };
        let idx = match opts.get::<LuaValue>("indices")? {
            LuaValue::Nil => return Err(mlua::Error::runtime("a mesh with `positions` needs `indices`")),
            v => indices(&v)?,
        };
        Mesh::from_parts(image.id, positions, uvs, cols, idx).map_err(mlua::Error::runtime)?
    };
    Ok(mesh)
}

/// Option names `kulisa.mesh` handles itself (the rest are node properties).
pub(super) const OPTIONS: &[&str] =
    &["image", "cols", "rows", "width", "height", "positions", "uvs", "colors", "indices"];

/// Mesh-only read properties: `Some(None)` is a nil (`cols` of a free-form mesh).
pub(super) fn get(st: &State, id: NodeId, key: &str) -> Option<Option<f64>> {
    let Some(NodeKind::Mesh(m)) = st.scene.get(id).map(|n| &n.kind) else { return None };
    Some(match key {
        "vertex_count" => Some(m.vertex_count() as f64),
        "triangle_count" => Some((m.indices.len() / 3) as f64),
        "cols" => m.grid.map(|g| g.0 as f64),
        "rows" => m.grid.map(|g| g.1 as f64),
        _ => return None,
    })
}

/// `mesh:set_positions(data)` and friends; `name` picks which array.
pub(super) fn set_array(st: &mut State, id: NodeId, name: &str, v: &LuaValue) -> mlua::Result<()> {
    let m = mesh_mut(st, id)?;
    let n = m.vertex_count();
    match name {
        "positions" => {
            let p = pairs(v, name)?;
            expect(name, p.len(), n)?;
            m.positions = p;
        }
        "uvs" => {
            let p = pairs(v, name)?;
            expect(name, p.len(), n)?;
            m.uvs = p;
        }
        "colors" => {
            let c = colors(v)?;
            expect(name, c.len(), n)?;
            m.colors = c;
        }
        _ => {
            let idx = indices(v)?;
            if !idx.len().is_multiple_of(3) || idx.iter().any(|&i| i as usize >= n) {
                return Err(mlua::Error::runtime("`indices` must come in threes and name existing vertices"));
            }
            m.indices = idx;
            m.grid = None;
        }
    }
    Ok(())
}

pub(super) fn set_geometry(st: &mut State, id: NodeId, o: &Table) -> mlua::Result<()> {
    let keep = mesh_mut(st, id)?.texture;
    *mesh_mut(st, id)? = make_from_arrays(o, keep)?;
    Ok(())
}

fn make_from_arrays(o: &Table, texture: kulisa_core::TextureId) -> mlua::Result<Mesh> {
    let positions = pairs(&o.get::<LuaValue>("positions")?, "positions")?;
    let uvs = match o.get::<LuaValue>("uvs")? {
        LuaValue::Nil => Vec::new(),
        v => pairs(&v, "uvs")?,
    };
    let cols = match o.get::<LuaValue>("colors")? {
        LuaValue::Nil => Vec::new(),
        v => colors(&v)?,
    };
    let idx = indices(&o.get::<LuaValue>("indices")?)?;
    Mesh::from_parts(texture, positions, uvs, cols, idx).map_err(mlua::Error::runtime)
}

/// `mesh:positions()` / `mesh:uvs()` / `mesh:colors()`: a flat table, or written into `buf`.
pub(super) fn get_array(
    lua: &Lua,
    st: &mut State,
    id: NodeId,
    name: &str,
    buf: Option<Buffer>,
) -> mlua::Result<LuaValue> {
    let m = mesh_mut(st, id)?;
    let data: Vec<f32> = match name {
        "positions" => m.positions.iter().flat_map(|p| [p.x, p.y]).collect(),
        "uvs" => m.uvs.iter().flat_map(|p| [p.x, p.y]).collect(),
        _ => m.colors.iter().flat_map(|c| linear_to_srgb(*c)).collect(),
    };
    match buf {
        Some(b) => {
            if b.len() < data.len() * 4 {
                return Err(mlua::Error::runtime(format!(
                    "the buffer holds {} bytes but {} are needed",
                    b.len(),
                    data.len() * 4
                )));
            }
            let bytes: Vec<u8> = data.iter().flat_map(|f| f.to_le_bytes()).collect();
            b.write_bytes(0, &bytes);
            Ok(LuaValue::Buffer(b))
        }
        None => Ok(LuaValue::Table(lua.create_sequence_from(data)?)),
    }
}

fn linear_to_srgb(c: Color) -> [f32; 4] {
    let f = |v: f32| if v <= 0.003_130_8 { v * 12.92 } else { 1.055 * v.powf(1.0 / 2.4) - 0.055 };
    [f(c.r), f(c.g), f(c.b), c.a]
}

pub(super) fn vertex(st: &mut State, id: NodeId, i: usize) -> mlua::Result<(f32, f32, f32, f32)> {
    let m = mesh_mut(st, id)?;
    let k = check(m, i)?;
    Ok((m.positions[k].x, m.positions[k].y, m.uvs[k].x, m.uvs[k].y))
}

fn check(m: &Mesh, i: usize) -> mlua::Result<usize> {
    if i == 0 || i > m.vertex_count() {
        return Err(mlua::Error::runtime(format!("vertex {i} does not exist (1 to {})", m.vertex_count())));
    }
    Ok(i - 1)
}

/// `mesh:set_vertex(i, x, y, u?, v?)`: moves a vertex; UVs change only when given.
pub(super) fn set_vertex(
    st: &mut State,
    id: NodeId,
    i: usize,
    xy: (f32, f32),
    uv: Option<(f32, f32)>,
) -> mlua::Result<()> {
    let m = mesh_mut(st, id)?;
    let k = check(m, i)?;
    m.positions[k] = Vec2::new(xy.0, xy.1);
    if let Some((u, v)) = uv {
        m.uvs[k] = Vec2::new(u, v);
    }
    Ok(())
}

pub(super) fn set_color(st: &mut State, id: NodeId, i: usize, c: &LuaValue) -> mlua::Result<()> {
    let c = color(c, "color")?;
    let m = mesh_mut(st, id)?;
    let k = check(m, i)?;
    m.colors[k] = c;
    Ok(())
}

/// `mesh:vertex_at(col, row)` → the 1-based vertex number in a grid mesh.
pub(super) fn grid_vertex(st: &mut State, id: NodeId, col: u32, row: u32) -> mlua::Result<usize> {
    let m = mesh_mut(st, id)?;
    let (cols, rows) = m.grid.ok_or_else(|| mlua::Error::runtime("this mesh is not a grid"))?;
    if col > cols || row > rows {
        return Err(mlua::Error::runtime(format!("the grid has columns 0 to {cols} and rows 0 to {rows}")));
    }
    Ok((row * (cols + 1) + col + 1) as usize)
}
