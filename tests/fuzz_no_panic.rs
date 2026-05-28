/// Fuzz-style no-panic test suite.
///
/// Invariant: for any byte slice input, both `decode_textual` and `decode_ax25`
/// must return `Ok(_)` or `Err(_)` — they must **never** panic.
///
/// These test cases cover: empty input, all-zero bytes, all-0xFF bytes,
/// valid headers with garbage data fields, truncated packets, binary bytes
/// in the info field, and crafted edge cases for each packet type.
use aprs_decode::AprsPacket;

fn no_panic_textual(input: &[u8]) {
    let _ = AprsPacket::decode_textual(input);
}

fn no_panic_ax25(input: &[u8]) {
    let _ = AprsPacket::decode_ax25(input);
}

// ─── Raw byte inputs ──────────────────────────────────────────────────────────

#[test] fn empty()           { no_panic_textual(b""); no_panic_ax25(b""); }
#[test] fn single_byte()     { no_panic_textual(b"!"); no_panic_ax25(b"!"); }
#[test] fn all_zeros()       { no_panic_textual(&[0u8; 100]); no_panic_ax25(&[0u8; 100]); }
#[test] fn all_0xff()        { no_panic_textual(&[0xFFu8; 100]); no_panic_ax25(&[0xFFu8; 100]); }
#[test] fn all_colons()      { no_panic_textual(&[b':'; 50]); }
#[test] fn random_printable() {
    let input: Vec<u8> = (33u8..=126).cycle().take(200).collect();
    no_panic_textual(&input);
}

// ─── Malformed headers ────────────────────────────────────────────────────────

#[test] fn no_arrow()                { no_panic_textual(b"W1AW:!data"); }
#[test] fn no_colon()                { no_panic_textual(b"W1AW>APRS"); }
#[test] fn empty_from()              { no_panic_textual(b">APRS:!data"); }
#[test] fn invalid_callsign_chars()  { no_panic_textual(b"W1!@>APRS:!data"); }
#[test] fn ssid_too_large()          { no_panic_textual(b"W1AW-99>APRS:!data"); }
#[test] fn no_info_field()           { no_panic_textual(b"W1AW>APRS:"); }
#[test] fn colon_in_via()            { no_panic_textual(b"W1AW>APRS,:data"); }
#[test] fn unicode_bytes_in_header() { no_panic_textual("W1ÄW>APRS:!data".as_bytes()); }

// ─── Truncated info fields ────────────────────────────────────────────────────

#[test] fn position_truncated_0()    { no_panic_textual(b"W1AW>APRS:!"); }
#[test] fn position_truncated_5()    { no_panic_textual(b"W1AW>APRS:!4903."); }
#[test] fn position_truncated_18()   { no_panic_textual(b"W1AW>APRS:!4903.50N/07201.75"); }
#[test] fn position_compressed_truncated() { no_panic_textual(b"W1AW>APRS:!/ABCD"); }
#[test] fn message_no_second_colon() { no_panic_textual(b"W1AW>APRS::W1AW-9   "); }
#[test] fn message_too_short()       { no_panic_textual(b"W1AW>APRS::W1AW"); }
#[test] fn object_truncated_name()   { no_panic_textual(b"W1AW>APRS:;OBJ"); }
#[test] fn object_no_timestamp()     { no_panic_textual(b"W1AW>APRS:;OBJ      *"); }
#[test] fn item_too_short()          { no_panic_textual(b"W1AW>APRS:)AB"); }
#[test] fn weather_truncated()       { no_panic_textual(b"W1AW>APRS:_1007182"); }
#[test] fn telemetry_no_hash()       { no_panic_textual(b"W1AW>APRS:Tno hash here"); }
#[test] fn grid_no_bracket()         { no_panic_textual(b"W1AW>APRS:[IO91SX"); }
#[test] fn grid_wrong_length()       { no_panic_textual(b"W1AW>APRS:[IO9]"); }
#[test] fn third_party_empty_inner() { no_panic_textual(b"W1AW>APRS:}"); }
#[test] fn third_party_invalid_inner(){ no_panic_textual(b"W1AW>APRS:}not-a-packet"); }
#[test] fn mic_e_short_info()        { no_panic_textual(b"W1AW>PPPPPP:`ABC"); }

// ─── Binary / non-printable bytes in info field ───────────────────────────────

#[test]
fn binary_in_info() {
    let mut pkt = b"W1AW>APRS:".to_vec();
    pkt.extend_from_slice(&[0x00, 0x01, 0xFF, 0x7F, 0x80]);
    no_panic_textual(&pkt);
}

