//! Visual design: palette, fonts, style and a few custom widgets.

use egui::{
    Color32, CornerRadius, FontData, FontDefinitions, FontFamily, FontId, Frame, Margin, Response,
    RichText, Sense, Stroke, TextStyle, Ui, Vec2, vec2,
};
use std::sync::Arc;

pub const BG: Color32 = Color32::from_rgb(14, 15, 19);
pub const PANEL: Color32 = Color32::from_rgb(20, 21, 27);
pub const CARD: Color32 = Color32::from_rgb(27, 29, 37);
pub const INPUT: Color32 = Color32::from_rgb(17, 18, 23);
pub const WIDGET: Color32 = Color32::from_rgb(36, 38, 48);
pub const WIDGET_HOVER: Color32 = Color32::from_rgb(46, 49, 62);
pub const BORDER: Color32 = Color32::from_rgb(42, 45, 56);
pub const TEXT: Color32 = Color32::from_rgb(232, 233, 240);
pub const TEXT_DIM: Color32 = Color32::from_rgb(150, 154, 170);
pub const TEXT_FAINT: Color32 = Color32::from_rgb(100, 104, 120);
pub const ACCENT: Color32 = Color32::from_rgb(124, 92, 255);
pub const ACCENT_HOVER: Color32 = Color32::from_rgb(143, 116, 255);
pub const ACCENT_SOFT: Color32 = Color32::from_rgb(52, 43, 96);
pub const GREEN: Color32 = Color32::from_rgb(74, 222, 128);
pub const AMBER: Color32 = Color32::from_rgb(251, 191, 36);
pub const RED: Color32 = Color32::from_rgb(248, 113, 113);

pub const BOLD: &str = "bold";

pub fn install(ctx: &egui::Context) {
    install_fonts(ctx);
    ctx.set_theme(egui::ThemePreference::Dark);

    let mut v = egui::Visuals::dark();
    v.panel_fill = PANEL;
    v.window_fill = CARD;
    v.window_stroke = Stroke::new(1.0, BORDER);
    v.window_corner_radius = CornerRadius::same(14);
    v.menu_corner_radius = CornerRadius::same(10);
    v.extreme_bg_color = INPUT;
    v.faint_bg_color = CARD;
    v.code_bg_color = INPUT;
    v.hyperlink_color = ACCENT_HOVER;
    v.selection.bg_fill = ACCENT.gamma_multiply(0.55);
    v.selection.stroke = Stroke::new(1.0, ACCENT_HOVER);
    v.override_text_color = None;

    let r = CornerRadius::same(8);
    let w = &mut v.widgets;
    w.noninteractive.bg_fill = CARD;
    w.noninteractive.weak_bg_fill = CARD;
    w.noninteractive.bg_stroke = Stroke::new(1.0, BORDER);
    w.noninteractive.fg_stroke = Stroke::new(1.0, TEXT);
    w.noninteractive.corner_radius = r;
    w.inactive.bg_fill = WIDGET;
    w.inactive.weak_bg_fill = WIDGET;
    w.inactive.bg_stroke = Stroke::NONE;
    w.inactive.fg_stroke = Stroke::new(1.0, TEXT);
    w.inactive.corner_radius = r;
    w.hovered.bg_fill = WIDGET_HOVER;
    w.hovered.weak_bg_fill = WIDGET_HOVER;
    w.hovered.bg_stroke = Stroke::new(1.0, Color32::from_rgb(70, 74, 92));
    w.hovered.fg_stroke = Stroke::new(1.5, Color32::WHITE);
    w.hovered.corner_radius = r;
    w.active.bg_fill = ACCENT_SOFT;
    w.active.weak_bg_fill = ACCENT_SOFT;
    w.active.bg_stroke = Stroke::new(1.0, ACCENT);
    w.active.fg_stroke = Stroke::new(1.5, Color32::WHITE);
    w.active.corner_radius = r;
    w.open = w.active;

    ctx.set_visuals_of(egui::Theme::Dark, v);
    ctx.all_styles_mut(|s| {
        s.spacing.item_spacing = vec2(8.0, 8.0);
        s.spacing.button_padding = vec2(12.0, 6.0);
        s.spacing.interact_size.y = 30.0;
        s.spacing.slider_width = 200.0;
        s.spacing.scroll.bar_width = 8.0;
        s.text_styles = [
            (TextStyle::Small, FontId::proportional(12.0)),
            (TextStyle::Body, FontId::proportional(14.5)),
            (TextStyle::Button, FontId::proportional(14.0)),
            (TextStyle::Heading, FontId::new(20.0, FontFamily::Name(BOLD.into()))),
            (TextStyle::Monospace, FontId::monospace(12.5)),
        ]
        .into();
    });
}

