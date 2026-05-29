use crate::error::AprsError;
use crate::util::parse_bytes;

/// Antenna directivity.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum Directivity {
    Omni,
    /// Direction in degrees (multiples of 45°).
    Degrees(u16),
}

impl Directivity {
    fn from_digit(d: u8) -> Option<Self> {
        if d == 0 {
            return Some(Directivity::Omni);
        }
        if d < 9 {
            return Some(Directivity::Degrees(d as u16 * 45));
        }
        None
    }

    fn as_digit(&self) -> u8 {
        match self {
            Directivity::Omni => 0,
            Directivity::Degrees(deg) => ((deg % 360) / 45) as u8,
        }
    }
}

/// A data extension field that follows the position in the comment field.
///
/// Extensions occupy exactly 7 bytes and are encoded in a fixed format that
/// depends on the extension type. Unknown or malformed extensions are discarded
/// (the caller treats the entire comment field as the comment in that case).
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum Extension {
    /// Course (degrees) and speed (knots). Format: `DDD/SSS`.
    DirectionSpeed {
        direction_degrees: u16,
        speed_knots: u16,
    },
    /// Power-Height-Gain-Directivity. Format: `PHGphgd`.
    Phg {
        power_watts: u32,
        antenna_height_feet: u32,
        antenna_gain_db: u8,
        directivity: Directivity,
    },
    /// Pre-calculated radio range. Format: `RNGrrrr`.
    Rng { range_miles: u16 },
    /// DF Strength-Height-Gain-Directivity. Format: `DFSshgd`.
    Dfs {
        s_points: u8,
        antenna_height_feet: u32,
        antenna_gain_db: u8,
        directivity: Directivity,
    },
}

impl Extension {
    /// Attempt to parse the first 7 bytes of `data` as an extension field.
    /// Returns `None` if unrecognized or malformed.
    pub fn parse(data: &[u8]) -> Option<Self> {
        if data.len() < 7 {
            return None;
        }
        let b = &data[..7];

        // Course/Speed: `DDD/SSS` — three digits, slash, three digits
        if b[3] == b'/'
            && b[..3].iter().all(|c| c.is_ascii_digit())
            && b[4..7].iter().all(|c| c.is_ascii_digit())
        {
            let dir: u16 = parse_bytes(&b[0..3])?;
            let spd: u16 = parse_bytes(&b[4..7])?;
            // direction 000 means unknown/not applicable (valid)
            return Some(Extension::DirectionSpeed {
                direction_degrees: dir,
                speed_knots: spd,
            });
        }

        // PHG: `PHGphgd`
        if b.starts_with(b"PHG")
            && b[3].is_ascii_digit()
            && b[4].is_ascii_digit()
            && b[5].is_ascii_digit()
            && b[6].is_ascii_digit()
        {
            let p = b[3] - b'0';
            let h = b[4]; // encoded as ASCII
            let g = b[5] - b'0';
            let d = b[6] - b'0';
            let power_watts = (p as u32) * (p as u32);
            let antenna_height_feet = 10 * (1u32 << (h.saturating_sub(48) as u32));
            let directivity = Directivity::from_digit(d)?;
            return Some(Extension::Phg {
                power_watts,
                antenna_height_feet,
                antenna_gain_db: g,
                directivity,
            });
        }

        // RNG: `RNGrrrr`
        if b.starts_with(b"RNG") && b[3..7].iter().all(|c| c.is_ascii_digit()) {
            let range: u16 = parse_bytes(&b[3..7])?;
            return Some(Extension::Rng { range_miles: range });
        }

        // DFS: `DFSshgd`
        if b.starts_with(b"DFS")
            && b[3].is_ascii_digit()
            && b[4].is_ascii_digit()
            && b[5].is_ascii_digit()
            && b[6].is_ascii_digit()
        {
            let s = b[3] - b'0';
            let h = b[4];
            let g = b[5] - b'0';
            let d = b[6] - b'0';
            let antenna_height_feet = 10 * (1u32 << (h.saturating_sub(48) as u32));
            let directivity = Directivity::from_digit(d)?;
            return Some(Extension::Dfs {
                s_points: s,
                antenna_gain_db: g,
                antenna_height_feet,
                directivity,
            });
        }

        None
    }

