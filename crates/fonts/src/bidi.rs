//! Right-to-left text (Arabic, Hebrew): paragraph direction, the visual order of a line
//! (Unicode bidi rules L1–L2), and turning text extracted in visual order back into the logical
//! order people type and read.

use unicode_bidi::{BidiInfo, Level};
use unicode_normalization::UnicodeNormalization as _;

/// A strong right-to-left character (Hebrew, Arabic, Syriac, Thaana, N'Ko, … and their
/// presentation forms) or an explicit right-to-left mark.
pub fn is_rtl_char(c: char) -> bool {
    matches!(c,
        '\u{0590}'..='\u{08FF}'
        | '\u{FB1D}'..='\u{FDFF}'
        | '\u{FE70}'..='\u{FEFF}'
        | '\u{10800}'..='\u{10FFF}'
        | '\u{1E800}'..='\u{1EFFF}'
        | '\u{200F}' | '\u{202B}' | '\u{202E}' | '\u{2067}')
}

/// Arabic-script characters (letters, marks, digits, punctuation and presentation forms):
/// text containing them has to be shaped (letters join) before it can be drawn.
pub fn is_arabic_char(c: char) -> bool {
    matches!(c, '\u{0600}'..='\u{06FF}' | '\u{0750}'..='\u{077F}' | '\u{0870}'..='\u{08FF}' | '\u{FB50}'..='\u{FDFF}' | '\u{FE70}'..='\u{FEFF}')
}

/// Whether `text` has any right-to-left character.
pub fn has_rtl(text: &str) -> bool {
    text.chars().any(is_rtl_char)
}

/// Whether a paragraph reads right to left: its first strong character is right-to-left
/// (rule P2), so Arabic text starting with a Latin word still counts when Arabic dominates.
pub fn is_rtl_paragraph(text: &str) -> bool {
    if !has_rtl(text) {
        return false;
    }
    let info = BidiInfo::new(text, None);
    info.paragraphs.first().is_some_and(|p| p.level.is_rtl())
}

/// The embedding level of each char of `line` in a paragraph of direction `rtl`.
pub fn levels(line: &str, rtl: bool) -> Vec<u8> {
    let base = if rtl { Level::rtl() } else { Level::ltr() };
    if !has_rtl(line) && !rtl {
        return vec![0; line.chars().count()];
    }
    let info = BidiInfo::new(line, Some(base));
    line.char_indices().map(|(i, _)| info.levels.get(i).map_or(base.number(), |l| l.number())).collect()
}

/// The visual order (left to right) of the chars of one line, as indexes into its chars.
/// `levels` comes from [`levels`]; trailing whitespace takes the paragraph level (L1).
pub fn visual_order(chars: &[char], levels: &[u8], rtl: bool) -> Vec<usize> {
    let n = chars.len().min(levels.len());
    let mut levels = levels[..n].to_vec();
    let para = u8::from(rtl);
    for i in (0..n).rev() {
        if chars[i].is_whitespace() {
            levels[i] = para;
        } else {
            break;
        }
    }
    let mut order: Vec<usize> = (0..n).collect();
    let max = levels.iter().copied().max().unwrap_or(0);
    let Some(lowest_odd) = levels.iter().copied().filter(|l| l % 2 == 1).min() else {
        return order;
    };
    let mut level = max;
    while level >= lowest_odd {
        let mut i = 0;
        while i < n {
            if levels[order[i]] >= level {
                let start = i;
                while i < n && levels[order[i]] >= level {
                    i += 1;
                }
                order[start..i].reverse();
            } else {
                i += 1;
            }
        }
        level = level.saturating_sub(1);
        if level == 0 {
            break;
        }
    }
    order
}

/// Text read off a page in visual order (left to right, as glyphs are placed) → logical order.
/// Lines without right-to-left text are returned unchanged. Arabic presentation forms
/// (already-joined letter shapes, ligatures such as لا) become ordinary letters again, so the
/// text can be edited and shaped anew.
pub fn visual_to_logical(text: &str) -> String {
    if !has_rtl(text) {
        return text.to_string();
    }
    let chars: Vec<char> = text.chars().collect();
    // Mostly right-to-left text is a right-to-left paragraph (in visual order the first strong
    // character is the last word's, so P2 can't be used).
    let rtl_count = chars.iter().filter(|c| is_rtl_char(**c)).count();
    let ltr_count = chars.iter().filter(|c| c.is_alphabetic() && !is_rtl_char(**c)).count();
    let rtl = rtl_count >= ltr_count;
    // The bidi rules applied to visual text: a right-to-left run reversed reads logically, and
    // numbers and Latin words inside it keep their own direction.
    let lv = levels(text, rtl);
    let order = visual_order(&chars, &lv, rtl);
    let reordered: String = order.iter().filter_map(|&i| chars.get(i)).collect();
    // Trailing/leading spaces moved by the reversal are not meaningful in a line.
    let reordered = reordered.trim().to_string();
    normalize_presentation_forms(&reordered)
}

/// Arabic presentation forms (U+FB50–U+FDFF, U+FE70–U+FEFF) → their ordinary letters.
pub fn normalize_presentation_forms(text: &str) -> String {
    if !text.chars().any(|c| matches!(c, '\u{FB50}'..='\u{FDFF}' | '\u{FE70}'..='\u{FEFE}')) {
        return text.to_string();
    }
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        if matches!(c, '\u{FB50}'..='\u{FDFF}' | '\u{FE70}'..='\u{FEFE}') {
            // Ligatures of words (ﷲ, ﷺ) expand to their letters too; a space-only result
            // (isolated tashkil forms decompose to space + mark) keeps just the mark.
            let decomposed: String = std::iter::once(c).nfkc().collect();
            out.extend(decomposed.chars().filter(|d| !(d.is_whitespace() && decomposed.chars().count() > 1)));
        } else {
            out.push(c);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paragraph_direction_follows_the_first_strong_character() {
        assert!(is_rtl_paragraph("استلام الدعوة والوثائق"));
        assert!(is_rtl_paragraph("3 أشهر"));
        assert!(!is_rtl_paragraph("PDF وثيقة"));
        assert!(!is_rtl_paragraph("Hello"));
    }

    #[test]
    fn arabic_words_read_right_to_left() {
        let line = "مرحبا بكم";
        let chars: Vec<char> = line.chars().collect();
        let order = visual_order(&chars, &levels(line, true), true);
        let visual: String = order.iter().map(|&i| chars[i]).collect();
        assert_eq!(visual, line.chars().rev().collect::<String>());
        // Numbers inside Arabic keep their order.
        let line = "عام 2026 م";
        let chars: Vec<char> = line.chars().collect();
        let visual: String = visual_order(&chars, &levels(line, true), true).iter().map(|&i| chars[i]).collect();
        assert_eq!(visual, "م 2026 ماع");
    }

    #[test]
    fn visual_text_reads_back_logically() {
        assert_eq!(visual_to_logical("Hello world"), "Hello world");
        let logical = "استلام الدعوة والوثائق";
        let visual: String = logical.chars().rev().collect();
        assert_eq!(visual_to_logical(&visual), logical);
        assert_eq!(visual_to_logical("م 2026 ماع"), "عام 2026 م");
        // Presentation forms (final meem, lam-alef ligature) become letters.
        assert_eq!(normalize_presentation_forms("\u{FEE2}\u{FEFB}"), "ملا");
    }
}
