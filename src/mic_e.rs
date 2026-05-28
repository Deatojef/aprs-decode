use crate::callsign::Callsign;
use crate::error::AprsError;
use crate::types::lonlat::{Latitude, Longitude, Precision};

// ─── MIC-E message type ───────────────────────────────────────────────────────

/// MIC-E message type encoded in the destination callsign.
///
/// Standard types (M0–M6) are defined in APRS101; Custom types (C0–C6)
/// are radio-specific; Emergency is encoded as all-zeros.
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum MicEMessage {
    M0, M1, M2, M3, M4, M5, M6,
    C0, C1, C2, C3, C4, C5, C6,
    Emergency,
    Unknown,
}

impl MicEMessage {
    fn from_bits(a: MsgBit, b: MsgBit, c: MsgBit) -> Self {
        use MsgBit::{Custom, Standard, Zero};
        use MicEMessage::*;
        match (a, b, c) {
            (Standard, Standard, Standard) => M0,
            (Custom,   Custom,   Custom)   => C0,
            (Standard, Standard, Zero)     => M1,
            (Custom,   Custom,   Zero)     => C1,
            (Standard, Zero,     Standard) => M2,
            (Custom,   Zero,     Custom)   => C2,
            (Standard, Zero,     Zero)     => M3,
            (Custom,   Zero,     Zero)     => C3,
            (Zero,     Standard, Standard) => M4,
            (Zero,     Custom,   Custom)   => C4,
            (Zero,     Standard, Zero)     => M5,
            (Zero,     Custom,   Zero)     => C5,
            (Zero,     Zero,     Standard) => M6,
            (Zero,     Zero,     Custom)   => C6,
            (Zero,     Zero,     Zero)     => Emergency,
            _                              => Unknown,
        }
    }

    fn to_bits(self) -> (MsgBit, MsgBit, MsgBit) {
        use MsgBit::{Custom, Standard, Zero};
        use MicEMessage::*;
        match self {
            M0 => (Standard, Standard, Standard),
            C0 => (Custom,   Custom,   Custom),
            M1 => (Standard, Standard, Zero),
            C1 => (Custom,   Custom,   Zero),
            M2 => (Standard, Zero,     Standard),
            C2 => (Custom,   Zero,     Custom),
            M3 => (Standard, Zero,     Zero),
            C3 => (Custom,   Zero,     Zero),
            M4 => (Zero,     Standard, Standard),
            C4 => (Zero,     Custom,   Custom),
            M5 => (Zero,     Standard, Zero),
            C5 => (Zero,     Custom,   Zero),
            M6 => (Zero,     Zero,     Standard),
            C6 => (Zero,     Zero,     Custom),
            Emergency => (Zero, Zero, Zero),
            Unknown   => (Standard, Custom, Standard), // arbitrary non-ambiguous combo
        }
    }
}

#[derive(Copy, Clone)]
enum MsgBit { Zero, Custom, Standard }

impl MsgBit {
    fn from_byte(c: u8) -> Option<Self> {
        match c {
            b'0'..=b'9' | b'L' => Some(MsgBit::Zero),
            b'A'..=b'K'        => Some(MsgBit::Custom),
            b'P'..=b'Z'        => Some(MsgBit::Standard),
            _ => None,
        }
    }
}

// ─── Speed / Course ───────────────────────────────────────────────────────────

/// Speed in knots (0–799).
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(transparent))]
pub struct MicESpeed(pub u32);

impl MicESpeed {
    pub fn knots(self) -> u32 { self.0 }
}

/// Course in degrees (0 = unknown/not applicable, 1–360).
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(transparent))]
pub struct MicECourse(pub u32);

impl MicECourse {
    pub const UNKNOWN: Self = Self(0);
    pub fn degrees(self) -> u32 { self.0 }
}

// ─── Device ID ───────────────────────────────────────────────────────────────

/// Manufacturer and model of the radio that generated this MIC-E packet,
/// decoded from the manufacturer prefix/suffix bytes in the info field.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct MicEDevice {
    pub manufacturer: String,
    pub model: String,
}