/// A Korean UI font: (file bytes, face index inside a .ttc collection).
type FontFile = (Vec<u8>, u32);

/// Nanum Gothic (SIL OFL 1.1, `assets/fonts/OFL.txt`), built into the binary: covers all
/// 11,172 modern Hangul syllables, so Korean always renders even on a system without any
/// Korean font (minimal Linux installs, WSL, non-Korean Windows without Malgun Gothic).
const NANUM_REGULAR: &[u8] = include_bytes!("../assets/fonts/NanumGothic-Regular.ttf");
const NANUM_BOLD: &[u8] = include_bytes!("../assets/fonts/NanumGothic-Bold.ttf");

fn builtin_korean_fonts() -> (FontFile, FontFile) {
    ((NANUM_REGULAR.to_vec(), 0), (NANUM_BOLD.to_vec(), 0))
}

/// Windows: Malgun Gothic from the font folder (the system UI font), else the built-in font.
#[cfg(windows)]
fn korean_fonts() -> (Option<FontFile>, Option<FontFile>) {
    let dir = std::env::var("WINDIR").unwrap_or_else(|_| "C:\\Windows".into());
    let load = |name: &str| std::fs::read(format!("{dir}\\Fonts\\{name}")).ok().map(|b| (b, 0));
    match (load("malgun.ttf"), load("malgunbd.ttf")) {
        (Some(r), b) => (Some(r), b),
        _ => {
            let (r, b) = builtin_korean_fonts();
            (Some(r), Some(b))
        }
    }
}

/// Linux and others: always the built-in font. System Korean fonts differ by distro and
/// are often missing (WSL, minimal installs), so one font makes the UI look the same everywhere.
#[cfg(not(windows))]
fn korean_fonts() -> (Option<FontFile>, Option<FontFile>) {
    let (r, b) = builtin_korean_fonts();
    (Some(r), Some(b))
}

fn font_data((bytes, index): FontFile) -> Arc<FontData> {
    let mut d = FontData::from_owned(bytes);
    d.index = index;
    Arc::new(d)
}

fn install_fonts(ctx: &egui::Context) {
    let mut fonts = FontDefinitions::default();
    let (regular, bold) = korean_fonts();

    let mut bold_family = Vec::new();
    if let Some(f) = bold {
        fonts.font_data.insert("malgun-bold".into(), font_data(f));
        bold_family.push("malgun-bold".to_owned());
    }
    if let Some(f) = regular {
        fonts.font_data.insert("malgun".into(), font_data(f));
        fonts.families.entry(FontFamily::Proportional).or_default().insert(0, "malgun".into());
        fonts.families.entry(FontFamily::Monospace).or_default().push("malgun".into());
        bold_family.push("malgun".to_owned());
    }
    // Fall back to the default fonts (incl. emoji/icons) for anything the Korean font lacks.
    bold_family.extend(fonts.families[&FontFamily::Proportional].iter().cloned());
    fonts.families.insert(FontFamily::Name(BOLD.into()), bold_family);
    ctx.set_fonts(fonts);
}

pub fn bold(text: impl Into<String>, size: f32) -> RichText {
    RichText::new(text).family(FontFamily::Name(BOLD.into())).size(size)
}

