//! DEM tiles: the in-memory elevation grid and PNG/WebP decoding.
//!
//! A [`DemTile`] is a row-major buffer of elevation samples in meters. A raw
//! raster-DEM tile (Mapbox Terrain-RGB or Terrarium, PNG or WebP) is turned into one by
//! [`decode_tile`], which maps every pixel through [`Encoding::decode`].

use crate::config::Encoding;
use crate::error::{Error, Result};

/// Decoded elevation values for a tile (or a buffered assembly of tiles).
#[derive(Debug, Clone)]
pub struct DemTile {
    pub width: u32,
    pub height: u32,
    /// `width * height` samples, row-major, in meters.
    pub data: Vec<f32>,
}

impl DemTile {
    pub fn new(width: u32, height: u32, data: Vec<f32>) -> Self {
        debug_assert_eq!(data.len() as u32, width * height);
        Self {
            width,
            height,
            data,
        }
    }

    /// Allocate a grid filled with `fill` (e.g. NaN for "no data yet").
    pub fn filled(width: u32, height: u32, fill: f32) -> Self {
        Self {
            width,
            height,
            data: vec![fill; (width * height) as usize],
        }
    }

    #[inline]
    pub fn get(&self, x: u32, y: u32) -> f32 {
        self.data[(y * self.width + x) as usize]
    }

    #[inline]
    pub fn set(&mut self, x: u32, y: u32, v: f32) {
        self.data[(y * self.width + x) as usize] = v;
    }

    /// Min/max over all finite samples, used to pick contour levels.
    /// Returns `None` if the grid has no finite values at all.
    pub fn extent(&self) -> Option<(f32, f32)> {
        let mut min = f32::INFINITY;
        let mut max = f32::NEG_INFINITY;
        for &v in &self.data {
            if v.is_finite() {
                min = min.min(v);
                max = max.max(v);
            }
        }
        if min <= max {
            Some((min, max))
        } else {
            None
        }
    }
}

/// Decode raster-DEM `bytes` (PNG or WebP) into an elevation grid
/// (RGBA8 → [`Encoding::decode`]), keeping the image's exact pixel dimensions.
/// The format is detected from the bytes.
pub fn decode_tile(bytes: &[u8], encoding: Encoding) -> Result<DemTile> {
    let img = image::load_from_memory(bytes)?.to_rgba8();
    let (width, height) = img.dimensions();
    if width == 0 || height == 0 {
        return Err(Error::Decode("DEM tile has zero dimension".into()));
    }

    let mut data = Vec::with_capacity((width * height) as usize);
    for pixel in img.pixels() {
        let [r, g, b, _a] = pixel.0;
        data.push(encoding.decode(r, g, b));
    }

    Ok(DemTile::new(width, height, data))
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{ImageFormat, Rgba, RgbaImage};
    use std::io::Cursor;

    /// Encode a single elevation value as a 1x1 Terrarium PNG and decode it back.
    fn roundtrip_terrarium(r: u8, g: u8, b: u8) -> f32 {
        let mut img = RgbaImage::new(1, 1);
        img.put_pixel(0, 0, Rgba([r, g, b, 255]));
        let mut buf = Vec::new();
        img.write_to(&mut Cursor::new(&mut buf), ImageFormat::Png)
            .unwrap();
        let grid = decode_tile(&buf, Encoding::Terrarium).unwrap();
        grid.get(0, 0)
    }

    #[test]
    fn decodes_known_terrarium_pixel() {
        // R=128,G=0,B=0 -> 128*256 - 32768 = 0 m.
        assert!(roundtrip_terrarium(128, 0, 0).abs() < 1e-3);
        // R=129,G=0,B=0 -> 256 m above.
        assert!((roundtrip_terrarium(129, 0, 0) - 256.0).abs() < 1e-3);
    }

    #[test]
    fn extent_ignores_nan() {
        let mut g = DemTile::filled(2, 2, f32::NAN);
        g.set(0, 0, 10.0);
        g.set(1, 1, 30.0);
        assert_eq!(g.extent(), Some((10.0, 30.0)));
        assert_eq!(DemTile::filled(2, 2, f32::NAN).extent(), None);
    }
}
