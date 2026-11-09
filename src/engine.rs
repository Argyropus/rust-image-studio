//! Background engine: owns the `sd-server` process (stable-diffusion.cpp), keeps the
//! model loaded in memory, and runs generation jobs through its async HTTP API.

use crate::config::{GenMode, GpuBackend, ModelVariant, Settings, StepMode, TURBO_SIGMAS};
use anyhow::{Context as _, Result, anyhow, bail};
use base64::Engine as _;
use egui::ColorImage;
use serde_json::{Value, json};
use std::collections::VecDeque;
use std::io::{BufRead, BufReader, Read};
use std::path::PathBuf;
use std::sync::Arc;
use std::process::{Child, Command, Stdio};
use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender, channel};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

#[derive(Clone)]
pub struct GenJob {
    pub id: u64,
    pub prompt: String,
    pub negative_prompt: String,
    pub width: u32,
    pub height: u32,
    pub seed: u64,
    pub cfg: f32,
    pub step_mode: StepMode,
    pub steps: u32,
    pub live_preview: bool,
    pub output_dir: PathBuf,
    pub mode: GenMode,
    pub strength: f32,
    /// Spectrum step cache (cache_mode=spectrum).
    pub fast: bool,
    /// Input images for image modes (first one is the img2img init image).
    pub inputs: Vec<Arc<image::RgbaImage>>,
    /// Partial-edit mask (white = repaint), in the first input image's aspect.
    pub mask: Option<Arc<image::GrayImage>>,
    /// Enabled LoRAs: file name in models/loras and weight.
    pub loras: Vec<(String, f32)>,
    /// Overrides the step schedule (few-step LoRAs).
    pub sigmas: Option<Vec<f32>>,
    /// Outpaint padding (left, right, top, bottom), used in GenMode::Outpaint.
    pub outpaint: crate::config::Pads,
    /// Outpaint with the padded canvas as an edit reference (needs the vision module):
    /// one pass, any size. Off = the old multi-pass inpaint fallback.
    pub guided: bool,
}

/// Instruction for reference-guided outpainting. The reference is the padded canvas with
/// the new area filled flat grey; the masked repaint keeps the middle pixel-aligned.
/// Tested against edge-stretch inpainting: no seams or cut-off tables, scene continues.
pub const OUTPAINT_GUIDE: &str = "회색으로 비어 있는 바깥 부분을 채워서 이 사진을 더 넓은 한 장의 사진으로 완성해줘. \
가운데 원래 장면은 그대로 두고, 배경·바닥·조명·원근·색감이 끊김 없이 자연스럽게 이어지게 해줘.";

pub struct GeneratedImage {
    pub path: PathBuf,
    pub image: ColorImage,
    pub thumb: ColorImage,
}

#[derive(Clone, PartialEq)]
pub enum EngineState {
    Stopped,
    /// Process launched; weights are being loaded.
    Loading { since: Instant },
    Ready,
    Error(String),
}

pub enum EngineEvent {
    State(EngineState),
    Log(String),
    JobStarted { id: u64 },
    /// `pass` = (current, total) for multi-pass outpainting.
    Progress { id: u64, step: u32, total: u32, preview: Option<ColorImage>, pass: Option<(u32, u32)> },
    /// Status line for a running job that restarts its progress (next outpaint pass, or a
    /// redraw), so the bar running full again isn't mistaken for "done".
    Note { id: u64, text: String },
    JobDone { job: GenJob, result: GeneratedImage, elapsed: f32 },
    JobFailed { id: u64, message: String },
    JobCancelled { id: u64 },
}

enum Cmd {
    Start(Settings),
    Stop,
    Enqueue(GenJob),
    Cancel(u64),
    Refresh,
    Shutdown,
}

/// UI-side handle.
pub struct Engine {
    tx: Sender<Cmd>,
    pub events: Receiver<EngineEvent>,
    thread: Option<JoinHandle<()>>,
}

impl Engine {
    pub fn spawn(ctx: egui::Context) -> Self {
        let (tx, cmd_rx) = channel();
        let (ev_tx, events) = channel();
        let thread = std::thread::Builder::new()
            .name("engine".into())
            .spawn(move || Worker::new(ctx, ev_tx).run(cmd_rx))
            .expect("spawn engine thread");
        Self { tx, events, thread: Some(thread) }
    }

    pub fn start(&self, settings: &Settings) {
        let _ = self.tx.send(Cmd::Start(settings.clone()));
    }
    pub fn stop(&self) {
        let _ = self.tx.send(Cmd::Stop);
    }
    pub fn enqueue(&self, job: GenJob) {
        let _ = self.tx.send(Cmd::Enqueue(job));
    }
    pub fn cancel(&self, id: u64) {
        let _ = self.tx.send(Cmd::Cancel(id));
    }
    /// Rescans model/LoRA files without reloading the model.
    pub fn refresh(&self) {
        let _ = self.tx.send(Cmd::Refresh);
    }

