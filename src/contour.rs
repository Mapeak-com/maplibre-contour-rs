//! Trace contour lines through a buffered [`DemGrid`] with marching squares.

use contour::ContourBuilder;
use geo_types::MultiLineString;

use crate::config::ThresholdRule;
use crate::dem::DemGrid;
use crate::error::{Error, Result};

/// One traced contour: elevation, major/minor level, and geometry.
#[derive(Debug, Clone)]
pub struct ContourLine {
    pub elevation: f32,
    /// 0 = minor; higher = coarser/major (see [`ThresholdRule`]).
    pub level: u32,
    /// Geometry in buffered-grid pixel coordinates; the MVT step shifts and
    /// scales it into tile/extent space.
    pub geometry: MultiLineString<f64>,
}

/// Trace contours for `rule`, scaling elevations by `multiplier` first (so
/// thresholds and the reported elevation are in the display unit).
///
/// Returns an empty `Vec` for a flat grid or a rule with no crossing levels.
pub fn generate_contours(
    grid: &DemGrid,
    rule: &ThresholdRule,
    multiplier: f32,
    smooth: bool,
) -> Result<Vec<ContourLine>> {
    let Some((min, max)) = grid.extent() else {
        return Ok(Vec::new());
    };
    let (min, max) = (min * multiplier, max * multiplier);

    let levels = rule.levels(min, max);
    if levels.is_empty() {
        return Ok(Vec::new());
    }

    // The `contour` crate works in f64; the DEM grid is f32.
    let values: Vec<f64> = grid.data.iter().map(|&v| (v * multiplier) as f64).collect();
    let thresholds: Vec<f64> = levels.iter().map(|&(e, _)| e as f64).collect();

    let builder = ContourBuilder::new(grid.width as usize, grid.height as usize, smooth);
    let lines = builder
        .lines(&values, &thresholds)
        .map_err(|e| Error::Contour(e.to_string()))?;

    // `lines` yields one entry per threshold, in order.
    let mut out = Vec::with_capacity(lines.len());
    for (line, &(elevation, level)) in lines.into_iter().zip(levels.iter()) {
        let (geometry, _) = line.into_inner();
        if !geometry.0.is_empty() {
            out.push(ContourLine {
                elevation,
                level,
                geometry,
            });
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ramp(w: u32, h: u32) -> DemGrid {
        let mut g = DemGrid::filled(w, h, 0.0);
        for y in 0..h {
            for x in 0..w {
                g.set(x, y, y as f32);
            }
        }
        g
    }

    #[test]
    fn ramp_traces_minor_and_major() {
        let grid = ramp(16, 101); // elevations 0..=100
        let rule = ThresholdRule {
            zoom: 0,
            intervals: vec![10.0, 50.0],
        };
        let lines = generate_contours(&grid, &rule, 1.0, false).unwrap();
        assert_eq!(lines.len(), 11); // 0,10,..,100
        let major: Vec<f32> = lines
            .iter()
            .filter(|l| l.level == 1)
            .map(|l| l.elevation)
            .collect();
        assert_eq!(major, vec![0.0, 50.0, 100.0]);
    }

    #[test]
    fn multiplier_scales_elevations() {
        let grid = ramp(8, 101);
        let rule = ThresholdRule {
            zoom: 0,
            intervals: vec![100.0],
        };
        // x3.28084 (feet): 100 ft thresholds now cross the 0..328 ft range.
        let lines = generate_contours(&grid, &rule, 3.28084, false).unwrap();
        let eles: Vec<f32> = lines.iter().map(|l| l.elevation).collect();
        assert_eq!(eles, vec![0.0, 100.0, 200.0, 300.0]);
    }

    #[test]
    fn flat_grid_has_no_contours() {
        let rule = ThresholdRule {
            zoom: 0,
            intervals: vec![10.0],
        };
        let lines = generate_contours(&DemGrid::filled(8, 8, 42.0), &rule, 1.0, true).unwrap();
        assert!(lines.is_empty());
    }
}
