//! Syntax coloring for the mod file viewer. Each language is a small
//! hand-written scanner: enough to tell keywords, strings, numbers and
//! comments apart in Lua, JSON, TOML, Markdown and SVG/XML, never a parser.
//! Text that doesn't match anything stays plain, so odd input still shows.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Token {
    Plain,
    Keyword,
    /// `nil`, `true`, `false`, `null`.
    Constant,
    /// Globals every Lua pack uses: `tankyard`, `math`, `pairs`…
    Builtin,
    Function,
    String,
    Number,
    Comment,
    /// JSON and TOML keys, XML attribute names.
    Key,
    Punct,
    /// Markdown headings, TOML tables.
    Heading,
    /// XML and SVG element names.
    Tag,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Language {
    Lua,
    Json,
    Toml,
    Markdown,
    Markup,
    Plain,
}

impl Language {
    pub fn for_file(name: &str) -> Self {
        match tankyard_mods::assets::extension(name).as_str() {
            "lua" => Self::Lua,
            "json" | "gltf" => Self::Json,
            "toml" => Self::Toml,
            "md" | "markdown" => Self::Markdown,
            "svg" | "xml" | "html" | "htm" => Self::Markup,
            _ => Self::Plain,
        }
    }
    pub fn name(self) -> &'static str {
        match self {
            Self::Lua => "Lua",
            Self::Json => "JSON",
            Self::Toml => "TOML",
            Self::Markdown => "Markdown",
            Self::Markup => "XML",
            Self::Plain => "Text",
        }
    }
}

pub type Line = Vec<(Token, String)>;

/// Colored lines of `text`. Tabs become four spaces.
pub fn highlight(language: Language, text: &str) -> Vec<Line> {
    let text = text.replace("\r\n", "\n").replace('\t', "    ");
    let mut out = Out::default();
    match language {
        Language::Lua => lua(&text, &mut out),
        Language::Json => json(&text, &mut out),
        Language::Toml => by_line(&text, &mut out, toml_line),
        Language::Markdown => markdown(&text, &mut out),
        Language::Markup => markup(&text, &mut out),
        Language::Plain => out.push(Token::Plain, &text),
    }
    out.finish()
}

#[derive(Default)]
struct Out {
    lines: Vec<Line>,
    current: Line,
}

impl Out {
    /// Add `text` as one token, starting a new line at each line break.
    fn push(&mut self, token: Token, text: &str) {
        for (i, part) in text.split('\n').enumerate() {
            if i > 0 {
                self.lines.push(std::mem::take(&mut self.current));
            }
            if part.is_empty() {
                continue;
            }
            match self.current.last_mut() {
                Some((last, joined)) if *last == token => joined.push_str(part),
                _ => self.current.push((token, part.to_owned())),
            }
        }
    }
    fn finish(mut self) -> Vec<Line> {
        self.lines.push(self.current);
        self.lines
    }
}

/// Byte length of the char at the start of `rest`.
fn char_len(rest: &str) -> usize {
    rest.chars().next().map_or(1, char::len_utf8)
}

fn ident_len(rest: &str) -> usize {
    rest.char_indices()
        .find(|&(i, c)| !(c == '_' || c.is_ascii_alphabetic() || (i > 0 && c.is_ascii_digit())))
        .map_or(rest.len(), |(i, _)| i)
}

fn number_len(rest: &str) -> usize {
    let bytes = rest.as_bytes();
    if rest.starts_with("0x") || rest.starts_with("0X") {
        return 2 + rest[2..]
            .bytes()
            .take_while(|b| b.is_ascii_hexdigit())
            .count();
    }
    let mut i = 0;
    while i < bytes.len() {
        let b = bytes[i];
        let exponent_sign =
            (b == b'-' || b == b'+') && i > 0 && matches!(bytes[i - 1], b'e' | b'E');
        if b.is_ascii_digit() || b == b'.' || b == b'_' || matches!(b, b'e' | b'E') || exponent_sign
        {
            i += 1;
        } else {
            break;
        }
    }
    i
}

/// A quoted string from `rest`'s opening quote to its closing one or the end
/// of the line, honoring backslash escapes.
fn quoted_len(rest: &str) -> usize {
    let quote = rest.as_bytes()[0];
    let mut escaped = false;
    for (i, b) in rest.bytes().enumerate().skip(1) {
        match b {
            b'\n' => return i,
            _ if escaped => escaped = false,
            b'\\' => escaped = true,
            _ if b == quote => return i + 1,
            _ => {}
        }
    }
    rest.len()
}

