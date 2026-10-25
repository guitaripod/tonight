use std::collections::HashMap;
use std::path::Path;

const V27: u32 = 0x07564427;
const V28: u32 = 0x07564428;
const V29: u32 = 0x07564429;

/// Everything the Steam client has cached about one app that the engine can use,
/// decoded from ids into plain facts. Nothing here needs a network call.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct LocalInfo {
    pub tags: Vec<u32>,
    pub categories: Vec<u32>,
    pub genres: Vec<u32>,
    pub primary_genre: Option<u32>,
    pub languages: u32,
    pub audio_full: u32,
    pub subtitles: u32,
    pub deck: u8,
    pub deck_input: Option<String>,
    pub needs_online: Option<bool>,
    pub review_percentage: i64,
    pub metacritic: i64,
    pub release_ts: i64,
}

#[derive(Debug, Clone)]
pub struct AppEntry {
    pub name: String,
    pub kind: String,
    pub info: LocalInfo,
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
    fn i32(&mut self) -> Option<i32> {
        self.u32().map(|v| v as i32)
    }
    fn i64(&mut self) -> Option<i64> {
        let s = self.b.get(self.i..self.i + 8)?;
        self.i += 8;
        Some(i64::from_le_bytes(s.try_into().ok()?))
    }
    fn skip(&mut self, n: usize) -> Option<()> {
        self.i = self.i.checked_add(n).filter(|i| *i <= self.b.len())?;
        Some(())
    }
    fn cstr(&mut self) -> Option<String> {
        let end = self.b.get(self.i..)?.iter().position(|c| *c == 0)? + self.i;
        let s = String::from_utf8_lossy(&self.b[self.i..end]).into_owned();
        self.i = end + 1;
        Some(s)
    }
}

/// One node of Steam's binary KeyValues tree. Only the shapes `appinfo` uses.
#[derive(Debug, Clone)]
enum Kv {
    Map(Vec<(String, Kv)>),
    Text(String),
    Int(i64),
}

impl Kv {
    fn get(&self, key: &str) -> Option<&Kv> {
        match self {
            Kv::Map(items) => items.iter().find(|(k, _)| k == key).map(|(_, v)| v),
            _ => None,
        }
    }

    fn entries(&self) -> &[(String, Kv)] {
        match self {
            Kv::Map(items) => items,
            _ => &[],
        }
    }

    fn as_i64(&self) -> Option<i64> {
        match self {
            Kv::Int(i) => Some(*i),
            Kv::Text(t) => t.trim().parse().ok(),
            Kv::Map(_) => None,
        }
    }

    fn as_text(&self) -> Option<&str> {
        match self {
            Kv::Text(t) => Some(t),
            _ => None,
        }
    }

    fn truthy(&self) -> bool {
        match self {
            Kv::Text(t) => t == "true" || t == "1",
            Kv::Int(i) => *i != 0,
            Kv::Map(_) => false,
        }
    }

    fn int_at(&self, key: &str) -> Option<i64> {
        self.get(key).and_then(Kv::as_i64)
    }
}

/// Reads the Steam client's binary app metadata cache. This is how tonight can
/// name every app you own with no network call and no API key: the client has
/// already downloaded the answer, tags and audio flags included.
pub fn parse(path: &Path) -> Option<HashMap<i64, AppEntry>> {
    let data = std::fs::read(path).ok()?;
    parse_bytes(&data)
}

