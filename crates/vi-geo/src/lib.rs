//! H3 spatial intelligence. Cells are stored alongside every case at multiple
//! resolutions so disparity queries (conviction rate by neighborhood, jury-pool
//! composition joins against ACS census data) are pure SQL.
#![forbid(unsafe_code)]

use h3o::{CellIndex, LatLng, Resolution};
use std::str::FromStr;

#[derive(Debug, thiserror::Error)]
#[error("{0}")]
pub struct GeoError(String);

/// Multi-resolution ladder used at ingest time.
pub const RES_LADDER: [u8; 3] = [5, 7, 9];

pub fn cell_for(lat: f64, lng: f64, res: u8) -> Result<String, GeoError> {
    let ll = LatLng::new(lat, lng).map_err(|e| GeoError(e.to_string()))?;
    let r = Resolution::try_from(res).map_err(|e| GeoError(e.to_string()))?;
    Ok(ll.to_cell(r).to_string())
}

pub fn ladder(lat: f64, lng: f64) -> Vec<(u8, String)> {
    RES_LADDER
        .iter()
        .filter_map(|&r| cell_for(lat, lng, r).ok().map(|c| (r, c)))
        .collect()
}

fn parse_cell(cell: &str) -> Result<CellIndex, GeoError> {
    CellIndex::from_str(cell).map_err(|e| GeoError(e.to_string()))
}

/// Inclusive k-ring (H3 `grid_disk`): origin cell plus neighbors out to `k`.
pub fn k_ring(cell: &str, k: u32) -> Result<Vec<String>, GeoError> {
    let idx = parse_cell(cell)?;
    Ok(idx
        .grid_disk::<Vec<_>>(k)
        .into_iter()
        .map(|c| c.to_string())
        .collect())
}

/// Ordered boundary vertices as (lat, lng) degrees for GeoJSON-style payloads.
pub fn cell_boundary(cell: &str) -> Result<Vec<(f64, f64)>, GeoError> {
    let idx = parse_cell(cell)?;
    Ok(idx
        .boundary()
        .iter()
        .map(|ll| (ll.lat(), ll.lng()))
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deterministic_cells() {
        let a = cell_for(37.7749, -122.4194, 8).unwrap();
        assert_eq!(a, cell_for(37.7749, -122.4194, 8).unwrap());
        assert!(!a.is_empty());
        assert_eq!(ladder(37.7749, -122.4194).len(), RES_LADDER.len());
    }

    #[test]
    fn rejects_out_of_range() {
        assert!(cell_for(0.0, 0.0, 16).is_err());
        assert!(cell_for(0.0, 0.0, 99).is_err());
    }

    #[test]
    fn k_ring_includes_origin_and_is_deterministic() {
        let cell = cell_for(37.7749, -122.4194, 8).unwrap();
        let a = k_ring(&cell, 1).unwrap();
        let b = k_ring(&cell, 1).unwrap();
        assert_eq!(a, b);
        assert!(a.contains(&cell));
        // origin + up to 6 neighbors at k=1
        assert!(!a.is_empty() && a.len() <= 7);
        let k0 = k_ring(&cell, 0).unwrap();
        assert_eq!(k0, vec![cell.clone()]);
        assert!(cell_boundary(&cell).unwrap().len() >= 6);
    }

    #[test]
    fn k_ring_rejects_garbage() {
        assert!(k_ring("not-a-cell", 1).is_err());
    }
}
