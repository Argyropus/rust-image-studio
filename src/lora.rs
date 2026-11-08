//! LoRA files under `models/loras`, plus the recommended ones we know how to drive.

use crate::config::ModelSource;
use serde::{Deserialize, Serialize};
use std::path::Path;

pub const LIGHTNING_FILE: &str = "qwen21_lightning_4step.safetensors";
pub const CONSISTENCY_FILE: &str = "qwen21_consistency_edit.safetensors";
pub const ANGLES_FILE: &str = "qwen21_multiple_angles_v2.safetensors";

/// LightX2V Lightning: DMD-distilled 4-step LoRA (text-to-image and editing).
pub const LIGHTNING_SOURCE: ModelSource = ModelSource {
    label: "Lightning 4단계 고속 LoRA",
    label_en: "Lightning 4-step speed LoRA",
    file: LIGHTNING_FILE,
    url: "https://huggingface.co/lightx2v/Qwen-Image-2.1-Lightning/resolve/main/pytorch_lora_weights.safetensors",
    approx_bytes: 67_144_280,
    unzip: false,
};

/// ausboss Consistency (step 1500): keeps edits on the source image's frame.
pub const CONSISTENCY_SOURCE: ModelSource = ModelSource {
    label: "편집 구도 고정 LoRA",
    label_en: "Edit consistency LoRA",
    file: CONSISTENCY_FILE,
    url: "https://huggingface.co/ausboss/Qwen-Image-2.1-Consistency-LoRA/resolve/main/qwen-image-2.1-consistency.safetensors",
    approx_bytes: 159_436_496,
    unzip: false,
};

/// akhaliq Multiple-Angles v2 (step 1500): re-shoots the subject from a chosen camera angle.
pub const ANGLES_SOURCE: ModelSource = ModelSource {
    label: "다각도 재촬영 LoRA",
    label_en: "Multiple-angles LoRA",
    file: ANGLES_FILE,
    url: "https://huggingface.co/akhaliq/Qwen-Image-2.1-Multiple-Angles-LoRA/resolve/main/checkpoints_v2/angles_v2_full_qwen21_multiple_angles_v2_qwen21_multiple_angles_v2_000001500.safetensors",
    approx_bytes: 318_819_792,
    unzip: false,
};

/// Recommended LoRAs offered for download, with a one-line description.
pub const RECOMMENDED: [(&ModelSource, (&str, &str)); 3] = [
    (&LIGHTNING_SOURCE, ("4단계로 생성 · 속도와 화질의 중간", "Generates in 4 steps · between speed and quality")),
    (&CONSISTENCY_SOURCE, ("편집 때 구도·위치가 틀어지지 않게 고정", "Keeps framing and positions from drifting in edits")),
    (&ANGLES_SOURCE, ("제품·인물을 다른 카메라 각도에서 다시 촬영", "Re-shoots a product or person from another camera angle")),
];

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Kind {
    /// 4-step distillation.
    Lightning,
    /// Edit-only: keeps the edit on the source frame.
    Consistency,
    /// Edit-only: camera-angle control, prompt `<mva> {azimuth}, {elevation}[ close-up]`.
    Angles,
    Other,
}

pub fn kind(file: &str) -> Kind {
    let f = file.to_ascii_lowercase();
    if f.contains("lightning") {
        Kind::Lightning
    } else if f.contains("consistency") {
        Kind::Consistency
    } else if f.contains("multiple_angles") || f.contains("multiple-angles") {
        Kind::Angles
    } else {
        Kind::Other
    }
}

/// Few-step distillation LoRAs that need their own schedule.
pub fn is_speed(file: &str) -> bool {
    kind(file) == Kind::Lightning
}

/// LoRAs trained on edit pairs: only meaningful in edit mode.
pub fn is_edit_only(file: &str) -> bool {
    matches!(kind(file), Kind::Consistency | Kind::Angles)
}

/// Two LoRAs that should not be on together: two speed schedules, or "keep the frame"
/// together with "move the camera".
pub fn conflicts(a: &str, b: &str) -> bool {
    let (ka, kb) = (kind(a), kind(b));
    a != b
        && ((is_speed(a) && is_speed(b))
            || matches!((ka, kb), (Kind::Consistency, Kind::Angles) | (Kind::Angles, Kind::Consistency)))
}

/// Lightning's training schedule: sigmas [1, .75, .5, .25] through the exponential
/// dynamic shift (base 0.5 @ 256 tokens → max 0.9 @ 8192 tokens), then 0.
pub fn lightning_sigmas(width: u32, height: u32) -> Vec<f32> {
    let seq = (width / 16) as f32 * (height / 16) as f32;
    let mu = 0.5 + (seq - 256.0) / (8192.0 - 256.0) * (0.9 - 0.5);
    let em = mu.exp();
    let mut v: Vec<f32> = [1.0f32, 0.75, 0.5, 0.25].iter().map(|&t| em / (em + (1.0 / t - 1.0))).collect();
    v.push(0.0);
    v
}

/// Step schedule required by a speed LoRA, if `file` is one.
pub fn speed_sigmas(file: &str, width: u32, height: u32) -> Option<Vec<f32>> {
    match kind(file) {
        Kind::Lightning => Some(lightning_sigmas(width, height)),
        _ => None,
    }
}

