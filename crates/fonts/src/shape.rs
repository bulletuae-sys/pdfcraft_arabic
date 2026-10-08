//! Shaped text for scripts whose letters change shape with their neighbours (Arabic): one line
//! is split into bidi runs, each run is shaped with HarfRust in the face that has its letters,
//! and the glyphs come back in visual order, ready to be drawn left to right.
//!
//! Faces: Noto Sans Arabic (craft-fonts, `Arab`) for Arabic, the bundled Inter for Latin,
//! digits and punctuation, and the Japanese document face for anything else it covers.

use skrifa::instance::{LocationRef, Size};
use skrifa::outline::DrawSettings;
use skrifa::{FontRef, MetadataProvider};

use crate::bidi;
use crate::script::{Flatten, GlyphError, GlyphOutline};

static INTER: &[u8] = include_bytes!("../../../assets/fonts/Inter-Regular.ttf");

/// Longest line shaped at once (chars), so hostile text can't make shaping unbounded.
pub const MAX_SHAPED_CHARS: usize = 4096;

/// Which face a shaped glyph comes from.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Face {
    Arabic,
    Latin,
    Japanese,
}

impl Face {
    fn bytes(self) -> Option<&'static [u8]> {
        match self {
            Face::Arabic => crate::document_arabic_font().map(|f| f.bytes),
            Face::Latin => Some(INTER),
            Face::Japanese => crate::document_japanese_font().map(|f| f.bytes),
        }
    }

    /// The family name, for messages ("Noto Sans Arabic").
    pub fn family(self) -> &'static str {
        match self {
            Face::Arabic => crate::document_arabic_font().map_or("Noto Sans Arabic", |f| f.family),
            Face::Latin => "Inter",
            Face::Japanese => crate::document_japanese_font().map_or("Shippori Mincho", |f| f.family),
        }
    }

    fn has(self, c: char) -> bool {
        self.bytes().and_then(|b| FontRef::new(b).ok()).is_some_and(|f| f.charmap().map(c).is_some())
    }
}

/// One glyph of a shaped line, in em units (font size 1).
#[derive(Clone, Debug, PartialEq)]
pub struct ShapedGlyph {
    pub face: Face,
    pub gid: u32,
    pub advance: f64,
    pub x_offset: f64,
    pub y_offset: f64,
    /// The text this glyph stands for, in visual order (reversed for right-to-left glyphs, as
    /// text extraction reads glyphs left to right); empty for the extra glyphs of a cluster.
    pub text: String,
}

/// A shaped line: glyphs left to right, and its width in em.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ShapedLine {
    pub glyphs: Vec<ShapedGlyph>,
    pub width: f64,
}

/// Whether `text` must be shaped to be drawn (it has Arabic letters).
pub fn needs_shaping(text: &str) -> bool {
    text.chars().any(bidi::is_arabic_char)
}

/// Combining marks and joiners stay in the face of the letter they belong to.
fn is_attached(c: char) -> bool {
    matches!(c, '\u{064B}'..='\u{065F}' | '\u{0670}' | '\u{06D6}'..='\u{06ED}' | '\u{08D3}'..='\u{08FF}' | '\u{200C}' | '\u{200D}' | '\u{0640}')
        || matches!(c, '\u{0300}'..='\u{036F}')
}

/// The face for each char of a line.
fn faces(chars: &[char]) -> Result<Vec<Face>, GlyphError> {
    let mut out: Vec<Face> = Vec::with_capacity(chars.len());
    for (i, &c) in chars.iter().enumerate() {
        let prev = out.last().copied();
        let face = if bidi::is_arabic_char(c) {
            if crate::document_arabic_font().is_none() {
                return Err(GlyphError::NoFont);
            }
            Face::Arabic
        } else if is_attached(c)
            && let Some(p) = prev
            && p.has(c)
        {
            p
        } else if c.is_whitespace() {
            // A space between Arabic words stays Arabic (one run, its own spacing).
            match (prev, chars.get(i + 1).copied()) {
                (Some(Face::Arabic), Some(n)) if bidi::is_arabic_char(n) && Face::Arabic.has(' ') => Face::Arabic,
                _ => Face::Latin,
            }
        } else if Face::Latin.has(c) {
            Face::Latin
        } else if Face::Japanese.has(c) {
            Face::Japanese
        } else if Face::Arabic.has(c) {
            Face::Arabic
        } else {
            return Err(GlyphError::Missing);
        };
        out.push(face);
    }
    Ok(out)
}