pub fn card() -> Frame {
    Frame::new()
        .fill(CARD)
        .stroke(Stroke::new(1.0, BORDER))
        .corner_radius(12)
        .inner_margin(Margin::same(14))
}

pub fn section(ui: &mut Ui, title: &str) {
    ui.add_space(2.0);
    ui.label(bold(title, 13.0).color(TEXT_DIM));
}

/// Primary call-to-action button.
pub fn primary_button(ui: &mut Ui, text: &str, height: f32, enabled: bool) -> Response {
    let width = ui.available_width();
    let (rect, resp) = ui.allocate_exact_size(vec2(width, height), if enabled { Sense::click() } else { Sense::hover() });
    if ui.is_rect_visible(rect) {
        let fill = if !enabled {
            WIDGET
        } else if resp.is_pointer_button_down_on() {
            ACCENT
        } else if resp.hovered() {
            ACCENT_HOVER
        } else {
            ACCENT
        };
        let p = ui.painter();
        if enabled {
            p.rect_filled(rect.expand(2.0), 12, ACCENT.gamma_multiply(0.18));
        }
        p.rect_filled(rect, 10, fill);
        p.text(
            rect.center(),
            egui::Align2::CENTER_CENTER,
            text,
            FontId::new(16.0, FontFamily::Name(BOLD.into())),
            if enabled { Color32::WHITE } else { TEXT_FAINT },
        );
    }
    if enabled { resp.on_hover_cursor(egui::CursorIcon::PointingHand) } else { resp }
}

/// A row of equally-sized toggle buttons; returns true if the selection changed.
pub fn segmented(ui: &mut Ui, selected: &mut usize, labels: &[&str]) -> bool {
    segmented_impl(ui, selected, labels, None)
}

/// `segmented` with a hover tooltip per segment.
pub fn segmented_tips(ui: &mut Ui, selected: &mut usize, labels: &[&str], tips: &[&str]) -> bool {
    segmented_impl(ui, selected, labels, Some(tips))
}

fn segmented_impl(ui: &mut Ui, selected: &mut usize, labels: &[&str], tips: Option<&[&str]>) -> bool {
    let mut changed = false;
    let width = ui.available_width();
    let h = 32.0;
    let (rect, _) = ui.allocate_exact_size(vec2(width, h), Sense::hover());
    ui.painter().rect_filled(rect, 9, INPUT);
    let seg_w = (width - 4.0) / labels.len() as f32;
    for (i, label) in labels.iter().enumerate() {
        let r = egui::Rect::from_min_size(rect.min + vec2(2.0 + seg_w * i as f32, 2.0), vec2(seg_w, h - 4.0));
        let mut resp = ui.interact(r, ui.scope_id().with(("seg", label, i)), Sense::click());
        if let Some(tip) = tips.and_then(|t| t.get(i)) {
            resp = resp.on_hover_text(*tip);
        }
        let on = *selected == i;
        if on {
            ui.painter().rect_filled(r, 7, ACCENT_SOFT);
            ui.painter().rect_stroke(r, 7, Stroke::new(1.0, ACCENT), egui::StrokeKind::Inside);
        } else if resp.hovered() {
            ui.painter().rect_filled(r, 7, WIDGET);
        }
        ui.painter().text(
            r.center(),
            egui::Align2::CENTER_CENTER,
            *label,
            FontId::proportional(13.5),
            if on { Color32::WHITE } else { TEXT_DIM },
        );
        if resp.clicked() && !on {
            *selected = i;
            changed = true;
        }
    }
    changed
}

