use crate::error::AprsError;
use crate::types::compressed::{Altitude, CompressedCs};
use crate::types::lonlat::{Latitude, Longitude, Precision};
use crate::types::symbol::Symbol;
use std::ops::RangeInclusive;

/// DAO (Datum Ambiguity Override) precision extension parsed from the comment field.
///
/// When present, DAO provides sub-hundredth-of-a-minute precision beyond what the
/// standard DDmm.mm format can encode. The offsets are applied to lat/lon at parse
/// time so callers always receive refined coordinates.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum Dao {
    /// Human-readable form `!WXY!`: X and Y are decimal digits 0–9 encoding an extra
    /// digit of minute resolution (thousandths of a minute ≈ 1.85 m).
    HumanReadable { lat_digit: u8, lon_digit: u8 },
    /// Base-91 form `!wxy!`: two base-91 characters encoding the sub-hundredth position
    /// within the current cell (≈ 0.2 m resolution).
    Base91 { lat_offset: u8, lon_offset: u8 },
}

impl Dao {
    /// Latitude and longitude adjustments in degrees that refine the base position.
    /// These are always non-negative; the caller applies them in the direction of the
    /// base coordinate's sign.
    pub fn offsets_degrees(&self) -> (f64, f64) {
        match self {
            Dao::HumanReadable { lat_digit, lon_digit } => {
                // each digit = 0.001 minutes = 0.001/60 degrees
                ((*lat_digit as f64) / 60_000.0, (*lon_digit as f64) / 60_000.0)
            }
            Dao::Base91 { lat_offset, lon_offset } => {
                // value 0..90 → 0..0.01 minutes = 0..0.01/60 degrees
                ((*lat_offset as f64) / (91.0 * 6000.0), (*lon_offset as f64) / (91.0 * 6000.0))
            }
        }
    }

    /// Scan `data` for the `!X__!` DAO pattern and return the first match.
    pub(crate) fn find_in_comment(data: &[u8]) -> Option<Self> {
        for i in 0..data.len().saturating_sub(4) {
            if data[i] == b'!' && data.get(i + 4) == Some(&b'!') {
                let prefix = data[i + 1];
                let d1 = data[i + 2];
                let d2 = data[i + 3];
                if prefix.is_ascii_uppercase() && d1.is_ascii_digit() && d2.is_ascii_digit() {
                    return Some(Dao::HumanReadable {
                        lat_digit: d1 - b'0',
                        lon_digit: d2 - b'0',
                    });
                }
                if prefix.is_ascii_lowercase() && (0x21..=0x7B).contains(&d1) && (0x21..=0x7B).contains(&d2) {
                    return Some(Dao::Base91 {
                        lat_offset: d1 - 33,
                        lon_offset: d2 - 33,
                    });
                }
            }
        }
        None
    }
}

/// A parsed APRS position, combining coordinates, symbol, and optional metadata.
///
/// DAO offsets (when present) are applied to `latitude` and `longitude` at parse
/// time — callers receive the most precise available value directly.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Position {
    pub latitude: Latitude,
    pub longitude: Longitude,
    pub precision: Precision,
    pub symbol: Symbol,
    /// The compressed csT block, or `None` for uncompressed positions.
    pub compressed_cs: Option<CompressedCs>,
    /// Altitude from `/A=NNNNNN` in the comment field. Not from compressed csT.
    pub altitude: Option<Altitude>,
    /// The raw DAO token, kept for round-trip fidelity.
    pub dao: Option<Dao>,
}

impl Position {
    pub fn latitude_bounding(&self) -> RangeInclusive<f64> {
        self.precision.range(self.latitude.value())
    }

    pub fn longitude_bounding(&self) -> RangeInclusive<f64> {
        self.precision.range(self.longitude.value())
    }

    /// Parse either a compressed or uncompressed position from the head of `b`.
    ///
    /// Returns `(remaining, position)` where `remaining` is the slice following the
    /// parsed position bytes (i.e. the comment field).
    pub(crate) fn parse(b: &[u8]) -> Result<(Option<&[u8]>, Self), AprsError> {
        if b.is_empty() {
            return Err(AprsError::UnsupportedPositionFormat);
        }
        if b[0].is_ascii_digit() {
            Self::parse_uncompressed(b)
        } else {
            Self::parse_compressed(b)
        }
    }

    fn parse_uncompressed(b: &[u8]) -> Result<(Option<&[u8]>, Self), AprsError> {
        if b.len() < 19 {
            return Err(AprsError::TruncatedPacket { expected: 19, got: b.len() });
        }
        let (lat, precision) = Latitude::parse_uncompressed(&b[0..8])?;
        let symbol_table = b[8] as char;
        let lon = Longitude::parse_uncompressed(&b[9..18], precision)?;
        let symbol_code = b[18] as char;
        let symbol = Symbol::new(symbol_table, symbol_code);

        let comment = b.get(19..);
        let comment_bytes = comment.unwrap_or_default();

        let altitude = altitude_in_comment(comment_bytes);
        let dao = Dao::find_in_comment(comment_bytes);

        // Apply DAO offsets to refine coordinates
        let (lat, lon) = if let Some(ref d) = dao {
            let (dlat, dlon) = d.offsets_degrees();
            let lat_sign = if lat.value() >= 0.0 { 1.0 } else { -1.0 };
            let lon_sign = if lon.value() >= 0.0 { 1.0 } else { -1.0 };
            let new_lat = Latitude::new(lat.value() + lat_sign * dlat).unwrap_or(lat);
            let new_lon = Longitude::new(lon.value() + lon_sign * dlon).unwrap_or(lon);
            (new_lat, new_lon)
        } else {
            (lat, lon)
        };

        Ok((
            b.get(19..),
            Self {
                latitude: lat,
                longitude: lon,
                precision,
                symbol,
                compressed_cs: None,
                altitude,
                dao,
            },
        ))
    }