/// Lua's `[[ … ]]` and `[==[ … ]==]` brackets, if `rest` opens one.
fn long_bracket_len(rest: &str) -> Option<usize> {
    let level = rest
        .strip_prefix('[')?
        .bytes()
        .take_while(|&b| b == b'=')
        .count();
    if rest.as_bytes().get(level + 1) != Some(&b'[') {
        return None;
    }
    let close = format!("]{}]", "=".repeat(level));
    Some(
        rest[level + 2..]
            .find(&close)
            .map_or(rest.len(), |end| level + 2 + end + close.len()),
    )
}

const LUA_KEYWORDS: &[&str] = &[
    "and", "break", "do", "else", "elseif", "end", "for", "function", "goto", "if", "in", "local",
    "not", "or", "repeat", "return", "then", "until", "while",
];
const LUA_BUILTINS: &[&str] = &[
    "tankyard",
    "self",
    "math",
    "string",
    "table",
    "pairs",
    "ipairs",
    "next",
    "select",
    "type",
    "tostring",
    "tonumber",
    "print",
    "require",
    "error",
    "assert",
    "pcall",
    "xpcall",
    "unpack",
    "setmetatable",
    "getmetatable",
    "rawget",
    "rawset",
    "rawequal",
    "rawlen",
    "coroutine",
    "bit",
];

fn lua(text: &str, out: &mut Out) {
    let mut i = 0;
    let mut after_dot = false;
    while i < text.len() {
        let rest = &text[i..];
        let c = rest.as_bytes()[0];
        let (token, len) = if let Some(comment) = rest.strip_prefix("--") {
            let len = long_bracket_len(comment)
                .unwrap_or_else(|| comment.find('\n').unwrap_or(comment.len()));
            (Token::Comment, 2 + len)
        } else if let Some(len) = long_bracket_len(rest) {
            (Token::String, len)
        } else if c == b'"' || c == b'\'' {
            (Token::String, quoted_len(rest))
        } else if c.is_ascii_digit()
            || (c == b'.' && rest.as_bytes().get(1).is_some_and(u8::is_ascii_digit))
        {
            (Token::Number, number_len(rest))
        } else if c == b'_' || c.is_ascii_alphabetic() {
            let len = ident_len(rest);
            let word = &rest[..len];
            let next = rest[len..]
                .trim_start_matches(' ')
                .as_bytes()
                .first()
                .copied();
            let token = if after_dot {
                if matches!(next, Some(b'(' | b'"' | b'{')) {
                    Token::Function
                } else {
                    Token::Plain
                }
            } else if LUA_KEYWORDS.contains(&word) {
                Token::Keyword
            } else if matches!(word, "nil" | "true" | "false") {
                Token::Constant
            } else if LUA_BUILTINS.contains(&word) {
                Token::Builtin
            } else if matches!(next, Some(b'(' | b'"' | b'{')) {
                Token::Function
            } else {
                Token::Plain
            };
            (token, len)
        } else if c.is_ascii_whitespace() {
            (
                Token::Plain,
                rest.bytes().take_while(u8::is_ascii_whitespace).count(),
            )
        } else if c.is_ascii_punctuation() {
            (Token::Punct, 1)
        } else {
            (Token::Plain, char_len(rest))
        };
        if token != Token::Plain || !rest[..len].trim().is_empty() {
            after_dot = token == Token::Punct
                && matches!(c, b'.' | b':')
                && rest.as_bytes().get(1) != Some(&b'.');
        }
        out.push(token, &rest[..len]);
        i += len;
    }
}

fn json(text: &str, out: &mut Out) {
    let mut i = 0;
    while i < text.len() {
        let rest = &text[i..];
        let c = rest.as_bytes()[0];
        let (token, len) = if c == b'"' {
            let len = quoted_len(rest);
            let key = rest[len..].trim_start().starts_with(':');
            (if key { Token::Key } else { Token::String }, len)
        } else if c.is_ascii_digit() || c == b'-' {
            (Token::Number, 1 + number_len(&rest[1..]))
        } else if c.is_ascii_alphabetic() {
            let len = ident_len(rest);
            let token = if matches!(&rest[..len], "true" | "false" | "null") {
                Token::Constant
            } else {
                Token::Plain
            };
            (token, len)
        } else if c.is_ascii_whitespace() {
            (
                Token::Plain,
                rest.bytes().take_while(u8::is_ascii_whitespace).count(),
            )
        } else if c.is_ascii_punctuation() {
            (Token::Punct, 1)
        } else {
            (Token::Plain, char_len(rest))
        };
        out.push(token, &rest[..len]);
        i += len;
    }
}

