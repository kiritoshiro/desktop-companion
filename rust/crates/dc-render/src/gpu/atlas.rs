//! Shelf rectangle packer for the glyph and image atlases.

pub(crate) struct Shelves {
    pub width: u32,
    pub height: u32,
    shelves: Vec<(u32, u32, u32)>, // (y, height, next x)
    next_y: u32,
}

impl Shelves {
    pub fn new(width: u32, height: u32) -> Self {
        Self { width, height, shelves: Vec::new(), next_y: 0 }
    }

    pub fn reset(&mut self) {
        self.shelves.clear();
        self.next_y = 0;
    }

    /// Top-left of a free `w × h` area, or `None` when full.
    pub fn alloc(&mut self, w: u32, h: u32) -> Option<(u32, u32)> {
        if w > self.width || h > self.height {
            return None;
        }
        let mut best: Option<usize> = None;
        for (i, s) in self.shelves.iter().enumerate() {
            if s.1 >= h && s.2 + w <= self.width && s.1 <= h + h / 2 + 2 && best.is_none_or(|b| self.shelves[b].1 > s.1)
            {
                best = Some(i);
            }
        }
        if let Some(i) = best {
            let s = &mut self.shelves[i];
            let x = s.2;
            s.2 += w;
            return Some((x, s.0));
        }
        if self.next_y + h > self.height {
            return None;
        }
        let y = self.next_y;
        self.next_y += h;
        self.shelves.push((y, h, w));
        Some((0, y))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn packs_without_overlap() {
        let mut s = Shelves::new(64, 64);
        let mut rects = Vec::new();
        while let Some((x, y)) = s.alloc(10, 7) {
            rects.push((x, y));
        }
        assert_eq!(rects.len(), 6 * 9);
        for (i, a) in rects.iter().enumerate() {
            for b in &rects[i + 1..] {
                let overlap = a.0 < b.0 + 10 && b.0 < a.0 + 10 && a.1 < b.1 + 7 && b.1 < a.1 + 7;
                assert!(!overlap);
            }
        }
        s.reset();
        assert_eq!(s.alloc(64, 64), Some((0, 0)));
    }
}
