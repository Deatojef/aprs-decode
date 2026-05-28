//! APRS weather data — both position-embedded and positionless reports.
//!
//! All fields use unit-aware newtypes with conversion methods so callers
//! never need to know the native APRS wire units.

use crate::error::AprsError;
use crate::util::parse_bytes;

// ─── Unit newtypes ────────────────────────────────────────────────────────────

/// Wind direction in degrees (0–360; 0 means unknown/variable).
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(transparent))]
pub struct WindDirection(pub u16);

impl WindDirection {
    pub fn degrees(self) -> u16 { self.0 }
}

/// Wind speed in statute miles per hour (APRS native unit).
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(transparent))]
pub struct WindSpeed(pub u16);

impl WindSpeed {
    pub fn mph(self) -> u16 { self.0 }
    pub fn knots(self) -> f32 { self.0 as f32 * 0.868_976 }
    pub fn kph(self) -> f32 { self.0 as f32 * 1.609_344 }
    pub fn m_per_s(self) -> f32 { self.0 as f32 * 0.447_04 }
}

/// Temperature in degrees Fahrenheit (APRS native unit).
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(transparent))]
pub struct Temperature(pub i16);

impl Temperature {
    pub fn fahrenheit(self) -> i16 { self.0 }
    pub fn celsius(self) -> f32 { (self.0 as f32 - 32.0) * 5.0 / 9.0 }
    pub fn kelvin(self) -> f32 { self.celsius() + 273.15 }
}

/// Rainfall in hundredths of an inch (APRS native unit).
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(transparent))]
pub struct Rainfall(pub u16);

impl Rainfall {
    pub fn hundredths_inch(self) -> u16 { self.0 }
    pub fn inches(self) -> f32 { self.0 as f32 / 100.0 }
    pub fn mm(self) -> f32 { self.inches() * 25.4 }
}

/// Relative humidity in percent (0–100; the wire value `00` means 100%).
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(transparent))]
pub struct Humidity(pub u8);

impl Humidity {
    pub fn percent(self) -> u8 { self.0 }
}

/// Barometric pressure in tenths of a millibar (= hundredths of hPa).
///
/// Example: 10250 → 1025.0 hPa.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(transparent))]
pub struct Pressure(pub u32);

impl Pressure {
    pub fn tenths_mbar(self) -> u32 { self.0 }
    pub fn hpa(self) -> f32 { self.0 as f32 / 10.0 }
    pub fn mbar(self) -> f32 { self.hpa() }
}

/// Solar luminosity in watts per square metre.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(transparent))]
pub struct Luminosity(pub u16);

impl Luminosity {
    pub fn w_per_m2(self) -> u16 { self.0 }
}

/// Snowfall in tenths of an inch over the last 24 hours.
#[derive(Debug, Copy, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(transparent))]
pub struct Snowfall(pub f32);

impl Snowfall {
    pub fn tenths_inch(self) -> f32 { self.0 }
    pub fn inches(self) -> f32 { self.0 / 10.0 }
    pub fn cm(self) -> f32 { self.inches() * 2.54 }
}

// ─── AprsWeatherData ─────────────────────────────────────────────────────────

/// Weather data fields, parsed from either a position packet (symbol `/_`)
/// or a positionless weather report (DTI `_`).
///
/// Every field is an `Option` — not all transmitting stations send all fields.
/// Native APRS units are preserved; use the conversion methods on each type
/// to obtain SI or other values.
#[derive(Debug, Clone, PartialEq, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct AprsWeatherData {
    /// Wind direction (degrees, 0=unknown). Native: degrees.
    pub wind_direction: Option<WindDirection>,
    /// Sustained wind speed. Native: mph.
    pub wind_speed: Option<WindSpeed>,
    /// Peak wind gust in last 5 minutes. Native: mph.
    pub wind_gust: Option<WindSpeed>,
    /// Temperature. Native: °F.
    pub temperature: Option<Temperature>,
    /// Rainfall in the last hour. Native: hundredths of an inch.
    pub rain_last_hour: Option<Rainfall>,
    /// Rainfall in the last 24 hours. Native: hundredths of an inch.
    pub rain_last_24h: Option<Rainfall>,
    /// Rainfall since midnight. Native: hundredths of an inch.
    pub rain_since_midnight: Option<Rainfall>,
    /// Relative humidity 0–100 (wire `00` = 100%). Native: percent.
    pub humidity: Option<Humidity>,
    /// Barometric pressure. Native: tenths of a millibar.
    pub barometric_pressure: Option<Pressure>,
    /// Solar radiation. Native: W/m².
    pub luminosity: Option<Luminosity>,
    /// Snowfall in the last 24 hours. Native: tenths of an inch.
    pub snow_last_24h: Option<Snowfall>,
    /// Raw rain counter (implementation-specific).
    pub raw_rain_counter: Option<u16>,
}

