use crate::callsign::Callsign;
use crate::error::AprsError;
use std::fmt;

/// A Q-construct used on APRS-IS to describe how a packet entered the internet.
///
/// Defined in the APRS-IS Q-construct specification.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum QConstruct {
    /// qAC — server login verified
    Ac,
    /// qAX — no verification (unverified login)
    Ax,
    /// qAO — heard via RF, originated on internet
    Ao,
    /// qAR — via bidirectional internet gateway
    Ar,
    /// qAS — via server without verification
    As,
    /// qAT — traced via internet
    At,
    /// qAI — server-generated packet
    Ai,
    /// qAo — heard directly via RF (lowercase o)
    AoRf,
    /// qAr — received from RF to internet
    ArRf,
    /// qAZ — zero hop (RF or direct)
    Az,
    /// Unknown Q-construct token
    Unknown(String),
}

impl QConstruct {
    fn from_bytes(bytes: &[u8]) -> Self {
        match bytes {
            b"qAC" => QConstruct::Ac,
            b"qAX" => QConstruct::Ax,
            b"qAO" => QConstruct::Ao,
            b"qAR" => QConstruct::Ar,
            b"qAS" => QConstruct::As,
            b"qAT" => QConstruct::At,
            b"qAI" => QConstruct::Ai,
            b"qAo" => QConstruct::AoRf,
            b"qAr" => QConstruct::ArRf,
            b"qAZ" => QConstruct::Az,
            other => QConstruct::Unknown(String::from_utf8_lossy(other).into_owned()),
        }
    }

    fn as_bytes(&self) -> &[u8] {
        match self {
            QConstruct::Ac => b"qAC",
            QConstruct::Ax => b"qAX",
            QConstruct::Ao => b"qAO",
            QConstruct::Ar => b"qAR",
            QConstruct::As => b"qAS",
            QConstruct::At => b"qAT",
            QConstruct::Ai => b"qAI",
            QConstruct::AoRf => b"qAo",
            QConstruct::ArRf => b"qAr",
            QConstruct::Az => b"qAZ",
            QConstruct::Unknown(s) => s.as_bytes(),
        }
    }
}

impl fmt::Display for QConstruct {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", String::from_utf8_lossy(self.as_bytes()))
    }
}

/// One element of the APRS via path (the digipeater list).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum Digipeater {
    /// A callsign-based digipeater path element, with optional "has-been-heard" flag (`*`).
    Callsign(Callsign, bool),
    /// An APRS-IS Q-construct (e.g. `qAR,IGATE`).
    QConstruct(QConstruct, Callsign),
}

impl Digipeater {
    /// Parse one via element from its textual bytes (without the surrounding commas).
    pub fn decode_textual(input: &[u8]) -> Result<Self, AprsError> {
        // Q-constructs are `qA?` tokens (qAC, qAR, …); match the `qA` prefix so a
        // genuine callsign isn't misclassified.
        if input.starts_with(b"qA") {
            // Format: qXX,IGATECALL — but in the via list each element is comma-split
            // so a Q-construct is just the "qXX" token; the following callsign is
            // the next element. We store them together for fidelity.
            // In practice the APRS-IS format encodes them as separate comma elements,
            // so this case handles a bare qXX token.
            return Ok(Digipeater::QConstruct(
                QConstruct::from_bytes(input),
                Callsign::decode_textual(b"UNKNOWN").unwrap(), // placeholder; see parse_via
            ));
        }

        // Strip the heard flag
        let (call_bytes, heard) = if input.ends_with(b"*") {
            (&input[..input.len() - 1], true)
        } else {
            (input, false)
        };

        let callsign = Callsign::decode_textual(call_bytes).map_err(|_| AprsError::InvalidVia {
            raw: input.to_vec(),
        })?;

        Ok(Digipeater::Callsign(callsign, heard))
    }

    /// Write this digipeater element in textual APRS format.
    pub fn encode_textual(&self, out: &mut Vec<u8>) {
        match self {
            Digipeater::Callsign(call, heard) => {
                call.encode_textual(out);
                if *heard {
                    out.push(b'*');
                }
            }
            Digipeater::QConstruct(q, gw) => {
                out.extend_from_slice(q.as_bytes());
                out.push(b',');
                gw.encode_textual(out);
            }
        }
    }
}

impl fmt::Display for Digipeater {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Digipeater::Callsign(call, heard) => {
                write!(f, "{call}")?;
                if *heard {
                    write!(f, "*")?;
                }
                Ok(())
            }
            Digipeater::QConstruct(q, gw) => write!(f, "{q},{gw}"),
        }
    }
}

/// Parse the full via list (the comma-separated portion between `>TO` and `:`).
///
/// Handles the APRS-IS Q-construct pairing: `qAR,IGATE` appears as two consecutive
/// elements where the second is the gateway callsign.
pub(crate) fn parse_via(bytes: &[u8]) -> Result<Vec<Digipeater>, AprsError> {
    if bytes.is_empty() {
        return Ok(Vec::new());
    }

    let mut result = Vec::new();
    let mut iter = bytes.split(|&b| b == b',').peekable();

    while let Some(element) = iter.next() {
        if element.starts_with(b"qA") {
            let q = QConstruct::from_bytes(element);
            // The next element is the gateway callsign
            let gw = if let Some(next) = iter.next() {
                Callsign::decode_textual(next)
                    .map_err(|_| AprsError::InvalidVia { raw: next.to_vec() })?
            } else {
                Callsign::decode_textual(b"UNKNOWN").unwrap()
            };
            result.push(Digipeater::QConstruct(q, gw));
        } else {
            result.push(Digipeater::decode_textual(element)?);
        }
    }

    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn callsign_no_heard() {
        let d = Digipeater::decode_textual(b"WIDE2-2").unwrap();
        assert!(matches!(d, Digipeater::Callsign(_, false)));
    }

    #[test]
    fn callsign_heard() {
        let d = Digipeater::decode_textual(b"RELAY*").unwrap();
        assert!(matches!(d, Digipeater::Callsign(_, true)));
    }

    #[test]
    fn via_list_simple() {
        let via = parse_via(b"WIDE1-1,WIDE2-2").unwrap();
        assert_eq!(via.len(), 2);
    }

    #[test]
    fn via_list_with_q_construct() {
        let via = parse_via(b"RELAY*,qAR,KD9ABC").unwrap();
        assert_eq!(via.len(), 2);
        assert!(matches!(&via[1], Digipeater::QConstruct(QConstruct::Ar, _)));
    }

    #[test]
    fn via_list_empty() {
        let via = parse_via(b"").unwrap();
        assert!(via.is_empty());
    }

    #[test]
    fn encode_round_trip() {
        let via = parse_via(b"WIDE1-1,RELAY*").unwrap();
        let mut out = Vec::new();
        for (i, d) in via.iter().enumerate() {
            if i > 0 {
                out.push(b',');
            }
            d.encode_textual(&mut out);
        }
        assert_eq!(out, b"WIDE1-1,RELAY*");
    }
}
