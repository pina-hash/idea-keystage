//! User shortcut import (ARCHITECTURE.md section 9, DECISIONS.md item 4).
//!
//! Two routes, both unvalidated until a real export from SolidWorks 2025 and
//! 2026 is supplied:
//!
//! * Primary: the Keyboard tab's Copy List (Tools > Customize > Keyboard,
//!   Category "All Commands", Copy List), pasted or saved as `.txt`. The help
//!   documents only the columns Category, Command and Shortcut(s) (for Print
//!   List) and that letters are shown in uppercase, so the parser detects the
//!   delimiter, the header row and the encoding instead of assuming them.
//! * Experimental: a `.sldreg` file from the SOLIDWORKS Settings Wizard. Its
//!   format is not documented, so the sniffer recognizes empty, binary and
//!   Registration Entries text (UTF-8 or UTF-16LE), tokenizes the text form,
//!   and then reports that the keyboard section could not be read rather than
//!   guessing at undocumented keys.
//!
//! Research notes and fixture specification: `.work/research/sldreg.md`.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::keys;
use crate::modifiers::ModKey;
use crate::profile::{keys_to_steps, CaptionIndex, Profile, USER_IMPORT_SOURCE};

/// A shortcut from the user's own SolidWorks.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(default)]
pub struct UserShortcut {
    /// Canonical key names, modifiers first in the order Ctrl, Shift, Alt, AltGr, Win.
    pub keys: Vec<String>,
    /// The shortcut as written in the import.
    pub doc_keys: String,
    /// The Command cell: the caption.
    pub command: String,
    /// The Category cell.
    pub category: String,
    /// Always `"user-import"`.
    pub source: String,
}

impl Default for UserShortcut {
    fn default() -> Self {
        UserShortcut {
            keys: Vec::new(),
            doc_keys: String::new(),
            command: String::new(),
            category: String::new(),
            source: USER_IMPORT_SOURCE.to_string(),
        }
    }
}

/// Which route an import took.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ImportFormat {
    CopyList,
    Sldreg,
    Unknown,
}

/// A row the parser could not turn into a shortcut, kept as written.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RawRow {
    /// 1-based line number in the decoded text.
    pub line: usize,
    pub text: String,
}

/// The parse result.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ImportResult {
    pub format: ImportFormat,
    pub entries: Vec<UserShortcut>,
    /// Rows kept raw (unknown keys, malformed rows); never rendered.
    pub unparsed: Vec<RawRow>,
    pub warnings: Vec<String>,
    /// Always false until a real export has validated the parser.
    pub validated: bool,
}

impl ImportResult {
    fn empty(format: ImportFormat) -> Self {
        ImportResult {
            format,
            entries: Vec::new(),
            unparsed: Vec::new(),
            warnings: Vec::new(),
            validated: false,
        }
    }
}

/// A caption that an import adds or changes, for the preview.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Change {
    pub keys: Vec<String>,
    pub doc_keys: String,
    /// The imported caption.
    pub command: String,
    /// The sourced caption it replaces (changed entries only).
    pub previous: Option<String>,
    pub category: String,
}

/// The `shortcuts_import_preview` result (section 12).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ImportPreview {
    pub format: ImportFormat,
    pub entries: Vec<UserShortcut>,
    pub added: Vec<Change>,
    pub changed: Vec<Change>,
    pub unchanged: usize,
    pub warnings: Vec<String>,
    pub validated: bool,
}

/// Compares imported entries with the sourced profile's captions.
pub fn preview(result: &ImportResult, sourced: &Profile) -> ImportPreview {
    let idx = CaptionIndex::build(sourced, &[]);
    let mut added = Vec::new();
    let mut changed = Vec::new();
    let mut unchanged = 0;
    for e in &result.entries {
        let change = |previous: Option<String>| Change {
            keys: e.keys.clone(),
            doc_keys: e.doc_keys.clone(),
            command: e.command.clone(),
            previous,
            category: e.category.clone(),
        };
        let existing = keys_to_steps(&e.keys).and_then(|s| idx.lookup(&s).cloned());
        match existing {
            None => added.push(change(None)),
            Some(c) if c.command.trim().eq_ignore_ascii_case(e.command.trim()) => unchanged += 1,
            Some(c) => changed.push(change(Some(c.command))),
        }
    }
    ImportPreview {
        format: result.format,
        entries: result.entries.clone(),
        added,
        changed,
        unchanged,
        warnings: result.warnings.clone(),
        validated: false,
    }
}

/// Imports pasted text: always the Copy List route, unless the text is
/// Registration Entries text, which is handed to the `.sldreg` sniffer.
pub fn import_text(text: &str) -> ImportResult {
    let stripped = text.strip_prefix('\u{feff}').unwrap_or(text);
    if is_registry_text(stripped) {
        return sldreg_result(SldregReport::from_text(stripped));
    }
    parse_copy_list(stripped)
}

