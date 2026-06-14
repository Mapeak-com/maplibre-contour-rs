//! One-pass marching-squares contour tracing, ported from maplibre-contour's
//! `isolines.ts` (itself adapted from d3-contour).

use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::Arc;

use crate::config::ThresholdRule;
use crate::decode_image::DemTile;
use crate::error::Result;
use crate::height_tile::HeightTile;
use crate::tile::TileCoord;

/// Overzoomed tiles are upsampled until the contour grid reaches at least this
/// width, then averaged — maplibre-contour's `subsampleBelow` (default 100).
const SUBSAMPLE_BELOW: i64 = 100;

// Edge-midpoint encodings used by the marching-squares CASES table: a point
// `[a, b]` sits on the left (a==0), right (a==2), top (b==0), or bottom edge.
const LEFT: [i64; 2] = [0, 1];
const RIGHT: [i64; 2] = [2, 1];
const TOP: [i64; 2] = [1, 0];
const BOTTOM: [i64; 2] = [1, 2];

// Indexed by `(tl<<3)|(tr<<2)|(br<<1)|bl` (corner > threshold), each entry lists
// the `(start_edge, end_edge)` segments through that cell.
#[rustfmt::skip]
const CASES: [&[([i64; 2], [i64; 2])]; 16] = [
    &[],
    &[(BOTTOM, LEFT)],
    &[(RIGHT, BOTTOM)],
    &[(RIGHT, LEFT)],
    &[(TOP, RIGHT)],
    &[(BOTTOM, LEFT), (TOP, RIGHT)],
    &[(TOP, BOTTOM)],
    &[(TOP, LEFT)],
    &[(LEFT, TOP)],
    &[(BOTTOM, TOP)],
    &[(LEFT, TOP), (RIGHT, BOTTOM)],
    &[(RIGHT, TOP)],
    &[(LEFT, RIGHT)],
    &[(BOTTOM, RIGHT)],
    &[(LEFT, BOTTOM)],
    &[],
];

/// A partial contour line being stitched together; vertices run from its
/// `start` edge to its `end` edge. Coordinates are rounded on insertion to
/// match maplibre-contour's integer tile coordinates.
struct Fragment {
    start: i64,
    end: i64,
    level: i64,
    points: Vec<f64>,
}

impl Fragment {
    fn append(&mut self, x: f64, y: f64) {
        self.points.push(x.round());
        self.points.push(y.round());
    }
    fn prepend(&mut self, x: f64, y: f64) {
        self.points.insert(0, y.round());
        self.points.insert(0, x.round());
    }
    fn is_empty(&self) -> bool {
        self.points.len() < 2
    }
}

/// Interpolation fraction of `threshold` between corner values `a` and `c`.
#[inline]
fn ratio(a: f64, b: f64, c: f64) -> f64 {
    (b - a) / (c - a)
}

