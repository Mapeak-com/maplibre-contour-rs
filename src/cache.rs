//! LRU cache of decoded DEM tiles, keyed by coordinate, so adjacent and
//! overzoomed tiles reuse decoded ancestors instead of re-fetching PNGs.

use std::num::NonZeroUsize;
use std::sync::{Arc, Mutex};

use lru::LruCache;

use crate::decode_image::DemTile;
use crate::tile::TileCoord;

/// Thread-safe LRU cache of decoded elevation grids.
#[derive(Clone)]
pub struct DemCache {
    inner: Arc<Mutex<LruCache<TileCoord, Arc<DemTile>>>>,
}

impl DemCache {
    /// Create a cache holding up to `capacity` decoded tiles.
    pub fn new(capacity: usize) -> Self {
        let cap = NonZeroUsize::new(capacity.max(1)).expect("capacity >= 1");
        Self {
            inner: Arc::new(Mutex::new(LruCache::new(cap))),
        }
    }

    pub fn get(&self, coord: &TileCoord) -> Option<Arc<DemTile>> {
        self.inner.lock().unwrap().get(coord).cloned()
    }

    pub fn put(&self, coord: TileCoord, grid: Arc<DemTile>) {
        self.inner.lock().unwrap().put(coord, grid);
    }
}

impl Default for DemCache {
    fn default() -> Self {
        Self::new(64)
    }
}
