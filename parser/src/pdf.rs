use lopdf::{Dictionary, Document, Error, Object, ObjectId};
use rayon::prelude::*;
use std::{
    collections::{BTreeSet, HashMap},
    sync::Arc,
};

type M = [f32; 6];
const I: M = [1., 0., 0., 1., 0., 0.];

/// Fonts are immutable after loading, so pages share them through an `Arc`.
pub(crate) type FontCache = HashMap<ObjectId, Arc<Font>>;

/// A page's font resources, keyed by the resource name (`F1`, `F2`, ...).
type PageFonts = HashMap<Vec<u8>, Arc<Font>>;

fn resolve<'a>(doc: &'a Document, o: &'a Object) -> &'a Object {
    doc.dereference(o).map(|(_, o)| o).unwrap_or(o)
}

// PDF decoding: the statement's main font is Type0 / Identity-H.
// Strings are 2-byte glyph IDs. The current font (Tf) and map codes are tracked
// to Unicode through the font's /ToUnicode CMap.
/// One glyph's replacement text. Single `char` mappings are by far the
/// common case; ligatures and emoji need more than one UTF-16 unit.
#[derive(Debug, Clone)]
enum Glyph {
    Char(char),
    Text(Box<str>),
}

impl Glyph {
    fn push_to(&self, out: &mut String) {
        match self {
            Self::Char(c) => out.push(*c),
            Self::Text(s) => out.push_str(s),
        }
    }
}

/// Code -> glyph, from /ToUnicode.
///
/// Two-byte fonts map dense code ranges, so a 65,536-entry table avoids a
/// hash lookup per glyph. Sparse and one-byte fonts keep the hash map.
#[derive(Debug, Default)]
enum GlyphMap {
    #[default]
    Empty,
    Sparse(HashMap<u32, Glyph>),
    Dense {
        chars: Box<[Option<char>]>,
        /// Multi-character mappings, which do not fit in `chars`.
        wide: HashMap<u32, Glyph>,
    },
}

impl GlyphMap {
    /// Smallest map worth a 256 KiB dense table.
    const DENSE_MIN: usize = 4096;
    const DENSE_LEN: usize = 1 << 16;

    fn new(map: HashMap<u32, Glyph>, two_byte: bool) -> Self {
        if map.is_empty() {
            return Self::Empty;
        }
        if !two_byte || map.len() < Self::DENSE_MIN {
            return Self::Sparse(map);
        }

        let mut chars = vec![None; Self::DENSE_LEN].into_boxed_slice();
        let mut wide = HashMap::new();
        for (code, glyph) in map {
            match glyph {
                Glyph::Char(c) if (code as usize) < Self::DENSE_LEN => {
                    chars[code as usize] = Some(c);
                }
                glyph => {
                    wide.insert(code, glyph);
                }
            }
        }
        Self::Dense { chars, wide }
    }

    fn is_empty(&self) -> bool {
        matches!(self, Self::Empty)
    }

    /// Appends the glyph for `code`, returning `false` when the code is unmapped.
    fn push(&self, code: u32, out: &mut String) -> bool {
        let glyph = match self {
            Self::Empty => return false,
            Self::Sparse(map) => map.get(&code),
            Self::Dense { chars, wide } => {
                if let Some(c) = chars.get(code as usize).copied().flatten() {
                    out.push(c);
                    return true;
                }
                wide.get(&code)
            }
        };
        match glyph {
            Some(glyph) => {
                glyph.push_to(out);
                true
            }
            None => false,
        }
    }
}

#[derive(Default)]
pub(crate) struct Font {
    /// Type0 (composite) font: strings are 2-byte codes.
    two_byte: bool,
    glyphs: GlyphMap,
}

impl Font {
    /// Decodes `bytes` and appends the text to `out`, so callers can reuse one
    /// buffer instead of allocating a `String` per show operator.
    fn decode_into(&self, bytes: &[u8], out: &mut String) {
        if self.two_byte {
            for pair in bytes.as_chunks::<2>().0 {
                let code = u16::from_be_bytes([pair[0], pair[1]]) as u32;
                if !self.glyphs.push(code, out) && self.glyphs.is_empty() {
                    // No ToUnicode: heuristic for TrueType subsets where glyph = char - 29.
                    out.push(
                        char::from_u32(code + 29)
                            .filter(|c| c.is_ascii_graphic() || *c == ' ')
                            .unwrap_or('?'),
                    );
                }
            }
        } else {
            for &b in bytes {
                if !self.glyphs.push(b as u32, out) {
                    out.push(b as char); // Latin-1 ~ WinAnsi for ASCII text
                }
            }
        }
    }
}

