use std::collections::HashMap;
use std::path::Path;

const V27: u32 = 0x07564427;
const V28: u32 = 0x07564428;
const V29: u32 = 0x07564429;

#[derive(Debug, Clone)]
pub struct AppEntry {
    pub name: String,
    pub kind: String,
}

struct Cursor<'a> {
    b: &'a [u8],
    i: usize,
}

impl<'a> Cursor<'a> {
    fn u8(&mut self) -> Option<u8> {
        let v = *self.b.get(self.i)?;
        self.i += 1;
        Some(v)
    }
    fn u32(&mut self) -> Option<u32> {
        let s = self.b.get(self.i..self.i + 4)?;
        self.i += 4;
        Some(u32::from_le_bytes(s.try_into().ok()?))
    }
    fn skip(&mut self, n: usize) -> Option<()> {
        self.i = self.i.checked_add(n).filter(|i| *i <= self.b.len())?;
        Some(())
    }
    fn cstr(&mut self) -> Option<String> {
        let end = self.b[self.i..].iter().position(|c| *c == 0)? + self.i;
        let s = String::from_utf8_lossy(&self.b[self.i..end]).into_owned();
        self.i = end + 1;
        Some(s)
    }
}

/// Reads the Steam client's binary app metadata cache. This is how tonight can
/// name every app you own with no network call and no API key: the client has
/// already downloaded the answer.
pub fn parse(path: &Path) -> Option<HashMap<i64, AppEntry>> {
    let data = std::fs::read(path).ok()?;
    let mut c = Cursor { b: &data, i: 0 };
    let magic = c.u32()?;
    if !matches!(magic, V27 | V28 | V29) {
        return None;
    }
    let _universe = c.u32()?;

    let table = if magic == V29 {
        let offset = i64::from_le_bytes(data.get(8..16)?.try_into().ok()?) as usize;
        c.skip(8)?;
        Some(read_string_table(&data, offset)?)
    } else {
        None
    };

    let extra_sha = if magic == V27 { 0 } else { 20 };
    let mut out = HashMap::new();

    loop {
        let appid = c.u32()?;
        if appid == 0 {
            break;
        }
        let size = c.u32()? as usize;
        let body_start = c.i;
        let body_end = body_start.checked_add(size)?;
        if body_end > data.len() {
            break;
        }
        let kv_start = body_start + 4 + 4 + 8 + 20 + 4 + extra_sha;
        if kv_start < body_end {
            if let Some(entry) = extract(&data[kv_start..body_end], table.as_deref()) {
                out.insert(appid as i64, entry);
            }
        }
        c.i = body_end;
    }
    Some(out)
}

fn read_string_table(data: &[u8], offset: usize) -> Option<Vec<String>> {
    let count = u32::from_le_bytes(data.get(offset..offset + 4)?.try_into().ok()?) as usize;
    let mut c = Cursor {
        b: data,
        i: offset + 4,
    };
    let mut out = Vec::with_capacity(count.min(1 << 20));
    for _ in 0..count {
        out.push(c.cstr()?);
    }
    Some(out)
}

/// Walks the binary KeyValues blob far enough to read `appinfo/common/{name,type}`
/// and no further.
fn extract(blob: &[u8], table: Option<&[String]>) -> Option<AppEntry> {
    let mut c = Cursor { b: blob, i: 0 };
    let mut path: Vec<String> = vec![];
    let mut name = None;
    let mut kind = None;

    while c.i < blob.len() {
        let t = c.u8()?;
        if t == 0x08 {
            if path.pop().is_none() {
                break;
            }
            continue;
        }
        let key = match table {
            Some(t) => {
                let idx = c.u32()? as usize;
                t.get(idx)?.clone()
            }
            None => c.cstr()?,
        };
        let key_l = key.to_ascii_lowercase();
        match t {
            0x00 => path.push(key_l),
            0x01 => {
                let value = c.cstr()?;
                if path.len() == 2 && path[1] == "common" {
                    if key_l == "name" {
                        name = Some(value);
                    } else if key_l == "type" {
                        kind = Some(value.to_ascii_lowercase());
                    }
                }
            }
            0x02 => c.skip(4)?,
            0x07 => c.skip(8)?,
            _ => return finish(name, kind),
        }
        if name.is_some() && kind.is_some() {
            break;
        }
    }
    finish(name, kind)
}

fn finish(name: Option<String>, kind: Option<String>) -> Option<AppEntry> {
    let name = name?;
    if name.trim().is_empty() {
        return None;
    }
    Some(AppEntry {
        name,
        kind: kind.unwrap_or_default(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_local_client_cache_when_present() {
        let Some(home) = dirs::home_dir() else { return };
        let path = home.join(".steam/steam/appcache/appinfo.vdf");
        if !path.is_file() {
            return;
        }
        let map = parse(&path).expect("appinfo.vdf should parse");
        assert!(map.len() > 50, "expected many apps, got {}", map.len());
        if let Some(hl2) = map.get(&220) {
            assert_eq!(hl2.name, "Half-Life 2");
            assert_eq!(hl2.kind, "game");
        }
    }
}
