use eframe::egui::{self, Color32, FontData, FontDefinitions, FontFamily, Frame, Stroke};

pub const BACKGROUND: Color32 = Color32::from_rgb(13, 17, 23);
pub const PANEL: Color32 = Color32::from_rgb(23, 29, 38);
pub const BORDER: Color32 = Color32::from_rgb(47, 58, 72);
pub const MUTED: Color32 = Color32::from_rgb(143, 158, 179);
pub const TEXT: Color32 = Color32::from_rgb(229, 236, 245);
pub const ACCENT: Color32 = Color32::from_rgb(81, 224, 196);
pub const RED: Color32 = Color32::from_rgb(255, 100, 119);

pub fn install(context: &egui::Context) {
    context.set_theme(egui::ThemePreference::Dark);
    let mut style = (*context.style()).clone();
    style.visuals = egui::Visuals::dark();
    style.visuals.override_text_color = Some(TEXT);
    style.visuals.panel_fill = BACKGROUND;
    style.visuals.window_fill = PANEL;
    style.visuals.extreme_bg_color = BACKGROUND;
    style.visuals.selection.bg_fill = Color32::from_rgb(29, 78, 76);
    style.visuals.selection.stroke = Stroke::new(1.0_f32, ACCENT);
    style.visuals.widgets.inactive.weak_bg_fill = Color32::from_rgb(34, 44, 56);
    style.visuals.widgets.hovered.bg_stroke = Stroke::new(1.0_f32, ACCENT);
    style.spacing.item_spacing = egui::vec2(10.0, 10.0);
    style.spacing.button_padding = egui::vec2(16.0, 9.0);
    style
        .text_styles
        .insert(egui::TextStyle::Body, egui::FontId::proportional(15.0));
    style
        .text_styles
        .insert(egui::TextStyle::Button, egui::FontId::proportional(15.0));
    style
        .text_styles
        .insert(egui::TextStyle::Small, egui::FontId::proportional(12.0));
    context.set_style(style);

    // Use an installed Japanese font; no proprietary font is bundled in the executable.
    let mut paths = Vec::new();
    if let Some(windows) = std::env::var_os("WINDIR") {
        paths.push(std::path::PathBuf::from(windows).join("Fonts/meiryo.ttc"));
    }
    paths.extend([
        "/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc".into(),
        "/System/Library/Fonts/ヒラギノ角ゴシック W3.ttc".into(),
    ]);
    for path in paths {
        if let Ok(bytes) = std::fs::read(path) {
            let mut fonts = FontDefinitions::default();
            fonts
                .font_data
                .insert("japanese".into(), FontData::from_owned(bytes).into());
            for family in [FontFamily::Proportional, FontFamily::Monospace] {
                fonts
                    .families
                    .entry(family)
                    .or_default()
                    .push("japanese".into());
            }
            context.set_fonts(fonts);
            break;
        }
    }
}

pub fn panel() -> Frame {
    Frame::new()
        .fill(PANEL)
        .stroke(Stroke::new(1.0_f32, BORDER))
        .corner_radius(10)
        .inner_margin(18)
}
