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

/// How a run of rendered Markdown text looks.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Style {
    Text,
    Strong,
    Code,
    Link,
}

pub type Spans = Vec<(Style, String)>;

/// One block of a Markdown document, for the rendered view.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Block {
    Heading(u8, Spans),
    Paragraph(Spans),
    /// A list item: nesting depth, its marker (`•` or `1.`) and text.
    Item(usize, String, Spans),
    Quote(Spans),
    /// Fenced or indented code, shown as written.
    Code(Vec<String>),
    /// Rows of cells; the first row is a header when `header` is set.
    Table {
        header: bool,
        rows: Vec<Vec<Spans>>,
    },
    Rule,
}

/// Split Markdown into blocks. Covers what pack READMEs use; anything else
/// reads as plain paragraphs.
pub fn markdown_blocks(text: &str) -> Vec<Block> {
    let text = text.replace("\r\n", "\n").replace('\t', "    ");
    let mut blocks = Vec::new();
    let mut paragraph: Vec<&str> = Vec::new();
    let mut code: Option<Vec<String>> = None;
    let flush = |paragraph: &mut Vec<&str>, blocks: &mut Vec<Block>| {
        if !paragraph.is_empty() {
            blocks.push(Block::Paragraph(inline(&paragraph.join(" "))));
            paragraph.clear();
        }
    };
    let mut lines = text.split('\n').peekable();
    while let Some(line) = lines.next() {
        let body = line.trim_start();
        if body.starts_with("```") || body.starts_with("~~~") {
            match code.take() {
                Some(fenced) => blocks.push(Block::Code(fenced)),
                None => {
                    flush(&mut paragraph, &mut blocks);
                    code = Some(Vec::new());
                }
            }
            continue;
        }
        if let Some(fenced) = &mut code {
            fenced.push(line.to_owned());
            continue;
        }
        let indent = line.len() - body.len();
        if body.is_empty() {
            flush(&mut paragraph, &mut blocks);
            continue;
        }
        let rule = body.len() >= 3
            && b"-*_".iter().any(|&c| {
                body.bytes().all(|b| b == c || b == b' ')
                    && body.bytes().filter(|&b| b == c).count() >= 3
            });
        if rule {
            flush(&mut paragraph, &mut blocks);
            blocks.push(Block::Rule);
        } else if let Some(level) = heading_level(body) {
            flush(&mut paragraph, &mut blocks);
            let title = body[level as usize..]
                .trim()
                .trim_end_matches('#')
                .trim_end();
            blocks.push(Block::Heading(level, inline(title)));
        } else if let Some(quote) = body.strip_prefix('>') {
            flush(&mut paragraph, &mut blocks);
            let mut quoted = vec![quote.trim_start().to_owned()];
            while let Some(next) = lines.peek().and_then(|l| l.trim_start().strip_prefix('>')) {
                quoted.push(next.trim_start().to_owned());
                lines.next();
            }
            blocks.push(Block::Quote(inline(&quoted.join(" "))));
        } else if let Some((marker, rest)) = list_marker(body) {
            flush(&mut paragraph, &mut blocks);
            blocks.push(Block::Item(indent / 2, marker, inline(rest)));
        } else if body.starts_with('|') {
            flush(&mut paragraph, &mut blocks);
            let mut rows = vec![line.to_owned()];
            while let Some(next) = lines.peek().filter(|l| l.trim_start().starts_with('|')) {
                rows.push((*next).to_owned());
                lines.next();
            }
            blocks.push(table(&rows));
        } else if indent >= 4 && paragraph.is_empty() {
            let mut rows = vec![line[4..].to_owned()];
            while let Some(next) = lines
                .peek()
                .filter(|l| l.starts_with("    ") || l.trim().is_empty())
            {
                rows.push(next.get(4..).unwrap_or("").to_owned());
                lines.next();
            }
            while rows.last().is_some_and(|r| r.trim().is_empty()) {
                rows.pop();
            }
            blocks.push(Block::Code(rows));
        } else {
            paragraph.push(body);
        }
    }
    if let Some(fenced) = code {
        blocks.push(Block::Code(fenced));
    }
    flush(&mut paragraph, &mut blocks);
    blocks
}

