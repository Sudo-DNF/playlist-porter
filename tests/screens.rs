//! Render every screen headlessly.
//!
//! - `screens_run_without_panic` runs in the normal test suite (no GPU).
//! - `render_screenshots` is ignored by default. It renders with wgpu
//!   (software rendering works) and writes PNGs:
//!   `cargo test --test screens -- --ignored`
//!   Output goes to `target/screens/`, or to `docs/screenshots/` when
//!   `PP_SCREENSHOT_DIR=docs/screenshots` is set.

mod support;

use egui_kittest::Harness;
use playlist_porter::gui::app::draw;
use playlist_porter::gui::state::UiState;

type Fixture = (&'static str, fn() -> UiState);

fn fixtures() -> Vec<Fixture> {
    vec![
        ("setup", support::setup),
        ("signin", support::signin_idle),
        ("signin_waiting", support::signin_waiting),
        ("signin_error", support::signin_error),
        ("main", support::main),
        ("main_empty_filter", support::main_filtered_empty),
        ("detail", support::detail),
        ("export", support::export_form),
        ("export_overwrite", support::export_overwrite),
        ("export_toast", support::export_done_toast),
    ]
}

fn harness(
    state: UiState,
    theme: egui::Theme,
    size: egui::Vec2,
    wgpu: bool,
) -> Harness<'static, UiState> {
    let mut b = Harness::builder()
        .with_size(size)
        .with_theme(theme)
        .with_context_setup(playlist_porter::gui::widgets::setup_fonts);
    if wgpu {
        b = b.wgpu();
    }
    b.build_ui_state(
        |ui, s: &mut UiState| {
            let mut out = Vec::new();
            draw(ui, s, &mut out);
        },
        state,
    )
}

#[test]
fn screens_run_without_panic() {
    for (name, make) in fixtures() {
        for theme in [egui::Theme::Light, egui::Theme::Dark] {
            let mut h = harness(make(), theme, egui::vec2(800.0, 600.0), false);
            h.run_steps(4);
            let _ = name;
        }
    }
}

#[test]
#[ignore = "renders with wgpu; run explicitly to regenerate screenshots"]
fn render_screenshots() {
    let dir = std::env::var("PP_SCREENSHOT_DIR").unwrap_or_else(|_| "target/screens".into());
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(dir);
    std::fs::create_dir_all(&dir).unwrap();
    let sizes: Vec<(&str, egui::Vec2)> = match std::env::var("PP_SCREENSHOT_SIZE").as_deref() {
        Ok("large") => vec![("", egui::vec2(1100.0, 720.0))],
        _ => vec![("", egui::vec2(800.0, 600.0))],
    };
    let only = std::env::var("PP_ONLY").ok();
    for (name, make) in fixtures() {
        if only.as_deref().is_some_and(|o| !name.contains(o)) {
            continue;
        }
        for (theme, tname) in [(egui::Theme::Light, "light"), (egui::Theme::Dark, "dark")] {
            for (suffix, size) in &sizes {
                let mut h = harness(make(), theme, *size, true);
                h.run_steps(6);
                let img = h.render().expect("render");
                img.save(dir.join(format!("{name}_{tname}{suffix}.png")))
                    .unwrap();
            }
        }
    }
}