impl AprsWeatherData {
    /// Parse the weather field block.
    ///
    /// Expected start: `DDD/SSS` (wind direction / wind speed), then lettered
    /// single-char fields: `g` `t` `r` `p` `P` `h` `b` `l` `L` `s` `#`.
    ///
    /// Example: `220/004g005t077r000p000P000h50b09900`
    pub fn parse(b: &[u8]) -> Result<Self, AprsError> {
        if b.len() < 7 {
            return Err(AprsError::TruncatedPacket { expected: 7, got: b.len() });
        }

        let wind_direction = parse_opt_u16(&b[0..3]).map(WindDirection);

        if b[3] != b'/' {
            return Err(AprsError::TruncatedPacket { expected: 7, got: b.len() });
        }

        let wind_speed = parse_opt_u16(&b[4..7]).map(WindSpeed);

        let mut wind_gust = None;
        let mut temperature = None;
        let mut rain_last_hour = None;
        let mut rain_last_24h = None;
        let mut rain_since_midnight = None;
        let mut humidity = None;
        let mut barometric_pressure = None;
        let mut luminosity = None;
        let mut snow_last_24h = None;
        let mut raw_rain_counter = None;

        let mut i = 7usize;
        while i < b.len() {
            let key = b[i];
            i += 1;
            match key {
                b'g' => { if i + 3 <= b.len() { wind_gust = parse_opt_u16(&b[i..i+3]).map(WindSpeed); i += 3; } }
                b't' => { if i + 3 <= b.len() { temperature = parse_opt_i16(&b[i..i+3]).map(Temperature); i += 3; } }
                b'r' => { if i + 3 <= b.len() { rain_last_hour = parse_opt_u16(&b[i..i+3]).map(Rainfall); i += 3; } }
                b'p' => { if i + 3 <= b.len() { rain_last_24h = parse_opt_u16(&b[i..i+3]).map(Rainfall); i += 3; } }
                b'P' => { if i + 3 <= b.len() { rain_since_midnight = parse_opt_u16(&b[i..i+3]).map(Rainfall); i += 3; } }
                b'h' => {
                    if i + 2 <= b.len() {
                        humidity = parse_opt_u16(&b[i..i+2]).map(|v| Humidity(if v == 0 { 100 } else { v as u8 }));
                        i += 2;
                    }
                }
                b'b' => {
                    if i + 5 <= b.len() {
                        barometric_pressure = parse_bytes::<u32>(&b[i..i+5]).map(Pressure);
                        i += 5;
                    }
                }
                b'L' => { if i + 3 <= b.len() { luminosity = parse_opt_u16(&b[i..i+3]).map(|v| Luminosity(v + 1000)); i += 3; } }
                b'l' => { if i + 3 <= b.len() { luminosity = parse_opt_u16(&b[i..i+3]).map(Luminosity); i += 3; } }
                b's' => {
                    if i + 3 <= b.len() {
                        snow_last_24h = parse_opt_u16(&b[i..i+3]).map(|v| Snowfall(v as f32 / 10.0));
                        i += 3;
                    }
                }
                b'#' => { if i + 3 <= b.len() { raw_rain_counter = parse_opt_u16(&b[i..i+3]); i += 3; } }
                _ => break, // unknown field — stop here, rest is comment
            }
        }

        Ok(Self {
            wind_direction,
            wind_speed,
            wind_gust,
            temperature,
            rain_last_hour,
            rain_last_24h,
            rain_since_midnight,
            humidity,
            barometric_pressure,
            luminosity,
            snow_last_24h,
            raw_rain_counter,
        })
    }

