use crate::error::AprsError;
use crate::packet::AprsPacket;

/// A third-party APRS packet: a complete APRS packet forwarded from one
/// network to another (e.g. RF→APRS-IS gateway).
///
/// DTI: `}`
///
/// The payload after `}` is a full textual APRS packet that is recursively
/// decoded. Decode failures return an error rather than silently ignoring them.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct AprsThirdParty {
    /// The forwarded inner packet, recursively decoded.
    pub inner: Box<AprsPacket>,
}

impl AprsThirdParty {
    /// Decode from the information field (including the leading `}` DTI byte).
    pub(crate) fn parse(info: &[u8]) -> Result<Self, AprsError> {
        let inner_bytes = info.get(1..).unwrap_or_default();
        let inner = AprsPacket::decode_textual(inner_bytes)?;
        Ok(Self {
            inner: Box::new(inner),
        })
    }

    pub fn encode(&self) -> Result<Vec<u8>, AprsError> {
        let mut out = vec![b'}'];
        out.extend_from_slice(&self.inner.encode_textual()?);
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::AprsData;

    #[test]
    fn decode_third_party_position() {
        let tp = AprsThirdParty::parse(b"}WB0VGI-7>APOT30,W0RO-11*,WIDE2-1:!4228.35N/09101.45Wk")
            .unwrap();
        assert_eq!(tp.inner.from.to_string(), "WB0VGI-7");
        assert!(matches!(tp.inner.data, AprsData::Position(_)));
    }

    #[test]
    fn decode_third_party_message() {
        let tp =
            AprsThirdParty::parse(b"}W1ABC>APRS,WIDE1-1::DEST     :Hello from third party{123")
                .unwrap();
        assert_eq!(tp.inner.from.to_string(), "W1ABC");
        assert!(matches!(tp.inner.data, AprsData::Message(_)));
    }

    #[test]
    fn encode_round_trip() {
        let inner = b"}W1ABC>APRS::DEST     :Hello{001";
        let tp = AprsThirdParty::parse(inner).unwrap();
        let encoded = tp.encode().unwrap();
        assert_eq!(encoded.as_slice(), inner.as_slice());
    }
}
