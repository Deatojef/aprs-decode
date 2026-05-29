use crate::error::AprsError;
use crate::types::{Extension, Position, Timestamp};
use crate::weather::AprsWeatherData;

/// A decoded APRS position report.
///
/// Covers DTI bytes `!` `=` `/` `@`:
/// - `!` = no timestamp, messaging not supported
/// - `=` = no timestamp, messaging supported
/// - `/` = timestamp present, messaging not supported
/// - `@` = timestamp present, messaging supported
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct AprsPosition {
    pub timestamp: Option<Timestamp>,
    pub messaging_supported: bool,
    pub position: Position,
    /// Data extension (course/speed, PHG, RNG, DFS) if one was found in the comment.
    pub extension: Option<Extension>,
    /// Weather data, populated when the symbol is `/_` (weather station).
    pub weather: Option<AprsWeatherData>,
    /// Frequency in MHz extracted from the start of the comment field, if present.
    /// The raw comment is preserved in full for round-trip fidelity.
    pub frequency_mhz: Option<f32>,
    pub comment: Vec<u8>,
}

impl AprsPosition {
    /// Decode the information field of a position report (including the leading DTI byte).
    pub(crate) fn parse(info: &[u8]) -> Result<Self, AprsError> {
        let dti = *info.first().ok_or(AprsError::EmptyPacket)?;
        let messaging_supported = dti == b'=' || dti == b'@';
        let has_timestamp = dti == b'@' || dti == b'/';

        let (b, timestamp) = if has_timestamp {
            let ts_bytes = info.get(1..8).ok_or(AprsError::TruncatedPacket {
                expected: 8,
                got: info.len(),
            })?;
            (
                info.get(8..).unwrap_or_default(),
                Some(Timestamp::parse(ts_bytes)?),
            )
        } else {
            (info.get(1..).unwrap_or_default(), None)
        };

        let (remaining, position) = Position::parse(b)?;
        let comment = remaining.unwrap_or_default().to_vec();

        // Try to parse a data extension from the comment — failure is silently ignored
        let extension = Extension::parse(&comment);

        // Parse weather data when this is a weather-station position (symbol `/_`).
        // The comment always starts with the DDD/SSS wind dir/speed block, so we
        // always start from byte 0 regardless of whether an extension was also parsed.
        let weather = if position.symbol.table == '/' && position.symbol.code == '_' {
            crate::weather::AprsWeatherData::parse(&comment).ok()
        } else {
            None
        };

        // Extract frequency from the start of the comment (e.g. `146.520MHz T100`).
        let frequency_mhz = crate::util::extract_frequency_mhz(&comment);

        Ok(Self {
            timestamp,
            messaging_supported,
            position,
            extension,
            weather,
            frequency_mhz,
            comment,
        })
    }

    /// Encode this position report back to its information-field wire bytes.
    pub fn encode(&self) -> Vec<u8> {
        let mut out = Vec::new();
        let dti = match (self.timestamp.is_some(), self.messaging_supported) {
            (true, true) => b'@',
            (true, false) => b'/',
            (false, true) => b'=',
            (false, false) => b'!',
        };
        out.push(dti);

        if let Some(ref ts) = self.timestamp {
            ts.encode(&mut out);
        }

        if self.position.compressed_cs.is_some() {
            self.position.encode_compressed(&mut out);
        } else {
            self.position.encode_uncompressed(&mut out);
        }

        out.extend_from_slice(&self.comment);
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use approx::assert_relative_eq;

    #[test]
    fn no_timestamp_no_messaging() {
        let pos = AprsPosition::parse(b"!4903.50N/07201.75W-").unwrap();
        assert!(pos.timestamp.is_none());
        assert!(!pos.messaging_supported);
        assert_relative_eq!(
            pos.position.latitude.value(),
            49.05833333333333,
            epsilon = 1e-9
        );
        assert_relative_eq!(
            pos.position.longitude.value(),
            -72.02916666666667,
            epsilon = 1e-9
        );
        assert_eq!(pos.comment, b"");
    }

    #[test]
    fn no_timestamp_with_messaging() {
        let pos = AprsPosition::parse(b"=4903.50N/07201.75W-").unwrap();
        assert!(pos.timestamp.is_none());
        assert!(pos.messaging_supported);
    }

    #[test]
    fn with_timestamp_no_messaging() {
        let pos = AprsPosition::parse(b"/074849h4821.61N\\01224.49E^322/103/A=003054").unwrap();
        assert_eq!(pos.timestamp.unwrap(), Timestamp::Hhmmss(7, 48, 49));
        assert!(!pos.messaging_supported);
        assert_relative_eq!(
            pos.position.latitude.value(),
            48.36016666666667,
            epsilon = 1e-9
        );
        assert_relative_eq!(
            pos.position.longitude.value(),
            12.408166666666666,
            epsilon = 1e-9
        );
        assert_eq!(pos.position.symbol.table, '\\');
        assert_eq!(pos.position.symbol.code, '^');
        assert_eq!(pos.comment, b"322/103/A=003054");
    }

    #[test]
    fn with_timestamp_and_messaging() {
        let pos = AprsPosition::parse(b"@074849h4821.61N\\01224.49E^322/103/A=003054").unwrap();
        assert!(pos.timestamp.is_some());
        assert!(pos.messaging_supported);
    }

    #[test]
    fn with_comment_and_altitude() {
        let pos = AprsPosition::parse(b"!4903.50N/07201.75W-Hello/A=001000").unwrap();
        assert_eq!(pos.comment, b"Hello/A=001000");
        assert!(pos.position.altitude.is_some());
    }

    #[test]
    fn extension_course_speed_parsed() {
        let pos = AprsPosition::parse(b"/074849h4821.61N\\01224.49E^322/103/A=003054").unwrap();
        assert!(pos.extension.is_some());
        assert!(matches!(
            pos.extension.unwrap(),
            Extension::DirectionSpeed {
                direction_degrees: 322,
                speed_knots: 103
            }
        ));
    }

    #[test]
    fn compressed_no_timestamp() {
        let pos = AprsPosition::parse(b"!/ABCD#$%^- sT").unwrap();
        assert!(pos.timestamp.is_none());
        assert_relative_eq!(
            pos.position.latitude.value(),
            25.97004667573229,
            epsilon = 0.001
        );
        assert_relative_eq!(
            pos.position.longitude.value(),
            -171.95429033460567,
            epsilon = 0.001
        );
    }

    #[test]
    fn encode_round_trip_uncompressed() {
        let raw = b"!4903.50N/07201.75W-";
        let pos = AprsPosition::parse(raw).unwrap();
        let encoded = pos.encode();
        assert_eq!(&encoded, raw);
    }

    #[test]
    fn encode_round_trip_with_timestamp() {
        let raw = b"/074849h4821.61N\\01224.49E^322/103/A=003054";
        let pos = AprsPosition::parse(raw).unwrap();
        let encoded = pos.encode();
        assert_eq!(encoded, raw);
    }

    #[test]
    fn encode_round_trip_compressed() {
        let raw = b"!/ABCD#$%^- sT";
        let pos = AprsPosition::parse(raw).unwrap();
        let encoded = pos.encode();
        assert_eq!(&encoded, raw);
    }

    #[test]
    fn timestamp_validates_strictly() {
        // Hour 24 is invalid — aprs-parser-rs would have accepted this
        let err = AprsPosition::parse(b"/092460z4903.50N/07201.75W-");
        assert!(err.is_err());
    }
}
