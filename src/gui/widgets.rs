//! Small reusable widgets and helpers.

use egui::{
    Align, Button, Color32, IconData, Layout, Pos2, Rect, Response, RichText, Stroke, Ui,
    UiBuilder, Vec2,
};

use super::state::UiError;
use crate::core::error::README_SETUP_URL;

/// Accent used for the brand mark. A plain blue, deliberately unlike any
/// streaming service's branding.
pub const BRAND_BLUE: Color32 = Color32::from_rgb(0x4a, 0x90, 0xd9);

/// Window icon: an open box with an arrow leaving it, drawn procedurally so no
/// image assets or decoders are needed.
pub fn app_icon() -> IconData {
    const N: u32 = 64;
    let mut rgba = vec![0u8; (N * N * 4) as usize];
    let mut put = |x: i32, y: i32| {
        if (0..N as i32).contains(&x) && (0..N as i32).contains(&y) {
            let i = ((y as u32 * N + x as u32) * 4) as usize;
            rgba[i..i + 4].copy_from_slice(&[0x4a, 0x90, 0xd9, 0xff]);
        }
    };
    // Stroke thickness. The box is a left edge plus top and bottom stubs.
    let t = 5;
    for y in 8..56 {
        for dx in 0..t {
            put(8 + dx, y);
        }
    }
    for x in 8..30 {
        for dy in 0..t {
            put(x, 8 + dy);
            put(x, 51 + dy);
        }
    }
    for y in 8..20 {
        for dx in 0..t {
            put(26 + dx, y);
        }
    }
    for y in 44..56 {
        for dx in 0..t {
            put(26 + dx, y);
        }
    }
    // Arrow shaft.
    for x in 24..54 {
        for dy in 0..t {
            put(x, 30 + dy);
        }
    }
    // Arrow head.
    for i in 0..14 {
        for d in 0..t {
            put(54 - i + d - 2, 32 - i);
            put(54 - i + d - 2, 32 + i);
        }
    }
    IconData {
        rgba,
        width: N,
        height: N,
    }
}

/// Paint the brand mark (box + arrow) into a square of the given size.
pub fn brand_mark(ui: &mut egui::Ui, size: f32) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(Vec2::splat(size), egui::Sense::hover());
    let p = ui.painter();
    let s = size / 24.0;
    let at = |x: f32, y: f32| Pos2::new(rect.left() + x * s, rect.top() + y * s);
    let stroke = Stroke::new(2.0 * s.max(0.75), BRAND_BLUE);
    p.line_segment([at(9.0, 6.0), at(4.0, 6.0)], stroke);
    p.line_segment([at(4.0, 6.0), at(4.0, 18.0)], stroke);
    p.line_segment([at(4.0, 18.0), at(9.0, 18.0)], stroke);
    p.line_segment([at(9.0, 6.0), at(9.0, 9.0)], stroke);
    p.line_segment([at(9.0, 15.0), at(9.0, 18.0)], stroke);
    p.line_segment([at(9.0, 12.0), at(21.0, 12.0)], stroke);
    p.line_segment([at(17.0, 8.0), at(21.0, 12.0)], stroke);
    p.line_segment([at(17.0, 16.0), at(21.0, 12.0)], stroke);
    resp
}

// ---------------------------------------------------------------------------
// Colours that work in both the light and the dark theme. Status is never
// shown by colour alone: callers always pair these with an icon and text.

pub fn ok_color(ui: &Ui) -> Color32 {
    if ui.visuals().dark_mode {
        Color32::from_rgb(0x7c, 0xc8, 0x8a)
    } else {
        Color32::from_rgb(0x1e, 0x7a, 0x34)
    }
}

pub fn warn_color(ui: &Ui) -> Color32 {
    if ui.visuals().dark_mode {
        ui.visuals().warn_fg_color
    } else {
        Color32::from_rgb(0x9a, 0x5b, 0x00)
    }
}

pub fn err_color(ui: &Ui) -> Color32 {
    ui.visuals().error_fg_color
}

pub fn muted(ui: &Ui) -> Color32 {
    ui.visuals().weak_text_color()
}

// ---------------------------------------------------------------------------

/// A bordered card with comfortable padding.
pub fn card<R>(ui: &mut Ui, add: impl FnOnce(&mut Ui) -> R) -> R {
    egui::Frame::group(ui.style())
        .inner_margin(16)
        .corner_radius(6)
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            add(ui)
        })
        .inner
}

