/// A raw NMEA GPS sentence received via APRS.
///
/// DTI: `$`
///
/// The NMEA sentence is stored opaque — callers can parse specific sentence
/// types (GPGGA, GPRMC, etc.) from the `data` field themselves.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct AprsNmea {
    pub data: Vec<u8>,
}

impl AprsNmea {
    /// Decode from the information field (including the leading `$` DTI byte).
    pub(crate) fn parse(info: &[u8]) -> Self {
        Self {
            data: info.get(1..).unwrap_or_default().to_vec(),
        }
    }

    pub fn encode(&self) -> Vec<u8> {
        let mut out = vec![b'$'];
        out.extend_from_slice(&self.data);
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip() {
        let raw = b"$GPGGA,123519,4807.038,N,01131.000,E,1,08,0.9,545.4,M,47.0,M,,*47";
        let n = AprsNmea::parse(raw);
        assert_eq!(
            n.data,
            b"GPGGA,123519,4807.038,N,01131.000,E,1,08,0.9,545.4,M,47.0,M,,*47"
        );
        assert_eq!(n.encode().as_slice(), raw.as_slice());
    }
}
