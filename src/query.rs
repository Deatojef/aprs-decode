/// An APRS General Query packet.
///
/// DTI: `?`
///
/// Format: `?TYPE?` or `?TYPE?lat,lon,radius` (with optional geographic footprint).
///
/// Common query types: `APRS`, `IGATE`, `WX`, `VERSION`, `STATUS`.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct AprsQuery {
    /// The query type (between the two `?` characters).
    pub query_type: Vec<u8>,
    /// Optional geographic footprint: (latitude°, longitude°, radius km).
    pub footprint: Option<QueryFootprint>,
    /// Any trailing bytes after the footprint (preserved verbatim).
    pub trailing: Vec<u8>,
}

/// A geographic footprint associated with a query.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct QueryFootprint {
    pub latitude: f64,
    pub longitude: f64,
    pub radius_km: f32,
}

impl AprsQuery {
    /// Decode from the information field (including the leading `?` DTI byte).
    pub(crate) fn parse(info: &[u8]) -> Self {
        // Format: ?TYPE?[lat,lon,radius]
        // The first `?` is the DTI; the second `?` ends the type token.
        let body = info.get(1..).unwrap_or_default();

        let second_q = body.iter().position(|&b| b == b'?');
        let (query_type, after_type) = match second_q {
            Some(pos) => (
                body[..pos].to_vec(),
                body.get(pos + 1..).unwrap_or_default(),
            ),
            None => (body.to_vec(), &b""[..]),
        };

        let (footprint, trailing) = parse_footprint(after_type);

        Self {
            query_type,
            footprint,
            trailing,
        }
    }

    pub fn encode(&self) -> Vec<u8> {
        let mut out = vec![b'?'];
        out.extend_from_slice(&self.query_type);
        out.push(b'?');
        if let Some(ref fp) = self.footprint {
            out.extend_from_slice(
                format!("{},{},{}", fp.latitude, fp.longitude, fp.radius_km).as_bytes(),
            );
        }
        out.extend_from_slice(&self.trailing);
        out
    }
}

fn parse_footprint(b: &[u8]) -> (Option<QueryFootprint>, Vec<u8>) {
    if b.is_empty() {
        return (None, vec![]);
    }
    let parts: Vec<&[u8]> = b.splitn(4, |&c| c == b',').collect();
    if parts.len() >= 3 {
        let lat = parse_f64(parts[0]);
        let lon = parse_f64(parts[1]);
        let radius = parse_f32(parts[2]);
        if let (Some(lat), Some(lon), Some(radius)) = (lat, lon, radius) {
            let trailing = parts.get(3).map(|p| p.to_vec()).unwrap_or_default();
            return (
                Some(QueryFootprint {
                    latitude: lat,
                    longitude: lon,
                    radius_km: radius,
                }),
                trailing,
            );
        }
    }
    (None, b.to_vec())
}

fn parse_f64(b: &[u8]) -> Option<f64> {
    std::str::from_utf8(b).ok()?.trim().parse().ok()
}

fn parse_f32(b: &[u8]) -> Option<f32> {
    std::str::from_utf8(b).ok()?.trim().parse().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn simple_aprs_query() {
        let q = AprsQuery::parse(b"?APRS?");
        assert_eq!(q.query_type, b"APRS");
        assert!(q.footprint.is_none());
    }

    #[test]
    fn query_with_footprint() {
        let q = AprsQuery::parse(b"?APRS?49.0,-72.0,10");
        assert_eq!(q.query_type, b"APRS");
        let fp = q.footprint.unwrap();
        assert!((fp.latitude - 49.0).abs() < 0.01);
        assert!((fp.longitude - -72.0).abs() < 0.01);
        assert!((fp.radius_km - 10.0).abs() < 0.01);
    }

    #[test]
    fn no_second_question_mark() {
        // Some implementations omit the second `?`
        let q = AprsQuery::parse(b"?APRS");
        assert_eq!(q.query_type, b"APRS");
    }

    #[test]
    fn encode_round_trip() {
        let raw = b"?IGATE?";
        let q = AprsQuery::parse(raw);
        assert_eq!(q.encode().as_slice(), raw.as_slice());
    }
}
