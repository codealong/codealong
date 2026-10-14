//! Changing one entry of another program's JSON file (a harness's settings or state) while
//! leaving the rest as that program wrote it: every other value keeps its text, and every key
//! its place.

use std::path::Path;

use serde::Deserialize;

/// A JSON value from another program's file: text left as it was, or an object opened up to
/// change, its keys in their order.
pub enum Node {
    Raw(Box<serde_json::value::RawValue>),
    Object(Vec<(String, Node)>),
}

impl Node {
    /// An object's entries, their values left as text.
    pub fn entries(text: &str) -> anyhow::Result<Vec<(String, Node)>> {
        let Entries(entries) = serde_json::from_str(text)?;
        Ok(entries.into_iter().map(|(k, v)| (k, Node::Raw(v))).collect())
    }

    /// The object under `key` in `entries` (the last, as JSON.parse takes it), opened up, and made
    /// when it's missing or null.
    pub fn object_in<'a>(entries: &'a mut Vec<(String, Node)>, key: &str) -> anyhow::Result<&'a mut Vec<(String, Node)>> {
        let i = match entries.iter().rposition(|(k, _)| k == key) {
            Some(i) => i,
            None => {
                entries.push((key.to_string(), Node::Object(Vec::new())));
                entries.len() - 1
            }
        };
        if let Node::Raw(raw) = &entries[i].1 {
            let opened = if raw.get() == "null" { Vec::new() } else { Node::entries(raw.get())? };
            entries[i].1 = Node::Object(opened);
        }
        match &mut entries[i].1 {
            Node::Object(inner) => Ok(inner),
            Node::Raw(_) => unreachable!(),
        }
    }

}

impl serde::Serialize for Node {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeMap;
        match self {
            Node::Raw(raw) => raw.serialize(s),
            Node::Object(entries) => {
                let mut map = s.serialize_map(Some(entries.len()))?;
                for (k, v) in entries {
                    map.serialize_entry(k, v)?;
                }
                map.end()
            }
        }
    }
}

/// An object's entries in the order they're written, each value as its text.
struct Entries(Vec<(String, Box<serde_json::value::RawValue>)>);

impl<'de> Deserialize<'de> for Entries {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct Visit;
        impl<'de> serde::de::Visitor<'de> for Visit {
            type Value = Entries;
            fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                f.write_str("an object")
            }
            fn visit_map<A: serde::de::MapAccess<'de>>(self, mut map: A) -> Result<Entries, A::Error> {
                let mut entries = Vec::new();
                while let Some(entry) = map.next_entry()? {
                    entries.push(entry);
                }
                Ok(Entries(entries))
            }
        }
        d.deserialize_map(Visit)
    }
}

/// Replaces `file` with `text` in one step, so nobody reads it half written: through a link to
/// it, as the harnesses write theirs, and readable only by you, as they keep them.
pub fn write_whole(file: &Path, text: &str) -> anyhow::Result<()> {
    use std::io::Write;
    use std::os::unix::fs::OpenOptionsExt;
    let target = std::fs::canonicalize(file).unwrap_or_else(|_| file.to_path_buf());
    let name = target.file_name().and_then(|n| n.to_str()).unwrap_or("file");
    let temp = target.with_file_name(format!("{name}.codealong-{}", std::process::id()));
    let written = (|| -> std::io::Result<()> {
        let mut f = std::fs::OpenOptions::new().write(true).create(true).truncate(true).mode(0o600).open(&temp)?;
        f.write_all(text.as_bytes())?;
        f.sync_all()?;
        std::fs::rename(&temp, &target)
    })();
    if written.is_err() {
        let _ = std::fs::remove_file(&temp);
    }
    Ok(written?)
}