    pub fn shutdown(&mut self) {
        let _ = self.tx.send(Cmd::Shutdown);
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

impl Drop for Engine {
    fn drop(&mut self) {
        self.shutdown();
    }
}

struct Running {
    job: GenJob,
    remote_id: String,
    started: Instant,
    last_step: u32,
    cancel_sent: bool,
    attempt: u8,
    outpaint: Option<OutpaintRun>,
}

/// Multi-pass outpainting: each pass grows the previous result a little.
#[derive(Clone)]
struct OutpaintRun {
    passes: Vec<crate::config::Pads>,
    idx: usize,
    current: Arc<image::RgbaImage>,
    started: Instant,
    /// Re-draws of the current guided pass because its new area came back blank.
    retries: u8,
}

/// Redraws (with another seed) of a guided pass whose new area came back blank; some
/// seeds copy or smooth over the reference's grey placeholder instead of drawing.
const GUIDED_RETRIES: u8 = 2;

/// Re-noise strength for outpaint passes. 1.0 ignored the pre-filled colours and drew
/// unrelated scenes; 0.75 kept the layout from the edge-stretched fill.
const OUTPAINT_STRENGTH: f32 = 0.75;

struct Worker {
    ctx: egui::Context,
    events: Sender<EngineEvent>,
    agent: ureq::Agent,
    child: Option<Child>,
    base_url: String,
    state: EngineState,
    queue: VecDeque<GenJob>,
    running: Option<Running>,
    last_poll: Instant,
    warmup_enabled: bool,
    /// Remote id of the startup warm-up job while it runs.
    warmup: Option<(String, Instant)>,
}

impl Worker {
    fn new(ctx: egui::Context, events: Sender<EngineEvent>) -> Self {
        let agent: ureq::Agent = ureq::Agent::config_builder()
            .timeout_global(Some(Duration::from_secs(30)))
            .http_status_as_error(false)
            .proxy(None)
            .build()
            .into();
        Self {
            ctx,
            events,
            agent,
            child: None,
            base_url: String::new(),
            state: EngineState::Stopped,
            queue: VecDeque::new(),
            running: None,
            last_poll: Instant::now(),
            warmup_enabled: false,
            warmup: None,
        }
    }

    fn emit(&self, ev: EngineEvent) {
        let _ = self.events.send(ev);
        self.ctx.request_repaint();
    }

    fn set_state(&mut self, s: EngineState) {
        if self.state != s {
            self.state = s.clone();
            self.emit(EngineEvent::State(s));
        }
    }

    fn run(mut self, cmds: Receiver<Cmd>) {
        loop {
            match cmds.recv_timeout(Duration::from_millis(100)) {
                Ok(Cmd::Start(s)) => {
                    if let Err(e) = self.start(&s) {
                        self.set_state(EngineState::Error(format!("{e:#}")));
                    }
                }
                Ok(Cmd::Stop) => {
                    self.kill();
                    // A stop drops pending work too, so nothing runs unseen after a restart.
                    for job in std::mem::take(&mut self.queue) {
                        self.emit(EngineEvent::JobCancelled { id: job.id });
                    }
                    self.set_state(EngineState::Stopped);
                }
                Ok(Cmd::Enqueue(job)) => self.queue.push_back(job),
                Ok(Cmd::Cancel(id)) => self.cancel(id),
                Ok(Cmd::Refresh) => {
                    if !self.base_url.is_empty() {
                        let url = format!("{}/sdcpp/v1/models/refresh", self.base_url);
                        let _ = self.agent.post(&url).send_json(json!({}));
                    }
                }
                Ok(Cmd::Shutdown) | Err(RecvTimeoutError::Disconnected) => {
                    self.kill();
                    return;
                }
                Err(RecvTimeoutError::Timeout) => {}
            }
            self.tick();
        }
    }

    fn start(&mut self, s: &Settings) -> Result<()> {
        self.kill();
        let exe = s.server_exe();
        if !exe.is_file() {
            bail!("{}", tf!("{}를 찾을 수 없습니다: {}", "{} not found: {}", crate::config::SERVER_EXE, exe.display()));
        }
        for (name, p) in [
            (t!("이미지 생성 모델", "Image model"), &s.active_diffusion()),
            (t!("텍스트 인코더", "Text encoder"), &s.active_text_encoder()),
            ("VAE", &s.vae),
        ] {
            if !p.is_file() {
                bail!("{}", tf!("{} 파일이 없습니다: {}", "{} file is missing: {}", name, p.display()));
            }
        }

        let port = std::net::TcpListener::bind("127.0.0.1:0")?.local_addr()?.port();
        self.base_url = format!("http://127.0.0.1:{port}");

        let mut cmd = Command::new(&exe);
        cmd.current_dir(s.engine_dir())
            .arg("--listen-ip")
            .arg("127.0.0.1")
            .arg("--listen-port")
            .arg(port.to_string())
            .arg("--model-dir")
            .arg(s.models_dir())
            .arg("--diffusion-model")
            .arg(s.active_diffusion())
            .arg("--llm")
            .arg(s.active_text_encoder())
            .arg("--vae")
            .arg(&s.vae)
            .args(["--sigmas", "qwen-image-2.1-turbo", "--steps", "8"])
            .args(["--cfg-scale", "1.0", "--sampling-method", "euler"]);
        if s.vision_ok() {
            cmd.arg("--llm_vision").arg(&s.vision);
        }
        let lora_dir = s.lora_dir();
        let _ = std::fs::create_dir_all(&lora_dir);
        cmd.arg("--lora-model-dir").arg(&lora_dir);
        // Memory placement, pinned rather than left to sd.cpp's auto-fit (which depends on
        // the RAM free at startup and made every generation fail with ~7 GB free).
        // - All weights keep a RAM copy, so when VRAM runs short the diffusion model is
        //   restored by memcpy instead of re-read from disk (that cost ~6 s per image).
        // - A 6 GiB VRAM budget keeps graphs segmented instead of running out of memory.
        // - INT8 weights are 6.9 GB, so the text encoder streams from disk to fit 16 GB RAM.
        // - Under ~13 GB of RAM (8 GB PCs, WSL's default half-of-host limit) the Q4 set
        //   (10.2 GB in RAM) swapped: one 512² step took 76 s on a 7 GB WSL. The text encoder
        //   then streams from disk too: a new prompt costs a few seconds more, nothing else.
        //   The same happens when too little RAM is free right now (`weights_fit_in_ram`).
        let extra = split_args(&s.extra_args);
        let user_placement =
            extra.iter().any(|a| a == "--backend" || a == "--params-backend" || a == "--offload-to-cpu");
        // The AMD builds (ROCm, Vulkan) name their devices differently (ROCm0, Vulkan0),
        // so the device comes from the engine's own list; CUDA keeps its tested CUDA0.
        let backend = s.backend();
        // APU: the GPU's memory is system RAM, so a RAM copy of the weights plus the
        // `--max-vram` working set would hold them twice. Weights then live on the GPU
        // device itself, once.
        let unified = crate::config::unified_memory();
        if !user_placement {
            let device = if backend == GpuBackend::Cuda {
                "CUDA0".to_owned()
            } else {
                let list = list_devices(&exe, &s.engine_dir())?;
                for l in list.lines().filter(|l| !l.trim().is_empty()) {
                    self.emit(EngineEvent::Log(tf!("장치: {}", "Device: {}", l.replace('\t', " · ").trim_end())));
                }
                crate::config::pick_device(&list, backend).ok_or_else(|| {
                    anyhow!(
                        "{}",
                        tf!(
                            "엔진({})이 사용할 GPU를 찾지 못했습니다. 그래픽 드라이버를 최신으로 설치하거나, 설정 → GPU에서 다른 방식(예: Vulkan)을 고르세요.",
                            "The engine ({}) found no GPU to use. Install a current graphics driver, or pick another option (e.g. Vulkan) in Settings → GPU.",
                            backend.label()
                        )
                    )
                })?
            };
            cmd.arg("--backend").arg(format!("diffusion={device},te={device},vae={device}"));
            let size = |p: &std::path::Path| std::fs::metadata(p).map(|m| m.len()).unwrap_or(0);
            let mut weights = size(&s.active_diffusion()) + size(&s.active_text_encoder()) + size(&s.vae);
            if s.vision_ok() {
                weights += size(&s.vision);
            }
            let (total, available) = (crate::config::system_ram_bytes(), crate::config::available_ram_bytes());
            let in_ram = weights_fit_in_ram(total, available, weights);
            if unified {
                let shared = crate::config::gpu_vram_bytes().unwrap_or(0);
                let image_weights = size(&s.active_diffusion()) + size(&s.vae);
                let (te, diffusion) = unified_placement(&device, shared, in_ram, weights, image_weights);
                self.emit(EngineEvent::Log(tf!(
                    "통합 메모리 GPU (공유 {:.1}GB): 가중치를 GPU 메모리에 한 벌만 둡니다 (이미지 모델 {}, 텍스트 인코더 {})",
                    "Unified-memory GPU ({:.1} GB shared): weights are kept once, in GPU memory (image model {}, text encoder {})",
                    shared as f64 / (1u64 << 30) as f64,
                    diffusion,
                    te
                )));
                cmd.arg("--params-backend").arg(format!("diffusion={diffusion},te={te},vae={device}"));
            } else {
                if !in_ram && s.variant != ModelVariant::Int8 {
                    self.emit(EngineEvent::Log(tf!(
                        "여유 RAM {:.1}GB: 텍스트 인코더를 디스크에서 읽습니다 (새 프롬프트마다 몇 초 더, RAM 부족으로 멈추지 않음)",
                        "{:.1} GB RAM free: the text encoder is read from disk (a few seconds more per new prompt, no stalls from low RAM)",
                        available.or(total).unwrap_or(0) as f64 / (1u64 << 30) as f64
                    )));
                }
                let te = if s.variant == ModelVariant::Int8 || !in_ram { "disk" } else { "cpu" };
                cmd.arg("--params-backend").arg(format!("diffusion=cpu,te={te},vae=cpu"));
            }
        }
        // 4 GB cards: a 3 GiB budget leaves room for the desktop and driver, and the VAE
        // decodes in tiles (its 1024² decode is the biggest single VRAM spike). Measured on
        // an RTX 3060 Ti with all but ~3.5 GB of VRAM held: 1024² in 21 s + 7 s decode. Cards with
        // more memory keep the settings measured on 8 GB cards.
        let vram = crate::config::gpu_vram_bytes();
        let low_vram = crate::config::low_vram(vram);
        if low_vram && !unified {
            self.emit(EngineEvent::Log(tf!(
                "VRAM {:.1}GB 그래픽카드: 저VRAM 모드 (VRAM 한도 3GB, VAE 타일링)",
                "{:.1} GB VRAM card: low-VRAM mode (3 GB VRAM budget, VAE tiling)",
                vram.unwrap_or(0) as f64 / (1u64 << 30) as f64
            )));
        }
        if !extra.iter().any(|a| a == "--max-vram") {
            // 6 GiB was measured on 8 GB NVIDIA cards; the AMD builds use the same budget,
            // lowered on cards with less memory.
            // Card memory minus 2 GiB for the desktop and driver, at most 6: 8 GB and larger
            // cards keep 6 (as measured), 6 GB cards (GTX 1660 Ti, RTX 2060, laptop 3060/4050)
            // get 4 instead of filling the whole card, 4 GB cards 3.
            // Unified memory: 0 = the engine works with the memory free at the time, since a
            // fixed budget there is just more of the same RAM held back.
            let budget = if unified {
                0
            } else if low_vram {
                3
            } else {
                crate::config::vram_budget_gib(vram)
            };
            cmd.arg("--max-vram").arg(budget.to_string());
        }
        // Self-test only (QIS_SHOT_DIR): QIS_LOWVRAM_TILE=off|<size> to compare tilings.
        let tile_test = std::env::var_os("QIS_SHOT_DIR").and(std::env::var("QIS_LOWVRAM_TILE").ok());
        // 512×512 tiles on 4 and 6 GB cards: with ~3.5 GB free (4 GB card) and ~5.3 GB free
        // (6 GB card) the untiled 1024² decode ran out of memory and had to be retried;
        // 256² tiles took 17 s, 512² tiles 7 s; generation itself was unchanged.
        let tile_vae = vram.is_some_and(|b| b < 7 << 30);
        if tile_vae && !extra.iter().any(|a| a == "--vae-tiling") && tile_test.as_deref() != Some("off") {
            let size = tile_test.filter(|t| t != "off").unwrap_or_else(|| "512x512".to_owned());
            cmd.arg("--vae-tiling").arg("--vae-tile-size").arg(size);
        }
        cmd.args(extra);
        self.warmup_enabled = s.warmup;
        self.warmup = None;
        cmd.stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped());
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            const CREATE_NO_WINDOW: u32 = 0x0800_0000;
            cmd.creation_flags(CREATE_NO_WINDOW);
        }

        #[cfg(target_os = "linux")]
        die_with_parent(&mut cmd);

        self.emit(EngineEvent::Log(tf!("엔진 시작: {} (포트 {})", "Engine start: {} (port {})", exe.display(), port)));
        let mut child = cmd.spawn().context(t!("sd-server 실행 실패", "Failed to start sd-server"))?;
        #[cfg(windows)]
        kill_with_parent(&child);
        for pipe in [
            child.stdout.take().map(|p| Box::new(p) as Box<dyn Read + Send>),
            child.stderr.take().map(|p| Box::new(p) as Box<dyn Read + Send>),
        ]
        .into_iter()
        .flatten()
        {
            let tx = self.events.clone();
            std::thread::spawn(move || pump_log(pipe, tx));
        }
        self.child = Some(child);
        self.set_state(EngineState::Loading { since: Instant::now() });
        Ok(())
    }

    fn kill(&mut self) {
        if let Some(mut c) = self.child.take() {
            let _ = c.kill();
            let _ = c.wait();
        }
        if let Some(r) = self.running.take() {
            self.emit(EngineEvent::JobFailed { id: r.job.id, message: t!("엔진이 중지되었습니다", "The engine was stopped").into() });
        }
    }

    fn cancel(&mut self, id: u64) {
        if let Some(pos) = self.queue.iter().position(|j| j.id == id) {
            self.queue.remove(pos);
            self.emit(EngineEvent::JobCancelled { id });
            return;
        }
        let url = match &mut self.running {
            Some(r) if r.job.id == id && !r.cancel_sent => {
                r.cancel_sent = true;
                format!("{}/sdcpp/v1/jobs/{}/cancel", self.base_url, r.remote_id)
            }
            _ => return,
        };
        let _ = self.agent.post(&url).send_empty();
    }

    fn tick(&mut self) {
        // Detect a crashed/exited server.
        if let Some(child) = &mut self.child {
            if let Ok(Some(status)) = child.try_wait() {
                self.child = None;
                if let Some(r) = self.running.take() {
                    self.emit(EngineEvent::JobFailed { id: r.job.id, message: t!("엔진이 종료되었습니다", "The engine exited").into() });
                }
                self.set_state(EngineState::Error(tf!(
                    "엔진이 예기치 않게 종료되었습니다 ({}). 로그를 확인하세요.",
                    "The engine exited unexpectedly ({}). Check the log.",
                    status
                )));
                return;
            }
        }

        let interval = if self.running.is_some() { 250 } else { 500 };
        if self.last_poll.elapsed() < Duration::from_millis(interval) {
            return;
        }
        self.last_poll = Instant::now();

        match self.state.clone() {
            EngineState::Loading { .. } if self.warmup.is_some() => self.poll_warmup(),
            EngineState::Loading { .. } => self.poll_loading(),
            EngineState::Ready => {
                if self.running.is_some() {
                    if let Err(e) = self.poll_job() {
                        if let Some(r) = self.running.take() {
                            self.emit(EngineEvent::JobFailed { id: r.job.id, message: format!("{e:#}") });
                        }
                    }
                } else if let Some(job) = self.queue.pop_front() {
                    let id = job.id;
                    if let Err(e) = self.submit(job, 0) {
                        self.emit(EngineEvent::JobFailed { id, message: format!("{e:#}") });
                    }
                }
            }
            _ => {}
        }
    }