fn by_line(text: &str, out: &mut Out, each: fn(&str, &mut Out)) {
    for (i, line) in text.split('\n').enumerate() {
        if i > 0 {
            out.push(Token::Plain, "\n");
        }
        each(line, out);
    }
}

fn toml_line(line: &str, out: &mut Out) {
    let body = line.trim_start();
    out.push(Token::Plain, &line[..line.len() - body.len()]);
    if body.starts_with('#') {
        return out.push(Token::Comment, body);
    }
    if body.starts_with('[') {
        let end = body.find(']').map_or(body.len(), |e| {
            e + body[e..].bytes().take_while(|&b| b == b']').count()
        });
        out.push(Token::Heading, &body[..end]);
        return value(&body[end..], out);
    }
    match body.find('=') {
        Some(equals) if !body[..equals].contains(['"', '\'']) || body.starts_with(['"', '\'']) => {
            out.push(Token::Key, &body[..equals]);
            out.push(Token::Punct, "=");
            value(&body[equals + 1..], out);
        }
        _ => value(body, out),
    }
}

/// A TOML value, or the rest of a line after its key.
fn value(text: &str, out: &mut Out) {
    let mut i = 0;
    while i < text.len() {
        let rest = &text[i..];
        let c = rest.as_bytes()[0];
        let (token, len) = if c == b'#' {
            (Token::Comment, rest.len())
        } else if c == b'"' || c == b'\'' {
            (Token::String, quoted_len(rest))
        } else if c.is_ascii_digit()
            || ((c == b'-' || c == b'+') && rest.as_bytes().get(1).is_some_and(u8::is_ascii_digit))
        {
            // Dates and times stay one token: 2026-11-08T10:00:00Z.
            let len = rest
                .bytes()
                .take_while(|b| {
                    b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-' | b'+' | b':')
                })
                .count();
            (Token::Number, len)
        } else if c.is_ascii_alphabetic() {
            let len = ident_len(rest);
            let token = if matches!(&rest[..len], "true" | "false" | "inf" | "nan") {
                Token::Constant
            } else {
                Token::Plain
            };
            (token, len)
        } else if c.is_ascii_whitespace() {
            (
                Token::Plain,
                rest.bytes().take_while(u8::is_ascii_whitespace).count(),
            )
        } else if c.is_ascii_punctuation() {
            (Token::Punct, 1)
        } else {
            (Token::Plain, char_len(rest))
        };
        out.push(token, &rest[..len]);
        i += len;
    }
}

fn markdown(text: &str, out: &mut Out) {
    let mut fenced = false;
    for (i, line) in text.split('\n').enumerate() {
        if i > 0 {
            out.push(Token::Plain, "\n");
        }
        let body = line.trim_start();
        if body.starts_with("```") || body.starts_with("~~~") {
            fenced = !fenced;
            out.push(Token::Punct, line);
        } else if fenced {
            out.push(Token::String, line);
        } else if body.starts_with('#') {
            out.push(Token::Heading, line);
        } else {
            let indent = line.len() - body.len();
            out.push(Token::Plain, &line[..indent]);
            let marker = ["- ", "* ", "+ ", "> "]
                .iter()
                .find(|m| body.starts_with(*m))
                .map(|m| m.len())
                .or_else(|| {
                    let digits = body.bytes().take_while(u8::is_ascii_digit).count();
                    (digits > 0 && body[digits..].starts_with(". ")).then_some(digits + 2)
                })
                .unwrap_or(0);
            out.push(Token::Punct, &body[..marker]);
            inline_markdown(&body[marker..], out);
        }
    }
}

/// `code` spans and link targets inside a Markdown line.
fn inline_markdown(text: &str, out: &mut Out) {
    let mut i = 0;
    while i < text.len() {
        let rest = &text[i..];
        let (token, len) = if let Some(code) = rest.strip_prefix('`') {
            (Token::String, code.find('`').map_or(rest.len(), |e| e + 2))
        } else if rest.starts_with("](") {
            out.push(Token::Punct, "]");
            i += 1;
            continue;
        } else if rest.starts_with('(') && i > 0 && text.as_bytes()[i - 1] == b']' {
            (Token::Comment, rest.find(')').map_or(rest.len(), |e| e + 1))
        } else if rest.starts_with("**") {
            (Token::Punct, 2)
        } else {
            let first = char_len(rest);
            let len = rest[first..]
                .find(['`', ']', '(', '*'])
                .map_or(rest.len(), |e| first + e);
            (Token::Plain, len)
        };
        out.push(token, &rest[..len]);
        i += len;
    }
}

