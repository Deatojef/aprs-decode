use crate::error::AprsError;
use crate::util::parse_bytes;
use std::ops::{Deref, RangeInclusive};

/// Granularity of a parsed position coordinate, inferred from trailing-space ambiguity.
#[derive(Debug, Copy, Clone, PartialOrd, PartialEq, Ord, Eq, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum Precision {
    TenDegree,
    OneDegree,
    TenMinute,
    OneMinute,
    TenthMinute,
    #[default]
    HundredthMinute,
}

impl Precision {
    /// Width of the precision cell in degrees.
    pub fn width(self) -> f64 {
        match self {
            Precision::HundredthMinute => 1.0 / 6000.0,
            Precision::TenthMinute => 1.0 / 600.0,
            Precision::OneMinute => 1.0 / 60.0,
            Precision::TenMinute => 1.0 / 6.0,
            Precision::OneDegree => 1.0,
            Precision::TenDegree => 10.0,
        }
    }

    pub fn range(self, center: f64) -> RangeInclusive<f64> {
        let w = self.width();
        (center - w / 2.0)..=(center + w / 2.0)
    }

    fn num_blank_digits(self) -> u8 {
        match self {
            Precision::HundredthMinute => 0,
            Precision::TenthMinute => 1,
            Precision::OneMinute => 2,
            Precision::TenMinute => 3,
            Precision::OneDegree => 4,
            Precision::TenDegree => 5,
        }
    }

    fn from_blank_digits(blanks: u8) -> Option<Self> {
        Some(match blanks {
            0 => Precision::HundredthMinute,
            1 => Precision::TenthMinute,
            2 => Precision::OneMinute,
            3 => Precision::TenMinute,
            4 => Precision::OneDegree,
            5 => Precision::TenDegree,
            _ => return None,
        })
    }
}

/// APRS latitude value in decimal degrees (positive = North, negative = South).
#[derive(Debug, Copy, Clone, PartialOrd, PartialEq, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(transparent))]
pub struct Latitude(f64);

impl Deref for Latitude {
    type Target = f64;
    fn deref(&self) -> &f64 {
        &self.0
    }
}

impl Latitude {
    pub fn new(value: f64) -> Option<Self> {
        if value.is_nan() || !(-90.0..=90.0).contains(&value) {
            None
        } else {
            Some(Self(value))
        }
    }

    pub fn value(self) -> f64 {
        self.0
    }

    /// Decompose into (degrees, whole minutes, hundredths-of-minute, is_north).
    pub fn dmh(self) -> (u32, u32, u32, bool) {
        let (is_north, v) = if self.0 >= 0.0 {
            (true, self.0)
        } else {
            (false, -self.0)
        };
        let deg = v as u32;
        let min = ((v - deg as f64) * 60.0) as u32;
        let mut hdths = ((v - deg as f64 - min as f64 / 60.0) * 6000.0).round() as u32;
        let mut min = min;
        let mut deg = deg;
        if hdths >= 100 {
            hdths = 0;
            min += 1;
        }
        if min >= 60 {
            min = 0;
            deg += 1;
        }
        (deg, min, hdths, is_north)
    }

    /// Parse 8-byte uncompressed latitude: `DDmm.mmN` or `DDmm.mmS`.
    /// Trailing spaces encode ambiguity (reduced precision).
    pub(crate) fn parse_uncompressed(b: &[u8]) -> Result<(Self, Precision), AprsError> {
        if b.len() != 8 || b[4] != b'.' {
            return Err(AprsError::InvalidLatitude { raw: b.to_vec() });
        }
        let is_north = match b[7] {
            b'N' => true,
            b'S' => false,
            _ => return Err(AprsError::InvalidLatitude { raw: b.to_vec() }),
        };
        let (deg, b0) = parse_pair_ambiguous(&[b[0], b[1]], false)
            .ok_or_else(|| AprsError::InvalidLatitude { raw: b.to_vec() })?;
        let (min, b1) = parse_pair_ambiguous(&[b[2], b[3]], b0 > 0)
            .ok_or_else(|| AprsError::InvalidLatitude { raw: b.to_vec() })?;
        let (hdths, b2) = parse_pair_ambiguous(&[b[5], b[6]], b1 > 0)
            .ok_or_else(|| AprsError::InvalidLatitude { raw: b.to_vec() })?;
        let blanks = b0 + b1 + b2;
        let precision = Precision::from_blank_digits(blanks)
            .ok_or_else(|| AprsError::InvalidLatitude { raw: b.to_vec() })?;
        let value = deg as f64 + min as f64 / 60.0 + hdths as f64 / 6000.0;
        let value = if is_north { value } else { -value };
        let lat =
            Latitude::new(value).ok_or_else(|| AprsError::InvalidLatitude { raw: b.to_vec() })?;
        Ok((lat, precision))
    }