/// Tile showing an aspect-ratio rectangle icon with its label.
pub fn aspect_tile(ui: &mut Ui, on: bool, label: &str, hint: &str, w: f32, h: f32, size: Vec2) -> Response {
    let (rect, resp) = ui.allocate_exact_size(size, Sense::click());
    if ui.is_rect_visible(rect) {
        let p = ui.painter();
        let hot = resp.hovered();
        let fill = if on { ACCENT_SOFT } else if hot { WIDGET_HOVER } else { INPUT };
        p.rect_filled(rect, 9, fill);
        if on {
            p.rect_stroke(rect, 9, Stroke::new(1.0, ACCENT), egui::StrokeKind::Inside);
        }
        // The shape at its true proportions, inside a fixed 26 px box (filled softly so the
        // area difference between shapes reads too).
        let max = 26.0;
        let (iw, ih) = if w >= h { (max, max * h / w) } else { (max * w / h, max) };
        let icon = egui::Rect::from_center_size(egui::pos2(rect.center().x, rect.top() + 22.0), vec2(iw, ih));
        let ink = if on { Color32::WHITE } else if hot { TEXT } else { TEXT_DIM };
        p.rect_filled(icon, 3, ink.gamma_multiply(0.14));
        p.rect_stroke(icon, 3, Stroke::new(1.5, ink), egui::StrokeKind::Middle);
        p.text(
            egui::pos2(rect.center().x, rect.top() + 47.0),
            egui::Align2::CENTER_CENTER,
            label,
            FontId::new(12.5, FontFamily::Name(BOLD.into())),
            if on { Color32::WHITE } else { TEXT },
        );
        p.text(
            egui::pos2(rect.center().x, rect.top() + 62.0),
            egui::Align2::CENTER_CENTER,
            hint,
            FontId::proportional(10.5),
            if on { ACCENT_HOVER } else { TEXT_FAINT },
        );
    }
    resp.on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// Colored dot + text status pill.
pub fn status_pill(ui: &mut Ui, color: Color32, text: &str, pulse: bool) -> Response {
    Frame::new()
        .fill(color.gamma_multiply(0.12))
        .stroke(Stroke::new(1.0, color.gamma_multiply(0.45)))
        .corner_radius(20)
        .inner_margin(Margin::symmetric(12, 5))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 7.0;
                let (r, _) = ui.allocate_exact_size(vec2(8.0, 8.0), Sense::hover());
                let a = if pulse {
                    let t = ui.input(|i| i.time) as f32;
                    ui.ctx().request_repaint();
                    0.45 + 0.55 * (t * 3.0).sin().abs()
                } else {
                    1.0
                };
                ui.painter().circle_filled(r.center(), 4.0, color.gamma_multiply(a));
                ui.label(RichText::new(text).size(13.0).color(TEXT));
            });
        })
        .response
        .interact(Sense::click())
}

/// Card whose body folds away under a clickable title row; the row also shows a one-line
/// summary of the settings inside, so they can stay folded. Open/closed is remembered.
pub fn fold_card(ui: &mut Ui, key: &str, title: &str, summary: &str, default_open: bool, add: impl FnOnce(&mut Ui)) {
    let id = ui.make_persistent_id(("fold_card", key));
    let mut state = egui::collapsing_header::CollapsingState::load_with_default_open(ui.ctx(), id, default_open);
    // Full width of the column, measured outside the card (inside, the available width
    // can include the scroll bar's gutter and pushed cards past the right margin).
    let visible_right = ui.clip_rect().right() - 16.0;
    let outer_w = (visible_right - ui.cursor().left()).min(ui.available_width());
    let inner_w = (outer_w - 28.0).max(100.0);
    card().show(ui, |ui| {
        ui.set_min_width(inner_w);
        ui.set_max_width(inner_w);
        let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 22.0), Sense::click());
        let hot = resp.hovered();
        let open = state.is_open();
        let p = ui.painter();
        // Chevron: ▸ closed, ▾ open.
        let c = egui::pos2(rect.left() + 5.0, rect.center().y);
        let pts = if open {
            vec![c + vec2(-4.0, -2.0), c + vec2(0.0, 2.5), c + vec2(4.0, -2.0)]
        } else {
            vec![c + vec2(-1.5, -4.0), c + vec2(2.5, 0.0), c + vec2(-1.5, 4.0)]
        };
        p.add(egui::Shape::line(pts, Stroke::new(1.6, if hot { TEXT } else { TEXT_DIM })));
        p.text(
            egui::pos2(rect.left() + 18.0, rect.center().y),
            egui::Align2::LEFT_CENTER,
            title,
            FontId::new(13.0, FontFamily::Name(BOLD.into())),
            if hot { TEXT } else { TEXT_DIM },
        );
        if !summary.is_empty() {
            p.text(
                egui::pos2(rect.right(), rect.center().y),
                egui::Align2::RIGHT_CENTER,
                summary,
                FontId::proportional(12.0),
                if open { TEXT_FAINT } else { ACCENT_HOVER },
            );
        }
        if resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
            state.toggle(ui);
        }
        if state.is_open() {
            ui.add_space(8.0);
            add(ui);
        }
    });
    state.store(ui.ctx());
}

