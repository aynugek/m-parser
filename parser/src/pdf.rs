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
    fn decode(&self, bytes: &[u8]) -> String {
        let mut out = String::with_capacity(bytes.len());
        if self.two_byte {
            for pair in bytes.as_chunks::<2>().0 {
                let code = u16::from_be_bytes([pair[0], pair[1]]) as u32;
                if !self.glyphs.push(code, &mut out) && self.glyphs.is_empty() {
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
                if !self.glyphs.push(b as u32, &mut out) {
                    out.push(b as char); // Latin-1 ~ WinAnsi for ASCII text
                }
            }
        }
        out
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

/// Loads every font referenced by the document's pages exactly once.
///
/// Pages of one statement normally share their font objects, so this removes
/// the repeated `/ToUnicode` decompression and parsing per page.
pub(crate) fn load_fonts(doc: &Document) -> FontCache {
    let mut ids = BTreeSet::new();
    for page_id in doc.get_pages().into_values() {
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
    page_chunks(doc, page_id, &fonts)
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

fn page_chunks(doc: &Document, page_id: ObjectId, fonts: &PageFonts) -> Result<Vec<Chunk>, Error> {
    let content = doc.get_and_decode_page_content(page_id)?;
    let default_font = Font::default();
    let mut cur_font: &Font = &default_font;

    let mut out: Vec<Chunk> = Vec::new();
    let mut ctm = I;
    let mut stack: Vec<M> = Vec::new();
    let (mut tm, mut tlm, mut leading) = (I, I, 0.0f32);

    let mut emit = |tm: M, ctm: M, s: String| {
        if s.trim().is_empty() {
            return;
        }
        let m = mul(tm, ctm);
        out.push(Chunk {
            x: m[4],
            y: m[5],
            text: s,
        });
    };

    for op in &content.operations {
        let a = &op.operands;
        match op.operator.as_str() {
            "q" => stack.push(ctm),
            "Q" => {
                if let Some(m) = stack.pop() {
                    ctm = m;
                }
            }
            "cm" => ctm = mul(mat(a), ctm),
            "BT" => {
                tm = I;
                tlm = I;
            }
            "Tf" => {
                if let Some(font) = a
                    .first()
                    .and_then(|o| o.as_name().ok())
                    .and_then(|name| fonts.get(name))
                {
                    cur_font = font;
                }
            }
            "TL" => leading = num(a.first()),
            "Tm" => {
                tm = mat(a);
                tlm = tm;
            }
            "Td" => {
                tlm = mul(translate(num(a.first()), num(a.get(1))), tlm);
                tm = tlm;
            }
            "TD" => {
                leading = -num(a.get(1));
                tlm = mul(translate(num(a.first()), num(a.get(1))), tlm);
                tm = tlm;
            }
            "T*" => {
                tlm = mul(translate(0.0, -leading), tlm);
                tm = tlm;
            }
            "Tj" | "TJ" => {
                if let Some(o) = a.first() {
                    emit(tm, ctm, show(cur_font, o));
                }
            }
            "'" => {
                tlm = mul(translate(0.0, -leading), tlm);
                tm = tlm;
                if let Some(o) = a.first() {
                    emit(tm, ctm, show(cur_font, o));
                }
            }
            "\"" => {
                tlm = mul(translate(0.0, -leading), tlm);
                tm = tlm;
                if let Some(o) = a.get(2) {
                    emit(tm, ctm, show(cur_font, o));
                }
            }
            _ => {}
        }
    }
    Ok(out)
}

fn show(font: &Font, o: &Object) -> String {
    match o {
        Object::String(b, _) => font.decode(b),
        Object::Array(a) => {
            let mut s = String::with_capacity(a.len() * 4);
            for e in a {
                match e {
                    Object::String(b, _) => s.push_str(&font.decode(b)),
                    // Big negative kerning adjustment usually means a word gap.
                    Object::Integer(_) | Object::Real(_)
                        if e.as_float().map(|n| n < -200.0).unwrap_or(false) =>
                    {
                        s.push(' ');
                    }
                    _ => {}
                }
            }
            s
        }
        _ => String::new(),
    }
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

fn num(o: Option<&Object>) -> f32 {
    o.and_then(|o| o.as_float().ok()).unwrap_or(0.0)
}

fn mat(a: &[Object]) -> M {
    [
        num(a.first()),
        num(a.get(1)),
        num(a.get(2)),
        num(a.get(3)),
        num(a.get(4)),
        num(a.get(5)),
    ]
}
