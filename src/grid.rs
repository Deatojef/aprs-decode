use crate::error::AprsError;
use crate::types::lonlat::{Latitude, Longitude};

/// An APRS Maidenhead Grid Locator report.
///
/// DTI: `[`
///
/// Format: `[GGGG]` or `[GGGGSS]` where GGGG is a 4-char grid square
/// (2 uppercase letters + 2 digits) and SS is an optional 2-char subsquare
/// (2 letters, case-insensitive). The comment follows the closing `]`.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct AprsGridLocator {
    /// Grid locator: 4 or 6 characters (e.g. `IO91` or `IO91SX`).
    pub grid: Vec<u8>,
    pub comment: Vec<u8>,
}

impl AprsGridLocator {
    /// Decode from the information field (including the leading `[` DTI byte).
    pub(crate) fn parse(info: &[u8]) -> Result<Self, AprsError> {
        // info[0] = '[', then grid chars, then ']', then comment
        let body = info.get(1..).unwrap_or_default();

        let end = body
            .iter()
            .position(|&c| c == b']')
            .ok_or(AprsError::UnsupportedPositionFormat)?;

        let grid = body[..end].to_vec();

        if grid.len() != 4 && grid.len() != 6 {
            return Err(AprsError::UnsupportedPositionFormat);
        }
        if !grid[0].is_ascii_uppercase()
            || !grid[1].is_ascii_uppercase()
            || !grid[2].is_ascii_digit()
            || !grid[3].is_ascii_digit()
        {
            return Err(AprsError::UnsupportedPositionFormat);
        }
        if grid.len() == 6 && (!grid[4].is_ascii_alphabetic() || !grid[5].is_ascii_alphabetic()) {
            return Err(AprsError::UnsupportedPositionFormat);
        }

        let comment = body.get(end + 1..).unwrap_or_default().to_vec();
        Ok(Self { grid, comment })
    }

    /// Convert the Maidenhead grid square to approximate lat/lon (center of the cell).
    pub fn to_position(&self) -> Option<(Latitude, Longitude)> {
        if self.grid.len() < 4 {
            return None;
        }

        let fl = (self.grid[0] - b'A') as f64; // field longitude index
        let fa = (self.grid[1] - b'A') as f64; // field latitude index
        let sl = (self.grid[2] - b'0') as f64; // square longitude digit
        let sa = (self.grid[3] - b'0') as f64; // square latitude digit

        let (lon, lat) = if self.grid.len() >= 6 {
            // 6-char: subsquare adds 1/24 of a square (5' for lon, 2.5' for lat)
            let ssl = (self.grid[4].to_ascii_lowercase() - b'a') as f64;
            let ssa = (self.grid[5].to_ascii_lowercase() - b'a') as f64;
            let lon = fl * 20.0 + sl * 2.0 + ssl * (2.0 / 24.0) + (1.0 / 24.0) - 180.0;
            let lat = fa * 10.0 + sa * 1.0 + ssa * (1.0 / 24.0) + (0.5 / 24.0) - 90.0;
            (lon, lat)
        } else {
            // 4-char: center of the 2°×1° cell
            let lon = fl * 20.0 + sl * 2.0 + 1.0 - 180.0;
            let lat = fa * 10.0 + sa * 1.0 + 0.5 - 90.0;
            (lon, lat)
        };

        Some((Latitude::new(lat)?, Longitude::new(lon)?))
    }

    pub fn encode(&self) -> Vec<u8> {
        let mut out = vec![b'['];
        out.extend_from_slice(&self.grid);
        out.push(b']');
        out.extend_from_slice(&self.comment);
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use approx::assert_relative_eq;

    #[test]
    fn parse_4char() {
        let g = AprsGridLocator::parse(b"[IO91]").unwrap();
        assert_eq!(g.grid, b"IO91");
        assert!(g.comment.is_empty());
    }

    #[test]
    fn parse_6char_with_comment() {
        let g = AprsGridLocator::parse(b"[IO91SX]comment here").unwrap();
        assert_eq!(g.grid, b"IO91SX");
        assert_eq!(g.comment, b"comment here");
    }

    #[test]
    fn parse_missing_bracket() {
        assert!(AprsGridLocator::parse(b"[IO91SX").is_err());
    }

    #[test]
    fn parse_bad_length() {
        assert!(AprsGridLocator::parse(b"[IO9]").is_err());
    }

    #[test]
    fn to_position_4char() {
        let g = AprsGridLocator {
            grid: b"JO22".to_vec(),
            comment: vec![],
        };
        let (lat, lon) = g.to_position().unwrap();
        // JO22 center: lon = 9*20 + 2*2 + 1 - 180 = 5°, lat = 14*10 + 2 + 0.5 - 90 = 52.5°
        assert_relative_eq!(lat.value(), 52.5, epsilon = 0.01);
        assert_relative_eq!(lon.value(), 5.0, epsilon = 0.01);
    }

    #[test]
    fn to_position_6char() {
        let g = AprsGridLocator {
            grid: b"FN31pr".to_vec(),
            comment: vec![],
        };
        let (lat, lon) = g.to_position().unwrap();
        // FN31pr: near New York City area
        assert!(lat.value() > 41.0 && lat.value() < 42.0);
        assert!(lon.value() > -73.0 && lon.value() < -72.0);
    }

    #[test]
    fn encode_round_trip() {
        let raw = b"[IO91SX]Hello";
        let g = AprsGridLocator::parse(raw).unwrap();
        assert_eq!(g.encode().as_slice(), raw.as_slice());
    }
}