    /// Parse 4-byte base-91 compressed latitude.
    pub(crate) fn parse_compressed(b: &[u8]) -> Result<Self, AprsError> {
        let enc =
            base91_decode4(b).ok_or_else(|| AprsError::InvalidLatitude { raw: b.to_vec() })?;
        let value = 90.0 - enc / 380926.0;
        Latitude::new(value).ok_or_else(|| AprsError::InvalidLatitude { raw: b.to_vec() })
    }

    pub(crate) fn encode_uncompressed(&self, out: &mut Vec<u8>, precision: Precision) {
        let (deg, min, hdths, is_north) = self.dmh();
        let dir = if is_north { b'N' } else { b'S' };
        let blanks = precision.num_blank_digits() as usize;
        // digits: d0 d1 m0 m1 . h0 h1  (6 significant digits, then N/S)
        let mut digits = [0u8; 6];
        let _ = write_digits_6(&mut digits, deg, min, hdths);
        let end = 6usize.saturating_sub(blanks);
        let mut buf = [b' '; 6];
        buf[..end].copy_from_slice(&digits[..end]);
        out.extend_from_slice(&buf[..4]);
        out.push(b'.');
        out.extend_from_slice(&buf[4..6]);
        out.push(dir);
    }

    pub(crate) fn encode_compressed(&self, out: &mut Vec<u8>) {
        let value = (90.0 - self.0) * 380926.0;
        base91_encode4(value.round() as u32, out);
    }
}

/// APRS longitude value in decimal degrees (positive = East, negative = West).
#[derive(Debug, Copy, Clone, PartialOrd, PartialEq, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(transparent))]
pub struct Longitude(f64);

impl Deref for Longitude {
    type Target = f64;
    fn deref(&self) -> &f64 {
        &self.0
    }
}

impl Longitude {
    pub fn new(value: f64) -> Option<Self> {
        if value.is_nan() || !(-180.0..=180.0).contains(&value) {
            None
        } else {
            Some(Self(value))
        }
    }

    pub fn value(self) -> f64 {
        self.0
    }

    /// Decompose into (degrees, whole minutes, hundredths-of-minute, is_east).
    pub fn dmh(self) -> (u32, u32, u32, bool) {
        let (is_east, v) = if self.0 >= 0.0 {
            (true, self.0)
        } else {
            (false, -self.0)
        };
        let deg = v as u32;
        let min = ((v - deg as f64) * 60.0) as u32;
        let mut hdths = ((v - deg as f64 - min as f64 / 60.0) * 6000.0).round() as u32;
        let mut min = min;
        let mut deg = deg;
        if hdths >= 100 {
            hdths = 0;
            min += 1;
        }
        if min >= 60 {
            min = 0;
            deg += 1;
        }
        (deg, min, hdths, is_east)
    }