fn load_font(doc: &Document, f: &Dictionary) -> Font {
    let two_byte = f
        .get(b"Subtype")
        .ok()
        .and_then(|o| resolve(doc, o).as_name().ok())
        .map(|n| n == b"Type0")
        .unwrap_or(false);

    let mut map = HashMap::new();
    if let Ok(tu) = f.get(b"ToUnicode")
        && let Ok(s) = resolve(doc, tu).as_stream()
    {
        let data = s
            .decompressed_content()
            .unwrap_or_else(|_| s.content.clone());
        map = parse_to_unicode(&data);
    }
    Font {
        two_byte,
        glyphs: GlyphMap::new(map, two_byte),
    }
}

fn load_font_by_id(doc: &Document, id: ObjectId) -> Font {
    doc.get_dictionary(id)
        .map(|f| load_font(doc, f))
        .unwrap_or_default()
}

/// Loads every font referenced by `page_ids` exactly once.
///
/// Pages of one statement normally share their font objects, so this removes
/// the repeated `/ToUnicode` decompression and parsing per page. The caller
/// passes the page ids because walking the page tree is not free.
pub(crate) fn load_fonts(doc: &Document, page_ids: &[ObjectId]) -> FontCache {
    let mut ids = BTreeSet::new();
    for &page_id in page_ids {
        let Some(fonts) = page_font_dict(doc, page_id) else {
            continue;
        };
        for v in fonts.iter().map(|(_, v)| v) {
            if let Ok(id) = v.as_reference() {
                ids.insert(id);
            }
        }
    }

    ids.into_par_iter()
        .map(|id| (id, Arc::new(load_font_by_id(doc, id))))
        .collect()
}

/// Minimal ToUnicode CMap parser: handles bfchar and bfrange (both forms).
fn parse_to_unicode(data: &[u8]) -> HashMap<u32, Glyph> {
    let s = String::from_utf8_lossy(data);
    let mut map = HashMap::new();
    let mut rest: &str = &s;

    while let Some(i) = rest.find("beginbf") {
        let after = &rest[i + 7..];
        let is_range = after.starts_with("range");
        let end_kw = if is_range { "endbfrange" } else { "endbfchar" };
        let Some(e) = after.find(end_kw) else { break };
        let toks = tokens(&after[..e]);
        let mut k = 0;

        if !is_range {
            while k + 1 < toks.len() {
                if let (Tok::Hex(a), Tok::Hex(b)) = (&toks[k], &toks[k + 1]) {
                    map.insert(to_u32(a), to_glyph(b));
                    k += 2;
                } else {
                    k += 1;
                }
            }
        } else {
            while k + 2 < toks.len() {
                let (Tok::Hex(lo), Tok::Hex(hi)) = (&toks[k], &toks[k + 1]) else {
                    k += 1;
                    continue;
                };
                let (lo, hi) = (to_u32(lo), to_u32(hi).min(0xFFFF));
                match &toks[k + 2] {
                    Tok::Hex(d) => {
                        let units = utf16_units(d);
                        for c in lo..=hi {
                            map.insert(c, range_glyph(&units, (c - lo) as u16));
                        }
                        k += 3;
                    }
                    Tok::Open => {
                        k += 3;
                        let mut c = lo;
                        while k < toks.len() {
                            match &toks[k] {
                                Tok::Hex(d) => {
                                    map.insert(c, to_glyph(d));
                                    c += 1;
                                    k += 1;
                                }
                                Tok::Close => {
                                    k += 1;
                                    break;
                                }
                                _ => k += 1,
                            }
                        }
                    }
                    _ => k += 1,
                }
            }
        }
        rest = &after[e + end_kw.len()..];
    }
    map
}

fn to_glyph(b: &[u8]) -> Glyph {
    match b {
        [unit] => glyph_from_unit(*unit as u16),
        [hi, lo] => glyph_from_unit(u16::from_be_bytes([*hi, *lo])),
        _ => glyph_from_units(&utf16_units(b)),
    }
}

