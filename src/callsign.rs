use crate::error::AprsError;
use std::fmt;

/// Maximum length of an APRS callsign (base call only, without SSID).
/// AX.25 limits to 6; APRS-IS allows 9 for internet-only stations.
const MAX_CALL_LEN: usize = 9;

/// Fixed-capacity stack-allocated ASCII string for the base callsign.
#[derive(Clone, PartialEq, Eq, Hash)]
struct CallBuf {
    bytes: [u8; MAX_CALL_LEN],
    len: u8,
}

impl CallBuf {
    fn as_str(&self) -> &str {
        std::str::from_utf8(&self.bytes[..self.len as usize]).expect("callsign is always ASCII")
    }
}

/// An APRS callsign with optional numeric SSID (0–15).
///
/// Stored as uppercase ASCII in a fixed-size inline buffer — no heap allocation.
#[derive(Clone, PartialEq, Eq, Hash)]
pub struct Callsign {
    call: CallBuf,
    pub ssid: Option<u8>,
}

#[cfg(feature = "serde")]
impl serde::Serialize for Callsign {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&self.to_string())
    }
}

#[cfg(feature = "serde")]
impl<'de> serde::Deserialize<'de> for Callsign {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let s = String::deserialize(d)?;
        Callsign::decode_textual(s.as_bytes()).map_err(serde::de::Error::custom)
    }
}

impl Callsign {
    /// Parse a textual callsign (e.g. `W1AW-9`).
    pub fn decode_textual(input: &[u8]) -> Result<Self, AprsError> {
        let (call_bytes, ssid) = if let Some(pos) = input.iter().position(|&b| b == b'-') {
            let ssid_bytes = &input[pos + 1..];
            let ssid = parse_ssid(ssid_bytes)
                .ok_or_else(|| AprsError::InvalidCallsign { raw: input.to_vec() })?;
            (&input[..pos], Some(ssid))
        } else {
            (input, None)
        };

        let call = parse_call(call_bytes)
            .ok_or_else(|| AprsError::InvalidCallsign { raw: input.to_vec() })?;

        Ok(Callsign { call, ssid })
    }

    /// Decode a 7-byte AX.25 address field.
    ///
    /// AX.25 stores each character left-shifted by one bit. The SSID byte encodes
    /// the SSID in bits 1–4 and the end-of-address (EOA) flag in bit 0.
    /// Returns `(callsign, eoa)`.
    pub fn decode_ax25(bytes: &[u8]) -> Result<(Self, bool), AprsError> {
        if bytes.len() < 7 {
            return Err(AprsError::TruncatedPacket { expected: 7, got: bytes.len() });
        }
        let mut raw_call = [b' '; 6];
        for i in 0..6 {
            let shifted = bytes[i];
            // LSB of each call byte must be 0 in valid AX.25
            if shifted & 0x01 != 0 {
                return Err(AprsError::InvalidCallsign { raw: bytes[..7].to_vec() });
            }
            raw_call[i] = shifted >> 1;
        }
        // Trim trailing spaces
        let end = raw_call.iter().rposition(|&b| b != b' ').map(|p| p + 1).unwrap_or(0);
        let call = parse_call(&raw_call[..end])
            .ok_or_else(|| AprsError::InvalidCallsign { raw: bytes[..7].to_vec() })?;

        let ssid_byte = bytes[6];
        let ssid_val = (ssid_byte >> 1) & 0x0F;
        let ssid = if ssid_val == 0 { None } else { Some(ssid_val) };
        let eoa = ssid_byte & 0x01 != 0;

        Ok((Callsign { call, ssid }, eoa))
    }

    pub fn as_str(&self) -> &str {
        self.call.as_str()
    }

    /// Write this callsign in textual APRS format.
    pub fn encode_textual(&self, out: &mut Vec<u8>) {
        out.extend_from_slice(self.call.as_str().as_bytes());
        if let Some(ssid) = self.ssid {
            out.push(b'-');
            if ssid >= 10 {
                out.push(b'0' + ssid / 10);
            }
            out.push(b'0' + ssid % 10);
        }
    }

