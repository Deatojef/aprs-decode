# aprs-decode

A Rust library for parsing and encoding APRS (Automatic Packet Reporting System) packets.

Two input formats are supported:

- **Text (APRS-IS)** — the human-readable `FROM>TO,VIA:DATA` string format used by internet servers and log files
- **Binary (AX.25)** — raw UI frame bytes as received from a TNC or software modem

Both formats round-trip: every decoded packet can be re-encoded back to the original wire representation.

---

## Adding to your project

```toml
[dependencies]
aprs-decode = { path = "..." }          # local path while the crate is unpublished

# Enable JSON/serde support (optional):
aprs-decode = { path = "...", features = ["serde"] }
```

---

## Quick example

```rust
use aprs_decode::{AprsData, AprsPacket};

let raw = b"W1AW-9>APRS,WIDE1-1,WIDE2-2:!4903.50N/07201.75W-Home station";
let pkt = AprsPacket::decode_textual(raw)?;

println!("from: {}  to: {}", pkt.from, pkt.to);

if let AprsData::Position(pos) = &pkt.data {
    println!("lat: {:.4}  lon: {:.4}",
        pos.position.latitude.value(),
        pos.position.longitude.value());
    println!("symbol: {}/{}", pos.position.symbol.table, pos.position.symbol.code);
}
```

A runnable tour of all packet types is in [`examples/decode.rs`](examples/decode.rs):

```
cargo run --example decode
```

---

## Core API

### `AprsPacket`

The top-level type. Every decoded packet is an `AprsPacket`.

```rust
pub struct AprsPacket {
    pub from: Callsign,
    pub to:   Callsign,
    pub via:  Vec<Digipeater>,
    pub data: AprsData,
}
```

| Method | Description |
|---|---|
| `AprsPacket::decode_textual(&[u8])` | Parse an APRS-IS text frame |
| `AprsPacket::decode_ax25(&[u8])` | Parse a raw AX.25 UI frame |
| `pkt.encode_textual()` | Re-encode to APRS-IS text → `Vec<u8>` |
| `pkt.encode_ax25()` | Re-encode to AX.25 binary → `Vec<u8>` |

Both decode methods accept `&[u8]` rather than `&str` because APRS information fields can
contain arbitrary bytes (MIC-E in particular uses `\x1c` and `\x1d`).

### Decoding from AX.25

```rust
// bytes from a TNC, soundmodem, or rtl-sdr + multimon-ng
let pkt = AprsPacket::decode_ax25(&frame_bytes)?;
```

### Re-encoding

```rust
let pkt = AprsPacket::decode_textual(raw)?;

// convert text → binary
let ax25_frame = pkt.encode_ax25()?;

// convert binary → text
let pkt2 = AprsPacket::decode_ax25(&ax25_frame)?;
let text  = pkt2.encode_textual()?;
```

---

## Packet types — `AprsData`

The `data` field is an enum dispatched by the Data Type Indicator (DTI) — the first byte
of the information field.

### Position — `AprsData::Position(AprsPosition)`

DTI: `!` `=` `/` `@`

The most common packet type. Reports a station's geographic location.

```rust
pub struct AprsPosition {
    pub timestamp:           Option<Timestamp>,
    pub messaging_supported: bool,
    pub position:            Position,
    pub extension:           Option<Extension>,
    pub weather:             Option<AprsWeatherData>,  // set when symbol is /_
    pub frequency_mhz:       Option<f32>,
    pub comment:             Vec<u8>,
}
```

