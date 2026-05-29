use crate::error::AprsError;
use crate::util::trim_spaces_end;

/// The subtype of an APRS message packet, discriminated at parse time.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum MessageSubtype {
    /// Directed message to a specific station. `id` is the optional message number.
    Directed { id: Option<Vec<u8>> },
    /// Acknowledgement: `id` is the message number being ACK'd.
    Ack { id: Vec<u8> },
    /// Rejection: `id` is the message number being REJ'd.
    Rej { id: Vec<u8> },
    /// General bulletin (addressee starts with `BLN`).
    Bulletin,
    /// National Weather Service or equivalent weather alert bulletin.
    /// Addressee starts with `NWS`, `SKY`, `CWA`, or `BOM`.
    NwsBulletin,
    /// Telemetry parameter names. Text starts with `PARM.`; addressee is the described station.
    TelemetryParm,
    /// Telemetry unit/label names. Text starts with `UNIT.`.
    TelemetryUnit,
    /// Telemetry equation coefficients. Text starts with `EQNS.`.
    TelemetryEqns,
    /// Telemetry bit sense / project name. Text starts with `BITS.`.
    TelemetryBits,
    /// Directed station query. Text starts with `?`.
    DirectedQuery,
}

/// An APRS message, bulletin, ACK/REJ, or telemetry metadata packet.
///
/// DTI: `:`
///
/// Wire format: `:AAAAAAAAA:text{id`
/// where `AAAAAAAAA` is a space-padded 9-character addressee.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct AprsMessage {
    /// Addressee, trimmed of trailing spaces.
    pub addressee: Vec<u8>,
    /// Message text (without the `{` separator or message number).
    pub text: Vec<u8>,
    /// Subtype discriminated from the addressee and text content.
    pub subtype: MessageSubtype,
}

impl AprsMessage {
    /// Decode from the information field (including the leading `:` DTI byte).
    pub(crate) fn parse(info: &[u8]) -> Result<Self, AprsError> {
        // info = `:AAAAAAAAA:text{id`
        // We need at least `:` + 9-char addressee + `:` = 11 bytes
        if info.len() < 11 {
            return Err(AprsError::InvalidMessageMissingDelimiter);
        }
        // Byte 0 is the DTI `:`, bytes 1-9 are the addressee, byte 10 must be `:`
        if info[10] != b':' {
            return Err(AprsError::InvalidMessageMissingDelimiter);
        }
        let mut addressee = info[1..10].to_vec();
        trim_spaces_end(&mut addressee);

        let body = &info[11..]; // everything after the second `:`

        // Split body into text and optional id on `{`
        let (text_bytes, id_bytes) = if let Some(pos) = body.iter().position(|&b| b == b'{') {
            (&body[..pos], Some(&body[pos + 1..]))
        } else {
            (body, None)
        };

        let text = text_bytes.to_vec();
        let subtype = discriminate_subtype(&addressee, &text, id_bytes);

        Ok(Self {
            addressee,
            text,
            subtype,
        })
    }

    pub fn encode(&self) -> Vec<u8> {
        let mut out = Vec::new();
        out.push(b':');
        out.extend_from_slice(&self.addressee);
        // Pad addressee to 9 bytes
        out.extend(std::iter::repeat_n(
            b' ',
            9usize.saturating_sub(self.addressee.len()),
        ));
        out.push(b':');

        match &self.subtype {
            MessageSubtype::Ack { id } => {
                out.extend_from_slice(b"ack");
                out.extend_from_slice(id);
            }
            MessageSubtype::Rej { id } => {
                out.extend_from_slice(b"rej");
                out.extend_from_slice(id);
            }
            MessageSubtype::Directed { id } => {
                out.extend_from_slice(&self.text);
                if let Some(id) = id {
                    out.push(b'{');
                    out.extend_from_slice(id);
                }
            }
            _ => {
                // Bulletins, NWS, telemetry metadata, queries: text is the full body
                out.extend_from_slice(&self.text);
            }
        }
        out
    }
}