/// Shape one line (no line breaks) of a paragraph whose direction is `rtl`.
pub fn shape_line(line: &str, rtl: bool) -> Result<ShapedLine, GlyphError> {
    let chars: Vec<char> = line.chars().filter(|c| *c != '\n' && *c != '\r').collect();
    if chars.len() > MAX_SHAPED_CHARS {
        return Err(GlyphError::TooComplex);
    }
    if chars.is_empty() {
        return Ok(ShapedLine::default());
    }
    let text: String = chars.iter().collect();
    let faces = faces(&chars)?;
    let levels = bidi::levels(&text, rtl);
    let order = bidi::visual_order(&chars, &levels, rtl);
    // Group the visual order into runs: logically adjacent chars with one level and one face.
    let mut runs: Vec<(usize, usize, bool, Face)> = Vec::new(); // (first, last) logical, rtl, face
    let mut k = 0;
    while k < order.len() {
        let i = order[k];
        let (lv, face) = (levels.get(i).copied().unwrap_or(0), faces[i]);
        let run_rtl = lv % 2 == 1;
        let mut j = k + 1;
        let mut last = i;
        while j < order.len() {
            let n = order[j];
            let adjacent = if run_rtl { n + 1 == last } else { n == last + 1 };
            if !adjacent || levels.get(n).copied().unwrap_or(0) != lv || faces[n] != face {
                break;
            }
            last = n;
            j += 1;
        }
        let (first, end) = if run_rtl { (last, i) } else { (i, last) };
        runs.push((first, end, run_rtl, face));
        k = j;
    }
    let mut line_out = ShapedLine::default();
    for (first, last, run_rtl, face) in runs {
        let run: String = chars[first..=last].iter().collect();
        shape_run(&run, run_rtl, face, &mut line_out)?;
    }
    Ok(line_out)
}

fn shape_run(run: &str, rtl: bool, face: Face, out: &mut ShapedLine) -> Result<(), GlyphError> {
    let bytes = face.bytes().ok_or(GlyphError::NoFont)?;
    let font = harfrust::FontRef::new(bytes).map_err(|_| GlyphError::Missing)?;
    let upem = FontRef::new(bytes).map_err(|_| GlyphError::Missing)?.metrics(Size::unscaled(), LocationRef::default()).units_per_em.max(1) as f64;
    let data = harfrust::ShaperData::new(&font);
    let shaper = data.shaper(&font).build();
    let mut buffer = harfrust::UnicodeBuffer::new();
    buffer.push_str(run);
    buffer.guess_segment_properties();
    buffer.set_direction(if rtl { harfrust::Direction::RightToLeft } else { harfrust::Direction::LeftToRight });
    let shaped = shaper.shape(buffer, harfrust::ShapeOptions::new());
    let infos = shaped.glyph_infos();
    let positions = shaped.glyph_positions();
    // Cluster boundaries (byte offsets into `run`), to give each cluster its text.
    let mut bounds: Vec<usize> = infos.iter().map(|g| g.cluster as usize).collect();
    bounds.push(run.len());
    bounds.sort_unstable();
    bounds.dedup();
    let mut seen: Vec<u32> = Vec::new();
    for (info, pos) in infos.iter().zip(positions) {
        let text = if seen.contains(&info.cluster) {
            String::new()
        } else {
            seen.push(info.cluster);
            let start = info.cluster as usize;
            let end = bounds.iter().copied().find(|b| *b > start).unwrap_or(run.len());
            let t = run.get(start..end).unwrap_or_default();
            if rtl { t.chars().rev().collect() } else { t.to_string() }
        };
        let advance = pos.x_advance as f64 / upem;
        out.glyphs.push(ShapedGlyph {
            face,
            gid: info.glyph_id,
            advance,
            x_offset: pos.x_offset as f64 / upem,
            y_offset: pos.y_offset as f64 / upem,
            text,
        });
        out.width += advance;
    }
    Ok(())
}

