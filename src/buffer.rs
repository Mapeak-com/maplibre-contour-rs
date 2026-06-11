//! Build the buffered elevation grid a contour tile is traced from.
//!
//! This is the seam-prevention *and* overzoom step. It samples the DEM at
//! `source_zoom` (which may be a coarser ancestor of the requested tile) over
//! the requested tile's area plus a `buffer`-pixel margin, so:
//!
//! - adjacent tiles trace their shared edge from the same field → no seams;
//! - a tile above the DEM's max zoom is upsampled from its ancestor → overzoom.
//!
//! Both fall out of one bilinear sampler over the global DEM pixel field,
//! mirroring maplibre-contour's `fetchDem` + `combineNeighbors` + subsample.

use std::collections::HashMap;
use std::sync::Arc;

use crate::dem::DemGrid;
use crate::error::{Error, Result};
use crate::tile::TileCoord;

/// Sample a `(tile_size + 2*buffer)` square elevation grid for `coord`,
/// reading DEM tiles at `source_zoom` through `fetch`.
///
/// `fetch` returns the decoded grid for a source-zoom tile (or `None` if it
/// has no data); it is called once per needed tile. The center (ancestor) tile
/// is required.
pub fn sample_buffered(
    coord: TileCoord,
    source_zoom: u8,
    tile_size: u32,
    buffer: u32,
    mut fetch: impl FnMut(TileCoord) -> Result<Option<Arc<DemGrid>>>,
) -> Result<DemGrid> {
    let t = tile_size as i64;
    let b = buffer as i64;
    let out_size = tile_size + 2 * buffer;

    let dz = coord.z.saturating_sub(source_zoom);
    let scale = (1u64 << dz) as f64; // requested px per source px
    let n_src = 1i64 << source_zoom; // source tiles per axis
    let world = n_src * t; // source px per axis

    // The requested tile's top-left in requested-zoom pixels, minus the margin.
    let ox = coord.x as i64 * t - b;
    let oy = coord.y as i64 * t - b;

    // Source-pixel span we touch (±1 for bilinear interpolation).
    let to_src = |g: i64| (g as f64 / scale).floor() as i64;
    let sx_lo = to_src(ox) - 1;
    let sx_hi = to_src(ox + out_size as i64 - 1) + 1;
    let sy_lo = to_src(oy) - 1;
    let sy_hi = to_src(oy + out_size as i64 - 1) + 1;

    // Prefetch every source tile covering that span (x wraps, y is clamped).
    let mut tiles: HashMap<TileCoord, Arc<DemGrid>> = HashMap::new();
    for ty in sy_lo.div_euclid(t)..=sy_hi.div_euclid(t) {
        if ty < 0 || ty >= n_src {
            continue;
        }
        for tx in sx_lo.div_euclid(t)..=sx_hi.div_euclid(t) {
            let c = TileCoord::new(source_zoom, tx.rem_euclid(n_src) as u32, ty as u32);
            if let std::collections::hash_map::Entry::Vacant(e) = tiles.entry(c) {
                if let Some(grid) = fetch(c)? {
                    e.insert(grid);
                }
            }
        }
    }

    let center = TileCoord::new(source_zoom, coord.x >> dz, coord.y >> dz);
    let center = tiles
        .get(&center)
        .cloned()
        .ok_or_else(|| Error::Decode("center DEM tile is required".into()))?;

    // Elevation at an integer source pixel, wrapping x and clamping y/missing
    // tiles to the nearest available sample.
    let value_at = |px: i64, py: i64| -> f32 {
        let py = py.clamp(0, world - 1);
        let tx = px.div_euclid(t).rem_euclid(n_src);
        let ty = py.div_euclid(t);
        let c = TileCoord::new(source_zoom, tx as u32, ty as u32);
        let lx = px.rem_euclid(t);
        let ly = py - ty * t;
        match tiles.get(&c) {
            Some(g) => g.get((lx as u32).min(g.width - 1), (ly as u32).min(g.height - 1)),
            None => {
                let cx = (px - (center.width as i64) * (coord.x as i64 >> dz))
                    .clamp(0, center.width as i64 - 1) as u32;
                let cy = (py - (center.height as i64) * (coord.y as i64 >> dz))
                    .clamp(0, center.height as i64 - 1) as u32;
                center.get(cx, cy)
            }
        }
    };

    let mut out = DemGrid::filled(out_size, out_size, f32::NAN);
    for j in 0..out_size as i64 {
        let sy = (oy + j) as f64 / scale;
        let y0 = sy.floor();
        let fy = (sy - y0) as f32;
        let y0 = y0 as i64;
        for i in 0..out_size as i64 {
            let sx = (ox + i) as f64 / scale;
            let x0 = sx.floor();
            let fx = (sx - x0) as f32;
            let x0 = x0 as i64;

            let v00 = value_at(x0, y0);
            let v10 = value_at(x0 + 1, y0);
            let v01 = value_at(x0, y0 + 1);
            let v11 = value_at(x0 + 1, y0 + 1);
            let top = v00 + (v10 - v00) * fx;
            let bot = v01 + (v11 - v01) * fx;
            out.set(i as u32, j as u32, top + (bot - top) * fy);
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn solid(size: u32, fill: f32) -> Arc<DemGrid> {
        Arc::new(DemGrid::filled(size, size, fill))
    }

    /// Center plus 8 neighbors, identity zoom: margins come from neighbors.
    #[test]
    fn margins_sample_neighbors() {
        let t = 4;
        let grids = |c: TileCoord| -> Result<Option<Arc<DemGrid>>> {
            // Encode tile identity as elevation so we can see where samples come from.
            Ok(Some(solid(t, (c.x * 10 + c.y) as f32)))
        };
        // Tile (5,2,2): west neighbor x=1, value 1*10+2=12.
        let g = sample_buffered(TileCoord::new(5, 2, 2), 5, t, 1, grids).unwrap();
        assert_eq!(g.width, t + 2);
        assert_eq!(g.get(0, 3), 12.0); // left margin -> west tile (x=1,y=2)
        assert_eq!(g.get(3, 3), 22.0); // interior -> center (x=2,y=2)
        assert_eq!(g.get(5, 3), 32.0); // right margin -> east tile (x=3,y=2)
    }

    /// Overzoom: a child tile above the DEM zoom is sampled from its ancestor.
    #[test]
    fn overzoom_samples_ancestor() {
        let t = 8;
        // Ancestor at z2 with a horizontal ramp = column index.
        let mut anc = DemGrid::filled(t, t, 0.0);
        for y in 0..t {
            for x in 0..t {
                anc.set(x, y, x as f32);
            }
        }
        let anc = Arc::new(anc);
        let fetch = move |c: TileCoord| -> Result<Option<Arc<DemGrid>>> {
            Ok((c.z == 2 && c.x == 0 && c.y == 0).then(|| anc.clone()))
        };

        // Request z4 tile (0,0): source_zoom 2, scale 4 -> samples ancestor
        // columns [0, 2) upsampled across the tile. Values stay in [0, ~2).
        let g = sample_buffered(TileCoord::new(4, 0, 0), 2, t, 0, fetch).unwrap();
        let (min, max) = g.extent().unwrap();
        assert!(min >= 0.0 && max < 2.0, "got [{min}, {max}]");
        // Left edge ~0, right edge approaches 2 (one ancestor column per 4 px).
        assert!(g.get(0, 0) < 0.1);
        assert!(g.get(t - 1, 0) > 1.0);
    }

    #[test]
    fn missing_center_errors() {
        let none = |_c: TileCoord| -> Result<Option<Arc<DemGrid>>> { Ok(None) };
        assert!(sample_buffered(TileCoord::new(5, 2, 2), 5, 4, 1, none).is_err());
    }
}