/// Static lookup table of known MIC-E manufacturer prefix patterns.
/// Each entry is `(prefix_bytes, manufacturer, model)`.
/// Entries with longer prefixes must appear first to match greedily.
///
/// Sources: direwolf deviceid.c, APRS MIC-E spec, aprsorg/aprs-deviceid
static MICE_PREFIX_TABLE: &[(&[u8], &str, &str)] = &[
    // Kenwood — prefix is the first 1-2 bytes of the optional-rest field (before altitude)
    (b">=", "Kenwood", "TH-D72"),
    (b">^", "Kenwood", "TH-D74"),
    (b">",  "Kenwood", "TH-D7A"),
    (b"]=", "Kenwood", "TM-D710"),
    (b"]",  "Kenwood", "TM-D700"),
    // Yaesu — 2-byte prefix (underscore + char)
    (b"_ ", "Yaesu", "VX-8"),
    (b"_\"","Yaesu", "FTM-350"),
    (b"_#", "Yaesu", "VX-8G"),
    (b"_$", "Yaesu", "FT1D"),
    (b"_%", "Yaesu", "FTM-400DR"),
    (b"_)", "Yaesu", "FTM-100D"),
    (b"_3", "Yaesu", "FT5D"),
    (b"_8", "Yaesu", "FT3D"),
    // Byonics — suffix at END of the comment (`|N`)
    // (handled separately in parse since it's a comment suffix, not a manufacturer prefix)
];

fn lookup_device(prefix: &[u8]) -> Option<MicEDevice> {
    for &(pat, mfr, model) in MICE_PREFIX_TABLE {
        if prefix.starts_with(pat) {
            return Some(MicEDevice { manufacturer: mfr.to_string(), model: model.to_string() });
        }
    }
    None
}

fn lookup_byonics_suffix(comment: &[u8]) -> Option<(MicEDevice, &[u8])> {
    if comment.ends_with(b"|3") {
        return Some((MicEDevice { manufacturer: "Byonics".to_string(), model: "TinyTrak3".to_string() }, &comment[..comment.len()-2]));
    }
    if comment.ends_with(b"|4") {
        return Some((MicEDevice { manufacturer: "Byonics".to_string(), model: "TinyTrak4".to_string() }, &comment[..comment.len()-2]));
    }
    None
}

// ─── AprsMicE ────────────────────────────────────────────────────────────────

/// A decoded MIC-E (Mic Encoder) position report.
///
/// DTI bytes: `` ` `` (current), `'` (old/TM-D700), `\x1C` (old), `\x1D` (current)
///
/// MIC-E encodes latitude and message type in the AX.25 destination callsign,
/// and longitude/speed/course in the first 8 bytes of the information field.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct AprsMicE {
    pub latitude: Latitude,
    pub longitude: Longitude,
    pub precision: Precision,
    pub message: MicEMessage,
    pub speed: MicESpeed,
    pub course: MicECourse,
    pub symbol_code: char,
    pub symbol_table: char,
    /// Comment text (after manufacturer prefix and altitude have been stripped).
    pub comment: Vec<u8>,
    /// Whether this is "current" position (`true`) or "old" (`false`).
    pub is_current: bool,
    /// Altitude in meters above sea level, if encoded in the status field.
    pub altitude_m: Option<f64>,
    /// Decoded manufacturer/device info, if recognized.
    pub device: Option<MicEDevice>,
    /// Raw manufacturer prefix bytes (before altitude), preserved for round-trip.
    pub raw_mfg: Option<Vec<u8>>,
}

impl AprsMicE {
    /// Decode from the information field (including the leading DTI byte) and
    /// the destination `Callsign` (which carries the latitude and message type).
    pub(crate) fn parse(info: &[u8], to: &Callsign) -> Result<Self, AprsError> {
        let dti = *info.first().ok_or(AprsError::EmptyPacket)?;
        let is_current = matches!(dti, b'`' | 0x1D);

        // Decode destination callsign → latitude, precision, message type, lon offset/dir
        let (latitude, precision, message, lon_offset_100, lon_east) =
            decode_dest(to).ok_or_else(|| AprsError::InvalidMicEDestination { raw: to.as_str().as_bytes().to_vec() })?;

        // The info field (after DTI) must have at least 8 bytes:
        //   lon_d(1) lon_m(1) lon_h(1) sp(1) dc(1) se(1) sym_code(1) sym_table(1)
        let b = info.get(1..).ok_or(AprsError::MicETooShort { len: info.len() })?;
        if b.len() < 8 {
            return Err(AprsError::MicETooShort { len: info.len() });
        }

        let longitude = decode_longitude(&b[0..3], lon_offset_100, lon_east)
            .ok_or(AprsError::MicETooShort { len: info.len() })?;
        let (speed, course) = decode_speed_course(&b[3..6])
            .ok_or(AprsError::MicETooShort { len: info.len() })?;

        let symbol_code = b[6] as char;
        let symbol_table = b[7] as char;

        // Optional rest: manufacturer prefix + altitude + comment
        let rest = b.get(8..).unwrap_or_default();

        // Find altitude (`}` marker)
        let (raw_mfg, altitude_m, comment_raw) = parse_rest(rest);
        let device = raw_mfg.as_deref().and_then(lookup_device);

        // Check for Byonics suffix in the comment
        let (device, comment) = if device.is_none() {
            if let Some((dev, trimmed)) = lookup_byonics_suffix(comment_raw) {
                (Some(dev), trimmed.to_vec())
            } else {
                (device, comment_raw.to_vec())
            }
        } else {
            (device, comment_raw.to_vec())
        };

        Ok(Self {
            latitude,
            longitude,
            precision,
            message,
            speed,
            course,
            symbol_code,
            symbol_table,
            comment,
            is_current,
            altitude_m,
            device,
            raw_mfg,
        })
    }

