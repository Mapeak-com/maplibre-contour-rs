//! Serialize traced contours into a Mapbox Vector Tile.
//!
//! Contours arrive in `0..extent` tile coordinates (see [`crate::contour`]), so
//! this just builds one line feature per contour — carrying `ele` and `level`
//! attributes — and writes a single layer. geozero's [`ToMvt`] does the
//! command-stream encoding; we assemble the layer and key/value tables.

use geo_types::{Coord, Geometry, LineString, MultiLineString};
use geozero::mvt::{tile, Message, Tile};
use geozero::ToMvt;

use crate::config::ContourConfig;
use crate::contour::Contour;
use crate::error::{Error, Result};

/// Encode `contours` into MVT bytes for one tile.
pub fn encode_mvt(contours: &[Contour], config: &ContourConfig) -> Result<Vec<u8>> {
    let keys = vec![config.elevation_key.clone(), config.level_key.clone()];
    let mut values: Vec<tile::Value> = Vec::new();
    let mut features: Vec<tile::Feature> = Vec::with_capacity(contours.len());

    for (i, contour) in contours.iter().enumerate() {
        let geometry = MultiLineString(
            contour
                .lines
                .iter()
                .map(|flat| {
                    LineString(
                        flat.chunks_exact(2)
                            .map(|p| Coord { x: p[0], y: p[1] })
                            .collect(),
                    )
                })
                .collect(),
        );
        if geometry.0.iter().all(|ls| ls.0.len() < 2) {
            continue;
        }

        let mut feature = Geometry::MultiLineString(geometry)
            .to_mvt_unscaled()
            .map_err(|e| Error::Mvt(e.to_string()))?;
        feature.id = Some(i as u64 + 1);

        let ele_idx = intern(&mut values, dbl(contour.elevation as f64));
        let level_idx = intern(&mut values, int(contour.level as i64));
        feature.tags = vec![0, ele_idx, 1, level_idx]; // [key, value] pairs

        features.push(feature);
    }

    let layer = tile::Layer {
        version: 2,
        name: config.layer_name.clone(),
        features,
        keys,
        values,
        extent: Some(config.extent),
    };
    let tile = Tile {
        layers: vec![layer],
    };

    let mut buf = Vec::with_capacity(tile.encoded_len());
    tile.encode(&mut buf)
        .map_err(|e| Error::Mvt(e.to_string()))?;
    Ok(buf)
}

fn intern(values: &mut Vec<tile::Value>, v: tile::Value) -> u32 {
    if let Some(idx) = values.iter().position(|existing| *existing == v) {
        return idx as u32;
    }
    values.push(v);
    (values.len() - 1) as u32
}

fn dbl(x: f64) -> tile::Value {
    tile::Value {
        double_value: Some(x),
        ..Default::default()
    }
}

fn int(x: i64) -> tile::Value {
    tile::Value {
        int_value: Some(x),
        ..Default::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use geozero::mvt::tile::GeomType;

    fn contour(elevation: f32, level: u32, lines: Vec<Vec<f64>>) -> Contour {
        Contour {
            elevation,
            level,
            lines,
        }
    }

    #[test]
    fn encodes_one_layer_with_attributes() {
        let config = ContourConfig {
            extent: 4096,
            buffer_px: 1,
            ..Default::default()
        };
        let contours = vec![
            contour(100.0, 1, vec![vec![0.0, 0.0, 4096.0, 0.0]]),
            contour(50.0, 0, vec![vec![0.0, 0.0, 0.0, 4096.0]]),
        ];

        let bytes = encode_mvt(&contours, &config).unwrap();
        let decoded = Tile::decode(&bytes[..]).unwrap();
        assert_eq!(decoded.layers.len(), 1);
        let layer = &decoded.layers[0];
        assert_eq!(layer.name, "contours");
        assert_eq!(layer.extent, Some(4096));
        assert_eq!(layer.features.len(), 2);
        assert_eq!(layer.keys, vec!["ele", "level"]);

        let f = &layer.features[0];
        assert_eq!(f.r#type, Some(GeomType::Linestring as i32));
        assert_eq!(layer.values[f.tags[1] as usize].double_value, Some(100.0));
        assert_eq!(layer.values[f.tags[3] as usize].int_value, Some(1));
    }
}
