//! A window of decoded DEM tiles, sampled by global source-pixel coordinate.
//!
//! [`HeightTile`] prefetches the source-zoom tiles covering a region and samples
//! the elevation field with x-wrap, y-clamp, NaN for invalid/missing samples,
//! and NaN-aware bilinear interpolation — the equivalent of maplibre-contour's
//! `HeightTile` + `combineNeighbors`. `isolines::contour_tile` drives it to build
//! the (possibly overzoomed, subsampled, pixel-corner-averaged) contour grid.

use std::collections::HashMap;
use std::sync::Arc;

use crate::decode_image::DemTile;
use crate::error::Result;
use crate::tile::TileCoord;

// maplibre-contour's valid elevation range; values outside read as NaN so they
// don't drag contours toward nodata fill values.
const MIN_VALID_M: f32 = -12000.0;
const MAX_VALID_M: f32 = 9000.0;

/// Decoded DEM tiles at one zoom, addressed by global source pixel.
pub struct HeightTile {
    tiles: HashMap<TileCoord, Arc<DemTile>>,
    source_zoom: u8,
    tile_px: i64,
    n_src: i64,
    world: i64,
}

impl HeightTile {
    /// Prefetch the source tiles covering global-source-pixel box
    /// `[x0, x1] x [y0, y1]` (x wraps the antimeridian, y is clamped at poles).
    /// `fetch` is called once per needed tile.
    pub fn fetch(
        source_zoom: u8,
        tile_px: u32,
        x0: i64,
        x1: i64,
        y0: i64,
        y1: i64,
        mut fetch: impl FnMut(TileCoord) -> Result<Option<Arc<DemTile>>>,
    ) -> Result<Self> {
        let t = tile_px as i64;
        let n_src = 1i64 << source_zoom;
        let world = n_src * t;
        let mut tiles: HashMap<TileCoord, Arc<DemTile>> = HashMap::new();
        for ty in y0.div_euclid(t)..=y1.div_euclid(t) {
            if ty < 0 || ty >= n_src {
                continue;
            }
            for tx in x0.div_euclid(t)..=x1.div_euclid(t) {
                let c = TileCoord::new(source_zoom, tx.rem_euclid(n_src) as u32, ty as u32);
                if let std::collections::hash_map::Entry::Vacant(e) = tiles.entry(c) {
                    if let Some(g) = fetch(c)? {
                        e.insert(g);
                    }
                }
            }
        }
        Ok(Self {
            tiles,
            source_zoom,
            tile_px: t,
            n_src,
            world,
        })
    }

    /// Whether `coord`'s tile was found (the center is required to render).
    pub fn has(&self, coord: TileCoord) -> bool {
        self.tiles.contains_key(&coord)
    }

    /// NaN-aware elevation at an integer global source pixel.
    fn value(&self, px: i64, py: i64) -> f32 {
        if py < 0 || py >= self.world {
            return f32::NAN;
        }
        let t = self.tile_px;
        let tx = px.div_euclid(t).rem_euclid(self.n_src);
        let ty = py.div_euclid(t);
        let c = TileCoord::new(self.source_zoom, tx as u32, ty as u32);
        match self.tiles.get(&c) {
            None => f32::NAN,
            Some(g) => {
                let lx = px.rem_euclid(t).min(g.width as i64 - 1) as u32;
                let ly = (py - ty * t).min(g.height as i64 - 1) as u32;
                let v = g.get(lx, ly);
                // NaN is excluded too: `contains` is false for NaN, so `!` is true.
                if !(MIN_VALID_M..=MAX_VALID_M).contains(&v) {
                    f32::NAN
                } else {
                    v
                }
            }
        }
    }

    /// NaN-aware bilinear sample at a fractional global source pixel
    /// (maplibre-contour's lerp: a NaN endpoint falls back to the other).
    pub fn sample(&self, gx: f64, gy: f64) -> f32 {
        let lerp = |a: f32, b: f32, f: f32| -> f32 {
            if a.is_nan() {
                b
            } else if b.is_nan() {
                a
            } else {
                a + (b - a) * f
            }
        };
        let x0 = gx.floor();
        let y0 = gy.floor();
        let fx = (gx - x0) as f32;
        let fy = (gy - y0) as f32;
        let (x0, y0) = (x0 as i64, y0 as i64);
        let top = lerp(self.value(x0, y0), self.value(x0 + 1, y0), fx);
        let bot = lerp(self.value(x0, y0 + 1), self.value(x0 + 1, y0 + 1), fx);
        lerp(top, bot, fy)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn solid(size: u32, fill: f32) -> Arc<DemTile> {
        Arc::new(DemTile::filled(size, size, fill))
    }

    #[test]
    fn wraps_x_clamps_y_nans_missing() {
        // z2 world: 4 tiles/axis, 4px each -> 16px world.
        let field = HeightTile::fetch(2, 4, -1, 16, 0, 7, |c| {
            // Provide only tiles in row 0; encode tile x as the fill value.
            Ok((c.y == 0).then(|| solid(4, (c.x * 10) as f32)))
        })
        .unwrap();
        assert_eq!(field.sample(5.0, 1.0), 10.0); // px 5 -> tile x=1 -> 10
        assert_eq!(field.sample(0.0, 1.0), 0.0); // tile x=0
        assert!(field.sample(5.0, 6.0).is_nan()); // row 1 tile missing -> NaN
    }

    #[test]
    fn invalid_values_read_as_nan() {
        let field = HeightTile::fetch(0, 2, 0, 1, 0, 1, |_| Ok(Some(solid(2, -32768.0)))).unwrap();
        assert!(field.sample(0.0, 0.0).is_nan()); // below MIN_VALID_M
    }
}
