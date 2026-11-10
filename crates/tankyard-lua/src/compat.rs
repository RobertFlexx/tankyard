//! Source compatibility with packs written for LuaJIT.
//!
//! Luau runs Lua 5.1 code unchanged, and the vendored parser accepts LuaJIT's
//! Lua 5.3 bitwise operators. `goto` is the one LuaJIT statement Luau lacks.
//! Packs use it almost only to skip to the end of a loop body, which is
//! Luau's `continue`, so that form is rewritten; any other `goto` is reported
//! with its line instead of a bare syntax error.
use std::borrow::Cow;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Token<'a> {
    Name(&'a str),
    Keyword(&'a str),
    Symbol(&'a str),
    Number,
    String,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct Spanned<'a> {
    pub token: Token<'a>,
    pub start: usize,
    pub end: usize,
    pub line: usize,
}

const KEYWORDS: [&str; 22] = [
    "and", "break", "do", "else", "elseif", "end", "false", "for", "function", "goto", "if", "in",
    "local", "nil", "not", "or", "repeat", "return", "then", "true", "until", "while",
];
const SYMBOLS: [&str; 21] = [
    "...", "..=", "//=", "..", "==", "~=", "<=", ">=", "<<", ">>", "::", "//", "+=", "-=", "*=",
    "/=", "%=", "^=", "->", "[[", "]]",
];

/// The tokens of a chunk. Comments, whitespace and the contents of strings
/// are skipped; malformed input ends the list early and is left to the
/// compiler to report.
pub(crate) fn tokens(source: &str) -> Vec<Token<'_>> {
    spanned(source).into_iter().map(|t| t.token).collect()
}

/// `[[`, `[=[` … at `at`: the level of a long bracket.
fn long_bracket(bytes: &[u8], at: usize) -> Option<usize> {
    if bytes.get(at) != Some(&b'[') {
        return None;
    }
    let mut level = 0;
    while bytes.get(at + 1 + level) == Some(&b'=') {
        level += 1;
    }
    (bytes.get(at + 1 + level) == Some(&b'[')).then_some(level)
}

/// The index just past the long bracket opened at `at` with `level`.
fn skip_long(bytes: &[u8], at: usize, level: usize) -> usize {
    let mut i = at + level + 2;
    while i < bytes.len() {
        if bytes[i] == b']' {
            let mut j = i + 1;
            let mut equals = 0;
            while bytes.get(j) == Some(&b'=') {
                equals += 1;
                j += 1;
            }
            if equals == level && bytes.get(j) == Some(&b']') {
                return j + 1;
            }
        }
        i += 1;
    }
    bytes.len()
}

pub(crate) fn spanned(source: &str) -> Vec<Spanned<'_>> {
    let bytes = source.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    let mut line = 1;
    let count_lines =
        |from: usize, to: usize| bytes[from..to].iter().filter(|&&b| b == b'\n').count();
    while i < bytes.len() {
        let c = bytes[i];
        let start = i;
        if c == b'\n' {
            line += 1;
            i += 1;
            continue;
        }
        if c.is_ascii_whitespace() {
            i += 1;
            continue;
        }
        if c == b'-' && bytes.get(i + 1) == Some(&b'-') {
            if let Some(level) = long_bracket(bytes, i + 2) {
                let end = skip_long(bytes, i + 2, level);
                line += count_lines(i, end);
                i = end;
            } else {
                while i < bytes.len() && bytes[i] != b'\n' {
                    i += 1;
                }
            }
            continue;
        }
        if let Some(level) = long_bracket(bytes, i) {
            let end = skip_long(bytes, i, level);
            out.push(Spanned {
                token: Token::String,
                start,
                end,
                line,
            });
            line += count_lines(i, end);
            i = end;
            continue;
        }
        if c == b'"' || c == b'\'' || c == b'`' {
            i += 1;
            let mut braces = 0_usize;
            while i < bytes.len() {
                match bytes[i] {
                    b'\\' => i += 1,
                    b'{' if c == b'`' => braces += 1,
                    b'}' if c == b'`' && braces > 0 => braces -= 1,
                    b'\n' if c != b'`' && bytes.get(i.wrapping_sub(1)) != Some(&b'\\') => break,
                    q if q == c && braces == 0 => {
                        i += 1;
                        break;
                    }
                    _ => {}
                }
                i += 1;
            }
            let end = i.min(bytes.len());
            out.push(Spanned {
                token: Token::String,
                start,
                end,
                line,
            });
            line += count_lines(start, end);
            continue;
        }
        if c.is_ascii_digit() || (c == b'.' && bytes.get(i + 1).is_some_and(u8::is_ascii_digit)) {
            // As LuaJIT scans: a sign belongs to the number only right after
            // its exponent marker (`e`, or `p` in hexadecimal).
            let hex = bytes[start..].starts_with(b"0x") || bytes[start..].starts_with(b"0X");
            let exponent = if hex { b'p' } else { b'e' };
            while i < bytes.len() {
                let b = bytes[i];
                let signed = (b == b'+' || b == b'-') && (bytes[i - 1] | 0x20) == exponent;
                if b.is_ascii_alphanumeric() || b == b'_' || b == b'.' || signed {
                    i += 1;
                } else {
                    break;
                }
            }
            out.push(Spanned {
                token: Token::Number,
                start,
                end: i,
                line,
            });
            continue;
        }
        if c.is_ascii_alphabetic() || c == b'_' {
            while i < bytes.len() && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_') {
                i += 1;
            }
            let word = &source[start..i];
            out.push(Spanned {
                token: if KEYWORDS.contains(&word) {
                    Token::Keyword(word)
                } else {
                    Token::Name(word)
                },
                start,
                end: i,
                line,
            });
            continue;
        }
        let symbol = SYMBOLS
            .iter()
            .find(|s| source[i..].starts_with(**s))
            .map_or_else(|| &source[i..i + utf8_len(c)], |s| &source[i..i + s.len()]);
        i += symbol.len();
        out.push(Spanned {
            token: Token::Symbol(symbol),
            start,
            end: i,
            line,
        });
    }
    out
}