    fn poll_loading(&mut self) {
        let url = format!("{}/sdcpp/v1/models/status", self.base_url);
        let Ok(mut resp) = self.agent.get(&url).call() else {
            return; // server not listening yet
        };
        let Ok(v) = resp.body_mut().read_json::<Value>() else { return };
        match v["state"].as_str() {
            Some("ready") => {
                let secs = v["load_elapsed_seconds"].as_f64().unwrap_or(0.0);
                self.emit(EngineEvent::Log(tf!("모델 로드 완료 ({:.1}초)", "Model loaded ({:.1} s)", secs)));
                if self.warmup_enabled && self.start_warmup().is_ok() {
                    return; // stay in Loading until the weights are resident
                }
                self.set_state(EngineState::Ready);
            }
            Some("error") => {
                let msg = v["error"].as_str().unwrap_or(t!("모델 로드 실패", "Model load failed")).to_owned();
                self.set_state(EngineState::Error(msg));
            }
            _ => {}
        }
    }

    /// Runs one tiny 1-step generation so the text encoder, diffusion model and VAE are
    /// read from disk now rather than on the user's first image (measured ~36 s → ~18 s).
    fn start_warmup(&mut self) -> Result<()> {
        let body = json!({
            "prompt": "warm up",
            "negative_prompt": "",
            "width": 512,
            "height": 512,
            "seed": 0,
            "batch_count": 1,
            "sample_params": {
                "sample_method": "euler",
                "sample_steps": 1,
                "custom_sigmas": [1.0, 0.0],
                "guidance": { "txt_cfg": 1.0, "distilled_guidance": 3.5,
                              "slg": { "layers": [7, 8, 9], "layer_start": 0.01, "layer_end": 0.2, "scale": 0.0 } }
            },
            "preview": "none",
            "output_format": "png",
            "embed_image_metadata": false
        });
        let url = format!("{}/sdcpp/v1/img_gen", self.base_url);
        let mut resp = self.agent.post(&url).send_json(&body)?;
        let v: Value = resp.body_mut().read_json().unwrap_or(Value::Null);
        let id = v["id"].as_str().ok_or_else(|| anyhow!(t!("예열 요청 실패", "Warm-up request failed")))?.to_owned();
        self.emit(EngineEvent::Log(t!("예열 중: 모델을 메모리에 올리는 중…", "Warming up: loading the model into memory…").into()));
        self.warmup = Some((id, Instant::now()));
        Ok(())
    }

    fn poll_warmup(&mut self) {
        let Some((id, started)) = &self.warmup else { return };
        let url = format!("{}/sdcpp/v1/jobs/{id}", self.base_url);
        let started = *started;
        let status = self
            .agent
            .get(&url)
            .call()
            .ok()
            .and_then(|mut r| r.body_mut().with_config().limit(64 * 1024 * 1024).read_json::<Value>().ok())
            .map(|v| v["status"].as_str().unwrap_or("").to_owned());
        match status.as_deref() {
            Some("queued") | Some("generating") | None => {}
            other => {
                let secs = started.elapsed().as_secs_f32();
                let msg = match other {
                    Some("completed") => tf!("예열 완료 ({:.1}초)", "Warm-up done ({:.1} s)", secs),
                    _ => tf!("예열 건너뜀 ({:?}) — 첫 생성이 조금 느릴 수 있습니다", "Warm-up skipped ({:?}) — the first image may be a bit slower", other),
                };
                self.emit(EngineEvent::Log(msg));
                self.warmup = None;
                self.warmup_enabled = false;
                self.set_state(EngineState::Ready);
            }
        }
    }

    fn submit(&mut self, job: GenJob, attempt: u8) -> Result<()> {
        let op = if job.mode == GenMode::Outpaint {
            let src = job.inputs.first().ok_or_else(|| anyhow!(t!("입력 이미지가 없습니다", "No input image")))?.clone();
            if job.outpaint.iter().all(|p| *p <= 0.0) {
                bail!("{}", t!("확장할 방향을 하나 이상 정해 주세요", "Pick at least one direction to expand"));
            }
            let passes = if job.guided {
                // The reference keeps big jumps coherent; only very large growth is split.
                crate::config::plan_guided_passes(job.outpaint, crate::config::GUIDED_MAX_AREA)
            } else {
                crate::config::plan_outpaint_passes(job.outpaint, crate::config::OUTPAINT_MAX_STEP)
            };
            Some(OutpaintRun { passes, idx: 0, current: src, started: Instant::now(), retries: 0 })
        } else {
            None
        };
        self.submit_with(job, attempt, op)
    }

    fn submit_with(&mut self, job: GenJob, attempt: u8, op: Option<OutpaintRun>) -> Result<()> {
        let mut sample = json!({
            "sample_method": "euler",
            "sample_steps": job.steps,
            "guidance": {
                "txt_cfg": job.cfg,
                "distilled_guidance": 3.5,
                "slg": { "layers": [7, 8, 9], "layer_start": 0.01, "layer_end": 0.2, "scale": 0.0 }
            }
        });
        if let Some(sig) = &job.sigmas {
            // Schedule required by a few-step LoRA (Lightning: 4 steps).
            sample["custom_sigmas"] = json!(sig);
            sample["sample_steps"] = json!(sig.len().saturating_sub(1).max(1));
        } else if job.step_mode == StepMode::Turbo {
            sample["custom_sigmas"] = json!(TURBO_SIGMAS);
            sample["sample_steps"] = json!(TURBO_SIGMAS.len() - 1);
        }
        let mut body = json!({
            "prompt": job.prompt,
            "negative_prompt": job.negative_prompt,
            "width": job.width,
            "height": job.height,
            "seed": job.seed as i64,
            "batch_count": 1,
            "sample_params": sample,
            "preview": if job.live_preview { "proj" } else { "none" },
            "preview_interval": 1,
            "output_format": "png",
            "embed_image_metadata": true
        });
        if job.fast {
            body["cache_mode"] = json!("spectrum");
        }
        if !job.loras.is_empty() {
            // Paths are relative to --lora-model-dir (models/loras).
            body["lora"] = json!(job
                .loras
                .iter()
                .map(|(file, w)| json!({ "path": file, "multiplier": w, "is_high_noise": false }))
                .collect::<Vec<_>>());
        }
        match job.mode {
            GenMode::Text => {}
            GenMode::Variation => {
                let init = job.inputs.first().ok_or_else(|| anyhow!(t!("입력 이미지가 없습니다", "No input image")))?;
                body["init_image"] = json!(encode_init(init, job.width, job.height)?);
                body["strength"] = json!(job.strength.clamp(0.05, 1.0));
            }
            GenMode::Edit => {
                if job.inputs.is_empty() {
                    bail!("{}", t!("참조 이미지가 없습니다", "No reference image"));
                }
                let refs: Result<Vec<String>> =
                    job.inputs.iter().map(|r| encode_ref(r, job.width, job.height)).collect();
                body["ref_images"] = json!(refs?);
            }
            GenMode::Inpaint => {
                // Same crop/resize for image and mask so they stay aligned. White = repaint.
                let init = job.inputs.first().ok_or_else(|| anyhow!(t!("입력 이미지가 없습니다", "No input image")))?;
                let mask = job.mask.as_ref().ok_or_else(|| anyhow!(t!("수정할 부분을 브러시로 칠해 주세요", "Paint the area to change with the brush")))?;
                body["init_image"] = json!(encode_init(init, job.width, job.height)?);
                let m = image::DynamicImage::ImageLuma8((**mask).clone())
                    .resize_to_fill(job.width, job.height, image::imageops::FilterType::Triangle);
                body["mask_image"] = json!(encode_png_b64(&m)?);
                body["strength"] = json!(job.strength.clamp(0.05, 1.0));
            }
            GenMode::Outpaint if job.guided => {
                let op = op.as_ref().ok_or_else(|| anyhow!(t!("확장 계획이 없습니다", "No expand plan")))?;
                let pads = op.passes[op.idx];
                // Last pass: the requested size; earlier passes: same pixel budget, own shape.
                let (w, h) = if op.idx + 1 == op.passes.len() {
                    (job.width, job.height)
                } else {
                    let (cw, ch) = crate::config::padded_size(op.current.width(), op.current.height(), pads);
                    same_area_dims(cw, ch, job.width, job.height)
                };
                body["width"] = json!(w);
                body["height"] = json!(h);
                let (reference, init, mask) = guided_canvas(&op.current, pads, w, h);
                body["prompt"] = json!(guided_prompt(&job.prompt));
                body["ref_images"] = json!([encode_rgba_b64(&reference)?]);
                body["init_image"] = json!(encode_rgba_b64(&init)?);
                body["mask_image"] = json!(encode_png_b64(&image::DynamicImage::ImageLuma8(mask))?);
                body["strength"] = json!(1.0);
            }
            GenMode::Outpaint => {
                let op = op.as_ref().ok_or_else(|| anyhow!(t!("확장 계획이 없습니다", "No expand plan")))?;
                let pads = op.passes[op.idx];
                let (canvas, mask) = outpaint_canvas(&op.current, pads);
                // Last pass: the requested output size; earlier passes: same pixel budget
                // at that pass's shape.
                let (w, h) = if op.idx + 1 == op.passes.len() {
                    (job.width, job.height)
                } else {
                    same_area_dims(canvas.width(), canvas.height(), job.width, job.height)
                };
                body["width"] = json!(w);
                body["height"] = json!(h);
                body["init_image"] = json!(encode_init(&canvas, w, h)?);
                let m = image::DynamicImage::ImageLuma8(mask).resize_to_fill(w, h, image::imageops::FilterType::Triangle);
                body["mask_image"] = json!(encode_png_b64(&m)?);
                body["strength"] = json!(OUTPAINT_STRENGTH);
            }
        }
        // Dev aid: keep the exact request bodies of a self-test run for replaying.
        if let (Some(dir), Some(_)) = (std::env::var_os("QIS_SHOT_DIR"), std::env::var_os("QIS_DUMP_REQ")) {
            let path = std::path::Path::new(&dir).join(format!("req_{}_{}.json", job.id, job.seed));
            let _ = std::fs::write(path, body.to_string());
        }
        let url = format!("{}/sdcpp/v1/img_gen", self.base_url);
        let mut resp = self.agent.post(&url).send_json(&body)?;
        let status = resp.status().as_u16();
        let v: Value = resp.body_mut().read_json().unwrap_or(Value::Null);
        if status != 202 && status != 200 {
            bail!("{}", tf!("생성 요청 거부 (HTTP {}): {}", "Generation request refused (HTTP {}): {}", status, error_text(&v)));
        }
        let remote_id = v["id"].as_str().ok_or_else(|| anyhow!(t!("작업 ID 없음", "No job ID")))?.to_owned();
        if attempt == 0 && op.as_ref().is_none_or(|o| o.idx == 0) {
            self.emit(EngineEvent::JobStarted { id: job.id });
        }
        self.running = Some(Running {
            job,
            remote_id,
            started: Instant::now(),
            last_step: 0,
            cancel_sent: false,
            attempt,
            outpaint: op,
        });
        Ok(())
    }

