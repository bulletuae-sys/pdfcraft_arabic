//! The interface fonts with and without the optional craft-fonts build input (`CRAFT_FONTS_DIR`).

use egui::epaint::text::{Fonts, TextOptions};
use egui::{Color32, FontFamily, FontId};
use pdfcraft_ui_egui::theme;

const JAPANESE: &str = "日本語の文字";
const CHINESE: &str = "简体中文欢迎";

fn families() -> Vec<FontId> {
    vec![FontId::proportional(13.0), FontId::monospace(13.0), theme::medium(13.0), theme::semibold(17.0)]
}

/// Lays `text` out in every interface family and returns each galley's width.
fn layout_widths(fonts: &mut Fonts, text: &str) -> Vec<f32> {
    let mut view = fonts.with_pixels_per_point(2.0);
    families().into_iter().map(|id| view.layout_no_wrap(text.to_owned(), id, Color32::BLACK).size().x).collect()
}

/// Built with craft-fonts, Japanese text renders with real glyphs (no tofu) in every family,
/// from a craft-fonts face placed after the app's own fonts.
#[test]
fn japanese_ui_text_uses_craft_fonts() {
    if pdfcraft_fonts::ui_japanese_fonts().is_empty() {
        eprintln!("skipping japanese_ui_text_uses_craft_fonts: built without craft-fonts (set CRAFT_FONTS_DIR to run it)");
        return;
    }
    let defs = theme::font_definitions();
    for family in [FontFamily::Proportional, FontFamily::Monospace] {
        let stack = &defs.families[&family];
        let first_jp = stack.iter().position(|n| n.starts_with("BIZ UDPGothic")).expect("BIZ UDPGothic is a fallback");
        let own = stack.iter().position(|n| n == "Inter" || n == "JetBrainsMono").expect("the app's own font");
        assert!(own < first_jp, "{family:?}: {stack:?}");
        assert!(stack[first_jp..].iter().all(|n| pdfcraft_fonts::CRAFT_FONTS.iter().any(|f| f.name() == *n)), "{stack:?}");
    }
    let mut fonts = Fonts::new(TextOptions::default(), defs);
    for id in families() {
        assert!(fonts.has_glyphs(&id, JAPANESE), "{id:?} lacks {JAPANESE}");
    }
    // Real glyphs are about a full em wide each; tofu boxes and missing glyphs are not.
    for w in layout_widths(&mut fonts, JAPANESE) {
        assert!(w > 13.0 * 0.8 * JAPANESE.chars().count() as f32, "{w}");
    }
}

/// Chinese mode orders the CJK fallback with the `Hans` group first, so one line never
/// mixes faces with different vertical metrics. Without an allowed `Hans` face in the
/// build input the order part still holds; glyph coverage follows the craft-fonts checkout.
#[test]
fn chinese_ui_text_prefers_the_chinese_face() {
    if pdfcraft_fonts::ui_chinese_fonts().is_empty() {
        eprintln!(
            "skipping chinese_ui_text_prefers_the_chinese_face: no Hans face bundled (set CRAFT_FONTS_DIR with an allowed Chinese face to run it)"
        );
        return;
    }
    let defs = theme::font_definitions_for(true);
    let hans: Vec<String> = pdfcraft_fonts::ui_chinese_fonts().iter().map(|f| f.name()).collect();
    for family in [FontFamily::Proportional, FontFamily::Monospace] {
        let stack = &defs.families[&family];
        let first_zh = stack.iter().position(|n| hans.iter().any(|h| h == n)).expect("a Hans face is a fallback");
        let first_ja = stack.iter().position(|n| n.starts_with("BIZ UDPGothic")).expect("BIZ UDPGothic is a fallback");
        let own = stack.iter().position(|n| n == "Inter" || n == "JetBrainsMono").expect("the app's own font");
        assert!(own < first_zh && first_zh < first_ja, "{family:?}: {stack:?}");
    }
    let mut fonts = Fonts::new(TextOptions::default(), defs);
    for id in families() {
        assert!(fonts.has_glyphs(&id, CHINESE), "{id:?} lacks {CHINESE}");
    }
    for w in layout_widths(&mut fonts, CHINESE) {
        assert!(w > 13.0 * 0.8 * CHINESE.chars().count() as f32, "{w}");
    }
}

