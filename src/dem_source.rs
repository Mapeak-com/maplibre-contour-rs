//! Where raw DEM PNG bytes come from.
//!
//! [`TileSource`] is the coordinate-based abstraction the pipeline uses.
//! [`UrlTemplate`] turns a `{z}/{x}/{y}` pattern into a concrete URL, so a
//! source (or the host app, via FFI) can fetch — and intercept — by URL the
//! same way maplibre-contour's `DemSource({url})` does.

use crate::error::Result;
use crate::tile::TileCoord;

/// Fetches the raw (PNG) bytes for one DEM tile.
pub trait TileSource: Send + Sync {
    /// Return the bytes for `coord`, or `Ok(None)` for a tile that legitimately
    /// has no data (ocean / out of range) so the buffer step can clamp.
    fn fetch(&self, coord: TileCoord) -> Result<Option<Vec<u8>>>;
}

/// A DEM tile URL template with `{z}`, `{x}`, `{y}`, and `{-y}` (TMS, flipped
/// Y) placeholders.
#[derive(Debug, Clone)]
pub struct UrlTemplate(String);

impl UrlTemplate {
    pub fn new(pattern: impl Into<String>) -> Self {
        Self(pattern.into())
    }

    /// Substitute `coord` into the template.
    pub fn resolve(&self, coord: TileCoord) -> String {
        let flipped = (1u32 << coord.z).saturating_sub(1).saturating_sub(coord.y);
        self.0
            .replace("{z}", &coord.z.to_string())
            .replace("{x}", &coord.x.to_string())
            .replace("{-y}", &flipped.to_string())
            .replace("{y}", &coord.y.to_string())
    }
}

/// In-memory source for tests and examples.
#[derive(Default)]
pub struct MockTileSource {
    pub tiles: std::collections::HashMap<TileCoord, Vec<u8>>,
}

impl TileSource for MockTileSource {
    fn fetch(&self, coord: TileCoord) -> Result<Option<Vec<u8>>> {
        Ok(self.tiles.get(&coord).cloned())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_placeholders() {
        let t = UrlTemplate::new("slice://host/dem/{z}/{x}/{y}.webp");
        assert_eq!(
            t.resolve(TileCoord::new(11, 1200, 800)),
            "slice://host/dem/11/1200/800.webp"
        );
    }

    #[test]
    fn resolves_tms_y() {
        let t = UrlTemplate::new("https://host/{z}/{x}/{-y}.png");
        // z=2 -> 4 rows; y=1 flips to 2.
        assert_eq!(t.resolve(TileCoord::new(2, 0, 1)), "https://host/2/0/2.png");
    }
}
