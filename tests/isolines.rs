//! Golden isoline tests ported from maplibre-contour's `isolines.test.ts`.
//!
//! Each case feeds a tiny grid to `generate_isolines`, converts the output back
//! to grid-unit coordinates, rounds to 3 decimals, and checks it against the
//! expected line set — across all four 90° rotations for the symmetric cases.

use std::collections::BTreeMap;

use maplibre_contour_rs::contour::generate_isolines;
use maplibre_contour_rs::dem::DemGrid;

const EXTENT: f64 = 4096.0;

type Lines = BTreeMap<i64, Vec<Vec<f64>>>;

fn round3(n: f64) -> f64 {
    (n * 1000.0 + 0.001).round() / 1000.0
}

fn grid(rows: &[&[f32]]) -> DemGrid {
    let h = rows.len() as u32;
    let w = rows[0].len() as u32;
    let mut data = Vec::with_capacity((w * h) as usize);
    for r in rows {
        data.extend_from_slice(r);
    }
    DemGrid::new(w, h, data)
}

fn rotate(p: (f64, f64), a: (f64, f64), angle: i32) -> (f64, f64) {
    let theta = f64::from(angle) * std::f64::consts::PI / 180.0;
    let (sin, cos) = (theta.sin(), theta.cos());
    let (rx, ry) = (p.0 - a.0, p.1 - a.1);
    (
        round3(a.0 + rx * cos - ry * sin),
        round3(a.1 + ry * cos + rx * sin),
    )
}

/// Sample `base` through a rotation about its center (mirrors the TS harness).
fn rotated_grid(base: &DemGrid, rotation: i32) -> DemGrid {
    let size = base.width as i32;
    let c = (f64::from(size) - 1.0) / 2.0;
    let mut data = vec![0f32; (size * size) as usize];
    for y in 0..size {
        for x in 0..size {
            let (nx, ny) = rotate((f64::from(x), f64::from(y)), (c, c), rotation);
            data[(y * size + x) as usize] = base.get(nx.round() as u32, ny.round() as u32);
        }
    }
    DemGrid::new(size as u32, size as u32, data)
}

/// Run `generate_isolines` on `base` rotated by `rotation`, then rotate the
/// result back into the base grid's coordinate frame.
fn run(base: &DemGrid, interval: f32, rotation: i32) -> Lines {
    let g = rotated_grid(base, rotation);
    let size = f64::from(base.width);
    let c = (size - 1.0) / 2.0;
    let mut out: Lines = BTreeMap::new();
    for (ele, lines) in generate_isolines(&g, interval, EXTENT, 0) {
        let conv = lines
            .into_iter()
            .map(|line| {
                let mut rl = Vec::with_capacity(line.len());
                let mut i = 0;
                while i < line.len() {
                    let gx = line[i] * (size - 1.0) / EXTENT;
                    let gy = line[i + 1] * (size - 1.0) / EXTENT;
                    let (rx, ry) = rotate((gx, gy), (c, c), rotation);
                    rl.push(rx);
                    rl.push(ry);
                    i += 2;
                }
                rl
            })
            .collect();
        out.insert(ele, conv);
    }
    out
}

fn expected(pairs: &[(i64, &[&[f64]])]) -> Lines {
    pairs
        .iter()
        .map(|(ele, lines)| (*ele, lines.iter().map(|l| l.to_vec()).collect()))
        .collect()
}

/// Assert a case for the given rotations (0 only, or all four).
fn check(name: &str, interval: f32, g: &DemGrid, exp: &[(i64, &[&[f64]])], rotations: &[i32]) {
    let want = expected(exp);
    for &rot in rotations {
        let got = run(g, interval, rot);
        assert_eq!(got, want, "case '{name}' failed at rotation {rot}");
    }
}

const ALL: &[i32] = &[0, 90, 180, 270];
const ZERO: &[i32] = &[0];

#[test]
fn corner_halfway() {
    check(
        "corner halfway",
        2.0,
        &grid(&[&[1., 1.], &[1., 3.]]),
        &[(2, &[&[1., 0.5, 0.5, 1.]])],
        ALL,
    );
}