/// The glyph for a `bfrange` entry, whose last UTF-16 unit advances by `offset`.
fn range_glyph(units: &[u16], offset: u16) -> Glyph {
    match units {
        [] => Glyph::Text("".into()),
        [unit] => glyph_from_unit(unit.wrapping_add(offset)),
        _ => {
            let mut units = units.to_vec();
            if let Some(last) = units.last_mut() {
                *last = last.wrapping_add(offset);
            }
            glyph_from_units(&units)
        }
    }
}

fn glyph_from_units(units: &[u16]) -> Glyph {
    match units {
        [unit] => glyph_from_unit(*unit),
        _ => Glyph::Text(String::from_utf16_lossy(units).into_boxed_str()),
    }
}

fn glyph_from_unit(unit: u16) -> Glyph {
    match char::from_u32(unit as u32) {
        Some(c) => Glyph::Char(c),
        // Lone surrogate: the lossy decoder emits U+FFFD.
        None => Glyph::Char('\u{FFFD}'),
    }
}

fn to_u32(b: &[u8]) -> u32 {
    b.iter().fold(0u32, |a, &x| (a << 8) | x as u32)
}

fn utf16_units(b: &[u8]) -> Vec<u16> {
    if b.len() == 1 {
        vec![b[0] as u16]
    } else {
        b.chunks(2)
            .map(|c| ((c[0] as u16) << 8) | *c.get(1).unwrap_or(&0) as u16)
            .collect()
    }
}

enum Tok {
    Hex(Vec<u8>),
    Open,
    Close,
}

fn tokens(s: &str) -> Vec<Tok> {
    let b = s.as_bytes();
    let mut i = 0;
    let mut v = Vec::new();
    while i < b.len() {
        match b[i] {
            b'<' => {
                let mut j = i + 1;
                let mut h = String::new();
                while j < b.len() && b[j] != b'>' {
                    if (b[j] as char).is_ascii_hexdigit() {
                        h.push(b[j] as char);
                    }
                    j += 1;
                }
                if h.len() % 2 == 1 {
                    h.push('0');
                }
                let bytes = (0..h.len() / 2)
                    .map(|k| u8::from_str_radix(&h[2 * k..2 * k + 2], 16).unwrap_or(0))
                    .collect();
                v.push(Tok::Hex(bytes));
                i = j + 1;
            }
            b'[' => {
                v.push(Tok::Open);
                i += 1;
            }
            b']' => {
                v.push(Tok::Close);
                i += 1;
            }
            _ => i += 1,
        }
    }
    v
}

/// Fonts in the page's /Resources (walking up /Parent for inherited resources).
/// Extracts the positioned text chunks of a page, resolving the page's fonts.
pub(crate) fn page_text(
    doc: &Document,
    page_id: ObjectId,
    cache: &FontCache,
) -> Result<Vec<Chunk>, Error> {
    let fonts = page_fonts(doc, page_id, cache);
    let content = doc.get_page_content(page_id);
    Ok(scan_chunks(&content, &fonts))
}

/// The page's `/Font` dictionary, following inherited `/Resources`.
fn page_font_dict(doc: &Document, page_id: ObjectId) -> Option<&Dictionary> {
    let mut cur = page_id;
    let mut res: Option<&Dictionary> = None;
    for _ in 0..16 {
        let Ok(d) = doc.get_dictionary(cur) else {
            break;
        };
        if let Ok(r) = d.get(b"Resources") {
            res = resolve(doc, r).as_dict().ok();
            break;
        }
        match d.get(b"Parent").and_then(Object::as_reference) {
            Ok(p) => cur = p,
            Err(_) => break,
        }
    }

    res?.get(b"Font")
        .ok()
        .map(|o| resolve(doc, o))
        .and_then(|o| o.as_dict().ok())
}

fn page_fonts(doc: &Document, page_id: ObjectId, cache: &FontCache) -> PageFonts {
    let Some(fd) = page_font_dict(doc, page_id) else {
        return PageFonts::new();
    };

    let mut fonts = PageFonts::new();
    for (name, v) in fd.iter() {
        let font = match v.as_reference() {
            Ok(id) => cache
                .get(&id)
                .cloned()
                .unwrap_or_else(|| Arc::new(load_font_by_id(doc, id))),
            Err(_) => match resolve(doc, v).as_dict() {
                Ok(f) => Arc::new(load_font(doc, f)),
                Err(_) => continue,
            },
        };
        fonts.insert(name.clone(), font);
    }
    fonts
}