/// A pipe table. The `|---|---|` line marks the row above it as the header.
fn table(lines: &[String]) -> Block {
    let cells = |line: &str| -> Vec<String> {
        let line = line.trim();
        let line = line.strip_prefix('|').unwrap_or(line);
        let line = line.strip_suffix('|').unwrap_or(line);
        line.split('|').map(|cell| cell.trim().to_owned()).collect()
    };
    let separator = |row: &[String]| {
        row.iter().all(|cell| {
            let dashes = cell.trim_matches(':');
            !dashes.is_empty() && dashes.bytes().all(|b| b == b'-')
        })
    };
    let rows: Vec<Vec<String>> = lines.iter().map(|line| cells(line)).collect();
    let header = rows.len() > 1 && separator(&rows[1]);
    Block::Table {
        header,
        rows: rows
            .iter()
            .filter(|row| !separator(row))
            .map(|row| row.iter().map(|cell| inline(cell)).collect())
            .collect(),
    }
}

fn heading_level(line: &str) -> Option<u8> {
    let level = line.bytes().take_while(|&b| b == b'#').count();
    ((1..=6).contains(&level) && line[level..].starts_with(' ')).then_some(level as u8)
}

fn list_marker(line: &str) -> Option<(String, &str)> {
    for bullet in ["- ", "* ", "+ "] {
        if let Some(rest) = line.strip_prefix(bullet) {
            // Task lists keep their box.
            for (box_, mark) in [("[ ] ", "☐"), ("[x] ", "☑"), ("[X] ", "☑")] {
                if let Some(task) = rest.strip_prefix(box_) {
                    return Some((mark.to_owned(), task));
                }
            }
            return Some(("•".to_owned(), rest));
        }
    }
    let digits = line.bytes().take_while(u8::is_ascii_digit).count();
    let rest = line[digits..]
        .strip_prefix(". ")
        .or_else(|| line[digits..].strip_prefix(") "))?;
    (digits > 0 && digits < 10).then(|| (format!("{}.", &line[..digits]), rest))
}

/// Inline Markdown: `code`, **strong**, *emphasis* (shown plain), links and
/// images (their text), and `<https://…>` autolinks.
pub fn inline(text: &str) -> Spans {
    let mut spans: Spans = Vec::new();
    let mut push = |style: Style, text: &str| {
        if text.is_empty() {
            return;
        }
        match spans.last_mut() {
            Some((last, joined)) if *last == style => joined.push_str(text),
            _ => spans.push((style, text.to_owned())),
        }
    };
    let mut strong = false;
    let mut i = 0;
    while i < text.len() {
        let rest = &text[i..];
        let base = if strong { Style::Strong } else { Style::Text };
        if let Some(code) = rest.strip_prefix('`')
            && let Some(end) = code.find('`')
        {
            push(Style::Code, &code[..end]);
            i += end + 2;
        } else if rest.starts_with("**") || rest.starts_with("__") {
            strong = !strong;
            i += 2;
        } else if let Some((label, len)) = link(rest) {
            push(Style::Link, label);
            i += len;
        } else if let Some(url) = rest.strip_prefix('<')
            && let Some(end) = url.find('>')
            && (url.starts_with("http://") || url.starts_with("https://"))
        {
            push(Style::Link, &url[..end]);
            i += end + 2;
        } else if rest.starts_with('\\') && rest.len() > 1 {
            let len = char_len(&rest[1..]);
            push(base, &rest[1..1 + len]);
            i += 1 + len;
        } else if (rest.starts_with('*') || rest.starts_with('_')) && emphasis_marker(text, i) {
            i += 1;
        } else {
            let len = char_len(rest);
            push(base, &rest[..len]);
            i += len;
        }
    }
    spans
}

/// `[label](target)` or `![alt](src)`: the label and the length used.
fn link(text: &str) -> Option<(&str, usize)> {
    let image = usize::from(text.starts_with('!'));
    let rest = text[image..].strip_prefix('[')?;
    let close = rest.find("](")?;
    let target = rest[close + 2..].find(')')?;
    Some((&rest[..close], image + 1 + close + 2 + target + 1))
}

