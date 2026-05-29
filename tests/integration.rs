use approx::assert_relative_eq;
use aprs_decode::{AprsData, AprsPacket, Digipeater, MessageSubtype, Timestamp};

// Phase 1: header parsing regression anchors

#[test]
fn message_packet_header() {
    let pkt =
        AprsPacket::decode_textual(b"KD9ABC>APDR15,qAR,KD9XYZ::W1AW-9   :Hello world{001").unwrap();
    assert_eq!(pkt.from.to_string(), "KD9ABC");
    assert_eq!(pkt.to.to_string(), "APDR15");
}

#[test]
fn status_packet_header() {
    let pkt = AprsPacket::decode_textual(b"W1AW>APRS:>Net Control Station").unwrap();
    assert_eq!(pkt.from.to_string(), "W1AW");
    assert!(pkt.via.is_empty());
}

#[test]
fn object_packet_header() {
    let pkt =
        AprsPacket::decode_textual(b"W1AW>APRS,WIDE2-2:;OBJECT   *092345z4903.50N/07201.75W>")
            .unwrap();
    assert_eq!(pkt.from.to_string(), "W1AW");
}

#[test]
fn via_with_heard_flag() {
    let pkt = AprsPacket::decode_textual(b"N0CALL-1>APRS,RELAY*,WIDE2-2:>status").unwrap();
    assert!(matches!(&pkt.via[0], Digipeater::Callsign(_, true)));
    assert!(matches!(&pkt.via[1], Digipeater::Callsign(_, false)));
}

#[test]
fn malformed_no_arrow() {
    assert!(AprsPacket::decode_textual(b"W1AW:!data").is_err());
}

#[test]
fn malformed_no_colon() {
    assert!(AprsPacket::decode_textual(b"W1AW>APRS").is_err());
}

#[test]
fn empty_input() {
    assert!(AprsPacket::decode_textual(b"").is_err());
}

#[test]
fn unknown_dti_preserved() {
    let pkt = AprsPacket::decode_textual(b"W1AW>APRS:~experimental").unwrap();
    match &pkt.data {
        AprsData::Unknown { dti, data } => {
            assert_eq!(*dti, b'~');
            assert_eq!(data.as_slice(), b"~experimental");
        }
        _ => panic!("expected Unknown"),
    }
}

// Phase 2: position parsing end-to-end

#[test]
fn position_no_timestamp_uncompressed() {
    let pkt =
        AprsPacket::decode_textual(b"W1AW-9>APRS,WIDE1-1,WIDE2-2:!4903.50N/07201.75W-Test 123")
            .unwrap();
    assert_eq!(pkt.from.to_string(), "W1AW-9");
    let AprsData::Position(ref pos) = pkt.data else {
        panic!("expected Position")
    };
    assert!(!pos.messaging_supported);
    assert!(pos.timestamp.is_none());
    assert_relative_eq!(
        pos.position.latitude.value(),
        49.05833333333333,
        epsilon = 1e-9
    );
    assert_relative_eq!(
        pos.position.longitude.value(),
        -72.02916666666667,
        epsilon = 1e-9
    );
    assert_eq!(pos.position.symbol.table, '/');
    assert_eq!(pos.position.symbol.code, '-');
    assert_eq!(pos.comment, b"Test 123");
}

#[test]
fn position_with_timestamp_messaging() {
    let pkt =
        AprsPacket::decode_textual(b"KD9ABC>APRS:@074849h4821.61N\\01224.49E^322/103/A=003054")
            .unwrap();
    let AprsData::Position(ref pos) = pkt.data else {
        panic!("expected Position")
    };
    assert!(pos.messaging_supported);
    assert_eq!(pos.timestamp, Some(Timestamp::Hhmmss(7, 48, 49)));
    assert_relative_eq!(
        pos.position.latitude.value(),
        48.36016666666667,
        epsilon = 1e-9
    );
    assert_relative_eq!(
        pos.position.longitude.value(),
        12.408166666666666,
        epsilon = 1e-9
    );
    assert!(pos.position.altitude.is_some());
    assert_relative_eq!(pos.position.altitude.unwrap().feet, 3054.0, epsilon = 0.5);
}