    fn parse_compressed(b: &[u8]) -> Result<(Option<&[u8]>, Self), AprsError> {
        if b.len() < 13 {
            return Err(AprsError::TruncatedPacket { expected: 13, got: b.len() });
        }
        let symbol_table = b[0] as char;
        let lat = Latitude::parse_compressed(&b[1..5])?;
        let lon = Longitude::parse_compressed(&b[5..9])?;
        let symbol_code = b[9] as char;
        let symbol = Symbol::new(symbol_table, symbol_code);

        let cst = CompressedCs::parse(b[10], b[11], b[12])?;

        // Altitude from compressed csT (only when NmeaSource is Gga)
        let altitude = match &cst {
            CompressedCs::Altitude(a, _) => Some(Altitude::new(a.feet)),
            _ => None,
        };

        Ok((
            b.get(13..),
            Self {
                latitude: lat,
                longitude: lon,
                precision: Precision::default(),
                symbol,
                compressed_cs: Some(cst),
                altitude,
                dao: None,
            },
        ))
    }

    /// Encode as uncompressed position bytes (19 bytes: lat + sym_table + lon + sym_code).
    pub(crate) fn encode_uncompressed(&self, out: &mut Vec<u8>) {
        self.latitude.encode_uncompressed(out, self.precision);
        out.push(self.symbol.table as u8);
        self.longitude.encode_uncompressed(out);
        out.push(self.symbol.code as u8);
    }

    /// Encode as compressed position bytes (13 bytes: sym_table + lat(4) + lon(4) + sym_code + csT(3)).
    pub(crate) fn encode_compressed(&self, out: &mut Vec<u8>) {
        out.push(self.symbol.table as u8);
        self.latitude.encode_compressed(out);
        self.longitude.encode_compressed(out);
        out.push(self.symbol.code as u8);
        if let Some(ref cst) = self.compressed_cs {
            cst.encode(out);
        } else {
            // Fallback: no csT data — use space + sT placeholder
            out.extend_from_slice(b" sT");
        }
    }
}

/// Extract `/A=NNNNNN` altitude from a comment field.
pub(crate) fn altitude_in_comment(data: &[u8]) -> Option<Altitude> {
    let s = std::str::from_utf8(data).ok()?;
    let start = s.find("/A=")?;
    let rest = &s[start + 3..];
    let end = rest.find(|c: char| !c.is_ascii_digit()).unwrap_or(rest.len());
    let feet: u32 = rest[..end].parse().ok()?;
    Some(Altitude::new(feet as f64))
}

#[cfg(test)]
mod tests {
    use super::*;
    use approx::assert_relative_eq;

    #[test]
    fn uncompressed_basic() {
        let (rem, pos) = Position::parse(b"4903.50N/07201.75W-Hello").unwrap();
        assert_relative_eq!(pos.latitude.value(), 49.05833333333333, epsilon = 1e-9);
        assert_relative_eq!(pos.longitude.value(), -72.02916666666667, epsilon = 1e-9);
        assert_eq!(pos.symbol.table, '/');
        assert_eq!(pos.symbol.code, '-');
        assert_eq!(rem.unwrap(), b"Hello");
    }

    #[test]
    fn uncompressed_altitude_in_comment() {
        let (_, pos) = Position::parse(b"4903.50N/07201.75W-/A=003054").unwrap();
        assert!(pos.altitude.is_some());
        let alt = pos.altitude.unwrap();
        assert_relative_eq!(alt.feet, 3054.0, epsilon = 0.5);
    }

    #[test]
    fn dao_human_readable_applied() {
        // DAO !W56! refines lat by 5/60000 deg, lon by 6/60000 deg
        let (_, pos) = Position::parse(b"4903.50N/07201.75W-!W56!").unwrap();
        assert_relative_eq!(
            pos.latitude.value(),
            49.05833333333333 + 5.0 / 60_000.0,
            epsilon = 1e-9
        );
    }

    #[test]
    fn uncompressed_encode_round_trip() {
        let raw = b"4903.50N/07201.75W-";
        let (_, pos) = Position::parse(raw).unwrap();
        let mut out = Vec::new();
        pos.encode_uncompressed(&mut out);
        assert_eq!(&out, raw);
    }

    #[test]
    fn altitude_in_comment_extracted() {
        let alt = altitude_in_comment(b"/A=001000extra").unwrap();
        assert_relative_eq!(alt.feet, 1000.0, epsilon = 0.1);
    }

    #[test]
    fn compressed_parse_known() {
        // From aprs-parser-rs test: "/ABCD#$%^- sT" (no-timestamp, compressed, no cs)
        // symbol_table='/', lat=ABCD, lon=#$%^, symbol_code='-', c=' ', s='s', t='T'
        let (_, pos) = Position::parse(b"/ABCD#$%^- sT").unwrap();
        assert_relative_eq!(pos.latitude.value(), 25.97004667573229, epsilon = 0.001);
        assert_relative_eq!(pos.longitude.value(), -171.95429033460567, epsilon = 0.001);
        assert_eq!(pos.symbol.table, '/');
        assert_eq!(pos.symbol.code, '-');
    }
}
