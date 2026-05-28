use crate::callsign::Callsign;
use crate::capabilities::AprsCapabilities;
use crate::digipeater::{Digipeater, parse_via};
use crate::error::AprsError;
use crate::grid::AprsGridLocator;
use crate::item::AprsItem;
use crate::message::AprsMessage;
use crate::mic_e::AprsMicE;
use crate::nmea::AprsNmea;
use crate::object::AprsObject;
use crate::position::AprsPosition;
use crate::query::AprsQuery;
use crate::status::AprsStatus;
use crate::telemetry::AprsTelemetry;
use crate::third_party::AprsThirdParty;
use crate::user_defined::AprsUserDefined;
use crate::weather::AprsPositionlessWeather;

/// A fully decoded APRS packet.
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct AprsPacket {
    /// The source station (transmitter).
    pub from: Callsign,
    /// The destination callsign (AX.25 destination / APRS path destination).
    pub to: Callsign,
    /// The digipeater path.
    pub via: Vec<Digipeater>,
    /// The parsed packet content, discriminated by Data Type Indicator.
    pub data: AprsData,
}

/// The content of an APRS packet, dispatched by Data Type Indicator (DTI).
///
/// The DTI is the first byte of the AX.25 information field.
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[non_exhaustive]
pub enum AprsData {
    /// Position report. DTI: `!` `=` `/` `@`
    Position(AprsPosition),
    /// Message, bulletin, ACK/REJ, or telemetry metadata. DTI: `:`
    Message(AprsMessage),
    /// Status report. DTI: `>`
    Status(AprsStatus),
    /// MIC-E compressed position. DTI: `` ` `` `'` `\x1C` `\x1D`
    MicE(AprsMicE),
    /// Object report. DTI: `;`
    Object(AprsObject),
    /// Item report. DTI: `)`
    Item(AprsItem),
    /// Positionless weather report. DTI: `_`
    Weather(AprsPositionlessWeather),
    /// Telemetry data packet. DTI: `T` (followed by `#`)
    Telemetry(AprsTelemetry),
    /// Station capabilities. DTI: `<`
    Capabilities(AprsCapabilities),
    /// General query. DTI: `?`
    Query(AprsQuery),
    /// Maidenhead grid locator. DTI: `[`
    GridLocator(AprsGridLocator),
    /// Raw NMEA sentence. DTI: `$`
    Nmea(AprsNmea),
    /// Third-party forwarded packet. DTI: `}`
    ThirdParty(AprsThirdParty),
    /// User-defined / experimental packet. DTI: `{`
    UserDefined(AprsUserDefined),

    /// Packet type not yet implemented or not recognized.
    /// Preserves the DTI byte and the raw information field for caller inspection.
    Unknown {
        dti: u8,
        data: Vec<u8>,
    },
}

impl AprsPacket {
    /// Decode a textual APRS packet (APRS-IS format).
    ///
    /// Expected format: `FROM>TO[,VIA...]:DATA`
    ///
    /// The input is `&[u8]` rather than `&str` because APRS information fields may
    /// contain arbitrary bytes (e.g. MIC-E uses bytes 0x1C and 0x1D).
    pub fn decode_textual(input: &[u8]) -> Result<Self, AprsError> {
        if input.is_empty() {
            return Err(AprsError::EmptyPacket);
        }

        let colon = input.iter().position(|&b| b == b':')
            .ok_or(AprsError::MissingInfoDelimiter)?;

        let header = &input[..colon];
        let info = &input[colon + 1..];

        let arrow = header.iter().position(|&b| b == b'>')
            .ok_or(AprsError::MissingDestinationDelimiter)?;

        let from_bytes = &header[..arrow];
        let dest_via = &header[arrow + 1..];

        let (to_bytes, via_bytes) = if let Some(comma) = dest_via.iter().position(|&b| b == b',') {
            (&dest_via[..comma], &dest_via[comma + 1..])
        } else {
            (dest_via, &b""[..])
        };

        let from = Callsign::decode_textual(from_bytes)?;
        let to = Callsign::decode_textual(to_bytes)?;
        let via = parse_via(via_bytes)?;
        let data = dispatch_data(info, &to)?;

        Ok(AprsPacket { from, to, via, data })
    }