/// Trace every contour at multiples of `interval` over a `width`×`height` grid
/// sampled by `get`, in a single pass. Reads `buffer` pixels past each edge
/// (`get` must supply them). Returns lines keyed by elevation (`round`ed), each
/// a flat `[x1, y1, x2, y2, …]` list in `extent / (width - 1)` units per pixel.
fn isolines(
    width: i64,
    height: i64,
    get: impl Fn(i64, i64) -> f64,
    interval: f64,
    extent: f64,
    buffer: i64,
) -> BTreeMap<i64, Vec<Vec<f64>>> {
    let mut segments: BTreeMap<i64, Vec<Vec<f64>>> = BTreeMap::new();
    if !interval.is_finite() || interval <= 0.0 || width < 2 {
        return segments;
    }
    let mult = extent / (width as f64 - 1.0);
    let edge_index =
        |c: i64, r: i64, p: [i64; 2]| -> i64 { (c * 2 + p[0]) + (r * 2 + p[1]) * (width + 1) * 2 };

    // Fragments live in an arena keyed by index; the per-level maps point start
    // and end edges at the fragment currently terminating there, so adjacent
    // segments stitch in O(1). Merged fragments are abandoned in `frags`.
    let mut frags: Vec<Fragment> = Vec::new();
    let mut by_start: HashMap<i64, HashMap<i64, usize>> = HashMap::new();
    let mut by_end: HashMap<i64, HashMap<i64, usize>> = HashMap::new();

    for r in (1 - buffer)..(height + buffer) {
        let mut trd = get(0, r - 1);
        let mut brd = get(0, r);
        let mut min_r = trd.min(brd);
        let mut max_r = trd.max(brd);
        for c in (1 - buffer)..(width + buffer) {
            let tld = trd;
            let bld = brd;
            trd = get(c, r - 1);
            brd = get(c, r);
            let (min_l, max_l) = (min_r, max_r);
            min_r = trd.min(brd);
            max_r = trd.max(brd);
            if tld.is_nan() || trd.is_nan() || brd.is_nan() || bld.is_nan() {
                continue;
            }
            let cell_min = min_l.min(min_r);
            let cell_max = max_l.max(max_r);
            let m_start = (cell_min / interval).ceil() as i64;
            let m_end = (cell_max / interval).floor() as i64;
            for m in m_start..=m_end {
                let threshold = m as f64 * interval;
                let case = (usize::from(tld > threshold) << 3)
                    | (usize::from(trd > threshold) << 2)
                    | (usize::from(brd > threshold) << 1)
                    | usize::from(bld > threshold);
                if CASES[case].is_empty() {
                    continue;
                }
                let interp = |p: [i64; 2]| -> (f64, f64) {
                    if p[0] == 0 {
                        (
                            mult * (c - 1) as f64,
                            mult * (r as f64 - ratio(bld, threshold, tld)),
                        )
                    } else if p[0] == 2 {
                        (
                            mult * c as f64,
                            mult * (r as f64 - ratio(brd, threshold, trd)),
                        )
                    } else if p[1] == 0 {
                        (
                            mult * (c as f64 - ratio(trd, threshold, tld)),
                            mult * (r - 1) as f64,
                        )
                    } else {
                        (
                            mult * (c as f64 - ratio(brd, threshold, bld)),
                            mult * r as f64,
                        )
                    }
                };
                let ele = threshold.round() as i64;
                let bs = by_start.entry(m).or_default();
                let be = by_end.entry(m).or_default();
                for &(start_p, end_p) in CASES[case] {
                    let start_index = edge_index(c, r, start_p);
                    let end_index = edge_index(c, r, end_p);

                    if let Some(fi) = be.remove(&start_index) {
                        if let Some(gi) = bs.remove(&end_index) {
                            if fi == gi {
                                // closing a ring
                                let (x, y) = interp(end_p);
                                frags[fi].append(x, y);
                                if !frags[fi].is_empty() {
                                    segments
                                        .entry(ele)
                                        .or_default()
                                        .push(std::mem::take(&mut frags[fi].points));
                                }
                            } else {
                                // connect two fragments end-to-start
                                let g_points = std::mem::take(&mut frags[gi].points);
                                let g_end = frags[gi].end;
                                frags[fi].points.extend(g_points);
                                frags[fi].end = g_end;
                                be.insert(g_end, fi);
                            }
                        } else {
                            // extend the end of f
                            let (x, y) = interp(end_p);
                            frags[fi].append(x, y);
                            frags[fi].end = end_index;
                            be.insert(end_index, fi);
                        }
                    } else if let Some(fi) = bs.remove(&end_index) {
                        // extend the start of f
                        let (x, y) = interp(start_p);
                        frags[fi].prepend(x, y);
                        frags[fi].start = start_index;
                        bs.insert(start_index, fi);
                    } else {
                        // start a new fragment
                        let fi = frags.len();
                        let mut f = Fragment {
                            start: start_index,
                            end: end_index,
                            level: m,
                            points: Vec::new(),
                        };
                        let (sx, sy) = interp(start_p);
                        f.append(sx, sy);
                        let (ex, ey) = interp(end_p);
                        f.append(ex, ey);
                        frags.push(f);
                        bs.insert(start_index, fi);
                        be.insert(end_index, fi);
                    }
                }
            }
        }
    }

    // Fragments still open ran to the tile boundary; emit them as lines, in
    // creation order (matching maplibre-contour's insertion-ordered Map).
    let live: HashSet<usize> = by_start
        .values()
        .flat_map(|map| map.values().copied())
        .collect();
    for (fi, f) in frags.iter().enumerate() {
        if live.contains(&fi) && !f.is_empty() {
            let ele = (f.level as f64 * interval).round() as i64;
            segments.entry(ele).or_default().push(f.points.clone());
        }
    }

    segments
}

