//! Minimal end-to-end example.
//!
//! Run with: `cargo run --example generate_tile`
//!
//! Builds a synthetic "hill" DEM (a Gaussian bump) for the requested tile and
//! its 8 neighbors, runs the full decode → buffer → contour → MVT pipeline,
//! and prints how many contour features came out. Swap [`hill_source`] for a
//! real HTTP / on-disk source to tile actual Terrarium or Mapbox DEMs.

use std::collections::HashMap;
use std::io::Cursor;

use maplibre_contour_rs::config::ThresholdRule;
use maplibre_contour_rs::source::MockTileSource;
use maplibre_contour_rs::tile::Neighborhood;
use maplibre_contour_rs::{ContourConfig, ContourTiler, Encoding, TileCoord};

use image::{ImageFormat, Rgba, RgbaImage};

const TILE: u32 = 256;

/// Encode meters as Terrarium RGB: `h = R*256 + G + B/256 - 32768`.
fn terrarium_rgb(height: f32) -> [u8; 3] {
    let v = (height + 32768.0).clamp(0.0, 65535.999);
    let r = (v / 256.0).floor();
    let g = (v - r * 256.0).floor();
    let b = ((v - r * 256.0 - g) * 256.0).round();
    [r as u8, g as u8, b as u8]
}

/// Render one tile's DEM as a Terrarium PNG from a global height field.
fn dem_png(coord: TileCoord, height: impl Fn(f64, f64) -> f32) -> Vec<u8> {
    let mut img = RgbaImage::new(TILE, TILE);
    for py in 0..TILE {
        for px in 0..TILE {
            let gx = (coord.x * TILE + px) as f64;
            let gy = (coord.y * TILE + py) as f64;
            let [r, g, b] = terrarium_rgb(height(gx, gy));
            img.put_pixel(px, py, Rgba([r, g, b, 255]));
        }
    }
    let mut buf = Vec::new();
    img.write_to(&mut Cursor::new(&mut buf), ImageFormat::Png)
        .unwrap();
    buf
}

/// A mock source serving a Gaussian "hill" centered on `center`'s neighborhood.
fn hill_source(center: TileCoord) -> MockTileSource {
    // Center the bump on the middle of the requested tile.
    let cx = (center.x as f64 + 0.5) * TILE as f64;
    let cy = (center.y as f64 + 0.5) * TILE as f64;
    let height = move |gx: f64, gy: f64| {
        let d2 = (gx - cx).powi(2) + (gy - cy).powi(2);
        // ~1200 m peak, ~half a tile wide.
        (1200.0 * (-d2 / (2.0 * (TILE as f64 * 0.5).powi(2))).exp()) as f32
    };

    let mut tiles = HashMap::new();
    for c in Neighborhood::around(center).coords() {
        tiles.insert(c, dem_png(c, height));
    }
    MockTileSource { tiles }
}

fn main() {
    let coord = TileCoord::new(12, 2048, 1361);

    let config = ContourConfig {
        encoding: Encoding::Terrarium,
        tile_size: TILE,
        // Every 100 m, major every 500 m, from zoom 0 up.
        thresholds: vec![ThresholdRule {
            zoom: 0,
            intervals: vec![100.0, 500.0],
        }],
        ..Default::default()
    };
    let interval = config.thresholds_for(coord.z).unwrap().intervals[0];
    let tiler = ContourTiler::new(hill_source(coord), config);

    match tiler.tile(coord) {
        Ok(bytes) => {
            println!(
                "encoded {} MVT bytes for {coord:?} (contours every {interval} m)",
                bytes.len()
            )
        }
        Err(e) => eprintln!("error: {e}"),
    }
}