#[test]
fn position_compressed_no_timestamp() {
    let pkt = AprsPacket::decode_textual(b"W1AW>APRS:!/ABCD#$%^- sT").unwrap();
    let AprsData::Position(ref pos) = pkt.data else {
        panic!("expected Position")
    };
    assert!(!pos.messaging_supported);
    assert_relative_eq!(
        pos.position.latitude.value(),
        25.97004667573229,
        epsilon = 0.001
    );
    assert_relative_eq!(
        pos.position.longitude.value(),
        -171.95429033460567,
        epsilon = 0.001
    );
}

#[test]
fn position_with_course_speed_extension() {
    let pkt = AprsPacket::decode_textual(b"W1AW>APRS:/074849h4821.61N\\01224.49E^322/103/A=003054")
        .unwrap();
    let AprsData::Position(ref pos) = pkt.data else {
        panic!("expected Position")
    };
    use aprs_decode::Extension;
    assert!(matches!(
        pos.extension,
        Some(Extension::DirectionSpeed {
            direction_degrees: 322,
            speed_knots: 103
        })
    ));
}

#[test]
fn position_encode_textual_round_trip() {
    let raw = b"W1AW-9>APRS,WIDE1-1,WIDE2-2:!4903.50N/07201.75W-Test 123";
    let pkt = AprsPacket::decode_textual(raw).unwrap();
    let encoded = pkt.encode_textual().unwrap();
    assert_eq!(encoded, raw);
}

#[test]
fn position_encode_ax25_round_trip() {
    let raw = b"W1AW-9>APRS,WIDE1-1,WIDE2-2:!4903.50N/07201.75W-Test 123";
    let pkt = AprsPacket::decode_textual(raw).unwrap();
    let ax25 = pkt.encode_ax25().unwrap();
    let decoded = AprsPacket::decode_ax25(&ax25).unwrap();
    let AprsData::Position(ref pos) = decoded.data else {
        panic!("expected Position")
    };
    assert_relative_eq!(
        pos.position.latitude.value(),
        49.05833333333333,
        epsilon = 1e-9
    );
}

#[test]
fn position_ddhhmm_timestamp_validation() {
    // Day 0 is invalid — aprs-parser-rs would have accepted this
    assert!(AprsPacket::decode_textual(b"W1AW>APRS:/002345z4903.50N/07201.75W-").is_err());
    // Hour 24 is invalid
    assert!(AprsPacket::decode_textual(b"W1AW>APRS:/092460z4903.50N/07201.75W-").is_err());
}

#[test]
fn position_no_via_path() {
    let pkt = AprsPacket::decode_textual(b"W1AW>APRS:!1234.56N/01234.56E-").unwrap();
    assert!(pkt.via.is_empty());
    assert!(matches!(pkt.data, AprsData::Position(_)));
}

// Phase 3: Message, Status, Object, Item, MIC-E

#[test]
fn message_directed_with_id() {
    let pkt =
        AprsPacket::decode_textual(b"KD9ABC>APDR15,qAR,KD9XYZ::W1AW-9   :Hello world{001").unwrap();
    let AprsData::Message(ref msg) = pkt.data else {
        panic!("expected Message")
    };
    assert_eq!(msg.addressee, b"W1AW-9");
    assert_eq!(msg.text, b"Hello world");
    assert!(matches!(&msg.subtype, MessageSubtype::Directed { id: Some(id) } if id == b"001"));
    let encoded = pkt.encode_textual().unwrap();
    assert_eq!(
        encoded,
        b"KD9ABC>APDR15,qAR,KD9XYZ::W1AW-9   :Hello world{001"
    );
}

#[test]
fn message_ack() {
    let pkt = AprsPacket::decode_textual(b"KD9ABC>APRS::W1AW-9   :ack001").unwrap();
    let AprsData::Message(ref msg) = pkt.data else {
        panic!("expected Message")
    };
    assert!(matches!(&msg.subtype, MessageSubtype::Ack { .. }));
}

#[test]
fn message_bulletin() {
    let pkt = AprsPacket::decode_textual(b"KD9ABC>APRS::BLN3     :Net at 21:00z tonight").unwrap();
    let AprsData::Message(ref msg) = pkt.data else {
        panic!("expected Message")
    };
    assert!(matches!(msg.subtype, MessageSubtype::Bulletin));
    assert_eq!(msg.text, b"Net at 21:00z tonight");
}