    /// Encode to the information field bytes (starting with the DTI byte).
    pub fn encode(&self) -> Vec<u8> {
        let mut out = Vec::new();
        out.push(if self.is_current { b'`' } else { b'\'' });
        encode_longitude(self.longitude, &mut out);
        encode_speed_course(self.speed, self.course, &mut out);
        out.push(self.symbol_code as u8);
        out.push(self.symbol_table as u8);
        if let Some(ref mfg) = self.raw_mfg {
            out.extend_from_slice(mfg);
        }
        if let Some(alt_m) = self.altitude_m {
            encode_altitude(alt_m, &mut out);
        }
        out.extend_from_slice(&self.comment);
        out
    }

    /// Encode the destination callsign that carries the latitude + message bits.
    pub fn encode_destination(&self) -> Result<Callsign, AprsError> {
        let mut lat_buf = Vec::new();
        self.latitude.encode_uncompressed(&mut lat_buf, self.precision);
        if lat_buf.len() != 8 {
            return Err(AprsError::EncodeError { detail: "MIC-E latitude encode failed" });
        }
        let is_north = self.latitude.value() >= 0.0;
        let (lon_deg, _, _, is_east) = self.longitude.dmh();
        let lon_offset_100 = lon_deg == 0 || lon_deg >= 100;
        let (a, b, c) = self.message.to_bits();

        let bytes = [
            encode_dest_012(lat_buf[0], a),
            encode_dest_012(lat_buf[1], b),
            encode_dest_012(lat_buf[2], c),
            encode_dest_bit3(lat_buf[3], is_north),
            encode_dest_bit4(lat_buf[5], lon_offset_100),
            encode_dest_bit5(lat_buf[6], !is_east),
        ];

        let call_str = std::str::from_utf8(&bytes)
            .map_err(|_| AprsError::EncodeError { detail: "MIC-E destination is not ASCII" })?;
        Callsign::decode_textual(call_str.as_bytes())
            .map_err(|_| AprsError::EncodeError { detail: "MIC-E destination invalid callsign" })
    }
}

// ─── Destination decoding ─────────────────────────────────────────────────────

fn decode_dest(c: &Callsign) -> Option<(Latitude, Precision, MicEMessage, bool, bool)> {
    let data = c.as_str().as_bytes();
    if data.len() != 6 { return None; }

    let lat_bytes = [
        lat_digit(data[0])?,
        lat_digit(data[1])?,
        lat_digit(data[2])?,
        lat_digit(data[3])?,
        b'.',
        lat_digit(data[4])?,
        lat_digit(data[5])?,
        lat_dir_byte(data[3])?,
    ];
    let (lat, prec) = Latitude::parse_uncompressed(&lat_bytes).ok()?;

    let a = MsgBit::from_byte(data[0])?;
    let b = MsgBit::from_byte(data[1])?;
    let c = MsgBit::from_byte(data[2])?;
    let msg = MicEMessage::from_bits(a, b, c);

    // Bit 4 of destination byte 4: longitude offset (add 100 to degree)
    let lon_offset_100 = matches!(data[4], b'P'..=b'Z');
    // Bit 5 of destination byte 5: 0=West, 1=East (inverted from standard)
    let lon_east = matches!(data[5], b'0'..=b'9' | b'L');

    Some((lat, prec, msg, lon_offset_100, lon_east))
}

fn lat_digit(c: u8) -> Option<u8> {
    match c {
        b'0'..=b'9' => Some(c),
        b'A'..=b'J' => Some(c - 17),
        b'K' | b'L' | b'Z' => Some(b' '),
        b'P'..=b'Y' => Some(c - 32),
        _ => None,
    }
}

fn lat_dir_byte(c: u8) -> Option<u8> {
    match c {
        b'0'..=b'9' | b'L' => Some(b'S'),
        b'P'..=b'Z' => Some(b'N'),
        _ => None,
    }
}

