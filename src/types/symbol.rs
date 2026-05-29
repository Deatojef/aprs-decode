/// An APRS position symbol, consisting of a symbol table identifier and a symbol code.
///
/// - `table == '/'`: primary symbol table
/// - `table == '\\'`: alternate symbol table
/// - `table` is `'A'..='Z'` or `'0'..='9'`: alternate table with an alphanumeric overlay
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Symbol {
    pub table: char,
    pub code: char,
}

impl Symbol {
    pub fn new(table: char, code: char) -> Self {
        Self { table, code }
    }

    pub fn is_primary_table(self) -> bool {
        self.table == '/'
    }

    pub fn is_alternate_table(self) -> bool {
        self.table == '\\'
    }

    /// Returns the overlay character if this symbol uses an alphanumeric overlay.
    pub fn overlay(self) -> Option<char> {
        if self.table.is_ascii_alphanumeric() && self.table != '/' {
            Some(self.table)
        } else {
            None
        }
    }

    /// Look up the human-readable description of this symbol.
    ///
    /// Returns `None` for reserved, overlay-only, or TNC-internal codes.
    pub fn description(self) -> Option<&'static str> {
        let idx = self.code as usize;
        if !(33..=126).contains(&idx) {
            return None;
        }
        let i = idx - 33; // 0-based index into the tables
        if self.is_primary_table() {
            PRIMARY[i]
        } else {
            // Overlays use the same alternate table as the `\` table
            ALTERNATE[i]
        }
    }
}

// ─── Symbol tables ────────────────────────────────────────────────────────────
// Each array is indexed by (code as usize - 33), covering ASCII 33 ('!') to 126 ('~').
// Sources: APRS101.pdf, aprs.org/symbols, aprs.fi symbol reference.

/// Primary symbol table (table ID = `/`).
#[rustfmt::skip]
const PRIMARY: [Option<&'static str>; 94] = [
    Some("Police, Sheriff"),            // ! 33
    None,                               // " 34  (no symbol)
    Some("Digipeater"),                 // # 35
    Some("Phone"),                      // $ 36
    Some("DX Cluster"),                 // % 37
    Some("HF Gateway"),                 // & 38
    Some("Small Aircraft"),             // ' 39
    Some("Mobile Satellite Station"),   // ( 40
    Some("Wheelchair, Handicapped"),    // ) 41
    Some("Snowflake"),                  // * 42
    Some("Red Cross"),                  // + 43
    Some("Boy Scouts"),                 // , 44
    Some("House"),                      // - 45
    Some("X"),                          // . 46
    Some("Dot"),                        // / 47
    Some("Circle 0"),                   // 0 48
    Some("Circle 1"),                   // 1 49
    Some("Circle 2"),                   // 2 50
    Some("Circle 3"),                   // 3 51
    Some("Circle 4"),                   // 4 52
    Some("Circle 5"),                   // 5 53
    Some("Circle 6"),                   // 6 54
    Some("Circle 7"),                   // 7 55
    Some("Circle 8"),                   // 8 56
    Some("Circle 9"),                   // 9 57
    Some("Fire"),                       // : 58
    Some("Campground, Tent"),           // ; 59
    Some("Motorcycle"),                 // < 60
    Some("Railroad Engine"),            // = 61
    Some("Car"),                        // > 62
    Some("File Server"),                // ? 63
    Some("Hurricane, Tropical Storm"),  // @ 64
    None, None, None, None, None,       // A-E 65-69 (overlay)
    None, None, None, None, None,       // F-J 70-74 (overlay)
    None, None, None, None, None,       // K-O 75-79 (overlay)
    None, None, None, None, None,       // P-T 80-84 (overlay)
    None, None, None, None, None,       // U-Y 85-89 (overlay)
    None,                               // Z 90  (overlay)
    Some("Jogger"),                     // [ 91
    Some("Triangle"),                   // \ 92
    Some("PBBS"),                       // ] 93
    Some("Large Aircraft"),             // ^ 94
    Some("Weather Station"),            // _ 95
    Some("Satellite Dish"),             // ` 96
    Some("Ambulance"),                  // a 97
    Some("Bike"),                       // b 98
    Some("Incident Command Post"),      // c 99
    Some("Fire Dept"),                  // d 100
    Some("Horse, Equestrian"),          // e 101
    Some("Fire Truck"),                 // f 102
    Some("Glider"),                     // g 103
    Some("Hospital"),                   // h 104
    Some("IOTA"),                       // i 105
    Some("Jeep"),                       // j 106
    Some("Truck"),                      // k 107
    Some("Laptop"),                     // l 108
    Some("Mic-E Repeater"),             // m 109
    Some("Node"),                       // n 110
    Some("EOC"),                        // o 111
    Some("Rover, Dog"),                 // p 112
    Some("Grid Square"),                // q 113
    Some("Antenna"),                    // r 114
    Some("Power Boat"),                 // s 115
    Some("Truck Stop"),                 // t 116
    Some("18-Wheeler"),                 // u 117
    Some("Van"),                        // v 118
    Some("Water Station"),              // w 119
    Some("APRS"),                       // x 120
    Some("Yagi Antenna"),               // y 121
    Some("Shelter"),                    // z 122
    None,                               // { 123
    None,                               // | 124  (TNC stream switch)
    None,                               // } 125
    None,                               // ~ 126  (TNC stream switch)
];

