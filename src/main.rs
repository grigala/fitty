#[cfg(target_arch = "wasm32")]
fn main() {} // The wasm runner is defined in lib.rs

#[cfg(not(target_arch = "wasm32"))]
fn main() -> eframe::Result<()> {
    let opts = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Fitty")
            .with_inner_size([1480.0, 1020.0])
            .with_drag_and_drop(true),
        ..Default::default()
    };
    eframe::run_native(
        "FIT File Analyzer",
        opts,
        Box::new(|cc| Ok(Box::new(fitty::app::FittyApp::new(cc)))),
    )
}