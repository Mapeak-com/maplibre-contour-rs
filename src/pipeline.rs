//! The end-to-end pipeline: tile coord in, MVT bytes out.

use std::sync::Arc;

use crate::buffer::sample_buffered;
use crate::cache::DemCache;
use crate::config::ContourConfig;
use crate::contour::contour_tile;
use crate::dem::{decode_tile, DemGrid};
use crate::error::Result;
use crate::mvt::encode_mvt;
use crate::source::TileSource;
use crate::tile::TileCoord;

/// Owns the source, cache, and config; produces contour MVT tiles.
pub struct ContourTiler<S: TileSource> {
    source: S,
    cache: DemCache,
    config: ContourConfig,
}

impl<S: TileSource> ContourTiler<S> {
    pub fn new(source: S, config: ContourConfig) -> Self {
        Self {
            source,
            cache: DemCache::default(),
            config,
        }
    }

    pub fn with_cache(mut self, cache: DemCache) -> Self {
        self.cache = cache;
        self
    }

    pub fn config(&self) -> &ContourConfig {
        &self.config
    }

    /// Fetch (or pull from cache) and decode a single DEM tile.
    fn dem_tile(&self, coord: TileCoord) -> Result<Option<Arc<DemGrid>>> {
        if let Some(grid) = self.cache.get(&coord) {
            return Ok(Some(grid));
        }
        match self.source.fetch(coord)? {
            None => Ok(None),
            Some(bytes) => {
                let grid = Arc::new(decode_tile(&bytes, self.config.encoding)?);
                self.cache.put(coord, grid.clone());
                Ok(Some(grid))
            }
        }
    }

    /// Generate the contour MVT for `coord`. Returns an empty-layer tile when no
    /// threshold rule applies at this zoom.
    pub fn tile(&self, coord: TileCoord) -> Result<Vec<u8>> {
        let Some(rule) = self.config.thresholds_for(coord.z) else {
            return encode_mvt(&[], &self.config);
        };

        let source_zoom = self.config.source_zoom(coord.z);
        let buffered = sample_buffered(
            coord,
            source_zoom,
            self.config.tile_size,
            self.config.buffer_px,
            |c| self.dem_tile(c),
        )?;

        let contours = contour_tile(
            &buffered,
            self.config.tile_size,
            self.config.buffer_px,
            rule,
            self.config.multiplier,
            self.config.extent,
        );
        encode_mvt(&contours, &self.config)
    }
}
