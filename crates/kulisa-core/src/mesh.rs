//! Triangle meshes: per-vertex positions, UVs and colors that scripts can edit every frame.

use glam::{Vec2, Vec4};

use crate::{Color, Rect, TextureId};

/// A textured triangle mesh in local units. Deformable grids are meshes built by
/// [`Mesh::grid`]; scripts may also supply their own vertices and indices.
#[derive(Clone, Debug, PartialEq)]
pub struct Mesh {
    pub texture: TextureId,
    pub positions: Vec<Vec2>,
    /// Texture coordinates, 0..1 over the whole image.
    pub uvs: Vec<Vec2>,
    /// Multiplied with the node's inherited tint and opacity.
    pub colors: Vec<Color>,
    /// Per-vertex numbers handed to shaders as `in.data` (normals, heights, flags...), not color
    /// managed and not touched by tint or opacity.
    pub data: Vec<Vec4>,
    /// Triangle list into the vertex arrays.
    pub indices: Vec<u32>,
    /// Columns and rows of cells when this was built as a regular grid.
    pub grid: Option<(u32, u32)>,
}

/// The most vertices one mesh may have (a 255x255 grid): keeps scripts from allocating
/// absurd buffers by mistake.
pub const MAX_VERTICES: usize = 65_536;

impl Mesh {
    /// A regular grid of `cols` x `rows` cells covering `size`, with UVs spread over `uv`.
    /// Vertices run row by row (`row * (cols + 1) + col`), two triangles per cell.
    pub fn grid(texture: TextureId, cols: u32, rows: u32, size: Vec2, uv: Rect) -> Mesh {
        let (cols, rows) = (cols.max(1), rows.max(1));
        let n = ((cols + 1) * (rows + 1)) as usize;
        let mut positions = Vec::with_capacity(n);
        let mut uvs = Vec::with_capacity(n);
        for r in 0..=rows {
            for c in 0..=cols {
                let f = Vec2::new(c as f32 / cols as f32, r as f32 / rows as f32);
                positions.push(size * f);
                uvs.push(uv.min + uv.size() * f);
            }
        }
        let mut indices = Vec::with_capacity((cols * rows * 6) as usize);
        for r in 0..rows {
            for c in 0..cols {
                let i = r * (cols + 1) + c;
                let (a, b, d, e) = (i, i + 1, i + cols + 1, i + cols + 2);
                indices.extend([a, b, e, a, e, d]);
            }
        }
        Mesh {
            texture,
            positions,
            uvs,
            colors: vec![Color::WHITE; n],
            data: vec![Vec4::ZERO; n],
            indices,
            grid: Some((cols, rows)),
        }
    }

    /// A mesh from raw arrays. `uvs`, `colors` and `data` are filled in (zero, white, zero)
    /// when empty.
    pub fn from_parts(
        texture: TextureId,
        positions: Vec<Vec2>,
        mut uvs: Vec<Vec2>,
        mut colors: Vec<Color>,
        mut data: Vec<Vec4>,
        indices: Vec<u32>,
    ) -> Result<Mesh, String> {
        if uvs.is_empty() {
            uvs = vec![Vec2::ZERO; positions.len()];
        }
        if colors.is_empty() {
            colors = vec![Color::WHITE; positions.len()];
        }
        if data.is_empty() {
            data = vec![Vec4::ZERO; positions.len()];
        }
        let m = Mesh { texture, positions, uvs, colors, data, indices, grid: None };
        m.validate()?;
        Ok(m)
    }

    pub fn vertex_count(&self) -> usize {
        self.positions.len()
    }

    /// Checks that the arrays agree and the triangles point at real vertices.
    pub fn validate(&self) -> Result<(), String> {
        let n = self.positions.len();
        if n > MAX_VERTICES {
            return Err(format!("a mesh can have at most {MAX_VERTICES} vertices, not {n}"));
        }
        if self.uvs.len() != n || self.colors.len() != n || self.data.len() != n {
            return Err(format!(
                "mesh arrays differ in length: {n} positions, {} uvs, {} colors, {} data",
                self.uvs.len(),
                self.colors.len(),
                self.data.len()
            ));
        }
        if !self.indices.len().is_multiple_of(3) {
            return Err(format!("mesh indices come in threes, got {}", self.indices.len()));
        }
        if let Some(i) = self.indices.iter().find(|&&i| i as usize >= n) {
            return Err(format!("mesh index {} is out of range ({n} vertices)", i + 1));
        }
        Ok(())
    }