/// iOS-style on/off switch; returns the response (clicked = toggled).
pub fn switch(ui: &mut Ui, on: &mut bool) -> Response {
    let size = vec2(38.0, 22.0);
    let (rect, mut resp) = ui.allocate_exact_size(size, Sense::click());
    if resp.clicked() {
        *on = !*on;
        resp.mark_changed();
    }
    if ui.is_rect_visible(rect) {
        let t = ui.ctx().animate_bool_responsive(resp.id, *on);
        let track = Color32::from_rgb(
            (WIDGET.r() as f32 + (ACCENT.r() as f32 - WIDGET.r() as f32) * t) as u8,
            (WIDGET.g() as f32 + (ACCENT.g() as f32 - WIDGET.g() as f32) * t) as u8,
            (WIDGET.b() as f32 + (ACCENT.b() as f32 - WIDGET.b() as f32) * t) as u8,
        );
        ui.painter().rect_filled(rect, 11, track);
        if !*on {
            ui.painter().rect_stroke(rect, 11, Stroke::new(1.0, BORDER), egui::StrokeKind::Inside);
        }
        let x = egui::lerp((rect.left() + 11.0)..=(rect.right() - 11.0), t);
        ui.painter().circle_filled(egui::pos2(x, rect.center().y), 8.0, Color32::WHITE);
    }
    resp.on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// Line icons drawn with the painter, so every toolbar glyph shares one stroke style
/// (emoji glyphs come in mixed colours and sizes).
#[derive(Clone, Copy, PartialEq)]
pub enum Icon {
    Reload,
    Expand,
    Sparkle,
    Image,
    Brush,
    Zoom,
    Copy,
    Save,
    Folder,
    Trash,
    Gear,
    Log,
    Help,
    GridSmall,
    GridLarge,
}

/// Paints `icon` in a 16×16 design grid scaled to `size`, centred on `c`.
pub fn paint_icon(p: &egui::Painter, c: egui::Pos2, size: f32, icon: Icon, color: Color32) {
    let k = size / 16.0;
    let pt = |x: f32, y: f32| c + vec2(x, y) * k;
    let st = Stroke::new(1.5 * k.max(1.0), color);
    let line = |pts: &[(f32, f32)]| {
        p.add(egui::Shape::line(pts.iter().map(|&(x, y)| pt(x, y)).collect(), st));
    };
    let closed = |pts: &[(f32, f32)]| {
        p.add(egui::Shape::closed_line(pts.iter().map(|&(x, y)| pt(x, y)).collect(), st));
    };
    match icon {
        Icon::Reload => {
            let r = 5.5;
            let (a0, a1) = (-50f32.to_radians(), 230f32.to_radians());
            let arc: Vec<(f32, f32)> = (0..=24)
                .map(|i| {
                    let a = a0 + (a1 - a0) * i as f32 / 24.0;
                    (r * a.cos(), r * a.sin())
                })
                .collect();
            line(&arc);
            // Chevron arrowhead at the start of the arc, pointing against the sweep.
            let (s, cth) = (a0.sin(), a0.cos());
            let (tx, ty) = (-s, cth); // sweep tangent
            let (px, py) = (r * cth, r * s);
            line(&[
                (px + 3.0 * tx + 3.0 * cth, py + 3.0 * ty + 3.0 * s),
                (px, py),
                (px + 3.0 * tx - 3.0 * cth, py + 3.0 * ty - 3.0 * s),
            ]);
        }
        Icon::Expand => {
            for (sx, sy) in [(-1.0, -1.0), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)] {
                line(&[(7.0 * sx, 2.5 * sy), (7.0 * sx, 7.0 * sy), (2.5 * sx, 7.0 * sy)]);
            }
            p.rect_stroke(
                egui::Rect::from_min_max(pt(-3.0, -3.0), pt(3.0, 3.0)),
                1.5 * k,
                st,
                egui::StrokeKind::Middle,
            );
        }
        Icon::Sparkle => {
            let star = |cx: f32, cy: f32, r: f32| {
                let pts: Vec<(f32, f32)> = (0..8)
                    .map(|i| {
                        let a = i as f32 * std::f32::consts::FRAC_PI_4 - std::f32::consts::FRAC_PI_2;
                        let rr = if i % 2 == 0 { r } else { r * 0.36 };
                        (cx + rr * a.cos(), cy + rr * a.sin())
                    })
                    .collect();
                closed(&pts);
            };
            star(-1.5, 1.5, 6.0);
            p.circle_filled(pt(5.0, -5.0), 1.3 * k, color);
            p.circle_filled(pt(5.5, 3.5), 0.9 * k, color);
        }
        Icon::Image => {
            p.rect_stroke(
                egui::Rect::from_min_max(pt(-7.0, -6.0), pt(7.0, 6.0)),
                2.0 * k,
                st,
                egui::StrokeKind::Middle,
            );
            line(&[(-7.0, 4.0), (-2.5, -0.5), (1.5, 3.5), (3.5, 1.5), (7.0, 5.0)]);
            p.circle_filled(pt(3.0, -2.5), 1.4 * k, color);
        }
        Icon::Brush => {
            p.line_segment([pt(6.5, -6.5), pt(-0.5, 0.5)], Stroke::new(2.0 * k.max(1.0), color));
            p.circle_filled(pt(-3.0, 3.0), 3.0 * k, color);
        }
        Icon::Zoom => {
            p.circle_stroke(pt(-1.5, -1.5), 5.0 * k, st);
            p.line_segment([pt(2.3, 2.3), pt(6.5, 6.5)], Stroke::new(2.0 * k.max(1.0), color));
        }
        Icon::Copy => {
            p.rect_stroke(
                egui::Rect::from_min_max(pt(-3.0, -3.0), pt(7.0, 7.0)),
                2.0 * k,
                st,
                egui::StrokeKind::Middle,
            );
            line(&[(-3.5, 3.0), (-7.0, 3.0), (-7.0, -7.0), (3.0, -7.0), (3.0, -3.5)]);
        }
        Icon::Save => {
            line(&[(0.0, -7.0), (0.0, 3.0)]);
            line(&[(-4.0, -1.0), (0.0, 3.0), (4.0, -1.0)]);
            line(&[(-7.0, 2.0), (-7.0, 7.0), (7.0, 7.0), (7.0, 2.0)]);
        }
        Icon::Folder => {
            closed(&[(-7.0, -5.5), (-2.0, -5.5), (0.0, -3.5), (7.0, -3.5), (7.0, 6.0), (-7.0, 6.0)]);
        }
        Icon::Trash => {
            line(&[(-7.0, -4.5), (7.0, -4.5)]);
            line(&[(-2.5, -4.5), (-2.5, -7.0), (2.5, -7.0), (2.5, -4.5)]);
            line(&[(-5.5, -4.5), (-4.5, 7.0), (4.5, 7.0), (5.5, -4.5)]);
            line(&[(-1.5, -1.0), (-1.5, 4.0)]);
            line(&[(1.5, -1.0), (1.5, 4.0)]);
        }
        Icon::Gear => {
            p.circle_stroke(pt(0.0, 0.0), 4.2 * k, st);
            for i in 0..8 {
                let a = i as f32 * std::f32::consts::FRAC_PI_4;
                let (s, c2) = a.sin_cos();
                p.line_segment([pt(5.2 * c2, 5.2 * s), pt(7.4 * c2, 7.4 * s)], Stroke::new(2.0 * k.max(1.0), color));
            }
        }
        Icon::Log => {
            p.rect_stroke(
                egui::Rect::from_min_max(pt(-6.0, -7.0), pt(6.0, 7.0)),
                2.0 * k,
                st,
                egui::StrokeKind::Middle,
            );
            for y in [-3.0, 0.0, 3.0] {
                line(&[(-3.0, y), (3.0, y)]);
            }
        }
        Icon::Help => {
            p.circle_stroke(pt(0.0, 0.0), 7.0 * k, st);
            p.text(pt(0.0, 0.3), egui::Align2::CENTER_CENTER, "?", FontId::proportional(11.0 * k), color);
        }
        Icon::GridSmall => {
            for (x, y) in [(-6.0, -6.0), (-1.0, -6.0), (4.0, -6.0), (-6.0, -1.0), (-1.0, -1.0), (4.0, -1.0), (-6.0, 4.0), (-1.0, 4.0), (4.0, 4.0)] {
                p.rect_filled(egui::Rect::from_min_size(pt(x, y), vec2(3.0, 3.0) * k), 0.5, color);
            }
        }
        Icon::GridLarge => {
            for (x, y) in [(-6.5, -6.5), (0.5, -6.5), (-6.5, 0.5), (0.5, 0.5)] {
                p.rect_filled(egui::Rect::from_min_size(pt(x, y), vec2(6.0, 6.0) * k), 1.0, color);
            }
        }
    }
}

