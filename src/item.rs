use crate::error::AprsError;
use crate::types::{Extension, Position};

/// An APRS Item Report.
///
/// DTI: `)`
///
/// Items are similar to objects but lack a timestamp and have a variable-length
/// name (3–9 characters). They are intended for inanimate things reported
/// occasionally on a map (checkpoints, aid posts, etc.).
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct AprsItem {
    /// Item name (3–9 chars; any char except `!` and ` `).
    pub name: Vec<u8>,
    /// `true` if the item is live/active (`!`); `false` if killed (` `).
    pub live: bool,
    pub position: Position,
    /// Optional data extension (course/speed, PHG, RNG, DFS).
    pub extension: Option<Extension>,
    pub comment: Vec<u8>,
}

impl AprsItem {
    /// Decode from the information field (including the leading `)` DTI byte).
    pub(crate) fn parse(info: &[u8]) -> Result<Self, AprsError> {
        // Format: )NAME...!latSymlonSymComment   (live)
        //         )NAME... latSymlonSymComment   (killed)
        // NAME is 3-9 chars, terminated by `!` (live) or ` ` (killed)
        if info.len() < 5 {
            return Err(AprsError::InvalidItem {
                detail: "packet too short",
            });
        }

        // Collect name bytes (starting at index 1, after `)`)
        let mut name = Vec::with_capacity(9);
        let mut liveness_idx = None;

        for (i, &b) in info.iter().enumerate().skip(1).take(9) {
            if b == b'!' || b == b' ' {
                liveness_idx = Some(i);
                break;
            }
            name.push(b);
        }

        if name.len() < 3 {
            return Err(AprsError::InvalidItem {
                detail: "name too short (< 3 chars)",
            });
        }

        let liveness_idx = liveness_idx.ok_or(AprsError::InvalidItem {
            detail: "liveness byte not found",
        })?;

        let live = match info[liveness_idx] {
            b'!' => true,
            b' ' | b'_' => false,
            _ => {
                return Err(AprsError::InvalidItem {
                    detail: "invalid liveness byte",
                });
            }
        };

        let position_bytes = info.get(liveness_idx + 1..).ok_or(AprsError::InvalidItem {
            detail: "truncated after liveness",
        })?;

        let (remaining, position) = Position::parse(position_bytes)?;
        let comment_raw = remaining.unwrap_or_default();

        let (extension, comment) = if position.compressed_cs.is_none() {
            if let Some(ext) = Extension::parse(comment_raw) {
                (Some(ext), comment_raw.get(7..).unwrap_or_default().to_vec())
            } else {
                (None, comment_raw.to_vec())
            }
        } else {
            (None, comment_raw.to_vec())
        };

        Ok(Self {
            name,
            live,
            position,
            extension,
            comment,
        })
    }

    pub fn encode(&self) -> Vec<u8> {
        let mut out = vec![b')'];
        out.extend_from_slice(&self.name);
        out.push(if self.live { b'!' } else { b' ' });

        if self.extension.is_some() || self.position.compressed_cs.is_none() {
            self.position.encode_uncompressed(&mut out);
            if let Some(ref ext) = self.extension {
                ext.encode(&mut out);
            }
        } else {
            self.position.encode_compressed(&mut out);
        }

        out.extend_from_slice(&self.comment);
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use approx::assert_relative_eq;

    #[test]
    fn parse_live_item() {
        let item = AprsItem::parse(b")AIDV#2!4903.50N/07201.75WA").unwrap();
        assert_eq!(item.name, b"AIDV#2");
        assert!(item.live);
        assert_relative_eq!(
            item.position.latitude.value(),
            49.05833333333333,
            epsilon = 1e-9
        );
        assert_relative_eq!(
            item.position.longitude.value(),
            -72.02916666666667,
            epsilon = 1e-9
        );
        assert_eq!(item.position.symbol.table, '/');
        assert_eq!(item.position.symbol.code, 'A');
    }

    #[test]
    fn parse_dead_item() {
        let item = AprsItem::parse(b")AID 4903.50N/07201.75WA").unwrap();
        assert_eq!(item.name, b"AID");
        assert!(!item.live);
    }

    #[test]
    fn parse_with_extension() {
        let item = AprsItem::parse(b")AID 4903.50N/07201.75WAPHG5132").unwrap();
        assert!(item.extension.is_some());
        assert!(item.comment.is_empty());
    }

    #[test]
    fn parse_compressed_item() {
        let item = AprsItem::parse(b")MOBIL!\\5L!!<*e79 sT").unwrap();
        assert_eq!(item.name, b"MOBIL");
        assert!(item.live);
        assert_relative_eq!(item.position.latitude.value(), 49.5, epsilon = 0.01);
    }

    #[test]
    fn name_too_short() {
        assert!(AprsItem::parse(b")AB!4903.50N/07201.75WA").is_err());
    }

    #[test]
    fn encode_round_trip_live() {
        let raw = b")AIDV#2!4903.50N/07201.75WA";
        let item = AprsItem::parse(raw).unwrap();
        assert_eq!(item.encode(), raw);
    }

    #[test]
    fn encode_round_trip_dead() {
        let raw = b")AID 4903.50N/07201.75WA";
        let item = AprsItem::parse(raw).unwrap();
        assert_eq!(item.encode(), raw);
    }

    #[test]
    fn encode_round_trip_with_extension() {
        let raw = b")AID 4903.50N/07201.75WAPHG5132";
        let item = AprsItem::parse(raw).unwrap();
        assert_eq!(item.encode(), raw);
    }
}
