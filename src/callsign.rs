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

/// Maximum length of an SSID in textual form. AX.25 SSIDs are a single digit
/// 0–15; APRS-IS and D-STAR gateways additionally use short alphanumeric SSIDs
/// (e.g. `-S`, `-B`, `-C`). A small bound keeps storage allocation-free.
const MAX_SSID_LEN: usize = 6;

/// Fixed-capacity stack-allocated ASCII string for an alphanumeric SSID.
#[derive(Clone, PartialEq, Eq, Hash)]
struct SsidBuf {
    bytes: [u8; MAX_SSID_LEN],
    len: u8,
}

impl SsidBuf {
    fn from_uppercased(src: &[u8]) -> Self {
        let mut bytes = [0u8; MAX_SSID_LEN];
        for (i, &b) in src.iter().enumerate() {
            bytes[i] = b.to_ascii_uppercase();
        }
        SsidBuf {
            bytes,
            len: src.len() as u8,
        }
    }

    fn as_str(&self) -> &str {
        std::str::from_utf8(&self.bytes[..self.len as usize]).expect("ssid is always ASCII")
    }
}

/// An APRS callsign with an optional SSID.
///
/// The SSID is usually a numeric 0–15 (as in AX.25) but APRS-IS and D-STAR
/// gateways also use short alphanumeric SSIDs such as `-S` or `-B`. Both the
/// base call and SSID are stored as uppercase ASCII in fixed-size inline
/// buffers — no heap allocation.
#[derive(Clone, PartialEq, Eq, Hash)]
pub struct Callsign {
    call: CallBuf,
    ssid: Option<SsidBuf>,
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
    /// Parse a textual callsign (e.g. `W1AW-9`, or a D-STAR `K0HRV-S`).
    pub fn decode_textual(input: &[u8]) -> Result<Self, AprsError> {
        let (call_bytes, ssid) = if let Some(pos) = input.iter().position(|&b| b == b'-') {
            // `parse_ssid` returns `None` when invalid, `Some(None)` when the SSID
            // normalizes to "no SSID" (e.g. `-0`, matching AX.25), and `Some(Some)`
            // otherwise.
            let ssid = parse_ssid(&input[pos + 1..]).ok_or_else(|| AprsError::InvalidCallsign {
                raw: input.to_vec(),
            })?;
            (&input[..pos], ssid)
        } else {
            (input, None)
        };

        let call = parse_call(call_bytes).ok_or_else(|| AprsError::InvalidCallsign {
            raw: input.to_vec(),
        })?;

        Ok(Callsign { call, ssid })
    }

    /// Decode a 7-byte AX.25 address field.
    ///
    /// AX.25 stores each character left-shifted by one bit. The SSID byte encodes
    /// the SSID in bits 1–4 and the end-of-address (EOA) flag in bit 0.
    /// Returns `(callsign, eoa)`.
    pub fn decode_ax25(bytes: &[u8]) -> Result<(Self, bool), AprsError> {
        if bytes.len() < 7 {
            return Err(AprsError::TruncatedPacket {
                expected: 7,
                got: bytes.len(),
            });
        }
        let mut raw_call = [b' '; 6];
        for i in 0..6 {
            let shifted = bytes[i];
            // LSB of each call byte must be 0 in valid AX.25
            if shifted & 0x01 != 0 {
                return Err(AprsError::InvalidCallsign {
                    raw: bytes[..7].to_vec(),
                });
            }
            raw_call[i] = shifted >> 1;
        }
        // Trim trailing spaces
        let end = raw_call
            .iter()
            .rposition(|&b| b != b' ')
            .map(|p| p + 1)
            .unwrap_or(0);
        let call = parse_call(&raw_call[..end]).ok_or_else(|| AprsError::InvalidCallsign {
            raw: bytes[..7].to_vec(),
        })?;

        let ssid_byte = bytes[6];
        let ssid_val = (ssid_byte >> 1) & 0x0F;
        // AX.25 SSIDs are always numeric; store the canonical decimal form.
        let ssid = if ssid_val == 0 {
            None
        } else {
            let mut tmp = [0u8; 2];
            let s: &[u8] = if ssid_val >= 10 {
                tmp[0] = b'0' + ssid_val / 10;
                tmp[1] = b'0' + ssid_val % 10;
                &tmp[..2]
            } else {
                tmp[0] = b'0' + ssid_val;
                &tmp[..1]
            };
            Some(SsidBuf::from_uppercased(s))
        };
        let eoa = ssid_byte & 0x01 != 0;

        Ok((Callsign { call, ssid }, eoa))
    }

    pub fn as_str(&self) -> &str {
        self.call.as_str()
    }

    /// The SSID in textual form (e.g. `"9"`, `"S"`), or `None` for no SSID.
    pub fn ssid(&self) -> Option<&str> {
        self.ssid.as_ref().map(SsidBuf::as_str)
    }