/// Imports a file's bytes. `file_name` picks the route by extension
/// (`.sldreg`), otherwise the bytes are decoded as text and parsed as a
/// Copy List.
pub fn import_bytes(bytes: &[u8], file_name: Option<&str>) -> ImportResult {
    let is_sldreg = file_name
        .map(|n| n.trim().to_ascii_lowercase().ends_with(".sldreg"))
        .unwrap_or(false);
    if is_sldreg {
        return sldreg_result(sniff_sldreg(bytes));
    }
    match decode_text(bytes) {
        Ok(decoded) => {
            let mut r = import_text(&decoded.text);
            if let Some(w) = decoded.warning {
                r.warnings.insert(0, w);
            }
            r
        }
        Err(DecodeError::Empty) => {
            let mut r = ImportResult::empty(ImportFormat::Unknown);
            r.warnings.push("The file is empty.".into());
            r
        }
        Err(DecodeError::Binary) => {
            let mut r = ImportResult::empty(ImportFormat::Unknown);
            r.warnings.push(
                "The file is not text. Paste the Copy List from the Keyboard tab instead.".into(),
            );
            r
        }
    }
}

fn sldreg_result(report: Result<SldregReport, SldregError>) -> ImportResult {
    let mut r = ImportResult::empty(ImportFormat::Sldreg);
    match report {
        Ok(rep) => {
            r.warnings
                .push(SldregError::NoKnownKeyboardSection.to_string());
            r.warnings.push(format!(
                "The file holds {} registry key(s) and {} value(s); none is a documented keyboard section.",
                rep.file.keys.len(),
                rep.file.keys.iter().map(|k| k.values.len()).sum::<usize>()
            ));
        }
        Err(e) => r.warnings.push(e.to_string()),
    }
    r
}

/// Decoded text and a note about how it was decoded.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Decoded {
    pub text: String,
    pub encoding: TextEncoding,
    pub warning: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TextEncoding {
    Utf8,
    Utf8Bom,
    Utf16LeBom,
    Utf16BeBom,
    Utf16LeNoBom,
    Windows1252,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum DecodeError {
    #[error("the file is empty")]
    Empty,
    #[error("the file is not text")]
    Binary,
}

/// Decodes import bytes: UTF-8 with or without BOM, UTF-16LE or UTF-16BE
/// with BOM, UTF-16LE without BOM (detected from zero bytes), and finally
/// Windows-1252 (with a warning). Bytes that look binary are refused.
pub fn decode_text(bytes: &[u8]) -> Result<Decoded, DecodeError> {
    if bytes.is_empty() {
        return Err(DecodeError::Empty);
    }
    if let Some(rest) = bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]) {
        let text = String::from_utf8_lossy(rest).into_owned();
        return Ok(Decoded {
            text,
            encoding: TextEncoding::Utf8Bom,
            warning: None,
        });
    }
    if let Some(rest) = bytes.strip_prefix(&[0xFF, 0xFE]) {
        return Ok(Decoded {
            text: decode_utf16(rest, true),
            encoding: TextEncoding::Utf16LeBom,
            warning: None,
        });
    }
    if let Some(rest) = bytes.strip_prefix(&[0xFE, 0xFF]) {
        return Ok(Decoded {
            text: decode_utf16(rest, false),
            encoding: TextEncoding::Utf16BeBom,
            warning: None,
        });
    }
    if looks_utf16le(bytes) {
        let text = decode_utf16(bytes, true);
        if !looks_binary_text(&text) {
            return Ok(Decoded {
                text,
                encoding: TextEncoding::Utf16LeNoBom,
                warning: None,
            });
        }
    }
    if let Ok(s) = std::str::from_utf8(bytes) {
        if looks_binary_text(s) {
            return Err(DecodeError::Binary);
        }
        return Ok(Decoded {
            text: s.to_string(),
            encoding: TextEncoding::Utf8,
            warning: None,
        });
    }
    let text: String = bytes.iter().map(|b| windows_1252(*b)).collect();
    if looks_binary_text(&text) {
        return Err(DecodeError::Binary);
    }
    Ok(Decoded {
        text,
        encoding: TextEncoding::Windows1252,
        warning: Some("The file was not UTF-8 or UTF-16, so it was read as Windows-1252.".into()),
    })
}

fn decode_utf16(bytes: &[u8], little: bool) -> String {
    let units: Vec<u16> = bytes
        .chunks_exact(2)
        .map(|c| {
            if little {
                u16::from_le_bytes([c[0], c[1]])
            } else {
                u16::from_be_bytes([c[0], c[1]])
            }
        })
        .collect();
    String::from_utf16_lossy(&units)
}