// Text chunks
#[derive(Debug, Clone)]
pub(crate) struct Chunk {
    pub x: f32,
    pub y: f32,
    pub text: String,
}

// Content-stream scanner
//
// A page's content stream is a flat sequence of `operand... operator` groups.
// Statements use about ten operators, so instead of materialising every
// operation as an owned `Operation` (a `String` operator plus a `Vec<Object>`
// of operands each, thousands per page), this walks the decompressed bytes
// once, keeps operands in fixed slots and decodes text straight into one
// reusable buffer.

const fn is_ws(b: u8) -> bool {
    matches!(b, 0 | 9 | 10 | 12 | 13 | 32)
}

const fn is_delim(b: u8) -> bool {
    matches!(
        b,
        b'(' | b')' | b'<' | b'>' | b'[' | b']' | b'{' | b'}' | b'/' | b'%'
    )
}

const fn is_regular(b: u8) -> bool {
    !is_ws(b) && !is_delim(b)
}

/// Index of the end of the line that starts a `%` comment at `i`.
fn skip_comment(data: &[u8], mut i: usize) -> usize {
    while i < data.len() && data[i] != b'\n' && data[i] != b'\r' {
        i += 1;
    }
    i
}

/// Reads a literal string whose opening `(` is just before `i`, appending its
/// unescaped bytes to `out`. Returns the index after the closing `)`.
fn read_literal(data: &[u8], mut i: usize, out: &mut Vec<u8>) -> usize {
    let mut depth = 1u32;
    while i < data.len() {
        let b = data[i];
        i += 1;
        match b {
            b'\\' => {
                let Some(&e) = data.get(i) else { break };
                i += 1;
                match e {
                    b'n' => out.push(b'\n'),
                    b'r' => out.push(b'\r'),
                    b't' => out.push(b'\t'),
                    b'b' => out.push(0x08),
                    b'f' => out.push(0x0C),
                    b'0'..=b'7' => {
                        let mut v = u32::from(e - b'0');
                        for _ in 0..2 {
                            match data.get(i) {
                                Some(&d) if (b'0'..=b'7').contains(&d) => {
                                    v = v * 8 + u32::from(d - b'0');
                                    i += 1;
                                }
                                _ => break,
                            }
                        }
                        out.push(v as u8);
                    }
                    // Backslash-newline is a line continuation.
                    b'\r' => {
                        if data.get(i) == Some(&b'\n') {
                            i += 1;
                        }
                    }
                    b'\n' => {}
                    other => out.push(other),
                }
            }
            b'(' => {
                depth += 1;
                out.push(b);
            }
            b')' => {
                depth -= 1;
                if depth == 0 {
                    return i;
                }
                out.push(b);
            }
            _ => out.push(b),
        }
    }
    i
}

/// Reads a hex string whose opening `<` is just before `i`, appending its
/// bytes to `out`. Returns the index after the closing `>`.
fn read_hex(data: &[u8], mut i: usize, out: &mut Vec<u8>) -> usize {
    let mut high: Option<u8> = None;
    while i < data.len() {
        let b = data[i];
        i += 1;
        let v = match b {
            b'>' => break,
            b'0'..=b'9' => b - b'0',
            b'a'..=b'f' => b - b'a' + 10,
            b'A'..=b'F' => b - b'A' + 10,
            _ => continue,
        };
        match high.take() {
            Some(h) => out.push(h << 4 | v),
            None => high = Some(v),
        }
    }
    // An odd digit count is padded with a trailing zero.
    if let Some(h) = high {
        out.push(h << 4);
    }
    i
}