    /// The SSID as a number, if it is numeric (0–15). Returns `None` for no SSID
    /// or for an alphanumeric (e.g. D-STAR) SSID.
    pub fn ssid_numeric(&self) -> Option<u8> {
        self.ssid()
            .and_then(|s| s.parse::<u8>().ok())
            .filter(|v| *v <= 15)
    }

    /// Write this callsign in textual APRS format.
    pub fn encode_textual(&self, out: &mut Vec<u8>) {
        out.extend_from_slice(self.call.as_str().as_bytes());
        if let Some(ref ssid) = self.ssid {
            out.push(b'-');
            out.extend_from_slice(ssid.as_str().as_bytes());
        }
    }

    /// Write this callsign as a 7-byte AX.25 address field.
    ///
    /// AX.25 can only represent numeric SSIDs 0–15; an alphanumeric SSID (only
    /// valid in textual/APRS-IS form) is encoded as SSID 0.
    pub fn encode_ax25(&self, out: &mut Vec<u8>, eoa: bool) {
        let call = self.call.as_str().as_bytes();
        for i in 0..6 {
            let b = if i < call.len() { call[i] } else { b' ' };
            out.push(b << 1);
        }
        let ssid_val = self.ssid_numeric().unwrap_or(0) & 0x0F;
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
        if let Some(ref ssid) = self.ssid {
            write!(f, "-{}", ssid.as_str())?;
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
    Some(CallBuf {
        bytes: buf,
        len: bytes.len() as u8,
    })
}

/// Parse the SSID portion (the bytes after `-`).
///
/// Returns `None` for an invalid SSID, `Some(None)` when it normalizes to
/// "no SSID" (`-0`, matching AX.25), and `Some(Some(..))` otherwise. A purely
/// numeric SSID is validated against the AX.25 0–15 range and canonicalized
/// (no leading zeros); a short alphanumeric SSID (e.g. D-STAR `-S`, `-B`) is
/// accepted as-is.
fn parse_ssid(bytes: &[u8]) -> Option<Option<SsidBuf>> {
    if bytes.is_empty() || bytes.len() > MAX_SSID_LEN {
        return None;
    }
    if bytes.iter().all(u8::is_ascii_digit) {
        // Numeric SSID: enforce the AX.25 0–15 range and canonicalize.
        let mut val: u8 = 0;
        for &b in bytes {
            val = val.checked_mul(10)?.checked_add(b - b'0')?;
        }
        if val > 15 {
            return None;
        }
        if val == 0 {
            return Some(None);
        }
        let mut tmp = [0u8; 2];
        let s: &[u8] = if val >= 10 {
            tmp[0] = b'0' + val / 10;
            tmp[1] = b'0' + val % 10;
            &tmp[..2]
        } else {
            tmp[0] = b'0' + val;
            &tmp[..1]
        };
        return Some(Some(SsidBuf::from_uppercased(s)));
    }
    // Alphanumeric SSID (APRS-IS / D-STAR), e.g. `-S`, `-B`, `-RPT`.
    if !bytes.iter().all(u8::is_ascii_alphanumeric) {
        return None;
    }
    Some(Some(SsidBuf::from_uppercased(bytes)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn textual_no_ssid() {
        let c = Callsign::decode_textual(b"W1AW").unwrap();
        assert_eq!(c.as_str(), "W1AW");
        assert_eq!(c.ssid(), None);
    }

    #[test]
    fn textual_with_ssid() {
        let c = Callsign::decode_textual(b"W1AW-9").unwrap();
        assert_eq!(c.as_str(), "W1AW");
        assert_eq!(c.ssid(), Some("9"));
        assert_eq!(c.ssid_numeric(), Some(9));
    }

    #[test]
    fn textual_ssid_15() {
        let c = Callsign::decode_textual(b"N0CALL-15").unwrap();
        assert_eq!(c.ssid(), Some("15"));
        assert_eq!(c.ssid_numeric(), Some(15));
    }

    #[test]
    fn textual_ssid_16_invalid() {
        assert!(Callsign::decode_textual(b"N0CALL-16").is_err());
    }

    #[test]
    fn textual_ssid_0_normalized_to_none() {
        let c = Callsign::decode_textual(b"W1AW-0").unwrap();
        assert_eq!(c.ssid(), None);
        assert_eq!(c.to_string(), "W1AW");
    }

    #[test]
    fn textual_alphanumeric_ssid_dstar() {
        // D-STAR gateways use alphanumeric SSIDs (`-S`, `-B`, `-C`).
        let c = Callsign::decode_textual(b"K0HRV-S").unwrap();
        assert_eq!(c.as_str(), "K0HRV");
        assert_eq!(c.ssid(), Some("S"));
        assert_eq!(c.ssid_numeric(), None);
        assert_eq!(c.to_string(), "K0HRV-S");
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
