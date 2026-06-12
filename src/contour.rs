//! One-pass marching-squares contour tracing, ported from maplibre-contour's
//! `isolines.ts` (itself adapted from d3-contour).

use std::collections::{BTreeMap, HashMap, HashSet};

use crate::config::ThresholdRule;
use crate::dem::DemGrid;

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

/// Trace contours over a materialized [`DemGrid`] (origin at its top-left,
/// edges clamped). The thin entry point used by the golden tests; the pipeline
/// uses [`contour_tile`].
pub fn generate_isolines(
    grid: &DemGrid,
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

/// Trace the contours for `rule` over a buffered tile grid (as produced by
/// [`crate::buffer::sample_buffered`]), scaling sampled elevations by
/// `multiplier`. Coordinates come out in `0..extent` tile space (the center
/// tile occupies the full range; the buffer margin lands just outside it).
pub fn contour_tile(
    buffered: &DemGrid,
    tile_size: u32,
    buffer: u32,
    rule: &ThresholdRule,
    multiplier: f32,
    extent: u32,
) -> Vec<Contour> {
    let interval = rule.interval();
    if interval <= 0.0 {
        return Vec::new();
    }
    let t = tile_size as i64;
    let b = buffer as i64;
    let m = multiplier as f64;
    let (bw, bh) = (buffered.width as i64, buffered.height as i64);

    let iso = isolines(
        t,
        t,
        |x, y| {
            let bx = (x + b).clamp(0, bw - 1) as u32;
            let by = (y + b).clamp(0, bh - 1) as u32;
            buffered.get(bx, by) as f64 * m
        },
        interval as f64,
        extent as f64,
        b,
    );

    iso.into_iter()
        .map(|(ele_key, lines)| {
            let elevation = ele_key as f32;
            Contour {
                elevation,
                level: rule.level_for(elevation),
                lines,
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn contour_tile_tags_major_levels_in_tile_space() {
        // 4x4 grid, ramp 5/15/25/35 by row; tile_size 4, no buffer. Crossings
        // land at 10/20/30, so 20 is the only major (multiple of 20).
        let mut g = DemGrid::filled(4, 4, 0.0);
        for y in 0..4 {
            for x in 0..4 {
                g.set(x, y, y as f32 * 10.0 + 5.0);
            }
        }
        let rule = ThresholdRule {
            zoom: 0,
            intervals: vec![10.0, 20.0],
        };
        let contours = contour_tile(&g, 4, 0, &rule, 1.0, 4096);

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
