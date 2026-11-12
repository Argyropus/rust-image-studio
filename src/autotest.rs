//! Opt-in self-check used during development: drive one generation and save window
//! screenshots, then exit. Inert unless `QIS_SHOT_DIR` is set.
//!
//!   QIS_SHOT_DIR=<dir>      where to write idle.png / progress.png / done.png
//!   QIS_AUTOPROMPT=<text>   prompt to generate once the engine is ready (optional)

use egui::{Context, ScreenshotCallback, ViewportCommand};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Instant;

pub struct AutoTest {
    dir: PathBuf,
    pub prompt: Option<String>,
    start: Instant,
    submitted: bool,
    initial_count: Option<usize>,
    done_at: Option<Instant>,
    requested: Vec<&'static str>,
    taken: Arc<Mutex<Vec<&'static str>>>,
    /// Engine log lines already written to engine_log.txt.
    pub logged: usize,
    /// Set by the app while background work (the first-run downloads) should finish before
    /// the "idle" screenshot.
    pub hold: bool,
}

impl AutoTest {
    pub fn from_env() -> Option<Self> {
        let dir = PathBuf::from(std::env::var_os("QIS_SHOT_DIR")?);
        let _ = std::fs::create_dir_all(&dir);
        Some(Self {
            dir,
            prompt: std::env::var("QIS_AUTOPROMPT").ok().filter(|p| !p.trim().is_empty()),
            start: Instant::now(),
            submitted: false,
            initial_count: None,
            done_at: None,
            requested: Vec::new(),
            taken: Arc::default(),
            logged: 0,
            hold: false,
        })
    }

    fn has(&self, name: &str) -> bool {
        self.taken.lock().unwrap().contains(&name)
    }

    pub fn wants_generation(&mut self, ready: bool) -> bool {
        if ready && self.prompt.is_some() && !self.submitted && self.has("idle") {
            self.submitted = true;
            return true;
        }
        false
    }

    pub fn shoot(&mut self, ctx: &Context, name: &'static str) {
        if self.requested.contains(&name) {
            return;
        }
        self.requested.push(name);
        let path = self.dir.join(format!("{name}.png"));
        let taken = self.taken.clone();
        let repaint = ctx.clone();
        ctx.send_viewport_cmd(ViewportCommand::Screenshot(ScreenshotCallback::new(move |img| {
            let _ = image::save_buffer(
                &path,
                img.as_raw(),
                img.width() as u32,
                img.height() as u32,
                image::ExtendedColorType::Rgba8,
            );
            taken.lock().unwrap().push(name);
            repaint.request_repaint();
        })));
    }

    pub fn tick(&mut self, ctx: &Context, step: Option<u32>, history_len: usize, idle: bool) {
        ctx.request_repaint_after(std::time::Duration::from_millis(200));
        let initial = *self.initial_count.get_or_insert(history_len);

        if self.start.elapsed().as_secs_f32() > 3.0 && !self.hold {
            self.shoot(ctx, "idle");
        }
        if self.submitted && step.is_some_and(|s| s >= 3) {
            self.shoot(ctx, "progress");
        }
        if self.submitted && history_len > initial && idle {
            let t = *self.done_at.get_or_insert_with(Instant::now);
            if t.elapsed().as_secs_f32() > 1.5 {
                self.shoot(ctx, "done");
            }
        }

        let finished = if self.prompt.is_some() { self.has("done") } else { self.has("idle") };
        if finished || self.start.elapsed().as_secs() > 600 {
            ctx.send_viewport_cmd(ViewportCommand::Close);
        }
    }
}
