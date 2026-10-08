//! Renders Arabic in a text box (the Edit text editor's widget) to a PNG for visual checks:
//! `ARABIC_SHOT=out.png cargo test -p pdfcraft-ui-egui --test arabic_shot`.

use egui_kittest::Harness;

#[test]
fn arabic_text_edit_shot() {
    let Ok(out) = std::env::var("ARABIC_SHOT") else { return };
    let mut text = String::from("استلام الدعوة والوثائق خلال 3 أشهر (PDF)");
    let mut first = true;
    let mut h = Harness::builder().with_size(egui::vec2(520.0, 300.0)).with_pixels_per_point(2.0).build_ui(move |ui| {
        if first {
            pdfcraft_ui_egui::theme::install_fonts(ui.ctx());
            first = false;
        }
        ui.label("Label: ضابط الاتصال — وفقاً لإجراءات المنظمة");
        let align = if pdfcraft_fonts::bidi::is_rtl_paragraph(&text) { egui::Align::RIGHT } else { egui::Align::LEFT };
        let mut copy = text.clone();
        ui.add(egui::TextEdit::singleline(&mut copy).font(egui::FontId::proportional(18.0)).desired_width(480.0));
        let mut latin = String::from("Right aligned latin");
        ui.add(egui::TextEdit::multiline(&mut latin).horizontal_align(egui::Align::RIGHT).desired_rows(1).desired_width(480.0));
        let mut copy2 = text.clone();
        ui.add(egui::TextEdit::multiline(&mut copy2).font(egui::FontId::proportional(18.0)).desired_rows(1).desired_width(480.0));
        ui.add(egui::TextEdit::multiline(&mut text).font(egui::FontId::proportional(18.0)).horizontal_align(align).desired_width(480.0));
    });
    h.run_steps(3);
    h.render().unwrap().save(out).unwrap();
}