fn looks_utf16le(bytes: &[u8]) -> bool {
    if bytes.len() < 4 || !bytes.len().is_multiple_of(2) {
        return false;
    }
    let pairs = bytes.len() / 2;
    let odd_zero = bytes.iter().skip(1).step_by(2).filter(|b| **b == 0).count();
    let even_zero = bytes.iter().step_by(2).filter(|b| **b == 0).count();
    odd_zero * 10 >= pairs * 6 && even_zero * 10 <= pairs
}

/// Control characters other than tab, line feed, carriage return and form
/// feed mark binary data.
fn looks_binary_text(s: &str) -> bool {
    let total = s.chars().count().max(1);
    let bad = s
        .chars()
        .filter(|c| {
            (c.is_control() && !matches!(c, '\t' | '\n' | '\r' | '\u{c}')) || *c == '\u{fffd}'
        })
        .count();
    bad * 20 > total || s.contains('\0')
}

fn windows_1252(b: u8) -> char {
    const HIGH: [char; 32] = [
        '\u{20AC}', '\u{FFFD}', '\u{201A}', '\u{0192}', '\u{201E}', '\u{2026}', '\u{2020}',
        '\u{2021}', '\u{02C6}', '\u{2030}', '\u{0160}', '\u{2039}', '\u{0152}', '\u{FFFD}',
        '\u{017D}', '\u{FFFD}', '\u{FFFD}', '\u{2018}', '\u{2019}', '\u{201C}', '\u{201D}',
        '\u{2022}', '\u{2013}', '\u{2014}', '\u{02DC}', '\u{2122}', '\u{0161}', '\u{203A}',
        '\u{0153}', '\u{FFFD}', '\u{017E}', '\u{0178}',
    ];
    match b {
        0x80..=0x9F => HIGH[(b - 0x80) as usize],
        _ => b as char,
    }
}

