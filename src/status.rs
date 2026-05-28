use crate::error::AprsError;
use crate::types::Timestamp;

/// An APRS Status Report.
///
/// DTI: `>`
///
/// Announces the station's current mission or status as a single free-text line.
/// APRS101 restricts the timestamp to DDHHMM format, but HHMMSS is also seen in practice.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct AprsStatus {
    /// Optional timestamp. The spec allows only `DDHHMM`; `HHMMSS` is a common extension.
    pub timestamp: Option<Timestamp>,
    pub comment: Vec<u8>,
}

impl AprsStatus {
    /// Decode from the information field (including the leading `>` DTI byte).
    pub(crate) fn parse(info: &[u8]) -> Result<Self, AprsError> {
        // Strip the leading `>` DTI
        let b = info.get(1..).unwrap_or_default();

        // Opportunistically try to parse the first 7 bytes as a timestamp.
        // On failure (invalid or too short) the whole field is the comment.
        let timestamp = b.get(..7).and_then(|ts| Timestamp::parse(ts).ok());
        let comment = if timestamp.is_some() {
            b.get(7..).unwrap_or_default().to_vec()
        } else {
            b.to_vec()
        };

        Ok(Self { timestamp, comment })
    }

    pub fn encode(&self) -> Vec<u8> {
        let mut out = vec![b'>'];
        if let Some(ref ts) = self.timestamp {
            ts.encode(&mut out);
        }
        out.extend_from_slice(&self.comment);
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_timestamp_no_comment() {
        let s = AprsStatus::parse(b">").unwrap();
        assert!(s.timestamp.is_none());
        assert!(s.comment.is_empty());
    }

    #[test]
    fn with_ddhhmm_timestamp() {
        let s = AprsStatus::parse(b">312359zSystem online").unwrap();
        assert_eq!(s.timestamp, Some(Timestamp::Ddhhmm(31, 23, 59)));
        assert_eq!(s.comment, b"System online");
    }

    #[test]
    fn with_hhmmss_timestamp() {
        let s = AprsStatus::parse(b">235959hHi there!").unwrap();
        assert_eq!(s.timestamp, Some(Timestamp::Hhmmss(23, 59, 59)));
        assert_eq!(s.comment, b"Hi there!");
    }

    #[test]
    fn no_timestamp_with_comment() {
        let s = AprsStatus::parse(b">12.6V 0.2A 22degC").unwrap();
        assert!(s.timestamp.is_none());
        assert_eq!(s.comment, b"12.6V 0.2A 22degC");
    }

    #[test]
    fn encode_round_trip() {
        for raw in [
            b">312359zSystem online".as_slice(),
            b">Hi there!".as_slice(),
            b">".as_slice(),
        ] {
            let s = AprsStatus::parse(raw).unwrap();
            assert_eq!(s.encode(), raw);
        }
    }
}