/// A tinted callout box (info / warning).
pub fn callout<R>(ui: &mut Ui, tint: Color32, add: impl FnOnce(&mut Ui) -> R) -> R {
    egui::Frame::new()
        .fill(tint.gamma_multiply(0.12))
        .stroke(Stroke::new(1.0, tint.gamma_multiply(0.6)))
        .inner_margin(12)
        .corner_radius(6)
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            add(ui)
        })
        .inner
}

/// Lay out contents in a centred column at most `max_w` wide.
pub fn centered<R>(ui: &mut Ui, max_w: f32, add: impl FnOnce(&mut Ui) -> R) -> R {
    let avail = ui.available_rect_before_wrap();
    let w = avail.width().min(max_w);
    let left = avail.center().x - w / 2.0;
    let rect = Rect::from_min_size(Pos2::new(left, avail.top()), Vec2::new(w, avail.height()));
    ui.scope_builder(
        UiBuilder::new()
            .max_rect(rect)
            .layout(Layout::top_down(Align::Min)),
        |ui| {
            ui.set_width(w);
            add(ui)
        },
    )
    .inner
}

pub fn heading(ui: &mut Ui, text: impl Into<String>) {
    ui.label(RichText::new(text).size(22.0).strong());
}

pub fn section_title(ui: &mut Ui, text: impl Into<String>) {
    ui.label(RichText::new(text).size(16.0).strong());
}

/// The main call-to-action button (filled with the selection colour).
pub fn primary_button(ui: &Ui, text: impl Into<String>) -> Button<'static> {
    let v = ui.visuals();
    Button::new(RichText::new(text.into()).color(v.selection.stroke.color))
        .fill(v.selection.bg_fill)
        .min_size(Vec2::new(0.0, 30.0))
}

pub fn danger_button(text: impl Into<String>) -> Button<'static> {
    Button::new(RichText::new(text.into()).color(Color32::WHITE))
        .fill(Color32::from_rgb(0x9b, 0x2c, 0x2c))
        .min_size(Vec2::new(0.0, 30.0))
}

pub fn button(text: impl Into<String>) -> Button<'static> {
    Button::new(text.into()).min_size(Vec2::new(0.0, 30.0))
}

/// A link that opens in the system browser, marked as external.
pub fn ext_link(ui: &mut Ui, text: &str, url: &str) -> Response {
    ui.hyperlink_to(format!("{text} ⤴"), url)
}

/// A small outlined badge, e.g. "Collaborative". Painted directly so it
/// keeps its own height inside table cells.
pub fn badge(ui: &mut Ui, text: impl Into<String>, color: Color32) -> Response {
    let font = egui::FontId::proportional(12.5);
    let galley = ui.painter().layout_no_wrap(text.into(), font, color);
    let pad = Vec2::new(6.0, 2.0);
    let (rect, resp) = ui.allocate_exact_size(galley.size() + 2.0 * pad, egui::Sense::hover());
    if ui.is_rect_visible(rect) {
        let p = ui.painter();
        p.rect(
            rect,
            4.0,
            color.gamma_multiply(0.08),
            Stroke::new(1.0, color.gamma_multiply(0.7)),
            egui::StrokeKind::Inside,
        );
        p.galley(rect.min + pad, galley, color);
    }
    resp
}

/// Error panel with message, hint and optional README link. Returns true
/// when the user dismisses it.
pub fn error_panel(ui: &mut Ui, err: &UiError) -> bool {
    let color = err_color(ui);
    let mut dismissed = false;
    callout(ui, color, |ui| {
        ui.horizontal_top(|ui| {
            ui.label(RichText::new("⚠").size(18.0).color(color));
            ui.vertical(|ui| {
                ui.set_width((ui.available_width() - 30.0).max(100.0));
                ui.label(RichText::new(&err.message).color(ui.visuals().strong_text_color()));
                if let Some(h) = &err.hint {
                    ui.label(RichText::new(h).color(muted(ui)));
                }
                if err.setup_link {
                    ext_link(ui, "README: setup steps", README_SETUP_URL);
                }
            });
            ui.with_layout(Layout::right_to_left(Align::Min), |ui| {
                if ui.small_button("✖").on_hover_text("Dismiss").clicked() {
                    dismissed = true;
                }
            });
        });
    });
    dismissed
}