    /// Parse 9-byte uncompressed longitude: `DDDmm.mmE` or `DDDmm.mmW`.
    /// Uses the latitude's precision to mask low-order digits.
    pub(crate) fn parse_uncompressed(b: &[u8], precision: Precision) -> Result<Self, AprsError> {
        if b.len() != 9 || b[5] != b'.' {
            return Err(AprsError::InvalidLongitude { raw: b.to_vec() });
        }
        let is_east = match b[8] {
            b'E' => true,
            b'W' => false,
            _ => return Err(AprsError::InvalidLongitude { raw: b.to_vec() }),
        };
        // Assemble 7 digit positions: deg(3) min(2) frac(2)
        let mut digits = [0u8; 7];
        digits[0..5].copy_from_slice(&b[0..5]);
        digits[5..7].copy_from_slice(&b[6..8]);
        // Zero out low-order digits according to precision
        let blanks = precision.num_blank_digits() as usize;
        for d in digits.iter_mut().skip(7usize.saturating_sub(blanks)) {
            *d = b'0';
        }
        let deg = parse_bytes::<u32>(&digits[0..3])
            .ok_or_else(|| AprsError::InvalidLongitude { raw: b.to_vec() })?;
        let min = parse_bytes::<u32>(&digits[3..5])
            .ok_or_else(|| AprsError::InvalidLongitude { raw: b.to_vec() })?;
        let hdths = parse_bytes::<u32>(&digits[5..7])
            .ok_or_else(|| AprsError::InvalidLongitude { raw: b.to_vec() })?;
        let value = deg as f64 + min as f64 / 60.0 + hdths as f64 / 6000.0;
        let value = if is_east { value } else { -value };
        Longitude::new(value).ok_or_else(|| AprsError::InvalidLongitude { raw: b.to_vec() })
    }

    /// Parse 4-byte base-91 compressed longitude.
    pub(crate) fn parse_compressed(b: &[u8]) -> Result<Self, AprsError> {
        let enc =
            base91_decode4(b).ok_or_else(|| AprsError::InvalidLongitude { raw: b.to_vec() })?;
        let value = enc / 190463.0 - 180.0;
        Longitude::new(value).ok_or_else(|| AprsError::InvalidLongitude { raw: b.to_vec() })
    }

    pub(crate) fn encode_uncompressed(&self, out: &mut Vec<u8>) {
        let (deg, min, hdths, is_east) = self.dmh();
        let dir = if is_east { b'E' } else { b'W' };
        out.extend_from_slice(
            format!("{:03}{:02}.{:02}{}", deg, min, hdths, dir as char).as_bytes(),
        );
    }

    pub(crate) fn encode_compressed(&self, out: &mut Vec<u8>) {
        let value = (180.0 + self.0) * 190463.0;
        base91_encode4(value.round() as u32, out);
    }
}

// --- base-91 helpers (APRS standard: char 33..=123) ---

pub(crate) fn base91_decode4(b: &[u8]) -> Option<f64> {
    if b.len() < 4 {
        return None;
    }
    let mut val = 0.0f64;
    for &byte in &b[..4] {
        let d = byte.checked_sub(33)?;
        if d > 90 {
            return None;
        }
        val = val * 91.0 + d as f64;
    }
    Some(val)
}

pub(crate) fn base91_encode4(mut val: u32, out: &mut Vec<u8>) {
    let mut buf = [33u8; 4]; // '!' = 33 = base-91 zero
    for i in (0..4).rev() {
        buf[i] = (val % 91) as u8 + 33;
        val /= 91;
    }
    out.extend_from_slice(&buf);
}

pub(crate) fn base91_decode1(b: u8) -> Option<u8> {
    b.checked_sub(33)
}

pub(crate) fn base91_encode1(v: u8) -> u8 {
    v + 33
}

// --- internal helpers ---

/// Parse a 2-byte pair allowing trailing spaces for ambiguity.
/// Returns `(value, num_spaces_found)`.
fn parse_pair_ambiguous(b: &[u8; 2], must_be_spaces: bool) -> Option<(u32, u8)> {
    if must_be_spaces {
        return if b == b"  " { Some((0, 2)) } else { None };
    }
    match (b[0], b[1]) {
        (b' ', b' ') => Some((0, 2)),
        (d, b' ') if d.is_ascii_digit() => Some(((d - b'0') as u32 * 10, 1)),
        (d0, d1) if d0.is_ascii_digit() && d1.is_ascii_digit() => {
            Some(((d0 - b'0') as u32 * 10 + (d1 - b'0') as u32, 0))
        }
        _ => None,
    }
}