/// Trace contours over a materialized [`DemTile`] (origin at its top-left,
/// edges clamped). The thin entry point used by the golden tests; the pipeline
/// uses [`contour_tile`].
pub fn generate_isolines(
    grid: &DemTile,
    interval: f32,
    extent: f64,
    buffer: u32,
) -> BTreeMap<i64, Vec<Vec<f64>>> {
    let (w, h) = (grid.width as i64, grid.height as i64);
    isolines(
        w,
        h,
        |x, y| grid.get(x.clamp(0, w - 1) as u32, y.clamp(0, h - 1) as u32) as f64,
        interval as f64,
        extent,
        buffer as i64,
    )
}

/// A contour at one elevation: its major/minor `level` plus the geometry, each
/// line a flat `[x1, y1, x2, y2, …]` list in `0..extent` tile coordinates.
#[derive(Debug, Clone)]
pub struct Contour {
    pub elevation: f32,
    /// 0 = minor; higher = coarser/major (see [`ThresholdRule::level_for`]).
    pub level: u32,
    pub lines: Vec<Vec<f64>>,
}

/// Trace the contours for `rule` over the tile at `coord`, reading the DEM at
/// `source_zoom` (a coarser ancestor when overzooming) through `fetch`. Faithful
/// to maplibre-contour's `fetchContourTile`:
///
/// 1. crop the ancestor to this tile's area (`source_width >> dz` pixels);
/// 2. upsample (bilinear, pixel-center aligned) until the grid is at least
///    `SUBSAMPLE_BELOW` wide — so heavily-overzoomed tiles trace from a small,
///    smooth grid rather than the full source resolution;
/// 3. `averagePixelCentersToGrid` — each grid corner is the mean of its four
///    neighbouring pixel centres (a `W+1`-wide grid mapped with `extent/W` units
///    per pixel), which aligns contours with the DEM and lightly smooths them;
/// 4. scale by `multiplier` and trace.
///
/// Coordinates come out in `0..extent` tile space; the `buffer` margin lands
/// just outside. Returns an empty `Vec` when no DEM covers the tile.
#[allow(clippy::too_many_arguments)]
pub fn contour_tile(
    coord: TileCoord,
    source_zoom: u8,
    source_width: u32,
    rule: &ThresholdRule,
    multiplier: f32,
    extent: u32,
    buffer: u32,
    fetch: impl FnMut(TileCoord) -> Result<Option<Arc<DemTile>>>,
) -> Result<Vec<Contour>> {
    let interval = rule.interval();
    if interval <= 0.0 {
        return Ok(Vec::new());
    }

    let dz = u32::from(coord.z - source_zoom);
    let div = 1i64 << dz;
    let sw = source_width as i64;
    let crop_w = (sw >> dz).max(1); // ancestor pixels covering this tile

    // Upsample factor so the contour grid is >= SUBSAMPLE_BELOW (maplibre).
    let mut f = 1i64;
    let mut w_out = crop_w;
    while w_out < SUBSAMPLE_BELOW {
        f *= 2;
        w_out *= 2;
    }
    // `subsamplePixelCenters` sample offset (0 when no upsampling).
    let sub = 0.5 - 1.0 / (2.0 * f as f64);
    let m = multiplier as f64;
    let b = buffer as i64;

    // Global source-pixel origin of this tile's cropped ancestor region.
    let cx = coord.x as i64;
    let cy = coord.y as i64;
    let crop_ox = (cx >> dz) * sw + (cx & (div - 1)) * crop_w;
    let crop_oy = (cy >> dz) * sw + (cy & (div - 1)) * crop_w;
    // Upsampled grid pixel -> fractional global source pixel.
    let gpx = |ox: i64| crop_ox as f64 + ox as f64 / f as f64 - sub;
    let gpy = |oy: i64| crop_oy as f64 + oy as f64 / f as f64 - sub;

    // Source-pixel box we touch: the tracer reads grid corners `[-b, w_out+b]`,
    // averaging reads upsampled px `[corner-1, corner]`, bilinear ±1 more.
    let (lo, hi) = (-b - 1, w_out + b);
    let field = HeightTile::fetch(
        source_zoom,
        source_width,
        gpx(lo).floor() as i64 - 1,
        gpx(hi).ceil() as i64 + 1,
        gpy(lo).floor() as i64 - 1,
        gpy(hi).ceil() as i64 + 1,
        fetch,
    )?;
    let center = TileCoord::new(source_zoom, coord.x >> dz, coord.y >> dz);
    if !field.has(center) {
        return Ok(Vec::new());
    }

    // averagePixelCentersToGrid: corner (gx,gy) = mean of the 4 upsampled pixel
    // centres around it, scaled by the elevation multiplier.
    let iso = isolines(
        w_out + 1,
        w_out + 1,
        |gx, gy| {
            let (mut sum, mut count) = (0.0, 0u32);
            for (dx, dy) in [(-1, -1), (0, -1), (-1, 0), (0, 0)] {
                let v = field.sample(gpx(gx + dx), gpy(gy + dy));
                if v.is_finite() {
                    sum += f64::from(v);
                    count += 1;
                }
            }
            if count == 0 {
                f64::NAN
            } else {
                (sum / f64::from(count)) * m
            }
        },
        interval as f64,
        extent as f64,
        b,
    );

    Ok(iso
        .into_iter()
        .map(|(ele_key, lines)| {
            let elevation = ele_key as f32;
            Contour {
                elevation,
                level: rule.level_for(elevation),
                lines,
            }
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ramp_tile(size: u32) -> Arc<DemTile> {
        // North-south ramp spanning 5..35 m, so contours land at 10/20/30.
        let mut g = DemTile::filled(size, size, 0.0);
        for y in 0..size {
            let h = 5.0 + (y as f32 / (size - 1) as f32) * 30.0;
            for x in 0..size {
                g.set(x, y, h);
            }
        }
        Arc::new(g)
    }

    #[test]
    fn contour_tile_tags_major_levels_in_tile_space() {
        // A single z0 tile (no overzoom); a north-south ramp crosses 10/20/30,
        // so 20 (a multiple of the major interval) is the only major line.
        let tile = ramp_tile(120); // >= SUBSAMPLE_BELOW so no upsampling
        let rule = ThresholdRule {
            zoom: 0,
            intervals: vec![10.0, 20.0],
        };
        let contours = contour_tile(TileCoord::new(0, 0, 0), 0, 120, &rule, 1.0, 4096, 0, |_| {
            Ok(Some(tile.clone()))
        })
        .unwrap();

        let major: Vec<f32> = contours
            .iter()
            .filter(|c| c.level == 1)
            .map(|c| c.elevation)
            .collect();
        assert_eq!(major, vec![20.0]);

        // Every coordinate sits in tile space.
        for v in contours.iter().flat_map(|c| c.lines.iter().flatten()) {
            assert!((0.0..=4096.0).contains(v), "coord {v} out of tile range");
        }
    }
}
