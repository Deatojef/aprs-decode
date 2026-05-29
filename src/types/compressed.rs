use crate::error::AprsError;
use crate::types::lonlat::{base91_decode1, base91_encode1};

// --- Compression type byte (T byte) ---

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum GpsFix {
    Old,
    Current,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum NmeaSource {
    Other,
    Gll,
    Gga,
    Rmc,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum Origin {
    Compressed,
    TncBText,
    Software,
    Tbd,
    Kpc3,
    Pico,
    Other,
    Digipeater,
}

/// The compression-type byte (T byte) following the csT block in a compressed position.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct CompressionType {
    pub gps_fix: GpsFix,
    pub nmea_source: NmeaSource,
    pub origin: Origin,
}

impl From<u8> for CompressionType {
    fn from(b: u8) -> Self {
        let gps_fix = if b & (1 << 5) != 0 { GpsFix::Current } else { GpsFix::Old };
        let nmea_source = match (b & (1 << 4) != 0, b & (1 << 3) != 0) {
            (false, false) => NmeaSource::Other,
            (false, true) => NmeaSource::Gll,
            (true, false) => NmeaSource::Gga,
            (true, true) => NmeaSource::Rmc,
        };
        let origin = match (b & (1 << 2) != 0, b & (1 << 1) != 0, b & 1 != 0) {
            (false, false, false) => Origin::Compressed,
            (false, false, true) => Origin::TncBText,
            (false, true, false) => Origin::Software,
            (false, true, true) => Origin::Tbd,
            (true, false, false) => Origin::Kpc3,
            (true, false, true) => Origin::Pico,
            (true, true, false) => Origin::Other,
            (true, true, true) => Origin::Digipeater,
        };
        Self { gps_fix, nmea_source, origin }
    }
}

impl From<CompressionType> for u8 {
    fn from(t: CompressionType) -> u8 {
        let b5 = t.gps_fix == GpsFix::Current;
        let (b4, b3) = match t.nmea_source {
            NmeaSource::Other => (false, false),
            NmeaSource::Gll => (false, true),
            NmeaSource::Gga => (true, false),
            NmeaSource::Rmc => (true, true),
        };
        let (b2, b1, b0) = match t.origin {
            Origin::Compressed => (false, false, false),
            Origin::TncBText => (false, false, true),
            Origin::Software => (false, true, false),
            Origin::Tbd => (false, true, true),
            Origin::Kpc3 => (true, false, false),
            Origin::Pico => (true, false, true),
            Origin::Other => (true, true, false),
            Origin::Digipeater => (true, true, true),
        };
        (b5 as u8) << 5
            | (b4 as u8) << 4
            | (b3 as u8) << 3
            | (b2 as u8) << 2
            | (b1 as u8) << 1
            | (b0 as u8)
    }
}

// --- Course/Speed encoded as 2 base-91 bytes ---

#[derive(Debug, Copy, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct CourseSpeed {
    pub course_degrees: u16,
    pub speed_knots: f64,
}

impl CourseSpeed {
    pub fn new(course_degrees: u16, speed_knots: f64) -> Self {
        Self { course_degrees, speed_knots }
    }

    pub(crate) fn from_cs(c: u8, s: u8) -> Self {
        Self {
            course_degrees: c as u16 * 4,
            speed_knots: 1.08_f64.powi(s as i32) - 1.0,
        }
    }

    pub(crate) fn to_cs(self) -> (u8, u8) {
        let c = (self.course_degrees / 4) as u8;
        let s = ((self.speed_knots + 1.0).ln() / 1.08_f64.ln()).round() as u8;
        (c, s)
    }
}

// --- Radio range encoded as 1 base-91 byte ---

#[derive(Debug, Copy, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct RadioRange {
    pub range_miles: f64,
}

impl RadioRange {
    pub fn new(range_miles: f64) -> Self {
        Self { range_miles }
    }

    pub(crate) fn from_s(s: u8) -> Self {
        Self { range_miles: 2.0 * 1.08_f64.powi(s as i32) }
    }

    pub(crate) fn to_s(self) -> u8 {
        ((self.range_miles / 2.0).ln() / 1.08_f64.ln()).round() as u8
    }
}

// --- Compressed altitude (2 base-91 bytes via GGA) ---

#[derive(Debug, Copy, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct CompressedAltitude {
    pub feet: f64,
}

impl CompressedAltitude {
    pub fn new(feet: f64) -> Self {
        Self { feet }
    }

    pub fn meters(self) -> f64 {
        self.feet * 0.3048
    }

    pub(crate) fn from_cs(c: u8, s: u8) -> Self {
        Self { feet: 1.002_f64.powi(c as i32 * 91 + s as i32) }
    }

