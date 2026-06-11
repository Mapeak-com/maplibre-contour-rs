//! Transform contour geometry into tile/extent space and serialize to MVT.
//!
//! Lines arrive in buffered-grid pixel coordinates; here we drop the buffer
//! margin, scale into `0..extent`, and write one layer with a feature per
//! contour carrying `ele` and `level` attributes. geozero's [`ToMvt`] does the
//! command-stream encoding; we assemble the layer and key/value tables.

use geo_types::{Coord, Geometry, LineString, MultiLineString};
use geozero::mvt::{tile, Message, Tile};
use geozero::ToMvt;

use crate::config::ContourConfig;
use crate::contour::ContourLine;
use crate::error::{Error, Result};

/// Encode `lines` into MVT bytes for one tile.
pub fn encode_mvt(lines: &[ContourLine], config: &ContourConfig) -> Result<Vec<u8>> {
    let buffer = config.buffer_px as f64;
    let scale = config.extent as f64 / config.tile_size as f64;
    let to_tile = |c: Coord<f64>| Coord {
        x: (c.x - buffer) * scale,
        y: (c.y - buffer) * scale,
    };

    let keys = vec![config.elevation_key.clone(), config.level_key.clone()];
    let mut values: Vec<tile::Value> = Vec::new();
    let mut features: Vec<tile::Feature> = Vec::with_capacity(lines.len());

    for (i, line) in lines.iter().enumerate() {
        let geometry = MultiLineString(
            line.geometry
                .0
                .iter()
                .map(|ls| LineString(ls.0.iter().map(|&c| to_tile(c)).collect()))
                .collect(),
        );

        let mut feature = Geometry::MultiLineString(geometry)
            .to_mvt_unscaled()
            .map_err(|e| Error::Mvt(e.to_string()))?;
        feature.id = Some(i as u64 + 1);

        let ele_idx = intern(&mut values, dbl(line.elevation as f64));
        let level_idx = intern(&mut values, int(line.level as i64));
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

    fn line_at(elevation: f32, level: u32, pts: &[(f64, f64)]) -> ContourLine {
        ContourLine {
            elevation,
            level,
            geometry: MultiLineString(vec![LineString(
                pts.iter().map(|&(x, y)| Coord { x, y }).collect(),
            )]),
        }
    }

    #[test]
    fn encodes_one_layer_with_attributes() {
        let config = ContourConfig {
            tile_size: 256,
            extent: 4096,
            buffer_px: 1,
            ..Default::default()
        };
        let lines = vec![
            line_at(100.0, 1, &[(1.0, 1.0), (257.0, 1.0)]),
            line_at(50.0, 0, &[(1.0, 1.0), (1.0, 257.0)]),
        ];

        let bytes = encode_mvt(&lines, &config).unwrap();
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