fn markup(text: &str, out: &mut Out) {
    let mut i = 0;
    while i < text.len() {
        let rest = &text[i..];
        if rest.starts_with("<!--") {
            let len = rest.find("-->").map_or(rest.len(), |e| e + 3);
            out.push(Token::Comment, &rest[..len]);
            i += len;
        } else if rest.starts_with('<') {
            i += tag(rest, out);
        } else {
            let len = rest.find('<').unwrap_or(rest.len());
            out.push(Token::Plain, &rest[..len]);
            i += len;
        }
    }
}

/// One `<tag attribute="value">`, returning its length.
fn tag(text: &str, out: &mut Out) -> usize {
    let open = if text.starts_with("</") || text.starts_with("<?") || text.starts_with("<!") {
        2
    } else {
        1
    };
    out.push(Token::Punct, &text[..open]);
    let mut i = open;
    let name = text[i..]
        .find(|c: char| c.is_whitespace() || c == '>' || c == '/')
        .unwrap_or(text.len() - i);
    out.push(Token::Tag, &text[i..i + name]);
    i += name;
    while i < text.len() {
        let rest = &text[i..];
        let c = rest.as_bytes()[0];
        if c == b'>' {
            out.push(Token::Punct, ">");
            return i + 1;
        }
        let (token, len) = if rest.starts_with("/>") || rest.starts_with("?>") {
            out.push(Token::Punct, &rest[..2]);
            return i + 2;
        } else if c == b'"' || c == b'\'' {
            let len = rest[1..].find(c as char).map_or(rest.len(), |e| e + 2);
            (Token::String, len)
        } else if c == b'=' {
            (Token::Punct, 1)
        } else if c.is_ascii_whitespace() {
            (
                Token::Plain,
                rest.bytes().take_while(u8::is_ascii_whitespace).count(),
            )
        } else {
            let len = rest
                .find(|c: char| c.is_whitespace() || matches!(c, '=' | '>' | '/' | '"' | '\''))
                .filter(|&e| e > 0)
                .unwrap_or_else(|| char_len(rest));
            (Token::Key, len)
        };
        out.push(token, &rest[..len]);
        i += len;
    }
    i
}

