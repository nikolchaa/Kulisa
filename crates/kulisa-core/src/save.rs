//! Versioned save files: `"KSAV" | format u16 LE | header Value | body Value`.
//!
//! The header is readable on its own (slot lists, thumbnails). Bodies are plain `Value`
//! trees, so unknown fields are ignored and missing ones fall back to defaults. Games
//! version their own data with `save_version` and upgrade old saves with [`Migrations`].

use crate::Value;

const MAGIC: &[u8; 4] = b"KSAV";
/// Version of the container and engine-owned body layout.
pub const FORMAT_VERSION: u16 = 1;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct SaveHeader {
    pub game_id: String,
    pub game_version: String,
    /// The game's own data version; see [`Migrations`].
    pub save_version: u32,
    /// Seconds since the Unix epoch.
    pub timestamp: u64,
    pub slot: String,
    /// Seconds played.
    pub playtime: f64,
    /// Short description for load screens (e.g. the last line of dialogue).
    pub title: String,
    /// PNG bytes.
    pub thumbnail: Option<Vec<u8>>,
}

impl SaveHeader {
    fn to_value(&self) -> Value {
        Value::map()
            .with("game_id", self.game_id.as_str())
            .with("game_version", self.game_version.as_str())
            .with("save_version", self.save_version)
            .with("timestamp", self.timestamp as i64)
            .with("slot", self.slot.as_str())
            .with("playtime", self.playtime)
            .with("title", self.title.as_str())
            .with("thumbnail", self.thumbnail.clone().map(Value::Bytes))
    }

