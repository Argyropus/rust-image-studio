//! Brush mask editor for partial edits (inpainting). White = repaint.

use crate::theme::*;
use egui::{Color32, ColorImage, Pos2, Rect, Sense, Stroke, TextureHandle, TextureOptions, Ui, Vec2};

/// Longest side of the mask buffer; it is scaled to the output size when sent.
const MAX_SIDE: u32 = 1024;

pub struct MaskEditor {
    pub w: u32,
    pub h: u32,
    pub data: Vec<u8>,
    pub brush: f32,
    pub erase: bool,
    tex: Option<TextureHandle>,
    /// Whole overlay must be re-uploaded (new/cleared/inverted mask).
    dirty_all: bool,
    /// Pixel box touched by brush strokes since the last upload: x0, y0, x1, y1 (inclusive).
    dirty_box: Option<[u32; 4]>,
    last: Option<Pos2>,
}

/// Overlay colour for a mask value (translucent red where the mask is set).
fn overlay_px(v: u8) -> Color32 {
    Color32::from_rgba_unmultiplied(255, 60, 80, (v as u16 * 140 / 255) as u8)
}

impl MaskEditor {
    pub fn new(img_w: u32, img_h: u32) -> Self {
        let s = (MAX_SIDE as f32 / img_w.max(img_h) as f32).min(1.0);
        let w = ((img_w as f32 * s).round() as u32).max(1);
        let h = ((img_h as f32 * s).round() as u32).max(1);
        Self {
            w,
            h,
            data: vec![0; (w * h) as usize],
            brush: 60.0,
            erase: false,
            tex: None,
            dirty_all: true,
            dirty_box: None,
            last: None,
        }
    }

    pub fn is_empty(&self) -> bool {
        !self.data.iter().any(|v| *v > 0)
    }

    pub fn clear(&mut self) {
        self.data.fill(0);
        self.dirty_all = true;
    }

    pub fn invert(&mut self) {
        for v in &mut self.data {
            *v = 255 - *v;
        }
        self.dirty_all = true;
    }

    /// Mask as an image, softened a little so the repainted area blends at the edges.
    pub fn to_gray(&self) -> image::GrayImage {
        let img = image::GrayImage::from_raw(self.w, self.h, self.data.clone()).expect("mask size");
        image::imageops::blur(&img, (self.w.max(self.h) as f32 / 256.0).max(1.5))
    }

    fn stamp(&mut self, c: Pos2, r: f32, value: u8) {
        let (x0, x1) = ((c.x - r).floor().max(0.0) as i32, (c.x + r).ceil().min(self.w as f32 - 1.0) as i32);
        let (y0, y1) = ((c.y - r).floor().max(0.0) as i32, (c.y + r).ceil().min(self.h as f32 - 1.0) as i32);
        if x0 > x1 || y0 > y1 {
            return; // brush entirely outside the mask
        }
        let r2 = r * r;
        for y in y0..=y1 {
            for x in x0..=x1 {
                let (dx, dy) = (x as f32 + 0.5 - c.x, y as f32 + 0.5 - c.y);
                if dx * dx + dy * dy <= r2 {
                    self.data[(y as u32 * self.w + x as u32) as usize] = value;
                }
            }
        }
        let (x0, y0, x1, y1) = (x0 as u32, y0 as u32, x1 as u32, y1 as u32);
        self.dirty_box = Some(match self.dirty_box {
            Some([a, b, c, d]) => [a.min(x0), b.min(y0), c.max(x1), d.max(y1)],
            None => [x0, y0, x1, y1],
        });
    }

    /// Paints with the left button (right button or eraser mode erases) over `rect`,
    /// where the source image is displayed, and draws the red overlay.
    pub fn ui(&mut self, ui: &mut Ui, rect: Rect) {
        let resp = ui.interact(rect, ui.scope_id().with("mask_canvas"), Sense::click_and_drag());
        let scale = self.w as f32 / rect.width();
        let to_mask = |p: Pos2| Pos2::new((p.x - rect.left()) * scale, (p.y - rect.top()) * scale);

        let (primary, secondary) = ui.input(|i| (i.pointer.primary_down(), i.pointer.secondary_down()));
        let painting = (resp.dragged() || resp.is_pointer_button_down_on()) && (primary || secondary);
        if painting {
            if let Some(p) = ui.input(|i| i.pointer.interact_pos()) {
                let value = if self.erase || secondary { 0 } else { 255 };
                let cur = to_mask(p);
                let r = self.brush / 2.0;
                // Interpolate between frames so fast strokes stay continuous.
                let from = self.last.unwrap_or(cur);
                let steps = ((cur - from).length() / (r * 0.4).max(1.0)).ceil().max(1.0) as i32;
                for s in 0..=steps {
                    let t = s as f32 / steps as f32;
                    self.stamp(from + (cur - from) * t, r, value);
                }
                self.last = Some(cur);
            }
        } else {
            self.last = None;
        }

        // Upload only what changed: a brush stroke touches a small box, so re-sending the
        // whole (up to 1024²) overlay every frame was the main cost while painting.
        if self.dirty_all || self.tex.is_none() {
            let pixels = self.data.iter().map(|v| overlay_px(*v)).collect();
            let img = ColorImage::new([self.w as usize, self.h as usize], pixels);
            match &mut self.tex {
                Some(t) => t.set(img, TextureOptions::LINEAR),
                None => self.tex = Some(ui.ctx().load_texture("mask_overlay", img, TextureOptions::LINEAR)),
            }
            self.dirty_all = false;
            self.dirty_box = None;
        } else if let (Some([x0, y0, x1, y1]), Some(t)) = (self.dirty_box.take(), self.tex.as_mut()) {
            let (bw, bh) = ((x1 - x0 + 1) as usize, (y1 - y0 + 1) as usize);
            let mut pixels = Vec::with_capacity(bw * bh);
            for y in y0..=y1 {
                let row = (y * self.w) as usize;
                pixels.extend(self.data[row + x0 as usize..=row + x1 as usize].iter().map(|v| overlay_px(*v)));
            }
            t.set_partial([x0 as usize, y0 as usize], ColorImage::new([bw, bh], pixels), TextureOptions::LINEAR);
        }
        if let Some(t) = &self.tex {
            ui.painter().image(t.id(), rect, Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)), Color32::WHITE);
        }

        // Brush cursor.
        if let Some(p) = resp.hover_pos() {
            let r = self.brush / 2.0 / scale;
            let col = if self.erase { TEXT_DIM } else { Color32::WHITE };
            ui.painter().circle_stroke(p, r, Stroke::new(1.5, Color32::from_black_alpha(160)));
            ui.painter().circle_stroke(p, r - 1.0, Stroke::new(1.0, col));
            ui.ctx().set_cursor_icon(egui::CursorIcon::Crosshair);
        }
        // Ctrl + mouse wheel (egui reports it as zoom) or [ / ] adjust the brush size.
        if resp.hovered() {
            let (zoom, smaller, bigger) =
                ui.input(|i| (i.zoom_delta(), i.key_pressed(egui::Key::OpenBracket), i.key_pressed(egui::Key::CloseBracket)));
            let mut b = self.brush * zoom;
            if smaller {
                b *= 0.8;
            }
            if bigger {
                b *= 1.25;
            }
            self.brush = b.clamp(4.0, 400.0);
        }
        let _ = Vec2::ZERO;
    }
}