/// Alternate symbol table (table ID = `\` or any overlay character).
#[rustfmt::skip]
const ALTERNATE: [Option<&'static str>; 94] = [
    Some("Emergency"),                  // ! 33
    None,                               // " 34
    Some("Digipeater (numbered)"),      // # 35
    Some("ATM, Bank"),                  // $ 36
    Some("Accident Scene"),             // % 37
    Some("Haze"),                       // & 38
    Some("Flash"),                      // ' 39
    Some("Cloud"),                      // ( 40
    Some("Sunny, Partly Cloudy"),       // ) 41
    Some("Snow"),                       // * 42
    Some("Church"),                     // + 43
    Some("Girl Scouts"),                // , 44
    Some("House, Shack"),               // - 45
    Some("X"),                          // . 46
    Some("Circle"),                     // / 47
    Some("Circle 0 (overlay)"),         // 0 48
    Some("Circle 1 (overlay)"),         // 1 49
    Some("Circle 2 (overlay)"),         // 2 50
    Some("Circle 3 (overlay)"),         // 3 51
    Some("Circle 4 (overlay)"),         // 4 52
    Some("Circle 5 (overlay)"),         // 5 53
    Some("Circle 6 (overlay)"),         // 6 54
    Some("Circle 7 (overlay)"),         // 7 55
    Some("Circle 8 (overlay)"),         // 8 56
    Some("Circle 9 (overlay)"),         // 9 57
    Some("Hail"),                       // : 58
    Some("Park, Picnic"),               // ; 59
    Some("NWS Advisory"),               // < 60
    Some("Railroad Station"),           // = 61
    Some("Info Kiosk"),                 // > 62
    Some("Work Zone"),                  // ? 63
    Some("Tornado"),                    // @ 64
    None, None, None, None, None,       // A-E 65-69 (overlay)
    None, None, None, None, None,       // F-J 70-74 (overlay)
    None, None, None, None, None,       // K-O 75-79 (overlay)
    None, None, None, None, None,       // P-T 80-84 (overlay)
    None, None, None, None, None,       // U-Y 85-89 (overlay)
    None,                               // Z 90  (overlay)
    Some("Wall Cloud"),                 // [ 91
    Some("Misc Aircraft"),              // \ 92
    Some("Rocket Launch"),              // ] 93
    Some("Jet Aircraft"),               // ^ 94
    Some("Funnel Cloud"),               // _ 95
    Some("Rain Shower"),                // ` 96
    Some("ARES"),                       // a 97
    Some("Blowing Snow"),               // b 98
    Some("Coast Guard"),                // c 99
    Some("Drizzle"),                    // d 100
    Some("Smoke"),                      // e 101
    Some("Freezing Rain"),              // f 102
    Some("Snow Shower"),                // g 103
    Some("Haze"),                       // h 104
    Some("Rain Shower"),                // i 105
    Some("Lightning"),                  // j 106
    Some("Kenwood Radio"),              // k 107
    Some("Lighthouse"),                 // l 108
    Some("MARS"),                       // m 109
    Some("Navigation Buoy"),            // n 110
    Some("Rocket"),                     // o 111
    Some("Parking"),                    // p 112
    Some("Earthquake"),                 // q 113
    Some("Restaurant"),                 // r 114
    Some("Satellite"),                  // s 115
    Some("Thunderstorm"),               // t 116
    Some("Sunny"),                      // u 117
    Some("VORTAC, Nav Aid"),            // v 118
    Some("NWS Site"),                   // w 119
    Some("Pharmacy"),                   // x 120
    Some("Radiosonde"),                 // y 121
    Some("Shelter"),                    // z 122
    Some("Fog"),                        // { 123
    None,                               // | 124  (TNC stream switch)
    None,                               // } 125
    None,                               // ~ 126  (TNC stream switch)
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn primary_car() {
        let s = Symbol::new('/', '>');
        assert_eq!(s.description(), Some("Car"));
    }

    #[test]
    fn primary_house() {
        let s = Symbol::new('/', '-');
        assert_eq!(s.description(), Some("House"));
    }

    #[test]
    fn primary_weather_station() {
        let s = Symbol::new('/', '_');
        assert_eq!(s.description(), Some("Weather Station"));
    }

    #[test]
    fn alternate_tornado() {
        let s = Symbol::new('\\', '@');
        assert_eq!(s.description(), Some("Tornado"));
    }

    #[test]
    fn overlay_uses_alternate_table() {
        // Numeric overlay (e.g. '3') uses alternate table descriptions
        let s = Symbol::new('3', '>');
        assert_eq!(s.description(), Some("Info Kiosk"));
    }

    #[test]
    fn reserved_overlay_code_returns_none() {
        // A-Z codes in both tables are overlay-only
        let s = Symbol::new('/', 'A');
        assert_eq!(s.description(), None);
    }

    #[test]
    fn out_of_range_code_returns_none() {
        let s = Symbol::new('/', '\x01');
        assert_eq!(s.description(), None);
    }

    #[test]
    fn all_primary_entries_no_panic() {
        for code in '!'..='~' {
            let _ = Symbol::new('/', code).description();
        }
    }

    #[test]
    fn all_alternate_entries_no_panic() {
        for code in '!'..='~' {
            let _ = Symbol::new('\\', code).description();
        }
    }
}