    /// True when the running guided outpaint pass finished with part of its new area left
    /// blank (see `border_left_blank`: flat slab, frosted panel, one-colour patch) and may
    /// still be redrawn.
    fn guided_border_blank(&self, v: &Value) -> bool {
        let Some(r) = &self.running else { return false };
        let Some(op) = r.outpaint.as_ref().filter(|o| r.job.guided && o.retries < GUIDED_RETRIES && !r.cancel_sent) else {
            return false;
        };
        let img = v["result"]["images"][0]["b64_json"]
            .as_str()
            .and_then(|b| base64::engine::general_purpose::STANDARD.decode(b).ok())
            .and_then(|bytes| image::load_from_memory(&bytes).ok())
            .map(|i| i.into_rgba8());
        img.is_some_and(|img| border_left_blank(&img, op.current.dimensions(), op.passes[op.idx]))
    }

    fn poll_job(&mut self) -> Result<()> {
        let Some(r) = &self.running else { return Ok(()) };
        let url = format!("{}/sdcpp/v1/jobs/{}", self.base_url, r.remote_id);
        let mut resp = self.agent.get(&url).call()?;
        let code = resp.status().as_u16();
        let v: Value = resp
            .body_mut()
            .with_config()
            .limit(512 * 1024 * 1024)
            .read_json()
            .context(t!("작업 상태 응답 파싱 실패", "Could not parse the job status"))?;
        if code != 200 {
            bail!("{}", tf!("작업 조회 실패 (HTTP {}): {}", "Job status request failed (HTTP {}): {}", code, error_text(&v)));
        }

        match v["status"].as_str().unwrap_or("") {
            "completed" if self.guided_border_blank(&v) => {
                // The model left the new area blank: draw this pass again, new seed.
                let mut r = self.running.take().unwrap();
                let mut op = r.outpaint.take().unwrap();
                op.retries += 1;
                r.job.seed = r.job.seed.wrapping_add(7919 * op.retries as u64);
                let text = tf!("새 부분이 비어 보여 다시 그리는 중 ({}/{})", "The new area looked blank, redrawing ({}/{})", op.retries, GUIDED_RETRIES);
                self.emit(EngineEvent::Log(tf!("확장: {}", "Expand: {}", text)));
                let id = r.job.id;
                self.emit(EngineEvent::Note { id, text });
                if let Err(e) = self.submit_with(r.job, 1, Some(op)) {
                    self.emit(EngineEvent::JobFailed { id, message: format!("{e:#}") });
                }
            }
            "completed" if self.running.as_ref().and_then(|r| r.outpaint.as_ref()).is_some_and(|o| o.idx + 1 < o.passes.len()) => {
                // Intermediate outpaint pass: feed the result into the next one.
                let r = self.running.take().unwrap();
                let id = r.job.id;
                let next = (|| -> Result<image::RgbaImage> {
                    let b64 = v["result"]["images"][0]["b64_json"].as_str().ok_or_else(|| anyhow!(t!("결과 이미지가 비어 있습니다", "The result image is empty")))?;
                    Ok(image::load_from_memory(&base64::engine::general_purpose::STANDARD.decode(b64)?)?.into_rgba8())
                })();
                let res = next.and_then(|img| {
                    let mut op = r.outpaint.clone().unwrap();
                    op.retries = 0;
                    let img = if r.job.guided {
                        guided_finish(&img, &op.current, op.passes[op.idx])
                    } else {
                        composite_kept(&img, &op.current, op.passes[op.idx])
                    };
                    op.idx += 1;
                    op.current = Arc::new(img);
                    self.emit(EngineEvent::Log(tf!("확장 {}/{}단계", "Expand pass {}/{}", op.idx + 1, op.passes.len())));
                    self.emit(EngineEvent::Note { id, text: tf!("{}단계 완료 · 이어서 더 넓히는 중", "Pass {} done · widening further", op.idx) });
                    self.submit_with(r.job, 0, Some(op))
                });
                if let Err(e) = res {
                    self.emit(EngineEvent::JobFailed { id, message: format!("{e:#}") });
                }
            }
            "completed" => {
                // Decode and save while the job is still `running`, then always resolve it:
                // an early `?` here used to leave the UI showing "생성 중" forever.
                let saved = (|| -> Result<GeneratedImage> {
                    let b64 = v["result"]["images"][0]["b64_json"]
                        .as_str()
                        .ok_or_else(|| anyhow!(t!("결과 이미지가 비어 있습니다", "The result image is empty")))?;
                    let png = base64::engine::general_purpose::STANDARD.decode(b64)?;
                    let r = self.running.as_ref().unwrap();
                    let png = match &r.outpaint {
                        Some(op) if r.job.guided => {
                            let img = image::load_from_memory(&png)?.into_rgba8();
                            png_with_text_of(&guided_finish(&img, &op.current, op.passes[op.idx]), &png)?
                        }
                        Some(op) => {
                            let img = image::load_from_memory(&png)?.into_rgba8();
                            png_with_text_of(&composite_kept(&img, &op.current, op.passes[op.idx]), &png)?
                        }
                        None => png,
                    };
                    save_output(&r.job, &png)
                })();
                let r = self.running.take().unwrap();
                match saved {
                    Ok(result) => {
                        let started = r.outpaint.as_ref().map_or(r.started, |o| o.started);
                        let elapsed = started.elapsed().as_secs_f32();
                        self.emit(EngineEvent::JobDone { job: r.job, result, elapsed });
                    }
                    Err(e) => self.emit(EngineEvent::JobFailed { id: r.job.id, message: tf!("결과 저장 실패: {:#}", "Could not save the result: {:#}", e) }),
                }
            }
            "failed" => {
                let r = self.running.take().unwrap();
                let message = error_text(&v["error"]);
                // A transient VRAM shortage (e.g. another app grabbed memory) usually
                // succeeds on a second try once the engine has released its buffers.
                if r.attempt == 0 && !r.cancel_sent {
                    self.emit(EngineEvent::Log(tf!("생성 실패, 1회 재시도: {}", "Generation failed, retrying once: {}", message)));
                    let id = r.job.id;
                    if let Err(e) = self.submit_with(r.job, 1, r.outpaint) {
                        self.emit(EngineEvent::JobFailed { id, message: format!("{e:#}") });
                    }
                } else {
                    self.emit(EngineEvent::JobFailed { id: r.job.id, message });
                }
            }
            "cancelled" => {
                let r = self.running.take().unwrap();
                self.emit(EngineEvent::JobCancelled { id: r.job.id });
            }
            _ => {
                let p = &v["preview"];
                let step = p["step"].as_u64().unwrap_or(0) as u32;
                let total = p["total_steps"].as_u64().unwrap_or(0) as u32;
                let r = self.running.as_mut().unwrap();
                if step != r.last_step && total > 0 {
                    r.last_step = step;
                    let id = r.job.id;
                    let pass = r.outpaint.as_ref().map(|o| (o.idx as u32 + 1, o.passes.len() as u32));
                    let preview = p["b64_json"]
                        .as_str()
                        .and_then(|b| base64::engine::general_purpose::STANDARD.decode(b).ok())
                        .and_then(|bytes| decode_png(&bytes).ok());
                    self.emit(EngineEvent::Progress { id, step, total, preview, pass });
                }
            }
        }
        Ok(())
    }
}

impl Drop for Worker {
    fn drop(&mut self) {
        self.kill();
    }
}

fn error_text(v: &Value) -> String {
    if let Some(s) = v.as_str() {
        return s.to_owned();
    }
    for key in ["message", "error"] {
        match &v[key] {
            Value::String(s) => return s.clone(),
            Value::Object(_) => return error_text(&v[key]),
            _ => {}
        }
    }
    if v.is_null() { t!("알 수 없는 오류", "Unknown error").into() } else { v.to_string() }
}

fn save_output(job: &GenJob, png: &[u8]) -> Result<GeneratedImage> {
    std::fs::create_dir_all(&job.output_dir)
        .with_context(|| tf!("출력 폴더 생성 실패: {}", "Could not create the output folder: {}", job.output_dir.display()))?;
    let stamp = chrono::Local::now().format("%Y%m%d_%H%M%S");
    let mut path = job.output_dir.join(format!("{stamp}_{}.png", job.seed));
    let mut n = 1;
    while path.exists() {
        path = job.output_dir.join(format!("{stamp}_{}_{n}.png", job.seed));
        n += 1;
    }
    std::fs::write(&path, png)?;
    // Decode once; derive the full texture and the cached thumbnail from it.
    let decoded = image::load_from_memory(png)?;
    let thumb = thumb_rgba(&decoded);
    write_thumb_cache(&path, &thumb);
    let image = rgba_to_color(&decoded.into_rgba8());
    Ok(GeneratedImage { path, image, thumb: rgba_to_color(&thumb) })
}

/// Builds the outpaint canvas: the source placed inside a larger frame whose new area is
/// pre-filled with the source's colours (heavily smoothed), plus a mask that is white over
/// the new area and slightly overlaps the old edges so the seam is repainted too.
pub fn outpaint_canvas(src: &image::RgbaImage, p: crate::config::Pads) -> (image::RgbaImage, image::GrayImage) {
    use image::imageops;
    let (w0, h0) = src.dimensions();
    let (cw, ch) = crate::config::padded_size(w0, h0, p);
    let (ox, oy) = ((w0 as f32 * p[0]).round() as i64, (h0 as f32 * p[2]).round() as i64);
    // Fill: each new pixel copies the nearest edge pixel of the source, then everything is
    // blurred, so the new area starts from the colours just inside each edge.
    let mut canvas = image::RgbaImage::from_fn(cw, ch, |x, y| {
        let sx = (x as i64 - ox).clamp(0, w0 as i64 - 1) as u32;
        let sy = (y as i64 - oy).clamp(0, h0 as i64 - 1) as u32;
        *src.get_pixel(sx, sy)
    });
    canvas = imageops::fast_blur(&canvas, (w0.max(h0) as f32 / 30.0).max(1.0));
    imageops::overlay(&mut canvas, src, ox, oy);

    // Overlap repainted inside the old edge: 3% left a faint seam in tests, so blend wider.
    let ov = ((w0.min(h0) as f32) * 0.08).round() as i64;
    let x0 = ox + if p[0] > 0.0 { ov } else { 0 };
    let x1 = ox + w0 as i64 - if p[1] > 0.0 { ov } else { 0 };
    let y0 = oy + if p[2] > 0.0 { ov } else { 0 };
    let y1 = oy + h0 as i64 - if p[3] > 0.0 { ov } else { 0 };
    let mut mask = image::GrayImage::from_pixel(cw, ch, image::Luma([255]));
    for y in y0.max(0)..y1.min(ch as i64) {
        for x in x0.max(0)..x1.min(cw as i64) {
            mask.put_pixel(x as u32, y as u32, image::Luma([0]));
        }
    }
    let mask = imageops::fast_blur(&mask, (ov as f32 / 2.0).max(1.0));
    (canvas, mask)
}

