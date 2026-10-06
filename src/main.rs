#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod decoder;
mod payload;
mod video;
mod zbar;

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default().with_inner_size([1_350.0, 840.0]),
        ..Default::default()
    };
    eframe::run_native(
        "QR Video Extractor",
        options,
        Box::new(|cc| Ok(Box::new(app::QrExtractorApp::new(cc)))),
    )
}