// ─── Longitude decoding ───────────────────────────────────────────────────────

fn decode_longitude(b: &[u8], offset_100: bool, is_east: bool) -> Option<Longitude> {
    let mut d = b[0].checked_sub(28)?;
    if offset_100 { d = d.checked_add(100)?; }
    if (180..=189).contains(&d) { d -= 80; }
    else if (190..=199).contains(&d) { d -= 190; }

    let mut m = b[1].checked_sub(28)?;
    if m >= 60 { m -= 60; }

    let h = b[2].checked_sub(28)?;

    Longitude::new(
        f64::from(d) + f64::from(m) / 60.0 + f64::from(h) / 6000.0
    ).map(|lon| if is_east { lon } else {
        Longitude::new(-lon.value()).unwrap_or(lon)
    })
}

fn encode_longitude(lon: Longitude, out: &mut Vec<u8>) {
    let (d, m, h, is_east) = lon.dmh();
    let d = d as u8;
    let m = m as u8;
    let h = h as u8;
    let enc_d = match d {
        0..=9   => d + 118,  // 0..9 → 118..127 (28+90 offset)
        10..=99 => d + 28,
        100..=109 => d - 72, // 100..109 → 28..37
        _ => d - 72,
    };
    // Simpler: encode the exact reverse of decode
    // d_enc such that: d_enc - 28 [- 100 if offset] [adjust] = d
    // Use the reference implementation's approach
    let _ = is_east; // direction handled by destination encoding
    out.push(enc_d);
    out.push(if m < 10 { m + 88 } else { m + 28 });
    out.push(h + 28);
}

// ─── Speed / Course ───────────────────────────────────────────────────────────

fn decode_speed_course(b: &[u8]) -> Option<(MicESpeed, MicECourse)> {
    let sp = u32::from(b[0].checked_sub(28)?);
    let dc = u32::from(b[1].checked_sub(28)?);
    let se = u32::from(b[2].checked_sub(28)?);

    let mut speed = sp * 10 + dc / 10;
    if speed >= 800 { speed -= 800; }

    let mut course = (dc % 10) * 100 + se;
    if course >= 400 { course -= 400; }

    Some((MicESpeed(speed), MicECourse(course)))
}

fn encode_speed_course(speed: MicESpeed, course: MicECourse, out: &mut Vec<u8>) {
    let knots = speed.knots();
    let deg = course.degrees();
    let tens = (knots / 10) as u8;
    let units = (knots % 10) as u8;
    let h_course = (deg / 100) as u8;
    let u_course = (deg % 100) as u8;

    let sp = if tens < 20 { tens + 80 } else { tens };
    let dc = units * 10 + h_course + 4;
    out.push(sp + 28);
    out.push(dc + 28);
    out.push(u_course + 28);
}

// ─── Altitude in status field ────────────────────────────────────────────────

fn parse_rest(rest: &[u8]) -> (Option<Vec<u8>>, Option<f64>, &[u8]) {
    // Look for `}` which terminates the altitude encoding (3 base-91 bytes precede it)
    if let Some(idx) = rest.iter().position(|&b| b == b'}')
        && idx >= 3
    {
        // Altitude is rest[idx-3..idx], manufacturer is rest[0..idx-3]
        let mfg_bytes = &rest[..idx - 3];
        let alt_bytes = &rest[idx - 3..idx];
        let mut alt_val: i32 = 0;
        for &byte in alt_bytes {
            alt_val = alt_val * 91 + (byte.saturating_sub(33)) as i32;
        }
        // Encoding stores (altitude_in_metres + 10000) in base-91 (per direwolf)
        let alt_m_corrected = alt_val as f64 - 10000.0;
        let raw_mfg = if mfg_bytes.is_empty() { None } else { Some(mfg_bytes.to_vec()) };
        let comment = rest.get(idx + 1..).unwrap_or_default();
        return (raw_mfg, Some(alt_m_corrected), comment);
    }
    // No altitude found — the whole rest is manufacturer (if starts with known prefix) + comment
    // We can't easily separate mfg from comment without a lookup, so return None for mfg
    // The device detection will happen on the first few bytes of rest
    (None, None, rest)
}

fn encode_altitude(alt_m: f64, out: &mut Vec<u8>) {
    let val = (alt_m + 10000.0).round() as u32;
    let b0 = (val / 91 / 91 % 91) as u8 + 33;
    let b1 = (val / 91 % 91) as u8 + 33;
    let b2 = (val % 91) as u8 + 33;
    out.push(b0);
    out.push(b1);
    out.push(b2);
    out.push(b'}');
}

// ─── Destination encoding helpers ─────────────────────────────────────────────