    /// Encode weather fields to bytes (without any header).
    pub fn encode(&self, out: &mut Vec<u8>) {
        match self.wind_direction {
            Some(d) => out.extend_from_slice(format!("{:03}", d.degrees()).as_bytes()),
            None    => out.extend_from_slice(b"..."),
        }
        out.push(b'/');
        match self.wind_speed {
            Some(s) => out.extend_from_slice(format!("{:03}", s.mph()).as_bytes()),
            None    => out.extend_from_slice(b"..."),
        }
        if let Some(g) = self.wind_gust         { out.extend_from_slice(format!("g{:03}", g.mph()).as_bytes()); }
        if let Some(t) = self.temperature        { out.extend_from_slice(format!("t{:03}", t.fahrenheit()).as_bytes()); }
        if let Some(r) = self.rain_last_hour     { out.extend_from_slice(format!("r{:03}", r.hundredths_inch()).as_bytes()); }
        if let Some(p) = self.rain_last_24h      { out.extend_from_slice(format!("p{:03}", p.hundredths_inch()).as_bytes()); }
        if let Some(p) = self.rain_since_midnight{ out.extend_from_slice(format!("P{:03}", p.hundredths_inch()).as_bytes()); }
        if let Some(h) = self.humidity {
            let v = if h.percent() == 100 { 0 } else { h.percent() };
            out.extend_from_slice(format!("h{:02}", v).as_bytes());
        }
        if let Some(b_val) = self.barometric_pressure { out.extend_from_slice(format!("b{:05}", b_val.tenths_mbar()).as_bytes()); }
        if let Some(l) = self.luminosity {
            if l.w_per_m2() >= 1000 {
                out.extend_from_slice(format!("L{:03}", l.w_per_m2() - 1000).as_bytes());
            } else {
                out.extend_from_slice(format!("l{:03}", l.w_per_m2()).as_bytes());
            }
        }
        if let Some(s) = self.snow_last_24h { out.extend_from_slice(format!("s{:03}", (s.tenths_inch() * 10.0) as u16).as_bytes()); }
        if let Some(r) = self.raw_rain_counter { out.extend_from_slice(format!("#{:03}", r).as_bytes()); }
    }
}

// ─── AprsPositionlessWeather ──────────────────────────────────────────────────

/// A positionless weather report. DTI: `_`.
///
/// Format: `_MMDDHHMM` (local-time timestamp) + weather fields.
/// The timestamp is stored as raw bytes since the MMDDHHMM format is distinct
/// from the DDHHMM/HHMMSS formats used in other APRS packets.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct AprsPositionlessWeather {
    /// 8-byte MMDDHHMM timestamp in local time.
    pub timestamp: Vec<u8>,
    pub weather: AprsWeatherData,
    pub comment: Vec<u8>,
}

impl AprsPositionlessWeather {
    /// Decode from the information field (including the leading `_` DTI byte).
    pub(crate) fn parse(info: &[u8]) -> Result<Self, AprsError> {
        // info[0] = '_', info[1..9] = MMDDHHMM (8 bytes), info[9..] = weather data
        if info.len() < 9 {
            return Err(AprsError::TruncatedPacket { expected: 9, got: info.len() });
        }
        let timestamp = info[1..9].to_vec();
        let weather_bytes = &info[9..];
        let weather = AprsWeatherData::parse(weather_bytes)?;
        Ok(Self { timestamp, weather, comment: vec![] })
    }

    pub fn encode(&self) -> Vec<u8> {
        let mut out = vec![b'_'];
        out.extend_from_slice(&self.timestamp);
        self.weather.encode(&mut out);
        out.extend_from_slice(&self.comment);
        out
    }
}

// ─── Helpers ─────────────────────────────────────────────────────────────────

/// Parse a 2–5 byte field as u16, returning None for all-spaces or all-dots.
fn parse_opt_u16(b: &[u8]) -> Option<u16> {
    if b.iter().all(|&c| c == b'.' || c == b' ') {
        return None;
    }
    parse_bytes(b)
}