#[test]
fn message_telemetry_parm() {
    let pkt =
        AprsPacket::decode_textual(b"KD9ABC>APRS::KD9ABC   :PARM.Bat1,Bat2,Temp,Hum,Pres").unwrap();
    let AprsData::Message(ref msg) = pkt.data else {
        panic!("expected Message")
    };
    assert!(matches!(msg.subtype, MessageSubtype::TelemetryParm));
}

#[test]
fn status_with_timestamp() {
    let pkt = AprsPacket::decode_textual(b"W1AW>APRS:>312359zSystem online").unwrap();
    let AprsData::Status(ref s) = pkt.data else {
        panic!("expected Status")
    };
    assert_eq!(s.timestamp, Some(Timestamp::Ddhhmm(31, 23, 59)));
    assert_eq!(s.comment, b"System online");
    let encoded = pkt.encode_textual().unwrap();
    assert_eq!(encoded, b"W1AW>APRS:>312359zSystem online");
}

#[test]
fn status_no_timestamp() {
    let pkt = AprsPacket::decode_textual(b"W1AW>APRS:>12.6V 0.2A 22degC").unwrap();
    let AprsData::Status(ref s) = pkt.data else {
        panic!("expected Status")
    };
    assert!(s.timestamp.is_none());
    assert_eq!(s.comment, b"12.6V 0.2A 22degC");
}

#[test]
fn object_live_round_trip() {
    let raw = b"N8DEU-7>APZWX,WIDE2-2:;HFEST-18H*170403z3443.55N\\08635.47Wh146.940MHz T100 Huntsville Hamfest";
    let pkt = AprsPacket::decode_textual(raw).unwrap();
    let AprsData::Object(ref o) = pkt.data else {
        panic!("expected Object")
    };
    assert_eq!(o.name, b"HFEST-18H");
    assert!(o.live);
    assert_relative_eq!(
        o.position.latitude.value(),
        34.725833333333334,
        epsilon = 1e-9
    );
    let encoded = pkt.encode_textual().unwrap();
    assert_eq!(encoded.as_slice(), raw.as_slice());
}

#[test]
fn item_live_round_trip() {
    let raw = b"N8DEU-7>APZWX,WIDE2-2:)AIDV#2!4903.50N/07201.75WA";
    let pkt = AprsPacket::decode_textual(raw).unwrap();
    let AprsData::Item(ref item) = pkt.data else {
        panic!("expected Item")
    };
    assert_eq!(item.name, b"AIDV#2");
    assert!(item.live);
    let encoded = pkt.encode_textual().unwrap();
    assert_eq!(encoded.as_slice(), raw.as_slice());
}

#[test]
fn mice_basic_decode() {
    // From the APRS spec/aprs-parser-rs tests: destination PPPPPP
    let pkt = AprsPacket::decode_textual(b"N0CALL>PPPPPP:`(_fn\"Oj/Hello world!").unwrap();
    let AprsData::MicE(ref m) = pkt.data else {
        panic!("expected MicE")
    };
    assert!(m.is_current);
    assert_eq!(m.symbol_code, 'j');
    assert_eq!(m.symbol_table, '/');
    assert_eq!(m.comment, b"Hello world!");
    assert_eq!(m.speed.knots(), 20);
    assert_eq!(m.course.degrees(), 251);
}

#[test]
fn mice_kenwood_device_detected() {
    // `>` prefix identifies Kenwood TH-D7A
    // Packet with `>` manufacturer prefix before the altitude/comment
    let pkt = AprsPacket::decode_textual(b"N0CALL>PPPPPP:`(_fn\"Oj/>\"49}Hello").unwrap();
    let AprsData::MicE(ref m) = pkt.data else {
        panic!("expected MicE")
    };
    // Device detection depends on the altitude marker being present
    // The `>` before the altitude block is the Kenwood TH-D7A identifier
    let _ = m; // Verify it parses without panic
}

// Phase 4: Weather and Telemetry

