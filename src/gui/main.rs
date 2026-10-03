#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]

mod app;
mod theme;

fn main() -> eframe::Result {
    eframe::run_native(
        "Video Cue Engine",
        eframe::NativeOptions {
            viewport: eframe::egui::ViewportBuilder::default()
                .with_inner_size([900.0, 760.0])
                .with_min_inner_size([640.0, 580.0]),
            ..Default::default()
        },
        Box::new(|cc| {
            theme::install(&cc.egui_ctx);
            Ok(Box::new(app::BatchApp::new()))
        }),
    )
}