#[test]
fn mic_e_dti_0x1c() {
    let mut pkt = b"W1AW>PPPPPP:".to_vec();
    pkt.push(0x1C); // Old MIC-E DTI
    pkt.extend_from_slice(&[0x00; 20]);
    no_panic_textual(&pkt);
}

#[test]
fn null_bytes_everywhere() {
    let mut pkt = b"W1AW>APRS:!".to_vec();
    pkt.extend_from_slice(&[0x00; 50]);
    no_panic_textual(&pkt);
}

// ─── Valid-looking but semantically invalid ───────────────────────────────────

#[test] fn invalid_timestamp_day_0()   { no_panic_textual(b"W1AW>APRS:/002345z4903.50N/07201.75W-"); }
#[test] fn invalid_timestamp_hour_24() { no_panic_textual(b"W1AW>APRS:/092460z4903.50N/07201.75W-"); }
#[test] fn invalid_lat_direction()     { no_panic_textual(b"W1AW>APRS:!4903.50X/07201.75W-"); }
#[test] fn invalid_lon_direction()     { no_panic_textual(b"W1AW>APRS:!4903.50N/07201.75X-"); }
#[test] fn lat_all_spaces()            { no_panic_textual(b"W1AW>APRS:!       N/07201.75W-"); }
#[test] fn lon_out_of_range()          { no_panic_textual(b"W1AW>APRS:!9999.99N/99999.99W-"); }
#[test] fn base91_out_of_range()       { no_panic_textual(b"W1AW>APRS:!/\x00\x00\x00\x00\x00\x00\x00\x00- sT"); }

// ─── Each DTI with garbage data ───────────────────────────────────────────────

fn garbage_pkt(dti: u8) -> Vec<u8> {
    let mut p = b"W1AW>APRS:".to_vec();
    p.push(dti);
    p.extend_from_slice(&[b'X'; 30]);
    p
}

#[test] fn dti_bang_garbage()         { no_panic_textual(&garbage_pkt(b'!')); }
#[test] fn dti_equals_garbage()       { no_panic_textual(&garbage_pkt(b'=')); }
#[test] fn dti_slash_garbage()        { no_panic_textual(&garbage_pkt(b'/')); }
#[test] fn dti_at_garbage()           { no_panic_textual(&garbage_pkt(b'@')); }
#[test] fn dti_colon_garbage()        { no_panic_textual(&garbage_pkt(b':')); }
#[test] fn dti_gt_garbage()           { no_panic_textual(&garbage_pkt(b'>')); }
#[test] fn dti_backtick_garbage()     { no_panic_textual(&garbage_pkt(b'`')); }
#[test] fn dti_semicolon_garbage()    { no_panic_textual(&garbage_pkt(b';')); }
#[test] fn dti_rparen_garbage()       { no_panic_textual(&garbage_pkt(b')')); }
#[test] fn dti_underscore_garbage()   { no_panic_textual(&garbage_pkt(b'_')); }
#[test] fn dti_T_garbage()            { no_panic_textual(&garbage_pkt(b'T')); }
#[test] fn dti_lt_garbage()           { no_panic_textual(&garbage_pkt(b'<')); }
#[test] fn dti_question_garbage()     { no_panic_textual(&garbage_pkt(b'?')); }
#[test] fn dti_lbracket_garbage()     { no_panic_textual(&garbage_pkt(b'[')); }
#[test] fn dti_dollar_garbage()       { no_panic_textual(&garbage_pkt(b'$')); }
#[test] fn dti_rcurly_garbage()       { no_panic_textual(&garbage_pkt(b'}')); }
#[test] fn dti_lcurly_garbage()       { no_panic_textual(&garbage_pkt(b'{')); }
#[test] fn dti_unknown_garbage()      { no_panic_textual(&garbage_pkt(b'~')); }

// ─── AX.25 binary inputs ─────────────────────────────────────────────────────

#[test] fn ax25_too_short()           { no_panic_ax25(&[0u8; 10]); }
#[test] fn ax25_all_zeros_long()      { no_panic_ax25(&[0u8; 50]); }
#[test] fn ax25_wrong_control()       {
    let mut frame = vec![0u8; 14];
    frame[6] = 0x61;  // EOA bit set on dest
    frame[13] = 0x61; // EOA bit set on source
    frame.push(0x99); // wrong control byte
    frame.push(0xF0);
    no_panic_ax25(&frame);
}
#[test] fn ax25_wrong_pid() {
    let mut frame = vec![0u8; 14];
    frame[6] = 0x61;
    frame[13] = 0x61;
    frame.push(0x03);
    frame.push(0x01); // wrong PID
    no_panic_ax25(&frame);
}
