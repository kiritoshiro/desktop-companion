//! A uniform bucket grid over creature positions.
//!
//! The Python worlds scan every spider for every fly (fear) and every
//! creature for every guard (intruders). With hundreds of spiders that is the
//! hot loop, so the Rust worlds bucket positions once per update and visit
//! only nearby candidates.
//!
//! Exactness: the Python loops accumulate in list order (a fly sums its
//! flee vector spider by spider; a guard keeps the first nearest intruder).
//! [`SpatialGrid::query`] returns candidate indices **sorted ascending**, so
//! running the original per-item logic over them visits the survivors in the
//! same order as the full scan and produces bit-identical results. Items
//! outside the query square are exactly those the Python loop would have
//! rejected on distance.
//!
//! No allocation after warm-up: `rebuild` reuses its buffers.

/// Below this many items a linear scan is cheaper than bucketing.
pub const GRID_MIN_ITEMS: usize = 24;

#[derive(Debug, Clone, Default)]
pub struct SpatialGrid {
    cell: f64,
    /// `(cell key, item index)`, sorted by key.
    entries: Vec<(u64, u32)>,
}

#[inline]
fn key(cx: i32, cy: i32) -> u64 {
    ((cx as u32 as u64) << 32) | (cy as u32 as u64)
}

impl SpatialGrid {
    pub fn new() -> Self {
        Self::default()
    }

    /// Re-bucket `points` (item `i` at `points(i)`) with the given cell size.
    pub fn rebuild<I: IntoIterator<Item = (f64, f64)>>(&mut self, cell: f64, points: I) {
        self.cell = cell.max(1.0);
        self.entries.clear();
        for (i, (x, y)) in points.into_iter().enumerate() {
            if !(x.is_finite() && y.is_finite()) {
                // Never lose an item: park non-finite positions in one cell
                // that every query also visits.
                self.entries.push((u64::MAX, i as u32));
                continue;
            }
            let cx = (x / self.cell).floor() as i32;
            let cy = (y / self.cell).floor() as i32;
            self.entries.push((key(cx, cy), i as u32));
        }
        self.entries.sort_unstable();
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Every item within the square of half-size `r` around `(x, y)` (and
    /// possibly a few more), written to `out` sorted ascending.
    pub fn query(&self, x: f64, y: f64, r: f64, out: &mut Vec<u32>) {
        out.clear();
        if self.entries.is_empty() {
            return;
        }
        if !(x.is_finite() && y.is_finite() && r.is_finite()) || r / self.cell > 64.0 {
            out.extend(self.entries.iter().map(|e| e.1));
            out.sort_unstable();
            return;
        }
        let x0 = ((x - r) / self.cell).floor() as i32;
        let x1 = ((x + r) / self.cell).floor() as i32;
        let y0 = ((y - r) / self.cell).floor() as i32;
        let y1 = ((y + r) / self.cell).floor() as i32;
        for cx in x0..=x1 {
            for cy in y0..=y1 {
                self.push_cell(key(cx, cy), out);
            }
        }
        self.push_cell(u64::MAX, out);
        out.sort_unstable();
        out.dedup();
    }

    fn push_cell(&self, k: u64, out: &mut Vec<u32>) {
        let start = self.entries.partition_point(|e| e.0 < k);
        for e in &self.entries[start..] {
            if e.0 != k {
                break;
            }
            out.push(e.1);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_query_finds_everything_near_and_sorts_it() {
        let pts: Vec<(f64, f64)> = (0..200).map(|i| ((i * 37 % 1000) as f64, (i * 91 % 700) as f64)).collect();
        let mut grid = SpatialGrid::new();
        grid.rebuild(50.0, pts.iter().copied());
        let mut out = Vec::new();
        grid.query(500.0, 350.0, 120.0, &mut out);
        let expected: Vec<u32> = (0..200u32)
            .filter(|&i| {
                let (x, y) = pts[i as usize];
                (x - 500.0).abs() <= 120.0 && (y - 350.0).abs() <= 120.0
            })
            .collect();
        for e in &expected {
            assert!(out.contains(e));
        }
        assert!(out.windows(2).all(|w| w[0] < w[1]));
    }
}