/// After an outpaint pass, puts the previous image's own pixels back wherever the pass
/// mask kept them (feathered at the seam). Each pass re-encodes the whole picture, and
/// without this the kept area drifted in colour pass after pass (visible as boxes).
/// Mirrors `encode_init`'s centre-crop so the previous image lands exactly in place.
pub fn composite_kept(result: &image::RgbaImage, prev: &image::RgbaImage, p: crate::config::Pads) -> image::RgbaImage {
    use image::imageops::{self, FilterType};
    let (rw, rh) = result.dimensions();
    let (w0, h0) = prev.dimensions();
    let (cw, ch) = crate::config::padded_size(w0, h0, p);
    let (_, mask) = outpaint_canvas(prev, p);
    // canvas → result: scale to fill, then centre-crop (same as encode_init).
    let s = (rw as f32 / cw as f32).max(rh as f32 / ch as f32);
    let (crop_x, crop_y) = ((cw as f32 * s - rw as f32) / 2.0, (ch as f32 * s - rh as f32) / 2.0);
    let ox = (w0 as f32 * p[0]) * s - crop_x;
    let oy = (h0 as f32 * p[2]) * s - crop_y;
    let (pw, ph) = (((w0 as f32) * s).round().max(1.0) as u32, ((h0 as f32) * s).round().max(1.0) as u32);
    let prev_s = imageops::resize(prev, pw, ph, FilterType::Lanczos3);
    let mask_s = imageops::resize(&mask, ((cw as f32) * s).round().max(1.0) as u32, ((ch as f32) * s).round().max(1.0) as u32, FilterType::Triangle);
    let mut out = result.clone();
    let (ox, oy) = (ox.round() as i64, oy.round() as i64);
    for y in 0..ph as i64 {
        for x in 0..pw as i64 {
            let (tx, ty) = (x + ox, y + oy);
            if tx < 0 || ty < 0 || tx >= rw as i64 || ty >= rh as i64 {
                continue;
            }
            let (mx, my) = ((tx as f32 + crop_x).round() as u32, (ty as f32 + crop_y).round() as u32);
            let m = mask_s.get_pixel(mx.min(mask_s.width() - 1), my.min(mask_s.height() - 1))[0] as f32 / 255.0;
            if m >= 0.999 {
                continue; // fully regenerated here
            }
            let a = prev_s.get_pixel(x as u32, y as u32);
            let b = out.get_pixel_mut(tx as u32, ty as u32);
            for c in 0..3 {
                b[c] = (a[c] as f32 * (1.0 - m) + b[c] as f32 * m).round() as u8;
            }
        }
    }
    out
}

/// Where the source sits inside a `w`×`h` frame that shows the whole padded canvas
/// (fitted, centred): (left, top, scale from source pixels to frame pixels).
fn guided_place(w0: u32, h0: u32, p: crate::config::Pads, w: u32, h: u32) -> (f32, f32, f32) {
    let (cw, ch) = (w0 as f32 * (1.0 + p[0] + p[1]), h0 as f32 * (1.0 + p[2] + p[3]));
    let s = (w as f32 / cw).min(h as f32 / ch);
    let ox = w as f32 / 2.0 - cw * s / 2.0 + w0 as f32 * p[0] * s;
    let oy = h as f32 / 2.0 - ch * s / 2.0 + h0 as f32 * p[2] * s;
    (ox, oy, s)
}

/// Approximate size of the finished guided outpaint (see `guided_finish`) for a source of
/// `w0`×`h0`, total pads `p`, generated at `w`×`h`.
pub fn guided_final_size(w0: u32, h0: u32, p: crate::config::Pads, w: u32, h: u32) -> (u32, u32) {
    let (_, _, s) = guided_place(w0, h0, p, w, h);
    let k = (1.0f32 / s).min(1.6).min(4096.0 / w.max(h) as f32).max(1.0);
    ((w as f32 * k).round() as u32, (h as f32 * k).round() as u32)
}

/// Which sides of the placed source border on new area: [left, right, top, bottom].
fn open_sides(ox: f32, oy: f32, sw: f32, sh: f32, w: u32, h: u32) -> [bool; 4] {
    [ox > 1.0, ox + sw < w as f32 - 1.0, oy > 1.0, oy + sh < h as f32 - 1.0]
}

/// Seam overlap (frame pixels) repainted just inside the source's open edges.
fn guided_overlap(sw: f32, sh: f32) -> f32 {
    (sw.min(sh) * 0.03).max(8.0)
}

/// Inputs for one reference-guided outpaint pass at the output size `w`×`h`:
/// (reference = source on flat grey, init = source on edge-stretched colours, mask = new
/// area plus a thin overlap inside the open edges, softened).
pub fn guided_canvas(
    src: &image::RgbaImage,
    p: crate::config::Pads,
    w: u32,
    h: u32,
) -> (image::RgbaImage, image::RgbaImage, image::GrayImage) {
    use image::imageops::{self, FilterType};
    let (w0, h0) = src.dimensions();
    let (ox, oy, s) = guided_place(w0, h0, p, w, h);
    let (sw, sh) = (((w0 as f32) * s).round().max(1.0) as u32, ((h0 as f32) * s).round().max(1.0) as u32);
    let src_s = imageops::resize(src, sw, sh, FilterType::Lanczos3);
    let (ix, iy) = (ox.round() as i64, oy.round() as i64);

    let mut reference = image::RgbaImage::from_pixel(w, h, image::Rgba([128, 128, 128, 255]));
    imageops::overlay(&mut reference, &src_s, ix, iy);

    let mut init = image::RgbaImage::from_fn(w, h, |x, y| {
        let sx = (x as i64 - ix).clamp(0, sw as i64 - 1) as u32;
        let sy = (y as i64 - iy).clamp(0, sh as i64 - 1) as u32;
        *src_s.get_pixel(sx, sy)
    });
    init = imageops::fast_blur(&init, (w.max(h) as f32 / 60.0).max(1.0));
    imageops::overlay(&mut init, &src_s, ix, iy);
    // Qwen-Image 2.1 is RGBA-native and its outputs carry faint alpha noise (226–255);
    // send fully opaque pictures, as in the tests that chose this method.
    for img in [&mut reference, &mut init] {
        for px in img.pixels_mut() {
            px[3] = 255;
        }
    }

    let open = open_sides(ox, oy, sw as f32, sh as f32, w, h);
    let ov = guided_overlap(sw as f32, sh as f32);
    let x0 = ox + if open[0] { ov } else { 0.0 };
    let x1 = ox + sw as f32 - if open[1] { ov } else { 0.0 };
    let y0 = oy + if open[2] { ov } else { 0.0 };
    let y1 = oy + sh as f32 - if open[3] { ov } else { 0.0 };
    let mut mask = image::GrayImage::from_pixel(w, h, image::Luma([255]));
    for y in (y0.round().max(0.0) as u32)..(y1.round().min(h as f32) as u32) {
        for x in (x0.round().max(0.0) as u32)..(x1.round().min(w as f32) as u32) {
            mask.put_pixel(x, y, image::Luma([0]));
        }
    }
    let mask = imageops::fast_blur(&mask, (ov / 3.0).max(1.0));
    (reference, init, mask)
}

use crate::config::LOW_RAM_BYTES;

/// RAM left over for Windows/Linux and the apps already open when all weights are in RAM.
const RAM_HEADROOM: u64 = 2 << 30;

/// Whether all weights can stay in RAM: the PC has the RAM (`LOW_RAM_BYTES`) and enough of
/// it is free now. On a 16 GB PC with a browser open, the 10.2 GB in-RAM set got paged
/// out and the engine stalled for minutes; reading the text encoder from disk instead
/// costs ~10 s per new prompt (a repeated prompt is cached) and nothing else.
fn weights_fit_in_ram(total: Option<u64>, available: Option<u64>, weights: u64) -> bool {
    total.is_none_or(|t| t >= LOW_RAM_BYTES) && available.is_none_or(|a| a >= weights + RAM_HEADROOM)
}

/// Where an APU keeps the text encoder and the image model: on the GPU `device` when all
/// weights fit in its shared memory with 2 GiB to spare for compute buffers and in the RAM
/// free now (`in_ram`: the same RAM). Otherwise the text encoder is read from disk (only a
/// new prompt pays for it). The image model stays on the GPU whenever it fits in the shared
/// memory, even with little RAM free: any other place but disk takes the same RAM or more,
/// and from disk it is re-read for every image (+8 s per image measured on Vulkan).
fn unified_placement<'a>(device: &'a str, shared: u64, in_ram: bool, weights: u64, image_weights: u64) -> (&'a str, &'a str) {
    if in_ram && weights + RAM_HEADROOM <= shared {
        (device, device)
    } else if image_weights + RAM_HEADROOM <= shared {
        ("disk", device)
    } else {
        ("disk", "disk")
    }
}

/// Generic scene text used when the user gave none (the fallback path needs a scene).
pub const OUTPAINT_DEFAULT: &str = "같은 공간이 넓게 보이는 한 장의 사진, 같은 배경, 같은 조명과 색감, 선명한 디테일";
/// What the English UI shows for it; sent to the model as the tested Korean one.
pub const OUTPAINT_DEFAULT_EN: &str = "One photo showing the same space wider, the same background, the same lighting and colors, sharp details";

/// The generic scene text in the UI language.
pub fn outpaint_default() -> &'static str {
    if crate::i18n::en() { OUTPAINT_DEFAULT_EN } else { OUTPAINT_DEFAULT }
}

/// True for the generic scene text in either language.
pub fn is_outpaint_default(s: &str) -> bool {
    let s = s.trim();
    s == OUTPAINT_DEFAULT || s == OUTPAINT_DEFAULT_EN
}