/// Skips a dictionary whose `<<` ends just before `i`. Strings inside it
/// (marked-content properties such as `/ActualText`) must not be mistaken for
/// text to show, so they are consumed but never decoded.
fn skip_dict(data: &[u8], mut i: usize, scratch: &mut Vec<u8>) -> usize {
    let mut depth = 1u32;
    while i < data.len() {
        match data[i] {
            b'(' => {
                scratch.clear();
                i = read_literal(data, i + 1, scratch);
            }
            b'<' if data.get(i + 1) == Some(&b'<') => {
                depth += 1;
                i += 2;
            }
            b'<' => {
                scratch.clear();
                i = read_hex(data, i + 1, scratch);
            }
            b'>' if data.get(i + 1) == Some(&b'>') => {
                depth -= 1;
                i += 2;
                if depth == 0 {
                    return i;
                }
            }
            b'%' => i = skip_comment(data, i),
            _ => i += 1,
        }
    }
    i
}

/// Reads a `TJ` array whose `[` ends just before `i`, decoding its strings
/// into `text` (which is cleared first). A large negative adjustment between
/// strings usually means a word gap. Returns the index after the `]`.
fn read_array(
    data: &[u8],
    mut i: usize,
    font: &Font,
    bytes: &mut Vec<u8>,
    text: &mut String,
) -> usize {
    text.clear();
    while i < data.len() {
        let b = data[i];
        match b {
            b']' => return i + 1,
            b'(' => {
                bytes.clear();
                i = read_literal(data, i + 1, bytes);
                font.decode_into(bytes, text);
            }
            b'<' if data.get(i + 1) == Some(&b'<') => i = skip_dict(data, i + 2, bytes),
            b'<' => {
                bytes.clear();
                i = read_hex(data, i + 1, bytes);
                font.decode_into(bytes, text);
            }
            b'%' => i = skip_comment(data, i),
            b'/' => {
                i += 1;
                while i < data.len() && is_regular(data[i]) {
                    i += 1;
                }
            }
            _ if is_regular(b) => {
                let start = i;
                while i < data.len() && is_regular(data[i]) {
                    i += 1;
                }
                if parse_number(&data[start..i]).is_some_and(|n| n < -200.0) {
                    text.push(' ');
                }
            }
            // Whitespace, and stray delimiters.
            _ => i += 1,
        }
    }
    i
}

/// Parses a PDF number (`12`, `-3.5`, `.25`, `4.`) without going through `str`.
fn parse_number(tok: &[u8]) -> Option<f32> {
    let (neg, digits) = match tok.first()? {
        b'-' => (true, &tok[1..]),
        b'+' => (false, &tok[1..]),
        _ => (false, tok),
    };
    let mut value = 0.0f64;
    let mut seen = false;
    let mut i = 0;
    while i < digits.len() && digits[i].is_ascii_digit() {
        value = value * 10.0 + f64::from(digits[i] - b'0');
        seen = true;
        i += 1;
    }
    if i < digits.len() && digits[i] == b'.' {
        i += 1;
        let mut scale = 0.1f64;
        while i < digits.len() && digits[i].is_ascii_digit() {
            value += f64::from(digits[i] - b'0') * scale;
            scale /= 10.0;
            seen = true;
            i += 1;
        }
    }
    if !seen || i != digits.len() {
        return None;
    }
    let value = if neg { -value } else { value };
    Some(value as f32)
}

/// Resolves `#xx` escapes in a name object.
fn unescape_name(name: &[u8]) -> Vec<u8> {
    let hex = |b: u8| char::from(b).to_digit(16).map(|d| d as u8);
    let mut out = Vec::with_capacity(name.len());
    let mut i = 0;
    while i < name.len() {
        if name[i] == b'#'
            && let (Some(h), Some(l)) = (
                name.get(i + 1).and_then(|&b| hex(b)),
                name.get(i + 2).and_then(|&b| hex(b)),
            )
        {
            out.push(h << 4 | l);
            i += 3;
        } else {
            out.push(name[i]);
            i += 1;
        }
    }
    out
}

/// Skips an inline image (`BI <dict> ID <data> EI`), whose binary data would
/// otherwise be tokenised as garbage. `i` is just after `BI`.
fn skip_inline_image(data: &[u8], mut i: usize) -> usize {
    let delimited = |i: usize, kw: &[u8; 2]| {
        data[i..i + 2] == *kw
            && (i == 0 || is_ws(data[i - 1]))
            && data.get(i + 2).is_none_or(|&b| is_ws(b))
    };
    while i + 2 <= data.len() {
        if delimited(i, b"ID") {
            i += 2;
            break;
        }
        i += 1;
    }
    while i + 2 <= data.len() {
        if delimited(i, b"EI") {
            return i + 2;
        }
        i += 1;
    }
    data.len()
}

