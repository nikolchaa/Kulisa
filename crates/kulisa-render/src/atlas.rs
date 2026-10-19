//! Shelf packing for texture atlas pages.

struct Shelf {
    y: u32,
    height: u32,
    x: u32,
}

/// Packs rectangles into a square page row by row ("shelves").
pub(crate) struct ShelfPacker {
    size: u32,
    shelves: Vec<Shelf>,
    next_y: u32,
}

impl ShelfPacker {
    pub fn new(size: u32) -> Self {
        Self { size, shelves: Vec::new(), next_y: 0 }
    }

    /// Reserves a `w` x `h` rectangle and returns its top-left corner.
    pub fn alloc(&mut self, w: u32, h: u32) -> Option<(u32, u32)> {
        if w > self.size || h > self.size {
            return None;
        }
        // Best fit: the shortest existing shelf that is tall enough and has room.
        let best = self
            .shelves
            .iter_mut()
            .filter(|s| s.height >= h && s.height <= h + h / 2 + 4 && self.size - s.x >= w)
            .min_by_key(|s| s.height);
        if let Some(s) = best {
            let pos = (s.x, s.y);
            s.x += w;
            return Some(pos);
        }
        if self.next_y + h > self.size {
            return None;
        }
        let y = self.next_y;
        self.next_y += h;
        self.shelves.push(Shelf { y, height: h, x: w });
        Some((0, y))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn packs_without_overlap_until_full() {
        let mut p = ShelfPacker::new(64);
        let mut rects = Vec::new();
        while let Some((x, y)) = p.alloc(10, 12) {
            rects.push((x, y, 10, 12));
        }
        assert_eq!(rects.len(), 6 * 5, "6 per row, 5 rows of 12 in 64");
        for (i, a) in rects.iter().enumerate() {
            assert!(a.0 + a.2 <= 64 && a.1 + a.3 <= 64);
            for b in &rects[i + 1..] {
                let overlap = a.0 < b.0 + b.2 && b.0 < a.0 + a.2 && a.1 < b.1 + b.3 && b.1 < a.1 + a.3;
                assert!(!overlap, "{a:?} overlaps {b:?}");
            }
        }
        assert_eq!(p.alloc(65, 1), None);
    }

    #[test]
    fn reuses_shelves_of_similar_height() {
        let mut p = ShelfPacker::new(100);
        assert_eq!(p.alloc(10, 20), Some((0, 0)));
        assert_eq!(p.alloc(10, 18), Some((10, 0)), "fits the 20px shelf");
        assert_eq!(p.alloc(10, 5), Some((0, 20)), "too small for that shelf; new one");
    }
}
