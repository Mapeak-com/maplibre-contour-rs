//! Encoding, contour thresholds, and the `ContourConfig` knob bag.
//!
//! Mirrors maplibre-contour's `DemSource` + contour options, including the
//! per-zoom `thresholds`, `multiplier`, and `overzoom` behavior.

/// How elevation (meters) is packed into the RGB channels of a DEM tile.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum Encoding {
    /// Mapbox Terrain-RGB: `height = -10000 + (R*65536 + G*256 + B) * 0.1`.
    Mapbox,
    /// Terrarium (AWS open dataset): `height = R*256 + G + B/256 - 32768`.
    Terrarium,
}

impl Encoding {
    /// Decode one pixel to elevation in meters.
    #[inline]
    pub fn decode(self, r: u8, g: u8, b: u8) -> f32 {
        let (r, g, b) = (r as f32, g as f32, b as f32);
        match self {
            Encoding::Mapbox => -10000.0 + (r * 65536.0 + g * 256.0 + b) * 0.1,
            Encoding::Terrarium => r * 256.0 + g + b / 256.0 - 32768.0,
        }
    }
}

/// Contour spacing for one zoom level.
///
/// `intervals[0]` is the minor spacing (lines are traced at every multiple of
/// it). Each further entry is a coarser spacing whose multiples are tagged with
/// a higher `level`: a line's level is the largest index `i` for which its
/// elevation is a multiple of `intervals[i]`. So `[200, 1000]` traces every
/// 200 m and marks every 1000 m as major (`level = 1`).
#[derive(Debug, Clone, uniffi::Record)]
pub struct ThresholdRule {
    /// Lowest zoom this rule applies to (used by [`ContourConfig::thresholds_for`]).
    pub zoom: u8,
    /// Contour spacings in meters (pre-`multiplier`), minor first.
    pub intervals: Vec<f32>,
}

impl ThresholdRule {
    /// The minor contour interval (`intervals[0]`); lines are traced at every
    /// multiple of it. 0 if the rule is empty.
    pub fn interval(&self) -> f32 {
        self.intervals.first().copied().unwrap_or(0.0)
    }

    /// The level for a contour at `elevation`: the largest index `i` for which
    /// `elevation` is a multiple of `intervals[i]` (matching maplibre-contour's
    /// `max(levels.map((l, i) => ele % l === 0 ? i : 0))`).
    pub fn level_for(&self, elevation: f32) -> u32 {
        let mut level = 0;
        for (i, &spacing) in self.intervals.iter().enumerate() {
            if spacing > 0.0 && is_multiple(elevation, spacing) {
                level = level.max(i as u32);
            }
        }
        level
    }
}

#[inline]
fn is_multiple(value: f32, spacing: f32) -> bool {
    let ratio = (value / spacing).round();
    (ratio * spacing - value).abs() <= 1e-3 * spacing.max(1.0)
}

/// Parse maplibre-contour's threshold spec, `"zoom*minor*major~zoom*minor..."`,
/// e.g. `"11*200*1000~12*10*100"`. Unparseable segments are skipped.
pub fn parse_thresholds(spec: &str) -> Vec<ThresholdRule> {
    spec.split('~')
        .filter_map(|part| {
            let mut nums = part.split('*').map(|s| s.trim());
            let zoom = nums.next()?.parse().ok()?;
            let intervals: Vec<f32> = nums.filter_map(|s| s.parse().ok()).collect();
            (!intervals.is_empty()).then_some(ThresholdRule { zoom, intervals })
        })
        .collect()
}