#[test]
fn corner_above_most_of_the_way() {
    check(
        "corner above most",
        2.0,
        &grid(&[&[1., 1.], &[1., 2.33333]]),
        &[(2, &[&[1., 0.75, 0.75, 1.]])],
        ALL,
    );
}

#[test]
fn two_contours() {
    check(
        "two contours",
        2.0,
        &grid(&[&[1., 1.], &[1., 5.]]),
        &[(2, &[&[1., 0.25, 0.25, 1.]]), (4, &[&[1., 0.75, 0.75, 1.]])],
        ALL,
    );
}

#[test]
fn edge_above_threshold() {
    check(
        "edge above threshold",
        2.0,
        &grid(&[&[1., 1.], &[2.33333, 2.33333]]),
        &[(2, &[&[1., 0.75, 0., 0.75]])],
        ALL,
    );
}

#[test]
fn corner_up_to_threshold_is_empty() {
    check(
        "corner up to threshold",
        2.0,
        &grid(&[&[1., 1.], &[1., 2.]]),
        &[],
        ALL,
    );
}

#[test]
fn omit_empty_point() {
    check(
        "omit empty point",
        2.0,
        &grid(&[&[2., 3.], &[3., 3.]]),
        &[(2, &[&[0., 0., 0., 0.]])],
        ALL,
    );
}

#[test]
fn side_up_to_threshold_is_empty() {
    check(
        "side up to threshold",
        2.0,
        &grid(&[&[1., 2.], &[1., 2.]]),
        &[],
        ALL,
    );
}

#[test]
fn side_down_to_threshold() {
    check(
        "side down to threshold",
        2.0,
        &grid(&[&[2., 3.], &[2., 3.]]),
        &[(2, &[&[0., 0., 0., 1.]])],
        ALL,
    );
}

#[test]
fn threshold_middle_is_empty() {
    check(
        "threshold middle",
        2.0,
        &grid(&[
            &[1., 1., 1., 1.],
            &[1., 2., 1., 1.],
            &[1., 2., 1., 1.],
            &[1., 1., 1., 1.],
        ]),
        &[],
        ALL,
    );
}

#[test]
fn corner_below_threshold() {
    check(
        "corner below threshold",
        2.0,
        &grid(&[&[1., 2.3333333], &[2.3333333, 2.3333333]]),
        &[(2, &[&[0.75, 0., 0., 0.75]])],
        ALL,
    );
}

#[test]
fn no_contours() {
    check("no contours", 2.0, &grid(&[&[1., 1.], &[1., 1.]]), &[], ALL);
}

#[test]
fn connect_segments() {
    check(
        "connect segments",
        2.0,
        &grid(&[&[1., 3., 3.], &[1., 1., 3.], &[1., 1., 1.]]),
        &[(2, &[&[0.5, 0., 1., 0.5, 1.5, 1., 2., 1.5]])],
        ALL,
    );
}

#[test]
fn saddle() {
    check(
        "saddle",
        2.0,
        &grid(&[&[1., 2.3333333], &[2.3333333, 1.]]),
        &[(2, &[&[0.25, 1., 0., 0.75], &[0.75, 0., 1., 0.25]])],
        ZERO,
    );
}

#[test]
fn center_point_above() {
    check(
        "center point above",
        2.0,
        &grid(&[&[1., 1., 1.], &[1., 3., 1.], &[1., 1., 1.]]),
        &[(2, &[&[1.5, 1., 1., 0.5, 0.5, 1., 1., 1.5, 1.5, 1.]])],
        ZERO,
    );
}

#[test]
fn center_point_below() {
    check(
        "center point below",
        2.0,
        &grid(&[&[3., 3., 3.], &[3., 1., 3.], &[3., 3., 3.]]),
        &[(2, &[&[1., 1.5, 0.5, 1., 1., 0.5, 1.5, 1., 1., 1.5]])],
        ZERO,
    );
}
