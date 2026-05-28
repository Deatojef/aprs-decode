/// A user-defined / experimental APRS packet.
///
/// DTI: `{`
///
/// The first byte after the DTI is a single-character experimenter ID, the
/// second is a packet type character, and the rest is opaque payload.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct AprsUserDefined {
    /// Experimenter / user ID (single byte).
    pub user_id: u8,
    /// User-defined packet type (single byte).
    pub packet_type: u8,
    /// Opaque payload.
    pub data: Vec<u8>,
}

impl AprsUserDefined {
    /// Decode from the information field (including the leading `{` DTI byte).
    pub(crate) fn parse(info: &[u8]) -> Self {
        let body = info.get(1..).unwrap_or_default();
        Self {
            user_id:     body.first().copied().unwrap_or(0),
            packet_type: body.get(1).copied().unwrap_or(0),
            data:        body.get(2..).unwrap_or_default().to_vec(),
        }
    }

    pub fn encode(&self) -> Vec<u8> {
        let mut out = vec![b'{', self.user_id, self.packet_type];
        out.extend_from_slice(&self.data);
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip() {
        let raw = b"{Qhello world";
        let ud = AprsUserDefined::parse(raw);
        assert_eq!(ud.user_id, b'Q');
        assert_eq!(ud.packet_type, b'h');
        assert_eq!(ud.data, b"ello world");
        assert_eq!(ud.encode().as_slice(), raw.as_slice());
    }

    #[test]
    fn minimal() {
        let ud = AprsUserDefined::parse(b"{AB");
        assert_eq!(ud.user_id, b'A');
        assert_eq!(ud.packet_type, b'B');
        assert!(ud.data.is_empty());
    }
}