fn discriminate_subtype(addressee: &[u8], text: &[u8], id: Option<&[u8]>) -> MessageSubtype {
    // ACK / REJ are identified by the message text prefix
    if text.starts_with(b"ack") {
        return MessageSubtype::Ack {
            id: text[3..].to_vec(),
        };
    }
    if text.starts_with(b"rej") {
        return MessageSubtype::Rej {
            id: text[3..].to_vec(),
        };
    }

    // Bulletins: addressee starts with BLN
    if addressee.starts_with(b"BLN") {
        return MessageSubtype::Bulletin;
    }

    // NWS weather bulletins: specific addressee prefixes
    if addressee.starts_with(b"NWS")
        || addressee.starts_with(b"SKY")
        || addressee.starts_with(b"CWA")
        || addressee.starts_with(b"BOM")
    {
        return MessageSubtype::NwsBulletin;
    }

    // Telemetry metadata (message to a station about its telemetry)
    if text.starts_with(b"PARM.") {
        return MessageSubtype::TelemetryParm;
    }
    if text.starts_with(b"UNIT.") {
        return MessageSubtype::TelemetryUnit;
    }
    if text.starts_with(b"EQNS.") {
        return MessageSubtype::TelemetryEqns;
    }
    if text.starts_with(b"BITS.") {
        return MessageSubtype::TelemetryBits;
    }

    // Directed query
    if text.starts_with(b"?") {
        return MessageSubtype::DirectedQuery;
    }

    MessageSubtype::Directed {
        id: id.map(|b| b.to_vec()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn directed_no_id() {
        let m = AprsMessage::parse(b":W1AW-9   :Hello world").unwrap();
        assert_eq!(m.addressee, b"W1AW-9");
        assert_eq!(m.text, b"Hello world");
        assert!(matches!(m.subtype, MessageSubtype::Directed { id: None }));
    }

    #[test]
    fn directed_with_id() {
        let m = AprsMessage::parse(b":DESTINATI:Hello World! This msg has a : colon {329A7D5Z4")
            .unwrap();
        assert_eq!(m.addressee, b"DESTINATI");
        assert_eq!(m.text, b"Hello World! This msg has a : colon ");
        assert!(matches!(
            m.subtype,
            MessageSubtype::Directed { id: Some(ref id) } if id == b"329A7D5Z4"
        ));
    }

    #[test]
    fn ack() {
        let m = AprsMessage::parse(b":W1AW-9   :ack001").unwrap();
        assert!(matches!(m.subtype, MessageSubtype::Ack { ref id } if id == b"001"));
    }

    #[test]
    fn rej() {
        let m = AprsMessage::parse(b":W1AW-9   :rej001").unwrap();
        assert!(matches!(m.subtype, MessageSubtype::Rej { ref id } if id == b"001"));
    }

    #[test]
    fn bulletin() {
        let m = AprsMessage::parse(b":BLN3     :Net at 21:00z tonight").unwrap();
        assert!(matches!(m.subtype, MessageSubtype::Bulletin));
    }

    #[test]
    fn nws_bulletin() {
        let m = AprsMessage::parse(b":NWS-WARN :Tornado warning in effect").unwrap();
        assert!(matches!(m.subtype, MessageSubtype::NwsBulletin));
    }

    #[test]
    fn telemetry_parm() {
        let m = AprsMessage::parse(b":KD9ABC   :PARM.Bat1,Bat2,Temp,Hum,Pres").unwrap();
        assert!(matches!(m.subtype, MessageSubtype::TelemetryParm));
    }

    #[test]
    fn telemetry_bits() {
        let m = AprsMessage::parse(b":KD9ABC   :BITS.11111111,My Project").unwrap();
        assert!(matches!(m.subtype, MessageSubtype::TelemetryBits));
    }

    #[test]
    fn directed_query() {
        let m = AprsMessage::parse(b":KD9ABC   :?APRSD").unwrap();
        assert!(matches!(m.subtype, MessageSubtype::DirectedQuery));
    }

    #[test]
    fn too_short() {
        assert!(AprsMessage::parse(b":W1AW:hi").is_err());
    }

    #[test]
    fn encode_round_trip_directed() {
        let raw = b":DESTINATI:Hello World! This msg has a : colon {329A7D5Z4";
        let m = AprsMessage::parse(raw).unwrap();
        assert_eq!(m.encode(), raw);
    }

    #[test]
    fn encode_round_trip_bulletin() {
        let raw = b":BLN3     :Net at 21:00z tonight";
        let m = AprsMessage::parse(raw).unwrap();
        assert_eq!(m.encode(), raw);
    }

    #[test]
    fn encode_ack() {
        let raw = b":W1AW-9   :ack001";
        let m = AprsMessage::parse(raw).unwrap();
        assert_eq!(m.encode(), raw);
    }
}