/// True when a side of a guided pass's new area came back blank: no structure at 1/8
/// scale while the source band next to it has plenty. Seen failures: the grey placeholder
/// copied as a flat slab, or turned into a smooth "frosted panel" with a hard edge. On
/// 19 test outputs it caught all 4 failures and none of the 15 good fills; plain walls and
/// studio backdrops pass because the source next to them is plain too.
pub fn border_left_blank(result: &image::RgbaImage, src_dims: (u32, u32), p: crate::config::Pads) -> bool {
    const B: u32 = 8;
    let (w, h) = result.dimensions();
    let (gw, gh) = ((w / B) as usize, (h / B) as usize);
    if gw < 8 || gh < 8 {
        return false;
    }
    // Colour and luminance averaged over 8×8 blocks (removes the model's fine grain).
    let mut rgb = vec![[0f32; 3]; gw * gh];
    let mut g = vec![0f32; gw * gh];
    for by in 0..gh {
        for bx in 0..gw {
            let mut sum = [0f32; 3];
            for y in 0..B {
                for x in 0..B {
                    let px = result.get_pixel(bx as u32 * B + x, by as u32 * B + y);
                    for c in 0..3 {
                        sum[c] += px[c] as f32;
                    }
                }
            }
            let m = sum.map(|s| s / (B * B) as f32);
            rgb[by * gw + bx] = m;
            g[by * gw + bx] = 0.299 * m[0] + 0.587 * m[1] + 0.114 * m[2];
        }
    }
    // Mean absolute neighbour difference inside a block rectangle [x0,x1)×[y0,y1).
    let structure = |x0: usize, x1: usize, y0: usize, y1: usize| -> Option<f32> {
        if x1 < x0 + 3 || y1 < y0 + 3 {
            return None;
        }
        let (mut dx, mut nx, mut dy, mut ny) = (0f32, 0f32, 0f32, 0f32);
        for y in y0..y1 {
            for x in x0..x1 {
                let v = g[y * gw + x];
                if x + 1 < x1 {
                    dx += (g[y * gw + x + 1] - v).abs();
                    nx += 1.0;
                }
                if y + 1 < y1 {
                    dy += (g[(y + 1) * gw + x] - v).abs();
                    ny += 1.0;
                }
            }
        }
        Some(dx / nx + dy / ny)
    };
    let (ox, oy, s) = guided_place(src_dims.0, src_dims.1, p, w, h);
    let (x1, y1) = (ox + src_dims.0 as f32 * s, oy + src_dims.1 as f32 * s);
    let q = |v: f32| ((v / B as f32).round().max(0.0) as usize).min(gw.max(gh));
    let (bw, bh) = ((x1 - ox) * 0.25, (y1 - oy) * 0.25);
    let (qx0, qx1, qy0, qy1) = (q(ox), q(x1).min(gw), q(oy), q(y1).min(gh));
    // (new strip, source band beside it), one block of margin from the seam.
    let sides = [
        ((0, qx0.saturating_sub(1), 0, gh), (qx0 + 1, q(ox + bw).min(gw), qy0, qy1)),
        ((qx1 + 1, gw, 0, gh), (q(x1 - bw), qx1.saturating_sub(1), qy0, qy1)),
        ((0, gw, 0, qy0.saturating_sub(1)), (qx0, qx1, qy0 + 1, q(oy + bh).min(gh))),
        ((0, gw, qy1 + 1, gh), (qx0, qx1, q(y1 - bh), qy1.saturating_sub(1))),
    ];
    // Dead blocks: identical to their right and lower neighbours (< 0.25 per channel).
    // Real fills keep some grain even on smooth walls; a slab painted one exact colour
    // (the grey placeholder, or a seen magenta 165,57,209) does not. Clipped white/black
    // backdrops are skipped. Good fills measured ≤ 0.14 of the new area, failures ≥ 0.28.
    let (mut dead, mut counted) = (0usize, 0usize);
    for by in 0..gh - 1 {
        for bx in 0..gw - 1 {
            let (x, y) = ((bx * B as usize) as f32, (by * B as usize) as f32);
            let near_src = x >= ox - 8.0 && x < x1 + 8.0 && y >= oy - 8.0 && y < y1 + 8.0;
            if near_src {
                continue;
            }
            counted += 1;
            let c = rgb[by * gw + bx];
            if c.iter().all(|v| *v > 245.0) || c.iter().all(|v| *v < 10.0) {
                continue;
            }
            let (r, d) = (rgb[by * gw + bx + 1], rgb[(by + 1) * gw + bx]);
            if (0..3).all(|k| (r[k] - c[k]).abs() < 0.25 && (d[k] - c[k]).abs() < 0.25) {
                dead += 1;
            }
        }
    }
    if counted > 50 && dead as f32 > 0.22 * counted as f32 {
        return true;
    }

    let min_blocks = 0.04 * (gw * gh) as f32;
    sides.iter().any(|&((a0, a1, b0, b1), (c0, c1, d0, d1))| {
        let blocks = (a1.saturating_sub(a0) * b1.saturating_sub(b0)) as f32;
        if blocks < min_blocks {
            return false;
        }
        match (structure(a0, a1, b0, b1), structure(c0, c1, d0, d1)) {
            // Failures measured 0.2–1.7 (vs 9–25 beside them); good fills ≥ 1.47, and the
            // low ones sat next to plain source bands (ratio ≥ 0.35).
            // A plain source band (white studio) never counts: blank there is right.
            (Some(new), Some(src)) => src >= 2.0 && (new < 1.0 || (new < 2.0 && new < 0.25 * src)),
            _ => false,
        }
    })
}

/// Prompt for a guided pass: the fixed instruction plus a scene description — the user's,
/// or the generic one. Replaying one request over 6 seeds, the bare instruction left the
/// border blank 2×, with any scene text 1× (that seed failed with every wording).
pub fn guided_prompt(scene: &str) -> String {
    let scene = scene.trim();
    let scene = if scene.is_empty() || is_outpaint_default(scene) { OUTPAINT_DEFAULT } else { scene };
    format!("{OUTPAINT_GUIDE} 장면 설명: {scene}")
}

/// Final picture of a guided pass: the generated frame enlarged (up to 1.6×, so the
/// original isn't shrunk much) and the original pasted back at that scale with a soft
/// seam inside the repainted overlap, so the old part stays exactly as it was.
pub fn guided_finish(result: &image::RgbaImage, src: &image::RgbaImage, p: crate::config::Pads) -> image::RgbaImage {
    use image::imageops::{self, FilterType};
    let (w, h) = result.dimensions();
    let (w0, h0) = src.dimensions();
    let (ox, oy, s) = guided_place(w0, h0, p, w, h);
    // Enlarge toward the source's own scale, but at most 1.6× and 4096 px on the long side.
    let k = (1.0f32 / s).min(1.6).min(4096.0 / w.max(h) as f32).max(1.0);
    let (fw, fh) = ((w as f32 * k).round() as u32, (h as f32 * k).round() as u32);
    let mut out = if k > 1.001 { imageops::resize(result, fw, fh, FilterType::Lanczos3) } else { result.clone() };
    let sf = s * k;
    let (sw, sh) = (((w0 as f32) * sf).round().max(1.0) as u32, ((h0 as f32) * sf).round().max(1.0) as u32);
    let src_f = if (w0, h0) == (sw, sh) { src.clone() } else { imageops::resize(src, sw, sh, FilterType::Lanczos3) };
    // Loop bounds come from the picture actually pasted (never past its edge).
    let (sw, sh) = src_f.dimensions();
    let (fx, fy) = (ox * k, oy * k);
    let open = open_sides(fx, fy, sw as f32, sh as f32, fw, fh);
    // Fade over most of the repainted overlap, so the seam falls where the model already
    // blended old and new.
    let feather = (guided_overlap(sw as f32 / k, sh as f32 / k) * k * 0.9).max(2.0);
    let (ix, iy) = (fx.round() as i64, fy.round() as i64);
    for y in 0..sh {
        for x in 0..sw {
            let (tx, ty) = (x as i64 + ix, y as i64 + iy);
            if tx < 0 || ty < 0 || tx >= fw as i64 || ty >= fh as i64 {
                continue;
            }
            let (xf, yf) = (x as f32 + 0.5, y as f32 + 0.5);
            let mut a = 1.0f32;
            if open[0] {
                a = a.min(xf / feather);
            }
            if open[1] {
                a = a.min((sw as f32 - xf) / feather);
            }
            if open[2] {
                a = a.min(yf / feather);
            }
            if open[3] {
                a = a.min((sh as f32 - yf) / feather);
            }
            let a = a.clamp(0.0, 1.0);
            let a = a * a * (3.0 - 2.0 * a); // smoothstep
            let sp = src_f.get_pixel(x, y);
            let d = out.get_pixel_mut(tx as u32, ty as u32);
            for c in 0..3 {
                d[c] = (sp[c] as f32 * a + d[c] as f32 * (1.0 - a)).round() as u8;
            }
        }
    }
    for px in out.pixels_mut() {
        px[3] = 255; // a photo: drop the VAE's faint alpha noise
    }
    out
}

/// Encodes `img` as PNG and carries over the text chunks (sd.cpp's generation record)
/// from `meta_png`, so a composited result still restores its settings when dropped.
fn png_with_text_of(img: &image::RgbaImage, meta_png: &[u8]) -> Result<Vec<u8>> {
    let mut buf = std::io::Cursor::new(Vec::new());
    img.write_to(&mut buf, image::ImageFormat::Png)?;
    let png = buf.into_inner();
    let mut texts = Vec::new();
    let mut i = 8;
    while i + 12 <= meta_png.len() {
        let len = u32::from_be_bytes(meta_png[i..i + 4].try_into()?) as usize;
        let kind = &meta_png[i + 4..i + 8];
        if i + 12 + len > meta_png.len() {
            break;
        }
        if kind == b"tEXt" || kind == b"iTXt" || kind == b"zTXt" {
            texts.extend_from_slice(&meta_png[i..i + 12 + len]); // length + type + data + crc
        }
        i += 12 + len;
    }
    // Insert right after IHDR (8-byte signature + 25-byte IHDR chunk).
    let mut out = Vec::with_capacity(png.len() + texts.len());
    out.extend_from_slice(&png[..33]);
    out.extend_from_slice(&texts);
    out.extend_from_slice(&png[33..]);
    Ok(out)
}

