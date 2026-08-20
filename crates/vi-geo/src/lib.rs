//! H3 spatial intelligence. Cells are stored alongside every case at multiple
//! resolutions so disparity queries (conviction rate by neighborhood, jury-pool
//! composition joins against ACS census data) are pure SQL.
#![forbid(unsafe_code)]

use h3o::{LatLng, Resolution};

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
}