fn split_lines(text: &str) -> Vec<&str> {
    text.split('\n')
        .map(|l| l.strip_suffix('\r').unwrap_or(l))
        .flat_map(|l| l.split('\r'))
        .collect()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Delim {
    Tab,
    Semicolon,
    Comma,
}

impl Delim {
    fn ch(self) -> char {
        match self {
            Delim::Tab => '\t',
            Delim::Semicolon => ';',
            Delim::Comma => ',',
        }
    }
}

fn detect_delimiter(lines: &[&str]) -> Delim {
    let rows: Vec<&&str> = lines
        .iter()
        .filter(|l| !l.trim().is_empty())
        .take(50)
        .collect();
    let rows_with = |c: char| rows.iter().filter(|l| l.matches(c).count() >= 2).count();
    let tab = rows.iter().filter(|l| l.contains('\t')).count();
    if tab > 0 {
        return Delim::Tab;
    }
    let semi = rows_with(';');
    let comma = rows_with(',');
    if semi > 0 && semi * 2 >= rows.len() {
        Delim::Semicolon
    } else if comma > 0 {
        Delim::Comma
    } else if semi > 0 {
        Delim::Semicolon
    } else {
        Delim::Tab
    }
}

/// Splits one line into cells, honoring double quotes (`""` is a quote).
fn split_cells(line: &str, d: Delim) -> Vec<String> {
    let mut cells = Vec::new();
    let mut cur = String::new();
    let mut in_quotes = false;
    let mut chars = line.chars().peekable();
    let mut at_cell_start = true;
    while let Some(c) = chars.next() {
        if in_quotes {
            if c == '"' {
                if chars.peek() == Some(&'"') {
                    cur.push('"');
                    chars.next();
                } else {
                    in_quotes = false;
                }
            } else {
                cur.push(c);
            }
            continue;
        }
        if c == '"' && at_cell_start && cur.trim().is_empty() {
            cur.clear();
            in_quotes = true;
            at_cell_start = false;
            continue;
        }
        if c == d.ch() {
            cells.push(std::mem::take(&mut cur));
            at_cell_start = true;
            continue;
        }
        at_cell_start = false;
        cur.push(c);
    }
    cells.push(cur);
    cells
}

#[derive(Clone, Copy, Debug)]
struct Columns {
    category: usize,
    command: usize,
    shortcut: usize,
}

fn header_columns(cells: &[String]) -> Option<Columns> {
    let norm: Vec<String> = cells
        .iter()
        .map(|c| c.trim().to_ascii_lowercase())
        .collect();
    let find = |pred: &dyn Fn(&str) -> bool| norm.iter().position(|c| pred(c));
    let category = find(&|c| c == "category")?;
    let command = find(&|c| c == "command" || c == "commands")?;
    let shortcut = find(&|c| c.starts_with("shortcut"))?;
    Some(Columns {
        category,
        command,
        shortcut,
    })
}

/// Parses Copy List text.
pub fn parse_copy_list(text: &str) -> ImportResult {
    let mut r = ImportResult::empty(ImportFormat::CopyList);
    let lines = split_lines(text);
    let delim = detect_delimiter(&lines);
    let mut columns = Columns {
        category: 0,
        command: 1,
        shortcut: 2,
    };
    let mut seen_header = false;
    let mut seen_rows = 0usize;
    let mut lowercase_rows = Vec::new();
    // Normalized binding -> commands that use it (for the duplicate warning).
    let mut bindings: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for (i, raw_line) in lines.iter().enumerate() {
        let line_no = i + 1;
        if raw_line.trim().is_empty() {
            continue;
        }
        let mut cells = split_cells(raw_line, delim);
        if !seen_header && seen_rows == 0 {
            if let Some(cols) = header_columns(&cells) {
                columns = cols;
                seen_header = true;
                continue;
            }
        }
        seen_rows += 1;
        let needed = columns.category.max(columns.command).max(columns.shortcut) + 1;
        if delim == Delim::Comma && cells.len() > needed && columns.shortcut + 1 == needed {
            // Unquoted commas inside the last cell (several shortcuts).
            let tail = cells.split_off(columns.shortcut).join(",");
            cells.push(tail);
        }
        if cells.len() < needed {
            r.warnings.push(format!(
                "Line {line_no}: expected {needed} columns, found {}; skipped.",
                cells.len()
            ));
            r.unparsed.push(RawRow {
                line: line_no,
                text: raw_line.to_string(),
            });
            continue;
        }
        let category = cells[columns.category].trim().to_string();
        let command = cells[columns.command].trim().to_string();
        let shortcut_cell = cells[columns.shortcut].trim().to_string();
        if shortcut_cell.is_empty() {
            continue;
        }
        if command.is_empty() {
            r.warnings.push(format!(
                "Line {line_no}: the Command cell is empty; skipped."
            ));
            r.unparsed.push(RawRow {
                line: line_no,
                text: raw_line.to_string(),
            });
            continue;
        }
        for piece in split_shortcuts(&shortcut_cell) {
            match parse_shortcut(&piece) {
                Ok(parsed) => {
                    if parsed.had_lowercase {
                        lowercase_rows.push(line_no);
                    }
                    let binding = parsed.keys.join("+");
                    let users = bindings.entry(binding).or_default();
                    if !users.iter().any(|c| c == &command) {
                        users.push(command.clone());
                    }
                    let entry = UserShortcut {
                        keys: parsed.keys,
                        doc_keys: piece.clone(),
                        command: command.clone(),
                        category: category.clone(),
                        source: USER_IMPORT_SOURCE.to_string(),
                    };
                    if !r.entries.contains(&entry) {
                        r.entries.push(entry);
                    }
                }
                Err(bad) => {
                    r.warnings.push(format!("Line {line_no}: \"{piece}\" has an unknown key \"{bad}\"; kept as written, not shown."));
                    r.unparsed.push(RawRow {
                        line: line_no,
                        text: raw_line.to_string(),
                    });
                }
            }
        }
    }
    if !lowercase_rows.is_empty() {
        let list = lowercase_rows
            .iter()
            .map(|n| n.to_string())
            .collect::<Vec<_>>()
            .join(", ");
        r.warnings.push(format!(
            "Lowercase letters were read as uppercase (line {list})."
        ));
    }
    for (binding, commands) in &bindings {
        if commands.len() > 1 {
            r.warnings.push(format!(
                "{} is assigned to more than one command ({}); the first one is used.",
                binding.replace('+', " + "),
                commands.join("; ")
            ));
        }
    }
    if r.entries.is_empty() && r.unparsed.is_empty() {
        r.warnings.push(
            "No shortcuts were found. Copy the list with Category set to All Commands.".into(),
        );
    }
    r
}

/// Splits a Shortcut(s) cell into single shortcuts. A comma or semicolon
/// separates shortcuts unless it is the key itself (`Ctrl+,`).
fn split_shortcuts(cell: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    for c in cell.chars() {
        if (c == ',' || c == ';') && !cur.trim().is_empty() && !cur.trim_end().ends_with('+') {
            out.push(cur.trim().to_string());
            cur.clear();
        } else {
            cur.push(c);
        }
    }
    if !cur.trim().is_empty() {
        out.push(cur.trim().to_string());
    }
    out
}

struct ParsedShortcut {
    keys: Vec<String>,
    had_lowercase: bool,
}

/// Parses one shortcut (`Ctrl + Shift + B`, `ctrl+b`, `Enter`, `Ctrl+,`).
/// On failure returns the token that was not understood.
fn parse_shortcut(s: &str) -> Result<ParsedShortcut, String> {
    let mut tokens: Vec<String> = Vec::new();
    let mut cur = String::new();
    let chars: Vec<char> = s.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if c == '+' {
            if cur.trim().is_empty() {
                // A '+' with nothing before it is the key itself; no
                // canonical key is named '+'.
                return Err("+".into());
            }
            tokens.push(cur.trim().to_string());
            cur.clear();
        } else {
            cur.push(c);
        }
        i += 1;
    }
    if cur.trim().is_empty() {
        return Err(if tokens.is_empty() {
            s.to_string()
        } else {
            "+".into()
        });
    }
    tokens.push(cur.trim().to_string());
    let mut mods = Vec::new();
    let mut key: Option<String> = None;
    let mut had_lowercase = false;
    for t in tokens {
        let compact: String = t.chars().filter(|c| !c.is_whitespace()).collect();
        if let Some(m) = ModKey::from_alias(&compact.to_ascii_lowercase()) {
            if !mods.contains(&m) {
                mods.push(m);
            }
            continue;
        }
        if key.is_some() {
            return Err(t);
        }
        if t.chars().count() == 1 && t.chars().all(|c| c.is_ascii_lowercase()) {
            had_lowercase = true;
        }
        key = Some(keys::parse_key_name(&t).ok_or(t)?);
    }
    let mut out: Vec<String> = ModKey::ORDER
        .iter()
        .filter(|m| mods.contains(m))
        .map(|m| m.name().to_string())
        .collect();
    if let Some(k) = key {
        out.push(k);
    }
    Ok(ParsedShortcut {
        keys: out,
        had_lowercase,
    })
}