#[derive(Clone, Copy, PartialEq)]
pub enum BarKind {
    Normal,
    /// Opens a menu: draws a small chevron after the label.
    Menu,
    /// Destructive: turns red on hover.
    Danger,
}

/// Flat toolbar button: line icon plus optional label. Transparent until hovered.
pub fn bar_button(ui: &mut Ui, icon: Icon, label: Option<&str>, tip: &str, kind: BarKind) -> Response {
    let h = 30.0;
    let font = FontId::proportional(13.5);
    let galley = label.map(|t| ui.painter().layout_no_wrap(t.to_owned(), font.clone(), TEXT));
    let chevron = kind == BarKind::Menu;
    let w = match &galley {
        Some(g) => 10.0 + 15.0 + 6.0 + g.size().x + if chevron { 13.0 } else { 0.0 } + 11.0,
        None => h + if chevron { 10.0 } else { 0.0 },
    };
    let (rect, resp) = ui.allocate_exact_size(vec2(w, h), Sense::click());
    if ui.is_rect_visible(rect) {
        let p = ui.painter();
        let hot = resp.hovered() || resp.is_pointer_button_down_on();
        let danger = kind == BarKind::Danger && hot;
        if resp.is_pointer_button_down_on() {
            p.rect_filled(rect, 7, if danger { RED.gamma_multiply(0.22) } else { WIDGET });
        } else if hot {
            p.rect_filled(rect, 7, if danger { RED.gamma_multiply(0.14) } else { WIDGET_HOVER });
        }
        let icon_c = if danger { RED } else if hot { TEXT } else { TEXT_DIM };
        let text_c = if danger { RED } else if hot { Color32::WHITE } else { TEXT };
        let mut x = rect.left();
        match &galley {
            Some(g) => {
                paint_icon(p, egui::pos2(x + 10.0 + 7.5, rect.center().y), 15.0, icon, icon_c);
                x += 10.0 + 15.0 + 6.0;
                p.galley_with_override_text_color(egui::pos2(x, rect.center().y - g.size().y / 2.0), g.clone(), text_c);
                x += g.size().x + 5.0;
            }
            None => {
                paint_icon(p, egui::pos2(x + h / 2.0, rect.center().y), 15.0, icon, icon_c);
                x += h - 6.0;
            }
        }
        if chevron {
            let cy = rect.center().y;
            let s = Stroke::new(1.4, TEXT_FAINT);
            p.add(egui::Shape::line(
                vec![egui::pos2(x + 1.0, cy - 1.5), egui::pos2(x + 4.0, cy + 1.5), egui::pos2(x + 7.0, cy - 1.5)],
                s,
            ));
        }
    }
    resp.on_hover_cursor(egui::CursorIcon::PointingHand).on_hover_text(tip)
}

