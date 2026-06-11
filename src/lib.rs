//! # maplibre-contour-rs
//!
//! Generate contour-line vector tiles (MVT) directly from raster-DEM tiles
//! (Mapbox Terrain-RGB or Terrarium encoding). This is a Rust port of
//! [`maplibre-contour`](https://github.com/onthegomap/maplibre-contour),
//! structured so the same core can be embedded in an Android/iOS app via FFI.
//!
//! ## Pipeline
//!
//! Fetch + decode a DEM tile and its neighbors ([`source`], [`dem`]) → sample a
//! buffered, optionally overzoomed elevation grid ([`buffer`]) → trace contours
//! ([`contour`]) → encode to MVT ([`mvt`]). [`pipeline::ContourTiler`] ties
//! these together over a [`cache`].
//!
//! With the `ffi` feature, [`ffi`] exposes a uniffi interface (a host-provided
//! [`ffi::DemTileFetcher`] + an [`ffi::ContourTiler`] object) for Kotlin/Swift;
//! see that module's docs for usage.

pub mod buffer;
pub mod cache;
pub mod config;
pub mod contour;
pub mod dem;
pub mod error;
pub mod mvt;
pub mod pipeline;
pub mod source;
pub mod tile;

#[cfg(feature = "ffi")]
pub mod ffi;

pub use config::{ContourConfig, Encoding};
pub use error::{Error, Result};
pub use pipeline::ContourTiler;
pub use tile::TileCoord;

#[cfg(feature = "ffi")]
uniffi::setup_scaffolding!();