    /// Encode the extension field as exactly 7 bytes into `out`.
    pub fn encode(&self, out: &mut Vec<u8>) {
        match self {
            Extension::DirectionSpeed {
                direction_degrees,
                speed_knots,
            } => {
                out.extend_from_slice(
                    format!("{:03}/{:03}", direction_degrees, speed_knots).as_bytes(),
                );
            }
            Extension::Phg {
                power_watts,
                antenna_height_feet,
                antenna_gain_db,
                directivity,
            } => {
                let p = (*power_watts as f64).sqrt() as u8;
                let h_log = if *antenna_height_feet >= 10 {
                    ((*antenna_height_feet / 10) as f64).log2() as u8 + 48
                } else {
                    48
                };
                out.extend_from_slice(b"PHG");
                out.push(p + b'0');
                out.push(h_log);
                out.push(antenna_gain_db + b'0');
                out.push(directivity.as_digit() + b'0');
            }
            Extension::Rng { range_miles } => {
                out.extend_from_slice(format!("RNG{:04}", range_miles).as_bytes());
            }
            Extension::Dfs {
                s_points,
                antenna_height_feet,
                antenna_gain_db,
                directivity,
            } => {
                let h_log = if *antenna_height_feet >= 10 {
                    ((*antenna_height_feet / 10) as f64).log2() as u8 + 48
                } else {
                    48
                };
                out.extend_from_slice(b"DFS");
                out.push(s_points + b'0');
                out.push(h_log);
                out.push(antenna_gain_db + b'0');
                out.push(directivity.as_digit() + b'0');
            }
        }
    }

    /// Returns an error if parsing fails where one was expected.
    pub fn require(data: &[u8]) -> Result<Self, AprsError> {
        Self::parse(data).ok_or(AprsError::UnsupportedPositionFormat)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn direction_speed() {
        let ext = Extension::parse(b"322/103").unwrap();
        assert!(matches!(
            ext,
            Extension::DirectionSpeed {
                direction_degrees: 322,
                speed_knots: 103
            }
        ));
    }

    #[test]
    fn direction_speed_encode_round_trip() {
        let ext = Extension::DirectionSpeed {
            direction_degrees: 322,
            speed_knots: 103,
        };
        let mut out = Vec::new();
        ext.encode(&mut out);
        assert_eq!(out, b"322/103");
        assert_eq!(Extension::parse(&out).unwrap(), ext);
    }

    #[test]
    fn rng_parse() {
        let ext = Extension::parse(b"RNG0050").unwrap();
        assert!(matches!(ext, Extension::Rng { range_miles: 50 }));
    }

    #[test]
    fn too_short_returns_none() {
        assert!(Extension::parse(b"12/1").is_none());
    }

    #[test]
    fn phg_valid() {
        let ext = Extension::parse(b"PHG5132").unwrap();
        assert!(matches!(ext, Extension::Phg { .. }));
    }

    #[test]
    fn phg_nondigit_height_returns_none() {
        // `z` in the height position previously caused a shift-left overflow panic.
        // It must now be rejected cleanly.
        assert!(Extension::parse(b"PHG0z00").is_none());
    }

    #[test]
    fn dfs_nondigit_height_returns_none() {
        assert!(Extension::parse(b"DFS0z00").is_none());
    }

    #[test]
    fn phg_max_digit_height_no_panic() {
        // Height digit 9 is the largest valid value (10 * 2^9 = 5120 ft).
        let ext = Extension::parse(b"PHG0900").unwrap();
        assert!(matches!(
            ext,
            Extension::Phg {
                antenna_height_feet: 5120,
                ..
            }
        ));
    }
}