/// Everything needed to turn DEM tiles into contour MVT tiles.
#[derive(Debug, Clone, uniffi::Record)]
pub struct ContourConfig {
    /// DEM pixel encoding.
    pub encoding: Encoding,
    /// Source DEM tile size in pixels (256 or 512).
    pub tile_size: u32,
    /// Output MVT extent (almost always 4096).
    pub extent: u32,
    /// Buffer, in tile pixels, sampled from neighbors on every edge to keep
    /// contours continuous across seams.
    pub buffer_px: u32,
    /// DEM tile URL template, with `{z}`/`{x}`/`{y}` (and `{-y}` for TMS)
    /// placeholders. Used by the FFI fetcher; the URL is what host code (and an
    /// HTTP interceptor) sees.
    pub dem_url_pattern: String,
    /// Highest zoom at which DEM tiles exist. Requests above it are served by
    /// overzooming an ancestor tile.
    pub dem_max_zoom: u8,
    /// Extra levels to coarsen the DEM by before sampling: the source tile zoom
    /// is `min(z - overzoom, dem_max_zoom)`. 0 = use the exact zoom when present.
    pub overzoom: u8,
    /// Per-zoom contour spacing rules; the active rule is the highest `zoom`
    /// that is `<= requested zoom` (see [`ContourConfig::thresholds_for`]).
    pub thresholds: Vec<ThresholdRule>,
    /// Elevation unit scale applied before contouring (1.0 = meters,
    /// 3.28084 = feet). Affects threshold matching and the `ele` attribute.
    pub multiplier: f32,
    /// Name of the layer written into the MVT.
    pub layer_name: String,
    /// Attribute key for the elevation value.
    pub elevation_key: String,
    /// Attribute key for the major/minor level.
    pub level_key: String,
}

impl ContourConfig {
    /// The threshold rule active at `zoom`, or `None` below the lowest rule
    /// (matching maplibre-contour: no contours under the min configured zoom).
    pub fn thresholds_for(&self, zoom: u8) -> Option<&ThresholdRule> {
        self.thresholds
            .iter()
            .filter(|r| r.zoom <= zoom)
            .max_by_key(|r| r.zoom)
    }

    /// The DEM tile zoom to sample for a contour tile at `zoom`.
    pub fn source_zoom(&self, zoom: u8) -> u8 {
        zoom.saturating_sub(self.overzoom).min(self.dem_max_zoom)
    }
}

impl Default for ContourConfig {
    fn default() -> Self {
        Self {
            encoding: Encoding::Terrarium,
            tile_size: 256,
            extent: 4096,
            buffer_px: 1,
            dem_url_pattern: String::new(),
            dem_max_zoom: 12,
            overzoom: 0,
            thresholds: vec![ThresholdRule {
                zoom: 0,
                intervals: vec![50.0, 250.0],
            }],
            multiplier: 1.0,
            layer_name: "contours".to_string(),
            elevation_key: "ele".to_string(),
            level_key: "level".to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decode_known_points() {
        assert!((Encoding::Mapbox.decode(0, 0, 0) - (-10000.0)).abs() < 1e-3);
        assert!(Encoding::Terrarium.decode(128, 0, 0).abs() < 1e-3);
    }

    #[test]
    fn level_tags_majors() {
        let rule = ThresholdRule {
            zoom: 11,
            intervals: vec![200.0, 1000.0],
        };
        assert_eq!(rule.interval(), 200.0);
        assert_eq!(rule.level_for(200.0), 0); // minor
        assert_eq!(rule.level_for(800.0), 0);
        assert_eq!(rule.level_for(1000.0), 1); // multiple of 1000 -> major
        assert_eq!(rule.level_for(2000.0), 1);
    }

    #[test]
    fn parse_and_lookup_thresholds() {
        let rules = parse_thresholds("11*200*1000~12*10*100~14*10*100");
        assert_eq!(rules.len(), 3);
        let cfg = ContourConfig {
            thresholds: rules,
            ..Default::default()
        };
        assert!(cfg.thresholds_for(10).is_none()); // below the lowest rule
        assert_eq!(
            cfg.thresholds_for(11).unwrap().intervals,
            vec![200.0, 1000.0]
        );
        assert_eq!(cfg.thresholds_for(13).unwrap().zoom, 12); // nearest <=
        assert_eq!(cfg.thresholds_for(20).unwrap().zoom, 14);
    }

    #[test]
    fn source_zoom_overzooms_above_dem_max() {
        let cfg = ContourConfig {
            dem_max_zoom: 11,
            overzoom: 1,
            ..Default::default()
        };
        assert_eq!(cfg.source_zoom(16), 11); // min(16-1, 11)
        assert_eq!(cfg.source_zoom(11), 10); // min(11-1, 11)
        assert_eq!(cfg.source_zoom(5), 4);
    }
}