/// Format `(deg, min, hdths)` into a 6-byte ASCII digit array without the decimal point.
fn write_digits_6(buf: &mut [u8; 6], deg: u32, min: u32, hdths: u32) -> Option<()> {
    let s = format!("{:02}{:02}{:02}", deg, min, hdths);
    if s.len() != 6 {
        return None;
    }
    buf.copy_from_slice(s.as_bytes());
    Some(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use approx::assert_relative_eq;

    #[test]
    fn lat_uncompressed_basic() {
        let (lat, prec) = Latitude::parse_uncompressed(b"4903.50N").unwrap();
        assert_relative_eq!(lat.value(), 49.05833333333333, epsilon = 1e-9);
        assert_eq!(prec, Precision::HundredthMinute);
    }

    #[test]
    fn lat_uncompressed_south() {
        let (lat, _) = Latitude::parse_uncompressed(b"4903.50S").unwrap();
        assert_relative_eq!(lat.value(), -49.05833333333333, epsilon = 1e-9);
    }

    #[test]
    fn lat_ambiguity_one_tenth() {
        let (lat, prec) = Latitude::parse_uncompressed(b"4903.5 N").unwrap();
        assert_eq!(prec, Precision::TenthMinute);
        assert_relative_eq!(lat.value(), 49.05833333333333, epsilon = 1e-4);
    }

    #[test]
    fn lat_ambiguity_one_minute() {
        let (lat, prec) = Latitude::parse_uncompressed(b"4903.  N").unwrap();
        assert_eq!(prec, Precision::OneMinute);
        assert_relative_eq!(lat.value(), 49.05, epsilon = 1e-4);
    }

    #[test]
    fn lat_invalid_direction() {
        assert!(Latitude::parse_uncompressed(b"4903.50W").is_err());
    }

    #[test]
    fn lat_out_of_range() {
        assert!(Latitude::new(90.1).is_none());
        assert!(Latitude::new(-90.1).is_none());
    }

    #[test]
    fn lon_uncompressed_east() {
        let lon = Longitude::parse_uncompressed(b"07201.75E", Precision::default()).unwrap();
        assert_relative_eq!(lon.value(), 72.02916666666667, epsilon = 1e-9);
    }

    #[test]
    fn lon_uncompressed_west() {
        let lon = Longitude::parse_uncompressed(b"07201.75W", Precision::default()).unwrap();
        assert_relative_eq!(lon.value(), -72.02916666666667, epsilon = 1e-9);
    }

    #[test]
    fn lon_invalid_direction() {
        assert!(Longitude::parse_uncompressed(b"07201.75N", Precision::default()).is_err());
    }

    #[test]
    fn lat_encode_round_trip() {
        let (lat, prec) = Latitude::parse_uncompressed(b"4903.50N").unwrap();
        let mut out = Vec::new();
        lat.encode_uncompressed(&mut out, prec);
        assert_eq!(out, b"4903.50N");
    }

    #[test]
    fn lon_encode_round_trip() {
        let lon = Longitude::parse_uncompressed(b"07201.75W", Precision::default()).unwrap();
        let mut out = Vec::new();
        lon.encode_uncompressed(&mut out);
        assert_eq!(out, b"07201.75W");
    }

    #[test]
    fn compressed_lat_round_trip() {
        let original = Latitude::new(49.05833).unwrap();
        let mut enc = Vec::new();
        original.encode_compressed(&mut enc);
        assert_eq!(enc.len(), 4);
        let decoded = Latitude::parse_compressed(&enc).unwrap();
        assert_relative_eq!(decoded.value(), original.value(), epsilon = 0.001);
    }

    #[test]
    fn compressed_lon_round_trip() {
        let original = Longitude::new(-72.029).unwrap();
        let mut enc = Vec::new();
        original.encode_compressed(&mut enc);
        assert_eq!(enc.len(), 4);
        let decoded = Longitude::parse_compressed(&enc).unwrap();
        assert_relative_eq!(decoded.value(), original.value(), epsilon = 0.001);
    }

    #[test]
    fn base91_decode4_known() {
        // From aprs-parser-rs test: "#$%^" => 1532410.0
        let val = base91_decode4(b"#$%^").unwrap();
        assert_relative_eq!(val, 1532410.0, epsilon = 0.5);
    }
}
