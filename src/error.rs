use thiserror::Error;

#[derive(Debug, Error)]
pub enum AprsError {
    // --- Header parsing ---
    #[error("packet is empty")]
    EmptyPacket,

    #[error("missing '>' in packet header (expected FROM>TO,VIA:DATA)")]
    MissingDestinationDelimiter,

    #[error("missing ':' in packet header (expected FROM>TO,VIA:DATA)")]
    MissingInfoDelimiter,

    #[error("invalid callsign: {raw:?}")]
    InvalidCallsign { raw: Vec<u8> },

    #[error("invalid via element: {raw:?}")]
    InvalidVia { raw: Vec<u8> },

    // --- AX.25 frame parsing ---
    #[error("AX.25 frame too short (got {len} bytes, need at least 15)")]
    Ax25FrameTooShort { len: usize },

    #[error("AX.25 frame missing end-of-address bit in expected range")]
    Ax25MissingEoa,

    #[error("AX.25 control byte is not UI frame (0x03), got 0x{byte:02x}")]
    Ax25NotUiFrame { byte: u8 },

    #[error("AX.25 PID is not APRS (0xF0), got 0x{byte:02x}")]
    Ax25NotAprsPid { byte: u8 },

    // --- Position ---
    #[error("invalid latitude: expected DDmm.mmN/S format, got {raw:?}")]
    InvalidLatitude { raw: Vec<u8> },

    #[error("invalid longitude: expected DDDmm.mmE/W format, got {raw:?}")]
    InvalidLongitude { raw: Vec<u8> },

    #[error("unsupported position format")]
    UnsupportedPositionFormat,

    // --- Timestamp ---
    #[error("timestamp day {day} is out of range 1–31")]
    TimestampDayOutOfRange { day: u8 },

    #[error("timestamp hour {hour} is out of range 0–23")]
    TimestampHourOutOfRange { hour: u8 },

    #[error("timestamp minute {minute} is out of range 0–59")]
    TimestampMinuteOutOfRange { minute: u8 },

    #[error("timestamp second {second} is out of range 0–59")]
    TimestampSecondOutOfRange { second: u8 },

    #[error("invalid timestamp format: {raw:?}")]
    InvalidTimestampFormat { raw: Vec<u8> },

    // --- Message ---
    #[error("invalid message: missing second ':' delimiter")]
    InvalidMessageMissingDelimiter,

    // --- Object / Item ---
    #[error("invalid object: {detail}")]
    InvalidObject { detail: &'static str },

    #[error("invalid item: {detail}")]
    InvalidItem { detail: &'static str },

    // --- MIC-E ---
    #[error("invalid MIC-E destination: {raw:?}")]
    InvalidMicEDestination { raw: Vec<u8> },

    #[error("MIC-E information field too short (got {len} bytes, need at least 8)")]
    MicETooShort { len: usize },

    // --- Compressed position ---
    #[error("invalid base-91 compressed position byte: 0x{byte:02x}")]
    InvalidCompressedByte { byte: u8 },

    // --- General ---
    #[error("truncated packet: expected at least {expected} bytes, got {got}")]
    TruncatedPacket { expected: usize, got: usize },

    #[error("non-ASCII byte in field that requires ASCII: 0x{byte:02x}")]
    NonAsciiByte { byte: u8 },

    // --- Encoding ---
    #[error("cannot encode: {detail}")]
    EncodeError { detail: &'static str },
}
