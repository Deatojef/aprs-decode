//! APRS telemetry report parsing and encoding.
//!
//! # Data packet
//! Format: `T#SSS,V1,V2,V3,V4,V5,BBBBBBBB[,comment]`
//!
//! # Metadata (sent as messages to a specific station)
//! - `PARM.n1,n2,...` — parameter names (5 analog + up to 8 digital)
//! - `UNIT.u1,u2,...` — unit labels
//! - `EQNS.a1,b1,c1,a2,...` — equation coefficients for linear conversion
//! - `BITS.bbbbbbbbb,project` — bit sense flags + project name

use crate::error::AprsError;

// ─── AprsTelemetry ───────────────────────────────────────────────────────────

/// An APRS telemetry data packet.
///
/// DTI: `T` (followed by `#`)
///
/// The APRS spec defines exactly 5 analog channels and 8 digital bits.
/// Using fixed-size arrays avoids heap allocation and makes the type encoding
/// match the protocol constraint.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct AprsTelemetry {
    /// Sequence number (000–999, or MIC-E style alphanumeric).
    pub sequence: Vec<u8>,
    /// Five analog channel values. `None` means the field was absent or unparseable.
    pub analog: [Option<f32>; 5],
    /// Eight digital channel bits packed into a byte (bit 7 = channel 1).
    pub digital: u8,
    pub comment: Vec<u8>,
}

impl AprsTelemetry {
    /// Decode from the information field (including the leading `T` DTI byte).
    pub(crate) fn parse(info: &[u8]) -> Result<Self, AprsError> {
        // info[0] = 'T', info[1] must be '#'
        if info.len() < 2 || info[1] != b'#' {
            return Err(AprsError::TruncatedPacket {
                expected: 2,
                got: info.len(),
            });
        }
        let body = &info[2..]; // skip "T#"

        // Split by commas: [seq, v1, v2, v3, v4, v5, bits, ...comment]
        let parts: Vec<&[u8]> = body.split(|&c| c == b',').collect();

        let sequence = parts.first().unwrap_or(&b"".as_slice()).to_vec();

        let mut analog = [None; 5];
        for (i, slot) in analog.iter_mut().enumerate() {
            if let Some(part) = parts.get(i + 1) {
                *slot = std::str::from_utf8(part)
                    .ok()
                    .and_then(|s| s.trim().parse::<f32>().ok());
            }
        }

        let digital = parts
            .get(6)
            .and_then(|part| {
                if part.len() >= 8 && part[..8].iter().all(|&c| c == b'0' || c == b'1') {
                    let mut val = 0u8;
                    for &bit in &part[..8] {
                        val = (val << 1) | (bit - b'0');
                    }
                    Some(val)
                } else {
                    None
                }
            })
            .unwrap_or(0);

        let comment = if parts.len() > 7 {
            let mut c = Vec::new();
            for (i, part) in parts[7..].iter().enumerate() {
                if i > 0 {
                    c.push(b',');
                }
                c.extend_from_slice(part);
            }
            c
        } else {
            vec![]
        };

        Ok(Self {
            sequence,
            analog,
            digital,
            comment,
        })
    }

    pub fn encode(&self) -> Vec<u8> {
        let mut out = b"T#".to_vec();
        out.extend_from_slice(&self.sequence);
        for val in &self.analog {
            out.push(b',');
            if let Some(v) = val {
                // Prefer integer formatting when the value is a whole number
                if *v == v.trunc() && v.is_finite() {
                    out.extend_from_slice(format!("{}", *v as i64).as_bytes());
                } else {
                    out.extend_from_slice(format!("{}", v).as_bytes());
                }
            }
        }
        out.push(b',');
        for i in (0..8).rev() {
            out.push(b'0' + ((self.digital >> i) & 1));
        }
        if !self.comment.is_empty() {
            out.push(b',');
            out.extend_from_slice(&self.comment);
        }
        out
    }
}

// ─── TelemetryMetadata ───────────────────────────────────────────────────────

/// Equation coefficients for one analog channel: `value = a + b*raw + c*raw²`.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct TelemetryEquation {
    pub a: f32,
    pub b: f32,
    pub c: f32,
}

impl TelemetryEquation {
    /// Apply the equation to a raw ADC value.
    pub fn apply(&self, raw: f32) -> f32 {
        self.a + self.b * raw + self.c * raw * raw
    }
}

/// Telemetry metadata assembled from PARM./UNIT./EQNS./BITS. message texts.
///
/// Each field corresponds to one of the four message types that APRS uses to
/// describe a station's telemetry channels. They are sent as directed messages
/// to the station whose telemetry they describe.
#[derive(Debug, Clone, PartialEq, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct TelemetryMetadata {
    /// Channel names: up to 5 analog + up to 8 digital = 13 entries.
    pub param_names: Vec<Option<Vec<u8>>>,
    /// Unit labels: same layout as param_names.
    pub unit_labels: Vec<Option<Vec<u8>>>,
    /// Equation coefficients for each of the 5 analog channels.
    pub equations: Vec<TelemetryEquation>,
    /// Bit sense flags: bit N (MSB first) is `true` if a `1` means "on".
    pub bit_sense: u8,
    /// Project name (from BITS. message, after the sense flags).
    pub project_name: Vec<u8>,
}

