use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq)]
pub enum Vdf {
    Str(String),
    Map(BTreeMap<String, Vdf>),
}

impl Vdf {
    pub fn as_str(&self) -> Option<&str> {
        match self {
            Vdf::Str(s) => Some(s),
            _ => None,
        }
    }

    pub fn as_map(&self) -> Option<&BTreeMap<String, Vdf>> {
        match self {
            Vdf::Map(m) => Some(m),
            _ => None,
        }
    }

    /// Case-insensitive lookup of a single child key.
    pub fn child(&self, key: &str) -> Option<&Vdf> {
        let m = self.as_map()?;
        let lower = key.to_ascii_lowercase();
        m.get(&lower).or_else(|| {
            m.iter()
                .find(|(k, _)| k.eq_ignore_ascii_case(&lower))
                .map(|(_, v)| v)
        })
    }

    /// Case-insensitive lookup down a slash-free path of keys.
    pub fn path(&self, keys: &[&str]) -> Option<&Vdf> {
        let mut cur = self;
        for k in keys {
            cur = cur.child(k)?;
        }
        Some(cur)
    }

    pub fn get_str(&self, key: &str) -> Option<&str> {
        self.child(key)?.as_str()
    }

    pub fn get_i64(&self, key: &str) -> Option<i64> {
        self.get_str(key)?.trim().parse::<i64>().ok()
    }
}

struct Lexer<'a> {
    b: &'a [u8],
    i: usize,
}

impl<'a> Lexer<'a> {
    fn new(s: &'a str) -> Self {
        Lexer {
            b: s.as_bytes(),
            i: 0,
        }
    }

    fn skip_trivia(&mut self) {
        loop {
            while self.i < self.b.len() && (self.b[self.i] as char).is_ascii_whitespace() {
                self.i += 1;
            }
            if self.i + 1 < self.b.len() && self.b[self.i] == b'/' && self.b[self.i + 1] == b'/' {
                while self.i < self.b.len() && self.b[self.i] != b'\n' {
                    self.i += 1;
                }
                continue;
            }
            break;
        }
    }

    fn quoted(&mut self) -> String {
        self.i += 1;
        let mut out = String::new();
        while self.i < self.b.len() {
            let c = self.b[self.i];
            if c == b'\\' && self.i + 1 < self.b.len() {
                let n = self.b[self.i + 1];
                out.push(match n {
                    b'n' => '\n',
                    b't' => '\t',
                    b'\\' => '\\',
                    b'"' => '"',
                    other => other as char,
                });
                self.i += 2;
                continue;
            }
            if c == b'"' {
                self.i += 1;
                break;
            }
            let start = self.i;
            let len = utf8_len(c);
            self.i += len;
            let end = self.i.min(self.b.len());
            out.push_str(&String::from_utf8_lossy(&self.b[start..end]));
        }
        out
    }

    fn bare(&mut self) -> String {
        let start = self.i;
        while self.i < self.b.len() {
            let c = self.b[self.i];
            if (c as char).is_ascii_whitespace() || c == b'{' || c == b'}' || c == b'"' {
                break;
            }
            self.i += 1;
        }
        String::from_utf8_lossy(&self.b[start..self.i]).into_owned()
    }

    fn token(&mut self) -> Option<Token> {
        self.skip_trivia();
        if self.i >= self.b.len() {
            return None;
        }
        match self.b[self.i] {
            b'{' => {
                self.i += 1;
                Some(Token::Open)
            }
            b'}' => {
                self.i += 1;
                Some(Token::Close)
            }
            b'"' => Some(Token::Word(self.quoted())),
            _ => {
                let w = self.bare();
                if w.is_empty() {
                    self.i += 1;
                    self.token()
                } else {
                    Some(Token::Word(w))
                }
            }
        }
    }
}

fn utf8_len(b: u8) -> usize {
    if b < 0x80 {
        1
    } else if b >> 5 == 0b110 {
        2
    } else if b >> 4 == 0b1110 {
        3
    } else if b >> 3 == 0b11110 {
        4
    } else {
        1
    }
}

enum Token {
    Open,
    Close,
    Word(String),
}

/// Parses Valve's text KeyValues format into a case-normalised tree.
///
/// Tolerant by design: unbalanced braces at EOF, `//` comments, bare tokens and
/// duplicate keys (last wins) all parse rather than error, because real Steam
/// files in the wild contain all of them.
pub fn parse(input: &str) -> Vdf {
    let mut lx = Lexer::new(input);
    let mut root = BTreeMap::new();
    parse_into(&mut lx, &mut root, 0);
    Vdf::Map(root)
}

fn parse_into(lx: &mut Lexer, map: &mut BTreeMap<String, Vdf>, depth: usize) {
    if depth > 64 {
        return;
    }
    while let Some(tok) = lx.token() {
        let key = match tok {
            Token::Close => return,
            Token::Open => continue,
            Token::Word(w) => w,
        };
        match lx.token() {
            Some(Token::Open) => {
                let mut inner = BTreeMap::new();
                parse_into(lx, &mut inner, depth + 1);
                map.insert(key.to_ascii_lowercase(), Vdf::Map(inner));
            }
            Some(Token::Word(v)) => {
                map.insert(key.to_ascii_lowercase(), Vdf::Str(v));
            }
            Some(Token::Close) => {
                map.insert(key.to_ascii_lowercase(), Vdf::Str(String::new()));
                return;
            }
            None => {
                map.insert(key.to_ascii_lowercase(), Vdf::Str(String::new()));
                return;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_nested_quoted_pairs() {
        let v = parse(r#""root" { "a" "1" "b" { "c" "hi there" } }"#);
        assert_eq!(v.path(&["root", "a"]).unwrap().as_str(), Some("1"));
        assert_eq!(
            v.path(&["root", "b", "c"]).unwrap().as_str(),
            Some("hi there")
        );
    }

    #[test]
    fn lookup_is_case_insensitive() {
        let v = parse(
            r#""UserLocalConfigStore" { "Software" { "Valve" { "Steam" { "apps" { "220" { "Playtime" "90" } } } } } }"#,
        );
        let apps = v
            .path(&["userlocalconfigstore", "software", "valve", "steam", "apps"])
            .unwrap();
        assert_eq!(
            apps.path(&["220", "playtime"]).unwrap().as_str(),
            Some("90")
        );
    }

    #[test]
    fn survives_comments_and_truncation() {
        let v = parse("\"r\"\n{\n// comment\n\"k\" \"v\"\n");
        assert_eq!(v.path(&["r", "k"]).unwrap().as_str(), Some("v"));
    }

    #[test]
    fn keeps_utf8_names_intact() {
        let v = parse(r#""r" { "name" "Ōkami HD — 大神" }"#);
        assert_eq!(
            v.path(&["r", "name"]).unwrap().as_str(),
            Some("Ōkami HD — 大神")
        );
    }
}