/// Output size with the aspect of `cw`×`ch` and about the pixel count of `w`×`h`,
/// snapped to multiples of 32.
fn same_area_dims(cw: u32, ch: u32, w: u32, h: u32) -> (u32, u32) {
    let area = w as f32 * h as f32;
    let aspect = cw as f32 / ch.max(1) as f32;
    let hh = (area / aspect).sqrt();
    (crate::config::snap32((hh * aspect).round() as u32), crate::config::snap32(hh.round() as u32))
}

/// Center-crops the init image to the output aspect and resizes it to the output size
/// (high-quality Lanczos, instead of the server's default nearest-neighbour).
fn encode_init(img: &image::RgbaImage, w: u32, h: u32) -> Result<String> {
    use image::imageops;
    // Centre-crop to the output aspect, then resize (crop view: no full-image copy).
    let (iw, ih) = img.dimensions();
    let scale = (w as f32 / iw as f32).max(h as f32 / ih as f32);
    let cw = ((w as f32 / scale).round() as u32).clamp(1, iw);
    let ch = ((h as f32 / scale).round() as u32).clamp(1, ih);
    let view = imageops::crop_imm(img, (iw - cw) / 2, (ih - ch) / 2, cw, ch).to_image();
    encode_rgba_b64(&imageops::resize(&view, w, h, imageops::FilterType::Lanczos3))
}

/// Reference images keep their aspect ratio; only shrink them to about the output's
/// pixel count so huge photos don't blow up encoding time and memory.
fn encode_ref(img: &image::RgbaImage, w: u32, h: u32) -> Result<String> {
    let target = (w as f64) * (h as f64);
    let have = (img.width() as f64) * (img.height() as f64);
    if have > target * 1.05 {
        let s = (target / have).sqrt();
        let nw = ((img.width() as f64 * s).round() as u32).max(64);
        let nh = ((img.height() as f64 * s).round() as u32).max(64);
        encode_rgba_b64(&image::imageops::resize(img, nw, nh, image::imageops::FilterType::Lanczos3))
    } else {
        encode_rgba_b64(img)
    }
}

fn encode_rgba_b64(img: &image::RgbaImage) -> Result<String> {
    let mut buf = std::io::Cursor::new(Vec::new());
    img.write_to(&mut buf, image::ImageFormat::Png)?;
    Ok(base64::engine::general_purpose::STANDARD.encode(buf.into_inner()))
}

fn encode_png_b64(img: &image::DynamicImage) -> Result<String> {
    let mut buf = std::io::Cursor::new(Vec::new());
    img.write_to(&mut buf, image::ImageFormat::Png)?;
    Ok(base64::engine::general_purpose::STANDARD.encode(buf.into_inner()))
}

pub fn rgba_to_color(img: &image::RgbaImage) -> ColorImage {
    ColorImage::from_rgba_unmultiplied([img.width() as usize, img.height() as usize], img.as_raw())
}

pub fn decode_png(bytes: &[u8]) -> Result<ColorImage> {
    Ok(rgba_to_color(&image::load_from_memory(bytes)?.into_rgba8()))
}

pub fn thumb_rgba(img: &image::DynamicImage) -> image::RgbaImage {
    img.thumbnail(320, 320).into_rgba8()
}

/// Gallery thumbnails are cached as small JPEGs in `<output>/.thumbs/`, so startup
/// doesn't re-decode every full-size PNG.
pub fn thumb_cache_path(file: &std::path::Path) -> Option<PathBuf> {
    let stem = file.file_stem()?;
    Some(file.parent()?.join(".thumbs").join(stem).with_extension("jpg"))
}

pub fn write_thumb_cache(file: &std::path::Path, thumb: &image::RgbaImage) {
    let Some(p) = thumb_cache_path(file) else { return };
    if let Some(dir) = p.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let rgb = image::DynamicImage::ImageRgba8(thumb.clone()).into_rgb8();
    let _ = rgb.save_with_format(&p, image::ImageFormat::Jpeg);
}

/// Thumbnail for a gallery file: the cached JPEG if present, else decoded and cached.
pub fn load_thumb(file: &std::path::Path) -> Option<ColorImage> {
    if let Some(p) = thumb_cache_path(file) {
        if let Ok(img) = image::open(&p) {
            return Some(rgba_to_color(&img.into_rgba8()));
        }
    }
    let thumb = thumb_rgba(&image::open(file).ok()?);
    write_thumb_cache(file, &thumb);
    Some(rgba_to_color(&thumb))
}

fn pump_log(pipe: Box<dyn Read + Send>, tx: Sender<EngineEvent>) {
    let mut reader = BufReader::new(pipe);
    let mut buf = Vec::new();
    loop {
        buf.clear();
        match reader.read_until(b'\n', &mut buf) {
            Ok(0) | Err(_) => return,
            Ok(_) => {
                let text = String::from_utf8_lossy(&buf);
                // sd.cpp redraws progress bars with '\r'; keep only the last frame.
                if let Some(line) = text.split('\r').map(str::trim_end).filter(|l| !l.is_empty()).last() {
                    if tx.send(EngineEvent::Log(line.to_owned())).is_err() {
                        return;
                    }
                }
            }
        }
    }
}

/// `sd-server --list-devices` ("name<TAB>description" per line on stdout; the backends'
/// own log lines go to stderr and are dropped). A list with a GPU is kept for the rest of
/// the run, so engine restarts don't pay for the device scan again. Gives up after 60 s.
fn list_devices(exe: &std::path::Path, dir: &std::path::Path) -> Result<String> {
    static CACHE: std::sync::Mutex<Vec<(PathBuf, String)>> = std::sync::Mutex::new(Vec::new());
    if let Some((_, l)) = CACHE.lock().ok().and_then(|c| c.iter().find(|(p, _)| p == exe).cloned()) {
        return Ok(l);
    }
    let mut cmd = Command::new(exe);
    cmd.current_dir(dir).arg("--list-devices").stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }
    #[cfg(target_os = "linux")]
    die_with_parent(&mut cmd);
    let mut child = cmd.spawn().context(t!("sd-server 실행 실패", "Failed to start sd-server"))?;
    let mut out = child.stdout.take().ok_or_else(|| anyhow!("no stdout"))?;
    let reader = std::thread::spawn(move || {
        let mut s = String::new();
        let _ = out.read_to_string(&mut s);
        s
    });
    let start = Instant::now();
    loop {
        if child.try_wait()?.is_some() {
            break;
        }
        if start.elapsed() > Duration::from_secs(60) {
            let _ = child.kill();
            let _ = child.wait();
            bail!("{}", t!("엔진이 GPU 목록을 60초 안에 내놓지 않았습니다.", "The engine did not list its GPUs within 60 s."));
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    let list = reader.join().unwrap_or_default();
    if list.lines().any(|l| l.contains('\t') && !l.starts_with("CPU")) {
        if let Ok(mut c) = CACHE.lock() {
            c.push((exe.to_path_buf(), list.clone()));
        }
    }
    Ok(list)
}

/// Splits a user-supplied argument string, honouring double quotes.
fn split_args(s: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut quoted = false;
    for c in s.chars() {
        match c {
            '"' => quoted = !quoted,
            c if c.is_whitespace() && !quoted => {
                if !cur.is_empty() {
                    out.push(std::mem::take(&mut cur));
                }
            }
            c => cur.push(c),
        }
    }
    if !cur.is_empty() {
        out.push(cur);
    }
    out
}

/// Puts a helper process (sd-server, the photo captioner) in the app's Job Object, so
/// Windows kills it if this app exits or crashes instead of leaving an orphan holding
/// gigabytes of VRAM/RAM. One job for the whole app: its handle is created once and
/// closes (killing the members) only when the app ends.
#[cfg(windows)]
pub fn kill_with_parent(child: &Child) {
    use std::os::windows::io::AsRawHandle;
    use windows_sys::Win32::System::JobObjects::*;
    static JOB: std::sync::OnceLock<usize> = std::sync::OnceLock::new();
    let job = *JOB.get_or_init(|| unsafe {
        let job = CreateJobObjectW(std::ptr::null(), std::ptr::null());
        if job.is_null() {
            return 0;
        }
        let mut info: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = std::mem::zeroed();
        info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
        SetInformationJobObject(
            job,
            JobObjectExtendedLimitInformation,
            &info as *const _ as *const _,
            std::mem::size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
        );
        job as usize
    });
    if job != 0 {
        unsafe {
            AssignProcessToJobObject(job as _, child.as_raw_handle() as _);
        }
    }
}

/// Linux counterpart of `kill_with_parent`: the kernel sends the child SIGKILL when the
/// thread that spawned it ends (`PR_SET_PDEATHSIG`), which includes this app exiting or
/// crashing. Both callers spawn from a thread that lives as long as the child (the engine
/// worker; the captioner's own thread, which waits for it), so it never fires early.
#[cfg(target_os = "linux")]
pub fn die_with_parent(cmd: &mut Command) {
    use std::os::unix::process::CommandExt;
    let parent = std::process::id() as libc::pid_t;
    // SAFETY: the closure runs in the forked child before exec and only makes
    // async-signal-safe calls (prctl, getppid, _exit).
    unsafe {
        cmd.pre_exec(move || {
            if libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGKILL as libc::c_ulong, 0, 0, 0) != 0 {
                return Err(std::io::Error::last_os_error());
            }
            // The app died between fork and prctl: don't outlive it.
            if libc::getppid() != parent {
                libc::_exit(1);
            }
            Ok(())
        });
    }
}

#[cfg(test)]
mod ram_tests {
    use super::weights_fit_in_ram;
    const GB: u64 = 1 << 30;

    #[test]
    fn text_encoder_leaves_ram_when_too_little_is_free() {
        let weights = 10 * GB;
        assert!(weights_fit_in_ram(Some(32 * GB), Some(20 * GB), weights));
        // 16 GB PC with a browser open: 8.7 GB free → read from disk.
        assert!(!weights_fit_in_ram(Some(16 * GB), Some(8 * GB + (700 << 20)), weights));
        // 8 GB PC: always from disk.
        assert!(!weights_fit_in_ram(Some(8 * GB), Some(7 * GB), 1));
        // Unknown figures keep the old behaviour.
        assert!(weights_fit_in_ram(None, None, weights));
    }