/// A classic hex dump of `bytes`: offset, sixteen bytes, and their printable
/// characters.
pub fn hex_dump(bytes: &[u8]) -> Vec<Line> {
    bytes
        .chunks(16)
        .enumerate()
        .map(|(row, chunk)| {
            let mut hex = String::with_capacity(50);
            for (i, byte) in chunk.iter().enumerate() {
                if i == 8 {
                    hex.push(' ');
                }
                hex.push_str(&format!("{byte:02x} "));
            }
            let width = 16 * 3 + 1;
            let text: String = chunk
                .iter()
                .map(|&b| {
                    if b.is_ascii_graphic() || b == b' ' {
                        b as char
                    } else {
                        '.'
                    }
                })
                .collect();
            vec![
                (Token::Comment, format!("{:08x}  ", row * 16)),
                (Token::Number, format!("{hex:width$} ")),
                (Token::String, text),
            ]
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tokens(language: Language, text: &str) -> Vec<(Token, String)> {
        highlight(language, text).into_iter().flatten().collect()
    }
    fn find(spans: &[(Token, String)], text: &str) -> Token {
        spans
            .iter()
            .find(|(_, t)| t == text)
            .unwrap_or_else(|| panic!("no span {text:?} in {spans:?}"))
            .0
    }

    #[test]
    fn lua_tells_keywords_strings_calls_and_comments_apart() {
        let spans = tokens(
            Language::Lua,
            "local bonus = 3 -- extra points\ntankyard.on(\"kill\", function(e) return nil end)\nlocal s = [[a\nb]]",
        );
        assert_eq!(find(&spans, "local"), Token::Keyword);
        assert_eq!(find(&spans, "3"), Token::Number);
        assert_eq!(find(&spans, "-- extra points"), Token::Comment);
        assert_eq!(find(&spans, "tankyard"), Token::Builtin);
        assert_eq!(find(&spans, "on"), Token::Function);
        assert_eq!(find(&spans, "\"kill\""), Token::String);
        assert_eq!(find(&spans, "nil"), Token::Constant);
        assert_eq!(find(&spans, " bonus "), Token::Plain);
    }

    #[test]
    fn multi_line_tokens_keep_their_color_on_every_line() {
        let lines = highlight(
            Language::Lua,
            "--[[ one\ntwo ]] x\nlocal s = [==[\nthree]==]",
        );
        assert_eq!(lines.len(), 4);
        assert_eq!(lines[0], vec![(Token::Comment, "--[[ one".to_owned())]);
        assert_eq!(lines[1][0], (Token::Comment, "two ]]".to_owned()));
        assert_eq!(lines[3][0], (Token::String, "three]==]".to_owned()));
    }

    #[test]
    fn json_keys_differ_from_string_values() {
        let spans = tokens(
            Language::Json,
            r#"{ "id": "overclock", "max": -1.5e3, "on": true }"#,
        );
        assert_eq!(find(&spans, "\"id\""), Token::Key);
        assert_eq!(find(&spans, "\"overclock\""), Token::String);
        assert_eq!(find(&spans, "-1.5e3"), Token::Number);
        assert_eq!(find(&spans, "true"), Token::Constant);
    }

    #[test]
    fn toml_tables_keys_values_and_comments() {
        let spans = tokens(
            Language::Toml,
            "[project]\nname = \"Overclock\" # shown\nversion = 2\nreleased = 2026-11-08\n[[maps]]",
        );
        assert_eq!(find(&spans, "[project]"), Token::Heading);
        assert_eq!(find(&spans, "name "), Token::Key);
        assert_eq!(find(&spans, "\"Overclock\""), Token::String);
        assert_eq!(find(&spans, "# shown"), Token::Comment);
        assert_eq!(find(&spans, "2026-11-08"), Token::Number);
        assert_eq!(find(&spans, "[[maps]]"), Token::Heading);
    }

    #[test]
    fn markup_tags_attributes_and_comments() {
        let spans = tokens(
            Language::Markup,
            "<!-- art -->\n<svg width=\"8\"><rect fill='#fff'/></svg>",
        );
        assert_eq!(find(&spans, "<!-- art -->"), Token::Comment);
        assert_eq!(find(&spans, "svg"), Token::Tag);
        assert_eq!(find(&spans, "width"), Token::Key);
        assert_eq!(find(&spans, "\"8\""), Token::String);
        assert_eq!(find(&spans, "'#fff'"), Token::String);
    }

    #[test]
    fn markdown_headings_code_and_lists() {
        let spans = tokens(
            Language::Markdown,
            "# Title\n- run `mod-bench`\n```\nlocal x\n```",
        );
        assert_eq!(find(&spans, "# Title"), Token::Heading);
        assert_eq!(find(&spans, "- "), Token::Punct);
        assert_eq!(find(&spans, "`mod-bench`"), Token::String);
        assert_eq!(find(&spans, "local x"), Token::String);
    }

    #[test]
    fn every_scanner_keeps_all_text_including_odd_input() {
        let samples = [
            "x = 'unterminated\n\"also",
            "--[[ never closed",
            "{\"a\": [1, 2, \"é\"]}\u{1F600}",
            "<svg <<< = \"\n>",
            "key = \"v\" # c\n[t\n= odd",
            "`open code\n](lonely",
            "\t\ttabs\r\nand crlf",
        ];
        for language in [
            Language::Lua,
            Language::Json,
            Language::Toml,
            Language::Markdown,
            Language::Markup,
            Language::Plain,
        ] {
            for sample in samples {
                let expected = sample.replace("\r\n", "\n").replace('\t', "    ");
                let joined = highlight(language, sample)
                    .iter()
                    .map(|line| line.iter().map(|(_, t)| t.as_str()).collect::<String>())
                    .collect::<Vec<_>>()
                    .join("\n");
                assert_eq!(joined, expected, "{language:?} changed {sample:?}");
            }
        }
    }

    #[test]
    fn hex_dump_shows_offsets_bytes_and_printable_text() {
        let lines = hex_dump(b"PK\x03\x04hello world, binary!");
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0][0].1, "00000000  ");
        assert!(lines[0][1].1.starts_with("50 4b 03 04 68 65 6c 6c  6f "));
        assert_eq!(lines[0][2].1, "PK..hello world,");
        assert_eq!(lines[1][0].1, "00000010  ");
        assert_eq!(lines[1][2].1, " binary!");
    }
}