fn parse_bytes(data: &[u8]) -> Option<HashMap<i64, AppEntry>> {
    let mut c = Cursor { b: data, i: 0 };
    let magic = c.u32()?;
    if !matches!(magic, V27 | V28 | V29) {
        return None;
    }
    let _universe = c.u32()?;

    let table = if magic == V29 {
        let offset = c.i64().filter(|o| *o >= 0)? as usize;
        Some(read_string_table(data, offset)?)
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

struct Reader<'a, 't> {
    c: Cursor<'a>,
    table: Option<&'t [String]>,
    broken: bool,
}

impl Reader<'_, '_> {
    fn key(&mut self) -> Option<String> {
        match self.table {
            Some(t) => {
                let idx = self.c.u32()? as usize;
                t.get(idx).cloned()
            }
            None => self.c.cstr(),
        }
    }

    /// Reads one object up to its end marker. Only `appinfo/common` is kept in
    /// memory; every other section is walked and discarded. A truncated or
    /// unknown-typed blob stops the walk and returns what was read so far.
    fn object(&mut self, depth: usize, keep: bool) -> Kv {
        let mut items = vec![];
        loop {
            let Some(t) = self.c.u8() else {
                self.broken = true;
                break;
            };
            if t == 0x08 {
                break;
            }
            let Some(key) = self.key() else {
                self.broken = true;
                break;
            };
            let key = key.to_ascii_lowercase();
            let value = match t {
                0x00 => {
                    let child_keep = match depth {
                        0 => true,
                        1 => key == "common",
                        _ => keep,
                    };
                    Some(self.object(depth + 1, child_keep))
                }
                0x01 => self.c.cstr().map(Kv::Text),
                0x02 | 0x04 | 0x06 => self.c.i32().map(|v| Kv::Int(v as i64)),
                0x03 => self.c.skip(4).map(|_| Kv::Int(0)),
                0x07 | 0x0A => self.c.i64().map(Kv::Int),
                _ => None,
            };
            let Some(value) = value else {
                self.broken = true;
                break;
            };
            if keep {
                items.push((key, value));
            }
            if self.broken {
                break;
            }
        }
        Kv::Map(items)
    }
}

fn extract(blob: &[u8], table: Option<&[String]>) -> Option<AppEntry> {
    let mut r = Reader {
        c: Cursor { b: blob, i: 0 },
        table,
        broken: false,
    };
    let root = r.object(0, true);
    let common = root.get("appinfo")?.get("common")?;
    let name = common.get("name")?.as_text()?.to_string();
    if name.trim().is_empty() {
        return None;
    }
    Some(AppEntry {
        name,
        kind: common
            .get("type")
            .and_then(Kv::as_text)
            .unwrap_or_default()
            .to_ascii_lowercase(),
        info: info_from(common),
    })
}

fn ordered_ints(node: Option<&Kv>) -> Vec<u32> {
    let Some(node) = node else { return vec![] };
    let mut rows: Vec<(i64, u32)> = node
        .entries()
        .iter()
        .filter_map(|(k, v)| Some((k.parse::<i64>().ok()?, u32::try_from(v.as_i64()?).ok()?)))
        .collect();
    rows.sort_by_key(|(k, _)| *k);
    rows.into_iter().map(|(_, v)| v).collect()
}

fn info_from(common: &Kv) -> LocalInfo {
    let mut info = LocalInfo {
        tags: ordered_ints(common.get("store_tags")),
        genres: ordered_ints(common.get("genres")),
        primary_genre: common
            .int_at("primary_genre")
            .and_then(|g| u32::try_from(g).ok()),
        review_percentage: common.int_at("review_percentage").unwrap_or(0),
        metacritic: common.int_at("metacritic_score").unwrap_or(0),
        release_ts: common
            .int_at("steam_release_date")
            .or_else(|| common.int_at("original_release_date"))
            .unwrap_or(0),
        ..Default::default()
    };

    if let Some(cat) = common.get("category") {
        let mut ids: Vec<u32> = cat
            .entries()
            .iter()
            .filter(|(_, v)| v.truthy())
            .filter_map(|(k, _)| k.strip_prefix("category_")?.parse().ok())
            .collect();
        ids.sort_unstable();
        info.categories = ids;
    }

    for (_, lang) in common
        .get("supported_languages")
        .map(Kv::entries)
        .unwrap_or_default()
    {
        if lang.get("supported").is_some_and(Kv::truthy) {
            info.languages += 1;
        }
        if lang.get("full_audio").is_some_and(Kv::truthy) {
            info.audio_full += 1;
        }
        if lang.get("subtitles").is_some_and(Kv::truthy) {
            info.subtitles += 1;
        }
    }

    if let Some(deck) = common.get("steam_deck_compatibility") {
        info.deck = deck
            .int_at("category")
            .and_then(|c| u8::try_from(c).ok())
            .unwrap_or(0);
        if let Some(cfg) = deck.get("configuration") {
            info.deck_input = cfg
                .get("supported_input")
                .and_then(Kv::as_text)
                .map(str::to_string);
            info.needs_online = cfg
                .get("requires_internet_for_singleplayer")
                .map(Kv::truthy);
        }
    }
    info
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text(out: &mut Vec<u8>, key: &str, value: &str) {
        out.push(0x01);
        out.extend(key.as_bytes());
        out.push(0);
        out.extend(value.as_bytes());
        out.push(0);
    }

    fn int(out: &mut Vec<u8>, key: &str, value: i32) {
        out.push(0x02);
        out.extend(key.as_bytes());
        out.push(0);
        out.extend(value.to_le_bytes());
    }

    fn open(out: &mut Vec<u8>, key: &str) {
        out.push(0x00);
        out.extend(key.as_bytes());
        out.push(0);
    }

    fn close(out: &mut Vec<u8>) {
        out.push(0x08);
    }

    fn sample_blob() -> Vec<u8> {
        let mut kv = vec![];
        open(&mut kv, "appinfo");
        int(&mut kv, "appid", 440);
        open(&mut kv, "config");
        text(&mut kv, "installdir", "x");
        close(&mut kv);
        open(&mut kv, "common");
        text(&mut kv, "name", "Sample Game");
        text(&mut kv, "type", "Game");
        open(&mut kv, "store_tags");
        int(&mut kv, "1", 1742);
        int(&mut kv, "0", 4182);
        close(&mut kv);
        open(&mut kv, "category");
        int(&mut kv, "category_2", 1);
        int(&mut kv, "category_13", 1);
        close(&mut kv);
        open(&mut kv, "supported_languages");
        open(&mut kv, "english");
        text(&mut kv, "supported", "true");
        text(&mut kv, "full_audio", "true");
        close(&mut kv);
        open(&mut kv, "french");
        text(&mut kv, "supported", "true");
        text(&mut kv, "subtitles", "true");
        close(&mut kv);
        close(&mut kv);
        open(&mut kv, "steam_deck_compatibility");
        int(&mut kv, "category", 3);
        open(&mut kv, "configuration");
        text(&mut kv, "supported_input", "gamepad");
        int(&mut kv, "requires_internet_for_singleplayer", 0);
        close(&mut kv);
        close(&mut kv);
        int(&mut kv, "review_percentage", 91);
        close(&mut kv);
        close(&mut kv);
        close(&mut kv);
        kv
    }

    fn v27_file(kv: &[u8]) -> Vec<u8> {
        let mut f = vec![];
        f.extend(V27.to_le_bytes());
        f.extend(1u32.to_le_bytes());
        let mut body = vec![];
        body.extend([0u8; 4 + 4 + 8 + 20 + 4]);
        body.extend(kv);
        f.extend(440u32.to_le_bytes());
        f.extend((body.len() as u32).to_le_bytes());
        f.extend(body);
        f.extend(0u32.to_le_bytes());
        f
    }

    #[test]
    fn decodes_tags_audio_and_deck_from_a_synthetic_cache() {
        let map = parse_bytes(&v27_file(&sample_blob())).expect("parses");
        let app = map.get(&440).expect("app present");
        assert_eq!(app.name, "Sample Game");
        assert_eq!(app.kind, "game");
        assert_eq!(app.info.tags, vec![4182, 1742]);
        assert_eq!(app.info.categories, vec![2, 13]);
        assert_eq!(app.info.languages, 2);
        assert_eq!(app.info.audio_full, 1);
        assert_eq!(app.info.subtitles, 1);
        assert_eq!(app.info.deck, 3);
        assert_eq!(app.info.deck_input.as_deref(), Some("gamepad"));
        assert_eq!(app.info.needs_online, Some(false));
        assert_eq!(app.info.review_percentage, 91);
    }

    #[test]
    fn a_truncated_blob_keeps_what_it_read() {
        let mut kv = sample_blob();
        kv.truncate(kv.len() - 40);
        let map = parse_bytes(&v27_file(&kv)).expect("parses");
        let app = map.get(&440).expect("name survives truncation");
        assert_eq!(app.name, "Sample Game");
        assert_eq!(app.info.tags, vec![4182, 1742]);
    }

    #[test]
    fn rejects_a_file_with_the_wrong_magic() {
        assert!(parse_bytes(&[1, 2, 3, 4, 5, 6, 7, 8]).is_none());
    }

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
            assert!(!hl2.info.tags.is_empty(), "tags should decode");
        }
    }
}