impl TelemetryMetadata {
    /// Parse the body of a `PARM.` message (text after the `PARM.` prefix).
    ///
    /// Returns up to 13 comma-separated names (5 analog + 8 digital).
    pub fn parse_parm(text: &[u8]) -> Vec<Option<Vec<u8>>> {
        parse_csv_fields(text, 13)
    }

    /// Parse the body of a `UNIT.` message.
    pub fn parse_unit(text: &[u8]) -> Vec<Option<Vec<u8>>> {
        parse_csv_fields(text, 13)
    }

    /// Parse the body of an `EQNS.` message.
    ///
    /// Format: `a1,b1,c1,a2,b2,c2,...` (5 triples, 15 values total).
    pub fn parse_eqns(text: &[u8]) -> Vec<TelemetryEquation> {
        let parts: Vec<&[u8]> = text.split(|&c| c == b',').collect();
        let mut result = Vec::with_capacity(5);
        for i in 0..5 {
            let a = parts.get(i * 3).and_then(|p| parse_f32(p)).unwrap_or(0.0);
            let b = parts
                .get(i * 3 + 1)
                .and_then(|p| parse_f32(p))
                .unwrap_or(1.0);
            let c = parts
                .get(i * 3 + 2)
                .and_then(|p| parse_f32(p))
                .unwrap_or(0.0);
            result.push(TelemetryEquation { a, b, c });
        }
        result
    }

    /// Parse the body of a `BITS.` message.
    ///
    /// Format: `BBBBBBBB,Project Name` where B is `0` or `1` (MSB = channel 1).
    pub fn parse_bits(text: &[u8]) -> (u8, Vec<u8>) {
        let comma = text.iter().position(|&b| b == b',');
        let sense_bytes = match comma {
            Some(pos) => &text[..pos],
            None => text,
        };
        let project = match comma {
            Some(pos) => text.get(pos + 1..).unwrap_or_default().to_vec(),
            None => vec![],
        };
        let mut sense = 0u8;
        for (i, &b) in sense_bytes.iter().enumerate().take(8) {
            if b == b'1' {
                sense |= 0x80 >> i;
            }
        }
        (sense, project)
    }
}

// ─── Helpers ─────────────────────────────────────────────────────────────────

fn parse_csv_fields(text: &[u8], max: usize) -> Vec<Option<Vec<u8>>> {
    text.split(|&c| c == b',')
        .take(max)
        .map(|part| {
            let trimmed: Vec<u8> = part.iter().copied().skip_while(|&b| b == b' ').collect();
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed)
            }
        })
        .collect()
}

fn parse_f32(b: &[u8]) -> Option<f32> {
    std::str::from_utf8(b).ok()?.trim().parse().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_basic_telemetry() {
        let t = AprsTelemetry::parse(b"T#001,100,200,300,400,500,10101010").unwrap();
        assert_eq!(t.sequence, b"001");
        assert_eq!(t.analog[0], Some(100.0));
        assert_eq!(t.analog[4], Some(500.0));
        assert_eq!(t.digital, 0b10101010);
        assert!(t.comment.is_empty());
    }

    #[test]
    fn parse_telemetry_with_comment() {
        let t = AprsTelemetry::parse(b"T#001,100,200,300,400,500,11110000,Hello World").unwrap();
        assert_eq!(t.digital, 0b11110000);
        assert_eq!(t.comment, b"Hello World");
    }

    #[test]
    fn encode_round_trip() {
        let raw = b"T#001,100,200,300,400,500,10101010,Test";
        let t = AprsTelemetry::parse(raw).unwrap();
        assert_eq!(t.encode().as_slice(), raw.as_slice());
    }

    #[test]
    fn parse_parm_names() {
        let names = TelemetryMetadata::parse_parm(b"Bat1,Bat2,Temp,Hum,Pres,LED1,LED2");
        assert_eq!(names[0].as_deref(), Some(b"Bat1".as_slice()));
        assert_eq!(names[4].as_deref(), Some(b"Pres".as_slice()));
        assert_eq!(names[5].as_deref(), Some(b"LED1".as_slice()));
    }

    #[test]
    fn parse_eqns() {
        let eqns = TelemetryMetadata::parse_eqns(b"0,0.01,0,0,0.01,0,0,1,0,0,1,0,0,1,0");
        assert_eq!(eqns.len(), 5);
        assert!((eqns[0].b - 0.01).abs() < 0.001);
        assert!((eqns[0].c).abs() < 0.001);
    }

    #[test]
    fn equation_apply() {
        let eq = TelemetryEquation {
            a: 0.0,
            b: 0.01,
            c: 0.0,
        };
        assert!((eq.apply(100.0) - 1.0).abs() < 0.001);
    }

    #[test]
    fn parse_bits() {
        let (sense, project) = TelemetryMetadata::parse_bits(b"11111111,My Station");
        assert_eq!(sense, 0xFF);
        assert_eq!(project, b"My Station");
    }

    #[test]
    fn parse_bits_mixed() {
        let (sense, _) = TelemetryMetadata::parse_bits(b"10110100,Test");
        assert_eq!(sense, 0b10110100);
    }

    #[test]
    fn missing_digital_bits_defaults_to_zero() {
        // If the 8th field is missing or not binary, default digital to 0
        let t = AprsTelemetry::parse(b"T#001,100,200,300,400,500").unwrap();
        assert_eq!(t.digital, 0);
    }
}