/// Why a `.sldreg` file could not be used.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum SldregError {
    #[error("The file is empty.")]
    Empty,
    #[error("This file is a SOLIDWORKS settings file, but its format is not one Keystage can read. Use Copy List from the Keyboard tab instead.")]
    UnrecognizedFormat,
    #[error("The settings file is damaged or cut short ({0}). Use Copy List from the Keyboard tab instead.")]
    Malformed(String),
    #[error("Could not read the keyboard section; use Copy List from the Keyboard tab instead.")]
    NoKnownKeyboardSection,
}

/// Data of one registry value.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RegData {
    String(String),
    Dword(u32),
    /// `hex:` or `hex(n):` with the type number (`None` for plain `hex:`).
    Hex(Option<u32>, Vec<u8>),
    /// `-`: delete the value.
    Delete,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RegValue {
    /// Empty for the default value (`@`).
    pub name: String,
    pub data: RegData,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RegKey {
    pub path: String,
    pub delete: bool,
    pub values: Vec<RegValue>,
}

/// A tokenized Registration Entries file.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RegFile {
    /// `Windows Registry Editor Version 5.00` or `REGEDIT4`.
    pub header: String,
    pub keys: Vec<RegKey>,
}

/// What the sniffer found in a `.sldreg` file that is Registration Entries text.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SldregReport {
    pub encoding: TextEncoding,
    pub file: RegFile,
}

impl SldregReport {
    fn from_text(text: &str) -> Result<SldregReport, SldregError> {
        let file = parse_reg_text(text)?;
        Ok(SldregReport {
            encoding: TextEncoding::Utf8,
            file,
        })
    }
}

fn is_registry_text(text: &str) -> bool {
    let first = text
        .trim_start_matches('\u{feff}')
        .lines()
        .find(|l| !l.trim().is_empty())
        .unwrap_or("");
    let t = first.trim();
    t.starts_with("Windows Registry Editor Version") || t == "REGEDIT4"
}

/// Sniffs a `.sldreg` file. Success means the bytes are Registration
/// Entries text and tokenized cleanly; the caller still has no documented
/// keyboard section to read (see [`SldregError::NoKnownKeyboardSection`]).
pub fn sniff_sldreg(bytes: &[u8]) -> Result<SldregReport, SldregError> {
    let decoded = match decode_text(bytes) {
        Ok(d) => d,
        Err(DecodeError::Empty) => return Err(SldregError::Empty),
        Err(DecodeError::Binary) => return Err(SldregError::UnrecognizedFormat),
    };
    if decoded.text.trim().is_empty() {
        return Err(SldregError::Empty);
    }
    if !is_registry_text(&decoded.text) {
        return Err(SldregError::UnrecognizedFormat);
    }
    let file = parse_reg_text(&decoded.text)?;
    Ok(SldregReport {
        encoding: decoded.encoding,
        file,
    })
}

