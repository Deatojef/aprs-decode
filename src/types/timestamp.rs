use crate::error::AprsError;
use crate::util::parse_bytes;

/// An APRS timestamp parsed from the packet header.
///
/// APRS101 defines three formats; the local-time format (`/`) is represented
/// as `Unsupported` since it is deprecated and ambiguous.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum Timestamp {
    /// Day-of-month (1–31), Hour (0–23), Minute (0–59) in UTC. Suffix `z`.
    Ddhhmm(u8, u8, u8),
    /// Hour (0–23), Minute (0–59), Second (0–59) in UTC. Suffix `h`.
    Hhmmss(u8, u8, u8),
    /// Local-time format (deprecated, suffix `/`). Stored as raw bytes.
    Unsupported(Vec<u8>),
}

impl Timestamp {
    /// Parse a 7-byte timestamp field.
    pub fn parse(b: &[u8]) -> Result<Self, AprsError> {
        if b.len() != 7 {
            return Err(AprsError::InvalidTimestampFormat { raw: b.to_vec() });
        }
        if b[6] == b'/' {
            return Ok(Timestamp::Unsupported(b.to_vec()));
        }
        let f1: u8 = parse_bytes(&b[0..2])
            .ok_or_else(|| AprsError::InvalidTimestampFormat { raw: b.to_vec() })?;
        let f2: u8 = parse_bytes(&b[2..4])
            .ok_or_else(|| AprsError::InvalidTimestampFormat { raw: b.to_vec() })?;
        let f3: u8 = parse_bytes(&b[4..6])
            .ok_or_else(|| AprsError::InvalidTimestampFormat { raw: b.to_vec() })?;

        match b[6] {
            b'z' | b'Z' => {
                validate_ddhhmm(f1, f2, f3, b)?;
                Ok(Timestamp::Ddhhmm(f1, f2, f3))
            }
            b'h' | b'H' => {
                validate_hhmmss(f1, f2, f3, b)?;
                Ok(Timestamp::Hhmmss(f1, f2, f3))
            }
            // Some trackers (e.g. certain MFJ/APMI firmwares) emit a non-standard
            // designator such as `#` in the 7th byte. The `@`/`/` DTI guarantees the
            // first 7 bytes are the timestamp field, so preserve the raw bytes rather
            // than failing the whole packet — the position that follows is well-formed.
            _ => Ok(Timestamp::Unsupported(b.to_vec())),
        }
    }

    /// Write the timestamp in its original wire format.
    pub fn encode(&self, out: &mut Vec<u8>) {
        match self {
            Timestamp::Ddhhmm(d, h, m) => {
                out.extend_from_slice(format!("{:02}{:02}{:02}z", d, h, m).as_bytes());
            }
            Timestamp::Hhmmss(h, m, s) => {
                out.extend_from_slice(format!("{:02}{:02}{:02}h", h, m, s).as_bytes());
            }
            Timestamp::Unsupported(raw) => out.extend_from_slice(raw),
        }
    }
}

fn validate_ddhhmm(day: u8, hour: u8, minute: u8, raw: &[u8]) -> Result<(), AprsError> {
    if day == 0 || day > 31 {
        return Err(AprsError::TimestampDayOutOfRange { day });
    }
    if hour > 23 {
        return Err(AprsError::TimestampHourOutOfRange { hour });
    }
    if minute > 59 {
        return Err(AprsError::TimestampMinuteOutOfRange { minute });
    }
    let _ = raw; // suppress unused warning
    Ok(())
}

fn validate_hhmmss(hour: u8, minute: u8, second: u8, raw: &[u8]) -> Result<(), AprsError> {
    if hour > 23 {
        return Err(AprsError::TimestampHourOutOfRange { hour });
    }
    if minute > 59 {
        return Err(AprsError::TimestampMinuteOutOfRange { minute });
    }
    if second > 59 {
        return Err(AprsError::TimestampSecondOutOfRange { second });
    }
    let _ = raw;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_ddhhmm() {
        assert_eq!(
            Timestamp::parse(b"092345z").unwrap(),
            Timestamp::Ddhhmm(9, 23, 45)
        );
    }

    #[test]
    fn parse_hhmmss() {
        assert_eq!(
            Timestamp::parse(b"074849h").unwrap(),
            Timestamp::Hhmmss(7, 48, 49)
        );
    }

    #[test]
    fn parse_local_unsupported() {
        assert!(matches!(
            Timestamp::parse(b"092345/").unwrap(),
            Timestamp::Unsupported(_)
        ));
    }

    #[test]
    fn parse_nonstandard_designator_preserved() {
        // A `#` designator (seen from some trackers) is preserved raw rather than
        // failing, so the well-formed position that follows can still be parsed.
        let ts = Timestamp::parse(b"291500#").unwrap();
        assert!(matches!(ts, Timestamp::Unsupported(_)));
        let mut out = Vec::new();
        ts.encode(&mut out);
        assert_eq!(out, b"291500#");
    }

    #[test]
    fn day_zero_invalid() {
        assert!(Timestamp::parse(b"002345z").is_err());
    }

    #[test]
    fn day_32_invalid() {
        assert!(Timestamp::parse(b"322345z").is_err());
    }

    #[test]
    fn hour_24_invalid() {
        assert!(Timestamp::parse(b"092445z").is_err());
    }

    #[test]
    fn minute_60_invalid() {
        assert!(Timestamp::parse(b"092360z").is_err());
    }

    #[test]
    fn second_60_invalid() {
        assert!(Timestamp::parse(b"074860h").is_err());
    }

    #[test]
    fn encode_round_trip_ddhhmm() {
        let ts = Timestamp::Ddhhmm(9, 23, 45);
        let mut out = Vec::new();
        ts.encode(&mut out);
        assert_eq!(Timestamp::parse(&out).unwrap(), ts);
    }

    #[test]
    fn encode_round_trip_hhmmss() {
        let ts = Timestamp::Hhmmss(7, 48, 49);
        let mut out = Vec::new();
        ts.encode(&mut out);
        assert_eq!(Timestamp::parse(&out).unwrap(), ts);
    }
}