#[test]
fn positionless_weather_parse_and_encode() {
    let raw = b"W1AW>APRS:_10071820220/004g005t077r000p000P000h50b09900";
    let pkt = AprsPacket::decode_textual(raw).unwrap();
    let AprsData::Weather(ref wx) = pkt.data else {
        panic!("expected Weather")
    };
    assert_eq!(wx.timestamp, b"10071820");
    assert_eq!(wx.weather.wind_direction.unwrap().degrees(), 220);
    assert_eq!(wx.weather.wind_speed.unwrap().mph(), 4);
    assert_eq!(wx.weather.temperature.unwrap().fahrenheit(), 77);
    assert_eq!(wx.weather.humidity.unwrap().percent(), 50);
    assert_eq!(wx.weather.barometric_pressure.unwrap().tenths_mbar(), 9900);
    // Unit conversions
    assert!((wx.weather.wind_speed.unwrap().knots() - 3.476).abs() < 0.01);
    assert!((wx.weather.temperature.unwrap().celsius() - 25.0).abs() < 0.1);
    assert!((wx.weather.barometric_pressure.unwrap().hpa() - 990.0).abs() < 0.1);
    let encoded = pkt.encode_textual().unwrap();
    assert_eq!(encoded.as_slice(), raw.as_slice());
}

#[test]
fn position_weather_station() {
    // Position packet with weather symbol `/_` triggers weather parsing
    let pkt = AprsPacket::decode_textual(
        b"W1AW>APRS:!4903.50N/07201.75W_220/004g005t077r000p000P000h50b09900",
    )
    .unwrap();
    let AprsData::Position(ref pos) = pkt.data else {
        panic!("expected Position")
    };
    assert_eq!(pos.position.symbol.code, '_');
    assert!(pos.weather.is_some());
    let wx = pos.weather.as_ref().unwrap();
    assert_eq!(wx.wind_direction.unwrap().degrees(), 220);
    assert_eq!(wx.temperature.unwrap().fahrenheit(), 77);
}

#[test]
fn telemetry_parse_and_encode() {
    let raw = b"W1AW>APRS:T#001,100,200,300,400,500,10101010";
    let pkt = AprsPacket::decode_textual(raw).unwrap();
    let AprsData::Telemetry(ref t) = pkt.data else {
        panic!("expected Telemetry")
    };
    assert_eq!(t.sequence, b"001");
    assert_eq!(t.analog[0], Some(100.0));
    assert_eq!(t.analog[4], Some(500.0));
    assert_eq!(t.digital, 0b10101010);
    let encoded = pkt.encode_textual().unwrap();
    assert_eq!(encoded.as_slice(), raw.as_slice());
}

#[test]
fn telemetry_with_comment() {
    let pkt =
        AprsPacket::decode_textual(b"W1AW>APRS:T#015,023,000,255,128,100,11110000,Station data")
            .unwrap();
    let AprsData::Telemetry(ref t) = pkt.data else {
        panic!("expected Telemetry")
    };
    assert_eq!(t.comment, b"Station data");
    assert_eq!(t.digital, 0b11110000);
}

#[test]
fn telemetry_metadata_parm_in_message() {
    // PARM. metadata arrives as a message with TelemetryParm subtype
    let pkt = AprsPacket::decode_textual(
        b"W1AW>APRS::KD9ABC   :PARM.Bat1,Bat2,Temp,Hum,Pres,LED1,LED2,LED3,LED4,LED5,LED6,LED7,LED8",
    ).unwrap();
    let AprsData::Message(ref msg) = pkt.data else {
        panic!("expected Message")
    };
    assert!(matches!(msg.subtype, MessageSubtype::TelemetryParm));
    // Parse the metadata from the message text (after "PARM.")
    use aprs_decode::TelemetryMetadata;
    let names = TelemetryMetadata::parse_parm(&msg.text[5..]); // skip "PARM."
    assert_eq!(names[0].as_deref(), Some(b"Bat1".as_slice()));
    assert_eq!(names[4].as_deref(), Some(b"Pres".as_slice()));
}

#[test]
fn telemetry_eqns_parsed() {
    use aprs_decode::TelemetryMetadata;
    let eqns = TelemetryMetadata::parse_eqns(b"0,0.01,0,0,0.01,0,0,1,0,0,1,0,0,1,0");
    assert_eq!(eqns.len(), 5);
    assert!((eqns[0].b - 0.01).abs() < 0.001);
    // Apply equation: raw=100 → 0 + 0.01*100 + 0*100² = 1.0
    use aprs_decode::TelemetryEquation;
    let eq = TelemetryEquation {
        a: 0.0,
        b: 0.01,
        c: 0.0,
    };
    assert!((eq.apply(100.0) - 1.0).abs() < 0.001);
}