fn emit(out: &mut Vec<Chunk>, tm: M, ctm: M, text: &str) {
    if text.trim().is_empty() {
        return;
    }
    let m = mul(tm, ctm);
    out.push(Chunk {
        x: m[4],
        y: m[5],
        text: text.to_owned(),
    });
}

/// Extracts the positioned text chunks from a decompressed content stream.
fn scan_chunks(data: &[u8], fonts: &PageFonts) -> Vec<Chunk> {
    let default_font = Font::default();
    let mut cur_font: &Font = &default_font;

    let mut out: Vec<Chunk> = Vec::new();
    let mut ctm = I;
    let mut stack: Vec<M> = Vec::new();
    let (mut tm, mut tlm, mut leading) = (I, I, 0.0f32);

    // Operands of the operator being collected. Only the first six numbers,
    // the last name and the last string/array operand are ever needed.
    let mut nums = [0.0f32; 6];
    let mut n_nums = 0usize;
    let mut name: &[u8] = &[];
    let mut text = String::new();
    let mut has_text = false;
    let mut bytes: Vec<u8> = Vec::new();

    let len = data.len();
    let mut i = 0;
    while i < len {
        let b = data[i];
        match b {
            _ if is_ws(b) => i += 1,
            b'%' => i = skip_comment(data, i),
            b'/' => {
                i += 1;
                let start = i;
                while i < len && is_regular(data[i]) {
                    i += 1;
                }
                name = &data[start..i];
            }
            b'(' => {
                bytes.clear();
                i = read_literal(data, i + 1, &mut bytes);
                text.clear();
                cur_font.decode_into(&bytes, &mut text);
                has_text = true;
            }
            b'<' if data.get(i + 1) == Some(&b'<') => i = skip_dict(data, i + 2, &mut bytes),
            b'<' => {
                bytes.clear();
                i = read_hex(data, i + 1, &mut bytes);
                text.clear();
                cur_font.decode_into(&bytes, &mut text);
                has_text = true;
            }
            b'[' => {
                i = read_array(data, i + 1, cur_font, &mut bytes, &mut text);
                has_text = true;
            }
            // Stray closing delimiters and procedure braces.
            b')' | b']' | b'>' | b'{' | b'}' => i += 1,
            _ => {
                let start = i;
                while i < len && is_regular(data[i]) {
                    i += 1;
                }
                let tok = &data[start..i];

                if matches!(tok[0], b'0'..=b'9' | b'+' | b'-' | b'.') {
                    if let Some(v) = parse_number(tok)
                        && n_nums < nums.len()
                    {
                        nums[n_nums] = v;
                        n_nums += 1;
                    }
                    continue;
                }

                match tok {
                    b"q" => stack.push(ctm),
                    b"Q" => {
                        if let Some(m) = stack.pop() {
                            ctm = m;
                        }
                    }
                    b"cm" => ctm = mul(nums, ctm),
                    b"BT" => {
                        tm = I;
                        tlm = I;
                    }
                    b"Tf" => {
                        let font = if name.contains(&b'#') {
                            fonts.get(unescape_name(name).as_slice())
                        } else {
                            fonts.get(name)
                        };
                        if let Some(font) = font {
                            cur_font = &**font;
                        }
                    }
                    b"TL" => leading = nums[0],
                    b"Tm" => {
                        tm = nums;
                        tlm = tm;
                    }
                    b"Td" => {
                        tlm = mul(translate(nums[0], nums[1]), tlm);
                        tm = tlm;
                    }
                    b"TD" => {
                        leading = -nums[1];
                        tlm = mul(translate(nums[0], nums[1]), tlm);
                        tm = tlm;
                    }
                    b"T*" => {
                        tlm = mul(translate(0.0, -leading), tlm);
                        tm = tlm;
                    }
                    b"Tj" | b"TJ" => {
                        if has_text {
                            emit(&mut out, tm, ctm, &text);
                        }
                    }
                    b"'" | b"\"" => {
                        tlm = mul(translate(0.0, -leading), tlm);
                        tm = tlm;
                        if has_text {
                            emit(&mut out, tm, ctm, &text);
                        }
                    }
                    b"BI" => i = skip_inline_image(data, i),
                    _ => {}
                }

                nums = [0.0; 6];
                n_nums = 0;
                name = &[];
                has_text = false;
            }
        }
    }
    out
}