    /// Bounding box of the vertices in local units.
    pub fn bounds(&self) -> Rect {
        Rect::bounding(&self.positions)
    }

    /// The texture coordinates at local point `p` if it lies inside a triangle. Where
    /// triangles overlap, the one drawn last (on top) wins.
    pub fn hit(&self, p: Vec2) -> Option<Vec2> {
        self.indices.as_chunks::<3>().0.iter().rev().find_map(|t| {
            let (a, b, c) = (t[0] as usize, t[1] as usize, t[2] as usize);
            let w = barycentric(p, self.positions[a], self.positions[b], self.positions[c])?;
            Some(self.uvs[a] * w[0] + self.uvs[b] * w[1] + self.uvs[c] * w[2])
        })
    }
}

/// Barycentric weights of `p` in triangle `abc`, or `None` outside it (or if it is flat).
fn barycentric(p: Vec2, a: Vec2, b: Vec2, c: Vec2) -> Option<[f32; 3]> {
    let (v0, v1, v2) = (b - a, c - a, p - a);
    let det = v0.x * v1.y - v1.x * v0.y;
    if det.abs() < 1e-12 {
        return None;
    }
    let w1 = (v2.x * v1.y - v1.x * v2.y) / det;
    let w2 = (v0.x * v2.y - v2.x * v0.y) / det;
    let w0 = 1.0 - w1 - w2;
    // A hair of slack so points on a shared edge belong to one of the two triangles.
    const E: f32 = -1e-5;
    (w0 >= E && w1 >= E && w2 >= E).then_some([w0, w1, w2])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn grid() -> Mesh {
        Mesh::grid(TextureId(1), 2, 2, Vec2::new(100.0, 50.0), Rect::UNIT)
    }

    #[test]
    fn grids_cover_their_size_with_two_triangles_per_cell() {
        let m = grid();
        assert_eq!(m.vertex_count(), 9);
        assert_eq!(m.indices.len(), 2 * 2 * 6);
        assert_eq!(m.positions[4], Vec2::new(50.0, 25.0));
        assert_eq!(m.positions[8], Vec2::new(100.0, 50.0));
        assert_eq!(m.uvs[2], Vec2::new(1.0, 0.0));
        assert!(m.validate().is_ok());
        assert_eq!(m.bounds(), Rect::new(0.0, 0.0, 100.0, 50.0));
    }

    #[test]
    fn hit_interpolates_uvs_and_follows_deformation() {
        let mut m = grid();
        let uv = m.hit(Vec2::new(25.0, 12.5)).unwrap();
        assert!((uv - Vec2::new(0.25, 0.25)).length() < 1e-5, "{uv}");
        assert!(m.hit(Vec2::new(101.0, 10.0)).is_none());
        // Pull the whole mesh right: the old left edge is no longer covered.
        for p in &mut m.positions {
            p.x += 40.0;
        }
        assert!(m.hit(Vec2::new(10.0, 10.0)).is_none());
        let uv = m.hit(Vec2::new(90.0, 25.0)).unwrap();
        assert!((uv.x - 0.5).abs() < 1e-5, "{uv}");
    }

    #[test]
    fn from_parts_checks_its_arrays() {
        let pos = vec![Vec2::ZERO, Vec2::X, Vec2::Y];
        let ok = Mesh::from_parts(TextureId(1), pos.clone(), vec![], vec![], vec![], vec![0, 1, 2]).unwrap();
        assert_eq!((ok.colors.len(), ok.data.len()), (3, 3));
        let err =
            Mesh::from_parts(TextureId(1), pos.clone(), vec![], vec![], vec![], vec![0, 1, 3]).unwrap_err();
        assert!(err.contains("index 4 is out of range"), "{err}");
        let err =
            Mesh::from_parts(TextureId(1), pos.clone(), vec![], vec![], vec![], vec![0, 1]).unwrap_err();
        assert!(err.contains("threes"), "{err}");
        let err =
            Mesh::from_parts(TextureId(1), pos, vec![Vec2::ZERO], vec![], vec![], vec![0, 1, 2]).unwrap_err();
        assert!(err.contains("differ in length"), "{err}");
    }
}
