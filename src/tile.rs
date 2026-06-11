//! Slippy-map (XYZ) tile coordinates and the 3x3 neighborhood helper.
//! Longitude (x) is cyclic; latitude (y) is clamped at the poles.

/// A standard slippy-map (XYZ) tile coordinate.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TileCoord {
    pub z: u8,
    pub x: u32,
    pub y: u32,
}

impl TileCoord {
    pub fn new(z: u8, x: u32, y: u32) -> Self {
        Self { z, x, y }
    }

    /// Number of tiles per axis at this zoom (`2^z`).
    #[inline]
    pub fn dim(&self) -> u64 {
        1u64 << self.z
    }

    /// Offset this tile by `(dx, dy)`.
    ///
    /// X wraps around the antimeridian (longitude is cyclic). Y does **not**
    /// wrap: returns `None` past the poles, where no tile exists.
    pub fn offset(&self, dx: i64, dy: i64) -> Option<TileCoord> {
        let n = self.dim() as i64;
        let ny = self.y as i64 + dy;
        if ny < 0 || ny >= n {
            return None;
        }
        let nx = (self.x as i64 + dx).rem_euclid(n);
        Some(TileCoord {
            z: self.z,
            x: nx as u32,
            y: ny as u32,
        })
    }

    /// The parent tile one zoom level up, if any.
    pub fn parent(&self) -> Option<TileCoord> {
        self.z.checked_sub(1).map(|z| TileCoord {
            z,
            x: self.x / 2,
            y: self.y / 2,
        })
    }
}

/// A 3x3 block of tile coordinates centered on `center`.
///
/// Indexed `[row][col]` where row 0 is north (y-1) and col 0 is west (x-1),
/// so `tiles[1][1]` is always the center. Edge/pole tiles that don't exist
/// are `None`.
#[derive(Debug, Clone)]
pub struct Neighborhood {
    pub center: TileCoord,
    pub tiles: [[Option<TileCoord>; 3]; 3],
}

impl Neighborhood {
    pub fn around(center: TileCoord) -> Self {
        let mut tiles = [[None; 3]; 3];
        for (ri, dy) in (-1..=1).enumerate() {
            for (ci, dx) in (-1..=1).enumerate() {
                tiles[ri][ci] = center.offset(dx, dy);
            }
        }
        Neighborhood { center, tiles }
    }

    /// All distinct, existing coordinates in this neighborhood (including
    /// center). Handy for prefetching from the [`crate::source::TileSource`].
    pub fn coords(&self) -> Vec<TileCoord> {
        let mut out = Vec::with_capacity(9);
        for row in &self.tiles {
            for c in row.iter().flatten() {
                out.push(*c);
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn offset_wraps_x_clamps_y() {
        let c = TileCoord::new(2, 0, 0); // 4x4 grid
        assert_eq!(c.offset(-1, 0), Some(TileCoord::new(2, 3, 0))); // x wraps
        assert_eq!(c.offset(0, -1), None); // y past the north pole
        assert_eq!(c.offset(1, 1), Some(TileCoord::new(2, 1, 1)));
    }

    #[test]
    fn parent_halves_coords() {
        assert_eq!(
            TileCoord::new(5, 10, 11).parent(),
            Some(TileCoord::new(4, 5, 5))
        );
        assert_eq!(TileCoord::new(0, 0, 0).parent(), None);
    }

    #[test]
    fn wraps_in_x_not_y() {
        // Top-left tile of the world at z=2 (4x4 grid).
        let n = Neighborhood::around(TileCoord::new(2, 0, 0));
        // West neighbor wraps to x=3.
        assert_eq!(n.tiles[1][0], Some(TileCoord::new(2, 3, 0)));
        // North neighbor is off the top -> None.
        assert_eq!(n.tiles[0][1], None);
        // Center is itself.
        assert_eq!(n.tiles[1][1], Some(TileCoord::new(2, 0, 0)));
    }
}