/// Camera azimuths of the Multiple-Angles LoRA (30° steps): (prompt label, Korean label).
pub const AZIMUTHS: [(&str, &str); 12] = [
    ("front view", "정면"),
    ("front-right view", "정면 오른쪽 30°"),
    ("front-right quarter view", "오른쪽 사선 60°"),
    ("right side view", "오른쪽 측면"),
    ("back-right quarter view", "뒤 오른쪽 사선 120°"),
    ("back-right view", "뒤 오른쪽 150°"),
    ("back view", "뒷면"),
    ("back-left view", "뒤 왼쪽 210°"),
    ("back-left quarter view", "뒤 왼쪽 사선 240°"),
    ("left side view", "왼쪽 측면"),
    ("front-left quarter view", "왼쪽 사선 300°"),
    ("front-left view", "정면 왼쪽 330°"),
];

/// Camera elevations: (prompt label, Korean label).
pub const ELEVATIONS: [(&str, &str); 4] = [
    ("eye-level shot", "눈높이"),
    ("elevated shot", "살짝 위 30°"),
    ("high-angle shot", "높은 곳 60°"),
    ("top-down shot", "바로 위 90°"),
];

/// English labels for the dial, same order as `AZIMUTHS` / `ELEVATIONS`.
const AZIMUTHS_EN: [&str; 12] = [
    "Front", "Front right 30°", "Right diagonal 60°", "Right side", "Back right diagonal 120°", "Back right 150°",
    "Back", "Back left 210°", "Back left diagonal 240°", "Left side", "Left diagonal 300°", "Front left 330°",
];
const ELEVATIONS_EN: [&str; 4] = ["Eye level", "Slightly above 30°", "High 60°", "Straight above 90°"];

/// Dial label of an azimuth in the UI language.
pub fn az_label(i: usize) -> &'static str {
    if crate::i18n::en() { AZIMUTHS_EN[i % 12] } else { AZIMUTHS[i % 12].1 }
}

/// Label of an elevation in the UI language.
pub fn el_label(i: usize) -> &'static str {
    if crate::i18n::en() { ELEVATIONS_EN[i % 4] } else { ELEVATIONS[i % 4].1 }
}

pub fn angles_prompt(az: usize, el: usize, close_up: bool) -> String {
    format!(
        "<mva> {}, {}{}",
        AZIMUTHS[az % 12].0,
        ELEVATIONS[el % 4].0,
        if close_up { " close-up" } else { "" }
    )
}

#[derive(Clone, Serialize, Deserialize, PartialEq)]
pub struct LoraSel {
    pub file: String,
    pub weight: f32,
    pub enabled: bool,
}

/// LoRA file names (sorted) found in `dir`.
pub fn scan(dir: &Path) -> Vec<String> {
    let mut v: Vec<String> = std::fs::read_dir(dir)
        .map(|rd| {
            rd.filter_map(|e| e.ok())
                .map(|e| e.path())
                .filter(|p| {
                    p.extension()
                        .and_then(|e| e.to_str())
                        .is_some_and(|e| matches!(e.to_ascii_lowercase().as_str(), "safetensors" | "gguf" | "ckpt"))
                })
                .filter_map(|p| p.file_name().map(|n| n.to_string_lossy().into_owned()))
                .collect()
        })
        .unwrap_or_default();
    v.sort();
    v
}

pub fn display_name(file: &str) -> String {
    match file {
        LIGHTNING_FILE => return t!("Lightning 4단계 고속", "Lightning 4-step").to_owned(),
        CONSISTENCY_FILE => return t!("편집 구도 고정", "Edit consistency").to_owned(),
        ANGLES_FILE => return t!("다각도 재촬영", "Multiple angles").to_owned(),
        _ => {}
    }
    Path::new(file).file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_else(|| file.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kinds_and_conflicts() {
        assert_eq!(kind(LIGHTNING_FILE), Kind::Lightning);
        assert_eq!(kind(CONSISTENCY_FILE), Kind::Consistency);
        assert_eq!(kind(ANGLES_FILE), Kind::Angles);
        assert!(conflicts(CONSISTENCY_FILE, ANGLES_FILE));
        assert!(!conflicts(LIGHTNING_FILE, CONSISTENCY_FILE));
        assert!(!conflicts(LIGHTNING_FILE, LIGHTNING_FILE));
    }

    #[test]
    fn lightning_schedule_is_monotonic() {
        let s = lightning_sigmas(1024, 1024);
        assert_eq!(s.len(), 5);
        assert!((s[0] - 1.0).abs() < 1e-6 && s[4] == 0.0);
        assert!(s.windows(2).all(|w| w[0] > w[1]));
        // 1024² → 4096 tokens → mu ≈ 0.69 → second sigma ≈ 0.857
        assert!((s[1] - 0.857).abs() < 0.01, "{s:?}");
    }

    #[test]
    fn angles_prompt_format() {
        assert_eq!(angles_prompt(3, 2, true), "<mva> right side view, high-angle shot close-up");
        assert_eq!(angles_prompt(0, 0, false), "<mva> front view, eye-level shot");
    }
}