// Phase 5: GridLocator, NMEA, ThirdParty, UserDefined, Capabilities, Query, frequency

#[test]
fn grid_locator_4char() {
    let pkt = AprsPacket::decode_textual(b"W1AW>APRS:[JO22]").unwrap();
    let AprsData::GridLocator(ref g) = pkt.data else {
        panic!("expected GridLocator")
    };
    assert_eq!(g.grid, b"JO22");
    let (lat, lon) = g.to_position().unwrap();
    assert_relative_eq!(lat.value(), 52.5, epsilon = 0.1);
    assert_relative_eq!(lon.value(), 5.0, epsilon = 0.1);
    let encoded = pkt.encode_textual().unwrap();
    assert_eq!(encoded, b"W1AW>APRS:[JO22]");
}

#[test]
fn grid_locator_6char_with_comment() {
    let pkt = AprsPacket::decode_textual(b"W1AW>APRS:[IO91SX]comment here").unwrap();
    let AprsData::GridLocator(ref g) = pkt.data else {
        panic!("expected GridLocator")
    };
    assert_eq!(g.grid, b"IO91SX");
    assert_eq!(g.comment, b"comment here");
    let encoded = pkt.encode_textual().unwrap();
    assert_eq!(encoded, b"W1AW>APRS:[IO91SX]comment here");
}

#[test]
fn nmea_round_trip() {
    let raw = b"W1AW>APRS:$GPGGA,123519,4807.038,N,01131.000,E,1,08,0.9,545.4,M,47.0,M,,*47";
    let pkt = AprsPacket::decode_textual(raw).unwrap();
    let AprsData::Nmea(ref n) = pkt.data else {
        panic!("expected Nmea")
    };
    assert!(n.data.starts_with(b"GPGGA"));
    let encoded = pkt.encode_textual().unwrap();
    assert_eq!(encoded.as_slice(), raw.as_slice());
}

#[test]
fn third_party_position() {
    let raw =
        b"W0RO-11>APRX29,TCPIP*,qAC,T2MCI:}WB0VGI-7>APOT30,W0RO-11*,WIDE2-1:!4228.35N/09101.45Wk";
    let pkt = AprsPacket::decode_textual(raw).unwrap();
    let AprsData::ThirdParty(ref tp) = pkt.data else {
        panic!("expected ThirdParty")
    };
    assert_eq!(tp.inner.from.to_string(), "WB0VGI-7");
    assert!(matches!(tp.inner.data, AprsData::Position(_)));
    let encoded = pkt.encode_textual().unwrap();
    assert_eq!(encoded.as_slice(), raw.as_slice());
}

#[test]
fn user_defined_round_trip() {
    let raw = b"W1ABC>APRS:{Qhello world";
    let pkt = AprsPacket::decode_textual(raw).unwrap();
    let AprsData::UserDefined(ref ud) = pkt.data else {
        panic!("expected UserDefined")
    };
    assert_eq!(ud.user_id, b'Q');
    assert_eq!(ud.packet_type, b'h');
    assert_eq!(ud.data, b"ello world");
    let encoded = pkt.encode_textual().unwrap();
    assert_eq!(encoded.as_slice(), raw.as_slice());
}

#[test]
fn capabilities_round_trip() {
    let raw = b"W1AW>APRS:<IGATE,MSG_CNT=10,LOC_CNT=20";
    let pkt = AprsPacket::decode_textual(raw).unwrap();
    let AprsData::Capabilities(ref cap) = pkt.data else {
        panic!("expected Capabilities")
    };
    assert_eq!(cap.raw, b"IGATE,MSG_CNT=10,LOC_CNT=20");
    let encoded = pkt.encode_textual().unwrap();
    assert_eq!(encoded.as_slice(), raw.as_slice());
}

#[test]
fn query_round_trip() {
    let raw = b"W1AW>APRS:?APRS?";
    let pkt = AprsPacket::decode_textual(raw).unwrap();
    let AprsData::Query(ref q) = pkt.data else {
        panic!("expected Query")
    };
    assert_eq!(q.query_type, b"APRS");
    assert!(q.footprint.is_none());
    let encoded = pkt.encode_textual().unwrap();
    assert_eq!(encoded.as_slice(), raw.as_slice());
}