    /// Write this callsign as a 7-byte AX.25 address field.
    pub fn encode_ax25(&self, out: &mut Vec<u8>, eoa: bool) {
        let call = self.call.as_str().as_bytes();
        for i in 0..6 {
            let b = if i < call.len() { call[i] } else { b' ' };
            out.push(b << 1);
        }
        let ssid_val = self.ssid.unwrap_or(0) & 0x0F;
        let eoa_bit: u8 = if eoa { 0x01 } else { 0x00 };
        // Bits 5 and 7 must be 1 per AX.25 spec (reserved, set to 1)
        out.push(0x60 | (ssid_val << 1) | eoa_bit);
    }
}

impl fmt::Debug for Callsign {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self}")
    }
}

impl fmt::Display for Callsign {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.call.as_str())?;
        if let Some(ssid) = self.ssid {
            write!(f, "-{ssid}")?;
        }
        Ok(())
    }
}

// --- private helpers ---

fn parse_call(bytes: &[u8]) -> Option<CallBuf> {
    if bytes.is_empty() || bytes.len() > MAX_CALL_LEN {
        return None;
    }
    let mut buf = [0u8; MAX_CALL_LEN];
    for (i, &b) in bytes.iter().enumerate() {
        if !b.is_ascii_alphanumeric() {
            return None;
        }
        buf[i] = b.to_ascii_uppercase();
    }
    Some(CallBuf { bytes: buf, len: bytes.len() as u8 })
}

fn parse_ssid(bytes: &[u8]) -> Option<u8> {
    if bytes.is_empty() || bytes.len() > 2 {
        return None;
    }
    let mut val: u8 = 0;
    for &b in bytes {
        if !b.is_ascii_digit() {
            return None;
        }
        val = val.checked_mul(10)?.checked_add(b - b'0')?;
    }
    if val > 15 {
        return None;
    }
    Some(val)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn textual_no_ssid() {
        let c = Callsign::decode_textual(b"W1AW").unwrap();
        assert_eq!(c.as_str(), "W1AW");
        assert_eq!(c.ssid, None);
    }

    #[test]
    fn textual_with_ssid() {
        let c = Callsign::decode_textual(b"W1AW-9").unwrap();
        assert_eq!(c.as_str(), "W1AW");
        assert_eq!(c.ssid, Some(9));
    }

    #[test]
    fn textual_ssid_15() {
        let c = Callsign::decode_textual(b"N0CALL-15").unwrap();
        assert_eq!(c.ssid, Some(15));
    }

    #[test]
    fn textual_ssid_16_invalid() {
        assert!(Callsign::decode_textual(b"N0CALL-16").is_err());
    }

    #[test]
    fn textual_lowercase_normalized() {
        let c = Callsign::decode_textual(b"w1aw").unwrap();
        assert_eq!(c.as_str(), "W1AW");
    }

    #[test]
    fn textual_empty_invalid() {
        assert!(Callsign::decode_textual(b"").is_err());
    }

    #[test]
    fn display_no_ssid() {
        let c = Callsign::decode_textual(b"W1AW").unwrap();
        assert_eq!(c.to_string(), "W1AW");
    }

    #[test]
    fn display_with_ssid() {
        let c = Callsign::decode_textual(b"W1AW-9").unwrap();
        assert_eq!(c.to_string(), "W1AW-9");
    }

    #[test]
    fn ax25_round_trip() {
        let original = Callsign::decode_textual(b"W1AW-9").unwrap();
        let mut encoded = Vec::new();
        original.encode_ax25(&mut encoded, true);
        assert_eq!(encoded.len(), 7);
        let (decoded, eoa) = Callsign::decode_ax25(&encoded).unwrap();
        assert_eq!(decoded, original);
        assert!(eoa);
    }

    #[test]
    fn encode_textual_round_trip() {
        let original = Callsign::decode_textual(b"KD9ABC-3").unwrap();
        let mut out = Vec::new();
        original.encode_textual(&mut out);
        let decoded = Callsign::decode_textual(&out).unwrap();
        assert_eq!(decoded, original);
    }
}
