use eframe::egui::{self, Color32, FontId, RichText, Stroke};
#[derive(Clone, Copy)]
pub struct Palette {
    pub bg: Color32,
    pub side: Color32,
    pub card: Color32,
    pub line: Color32,
    pub text: Color32,
    pub muted: Color32,
    pub accent: Color32,
    pub green: Color32,
    pub selected: Color32,
    pub hover: Color32,
    pub error: Color32,
}
pub fn palette(ctx: &egui::Context) -> Palette {
    if ctx.style().visuals.dark_mode {
        Palette {
            bg: Color32::from_rgb(16, 21, 29),
            side: Color32::from_rgb(21, 28, 38),
            card: Color32::from_rgb(26, 35, 47),
            line: Color32::from_rgb(47, 60, 77),
            text: Color32::from_rgb(231, 237, 245),
            muted: Color32::from_rgb(166, 180, 199),
            accent: Color32::from_rgb(115, 185, 255),
            green: Color32::from_rgb(115, 209, 175),
            selected: Color32::from_rgb(32, 52, 75),
            hover: Color32::from_rgb(39, 55, 74),
            error: Color32::from_rgb(53, 39, 37),
        }
    } else {
        Palette {
            bg: Color32::from_rgb(244, 246, 249),
            side: Color32::WHITE,
            card: Color32::WHITE,
            line: Color32::from_rgb(211, 219, 229),
            text: Color32::from_rgb(30, 41, 59),
            muted: Color32::from_rgb(82, 98, 119),
            accent: Color32::from_rgb(30, 83, 145),
            green: Color32::from_rgb(29, 112, 91),
            selected: Color32::from_rgb(227, 237, 249),
            hover: Color32::from_rgb(235, 240, 246),
            error: Color32::from_rgb(255, 238, 235),
        }
    }
}
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
    apply(ctx, false);
}
pub fn apply(ctx: &egui::Context, dark: bool) {
    ctx.set_visuals(if dark {
        egui::Visuals::dark()
    } else {
        egui::Visuals::light()
    });
    let p = palette(ctx);
    let mut style = (*ctx.style()).clone();
    style.visuals.override_text_color = Some(p.text);
    style.visuals.panel_fill = p.bg;
    style.visuals.window_fill = p.card;
    style.visuals.extreme_bg_color = p.side;
    style.visuals.faint_bg_color = p.side;
    style.visuals.selection.bg_fill = p.selected;
    style.visuals.selection.stroke = Stroke::new(1.0_f32, p.accent);
    style.visuals.widgets.noninteractive.bg_stroke = Stroke::new(1.0_f32, p.line);
    style.visuals.widgets.noninteractive.fg_stroke = Stroke::new(1.0_f32, p.text);
    for widgets in [
        &mut style.visuals.widgets.inactive,
        &mut style.visuals.widgets.hovered,
        &mut style.visuals.widgets.active,
    ] {
        widgets.corner_radius = 6.into();
        widgets.fg_stroke = Stroke::new(1.0_f32, p.text);
    }
    style.visuals.widgets.inactive.weak_bg_fill = p.card;
    style.visuals.widgets.inactive.bg_stroke = Stroke::new(1.0_f32, p.line);
    style.visuals.widgets.hovered.weak_bg_fill = p.hover;
    style.visuals.widgets.active.weak_bg_fill = p.selected;
    style.visuals.hyperlink_color = p.accent;
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
pub fn card(ui: &egui::Ui) -> egui::Frame {
    let p = palette(ui.ctx());
    egui::Frame::new()
        .fill(p.card)
        .stroke(Stroke::new(1.0_f32, p.line))
        .corner_radius(10)
        .inner_margin(16)
}
pub fn eyebrow(ui: &mut egui::Ui, text: &str) {
    let p = palette(ui.ctx());
    ui.label(RichText::new(text).size(11.0).color(p.muted).strong());
}
pub fn muted(ui: &mut egui::Ui, text: impl Into<String>) {
    let p = palette(ui.ctx());
    ui.label(RichText::new(text).color(p.muted));
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
    let p = palette(ui.ctx());
    let (rect, response) =
        ui.allocate_exact_size(egui::vec2(ui.available_width(), 42.0), egui::Sense::click());
    if active || response.hovered() {
        ui.painter()
            .rect_filled(rect, 6, if active { p.selected } else { p.card });
    }
    let color = if active { p.accent } else { p.muted };
    if active {
        ui.painter().rect_filled(
            egui::Rect::from_min_size(rect.min + egui::vec2(0.0, 11.0), egui::vec2(3.0, 20.0)),
            2,
            p.accent,
        );
    }
    let origin = rect.min + egui::vec2(16.0, 13.0);
    let painter = ui.painter();
    if icon == 0 {
        for x in 0..2 {
            for y in 0..2 {
                painter.rect_stroke(
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
        painter.rect_stroke(r, 3, Stroke::new(1.3_f32, color), egui::StrokeKind::Inside);
        for y in [5.0, 10.0] {
            painter.line_segment(
                [origin + egui::vec2(4.0, y), origin + egui::vec2(12.0, y)],
                Stroke::new(1.0_f32, color),
            );
        }
    }
    painter.text(
        rect.min + egui::vec2(44.0, 21.0),
        egui::Align2::LEFT_CENTER,
        label,
        FontId::proportional(14.0),
        if active { p.text } else { p.muted },
    );
    if let Some(count) = count {
        painter.text(
            rect.right_center() - egui::vec2(12.0, 0.0),
            egui::Align2::RIGHT_CENTER,
            count,
            FontId::proportional(12.0),
            p.muted,
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
    let p = palette(ui.ctx());
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
                    ui.label(RichText::new(label).color(p.muted));
                },
            );
            let response = ui.add(
                egui::Label::new(RichText::new(value).color(if value == "Not reported" {
                    p.muted
                } else {
                    p.text
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