/// A lone `*` or `_` that opens or closes emphasis, rather than one inside a
/// word like snake_case or a spaced-out `2 * 3`.
fn emphasis_marker(text: &str, at: usize) -> bool {
    let before = text[..at].chars().next_back();
    let after = text[at + 1..].chars().next();
    let word = |c: Option<char>| c.is_some_and(char::is_alphanumeric);
    let space = |c: Option<char>| c.is_none_or(char::is_whitespace);
    if text.as_bytes()[at] == b'_' && word(before) && word(after) {
        return false;
    }
    !(space(before) && space(after))
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
    fn markdown_splits_into_blocks() {
        let blocks = markdown_blocks(
            "# Overclock\n\nA total\nconversion.\n\n- one\n  - nested\n1. first\n- [x] done\n\n> quoted\n> more\n\n```lua\nlocal x = 1\n```\n---\n| a | b |\n|---|---|\n\n    indented\n    code\n",
        );
        assert_eq!(
            blocks,
            vec![
                Block::Heading(1, vec![(Style::Text, "Overclock".into())]),
                Block::Paragraph(vec![(Style::Text, "A total conversion.".into())]),
                Block::Item(0, "•".into(), vec![(Style::Text, "one".into())]),
                Block::Item(1, "•".into(), vec![(Style::Text, "nested".into())]),
                Block::Item(0, "1.".into(), vec![(Style::Text, "first".into())]),
                Block::Item(0, "☑".into(), vec![(Style::Text, "done".into())]),
                Block::Quote(vec![(Style::Text, "quoted more".into())]),
                Block::Code(vec!["local x = 1".into()]),
                Block::Rule,
                Block::Table {
                    header: true,
                    rows: vec![vec![
                        vec![(Style::Text, "a".into())],
                        vec![(Style::Text, "b".into())]
                    ]],
                },
                Block::Code(vec!["indented".into(), "code".into()]),
            ]
        );
    }

    #[test]
    fn markdown_inline_styles() {
        assert_eq!(
            inline(
                "Run `mod-bench` for **real** numbers, see [the docs](https://x.y) or <https://a.b>."
            ),
            vec![
                (Style::Text, "Run ".into()),
                (Style::Code, "mod-bench".into()),
                (Style::Text, " for ".into()),
                (Style::Strong, "real".into()),
                (Style::Text, " numbers, see ".into()),
                (Style::Link, "the docs".into()),
                (Style::Text, " or ".into()),
                (Style::Link, "https://a.b".into()),
                (Style::Text, ".".into()),
            ]
        );
        // Emphasis markers go; underscores inside names and lone stars stay.
        assert_eq!(
            inline("*fast* snake_case 2 * 3 \\*literal\\* ![logo](a.png)"),
            vec![
                (Style::Text, "fast snake_case 2 * 3 *literal* ".into()),
                (Style::Link, "logo".into()),
            ]
        );
    }

    #[test]
    fn markdown_tables_keep_cells_and_their_styles() {
        let blocks = markdown_blocks(
            "| Protocol | Effect |\n| :--- | ---: |\n| **Zero-G** | Half `power` |\n| Bare | |\n",
        );
        assert_eq!(
            blocks,
            vec![Block::Table {
                header: true,
                rows: vec![
                    vec![
                        vec![(Style::Text, "Protocol".into())],
                        vec![(Style::Text, "Effect".into())]
                    ],
                    vec![
                        vec![(Style::Strong, "Zero-G".into())],
                        vec![(Style::Text, "Half ".into()), (Style::Code, "power".into())],
                    ],
                    vec![vec![(Style::Text, "Bare".into())], vec![]],
                ],
            }]
        );
        // Without a separator line, there is no header.
        assert!(matches!(
            &markdown_blocks("| a | b |")[0],
            Block::Table { header: false, .. }
        ));
    }

    #[test]
    fn markdown_never_loses_unclosed_or_odd_input() {
        for text in [
            "```\nnever closed",
            "`open",
            "[label](",
            "**bold",
            "#nospace",
            "> ",
            "1.no",
            "| lone",
        ] {
            let blocks = markdown_blocks(text);
            assert!(
                !blocks.is_empty() || text.trim() == ">",
                "{text:?} vanished"
            );
        }
        assert_eq!(
            markdown_blocks("```\nnever closed"),
            vec![Block::Code(vec!["never closed".into()])]
        );
        assert_eq!(
            markdown_blocks("#nospace"),
            vec![Block::Paragraph(vec![(Style::Text, "#nospace".into())])]
        );
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