/// Two-column key/value row used in cards and the confirmation modal. The
/// closure gets the key label's id so inputs can be `labelled_by` it.
pub fn kv_row(ui: &mut Ui, key_w: f32, key: &str, add: impl FnOnce(&mut Ui, egui::Id)) {
    ui.horizontal_top(|ui| {
        let label = ui
            .allocate_ui_with_layout(Vec2::new(key_w, 20.0), Layout::top_down(Align::Min), |ui| {
                ui.set_width(key_w);
                ui.label(RichText::new(key).color(muted(ui)))
            })
            .inner;
        ui.vertical(|ui| add(ui, label.id));
    });
}

/// Use the bundled Hack font as a last-resort fallback for proportional text
/// (it covers arrows such as "→"), so the app and the headless tests render
/// the same glyphs without depending on system fonts.
pub fn setup_fonts(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();
    if let Some(fam) = fonts.families.get_mut(&egui::FontFamily::Proportional) {
        if !fam.iter().any(|f| f == "Hack") {
            fam.push("Hack".to_owned());
        }
    }
    ctx.set_fonts(fonts);
    // Slightly larger text than egui's defaults, in both themes.
    ctx.all_styles_mut(|style| {
        use egui::{FontFamily, FontId, TextStyle};
        style.text_styles = [
            (
                TextStyle::Small,
                FontId::new(12.0, FontFamily::Proportional),
            ),
            (TextStyle::Body, FontId::new(14.5, FontFamily::Proportional)),
            (
                TextStyle::Button,
                FontId::new(14.5, FontFamily::Proportional),
            ),
            (
                TextStyle::Heading,
                FontId::new(22.0, FontFamily::Proportional),
            ),
            (
                TextStyle::Monospace,
                FontId::new(13.0, FontFamily::Monospace),
            ),
        ]
        .into();
        style.spacing.button_padding = egui::vec2(10.0, 5.0);
        style.spacing.item_spacing = egui::vec2(8.0, 6.0);
    });
}

/// Monospace, selectable, read-only text in a sunken box (URLs, paths).
pub fn code_box(ui: &mut Ui, text: &str, rows: usize) {
    egui::Frame::new()
        .fill(ui.visuals().extreme_bg_color)
        .stroke(ui.visuals().widgets.noninteractive.bg_stroke)
        .inner_margin(8)
        .corner_radius(4)
        .show(ui, |ui| {
            let mut t = text;
            ui.add(
                egui::TextEdit::multiline(&mut t)
                    .font(egui::TextStyle::Monospace)
                    .frame(egui::Frame::NONE)
                    .desired_rows(rows)
                    .desired_width(f32::INFINITY),
            );
        });
}

/// Width of `text` in the given text style (for sizing table columns).
pub fn text_width(ui: &Ui, text: &str, style: egui::TextStyle) -> f32 {
    let font = style.resolve(ui.style());
    ui.painter()
        .layout_no_wrap(text.to_owned(), font, Color32::WHITE)
        .size()
        .x
}

/// Width of a default button with this label.
pub fn button_width(ui: &Ui, text: &str) -> f32 {
    // Plus a little slack: egui adds frame/atom spacing around the label.
    text_width(ui, text, egui::TextStyle::Button) + 2.0 * ui.spacing().button_padding.x + 8.0
}

/// Denser text and buttons inside tables.
pub fn table_style(ui: &mut Ui) {
    use egui::{FontFamily, FontId, TextStyle};
    let st = ui.style_mut();
    st.text_styles
        .insert(TextStyle::Body, FontId::new(13.5, FontFamily::Proportional));
    st.text_styles.insert(
        TextStyle::Button,
        FontId::new(13.5, FontFamily::Proportional),
    );
    st.spacing.button_padding = egui::vec2(7.0, 3.0);
    st.spacing.item_spacing.x = 6.0;
}

/// A fixed-width, left-aligned, wrapping text cell (for grids).
pub fn text_cell(ui: &mut Ui, width: f32, text: impl Into<egui::WidgetText>) {
    ui.allocate_ui_with_layout(Vec2::new(width, 0.0), Layout::top_down(Align::Min), |ui| {
        ui.set_width(width);
        ui.add(egui::Label::new(text).wrap());
    });
}

/// Show a path with the home directory abbreviated to `~`.
pub fn tilde(path: &std::path::Path) -> String {
    if let Some(home) = dirs::home_dir() {
        if let Ok(rest) = path.strip_prefix(&home) {
            return format!("~/{}", rest.display());
        }
    }
    path.display().to_string()
}
