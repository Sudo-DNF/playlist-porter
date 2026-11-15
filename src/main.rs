//! Entry point: builds the tokio runtime that the app owns for its whole life,
//! then opens the window. GUI only: there is no command-line interface.

use anyhow::Context as _;
use playlist_porter::gui::app::PorterApp;

fn main() -> anyhow::Result<()> {
    // Multi-thread runtime owned by the app; kept alive until the window closes.
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .thread_name("porter-worker")
        .build()
        .context("couldn't start the background runtime")?;

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("playlist-porter")
            .with_app_id("playlist-porter")
            .with_inner_size([1100.0, 760.0])
            .with_min_inner_size([800.0, 600.0])
            .with_icon(playlist_porter::gui::widgets::app_icon()),
        ..Default::default()
    };

    let handle = runtime.handle().clone();
    eframe::run_native(
        "playlist-porter",
        options,
        Box::new(move |cc| Ok(Box::new(PorterApp::new(cc, handle)))),
    )
    .map_err(|e| anyhow::anyhow!("the window couldn't be opened: {e}"))?;

    // Dropping the runtime here cancels any in-flight work; in-memory
    // Spotify/YouTube data goes with the app.
    runtime.shutdown_timeout(std::time::Duration::from_secs(2));
    Ok(())
}