/// Tokenizes Registration Entries text: a header line, `[key]` lines,
/// `"name"=data` and `@=data` lines, `\` line continuations and `;` comments.
pub fn parse_reg_text(text: &str) -> Result<RegFile, SldregError> {
    let text = text.trim_start_matches('\u{feff}');
    let raw_lines = split_lines(text);
    // Join continuation lines.
    let mut lines: Vec<(usize, String)> = Vec::new();
    let mut pending: Option<(usize, String)> = None;
    for (i, l) in raw_lines.iter().enumerate() {
        let piece = l.trim();
        let (start, mut acc) = pending.take().unwrap_or((i + 1, String::new()));
        acc.push_str(piece);
        if acc.ends_with('\\') && !acc.starts_with('[') {
            acc.pop();
            pending = Some((start, acc));
        } else {
            lines.push((start, acc));
        }
    }
    if let Some((n, _)) = pending {
        return Err(SldregError::Malformed(format!(
            "line {n} continues past the end of the file"
        )));
    }
    let mut iter = lines
        .into_iter()
        .filter(|(_, l)| !l.is_empty() && !l.starts_with(';'));
    let header = match iter.next() {
        Some((_, h)) if h.starts_with("Windows Registry Editor Version") || h == "REGEDIT4" => h,
        _ => return Err(SldregError::UnrecognizedFormat),
    };
    let mut keys_out: Vec<RegKey> = Vec::new();
    for (n, line) in iter {
        if let Some(rest) = line.strip_prefix('[') {
            let Some(inner) = rest.strip_suffix(']') else {
                return Err(SldregError::Malformed(format!(
                    "line {n} has an unterminated key"
                )));
            };
            let (delete, path) = match inner.strip_prefix('-') {
                Some(p) => (true, p),
                None => (false, inner),
            };
            if path.trim().is_empty() || path.contains('[') || path.contains(']') {
                return Err(SldregError::Malformed(format!(
                    "line {n} has an invalid key"
                )));
            }
            keys_out.push(RegKey {
                path: path.to_string(),
                delete,
                values: Vec::new(),
            });
            continue;
        }
        let Some(current) = keys_out.last_mut() else {
            return Err(SldregError::Malformed(format!(
                "line {n} has a value outside any key"
            )));
        };
        let (name, data_text) = parse_value_name(&line)
            .ok_or_else(|| SldregError::Malformed(format!("line {n} is not a value")))?;
        let data = parse_reg_data(data_text)
            .ok_or_else(|| SldregError::Malformed(format!("line {n} has unreadable data")))?;
        current.values.push(RegValue { name, data });
    }
    Ok(RegFile {
        header,
        keys: keys_out,
    })
}

fn parse_value_name(line: &str) -> Option<(String, &str)> {
    if let Some(rest) = line.strip_prefix('@') {
        return Some((String::new(), rest.trim_start().strip_prefix('=')?));
    }
    let rest = line.strip_prefix('"')?;
    let mut name = String::new();
    let mut chars = rest.char_indices();
    while let Some((i, c)) = chars.next() {
        match c {
            '\\' => {
                let (_, n) = chars.next()?;
                name.push(n);
            }
            '"' => {
                let after = rest[i + 1..].trim_start().strip_prefix('=')?;
                return Some((name, after));
            }
            _ => name.push(c),
        }
    }
    None
}