/// Rounded tray that groups toolbar buttons.
pub fn bar_group<R>(ui: &mut Ui, add: impl FnOnce(&mut Ui) -> R) -> R {
    Frame::new()
        .fill(INPUT)
        .stroke(Stroke::new(1.0, BORDER))
        .corner_radius(10)
        .inner_margin(Margin::same(3))
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing.x = 2.0;
            add(ui)
        })
        .inner
}

/// Compact "한 | EN" language switch for the top bar. Returns true when it changed.
pub fn lang_switch(ui: &mut Ui, english: &mut bool) -> bool {
    let mut changed = false;
    bar_group(ui, |ui| {
        // Keep "한" on the left even inside a right-to-left toolbar.
        let mut pair = [(false, "한", "한국어"), (true, "EN", "English")];
        if ui.layout().prefer_right_to_left() {
            pair.reverse();
        }
        {
            for (is_en, label, tip) in pair {
                let (rect, resp) = ui.allocate_exact_size(vec2(34.0, 30.0), Sense::click());
                let on = *english == is_en;
                let p = ui.painter();
                if on {
                    p.rect_filled(rect, 7, ACCENT_SOFT);
                    p.rect_stroke(rect, 7, Stroke::new(1.0, ACCENT), egui::StrokeKind::Inside);
                } else if resp.hovered() {
                    p.rect_filled(rect, 7, WIDGET_HOVER);
                }
                p.text(
                    rect.center(),
                    egui::Align2::CENTER_CENTER,
                    label,
                    FontId::proportional(13.0),
                    if on { Color32::WHITE } else { TEXT_DIM },
                );
                if resp.on_hover_cursor(egui::CursorIcon::PointingHand).on_hover_text(tip).clicked() && !on {
                    *english = is_en;
                    changed = true;
                }
            }
        }
    });
    changed
}

