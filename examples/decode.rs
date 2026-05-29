//! Quick tour of aprs-decode: parse several packet types and round-trip through AX.25.
//!
//! Packets in mic_e(), timestamped_position(), and telemetry() are real frames
//! captured from a local APRS-IS feed.

use aprs_decode::{AprsData, AprsPacket};

fn main() {
    position_report();
    message_packet();
    status_packet();
    ax25_round_trip();
    mic_e();
    timestamped_position();
    telemetry();
}

fn position_report() {
    let raw = b"W1AW-9>APRS,WIDE1-1,WIDE2-2:!4903.50N/07201.75W-Relay station";
    let pkt = AprsPacket::decode_textual(raw).expect("valid position packet");

    print!("[position]  {} -> {}", pkt.from, pkt.to);
    for digi in &pkt.via {
        print!(",{digi}");
    }
    println!();

    if let AprsData::Position(pos) = &pkt.data {
        let lat = pos.position.latitude.value();
        let lon = pos.position.longitude.value();
        let sym = &pos.position.symbol;
        println!("            lat={lat:.4}  lon={lon:.4}  symbol={}/{}", sym.table, sym.code);
        if !pos.comment.is_empty() {
            println!("            comment={}", String::from_utf8_lossy(&pos.comment));
        }
    }
}

fn message_packet() {
    let raw = b"KD9ABC>APDR15,qAR,KD9XYZ::W1AW-9   :Hello world{042";
    let pkt = AprsPacket::decode_textual(raw).expect("valid message packet");

    if let AprsData::Message(msg) = &pkt.data {
        println!(
            "[message]   {} -> {}  text={}",
            pkt.from,
            String::from_utf8_lossy(&msg.addressee),
            String::from_utf8_lossy(&msg.text),
        );
    }
}

fn status_packet() {
    let raw = b"KD9ABC>APRS:>Net check-in / QRV 144.390";
    let pkt = AprsPacket::decode_textual(raw).expect("valid status packet");

    if let AprsData::Status(s) = &pkt.data {
        println!("[status]    {} says: {}", pkt.from, String::from_utf8_lossy(&s.comment));
    }
}

fn mic_e() {
    // Real packet from WA0DE-9 captured on local APRS-IS feed.
    // MIC-E encodes lat/msg-type in the destination callsign; lon/speed/course
    // are in the first 8 bytes of the info field.
    let raw = b"WA0DE-9>S9RSVQ,WIDE1-1,WIDE2-1:`pDM\x1c\x1fU#/\"J;}ARESDEC CommTrailer";
    let pkt = AprsPacket::decode_textual(raw).expect("valid MIC-E packet");

    if let AprsData::MicE(m) = &pkt.data {
        print!("[mic-e]     {} -> {}", pkt.from, pkt.to);
        for digi in &pkt.via { print!(",{digi}"); }
        println!();
        println!(
            "            lat={:.4}  lon={:.4}  speed={}kts  course={}°  msg={:?}",
            m.latitude.value(),
            m.longitude.value(),
            m.speed.knots(),
            m.course.degrees(),
            m.message,
        );
        if let Some(alt) = m.altitude_m {
            println!("            alt={alt:.0}m");
        }
        if let Some(dev) = &m.device {
            println!("            device={} {}", dev.manufacturer, dev.model);
        }
        if !m.comment.is_empty() {
            println!("            comment={}", String::from_utf8_lossy(&m.comment));
        }
    }
}

fn timestamped_position() {
    // Real packet from K5KTI-1: position report with a zulu timestamp.
    let raw = b"K5KTI-1>APMI06,WIDE2-2:@282245z3854.24N/10445.93W-WX3in1Plus2.0 U=13.9V,T=??.?C/??.?F";
    let pkt = AprsPacket::decode_textual(raw).expect("valid timestamped position");

    if let AprsData::Position(pos) = &pkt.data {
        let lat = pos.position.latitude.value();
        let lon = pos.position.longitude.value();
        print!("[position]  {} -> {}", pkt.from, pkt.to);
        for digi in &pkt.via { print!(",{digi}"); }
        println!();
        print!("            lat={lat:.4}  lon={lon:.4}");
        if let Some(ts) = &pos.timestamp {
            print!("  timestamp={ts:?}");
        }
        println!();
        if !pos.comment.is_empty() {
            println!("            comment={}", String::from_utf8_lossy(&pos.comment));
        }
    }
}

fn telemetry() {
    // Real packet from QUAIL: five analog channels + eight digital bits.
    let raw = b"QUAIL>APOT30,WD4IXD-12,BADGR,WIDE2:T#035,130,029,069,041,048,00000000";
    let pkt = AprsPacket::decode_textual(raw).expect("valid telemetry packet");

    if let AprsData::Telemetry(t) = &pkt.data {
        print!("[telemetry] {} -> {}", pkt.from, pkt.to);
        for digi in &pkt.via { print!(",{digi}"); }
        println!();
        print!("            seq={}  analog=", String::from_utf8_lossy(&t.sequence));
        for (i, ch) in t.analog.iter().enumerate() {
            if i > 0 { print!(","); }
            match ch {
                Some(v) => print!("{v}"),
                None    => print!("-"),
            }
        }
        println!("  digital={:08b}", t.digital);
    }
}

fn ax25_round_trip() {
    let original = b"W1AW-9>APRS,WIDE1-1:!4903.50N/07201.75W-";
    let pkt = AprsPacket::decode_textual(original).expect("decode textual");

    let frame = pkt.encode_ax25().expect("encode AX.25");
    let recovered = AprsPacket::decode_ax25(&frame).expect("decode AX.25");

    println!(
        "[ax25]      round-trip OK: {} bytes  from={} to={}",
        frame.len(),
        recovered.from,
        recovered.to,
    );
}