    #[test]
    fn apu_keeps_one_copy_on_the_gpu() {
        use super::unified_placement;
        let (weights, image) = (10 * GB, 5 * GB);
        // 12 GiB GTT on a 16 GB box with plenty free: everything on the GPU.
        assert_eq!(unified_placement("Vulkan0", 12 * GB, true, weights, image), ("Vulkan0", "Vulkan0"));
        // Little RAM free, or shared memory short of all weights: the text encoder from disk.
        assert_eq!(unified_placement("Vulkan0", 12 * GB, false, weights, image), ("disk", "Vulkan0"));
        assert_eq!(unified_placement("Vulkan0", 8 * GB, true, weights, image), ("disk", "Vulkan0"));
        // Not even the image model fits the shared memory: both from disk.
        assert_eq!(unified_placement("Vulkan0", 6 * GB, false, weights, image), ("disk", "disk"));
    }
}

#[cfg(test)]
mod outpaint_tests {
    use super::{border_left_blank, composite_kept, guided_canvas, guided_finish, outpaint_canvas, png_with_text_of};

    /// Real outputs from the outpaint tests (scratch files, not in the repo):
    /// `QIS_BLANK_CASES="file|w0,h0|l,r,t,b|expect;..." cargo test blank_on_real -- --ignored`
    #[test]
    #[ignore]
    fn blank_detector_on_real_outputs() {
        for case in std::env::var("QIS_BLANK_CASES").unwrap().split(';').filter(|c| !c.is_empty()) {
            let f: Vec<&str> = case.split('|').collect();
            let img = image::open(f[0]).unwrap_or_else(|e| panic!("{}: {e}", f[0])).into_rgba8();
            let d: Vec<u32> = f[1].split(',').map(|v| v.parse().unwrap()).collect();
            let p: Vec<f32> = f[2].split(',').map(|v| v.parse().unwrap()).collect();
            let got = border_left_blank(&img, (d[0], d[1]), [p[0], p[1], p[2], p[3]]);
            assert_eq!(got, f[3] == "blank", "{}", f[0]);
        }
    }

    /// Dev aid: `QIS_CANVAS_SRC=img.png QIS_CANVAS_OUT=dir cargo test dump_guided -- --ignored`
    #[test]
    #[ignore]
    fn dump_guided_canvas() {
        let src = image::open(std::env::var("QIS_CANVAS_SRC").unwrap()).unwrap().into_rgba8();
        let out = std::path::PathBuf::from(std::env::var("QIS_CANVAS_OUT").unwrap());
        let (r, i, m) = guided_canvas(&src, [0.6, 0.6, 0.0, 0.0], 1312, 800);
        r.save(out.join("rs_ref.png")).unwrap();
        i.save(out.join("rs_init.png")).unwrap();
        m.save(out.join("rs_mask.png")).unwrap();
    }

    #[test]
    fn detects_a_border_left_blank_next_to_a_detailed_picture() {
        let p = [0.5, 0.5, 0.0, 0.0];
        // A busy source: 24 px checker.
        let busy = |x: u32, y: u32| if (x / 24 + y / 24) % 2 == 0 { 40u8 } else { 200u8 };
        let src = image::RgbaImage::from_fn(400, 400, |x, y| {
            let v = busy(x, y);
            image::Rgba([v, v, v, 255])
        });
        let (reference, _, _) = guided_canvas(&src, p, 1024, 512);
        assert!(border_left_blank(&reference, (400, 400), p), "unfilled grey border");
        // A drawn continuation has structure too (and the model's fine grain).
        let grain = |x: u32, y: u32| ((x.wrapping_mul(7919) ^ y.wrapping_mul(104_729)) % 9) as u8;
        let filled = image::RgbaImage::from_fn(1024, 512, |x, y| {
            let v = busy(x, y) + grain(x, y);
            image::Rgba([v, v / 2, v, 255])
        });
        assert!(!border_left_blank(&filled, (400, 400), p));
        // A plain white studio continuing a plain picture is a valid fill.
        let plain = image::RgbaImage::from_pixel(1024, 512, image::Rgba([250, 250, 250, 255]));
        assert!(!border_left_blank(&plain, (400, 400), p));
    }

    #[test]
    fn guided_canvas_places_source_and_masks_new_area() {
        let src = image::RgbaImage::from_pixel(400, 400, image::Rgba([10, 200, 10, 255]));
        // 1:1 → twice as wide, centred: source fills the middle half of a 1024×512 frame.
        let (reference, init, mask) = guided_canvas(&src, [0.5, 0.5, 0.0, 0.0], 1024, 512);
        assert_eq!(reference.dimensions(), (1024, 512));
        assert_eq!(*reference.get_pixel(10, 256), image::Rgba([128, 128, 128, 255]), "new area is flat grey");
        assert_eq!(reference.get_pixel(512, 256)[1], 200, "source in the middle");
        assert_eq!(init.get_pixel(512, 256)[1], 200);
        assert_eq!(mask.get_pixel(10, 256)[0], 255, "new area repainted");
        assert_eq!(mask.get_pixel(512, 256)[0], 0, "source kept");
        assert_eq!(mask.get_pixel(512, 2)[0], 0, "closed top edge kept (no overlap there)");
    }

    /// Runs the whole guided geometry (plan → canvas → finish, pass after pass) over many
    /// source sizes and expansions, like the engine does, and must not panic. A panic here
    /// kills the engine worker: the job never finishes and nothing reaches the gallery.
    #[test]
    fn guided_passes_never_panic() {
        let sizes = [(1434, 1894), (896, 1184), (1024, 1024), (1376, 768), (768, 1024), (2131, 1190), (513, 977)];
        let pads = [
            [0.4, 0.4, 0.25, 0.0],
            [0.5, 0.5, 0.5, 0.5],
            [0.39, 0.39, 0.0, 0.0],
            [1.0, 1.0, 0.0, 0.0],
            [0.0, 0.0, 0.25, 0.35],
            [0.05, 0.0, 0.0, 0.0],
            [0.3, 0.1, 0.2, 0.0],
            [1.0, 1.0, 1.0, 1.0],
        ];
        for &(w0, h0) in &sizes {
            for p in pads {
                let passes = crate::config::plan_guided_passes(p, crate::config::GUIDED_MAX_AREA);
                let mut cur = image::RgbaImage::from_pixel(w0, h0, image::Rgba([90, 120, 160, 255]));
                for (i, pass) in passes.iter().enumerate() {
                    let (cw, ch) = crate::config::padded_size(cur.width(), cur.height(), *pass);
                    let (w, h) = super::same_area_dims(cw, ch, 1024, 1024);
                    let _ = guided_canvas(&cur, *pass, w, h);
                    let result = image::RgbaImage::from_pixel(w, h, image::Rgba([200, 10, 10, 255]));
                    let _ = super::border_left_blank(&result, cur.dimensions(), *pass);
                    cur = guided_finish(&result, &cur, *pass);
                    assert!(cur.width() >= w && cur.height() >= h, "{w0}x{h0} {p:?} pass {i}");
                }
            }
        }
    }

    #[test]
    fn guided_finish_restores_original_at_higher_resolution() {
        let src = image::RgbaImage::from_pixel(1024, 1024, image::Rgba([10, 200, 10, 255]));
        // Generated at 1024×512: source shown at 512×512 (scale 0.5) → finish enlarges 1.6×.
        let result = image::RgbaImage::from_pixel(1024, 512, image::Rgba([200, 10, 10, 255]));
        let out = guided_finish(&result, &src, [0.5, 0.5, 0.0, 0.0]);
        assert_eq!(out.dimensions(), (1638, 819));
        assert_eq!(out.get_pixel(819, 410)[1], 200, "centre is the original");
        assert_eq!(out.get_pixel(5, 410)[0], 200, "new area keeps the generated pixels");
        // The seam fades: just inside the source's left edge it's a blend.
        let edge = out.get_pixel(410 + 2, 410);
        assert!(edge[0] > 10 && edge[0] < 200, "{edge:?}");
    }

    #[test]
    fn composite_restores_kept_pixels() {
        let prev = image::RgbaImage::from_pixel(400, 300, image::Rgba([10, 200, 10, 255]));
        let p = [0.5, 0.5, 0.0, 0.0];
        // A "generated" result that drifted everywhere (all red), at a smaller output size.
        let result = image::RgbaImage::from_pixel(512, 192, image::Rgba([200, 10, 10, 255]));
        let out = composite_kept(&result, &prev, p);
        assert_eq!(out.dimensions(), (512, 192));
        assert_eq!(out.get_pixel(256, 96)[1], 200, "centre must come back from prev");
        assert_eq!(out.get_pixel(5, 96)[0], 200, "new area keeps the generated pixels");
    }

    #[test]
    fn text_chunks_are_carried_over() {
        let img = image::RgbaImage::from_pixel(8, 8, image::Rgba([1, 2, 3, 255]));
        let text = b"parameters\0hello, SDCPP: {\"prompt\":\"x\",\"sample_params\":{}}";
        let mut meta = b"\x89PNG\r\n\x1a\n".to_vec();
        meta.extend((text.len() as u32).to_be_bytes());
        meta.extend(b"tEXt");
        meta.extend(text);
        meta.extend(crc32fast::hash(&[b"tEXt".as_slice(), text].concat()).to_be_bytes());
        let out = png_with_text_of(&img, &meta).unwrap();
        assert!(image::load_from_memory(&out).is_ok(), "still a valid PNG");
        let p = std::env::temp_dir().join("qis_text_chunk.png");
        std::fs::write(&p, &out).unwrap();
        assert_eq!(crate::metadata::read(&p).map(|r| r.prompt), Some("x".to_owned()));
        let _ = std::fs::remove_file(p);
    }

    #[test]
    fn canvas_and_mask_layout() {
        let src = image::RgbaImage::from_pixel(400, 300, image::Rgba([200, 10, 10, 255]));
        let (canvas, mask) = outpaint_canvas(&src, [0.5, 0.5, 0.0, 0.0]);
        assert_eq!(canvas.dimensions(), (800, 300));
        assert_eq!(mask.dimensions(), (800, 300));
        // New area is repainted, the middle of the original is kept, the original is placed at x=200.
        assert_eq!(mask.get_pixel(20, 150)[0], 255);
        assert_eq!(mask.get_pixel(400, 150)[0], 0);
        assert_eq!(canvas.get_pixel(400, 150), &image::Rgba([200, 10, 10, 255]));

        // Optional: dump the real canvas/mask for an engine check.
        if let (Some(src_path), Some(out)) = (std::env::var_os("QIS_OUTPAINT_SRC"), std::env::var_os("QIS_OUTPAINT_OUT")) {
            let img = image::open(src_path).unwrap().to_rgba8();
            let (c, m) = outpaint_canvas(&img, [0.375, 0.375, 0.0, 0.0]);
            let out = std::path::PathBuf::from(out);
            c.save(out.join("rs_canvas.png")).unwrap();
            m.save(out.join("rs_mask.png")).unwrap();
        }
    }
}