    pub(crate) fn to_cs(self) -> (u8, u8) {
        let v = (self.feet.ln() / 1.002_f64.ln()).round() as i32;
        ((v / 91) as u8, (v % 91) as u8)
    }
}

// --- /A=NNNNNN altitude in comment field ---

#[derive(Debug, Copy, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Altitude {
    pub feet: f64,
}

impl Altitude {
    pub fn new(feet: f64) -> Self {
        Self { feet }
    }

    pub fn meters(self) -> f64 {
        self.feet * 0.3048
    }
}

// --- Compressed csT block ---

/// The csT block in a compressed position, which encodes course/speed, radio range, or altitude.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum CompressedCs {
    CourseSpeed(CourseSpeed, CompressionType),
    RadioRange(RadioRange, CompressionType),
    Altitude(CompressedAltitude, CompressionType),
    /// Space character: csT is present but carries no data.
    None(CompressionType),
}

impl CompressedCs {
    /// Parse the 3-byte csT block (c byte, s byte, t byte).
    pub(crate) fn parse(c: u8, s: u8, t_raw: u8) -> Result<Self, AprsError> {
        // When c is space, or any byte is out of the valid base-91 / T-byte range
        // (seen in null-position packets that use '@' or space placeholders), fall
        // back to None rather than rejecting the whole packet.
        if c == b' ' {
            let t_val = t_raw.checked_sub(33).unwrap_or(0);
            return Ok(CompressedCs::None(CompressionType::from(t_val)));
        }
        let c_val = match base91_decode1(c) {
            Some(v) => v,
            None => return Ok(CompressedCs::None(CompressionType::from(0))),
        };
        let s_val = match base91_decode1(s) {
            Some(v) => v,
            None => return Ok(CompressedCs::None(CompressionType::from(0))),
        };
        let t = CompressionType::from(t_raw.checked_sub(33).unwrap_or(0));

        let cs = if t.nmea_source == NmeaSource::Gga {
            CompressedCs::Altitude(CompressedAltitude::from_cs(c_val, s_val), t)
        } else {
            match c_val {
                0..=89 => CompressedCs::CourseSpeed(CourseSpeed::from_cs(c_val, s_val), t),
                90 => CompressedCs::RadioRange(RadioRange::from_s(s_val), t),
                _ => return Err(AprsError::InvalidCompressedByte { byte: c }),
            }
        };
        Ok(cs)
    }

    pub(crate) fn encode(&self, out: &mut Vec<u8>) {
        match self {
            CompressedCs::CourseSpeed(cs, t) => {
                let (c, s) = cs.to_cs();
                out.push(base91_encode1(c));
                out.push(base91_encode1(s));
                out.push(u8::from(*t) + 33);
            }
            CompressedCs::RadioRange(rr, t) => {
                out.push(b'{');
                out.push(base91_encode1(rr.to_s()));
                out.push(u8::from(*t) + 33);
            }
            CompressedCs::Altitude(alt, t) => {
                let (c, s) = alt.to_cs();
                out.push(base91_encode1(c));
                out.push(base91_encode1(s));
                out.push(u8::from(*t) + 33);
            }
            CompressedCs::None(t) => {
                out.push(b' ');
                // 's' byte and T byte follow; use 'T' placeholder encoding
                out.push(b's');
                out.push(u8::from(*t) + 33);
            }
        }
    }

    pub fn compression_type(&self) -> CompressionType {
        match self {
            CompressedCs::CourseSpeed(_, t) => *t,
            CompressedCs::RadioRange(_, t) => *t,
            CompressedCs::Altitude(_, t) => *t,
            CompressedCs::None(t) => *t,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compression_type_round_trip() {
        let t = CompressionType {
            gps_fix: GpsFix::Current,
            nmea_source: NmeaSource::Gga,
            origin: Origin::Software,
        };
        let byte = u8::from(t);
        assert_eq!(CompressionType::from(byte), t);
    }

    #[test]
    fn course_speed_round_trip() {
        for c in 0u8..90 {
            for s in 0u8..91 {
                let cs = CourseSpeed::from_cs(c, s);
                assert_eq!(cs.to_cs(), (c, s));
            }
        }
    }

    #[test]
    fn radio_range_round_trip() {
        for s in 0u8..91 {
            let rr = RadioRange::from_s(s);
            assert_eq!(rr.to_s(), s);
        }
    }

    #[test]
    fn altitude_round_trip() {
        for c in 0u8..91 {
            for s in 0u8..91 {
                let alt = CompressedAltitude::from_cs(c, s);
                assert_eq!(alt.to_cs(), (c, s));
            }
        }
    }
}
