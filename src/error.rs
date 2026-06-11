//! Crate-wide error type.

use thiserror::Error;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, Error)]
pub enum Error {
    #[error("failed to decode DEM tile: {0}")]
    Decode(String),

    #[error("tile source error: {0}")]
    Source(String),

    #[error("requested tile is out of range for zoom {zoom}: ({x}, {y})")]
    TileOutOfRange { zoom: u8, x: i64, y: i64 },

    #[error("contour generation failed: {0}")]
    Contour(String),

    #[error("MVT encoding failed: {0}")]
    Mvt(String),

    #[error("image error: {0}")]
    Image(#[from] image::ImageError),

    #[error(transparent)]
    Io(#[from] std::io::Error),
}
