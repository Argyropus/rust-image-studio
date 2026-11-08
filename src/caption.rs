//! Scene description of a photo, for the expand prompt. Runs llama.cpp's multimodal CLI
//! (runtime/llama/llama-mtmd-cli[.exe], Vulkan build b12182) on the GPU with the Qwen3-VL
//! weights the app already has (text encoder + vision projector).
//!
//! It needs up to ~6.5 GB of VRAM (peak 7.7 GB used on the 8 GB card with the image engine
//! idle), so the app only runs it while the image engine is idle and holds generation
//! until it is done. Measured with the image engine loaded and idle: ~18–19 s per photo
//! (almost all of it loading weights; encode 0.3 s, text ~2 s), and the next image took
//! its usual ~18 s. The CPU build took ~58 s and slowed the next image to 39 s.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Arc;
use std::sync::mpsc::Sender;
use std::time::{Duration, Instant};

/// What the model is asked for: the surroundings to continue, not who is in the picture.
const ASK: &str = "이 사진을 바깥쪽으로 넓혀 그리려고 합니다. 사진 속 장소와 배경(바닥, 벽, 창, 하늘, 가구, 주변 사물), \
조명과 색감, 사진 느낌을 한국어로 쉼표로 이어진 짧은 묘사 한 줄(40~80자)로 써 주세요. 묘사만 출력하세요.";

/// The same request for the English UI (the description lands in the prompt box).
const ASK_EN: &str = "I want to extend this photo outward. Describe the place and background in the photo (floor, walls, \
windows, sky, furniture, surrounding objects), the lighting, colors and photographic feel as one short comma-separated line \
in English (15 to 30 words). Output only the description.";

/// Seconds after which a stuck run is killed.
const TIMEOUT: Duration = Duration::from_secs(240);

pub struct Captioner {
    pub exe: PathBuf,
    pub model: PathBuf,
    pub mmproj: PathBuf,
}

impl Captioner {
    /// Available when the llama.cpp CLI and both model files exist.
    pub fn find(runtime_dir: &Path, model: &Path, mmproj: &Path) -> Option<Self> {
        let exe = runtime_dir.join("llama").join(crate::config::CAPTION_EXE);
        (exe.is_file() && model.is_file() && mmproj.is_file()).then(|| Self {
            exe,
            model: model.to_path_buf(),
            mmproj: mmproj.to_path_buf(),
        })
    }

    /// Describes `img` on a background thread; sends (request id, result) when done.
    pub fn spawn(&self, id: u64, img: Arc<image::RgbaImage>, tx: Sender<(u64, Result<String, String>)>, ctx: egui::Context) {
        let (exe, model, mmproj) = (self.exe.clone(), self.model.clone(), self.mmproj.clone());
        std::thread::spawn(move || {
            let res = run(id, &exe, &model, &mmproj, &img);
            let _ = tx.send((id, res));
            ctx.request_repaint();
        });
    }
}

fn run(id: u64, exe: &Path, model: &Path, mmproj: &Path, img: &image::RgbaImage) -> Result<String, String> {
    // A 448 px JPEG keeps the vision encoder quick (≤ 256 image tokens). Scaled straight
    // from the borrowed picture: no full-size copy of a 12 MP photo.
    let (w, h) = img.dimensions();
    let s = (448.0 / w.max(h) as f32).min(1.0);
    let (tw, th) = (((w as f32 * s).round() as u32).max(1), ((h as f32 * s).round() as u32).max(1));
    let small = image::DynamicImage::ImageRgba8(image::imageops::thumbnail(img, tw, th)).to_rgb8();
    let path = std::env::temp_dir().join(format!("qis_caption_{}_{id}.jpg", std::process::id()));
    small.save(&path).map_err(|e| tf!("임시 이미지 저장 실패: {}", "Could not save the temporary image: {}", e))?;
    let mut cmd = Command::new(exe);
    cmd.arg("-m")
        .arg(model)
        .arg("--mmproj")
        .arg(mmproj)
        .arg("--image")
        .arg(&path)
        .arg("-p")
        .arg(if crate::i18n::en() { ASK_EN } else { ASK })
        .args(["-n", "120", "--temp", "0.2", "-c", "2048", "--image-max-tokens", "256", "--no-warmup"])
        // All layers on the GPU; DirectIO reads the weights without filling the file cache,
        // so the image engine's RAM-held weights are not pushed out.
        .args(["-ngl", "99", "-lm", "dio"])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    #[cfg(target_os = "linux")]
    crate::engine::die_with_parent(&mut cmd);
    let mut child = cmd.spawn().map_err(|e| tf!("설명 엔진을 실행할 수 없습니다: {}", "Could not start the description engine: {}", e))?;
    // Dies with the app: a leftover captioner would hold ~7 GB of VRAM.
    #[cfg(windows)]
    crate::engine::kill_with_parent(&child);
    let mut out = child.stdout.take().ok_or(t!("출력 없음", "No output"))?;
    let mut err = child.stderr.take().ok_or(t!("출력 없음", "No output"))?;
    // Read on helper threads so a hung process can still be killed after the timeout
    // (and neither pipe fills up and blocks it).
    let reader = std::thread::spawn(move || {
        let mut s = String::new();
        let _ = std::io::Read::read_to_string(&mut out, &mut s);
        s
    });
    let err_reader = std::thread::spawn(move || {
        let mut s = String::new();
        let _ = std::io::Read::read_to_string(&mut err, &mut s);
        s
    });
    let start = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) if start.elapsed() > TIMEOUT => {
                let _ = child.kill();
                let _ = std::fs::remove_file(&path);
                return Err(t!("사진 설명이 너무 오래 걸려 중단했습니다", "The photo description took too long and was stopped").into());
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(200)),
            Err(e) => return Err(e.to_string()),
        }
    }
    let _ = std::fs::remove_file(&path);
    let text = reader.join().unwrap_or_default();
    let log = err_reader.join().unwrap_or_default();
    clean(&text).ok_or_else(|| {
        // The last line llama.cpp flagged as an error (e.g. out of GPU memory), if any.
        let why = log
            .lines()
            .rev()
            .find(|l| l.to_ascii_lowercase().contains("error") || l.contains("failed"))
            .map(|l| l.trim().chars().take(160).collect::<String>());
        match why {
            Some(w) => tf!("사진 설명을 만들지 못했습니다: {}", "Could not describe the photo: {}", w),
            None => t!("사진 설명을 만들지 못했습니다", "Could not describe the photo").into(),
        }
    })
}

/// One tidy line from the model's answer.
fn clean(raw: &str) -> Option<String> {
    let line = raw.lines().map(str::trim).filter(|l| !l.is_empty()).collect::<Vec<_>>().join(" ");
    let line = line.trim_matches(|c: char| c == '"' || c == '“' || c == '”' || c == '\'' || c.is_whitespace());
    let line: String = line.chars().take(300).collect();
    (line.chars().count() >= 6).then_some(line)
}

#[cfg(test)]
mod tests {
    #[test]
    fn cleans_the_answer() {
        assert_eq!(
            super::clean("\n\n밝은 창가 옆 흰색 바닥, 자연광\n\n").as_deref(),
            Some("밝은 창가 옆 흰색 바닥, 자연광")
        );
        assert_eq!(super::clean("\"따뜻한 카페 실내, 오후 햇살\"").as_deref(), Some("따뜻한 카페 실내, 오후 햇살"));
        assert_eq!(super::clean("  \n"), None);
    }
}