    /// Decode a raw AX.25 UI frame.
    ///
    /// Frame structure:
    ///   Destination (7 bytes) + Source (7 bytes) + Repeaters (0–8 × 7 bytes)
    ///   + Control (0x03) + PID (0xF0) + Information field
    pub fn decode_ax25(input: &[u8]) -> Result<Self, AprsError> {
        if input.len() < 16 {
            return Err(AprsError::Ax25FrameTooShort { len: input.len() });
        }

        let (to, _) = Callsign::decode_ax25(&input[0..7])?;
        let (from, src_eoa) = Callsign::decode_ax25(&input[7..14])?;

        let mut pos = 14usize;
        let mut via = Vec::new();

        if !src_eoa {
            loop {
                if pos + 7 > input.len() {
                    return Err(AprsError::Ax25MissingEoa);
                }
                let (digi_call, eoa) = Callsign::decode_ax25(&input[pos..pos + 7])?;
                let heard = input[pos + 6] & 0x80 != 0;
                via.push(Digipeater::Callsign(digi_call, heard));
                pos += 7;
                if eoa { break; }
                if pos >= input.len() {
                    return Err(AprsError::Ax25MissingEoa);
                }
            }
        }

        if pos >= input.len() {
            return Err(AprsError::TruncatedPacket { expected: pos + 2, got: input.len() });
        }
        if input[pos] != 0x03 {
            return Err(AprsError::Ax25NotUiFrame { byte: input[pos] });
        }
        pos += 1;

        if pos >= input.len() {
            return Err(AprsError::TruncatedPacket { expected: pos + 1, got: input.len() });
        }
        if input[pos] != 0xF0 {
            return Err(AprsError::Ax25NotAprsPid { byte: input[pos] });
        }
        pos += 1;

        let info = &input[pos..];
        let data = dispatch_data(info, &to)?;

        Ok(AprsPacket { from, to, via, data })
    }

    /// Encode this packet to textual APRS-IS format.
    pub fn encode_textual(&self) -> Result<Vec<u8>, AprsError> {
        let mut out = Vec::new();
        self.from.encode_textual(&mut out);
        out.push(b'>');
        self.to.encode_textual(&mut out);
        for digi in &self.via {
            out.push(b',');
            digi.encode_textual(&mut out);
        }
        out.push(b':');
        self.encode_info(&mut out)?;
        Ok(out)
    }

    /// Encode this packet to a raw AX.25 UI frame.
    pub fn encode_ax25(&self) -> Result<Vec<u8>, AprsError> {
        let mut out = Vec::new();
        // Destination (EOA=0, more addresses follow)
        self.to.encode_ax25(&mut out, false);
        // Source (EOA=1 if no digipeaters, else 0)
        let src_eoa = self.via.is_empty();
        self.from.encode_ax25(&mut out, src_eoa);
        // Digipeaters
        for (i, digi) in self.via.iter().enumerate() {
            let is_last = i + 1 == self.via.len();
            match digi {
                Digipeater::Callsign(call, _heard) => {
                    call.encode_ax25(&mut out, is_last);
                }
                Digipeater::QConstruct(_, gw) => {
                    gw.encode_ax25(&mut out, is_last);
                }
            }
        }
        out.push(0x03); // Control: UI frame
        out.push(0xF0); // PID: no layer-3 (APRS)
        self.encode_info(&mut out)?;
        Ok(out)
    }

    fn encode_info(&self, out: &mut Vec<u8>) -> Result<(), AprsError> {
        match &self.data {
            AprsData::Position(pos) => {
                out.extend_from_slice(&pos.encode());
            }
            AprsData::Message(msg) => {
                out.extend_from_slice(&msg.encode());
            }
            AprsData::Status(s) => {
                out.extend_from_slice(&s.encode());
            }
            AprsData::MicE(m) => {
                out.extend_from_slice(&m.encode());
            }
            AprsData::Object(o) => {
                out.extend_from_slice(&o.encode());
            }
            AprsData::Item(i) => {
                out.extend_from_slice(&i.encode());
            }
            AprsData::Weather(w) => {
                out.extend_from_slice(&w.encode());
            }
            AprsData::Telemetry(t) => {
                out.extend_from_slice(&t.encode());
            }
            AprsData::Capabilities(c) => {
                out.extend_from_slice(&c.encode());
            }
            AprsData::Query(q) => {
                out.extend_from_slice(&q.encode());
            }
            AprsData::GridLocator(g) => {
                out.extend_from_slice(&g.encode());
            }
            AprsData::Nmea(n) => {
                out.extend_from_slice(&n.encode());
            }
            AprsData::ThirdParty(tp) => {
                out.extend_from_slice(&tp.encode()?);
            }
            AprsData::UserDefined(ud) => {
                out.extend_from_slice(&ud.encode());
            }
            AprsData::Unknown { dti: _, data } => {
                out.extend_from_slice(data);
            }
        }
        Ok(())
    }
}