fn utf8_len(first: u8) -> usize {
    match first {
        0xf0..=0xff => 4,
        0xe0..=0xef => 3,
        0xc0..=0xdf => 2,
        _ => 1,
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Block {
    Function,
    /// `while`/`for`; true until its `do` is read.
    Loop(bool),
    Repeat,
    Do,
    If,
}

struct Label {
    name: String,
    start: usize,
    end: usize,
    line: usize,
    /// The loop whose body this label ends, if it does.
    ends_loop: Option<usize>,
}

struct Goto {
    name: String,
    start: usize,
    end: usize,
    line: usize,
    /// The innermost loop around the goto, unless a function intervenes.
    innermost_loop: Option<usize>,
}

/// The source to give Luau: unchanged unless it uses `goto`. Precompiled
/// chunks (LuaJIT's or Luau's) are refused with LuaJIT's message.
pub(crate) fn luau_source<'a>(file: &str, source: &'a str) -> Result<Cow<'a, str>, String> {
    if source
        .bytes()
        .next()
        .is_some_and(|b| b == 0x1b || (b < b' ' && !b.is_ascii_whitespace()))
    {
        return Err(format!(
            "{file}: attempt to load a binary chunk (mode is 't')"
        ));
    }
    if !source.contains("goto") && !source.contains("::") {
        return Ok(Cow::Borrowed(source));
    }
    let tokens = spanned(source);
    if !tokens
        .iter()
        .any(|t| t.token == Token::Keyword("goto") || t.token == Token::Symbol("::"))
    {
        return Ok(Cow::Borrowed(source));
    }
    // Block ids, innermost last.
    let mut stack: Vec<(usize, Block)> = Vec::new();
    let mut next_id = 0;
    let mut labels = Vec::new();
    let mut gotos = Vec::new();
    let mut index = 0;
    while index < tokens.len() {
        let token = tokens[index];
        match token.token {
            Token::Keyword("function") => {
                stack.push((next_id, Block::Function));
                next_id += 1;
            }
            Token::Keyword("while" | "for") => {
                stack.push((next_id, Block::Loop(true)));
                next_id += 1;
            }
            Token::Keyword("do") => {
                if let Some((_, Block::Loop(awaiting @ true))) = stack.last_mut() {
                    *awaiting = false;
                } else {
                    stack.push((next_id, Block::Do));
                    next_id += 1;
                }
            }
            Token::Keyword("if") => {
                stack.push((next_id, Block::If));
                next_id += 1;
            }
            Token::Keyword("repeat") => {
                stack.push((next_id, Block::Repeat));
                next_id += 1;
            }
            Token::Keyword("end" | "until") => {
                stack.pop();
            }
            Token::Keyword("goto") => {
                if let Some(Spanned {
                    token: Token::Name(name),
                    end,
                    ..
                }) = tokens.get(index + 1)
                {
                    let innermost_loop = stack
                        .iter()
                        .rev()
                        .take_while(|(_, block)| *block != Block::Function)
                        .find(|(_, block)| matches!(block, Block::Loop(_) | Block::Repeat))
                        .map(|(id, _)| *id);
                    gotos.push(Goto {
                        name: (*name).to_owned(),
                        start: token.start,
                        end: *end,
                        line: token.line,
                        innermost_loop,
                    });
                    index += 2;
                    continue;
                }
            }
            Token::Symbol("::") => {
                if let (
                    Some(Spanned {
                        token: Token::Name(name),
                        ..
                    }),
                    Some(Spanned {
                        token: Token::Symbol("::"),
                        end,
                        ..
                    }),
                ) = (tokens.get(index + 1), tokens.get(index + 2))
                {
                    let closes = tokens.get(index + 3).map(|t| t.token);
                    let ends_loop = match (stack.last(), closes) {
                        (Some((id, Block::Loop(false))), Some(Token::Keyword("end")))
                        | (Some((id, Block::Repeat)), Some(Token::Keyword("until"))) => Some(*id),
                        _ => None,
                    };
                    labels.push(Label {
                        name: (*name).to_owned(),
                        start: token.start,
                        end: *end,
                        line: token.line,
                        ends_loop,
                    });
                    index += 3;
                    continue;
                }
            }
            _ => {}
        }
        index += 1;
    }
    let mut edits: Vec<(usize, usize, &str)> = Vec::new();
    for goto in &gotos {
        let target = labels.iter().find(|label| {
            label.name == goto.name
                && label.ends_loop.is_some()
                && label.ends_loop == goto.innermost_loop
        });
        if target.is_none() {
            return Err(format!(
                "{file}:{}: Luau has no goto; only a goto to a label that ends the loop body (`goto continue` … `::continue::` before `end`) is converted, to `continue`. Use continue, break or a flag here.",
                goto.line
            ));
        }
        edits.push((goto.start, goto.end, "continue"));
    }
    for label in &labels {
        if label.ends_loop.is_none() {
            return Err(format!(
                "{file}:{}: Luau has no goto labels; ::{}:: must be the last statement of a loop body to become a continue.",
                label.line, label.name
            ));
        }
        edits.push((label.start, label.end, ""));
    }
    edits.sort_by_key(|edit| edit.0);
    let mut out = String::with_capacity(source.len() + edits.len() * 4);
    let mut cursor = 0;
    for (start, end, text) in edits {
        out.push_str(&source[cursor..start]);
        out.push_str(text);
        // Keep line numbers for error messages.
        out.extend(source[start..end].chars().filter(|&c| c == '\n'));
        if text.is_empty() {
            out.push(' ');
        }
        cursor = end;
    }
    out.push_str(&source[cursor..]);
    Ok(Cow::Owned(out))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tokens_skip_comments_and_strings() {
        let source = "local a = 'x -- y' --[[ goto b ]] .. [==[ ::c:: ]==] -- d = 1\nb.c = 2";
        let names: Vec<_> = tokens(source)
            .into_iter()
            .filter_map(|t| match t {
                Token::Name(n) => Some(n),
                _ => None,
            })
            .collect();
        assert_eq!(names, ["a", "b", "c"]);
    }

    #[test]
    fn continue_labels_become_continue_and_lines_are_kept() {
        let source = "for i = 1, 3 do\n  if i == 2 then goto continue end\n  print(i)\n  ::continue::\nend\nrepeat\n  if x then goto skip end\n::skip::\nuntil true\n";
        let out = luau_source("main.lua", source).unwrap();
        assert!(!out.contains("goto") && !out.contains("::"), "{out}");
        assert_eq!(out.matches("continue").count(), 2, "{out}");
        assert_eq!(out.lines().count(), source.lines().count());
    }

    #[test]
    fn other_gotos_are_reported_with_their_line() {
        let error = luau_source("a.lua", "local x\n::top::\nx = 1\ngoto top\n").unwrap_err();
        assert!(error.starts_with("a.lua:4:"), "{error}");
        // A continue in an inner loop must not jump the outer loop's label.
        let nested = "for i = 1, 2 do\n for j = 1, 2 do goto next end\n ::next::\nend";
        assert!(luau_source("b.lua", nested).is_err());
        // Plain code is borrowed untouched.
        assert!(matches!(
            luau_source("c.lua", "local a = 1"),
            Ok(Cow::Borrowed(_))
        ));
    }
}