/// Without craft-fonts the interface fonts still install and lay out any text (Japanese falls
/// back to egui's replacement glyph) without panicking; Latin text is unaffected.
#[test]
fn ui_fonts_work_without_craft_fonts() {
    let mut fonts = Fonts::new(TextOptions::default(), theme::font_definitions());
    for id in families() {
        assert!(fonts.has_glyphs(&id, "PdfCraft"), "{id:?}");
        assert_eq!(fonts.has_glyphs(&id, JAPANESE), !pdfcraft_fonts::ui_japanese_fonts().is_empty(), "{id:?}");
    }
    assert!(layout_widths(&mut fonts, JAPANESE).iter().all(|w| w.is_finite() && *w > 0.0));
    assert!(layout_widths(&mut fonts, "PdfCraft").iter().all(|w| *w > 20.0));
    let ctx = egui::Context::default();
    theme::install_fonts(&ctx);
}

/// Latin-script catalogs (Czech, Brazilian Portuguese) are drawn entirely by the app's own faces
/// (Inter, JetBrains Mono): no letter falls through to egui's defaults or a CJK fallback. (egui's
/// `has_glyphs` can't answer this: with only the primary face it is also the replacement face.)
#[test]
fn primary_ui_fonts_cover_latin_catalogs() {
    use skrifa::MetadataProvider as _;
    let defs = theme::font_definitions();
    for (code, catalog) in [("cs", include_str!("../src/i18n/cs.tsv")), ("pt-br", include_str!("../src/i18n/pt-br.tsv"))] {
        let mut text = String::from("áčďéěíňóřšťúůýžÁČĎÉĚÍŇÓŘŠŤÚŮÝŽãõçâêôàÃÕÇÂÊÔÀ…");
        for line in catalog.lines().filter(|l| !l.starts_with('#')) {
            if let Some(translation) = line.split('\t').nth(2) {
                text.extend(translation.chars().filter(|c| !c.is_whitespace()));
            }
        }
        for name in ["Inter", "Inter-Medium", "Inter-SemiBold", "JetBrainsMono"] {
            let data = &defs.font_data[name];
            let font = skrifa::FontRef::from_index(&data.font, data.index).unwrap();
            let charmap = font.charmap();
            let mut missing: Vec<char> = text.chars().filter(|c| charmap.map(*c).is_none()).collect();
            missing.sort_unstable();
            missing.dedup();
            assert!(missing.is_empty(), "{code}: {name} lacks {missing:?}");
        }
    }
}

/// Built with craft-fonts' Noto Sans Arabic, Arabic interface text has real glyphs, joined
/// letters and right-to-left order: the first letter is on the right, the cursor before it at
/// its right edge, and clicking left of the text puts the cursor at its end.
#[test]
fn arabic_ui_text_is_shaped_right_to_left() {
    if pdfcraft_fonts::ui_arabic_fonts().is_empty() {
        eprintln!("skipping arabic_ui_text_is_shaped_right_to_left: built without craft-fonts' Arabic face");
        return;
    }
    let text = "استلام الدعوة";
    let n = text.chars().count();
    let mut fonts = Fonts::new(TextOptions::default(), theme::font_definitions());
    for id in families() {
        assert!(fonts.has_glyphs(&id, text), "{id:?} lacks Arabic");
    }
    let mut view = fonts.with_pixels_per_point(2.0);
    let galley = view.layout_no_wrap(text.to_owned(), FontId::proportional(13.0), Color32::BLACK);
    let row = &galley.rows[0];
    assert_eq!(row.glyphs.len(), n, "one glyph per char");
    // Logical first char (ا) is right of the last (ة); the first word is right of the second.
    assert!(row.glyphs[0].pos.x > row.glyphs[n - 1].pos.x);
    assert!(row.glyphs[1].pos.x > row.glyphs[8].pos.x);
    let before_first = galley.pos_from_cursor(egui::text::CCursor::new(0)).min.x;
    let at_end = galley.pos_from_cursor(egui::text::CCursor::new(n)).min.x;
    assert!(before_first > at_end + 20.0, "{before_first} vs {at_end}");
    assert_eq!(galley.cursor_from_pos(egui::vec2(-5.0, 5.0)).index, egui::text::CharIndex(n));
    assert_eq!(galley.cursor_from_pos(egui::vec2(galley.size().x + 5.0, 5.0)).index, egui::text::CharIndex(0));
    // Joined forms are narrower than the same letters isolated.
    let isolated: f32 = text
        .chars()
        .filter(|c| !c.is_whitespace())
        .map(|c| view.layout_no_wrap(c.to_string(), FontId::proportional(13.0), Color32::BLACK).size().x)
        .sum();
    assert!(galley.size().x < isolated, "{} vs isolated {isolated}", galley.size().x);
    // Latin text is unaffected.
    let latin = view.layout_no_wrap("Hello".to_owned(), FontId::proportional(13.0), Color32::BLACK);
    let g = &latin.rows[0].glyphs;
    assert!(g.windows(2).all(|w| w[0].pos.x < w[1].pos.x));
}
