mod app;
mod audio;
mod config;
mod library;
mod vinyl;

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_title("Orpheus")
            .with_inner_size([480.0, 560.0])
            .with_min_inner_size([360.0, 320.0]),
        ..Default::default()
    };
    eframe::run_native(
        "Orpheus",
        options,
        Box::new(|cc| Ok(Box::new(app::App::new(&cc.egui_ctx)))),
    )
}