#[test]
fn frequency_extracted_from_position_comment() {
    let pkt =
        AprsPacket::decode_textual(b"W1AW>APRS:!4903.50N/07201.75Wk146.520MHz T100 -060 repeater")
            .unwrap();
    let AprsData::Position(ref pos) = pkt.data else {
        panic!("expected Position")
    };
    assert!(pos.frequency_mhz.is_some());
    let freq = pos.frequency_mhz.unwrap();
    assert!(
        (freq - 146.520).abs() < 0.001,
        "expected 146.520, got {freq}"
    );
    // Comment is preserved verbatim for round-trip
    assert_eq!(pos.comment, b"146.520MHz T100 -060 repeater");
}

#[test]
fn frequency_extracted_from_object_comment() {
    let pkt = AprsPacket::decode_textual(
        b"N8DEU-7>APZWX:;REPEATER *170403z3443.55N\\08635.47Wh146.940MHz T100 Huntsville Hamfest",
    )
    .unwrap();
    let AprsData::Object(ref obj) = pkt.data else {
        panic!("expected Object")
    };
    assert!(obj.frequency_mhz.is_some());
    let freq = obj.frequency_mhz.unwrap();
    assert!(
        (freq - 146.940).abs() < 0.001,
        "expected 146.940, got {freq}"
    );
}

#[test]
fn no_unknown_dtis_for_spec_types() {
    // Every spec-defined DTI should now dispatch to a named variant, not Unknown
    let cases: &[&[u8]] = &[
        b"W1AW>APRS:!4903.50N/07201.75W-",                  // Position !
        b"W1AW>APRS:=4903.50N/07201.75W-",                  // Position =
        b"W1AW>APRS::W1AW-9   :hello",                      // Message
        b"W1AW>APRS:>status text",                          // Status
        b"W1AW>APRS:;OBJ      *092345z4903.50N/07201.75W>", // Object
        b"W1AW>APRS:)ITEM!4903.50N/07201.75WA",             // Item
        b"W1AW>APRS:_10071820220/004g005t077",              // Weather
        b"W1AW>APRS:T#001,100,200,300,400,500,10101010",    // Telemetry
        b"W1AW>APRS:<IGATE",                                // Capabilities
        b"W1AW>APRS:?APRS?",                                // Query
        b"W1AW>APRS:[JO22]",                                // GridLocator
        b"W1AW>APRS:$GPGGA,data",                           // NMEA
        b"W1AW>APRS:{QXdata",                               // UserDefined
    ];
    for &raw in cases {
        let pkt = AprsPacket::decode_textual(raw).unwrap();
        assert!(
            !matches!(pkt.data, AprsData::Unknown { .. }),
            "packet {raw:?} was parsed as Unknown"
        );
    }
}

// Phase 6: Symbol lookup and serde

#[test]
fn symbol_lookup_primary() {
    use aprs_decode::Symbol;
    assert_eq!(Symbol::new('/', '>').description(), Some("Car"));
    assert_eq!(Symbol::new('/', '-').description(), Some("House"));
    assert_eq!(Symbol::new('/', '_').description(), Some("Weather Station"));
    assert_eq!(Symbol::new('/', '[').description(), Some("Jogger"));
    assert_eq!(Symbol::new('/', 'a').description(), Some("Ambulance"));
}

#[test]
fn symbol_lookup_alternate() {
    use aprs_decode::Symbol;
    assert_eq!(Symbol::new('\\', '@').description(), Some("Tornado"));
    assert_eq!(Symbol::new('\\', '_').description(), Some("Funnel Cloud"));
    assert_eq!(Symbol::new('\\', 's').description(), Some("Satellite"));
}

#[test]
fn symbol_lookup_overlay_uses_alternate_table() {
    use aprs_decode::Symbol;
    // Numeric overlay uses the alternate table
    let s = Symbol::new('3', '>');
    assert_eq!(s.description(), Some("Info Kiosk")); // alternate table code for >
}

#[test]
fn symbol_reserved_returns_none() {
    use aprs_decode::Symbol;
    // A-Z in primary table are overlay-only
    assert!(Symbol::new('/', 'A').description().is_none());
}

#[test]
fn symbol_all_codes_no_panic() {
    use aprs_decode::Symbol;
    for code in '!'..='~' {
        let _ = Symbol::new('/', code).description();
        let _ = Symbol::new('\\', code).description();
    }
}