/// Dispatch the information field to the correct packet parser by DTI.
/// For MIC-E the destination callsign is also needed.
fn dispatch_data(info: &[u8], to: &Callsign) -> Result<AprsData, AprsError> {
    let dti = match info.first() {
        Some(&b) => b,
        None => return Ok(AprsData::Unknown { dti: 0, data: Vec::new() }),
    };

    match dti {
        b'!' | b'=' | b'/' | b'@' => AprsPosition::parse(info).map(AprsData::Position),
        b':'                       => AprsMessage::parse(info).map(AprsData::Message),
        b'>'                       => AprsStatus::parse(info).map(AprsData::Status),
        b'`' | b'\'' | 0x1C | 0x1D => AprsMicE::parse(info, to).map(AprsData::MicE),
        b';'                       => AprsObject::parse(info).map(AprsData::Object),
        b')'                       => AprsItem::parse(info).map(AprsData::Item),
        b'_'                       => AprsPositionlessWeather::parse(info).map(AprsData::Weather),
        b'T'                       => AprsTelemetry::parse(info).map(AprsData::Telemetry),
        b'<'                       => Ok(AprsData::Capabilities(AprsCapabilities::parse(info))),
        b'?'                       => Ok(AprsData::Query(AprsQuery::parse(info))),
        b'['                       => AprsGridLocator::parse(info).map(AprsData::GridLocator),
        b'$'                       => Ok(AprsData::Nmea(AprsNmea::parse(info))),
        b'}'                       => AprsThirdParty::parse(info).map(AprsData::ThirdParty),
        b'{'                       => Ok(AprsData::UserDefined(AprsUserDefined::parse(info))),
        _ => Ok(AprsData::Unknown { dti, data: info.to_vec() }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const POSITION_PACKET: &[u8] =
        b"W1AW-9>APRS,WIDE1-1,WIDE2-2:!4903.50N/07201.75W-Test";

    const MSG_PACKET: &[u8] =
        b"KD9ABC>APDR15,qAR,KD9XYZ::W1AW-9   :Hello world{001";

    #[test]
    fn decode_position_full() {
        let pkt = AprsPacket::decode_textual(POSITION_PACKET).unwrap();
        assert_eq!(pkt.from.to_string(), "W1AW-9");
        assert_eq!(pkt.to.to_string(), "APRS");
        assert_eq!(pkt.via.len(), 2);
        assert!(matches!(pkt.data, AprsData::Position(_)));
    }

    #[test]
    fn decode_message_header() {
        let pkt = AprsPacket::decode_textual(MSG_PACKET).unwrap();
        assert_eq!(pkt.from.to_string(), "KD9ABC");
        assert_eq!(pkt.to.to_string(), "APDR15");
        assert_eq!(pkt.via.len(), 1);
        assert!(matches!(pkt.data, AprsData::Message(_)));
    }

    #[test]
    fn empty_input_error() {
        assert!(AprsPacket::decode_textual(b"").is_err());
    }

    #[test]
    fn missing_arrow_error() {
        assert!(AprsPacket::decode_textual(b"W1AW:!hello").is_err());
    }

    #[test]
    fn missing_colon_error() {
        assert!(AprsPacket::decode_textual(b"W1AW>APRS,WIDE1").is_err());
    }

    #[test]
    fn no_via_path() {
        let pkt = AprsPacket::decode_textual(b"W1AW>APRS:>Status text").unwrap();
        assert!(pkt.via.is_empty());
    }

    #[test]
    fn unknown_dti_preserved() {
        let pkt = AprsPacket::decode_textual(b"W1AW>APRS:~custom data").unwrap();
        #[allow(unreachable_patterns)]
        match &pkt.data {
            AprsData::Unknown { dti, data } => {
                assert_eq!(*dti, b'~');
                assert_eq!(data.as_slice(), b"~custom data");
            }
            _ => panic!("expected Unknown"),
        }
    }

    #[test]
    fn encode_textual_round_trip() {
        let pkt = AprsPacket::decode_textual(POSITION_PACKET).unwrap();
        let encoded = pkt.encode_textual().unwrap();
        assert_eq!(encoded, POSITION_PACKET);
    }

    #[test]
    fn encode_ax25_round_trip() {
        let pkt = AprsPacket::decode_textual(POSITION_PACKET).unwrap();
        let ax25 = pkt.encode_ax25().unwrap();
        let decoded = AprsPacket::decode_ax25(&ax25).unwrap();
        assert_eq!(decoded.from.to_string(), "W1AW-9");
        assert_eq!(decoded.to.to_string(), "APRS");
    }
}