/// The outline of a shaped glyph (em units), moved by its offsets; `width` is its advance.
pub fn shaped_glyph_outline(g: &ShapedGlyph) -> Result<GlyphOutline, GlyphError> {
    let bytes = g.face.bytes().ok_or(GlyphError::NoFont)?;
    let font = FontRef::new(bytes).map_err(|_| GlyphError::Missing)?;
    let loc = LocationRef::default();
    let scale = 1.0 / font.metrics(Size::unscaled(), loc).units_per_em.max(1) as f64;
    let gid = skrifa::GlyphId::new(g.gid);
    let mut pen = Flatten::new(scale);
    pen.dx = g.x_offset;
    pen.dy = g.y_offset;
    if let Some(glyph) = font.outline_glyphs().get(gid) {
        let _ = glyph.draw(DrawSettings::unhinted(Size::unscaled(), loc), &mut pen);
    }
    pen.close();
    if pen.too_complex || pen.contours.len() > 256 {
        return Err(GlyphError::TooComplex);
    }
    let width = g.advance;
    if !width.is_finite() || !(-2.0..=4.0).contains(&width) {
        return Err(GlyphError::Missing);
    }
    let mut bbox = [0.0, 0.0, width.max(0.0), 0.0];
    let mut any = false;
    for p in pen.contours.iter().flatten() {
        if !p[0].is_finite() || !p[1].is_finite() {
            return Err(GlyphError::Missing);
        }
        if !any {
            bbox = [p[0], p[1], p[0], p[1]];
            any = true;
        } else {
            bbox[0] = bbox[0].min(p[0]);
            bbox[1] = bbox[1].min(p[1]);
            bbox[2] = bbox[2].max(p[0]);
            bbox[3] = bbox[3].max(p[1]);
        }
    }
    Ok(GlyphOutline { contours: pen.contours, width, bbox })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn latin_text_needs_no_shaping() {
        assert!(!needs_shaping("Hello, 2026"));
        assert!(needs_shaping("مرحبا"));
    }

    #[test]
    fn arabic_is_shaped_right_to_left_with_joined_letters() {
        if crate::document_arabic_font().is_none() {
            eprintln!("skipping: built without craft-fonts' Noto Sans Arabic (set CRAFT_FONTS_DIR)");
            assert_eq!(shape_line("سلام", true), Err(GlyphError::NoFont));
            return;
        }
        let line = shape_line("سلام عليكم", true).expect("shapes");
        assert!(line.width > 1.0, "{}", line.width);
        // Read left to right, the glyphs' text is the logical text reversed.
        let visual: String = line.glyphs.iter().map(|g| g.text.as_str()).collect();
        assert_eq!(visual, "سلام عليكم".chars().rev().collect::<String>());
        // Joined forms: the initial seen of سلام is not the isolated one.
        let isolated = shape_line("س", true).unwrap().glyphs[0].gid;
        let last = line.glyphs.last().unwrap();
        assert_eq!(last.text, "س");
        assert_ne!(last.gid, isolated);
        // Lam-alef (a ligature, or contextual forms, depending on the face) keeps both letters.
        let la = shape_line("لا", true).unwrap();
        assert_eq!(la.glyphs.iter().map(|g| g.text.as_str()).collect::<String>(), "ال");
        // Mixed with Latin and digits: those keep left-to-right order.
        let mixed = shape_line("تقرير PDF لعام 2026", true).unwrap();
        let visual: String = mixed.glyphs.iter().map(|g| g.text.as_str()).collect();
        assert!(visual.contains("PDF") && visual.contains("2026"), "{visual}");
        assert!(visual.find("2026") < visual.find("PDF"), "{visual}");
        for g in &mixed.glyphs {
            let o = shaped_glyph_outline(g).expect("outline");
            assert!(o.width.is_finite());
        }
    }
}