// matrix helpers (PDF row-vector convention: apply a, then b)
fn mul(a: M, b: M) -> M {
    [
        a[0] * b[0] + a[1] * b[2],
        a[0] * b[1] + a[1] * b[3],
        a[2] * b[0] + a[3] * b[2],
        a[2] * b[1] + a[3] * b[3],
        a[4] * b[0] + a[5] * b[2] + b[4],
        a[4] * b[1] + a[5] * b[3] + b[5],
    ]
}

fn translate(tx: f32, ty: f32) -> M {
    [1., 0., 0., 1., tx, ty]
}

#[cfg(test)]
mod tests {
    use super::*;
    use lopdf::LoadOptions;

    fn scan(src: &str) -> Vec<(f32, f32, String)> {
        scan_chunks(src.as_bytes(), &PageFonts::new())
            .into_iter()
            .map(|c| (c.x, c.y, c.text))
            .collect()
    }

    #[test]
    fn shows_text_at_the_text_matrix_position() {
        let got = scan("BT /F1 9 Tf 1 0 0 1 100 200 Tm (Hello) Tj ET");
        assert_eq!(got, [(100.0, 200.0, "Hello".to_string())]);
    }

    #[test]
    fn applies_the_current_transformation_matrix() {
        let got =
            scan("q 1 0 0 1 10 20 cm BT 1 0 0 1 5 5 Tm (x) Tj ET Q BT 1 0 0 1 5 5 Tm (y) Tj ET");
        assert_eq!(got[0], (15.0, 25.0, "x".to_string()));
        assert_eq!(got[1], (5.0, 5.0, "y".to_string()));
    }

    #[test]
    fn tj_array_turns_big_gaps_into_spaces() {
        let got = scan("BT [(A) -300 (B) -50 (C)] TJ ET");
        assert_eq!(got[0].2, "A BC");
    }

