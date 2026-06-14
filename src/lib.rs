//! # maplibre-contour-rs
//!
//! Generate contour-line vector tiles (MVT) directly from raster-DEM tiles
//! (Mapbox Terrain-RGB or Terrarium encoding). This is a Rust port of
//! [`maplibre-contour`](https://github.com/onthegomap/maplibre-contour),
//! structured so the same core can be embedded in an Android/iOS app via FFI.
//!
//! ## Pipeline
//!
//! Fetch + decode a DEM tile and its neighbors ([`dem_source`],
//! [`decode_image`]) → sample a buffered, optionally overzoomed elevation grid
//! ([`height_tile`]) → trace contours ([`isolines`]) → encode to MVT
//! ([`vtpbf`]). [`dem_manager::DemManager`] ties these together over
//! a [`cache`].
//!
//! Module names mirror their counterparts in maplibre-contour's TypeScript
//! source (`height-tile.ts`, `isolines.ts`, `decode-image.ts`, `dem-source.ts`,
//! `vtpbf.ts`, `local-dem-manager.ts`) so the two trees line up file-for-file.
//!
//! With the `ffi` feature, [`ffi`] exposes a uniffi interface (a host-provided
//! [`ffi::DemTileFetcher`] + an [`ffi::DemManager`] object) for Kotlin/Swift;
//! see that module's docs for usage.

pub mod cache;
pub mod config;
pub mod decode_image;
pub mod dem_manager;
pub mod dem_source;
pub mod error;
pub mod height_tile;
pub mod isolines;
pub mod tile;
pub mod vtpbf;

pub mod ffi;

pub use config::{ContourConfig, Encoding};
pub use dem_manager::DemManager;
pub use error::{Error, Result};
pub use tile::TileCoord;

uniffi::setup_scaffolding!();
