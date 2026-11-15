//! Small reusable widgets and helpers.

use egui::{Color32, IconData, Pos2, Rect, Stroke, Vec2};

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
    let _ = Rect::NOTHING;
    resp
}