- `!` / `=` — no timestamp; `=` additionally signals that the station can receive messages
- `/` / `@` — timestamp present; `@` additionally signals messaging support
- `position.latitude.value()` and `position.longitude.value()` return decimal degrees as `f64`
- `position.symbol` identifies the map icon (see [Symbols](#symbols))
- `extension` may contain course/speed, PHG (power-height-gain), RNG, or DFS data
- `weather` is populated when the symbol is `/_` (weather station); see [Weather](#weather-aprsdata-weather)

### MIC-E — `AprsData::MicE(AprsMicE)`

DTI: `` ` `` `'` `\x1c` `\x1d`

A compact encoding used by Kenwood and Yaesu HTs and mobiles. Latitude and message type are
encoded in the destination callsign; longitude, speed, and course are in the first 8 bytes of
the information field.

```rust
pub struct AprsMicE {
    pub latitude:     Latitude,
    pub longitude:    Longitude,
    pub precision:    Precision,
    pub message:      MicEMessage,   // M0–M6, C0–C6, Emergency, Unknown
    pub speed:        MicESpeed,     // .knots()
    pub course:       MicECourse,    // .degrees()
    pub symbol_code:  char,
    pub symbol_table: char,
    pub altitude_m:   Option<f64>,
    pub device:       Option<MicEDevice>,  // manufacturer + model if recognized
    pub comment:      Vec<u8>,
    pub is_current:   bool,          // true = current position, false = old
}
```

**Important:** MIC-E packets frequently contain non-printable bytes. Copy/pasting from web
tools like aprs.fi silently drops them, producing garbage speed, course, and altitude values.
When embedding MIC-E frames in source code, use `\xNN` escape sequences for any byte outside
printable ASCII. Reading directly from an APRS-IS socket or a saved `.tnc2` log file gives
byte-accurate data.

### Message — `AprsData::Message(AprsMessage)`

DTI: `:`

Directed messages, bulletins, ACKs, REJs, and telemetry metadata.

```rust
pub struct AprsMessage {
    pub addressee: Vec<u8>,        // destination call, trimmed of padding
    pub text:      Vec<u8>,
    pub subtype:   MessageSubtype,
}
```

`MessageSubtype` distinguishes:

| Variant | Meaning |
|---|---|
| `Directed { id }` | Directed message; `id` is the optional message number |
| `Ack { id }` | Acknowledgement |
| `Rej { id }` | Rejection |
| `Bulletin` | General bulletin (addressee starts with `BLN`) |
| `NwsBulletin` | NWS / weather alert bulletin |
| `TelemetryParm` / `TelemetryUnit` / `TelemetryEqns` / `TelemetryBits` | Telemetry metadata |
| `DirectedQuery` | Directed station query |

### Status — `AprsData::Status(AprsStatus)`

DTI: `>`

Free-text status message, optionally with a Maidenhead grid square or timestamp.

### Object — `AprsData::Object(AprsObject)`

DTI: `;`

Reports the location of something other than the sending station — a storm, event, or fixed
infrastructure. Objects have a 9-character name and a mandatory timestamp.

```rust
pub struct AprsObject {
    pub name:          Vec<u8>,
    pub live:          bool,       // false = object has been killed/removed
    pub timestamp:     Timestamp,
    pub position:      Position,
    pub extension:     Option<Extension>,
    pub frequency_mhz: Option<f32>,
    pub comment:       Vec<u8>,
}
```

### Item — `AprsData::Item(AprsItem)`

DTI: `)`

Similar to an Object but with a shorter name (1–9 characters) and no timestamp. Typically
used for points of interest.

### Weather — `AprsData::Weather(AprsPositionlessWeather)`

DTI: `_`

A positionless weather report (no coordinates). When a weather station transmits its position
*and* weather data simultaneously, the weather fields are embedded in a `Position` packet and
found in `AprsPosition::weather` instead.

Weather fields use unit-aware newtypes with conversion methods:

| Type | Native APRS unit | Conversion methods |
|---|---|---|
| `WindDirection` | degrees | `.degrees()` |
| `WindSpeed` | mph | `.mph()` `.knots()` `.kph()` `.m_per_s()` |
| `Temperature` | °F | `.fahrenheit()` `.celsius()` `.kelvin()` |
| `Rainfall` | 1/100 inch | `.hundredths_inch()` `.inches()` `.mm()` |
| `Humidity` | % | `.percent()` |
| `Pressure` | 1/10 mbar | `.tenths_mbar()` `.hpa()` `.mbar()` |

### Telemetry — `AprsData::Telemetry(AprsTelemetry)`

DTI: `T#`

Numeric sensor data from remote stations.

```rust
pub struct AprsTelemetry {
    pub sequence: Vec<u8>,          // sequence number (000–999)
    pub analog:   [Option<f32>; 5], // five analog channels; None = absent/unparseable
    pub digital:  u8,               // eight digital bits packed (bit 7 = channel 1)
    pub comment:  Vec<u8>,
}
```

To interpret the raw analog values with engineering units, the station must also transmit
telemetry metadata packets (`PARM.` / `UNIT.` / `EQNS.`), which arrive as
`AprsData::Message` with the appropriate `MessageSubtype`.

### Other types

| Variant | DTI | Description |
|---|---|---|
| `Capabilities(AprsCapabilities)` | `<` | Station capabilities list |
| `Query(AprsQuery)` | `?` | General network query |
| `GridLocator(AprsGridLocator)` | `[` | Maidenhead grid locator beacon |
| `Nmea(AprsNmea)` | `$` | Raw NMEA sentence pass-through |
| `ThirdParty(AprsThirdParty)` | `}` | Packet forwarded from another network |
| `UserDefined(AprsUserDefined)` | `{` | Experimental / application-specific |
| `Unknown { dti, data }` | any | Unrecognized DTI; raw bytes preserved |

The `Unknown` variant is intentional — unrecognized packets are not errors. The `dti` byte
and the full raw `data` are preserved for the caller to inspect.

---

## Supporting types

### `Callsign`

Stored as uppercase ASCII in a fixed inline buffer (no heap allocation). SSID range is 0–15.

```rust
pkt.from.as_str()      // "W1AW"
pkt.from.ssid          // Some(9)
pkt.from.to_string()   // "W1AW-9"
```

### `Digipeater`

Each element of `pkt.via` is one of:

- `Digipeater::Callsign(Callsign, bool)` — the `bool` is the "has been heard" flag (`*` suffix on wire)
- `Digipeater::QConstruct(QConstruct, Callsign)` — APRS-IS Q-construct paired with the IGate callsign

Common Q-constructs: `qAR` (bidirectional IGate), `qAO` (RF origin), `qAC` (verified login).

### `Timestamp`

```rust
Timestamp::Ddhhmm(day, hour, minute)  // "282245z" → Ddhhmm(28, 22, 45) UTC
Timestamp::Hhmmss(hour, minute, sec)  // "074849h" → Hhmmss(7, 48, 49) UTC
Timestamp::Unsupported(raw)           // local-time "/" suffix (deprecated in APRS101)
```

Note that `Ddhhmm` carries only the day-of-month, not the full date. The current month and
year must be inferred from wall-clock time by the application.

### `Position`

```rust
pub struct Position {
    pub latitude:  Latitude,
    pub longitude: Longitude,
    pub symbol:    Symbol,
    pub precision: Precision,
    pub altitude:  Option<Altitude>,  // present only in compressed position format
}
```

`Latitude::value()` and `Longitude::value()` return decimal degrees as `f64`
(negative = South / West).

`Precision` reflects how many digits were significant in the wire encoding:
`HundredthMinute` (full precision, ≈18 m) down to `TenDegree` (very coarse, ≈1100 km).

### Symbols

```rust
let sym = &pos.position.symbol;
sym.table             // '/' = primary table, '\\' = alternate, 'A'–'Z'/'0'–'9' = overlay
sym.code              // specific icon within the table
sym.description()     // Option<&'static str>, e.g. Some("Car"), Some("Weather Station")
sym.is_primary_table()
sym.is_alternate_table()
sym.overlay()         // Some('3') if an alphanumeric overlay, else None
```

### `Extension`

Optional 7-byte data extension that follows the position in the comment field:

```rust
Extension::DirectionSpeed { direction_degrees, speed_knots }
Extension::Phg { power_watts, antenna_height_feet, antenna_gain_db, directivity }
Extension::Rng { range_miles }
Extension::Dfs { s_points, antenna_height_feet, antenna_gain_db, directivity }
```

---

## Error handling

All decode functions return `Result<_, AprsError>`. `AprsError` implements `std::error::Error`
via [`thiserror`](https://docs.rs/thiserror) and has a structured variant for every failure mode.

```rust
match AprsPacket::decode_textual(raw) {
    Ok(pkt) => { /* use pkt */ }
    Err(e)  => eprintln!("parse error: {e}"),
}
```

Notable variants:

| Variant | Cause |
|---|---|
| `EmptyPacket` | Zero-length input |
| `MissingInfoDelimiter` | No `:` separating header from data |
| `MissingDestinationDelimiter` | No `>` separating source from destination |
| `InvalidCallsign` | Malformed callsign or SSID out of range 0–15 |
| `Ax25FrameTooShort` | Binary frame shorter than minimum AX.25 UI frame |
| `Ax25NotUiFrame` | Control byte is not `0x03` |
| `Ax25NotAprsPid` | PID byte is not `0xF0` |
| `TruncatedPacket` | Field shorter than its required length |
| `InvalidLatitude` / `InvalidLongitude` | Coordinate format error |
| `MicETooShort` | MIC-E info field shorter than 8 bytes |

---

## Optional features

| Feature | Effect |
|---|---|
| `serde` | Derives `Serialize` / `Deserialize` on all public types; `Callsign` serializes as a plain string |

```toml
aprs-decode = { path = "...", features = ["serde"] }
```

---

## APRS-IS connection note

To decode live packets, open a TCP connection to `rotate.aprs2.net:14580`, send a login line,
and read newline-delimited frames. Each line (excluding server comments that begin with `#`) is
a complete APRS-IS frame suitable for `AprsPacket::decode_textual`. Lines include a trailing
`\r\n` that the parser tolerates. Lines starting with `#` should be skipped — passing them to
`decode_textual` will return `Err(MissingInfoDelimiter)`.

---

## Specification references

- **APRS Protocol Reference 1.0.1** (APRS101.pdf) — the primary APRS specification
- **AX.25 Link Access Protocol for Amateur Packet Radio v2.2** — defines the binary frame format
- **APRS-IS Q-construct specification** — documents the `qAX` via tokens added by IGates

---

## License

Licensed under either of:

- **MIT License** ([LICENSE-MIT](LICENSE-MIT) or http://opensource.org/licenses/MIT)
- **Apache License, Version 2.0** ([LICENSE-APACHE](LICENSE-APACHE) or http://www.apache.org/licenses/LICENSE-2.0)

at your option.

