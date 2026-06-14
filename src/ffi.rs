//! uniffi bindings for Android (Kotlin) and iOS (Swift).
//!
//! Mirrors maplibre-contour: put the DEM URL pattern and contour options in
//! [`ContourConfig`], implement [`DemTileFetcher`] to return the bytes for a
//! resolved tile URL, and call [`DemManager::tile`] for the MVT bytes. The
//! library only ever hands you the resolved URL, so fetching it with your own
//! HTTP stack lets any interceptor or cache apply. Contours above
//! `dem_max_zoom` are overzoomed from the ancestor DEM automatically.
//!
//! # Generating the bindings
//!
//! Build the library, then run the bundled generator against it:
//!
//! ```text
//! cargo build --release
//! cargo run --bin uniffi-bindgen -- generate \
//!     --library target/release/libmaplibre_contour_rs.<dylib|so> \
//!     --language <kotlin|swift> --out-dir bindings
//! ```
//!
//! Prebuilt Android (`jniLibs` + Kotlin) and iOS (`.xcframework` + Swift)
//! artifacts are attached to each GitHub Release; the recipe lives in
//! `.github/workflows/release.yml`.
//!
//! # Kotlin
//!
//! ```kotlin
//! class HttpDemFetcher(private val client: OkHttpClient) : DemTileFetcher {
//!     override fun fetch(url: String): ByteArray? {
//!         val resp = client.newCall(Request.Builder().url(url).build()).execute()
//!         return resp.use { if (it.isSuccessful) it.body?.bytes() else null }
//!     }
//! }
//!
//! val config = defaultConfig().copy(
//!     demUrlPattern = "https://example.com/dem/{z}/{x}/{y}.png",
//!     encoding = Encoding.TERRARIUM,
//!     demMaxZoom = 11u,
//!     overzoom = 1u,
//!     thresholds = parseThresholdSpec("11*200*1000~12*10*100~13*10*100"),
//! )
//! val tiler = DemManager(HttpDemFetcher(client), config)
//! val mvt: ByteArray = tiler.tile(14u, 9000u, 6000u)
//! ```
//!
//! # Swift
//!
//! ```swift
//! final class HttpDemFetcher: DemTileFetcher {
//!     func fetch(url: String) throws -> Data? {
//!         guard let u = URL(string: url) else { return nil }
//!         return try? Data(contentsOf: u)
//!     }
//! }
//!
//! var config = defaultConfig()
//! config.demUrlPattern = "https://example.com/dem/{z}/{x}/{y}.png"
//! config.demMaxZoom = 11
//! config.overzoom = 1
//! config.thresholds = parseThresholdSpec("11*200*1000~12*10*100")
//! let tiler = DemManager(fetcher: HttpDemFetcher(), config: config)
//! let mvt = try tiler.tile(z: 14, x: 9000, y: 6000)
//! ```

use std::sync::Arc;

use crate::config::{parse_thresholds, ContourConfig, ThresholdRule};
use crate::dem_source::{TileSource, UrlTemplate};
use crate::tile::TileCoord;

/// Error surfaced across the FFI boundary (flattened to its message).
#[derive(Debug, thiserror::Error, uniffi::Error)]
#[uniffi(flat_error)]
pub enum FfiError {
    #[error("{0}")]
    Tiler(String),
}

impl From<crate::Error> for FfiError {
    fn from(e: crate::Error) -> Self {
        FfiError::Tiler(e.to_string())
    }
}

/// Returns the DEM PNG bytes for a tile URL. Implemented on the host side
/// (e.g. an OkHttp call that your interceptor can catch); return `None` for a
/// tile with no data.
#[uniffi::export(with_foreign)]
pub trait DemTileFetcher: Send + Sync {
    fn fetch(&self, url: String) -> Result<Option<Vec<u8>>, FfiError>;
}

/// Resolves coordinates to URLs via the config's pattern and delegates fetching
/// to the foreign [`DemTileFetcher`].
struct UrlSource {
    fetcher: Arc<dyn DemTileFetcher>,
    template: UrlTemplate,
}

impl TileSource for UrlSource {
    fn fetch(&self, coord: TileCoord) -> crate::Result<Option<Vec<u8>>> {
        self.fetcher
            .fetch(self.template.resolve(coord))
            .map_err(|e| crate::Error::Source(e.to_string()))
    }
}

/// A contour tiler usable from Kotlin/Swift. Thread-safe; share one instance.
#[derive(uniffi::Object)]
pub struct DemManager {
    inner: crate::dem_manager::DemManager<UrlSource>,
}

#[uniffi::export]
impl DemManager {
    /// Build a tiler from a fetcher and configuration. The DEM URL pattern is
    /// taken from `config.dem_url_pattern`.
    #[uniffi::constructor]
    pub fn new(fetcher: Arc<dyn DemTileFetcher>, config: ContourConfig) -> Arc<Self> {
        let source = UrlSource {
            fetcher,
            template: UrlTemplate::new(config.dem_url_pattern.clone()),
        };
        Arc::new(Self {
            inner: crate::dem_manager::DemManager::new(source, config),
        })
    }

    /// Generate the contour MVT for tile `z/x/y`.
    pub fn tile(&self, z: u8, x: u32, y: u32) -> Result<Vec<u8>, FfiError> {
        Ok(self.inner.tile(TileCoord::new(z, x, y))?)
    }
}

/// A default [`ContourConfig`] (Terrarium, 256 px, 4096 extent) to tweak.
#[uniffi::export]
pub fn default_config() -> ContourConfig {
    ContourConfig::default()
}

/// Parse maplibre-contour's threshold spec, e.g. `"11*200*1000~12*10*100"`,
/// into per-zoom rules for [`ContourConfig::thresholds`].
#[uniffi::export]
pub fn parse_threshold_spec(spec: String) -> Vec<ThresholdRule> {
    parse_thresholds(&spec)
}