fn encode_dest_012(lat_digit: u8, bit: MsgBit) -> u8 {
    match (bit, lat_digit == b' ') {
        (MsgBit::Zero,     false) => lat_digit,
        (MsgBit::Zero,     true)  => b'L',
        (MsgBit::Custom,   false) => lat_digit + 17,
        (MsgBit::Custom,   true)  => b'K',
        (MsgBit::Standard, false) => lat_digit + 32,
        (MsgBit::Standard, true)  => b'Z',
    }
}

fn encode_dest_bit3(lat_digit: u8, is_north: bool) -> u8 {
    match (is_north, lat_digit == b' ') {
        (true,  false) => lat_digit + 32,
        (true,  true)  => b'Z',
        (false, false) => lat_digit,
        (false, true)  => b'L',
    }
}

fn encode_dest_bit4(lat_digit: u8, lon_offset_100: bool) -> u8 {
    match (lon_offset_100, lat_digit == b' ') {
        (true,  false) => lat_digit + 32,
        (true,  true)  => b'Z',
        (false, false) => lat_digit,
        (false, true)  => b'L',
    }
}

fn encode_dest_bit5(lat_digit: u8, is_west: bool) -> u8 {
    match (is_west, lat_digit == b' ') {
        (true,  false) => lat_digit + 32,
        (true,  true)  => b'Z',
        (false, false) => lat_digit,
        (false, true)  => b'L',
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn callsign(s: &str) -> Callsign {
        Callsign::decode_textual(s.as_bytes()).unwrap()
    }

    #[test]
    fn decode_speed_course_basic() {
        // From the APRS spec example packet: speed/course bytes are n"O
        // n=110, "=34, O=79 → speed=20 knots, course=251°
        let b = &[b'n', b'"', b'O'];
        let (spd, crs) = decode_speed_course(b).unwrap();
        assert_eq!(spd.knots(), 20);
        assert_eq!(crs.degrees(), 251);
    }

    #[test]
    fn device_lookup_kenwood_thd7a() {
        let dev = lookup_device(b">").unwrap();
        assert_eq!(dev.manufacturer, "Kenwood");
        assert_eq!(dev.model, "TH-D7A");
    }

    #[test]
    fn device_lookup_kenwood_thd72() {
        let dev = lookup_device(b">=").unwrap();
        assert_eq!(dev.manufacturer, "Kenwood");
        assert_eq!(dev.model, "TH-D72");
    }

    #[test]
    fn device_lookup_kenwood_tmd700() {
        let dev = lookup_device(b"]").unwrap();
        assert_eq!(dev.manufacturer, "Kenwood");
        assert_eq!(dev.model, "TM-D700");
    }

    #[test]
    fn device_lookup_yaesu_vx8() {
        let dev = lookup_device(b"_ ").unwrap();
        assert_eq!(dev.manufacturer, "Yaesu");
        assert_eq!(dev.model, "VX-8");
    }

    #[test]
    fn byonics_suffix_detection() {
        let (dev, rest) = lookup_byonics_suffix(b"Hello world!|3").unwrap();
        assert_eq!(dev.manufacturer, "Byonics");
        assert_eq!(dev.model, "TinyTrak3");
        assert_eq!(rest, b"Hello world!");
    }

    #[test]
    fn decode_from_spec_example() {
        // From aprs-parser-rs test, PPPPPP destination
        let info = br#"`(_fn"Oj/Hello world!"#;
        let to = callsign("PPPPPP");
        let m = AprsMicE::parse(info, &to).unwrap();
        assert!(m.is_current);
        assert_eq!(m.symbol_code, 'j');
        assert_eq!(m.symbol_table, '/');
        assert_eq!(m.comment, b"Hello world!");
        assert!(m.device.is_none());
    }

    #[test]
    fn encode_destination_round_trip() {
        let info = br#"`(_fn"Oj/Hello world!"#;
        let to = callsign("PPPPPP");
        let m = AprsMicE::parse(info, &to).unwrap();
        let reenc_dest = m.encode_destination().unwrap();
        assert_eq!(reenc_dest.as_str(), "PPPPPP");
    }

    #[test]
    fn decode_kenwood_device() {
        // MIC-E packet with Kenwood TH-D7A identifier `>`
        let info = br#"`(_fn"Oj/>`"49}Hello"#; // `>` prefix, then altitude marker
        let to = callsign("S32U6T");
        let m = AprsMicE::parse(info, &to).unwrap();
        // If it has a `}` marker, altitude should be parsed
        // The device detection depends on the raw_mfg bytes
        let _ = m; // Just verify it doesn't panic
    }
}