    #[test]
    fn handles_escapes_nesting_and_hex_strings() {
        let got = scan(r"BT (a\(b\)\101 (nested) \\) Tj <4A4B4> Tj ET");
        assert_eq!(got[0].2, r"a(b)A (nested) \");
        assert_eq!(got[1].2, "JK@");
    }

    #[test]
    fn line_operators_move_the_text_line() {
        let got = scan("BT 14 TL 1 0 0 1 10 100 Tm (a) Tj T* (b) Tj (c) ' 0 -20 Td (d) Tj ET");
        let ys: Vec<f32> = got.iter().map(|c| c.1).collect();
        assert_eq!(ys, [100.0, 86.0, 72.0, 52.0]);
    }

    #[test]
    fn strings_inside_dictionaries_are_not_shown() {
        let got = scan("/Span <</ActualText (ghost) /K [(also ghost)]>> BDC BT (real) Tj ET EMC");
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].2, "real");
    }

    #[test]
    fn inline_images_and_comments_are_skipped() {
        let got = scan("BT (a) Tj ET q BI /W 1 /H 1 ID (x) Tj EI Q % (nope) Tj\nBT (b) Tj ET");
        let texts: Vec<_> = got.iter().map(|c| c.2.as_str()).collect();
        assert_eq!(texts, ["a", "b"]);
    }

    #[test]
    fn parses_pdf_numbers() {
        assert_eq!(parse_number(b"12"), Some(12.0));
        assert_eq!(parse_number(b"-3.5"), Some(-3.5));
        assert_eq!(parse_number(b".25"), Some(0.25));
        assert_eq!(parse_number(b"4."), Some(4.0));
        assert_eq!(parse_number(b"+7"), Some(7.0));
        assert_eq!(parse_number(b"-"), None);
        assert_eq!(parse_number(b"1x"), None);
    }

    #[test]
    fn resolves_name_escapes() {
        assert_eq!(unescape_name(b"F#31"), b"F1");
        assert_eq!(unescape_name(b"F#zz"), b"F#zz");
    }

    fn reference_chunks(doc: &Document, page_id: ObjectId, fonts: &PageFonts) -> Vec<Chunk> {
        fn num(o: Option<&Object>) -> f32 {
            o.and_then(|o| o.as_float().ok()).unwrap_or(0.0)
        }
        fn mat(a: &[Object]) -> M {
            std::array::from_fn(|i| num(a.get(i)))
        }
        fn show(font: &Font, o: &Object) -> String {
            let mut s = String::new();
            match o {
                Object::String(b, _) => font.decode_into(b, &mut s),
                Object::Array(a) => {
                    for e in a {
                        match e {
                            Object::String(b, _) => font.decode_into(b, &mut s),
                            Object::Integer(_) | Object::Real(_)
                                if e.as_float().map(|n| n < -200.0).unwrap_or(false) =>
                            {
                                s.push(' ')
                            }
                            _ => {}
                        }
                    }
                }
                _ => {}
            }
            s
        }

        let content = doc.get_and_decode_page_content(page_id).unwrap();
        let default_font = Font::default();
        let mut cur_font: &Font = &default_font;
        let mut out = Vec::new();
        let (mut ctm, mut stack) = (I, Vec::new());
        let (mut tm, mut tlm, mut leading) = (I, I, 0.0f32);
        for op in &content.operations {
            let a = &op.operands;
            let emit = |tm: M, ctm: M, s: String, out: &mut Vec<Chunk>| {
                if !s.trim().is_empty() {
                    let m = mul(tm, ctm);
                    out.push(Chunk {
                        x: m[4],
                        y: m[5],
                        text: s,
                    });
                }
            };
            match op.operator.as_str() {
                "q" => stack.push(ctm),
                "Q" => {
                    if let Some(m) = stack.pop() {
                        ctm = m
                    }
                }
                "cm" => ctm = mul(mat(a), ctm),
                "BT" => (tm, tlm) = (I, I),
                "Tf" => {
                    if let Some(f) = a
                        .first()
                        .and_then(|o| o.as_name().ok())
                        .and_then(|n| fonts.get(n))
                    {
                        cur_font = f;
                    }
                }
                "TL" => leading = num(a.first()),
                "Tm" => (tm, tlm) = (mat(a), mat(a)),
                "Td" | "TD" => {
                    if op.operator == "TD" {
                        leading = -num(a.get(1));
                    }
                    tlm = mul(translate(num(a.first()), num(a.get(1))), tlm);
                    tm = tlm;
                }
                "T*" => {
                    tlm = mul(translate(0.0, -leading), tlm);
                    tm = tlm;
                }
                "Tj" | "TJ" => {
                    if let Some(o) = a.first() {
                        emit(tm, ctm, show(cur_font, o), &mut out)
                    }
                }
                "'" | "\"" => {
                    tlm = mul(translate(0.0, -leading), tlm);
                    tm = tlm;
                    let o = if op.operator == "'" {
                        a.first()
                    } else {
                        a.get(2)
                    };
                    if let Some(o) = o {
                        emit(tm, ctm, show(cur_font, o), &mut out)
                    }
                }
                _ => {}
            }
        }
        out
    }

    /// Runs both implementations over every page of the sample statement.
    #[test]
    fn scanner_matches_lopdf_on_the_sample_statement() {
        let bytes = std::fs::read("statements/statement_1_123456.pdf").unwrap();
        let doc = Document::load_from_with_options(
            bytes.as_slice(),
            LoadOptions::with_password("123456"),
        )
        .unwrap();
        let page_ids: Vec<_> = doc.get_pages().into_values().collect();
        let cache = load_fonts(&doc, &page_ids);

        assert!(!page_ids.is_empty());
        for (n, &page_id) in page_ids.iter().enumerate() {
            let fonts = page_fonts(&doc, page_id, &cache);
            let want = reference_chunks(&doc, page_id, &fonts);
            let got = page_text(&doc, page_id, &cache).unwrap();

            assert_eq!(got.len(), want.len(), "chunk count, page {n}");
            for (g, w) in got.iter().zip(&want) {
                assert_eq!(g.text, w.text, "text, page {n}");
                assert!((g.x - w.x).abs() < 1e-2, "x, page {n}: {} vs {}", g.x, w.x);
                assert!((g.y - w.y).abs() < 1e-2, "y, page {n}: {} vs {}", g.y, w.y);
            }
        }
        // Content decoding must not have been trivially empty.
        assert!(page_text(&doc, page_ids[0], &cache).unwrap().len() > 10);
    }
}
