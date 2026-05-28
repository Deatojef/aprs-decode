/// An APRS Station Capabilities report.
///
/// DTI: `<`
///
/// Lists the capabilities of the station, typically as a comma-separated
/// list of `KEY=VALUE` pairs or bare tokens (e.g. `IGATE,MSG_CNT=0`).
/// The content is stored verbatim — the spec doesn't define a normative format.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct AprsCapabilities {
    /// Raw capability string (everything after the `<` DTI byte).
    pub raw: Vec<u8>,
}

impl AprsCapabilities {
    /// Decode from the information field (including the leading `<` DTI byte).
    pub(crate) fn parse(info: &[u8]) -> Self {
        Self { raw: info.get(1..).unwrap_or_default().to_vec() }
    }

    pub fn encode(&self) -> Vec<u8> {
        let mut out = vec![b'<'];
        out.extend_from_slice(&self.raw);
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip() {
        let raw = b"<IGATE,MSG_CNT=10,LOC_CNT=20";
        let cap = AprsCapabilities::parse(raw);
        assert_eq!(cap.raw, b"IGATE,MSG_CNT=10,LOC_CNT=20");
        assert_eq!(cap.encode().as_slice(), raw.as_slice());
    }

    #[test]
    fn empty_capabilities() {
        let cap = AprsCapabilities::parse(b"<");
        assert!(cap.raw.is_empty());
    }
}
