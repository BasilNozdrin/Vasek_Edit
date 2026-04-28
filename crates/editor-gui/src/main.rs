//! Entry point for vasek-edit GUI.

mod app;

use std::path::PathBuf;

fn main() -> anyhow::Result<()> {
    let path = std::env::args().nth(1).map(PathBuf::from);

    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_title("vasek-edit")
            .with_inner_size([1280.0, 800.0])
            .with_min_inner_size([400.0, 300.0]),
        ..Default::default()
    };

    eframe::run_native(
        "vasek-edit",
        options,
        Box::new(move |cc| Ok(Box::new(app::GuiApp::new(cc, path.as_deref())))),
    )
    .map_err(|e| anyhow::anyhow!("{e}"))?;

    Ok(())
}