/// Thin vertical separator between toolbar buttons.
pub fn bar_divider(ui: &mut Ui) {
    let (rect, _) = ui.allocate_exact_size(vec2(9.0, 30.0), Sense::hover());
    ui.painter().vline(rect.center().x, rect.center().y - 8.0..=rect.center().y + 8.0, Stroke::new(1.0, BORDER));
}

/// Small ↺ button that restores a recommended value. It only shows when the value differs,
/// but always keeps its slot so the row doesn't shift. Returns true when clicked.
pub fn reset_button(ui: &mut Ui, differs: bool, tip: &str) -> bool {
    let size = vec2(24.0, 24.0);
    if !differs {
        ui.allocate_exact_size(size, Sense::hover());
        return false;
    }
    let (rect, resp) = ui.allocate_exact_size(size, Sense::click());
    let hot = resp.hovered();
    if hot {
        ui.painter().rect_filled(rect, 6, WIDGET_HOVER);
    }
    paint_icon(ui.painter(), rect.center(), 13.0, Icon::Reload, if hot { TEXT } else { ACCENT_HOVER });
    resp.on_hover_cursor(egui::CursorIcon::PointingHand).on_hover_text(tip).clicked()
}

/// Subtle icon-style button used in toolbars.
pub fn tool_button(ui: &mut Ui, text: &str, tip: &str) -> Response {
    ui.add(egui::Button::new(RichText::new(text).size(13.5)).corner_radius(8))
        .on_hover_text(tip)
}