fn parse_reg_data(s: &str) -> Option<RegData> {
    let s = s.trim();
    if s == "-" {
        return Some(RegData::Delete);
    }
    if let Some(rest) = s.strip_prefix('"') {
        let mut out = String::new();
        let mut chars = rest.chars();
        while let Some(c) = chars.next() {
            match c {
                '\\' => out.push(chars.next()?),
                '"' => {
                    return if chars.as_str().trim().is_empty() {
                        Some(RegData::String(out))
                    } else {
                        None
                    };
                }
                _ => out.push(c),
            }
        }
        return None;
    }
    if let Some(hex) = s.strip_prefix("dword:") {
        if hex.len() != 8 {
            return None;
        }
        return u32::from_str_radix(hex, 16).ok().map(RegData::Dword);
    }
    let (kind, bytes_text) = if let Some(rest) = s.strip_prefix("hex(") {
        let close = rest.find(')')?;
        let k = u32::from_str_radix(&rest[..close], 16).ok()?;
        (Some(k), rest[close + 1..].strip_prefix(':')?)
    } else {
        (None, s.strip_prefix("hex:")?)
    };
    let mut bytes = Vec::new();
    for part in bytes_text.split(',') {
        let p = part.trim();
        if p.is_empty() {
            continue;
        }
        if p.len() > 2 {
            return None;
        }
        bytes.push(u8::from_str_radix(p, 16).ok()?);
    }
    Some(RegData::Hex(kind, bytes))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shortcut_pieces() {
        assert_eq!(split_shortcuts("Ctrl+B, Ctrl+R"), vec!["Ctrl+B", "Ctrl+R"]);
        assert_eq!(split_shortcuts("Ctrl+B,Ctrl+R"), vec!["Ctrl+B", "Ctrl+R"]);
        assert_eq!(split_shortcuts("Ctrl+,"), vec!["Ctrl+,"]);
        assert_eq!(
            split_shortcuts("Ctrl + , ; Ctrl+;"),
            vec!["Ctrl + ,", "Ctrl+;"]
        );
        assert_eq!(split_shortcuts("  "), Vec::<String>::new());
    }

    #[test]
    fn single_shortcuts() {
        let k = |s: &str| parse_shortcut(s).map(|p| p.keys);
        assert_eq!(k("Ctrl+B").unwrap(), vec!["Ctrl", "B"]);
        assert_eq!(k("Ctrl + R").unwrap(), vec!["Ctrl", "R"]);
        assert_eq!(k("Shift+Ctrl+B").unwrap(), vec!["Ctrl", "Shift", "B"]);
        assert_eq!(k("ctrl+b").unwrap(), vec!["Ctrl", "B"]);
        assert!(parse_shortcut("ctrl+b").unwrap().had_lowercase);
        assert_eq!(k("Enter").unwrap(), vec!["Enter"]);
        assert_eq!(k("Ctrl+Tab").unwrap(), vec!["Ctrl", "Tab"]);
        assert_eq!(k("Ctrl+,").unwrap(), vec!["Ctrl", ","]);
        assert_eq!(k("Spacebar").unwrap(), vec!["Space"]);
        assert_eq!(k("Ctrl+Shift").unwrap(), vec!["Ctrl", "Shift"]);
        assert_eq!(k("Ctrl+???"), Err("???".to_string()));
        assert_eq!(k("Ctrl++"), Err("+".to_string()));
        assert_eq!(k("Ctrl+A+B"), Err("B".to_string()));
    }

    #[test]
    fn delimiters_and_quotes() {
        assert_eq!(split_cells("a\tb\tc", Delim::Tab), vec!["a", "b", "c"]);
        assert_eq!(split_cells("a;b;c", Delim::Semicolon), vec!["a", "b", "c"]);
        assert_eq!(
            split_cells("a,\"x, y\",c", Delim::Comma),
            vec!["a", "x, y", "c"]
        );
        assert_eq!(
            split_cells("a,\"say \"\"hi\"\"\",c", Delim::Comma),
            vec!["a", "say \"hi\"", "c"]
        );
        assert_eq!(detect_delimiter(&["a;b;c", "d;e;f"]), Delim::Semicolon);
        assert_eq!(detect_delimiter(&["a,b,c"]), Delim::Comma);
        assert_eq!(detect_delimiter(&["a\tb\tc"]), Delim::Tab);
    }

    #[test]
    fn comma_rows_with_unquoted_shortcut_lists() {
        let r = parse_copy_list(
            "Category,Command,Shortcut(s)\nFixtureCategoryA,Fixture Command One,Ctrl+B, Ctrl+R\n",
        );
        assert_eq!(r.entries.len(), 2, "{r:?}");
        assert_eq!(r.entries[1].keys, vec!["Ctrl", "R"]);
    }

    #[test]
    fn header_in_another_order() {
        let r = parse_copy_list(
            "Shortcut(s)\tCommand\tCategory\nCtrl+B\tFixture Command One\tFixtureCategoryA\n",
        );
        assert_eq!(r.entries.len(), 1);
        assert_eq!(r.entries[0].command, "Fixture Command One");
        assert_eq!(r.entries[0].category, "FixtureCategoryA");
    }

    #[test]
    fn decoding() {
        assert_eq!(decode_text(b""), Err(DecodeError::Empty));
        let d = decode_text(b"\xEF\xBB\xBFabc").unwrap();
        assert_eq!(
            (d.text.as_str(), d.encoding),
            ("abc", TextEncoding::Utf8Bom)
        );
        let d = decode_text(&[0xFF, 0xFE, b'a', 0, b'b', 0]).unwrap();
        assert_eq!(
            (d.text.as_str(), d.encoding),
            ("ab", TextEncoding::Utf16LeBom)
        );
        let d = decode_text(&[0xFE, 0xFF, 0, b'a', 0, b'b']).unwrap();
        assert_eq!(
            (d.text.as_str(), d.encoding),
            ("ab", TextEncoding::Utf16BeBom)
        );
        let d = decode_text(&[b'a', 0, b'\t', 0, b'b', 0, b'c', 0]).unwrap();
        assert_eq!(
            (d.text.as_str(), d.encoding),
            ("a\tbc", TextEncoding::Utf16LeNoBom)
        );
        let d = decode_text(b"Caf\xE9\tx").unwrap();
        assert_eq!(d.text, "Caf\u{e9}\tx");
        assert_eq!(d.encoding, TextEncoding::Windows1252);
        assert!(d.warning.is_some());
        let binary: Vec<u8> = (0u8..64).collect();
        assert_eq!(decode_text(&binary), Err(DecodeError::Binary));
    }

    #[test]
    fn registry_tokenizer() {
        let text = "Windows Registry Editor Version 5.00\r\n\r\n; comment\r\n[HKEY_CURRENT_USER\\FIXTURE]\r\n\"A\"=dword:00000400\r\n\"B\"=\"x \\\"q\\\"\"\r\n@=hex:01,02,\\\r\n  03\r\n\"C\"=hex(7):41,00\r\n\"D\"=-\r\n[-HKEY_CURRENT_USER\\GONE]\r\n";
        let f = parse_reg_text(text).unwrap();
        assert_eq!(f.header, "Windows Registry Editor Version 5.00");
        assert_eq!(f.keys.len(), 2);
        let v = &f.keys[0].values;
        assert_eq!(
            v[0],
            RegValue {
                name: "A".into(),
                data: RegData::Dword(0x400)
            }
        );
        assert_eq!(
            v[1],
            RegValue {
                name: "B".into(),
                data: RegData::String("x \"q\"".into())
            }
        );
        assert_eq!(
            v[2],
            RegValue {
                name: String::new(),
                data: RegData::Hex(None, vec![1, 2, 3])
            }
        );
        assert_eq!(
            v[3],
            RegValue {
                name: "C".into(),
                data: RegData::Hex(Some(7), vec![0x41, 0])
            }
        );
        assert_eq!(
            v[4],
            RegValue {
                name: "D".into(),
                data: RegData::Delete
            }
        );
        assert!(f.keys[1].delete);
        assert!(matches!(
            parse_reg_text("REGEDIT4\n\"A\"=dword:1"),
            Err(SldregError::Malformed(_))
        ));
        assert!(matches!(
            parse_reg_text("REGEDIT4\n[K]\n\"A\"=dword:1"),
            Err(SldregError::Malformed(_))
        ));
        assert!(matches!(
            parse_reg_text("REGEDIT4\n[K]\n\"A\"=\"open"),
            Err(SldregError::Malformed(_))
        ));
        assert!(matches!(
            parse_reg_text("REGEDIT4\n[K]\n\"A\"=hex:1,\\"),
            Err(SldregError::Malformed(_))
        ));
        assert!(matches!(
            parse_reg_text("Hello\n[K]"),
            Err(SldregError::UnrecognizedFormat)
        ));
    }

    #[test]
    fn preview_against_sourced_profile() {
        use crate::profile::Shortcut;
        let sourced = Profile {
            id: "fixture".into(),
            shortcuts: vec![
                Shortcut {
                    keys: vec!["Ctrl".into(), "B".into()],
                    command: "Fixture Command One".into(),
                    ..Shortcut::default()
                },
                Shortcut {
                    keys: vec!["Ctrl".into(), "R".into()],
                    command: "Fixture Sourced Two".into(),
                    ..Shortcut::default()
                },
            ],
            ..Profile::default()
        };
        let r = parse_copy_list("FixtureCategoryA\tFixture Command One\tCtrl+B\nFixtureCategoryA\tFixture Command Two\tCtrl+R\nFixtureCategoryA\tFixture Command Six\tCtrl+Tab\n");
        let p = preview(&r, &sourced);
        assert_eq!(p.unchanged, 1);
        assert_eq!(p.changed.len(), 1);
        assert_eq!(
            p.changed[0].previous.as_deref(),
            Some("Fixture Sourced Two")
        );
        assert_eq!(p.added.len(), 1);
        assert_eq!(p.added[0].keys, vec!["Ctrl", "Tab"]);
        assert!(!p.validated);
        let json = serde_json::to_value(&p).unwrap();
        assert_eq!(json["format"], "copy_list");
    }

    #[test]
    fn pasted_registry_text_goes_to_the_sniffer() {
        let r = import_text("Windows Registry Editor Version 5.00\n\n[HKEY_CURRENT_USER\\FIXTURE]\n\"A\"=dword:00000001\n");
        assert_eq!(r.format, ImportFormat::Sldreg);
        assert!(r.entries.is_empty());
        assert!(r.warnings[0].contains("Could not read the keyboard section"));
    }

    #[test]
    fn empty_and_binary_text_files() {
        let r = import_bytes(b"", Some("list.txt"));
        assert_eq!(r.format, ImportFormat::Unknown);
        assert!(!r.warnings.is_empty());
        let r = import_bytes(&[0u8, 1, 2, 3, 4, 5, 6, 7], Some("list.txt"));
        assert_eq!(r.format, ImportFormat::Unknown);
        let r = import_text("");
        assert!(r.entries.is_empty());
        assert!(r.warnings.iter().any(|w| w.contains("No shortcuts")));
    }
}
