use crate::error::AprsError;
use crate::types::{Extension, Position, Timestamp};
use crate::util::trim_spaces_end;

/// An APRS Object Report.
///
/// DTI: `;`
///
/// Objects have a fixed 9-character name, a mandatory timestamp, and a position.
/// They are used to report the location of things other than the sending station
/// (storms, marathons, spacecraft, etc.).
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct AprsObject {
    /// Object name, trimmed of trailing spaces (original APRS format pads to 9 chars).
    pub name: Vec<u8>,
    /// `true` if the object is live/active; `false` if it has been killed/removed.
    pub live: bool,
    /// Mandatory timestamp.
    pub timestamp: Timestamp,
    pub position: Position,
    /// Optional data extension (course/speed, PHG, RNG, DFS) from the comment field.
    pub extension: Option<Extension>,
    /// Frequency in MHz extracted from the comment field, if present.
    pub frequency_mhz: Option<f32>,
    pub comment: Vec<u8>,
}

impl AprsObject {
    /// Decode from the information field (including the leading `;` DTI byte).
    pub(crate) fn parse(info: &[u8]) -> Result<Self, AprsError> {
        // Format: ;NNNNNNNNN*DDHHMMzLatSymLonSymComment
        //          ^ byte 0 = DTI ';'
        //           123456789  = 9-byte name (bytes 1-9)
        //                    ^ byte 10 = liveness (* or _)
        //                     1234567 = 7-byte timestamp (bytes 11-17)
        //                            ...position + comment
        if info.len() < 18 {
            return Err(AprsError::InvalidObject { detail: "packet too short" });
        }

        let mut name = info[1..10].to_vec();
        trim_spaces_end(&mut name);

        let live = match info[10] {
            b'*' => true,
            b'_' | b' ' => false, // spec says `_`, space is a common variant
            _ => return Err(AprsError::InvalidObject { detail: "invalid liveness byte" }),
        };

        let timestamp = Timestamp::parse(&info[11..18])?;

        let position_bytes = info.get(18..)
            .ok_or(AprsError::InvalidObject { detail: "truncated after timestamp" })?;

        let (remaining, position) = Position::parse(position_bytes)?;
        let comment_raw = remaining.unwrap_or_default();

        let (extension, comment) = if position.compressed_cs.is_none() {
            // For uncompressed positions, try to parse an extension from the comment
            if let Some(ext) = Extension::parse(comment_raw) {
                (Some(ext), comment_raw.get(7..).unwrap_or_default().to_vec())
            } else {
                (None, comment_raw.to_vec())
            }
        } else {
            (None, comment_raw.to_vec())
        };

        let frequency_mhz = crate::util::extract_frequency_mhz(&comment);
        Ok(Self { name, live, timestamp, position, extension, frequency_mhz, comment })
    }

    pub fn encode(&self) -> Vec<u8> {
        let mut out = vec![b';'];
        out.extend_from_slice(&self.name);
        out.extend(std::iter::repeat_n(b' ', 9usize.saturating_sub(self.name.len())));
        out.push(if self.live { b'*' } else { b'_' });
        self.timestamp.encode(&mut out);

        if self.extension.is_some() || self.position.compressed_cs.is_none() {
            self.position.encode_uncompressed(&mut out);
            if let Some(ref ext) = self.extension {
                ext.encode(&mut out);
            }
        } else {
            self.position.encode_compressed(&mut out);
        }

        out.extend_from_slice(&self.comment);
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use approx::assert_relative_eq;

    const LIVE_OBJ: &[u8] =
        b";HFEST-18H*170403z3443.55N\\08635.47Wh146.940MHz T100 Huntsville Hamfest";

    #[test]
    fn parse_live_object() {
        let o = AprsObject::parse(LIVE_OBJ).unwrap();
        assert_eq!(o.name, b"HFEST-18H");
        assert!(o.live);
        assert_eq!(o.timestamp, Timestamp::Ddhhmm(17, 4, 3));
        assert_relative_eq!(o.position.latitude.value(), 34.725833333333334, epsilon = 1e-9);
        assert_relative_eq!(o.position.longitude.value(), -86.59116666666667, epsilon = 1e-9);
        assert_eq!(o.position.symbol.table, '\\');
        assert_eq!(o.position.symbol.code, 'h');
        assert_eq!(o.comment, b"146.940MHz T100 Huntsville Hamfest");
    }

    #[test]
    fn parse_dead_object_space_liveness() {
        // Some encoders use space instead of _ for killed; accept both
        let o = AprsObject::parse(b";HFEST     170403z3443.55N\\08635.47Wh").unwrap();
        assert_eq!(o.name, b"HFEST");
        assert!(!o.live);
    }

    #[test]
    fn parse_dead_object_underscore_liveness() {
        let o = AprsObject::parse(b";HFEST    _170403z3443.55N\\08635.47Wh").unwrap();
        assert_eq!(o.name, b"HFEST");
        assert!(!o.live);
    }

    #[test]
    fn parse_with_extension() {
        let o = AprsObject::parse(
            b";HFEST    _170403z3443.55N\\08635.47WhPHG5132Comment"
        ).unwrap();
        assert!(o.extension.is_some());
    }

    #[test]
    fn parse_compressed_object() {
        let o = AprsObject::parse(
            b";CAR      _092345z/5L!!<*e7>7P[Moving to the north"
        ).unwrap();
        assert_eq!(o.name, b"CAR");
        assert!(!o.live);
        assert_relative_eq!(o.position.latitude.value(), 49.5, epsilon = 0.01);
        assert_eq!(o.comment, b"Moving to the north");
    }

    #[test]
    fn encode_round_trip_live() {
        // Our encoder uses `_` for dead and `*` for live per spec
        let o = AprsObject::parse(LIVE_OBJ).unwrap();
        assert_eq!(o.encode(), LIVE_OBJ);
    }

    #[test]
    fn timestamp_validates_strictly() {
        // Day 0 is invalid
        assert!(AprsObject::parse(
            b";HFEST-18H*002345z3443.55N\\08635.47Wh"
        ).is_err());
    }
}
