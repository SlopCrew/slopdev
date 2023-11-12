mod app;
use app::SlopDev;

fn main() -> anyhow::Result<()> {
    let native_options = eframe::NativeOptions {
        initial_window_size: Some([400.0, 100.0].into()),
        ..Default::default()
    };

    eframe::run_native(
        "slopdev",
        native_options,
        Box::new(|cc| Box::new(SlopDev::new(cc))),
    )
    .expect("failed to run eframe");

    Ok(())
}
