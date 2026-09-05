use eframe::egui::{self, Color32, FontId, RichText, Stroke};
pub const BG: Color32 = Color32::from_rgb(16, 21, 29);
pub const SIDE: Color32 = Color32::from_rgb(21, 28, 38);
pub const CARD: Color32 = Color32::from_rgb(26, 35, 47);
pub const LINE: Color32 = Color32::from_rgb(47, 60, 77);
pub const TEXT: Color32 = Color32::from_rgb(231, 237, 245);
pub const MUTED: Color32 = Color32::from_rgb(166, 180, 199);
pub const ACCENT: Color32 = Color32::from_rgb(115, 185, 255);
pub const GREEN: Color32 = Color32::from_rgb(115, 209, 175);
pub fn setup(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();
    fonts.font_data.insert(
        "Noto Sans".into(),
        egui::FontData::from_static(include_bytes!("../assets/NotoSans-Regular.ttf")).into(),
    );
    fonts
        .families
        .get_mut(&egui::FontFamily::Proportional)
        .unwrap()
        .insert(0, "Noto Sans".into());
    ctx.set_fonts(fonts);
    let mut style = (*ctx.style()).clone();
    style.visuals = egui::Visuals::dark();
    style.visuals.override_text_color = Some(TEXT);
    style.visuals.panel_fill = BG;
    style.visuals.window_fill = CARD;
    style.visuals.extreme_bg_color = SIDE;
    style.visuals.faint_bg_color = SIDE;
    style.visuals.selection.bg_fill = Color32::from_rgb(35, 65, 96);
    style.visuals.selection.stroke = Stroke::new(1.0_f32, ACCENT);
    style.visuals.widgets.noninteractive.bg_stroke = Stroke::new(1.0_f32, LINE);
    style.visuals.widgets.noninteractive.fg_stroke = Stroke::new(1.0_f32, TEXT);
    for widgets in [
        &mut style.visuals.widgets.inactive,
        &mut style.visuals.widgets.hovered,
        &mut style.visuals.widgets.active,
    ] {
        widgets.corner_radius = 6.into();
        widgets.fg_stroke = Stroke::new(1.0_f32, TEXT);
    }
    style.visuals.widgets.inactive.weak_bg_fill = CARD;
    style.visuals.widgets.inactive.bg_stroke = Stroke::new(1.0_f32, LINE);
    style.visuals.widgets.hovered.weak_bg_fill = Color32::from_rgb(39, 55, 74);
    style.spacing.item_spacing = egui::vec2(10.0, 6.0);
    style.spacing.button_padding = egui::vec2(13.0, 8.0);
    style.spacing.interact_size.y = 34.0;
    style
        .text_styles
        .insert(egui::TextStyle::Body, FontId::proportional(15.0));
    style
        .text_styles
        .insert(egui::TextStyle::Button, FontId::proportional(14.0));
    style
        .text_styles
        .insert(egui::TextStyle::Small, FontId::proportional(12.0));
    style
        .text_styles
        .insert(egui::TextStyle::Heading, FontId::proportional(26.0));
    style
        .text_styles
        .insert(egui::TextStyle::Monospace, FontId::monospace(13.0));
    ctx.set_style(style);
}
pub fn card() -> egui::Frame {
    egui::Frame::new()
        .fill(CARD)
        .stroke(Stroke::new(1.0_f32, LINE))
        .corner_radius(10)
        .inner_margin(16)
}
pub fn eyebrow(ui: &mut egui::Ui, text: &str) {
    ui.label(RichText::new(text).size(11.0).color(MUTED).strong());
}
pub fn muted(ui: &mut egui::Ui, text: impl Into<String>) {
    ui.label(RichText::new(text).color(MUTED));
}
pub fn badge(ui: &mut egui::Ui, text: &str, color: Color32) {
    egui::Frame::new()
        .fill(color.gamma_multiply(0.12))
        .corner_radius(4)
        .inner_margin(egui::Margin::symmetric(8, 4))
        .show(ui, |ui| {
            ui.label(RichText::new(text).size(12.0).color(color));
        });
}
pub fn nav(
    ui: &mut egui::Ui,
    label: &str,
    active: bool,
    count: Option<usize>,
    icon: usize,
) -> egui::Response {
    let (rect, response) =
        ui.allocate_exact_size(egui::vec2(ui.available_width(), 42.0), egui::Sense::click());
    if active || response.hovered() {
        ui.painter().rect_filled(
            rect,
            6,
            if active {
                Color32::from_rgb(32, 52, 75)
            } else {
                CARD
            },
        );
    }
    let color = if active { ACCENT } else { MUTED };
    if active {
        ui.painter().rect_filled(
            egui::Rect::from_min_size(rect.min + egui::vec2(0.0, 11.0), egui::vec2(3.0, 20.0)),
            2,
            ACCENT,
        );
    }
    let origin = rect.min + egui::vec2(16.0, 13.0);
    let p = ui.painter();
    if icon == 0 {
        for x in 0..2 {
            for y in 0..2 {
                p.rect_stroke(
                    egui::Rect::from_min_size(
                        origin + egui::vec2(x as f32 * 9.0, y as f32 * 9.0),
                        egui::vec2(6.0, 6.0),
                    ),
                    1,
                    Stroke::new(1.3_f32, color),
                    egui::StrokeKind::Inside,
                );
            }
        }
    } else {
        let r = egui::Rect::from_min_size(origin, egui::vec2(16.0, 16.0));
        p.rect_stroke(r, 3, Stroke::new(1.3_f32, color), egui::StrokeKind::Inside);
        for y in [5.0, 10.0] {
            p.line_segment(
                [origin + egui::vec2(4.0, y), origin + egui::vec2(12.0, y)],
                Stroke::new(1.0_f32, color),
            );
        }
    }
    p.text(
        rect.min + egui::vec2(44.0, 21.0),
        egui::Align2::LEFT_CENTER,
        label,
        FontId::proportional(14.0),
        if active { TEXT } else { MUTED },
    );
    if let Some(count) = count {
        p.text(
            rect.right_center() - egui::vec2(12.0, 0.0),
            egui::Align2::RIGHT_CENTER,
            count,
            FontId::proportional(12.0),
            MUTED,
        );
    }
    response.widget_info(|| {
        egui::WidgetInfo::selected(
            egui::WidgetType::SelectableLabel,
            ui.is_enabled(),
            active,
            label,
        )
    });
    response
}
pub fn facts(
    ui: &mut egui::Ui,
    rows: impl IntoIterator<Item = (impl AsRef<str>, impl AsRef<str>)>,
) {
    let width = ui.available_width();
    for (label, value) in rows {
        let label = label.as_ref();
        let value = value.as_ref();
        ui.horizontal_top(|ui| {
            ui.allocate_ui_with_layout(
                egui::vec2((width * 0.35).min(180.0), 20.0),
                egui::Layout::left_to_right(egui::Align::Min),
                |ui| {
                    ui.set_min_width((width * 0.35).min(180.0));
                    ui.label(RichText::new(label).color(MUTED));
                },
            );
            let response = ui.add(
                egui::Label::new(RichText::new(value).color(if value == "Not reported" {
                    MUTED
                } else {
                    TEXT
                }))
                .wrap()
                .selectable(true),
            );
            response.context_menu(|ui| {
                if ui.button("Copy value").clicked() {
                    ui.ctx().copy_text(value.to_owned());
                    ui.close();
                }
            });
        });
    }
}