    fn from_value(v: &Value) -> Self {
        let s = |k| v.get(k).as_str().unwrap_or_default().to_string();
        Self {
            game_id: s("game_id"),
            game_version: s("game_version"),
            save_version: v.get("save_version").as_int().unwrap_or(0) as u32,
            timestamp: v.get("timestamp").as_int().unwrap_or(0).max(0) as u64,
            slot: s("slot"),
            playtime: v.get("playtime").as_float().unwrap_or(0.0),
            title: s("title"),
            thumbnail: match v.get("thumbnail") {
                Value::Bytes(b) => Some(b.clone()),
                _ => None,
            },
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct SaveFile {
    pub header: SaveHeader,
    pub body: Value,
}

impl SaveFile {
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut out = MAGIC.to_vec();
        out.extend_from_slice(&FORMAT_VERSION.to_le_bytes());
        self.header.to_value().encode(&mut out);
        self.body.encode(&mut out);
        out
    }

    fn open(bytes: &[u8]) -> Result<(SaveHeader, &[u8]), String> {
        let rest = bytes.strip_prefix(MAGIC).ok_or("not a Kulisa save file")?;
        let (version, mut rest) = rest.split_at_checked(2).ok_or("save file is truncated")?;
        let version = u16::from_le_bytes([version[0], version[1]]);
        if version > FORMAT_VERSION {
            return Err(format!(
                "save format {version} was written by a newer Kulisa (this one reads up to {FORMAT_VERSION})"
            ));
        }
        let header = Value::decode(&mut rest).map_err(|e| format!("save header is damaged: {e}"))?;
        Ok((SaveHeader::from_value(&header), rest))
    }

    /// Reads only the header (fast; for slot lists).
    pub fn read_header(bytes: &[u8]) -> Result<SaveHeader, String> {
        Ok(Self::open(bytes)?.0)
    }

    pub fn from_bytes(bytes: &[u8]) -> Result<SaveFile, String> {
        let (header, mut rest) = Self::open(bytes)?;
        let body = Value::decode(&mut rest).map_err(|e| format!("save data is damaged: {e}"))?;
        Ok(SaveFile { header, body })
    }
}

type Migration = Box<dyn Fn(&mut Value) -> Result<(), String>>;

/// Upgrades a game's save bodies from older `save_version`s, one version at a time.
#[derive(Default)]
pub struct Migrations {
    steps: Vec<(u32, Migration)>,
}

impl Migrations {
    /// Registers the step that turns version `from` into `from + 1`.
    pub fn add(&mut self, from: u32, step: impl Fn(&mut Value) -> Result<(), String> + 'static) {
        self.steps.push((from, Box::new(step)));
    }

    /// Brings `save` up to `current`. Saves from a newer game version are refused.
    pub fn apply(&self, save: &mut SaveFile, current: u32) -> Result<(), String> {
        let from = save.header.save_version;
        if from > current {
            return Err(format!(
                "save is from a newer version of the game (data version {from}, this is {current})"
            ));
        }
        for v in from..current {
            if let Some((_, step)) = self.steps.iter().find(|(f, _)| *f == v) {
                step(&mut save.body).map_err(|e| format!("upgrading save from version {v}: {e}"))?;
            }
        }
        save.header.save_version = current;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn save() -> SaveFile {
        SaveFile {
            header: SaveHeader {
                game_id: "demo".into(),
                game_version: "1.0".into(),
                save_version: 1,
                timestamp: 1_760_000_000,
                slot: "quick".into(),
                playtime: 42.5,
                title: "You actually came.".into(),
                thumbnail: Some(vec![137, 80, 78, 71]),
            },
            body: Value::map().with("vars", Value::map().with("trust", 2i64)),
        }
    }

    #[test]
    fn roundtrips_and_reads_header_alone() {
        let bytes = save().to_bytes();
        assert_eq!(&bytes[..4], b"KSAV");
        assert_eq!(SaveFile::from_bytes(&bytes).unwrap(), save());
        assert_eq!(SaveFile::read_header(&bytes).unwrap(), save().header);
    }

    #[test]
    fn rejects_foreign_newer_and_damaged_files() {
        assert!(SaveFile::from_bytes(b"PK\x03\x04zip").unwrap_err().contains("not a Kulisa save"));
        let mut newer = save().to_bytes();
        newer[4] = 99;
        assert!(SaveFile::from_bytes(&newer).unwrap_err().contains("newer Kulisa"));
        let bytes = save().to_bytes();
        assert!(SaveFile::from_bytes(&bytes[..bytes.len() - 3]).unwrap_err().contains("damaged"));
    }

    #[test]
    fn tolerates_unknown_and_missing_header_fields() {
        let header = Value::map().with("game_id", "x").with("from_the_future", 1i64);
        let mut bytes = b"KSAV".to_vec();
        bytes.extend_from_slice(&FORMAT_VERSION.to_le_bytes());
        header.encode(&mut bytes);
        Value::Nil.encode(&mut bytes);
        let s = SaveFile::from_bytes(&bytes).unwrap();
        assert_eq!(s.header.game_id, "x");
        assert_eq!(s.header.thumbnail, None);
    }

    #[test]
    fn migrations_run_in_order() {
        let mut m = Migrations::default();
        // v1 -> v2: rename `trust` to `affection`; v2 -> v3: double it.
        m.add(1, |b| {
            let Value::Map(vars) = &mut b.get("vars").clone() else { return Err("no vars".into()) };
            let t = vars.remove("trust").unwrap_or_default();
            vars.insert("affection".into(), t);
            *b = b.clone().with("vars", Value::Map(vars.clone()));
            Ok(())
        });
        m.add(2, |b| {
            let a = b.get("vars").get("affection").as_int().unwrap_or(0);
            *b = b.clone().with("vars", Value::map().with("affection", a * 2));
            Ok(())
        });
        let mut s = save();
        m.apply(&mut s, 3).unwrap();
        assert_eq!(s.body.get("vars").get("affection").as_int(), Some(4));
        assert_eq!(s.header.save_version, 3);
        let mut future = save();
        future.header.save_version = 9;
        assert!(m.apply(&mut future, 3).unwrap_err().contains("newer version of the game"));
    }
}