/// Parse a 3-byte field as i16 (handles negative temperatures like `-10`).
fn parse_opt_i16(b: &[u8]) -> Option<i16> {
    if b.iter().all(|&c| c == b'.' || c == b' ') {
        return None;
    }
    parse_bytes(b)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_full_weather() {
        let wx = AprsWeatherData::parse(b"220/004g005t077r000p000P000h50b09900").unwrap();
        assert_eq!(wx.wind_direction.unwrap().degrees(), 220);
        assert_eq!(wx.wind_speed.unwrap().mph(), 4);
        assert_eq!(wx.wind_gust.unwrap().mph(), 5);
        assert_eq!(wx.temperature.unwrap().fahrenheit(), 77);
        assert_eq!(wx.rain_last_hour.unwrap().hundredths_inch(), 0);
        assert_eq!(wx.humidity.unwrap().percent(), 50);
        assert_eq!(wx.barometric_pressure.unwrap().tenths_mbar(), 9900);
    }

    #[test]
    fn temperature_conversion() {
        let t = Temperature(32); // 32°F = 0°C
        assert!((t.celsius() - 0.0).abs() < 0.01);
        let t = Temperature(212); // 212°F = 100°C
        assert!((t.celsius() - 100.0).abs() < 0.01);
    }

    #[test]
    fn wind_speed_conversion() {
        let s = WindSpeed(10); // 10 mph
        assert!((s.knots() - 8.68976).abs() < 0.001);
        assert!((s.kph() - 16.09344).abs() < 0.001);
    }

    #[test]
    fn pressure_conversion() {
        let p = Pressure(10250);
        assert!((p.hpa() - 1025.0).abs() < 0.01);
    }

    #[test]
    fn rainfall_conversion() {
        let r = Rainfall(100); // 100 hundredths = 1.00 inch
        assert!((r.inches() - 1.0).abs() < 0.001);
        assert!((r.mm() - 25.4).abs() < 0.01);
    }

    #[test]
    fn humidity_100_encoded_as_00() {
        let wx = AprsWeatherData::parse(b"000/000h00").unwrap();
        assert_eq!(wx.humidity.unwrap().percent(), 100);
    }

    #[test]
    fn negative_temperature() {
        let wx = AprsWeatherData::parse(b"000/000g000t-10").unwrap();
        assert_eq!(wx.temperature.unwrap().fahrenheit(), -10);
    }

    #[test]
    fn luminosity_high() {
        let wx = AprsWeatherData::parse(b"000/000L042").unwrap();
        assert_eq!(wx.luminosity.unwrap().w_per_m2(), 1042);
    }

    #[test]
    fn unknown_fields_stop_parsing() {
        // After an unknown field letter, parsing stops; rest is treated as comment
        let wx = AprsWeatherData::parse(b"220/004g005XUNKNOWN").unwrap();
        assert_eq!(wx.wind_direction.unwrap().degrees(), 220);
        assert_eq!(wx.wind_gust.unwrap().mph(), 5);
        assert!(wx.temperature.is_none());
    }

    #[test]
    fn encode_round_trip() {
        let raw = b"220/004g005t077r000p000P000h50b09900";
        let wx = AprsWeatherData::parse(raw).unwrap();
        let mut out = Vec::new();
        wx.encode(&mut out);
        assert_eq!(out.as_slice(), raw.as_slice());
    }

    #[test]
    fn positionless_parse() {
        let pw = AprsPositionlessWeather::parse(b"_10071820220/004g005t077").unwrap();
        assert_eq!(pw.timestamp, b"10071820");
        assert_eq!(pw.weather.wind_direction.unwrap().degrees(), 220);
        assert_eq!(pw.weather.temperature.unwrap().fahrenheit(), 77);
    }

    #[test]
    fn positionless_encode_round_trip() {
        let raw = b"_10071820220/004g005t077";
        let pw = AprsPositionlessWeather::parse(raw).unwrap();
        assert_eq!(pw.encode().as_slice(), raw.as_slice());
    }
}
