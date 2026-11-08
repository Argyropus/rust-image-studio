//! Persistent user settings and generation parameters.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Turbo's fixed 8-step schedule (plus the terminal 0), from the model card / sd.cpp recipe.
pub const TURBO_SIGMAS: [f32; 9] = [
    1.0, 0.978453, 0.95418, 0.926626, 0.89508, 0.845148, 0.704534, 0.414568, 0.0,
];

pub const DIFFUSION_FILE: &str = "qwen-image-2.1-turbo-Q4_K_M.gguf";
pub const TEXT_ENCODER_FILE: &str = "Qwen3-VL-8B-Instruct-UD-Q4_K_XL.gguf";
pub const VAE_FILE: &str = "qwen_image_2.1_vae_bf16.safetensors";
pub const VISION_FILE: &str = "mmproj-Qwen3-VL-8B-F16.gguf";
pub const INT8_FILE: &str = "qwen_image_2.1_turbo_int8_convrot.safetensors";

/// Optional faster/higher-fidelity diffusion weights (INT8 tensor-core GEMM on RTX 20+).
pub const INT8_SOURCE: ModelSource = ModelSource {
    label: "이미지 생성 모델 (Turbo INT8 ConvRot, 텐서코어)",
    label_en: "Image model (Turbo INT8 ConvRot, tensor cores)",
    file: INT8_FILE,
    url: "https://huggingface.co/Comfy-Org/Qwen-Image-2.1/resolve/main/diffusion_models/qwen_image_2.1_turbo_int8_convrot.safetensors",
    approx_bytes: 7_256_787_829,
    unzip: false,
};

/// Optional: only needed for instruction-based image editing (reference images).
pub const VISION_SOURCE: ModelSource = ModelSource {
    label: "비전 모듈 (Qwen3-VL mmproj F16, 편집 모드용)",
    label_en: "Vision module (Qwen3-VL mmproj F16, for Edit mode)",
    file: VISION_FILE,
    url: "https://huggingface.co/unsloth/Qwen3-VL-8B-Instruct-GGUF/resolve/main/mmproj-F16.gguf",
    approx_bytes: 1_159_030_336,
    unzip: false,
};

/// File name of the engine executable (`sd-server.exe` on Windows, `sd-server` elsewhere).
pub const SERVER_EXE: &str = if cfg!(windows) { "sd-server.exe" } else { "sd-server" };
/// File name of the photo describer's executable.
pub const CAPTION_EXE: &str = if cfg!(windows) { "llama-mtmd-cli.exe" } else { "llama-mtmd-cli" };

/// Microsoft Visual C++ 2015–2022 Redistributable (x64): the engine and describer DLLs
/// need MSVCP140 / VCRUNTIME140_1 / VCOMP140, which a clean Windows install lacks.
/// The setup downloads Microsoft's installer and runs it (one UAC prompt).
#[cfg(windows)]
pub const VCREDIST_SOURCE: ModelSource = ModelSource {
    label: "Visual C++ 런타임 (Microsoft, 설치 시 권한 확인 창이 한 번 뜹니다)",
    label_en: "Visual C++ runtime (Microsoft; asks for permission once to install)",
    file: "vc_redist.x64.exe",
    url: "https://aka.ms/vs/17/release/vc_redist.x64.exe",
    approx_bytes: 25_635_768,
    unzip: false,
};

/// True when the Visual C++ runtime DLLs the engine needs are installed system-wide.
#[cfg(windows)]
pub fn vcredist_installed() -> bool {
    let sys = std::env::var_os("SystemRoot").map(PathBuf::from).unwrap_or_else(|| PathBuf::from(r"C:\Windows")).join("System32");
    ["msvcp140.dll", "vcruntime140_1.dll", "vcomp140.dll"].iter().all(|d| sys.join(d).is_file())
}

/// True when the NVIDIA driver's CUDA library is present (the driver itself can't be
/// installed by the app).
#[cfg(windows)]
pub fn nvidia_driver_present() -> bool {
    let sys = std::env::var_os("SystemRoot").map(PathBuf::from).unwrap_or_else(|| PathBuf::from(r"C:\Windows")).join("System32");
    sys.join("nvcuda.dll").is_file()
}

/// Linux: the NVIDIA kernel driver is loaded (`/proc/driver/nvidia/version`) or its CUDA
/// library `libcuda.so.1` can be loaded (also true under WSL2). Checked once per run.
#[cfg(unix)]
pub fn nvidia_driver_present() -> bool {
    static PRESENT: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *PRESENT.get_or_init(|| Path::new("/proc/driver/nvidia/version").exists() || can_load_library("libcuda.so.1"))
}

/// True when the dynamic loader finds and loads `name` (then unloads it again).
#[cfg(unix)]
pub fn can_load_library(name: &str) -> bool {
    let Ok(c) = std::ffi::CString::new(name) else { return false };
    // SAFETY: `c` is NUL-terminated; a handle we got is closed right away.
    unsafe {
        let h = libc::dlopen(c.as_ptr(), libc::RTLD_LAZY | libc::RTLD_LOCAL);
        if h.is_null() {
            return false;
        }
        libc::dlclose(h);
    }
    true
}

/// What the Linux system lacks for parts of the app: (what, the Ubuntu/Debian package to
/// install, required for image generation). The engine archives bring their own CUDA
/// runtime; the rest comes from the distribution. Checked once per run.
#[cfg(target_os = "linux")]
pub fn missing_system_parts() -> &'static [(&'static str, &'static str, bool)] {
    static MISSING: std::sync::OnceLock<Vec<(&'static str, &'static str, bool)>> = std::sync::OnceLock::new();
    MISSING.get_or_init(|| {
        let mut v = Vec::new();
        // The engine is built on Ubuntu 22.04: it needs glibc 2.35 or newer.
        if glibc_version().is_some_and(|(major, minor)| (major, minor) < (2, 35)) {
            v.push(("glibc 2.35+", "Ubuntu 22.04 / Debian 12 or newer", true));
        }
        if !can_load_library("libstdc++.so.6") {
            v.push(("libstdc++.so.6", "libstdc++6", true));
        }
        if !can_load_library("libvulkan.so.1") {
            v.push(("libvulkan.so.1", "libvulkan1", false));
        }
        v
    })
}

/// The running glibc's version, e.g. (2, 39).
#[cfg(target_os = "linux")]
fn glibc_version() -> Option<(u32, u32)> {
    unsafe extern "C" {
        fn gnu_get_libc_version() -> *const libc::c_char;
    }
    // SAFETY: glibc returns a static NUL-terminated string.
    let v = unsafe { std::ffi::CStr::from_ptr(gnu_get_libc_version()) }.to_str().ok()?;
    let mut it = v.split('.').map(|p| p.parse::<u32>().ok());
    Some((it.next()??, it.next()??))
}

/// stable-diffusion.cpp CUDA 12 build the app was tested with (sd-server.exe + ggml DLLs).
#[cfg(windows)]
pub const ENGINE_SOURCE: ModelSource = ModelSource {
    label: "생성 엔진 (stable-diffusion.cpp, CUDA 12)",
    label_en: "Generation engine (stable-diffusion.cpp, CUDA 12)",
    file: "sd-master-f5df5f8-bin-win-cuda12-x64.zip",
    url: "https://github.com/leejet/stable-diffusion.cpp/releases/download/master-1052-f5df5f8/sd-master-f5df5f8-bin-win-cuda12-x64.zip",
    approx_bytes: 363_293_049,
    unzip: true,
};

/// NVIDIA CUDA 12 runtime DLLs published with that release.
#[cfg(windows)]
pub const CUDART_SOURCE: ModelSource = ModelSource {
    label: "CUDA 런타임 (cudart · cuBLAS)",
    label_en: "CUDA runtime (cudart · cuBLAS)",
    file: "cudart-sd-bin-win-cu12-x64.zip",
    url: "https://github.com/leejet/stable-diffusion.cpp/releases/download/master-1052-f5df5f8/cudart-sd-bin-win-cu12-x64.zip",
    approx_bytes: 563_452_046,
    unzip: true,
};

/// Where the Linux engine is published. stable-diffusion.cpp ships no Linux CUDA build, so
/// this repository builds one (`.github/workflows/linux-engine.yml`,
/// `scripts/build-linux-engine.sh`) and attaches it to this release.
#[cfg(not(windows))]
macro_rules! linux_engine_release {
    () => {
        "https://github.com/hid276/rust-image-studio/releases/download/engine-sd-master-1052-f5df5f8/"
    };
}

/// Approximate archive sizes (progress bar and disk-space check only). The CUDA runtime
/// one is measured from a local build with the CUDA 12.8 packages the workflow installs;
/// the engine one is estimated from the Windows cuda12 zip (same 9 GPU architectures),
/// since the local test build only had one.
#[cfg(not(windows))]
const LINUX_ENGINE_BYTES: u64 = 360_000_000;
#[cfg(not(windows))]
const LINUX_CUDART_BYTES: u64 = 594_371_512;

/// The same stable-diffusion.cpp tag built for Linux with CUDA 12.8 (sd-server, sd-cli,
/// libstable-diffusion.so, ggml backends; RUNPATH=$ORIGIN). Size: estimated; the
/// release's real size replaces it once published.
#[cfg(not(windows))]
pub const ENGINE_SOURCE: ModelSource = ModelSource {
    label: "생성 엔진 (stable-diffusion.cpp, CUDA 12)",
    label_en: "Generation engine (stable-diffusion.cpp, CUDA 12)",
    file: "sd-master-f5df5f8-bin-linux-cuda12-x64.tar.gz",
    url: concat!(linux_engine_release!(), "sd-master-f5df5f8-bin-linux-cuda12-x64.tar.gz"),
    approx_bytes: LINUX_ENGINE_BYTES,
    unzip: true,
};

/// libcudart.so.12, libcublas.so.12 and libcublasLt.so.12 from CUDA 12.8 (redistributable
/// under the CUDA Toolkit EULA), published next to the Linux engine.
#[cfg(not(windows))]
pub const CUDART_SOURCE: ModelSource = ModelSource {
    label: "CUDA 런타임 (cudart · cuBLAS)",
    label_en: "CUDA runtime (cudart · cuBLAS)",
    file: "cudart-sd-bin-linux-cu12-x64.tar.gz",
    url: concat!(linux_engine_release!(), "cudart-sd-bin-linux-cu12-x64.tar.gz"),
    approx_bytes: LINUX_CUDART_BYTES,
    unzip: true,
};

/// llama.cpp Vulkan build: runs the photo describer (llama-mtmd-cli) on the GPU.
#[cfg(windows)]
pub const CAPTION_SOURCE: ModelSource = ModelSource {
    label: "사진 설명 엔진 (llama.cpp, Vulkan)",
    label_en: "Photo describer (llama.cpp, Vulkan)",
    file: "llama-b12182-bin-win-vulkan-x64.zip",
    url: "https://github.com/ggml-org/llama.cpp/releases/download/b12182/llama-b12182-bin-win-vulkan-x64.zip",
    approx_bytes: 37_709_766,
    unzip: true,
};

/// The same llama.cpp release, Ubuntu x64 Vulkan build (a .tar.gz with one top folder,
/// `llama-b12182/`, which extraction strips). Needs `libvulkan1` and the NVIDIA driver's
/// Vulkan ICD at run time.
#[cfg(not(windows))]
pub const CAPTION_SOURCE: ModelSource = ModelSource {
    label: "사진 설명 엔진 (llama.cpp, Vulkan)",
    label_en: "Photo describer (llama.cpp, Vulkan)",
    file: "llama-b12182-bin-ubuntu-vulkan-x64.tar.gz",
    url: "https://github.com/ggml-org/llama.cpp/releases/download/b12182/llama-b12182-bin-ubuntu-vulkan-x64.tar.gz",
    approx_bytes: 35_871_160,
    unzip: true,
};

impl ModelSource {
    /// The label in the UI language.
    pub fn name(&self) -> &'static str {
        if crate::i18n::en() { self.label_en } else { self.label }
    }
}

/// A model file the app can fetch on first run.
pub struct ModelSource {
    pub label: &'static str,
    pub label_en: &'static str,
    pub file: &'static str,
    pub url: &'static str,
    pub approx_bytes: u64,
    /// An archive of program files (.zip, or .tar.gz on Linux), extracted where it was
    /// saved (then deleted).
    pub unzip: bool,
}

pub const MODEL_SOURCES: [ModelSource; 3] = [
    ModelSource {
        label: "이미지 생성 모델 (Qwen-Image-2.1-Turbo Q4_K_M)",
        label_en: "Image model (Qwen-Image-2.1-Turbo Q4_K_M)",
        file: DIFFUSION_FILE,
        url: "https://huggingface.co/unsloth/Qwen-Image-2.1-Turbo-GGUF/resolve/main/qwen-image-2.1-turbo-Q4_K_M.gguf",
        approx_bytes: 4_200_000_000,
        unzip: false,
    },
    ModelSource {
        label: "텍스트 인코더 (Qwen3-VL-8B UD-Q4_K_XL)",
        label_en: "Text encoder (Qwen3-VL-8B UD-Q4_K_XL)",
        file: TEXT_ENCODER_FILE,
        url: "https://huggingface.co/unsloth/Qwen3-VL-8B-Instruct-GGUF/resolve/main/Qwen3-VL-8B-Instruct-UD-Q4_K_XL.gguf",
        approx_bytes: 5_148_699_488,
        unzip: false,
    },
    ModelSource {
        label: "VAE (Qwen-Image-2.1)",
        label_en: "VAE (Qwen-Image-2.1)",
        file: VAE_FILE,
        url: "https://huggingface.co/unsloth/Qwen-Image-2.1-FP8/resolve/main/vae/qwen_image_2.1_vae_bf16.safetensors",
        approx_bytes: 675_508_656,
        unzip: false,
    },
];

/// Which diffusion weights to run.
#[derive(Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum ModelVariant {
    /// GGUF Q4_K_M, 4.2 GB.
    #[default]
    Q4,
    /// INT8 ConvRot, 7.3 GB: measured 1.02 s/step vs 1.66 (RTX 3060 Ti) and closer to BF16.
    Int8,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub runtime_dir: PathBuf,
    pub diffusion_model: PathBuf,
    pub text_encoder: PathBuf,
    pub vae: PathBuf,
    pub vision: PathBuf,
    pub int8_model: PathBuf,
    pub variant: ModelVariant,
    pub output_dir: PathBuf,
    /// Run a tiny generation at startup so the first real image is fast.
    pub warmup: bool,
    pub extra_args: String,
    pub recent_prompts: Vec<String>,
    /// UI language: false = 한국어 (default), true = English.
    pub english: bool,
}

impl Default for Settings {
    fn default() -> Self {
        let base = app_base_dir();
        let models = base.join("models");
        Self {
            runtime_dir: base.join("runtime"),
            diffusion_model: models.join(DIFFUSION_FILE),
            text_encoder: models.join(TEXT_ENCODER_FILE),
            vae: models.join(VAE_FILE),
            vision: models.join(VISION_FILE),
            int8_model: models.join(INT8_FILE),
            variant: ModelVariant::Q4,
            output_dir: base.join("outputs"),
            warmup: true,
            extra_args: String::new(),
            recent_prompts: Vec::new(),
            english: false,
        }
    }
}

impl Settings {
    /// The engine executable: `runtime/sd-server(.exe)`. On Linux a self-built `sd-server`
    /// on PATH is used when the runtime folder has none.
    pub fn server_exe(&self) -> PathBuf {
        let local = self.runtime_dir.join(SERVER_EXE);
        #[cfg(unix)]
        if !local.is_file() {
            if let Some(p) = find_on_path(SERVER_EXE) {
                return p;
            }
        }
        local
    }

    pub fn models_dir(&self) -> PathBuf {
        self.diffusion_model
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| app_base_dir().join("models"))
    }

    pub fn model_path_for(&self, file: &str) -> PathBuf {
        match file {
            DIFFUSION_FILE => self.diffusion_model.clone(),
            TEXT_ENCODER_FILE => self.text_encoder.clone(),
            VISION_FILE => self.vision.clone(),
            INT8_FILE => self.int8_model.clone(),
            _ => self.vae.clone(),
        }
    }

    /// Diffusion weights for the selected variant.
    pub fn active_diffusion(&self) -> &Path {
        match self.variant {
            ModelVariant::Q4 => &self.diffusion_model,
            ModelVariant::Int8 => &self.int8_model,
        }
    }

    /// Required files for the selected variant that are not on disk yet.
    pub fn missing_models(&self) -> Vec<&'static ModelSource> {
        let diffusion: &'static ModelSource = match self.variant {
            ModelVariant::Q4 => &MODEL_SOURCES[0],
            ModelVariant::Int8 => &INT8_SOURCE,
        };
        std::iter::once(diffusion)
            .chain(MODEL_SOURCES[1..].iter())
            .filter(|m| !self.model_path_for(m.file).is_file())
            .collect()
    }

    pub fn variant_label(&self) -> &'static str {
        match self.variant {
            ModelVariant::Q4 => "Q4_K_M",
            ModelVariant::Int8 => t!("INT8 텐서코어", "INT8 tensor cores"),
        }
    }

    /// LoRA folder (`models/loras`), passed to the engine as --lora-model-dir.
    pub fn lora_dir(&self) -> PathBuf {
        self.models_dir().join("loras")
    }

    /// Where a downloadable file goes (zips land in their extraction folder).
    pub fn dest_for(&self, src: &ModelSource) -> PathBuf {
        match src.file {
            #[cfg(windows)]
            f if f == VCREDIST_SOURCE.file => self.runtime_dir.join(f),
            f if f == ENGINE_SOURCE.file || f == CUDART_SOURCE.file => self.runtime_dir.join(f),
            f if f == CAPTION_SOURCE.file => self.runtime_dir.join("llama").join(f),
            f if crate::lora::RECOMMENDED.iter().any(|(s, _)| s.file == f) => self.lora_dir().join(f),
            f => self.model_path_for(f),
        }
    }

    /// True when a downloadable item is in place (for a zip: a file it contains).
    pub fn installed(&self, src: &ModelSource) -> bool {
        match src.file {
            #[cfg(windows)]
            f if f == VCREDIST_SOURCE.file => vcredist_installed(),
            f if f == ENGINE_SOURCE.file => self.server_exe().is_file(),
            #[cfg(windows)]
            f if f == CUDART_SOURCE.file => self.runtime_dir.join("cudart64_12.dll").is_file(),
            // Only our engine archive needs this download. A self-built sd-server, whether on
            // PATH or copied into runtime/ as the build help says, uses the system's CUDA
            // Toolkit; ours is recognised by the shared library it ships next to sd-server.
            #[cfg(not(windows))]
            f if f == CUDART_SOURCE.file => {
                let local = self.runtime_dir.join(SERVER_EXE);
                self.runtime_dir.join("libcublasLt.so.12").is_file()
                    || self.server_exe() != local
                    || (local.is_file() && !self.runtime_dir.join("libstable-diffusion.so").is_file())
            }
            f if f == CAPTION_SOURCE.file => self.runtime_dir.join("llama").join(CAPTION_EXE).is_file(),
            _ => self.dest_for(src).is_file(),
        }
    }

    /// Everything the first run fetches, in download order, with whether it is required.
    /// Required: engine + CUDA runtime + the three model files. Then the extras that the
    /// edit/expand/describe features and the LoRA card use.
    /// On Linux the engine and CUDA runtime come from this repository's own release (see
    /// `linux_engine_release!`); Windows also installs the Visual C++ runtime first.
    pub fn setup_items(&self) -> Vec<(&'static ModelSource, bool)> {
        let diffusion: &'static ModelSource = match self.variant {
            ModelVariant::Q4 => &MODEL_SOURCES[0],
            ModelVariant::Int8 => &INT8_SOURCE,
        };
        let mut v: Vec<(&'static ModelSource, bool)> = Vec::new();
        #[cfg(windows)]
        v.push((&VCREDIST_SOURCE, true));
        v.extend([(&ENGINE_SOURCE, true), (&CUDART_SOURCE, true)]);
        v.push((diffusion, true));
        v.extend(MODEL_SOURCES[1..].iter().map(|m| (m, true)));
        v.push((&VISION_SOURCE, false));
        v.extend(crate::lora::RECOMMENDED.iter().map(|(s, _)| (*s, false)));
        v.push((&CAPTION_SOURCE, false));
        v
    }

    /// Engine, CUDA runtime and models are all present: the app can generate.
    pub fn core_ready(&self) -> bool {
        self.runtime_ok() && self.setup_items().iter().filter(|(_, req)| *req).all(|(s, _)| self.installed(s))
    }

    pub fn vision_ok(&self) -> bool {
        self.vision.is_file()
    }

    pub fn runtime_ok(&self) -> bool {
        self.server_exe().is_file()
    }

    pub fn remember_prompt(&mut self, prompt: &str) {
        let p = prompt.trim();
        if p.is_empty() || crate::safety::blocked(p) {
            return;
        }
        self.recent_prompts.retain(|x| x != p);
        self.recent_prompts.insert(0, p.to_owned());
        self.recent_prompts.truncate(30);
    }
}

/// Free bytes on the drive holding `path` (the nearest existing ancestor).
#[cfg(windows)]
pub fn free_bytes(path: &Path) -> Option<u64> {
    use std::os::windows::ffi::OsStrExt;
    let dir = path.ancestors().find(|p| p.exists())?;
    let wide: Vec<u16> = dir.as_os_str().encode_wide().chain(std::iter::once(0)).collect();
    let mut free = 0u64;
    // SAFETY: `wide` is NUL-terminated and outlives the call; null outputs are allowed.
    let ok = unsafe {
        windows_sys::Win32::Storage::FileSystem::GetDiskFreeSpaceExW(wide.as_ptr(), &mut free, std::ptr::null_mut(), std::ptr::null_mut())
    };
    (ok != 0).then_some(free)
}

/// Free bytes available to this user on the file system holding `path` (`statvfs`).
#[cfg(unix)]
pub fn free_bytes(path: &Path) -> Option<u64> {
    use std::os::unix::ffi::OsStrExt;
    let dir = path.ancestors().find(|p| p.exists())?;
    let c = std::ffi::CString::new(dir.as_os_str().as_bytes()).ok()?;
    let mut st: libc::statvfs = unsafe { std::mem::zeroed() };
    // SAFETY: `c` is NUL-terminated and `st` is a valid out-pointer for the call.
    let ok = unsafe { libc::statvfs(c.as_ptr(), &mut st) } == 0;
    #[allow(clippy::unnecessary_cast)] // the field types differ between libc targets
    ok.then(|| st.f_bavail as u64 * st.f_frsize as u64)
}

#[cfg(not(any(windows, unix)))]
pub fn free_bytes(_path: &Path) -> Option<u64> {
    None
}

/// First executable file called `name` in a PATH directory.
#[cfg(unix)]
fn find_on_path(name: &str) -> Option<PathBuf> {
    use std::os::unix::fs::PermissionsExt;
    std::env::split_paths(&std::env::var_os("PATH")?)
        .map(|d| d.join(name))
        .find(|p| p.metadata().is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0))
}

/// The folder that holds `runtime/` and `models/`: next to the exe when shipped,
/// or the project root when run with `cargo run`.
pub fn app_base_dir() -> PathBuf {
    let exe_dir = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(Path::to_path_buf))
        .unwrap_or_else(|| PathBuf::from("."));
    let mut candidates = vec![exe_dir.clone()];
    // target/release/ -> project root
    if let Some(root) = exe_dir.parent().and_then(Path::parent) {
        candidates.push(root.to_path_buf());
    }
    if let Ok(cwd) = std::env::current_dir() {
        candidates.push(cwd);
    }
    candidates
        .iter()
        .find(|d| d.join("runtime").join(SERVER_EXE).is_file())
        .cloned()
        .unwrap_or(exe_dir)
}

#[derive(Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum GenMode {
    /// Text to image.
    #[default]
    Text,
    /// Classic img2img: start from the input image and re-noise it by `strength`.
    Variation,
    /// Instruction-based editing with reference images (needs the vision projector).
    /// ("Motion" was a short-lived mode that ran as an edit; old history entries map here.)
    #[serde(alias = "Motion")]
    Edit,
    /// Repaint only the brushed area of the input image (init image + mask).
    Inpaint,
    /// Extend the canvas past the input image's edges (outpainting).
    Outpaint,
}

/// Outpaint padding as fractions of the source size: left, right, top, bottom.
pub type Pads = [f32; 4];

/// Pads that extend an image of size (w, h) to the target aspect, centred.
pub fn pads_for_aspect(w: u32, h: u32, target_w: f32, target_h: f32) -> Pads {
    let (w, h) = (w as f32, h as f32);
    let target = target_w / target_h;
    let cur = w / h;
    if (cur - target).abs() < 0.01 {
        return [0.15, 0.15, 0.15, 0.15]; // already that shape: zoom out a little
    }
    if target > cur {
        let extra = (h * target / w - 1.0) / 2.0;
        [extra, extra, 0.0, 0.0]
    } else {
        let extra = (w / target / h - 1.0) / 2.0;
        [0.0, 0.0, extra, extra]
    }
}

/// Sides for dragging the outpaint frame (bit flags).
pub const SIDE_L: u8 = 1;
pub const SIDE_R: u8 = 2;
pub const SIDE_T: u8 = 4;
pub const SIDE_B: u8 = 8;

/// Which side(s) a press at (x, y) pulls, judged from where it is relative to the
/// image's centre: left/right/top/bottom by the dominant axis, a corner when both are
/// far out. `img` is (left, top, width, height) on screen.
pub fn side_at(x: f32, y: f32, img: (f32, f32, f32, f32)) -> u8 {
    let (l, t, w, h) = img;
    let dx = (x - (l + w / 2.0)) / (w / 2.0).max(1.0);
    let dy = (y - (t + h / 2.0)) / (h / 2.0).max(1.0);
    let horiz = if dx < 0.0 { SIDE_L } else { SIDE_R };
    let vert = if dy < 0.0 { SIDE_T } else { SIDE_B };
    if dx.abs() > 0.75 && dy.abs() > 0.75 {
        horiz | vert
    } else if dx.abs() >= dy.abs() {
        horiz
    } else {
        vert
    }
}

/// New padding after dragging `side` by (dx, dy) screen pixels, where the source image
/// is shown `img_w` × `img_h` pixels large. Dragging outward grows, inward shrinks;
/// clamped to 0–100 % and rounded to whole percent.
pub fn drag_pads(start: Pads, side: u8, dx: f32, dy: f32, img_w: f32, img_h: f32) -> Pads {
    let clamp = |v: f32| (v.clamp(0.0, 1.0) * 100.0).round() / 100.0;
    let mut p = start;
    if side & SIDE_L != 0 {
        p[0] = clamp(start[0] - dx / img_w);
    }
    if side & SIDE_R != 0 {
        p[1] = clamp(start[1] + dx / img_w);
    }
    if side & SIDE_T != 0 {
        p[2] = clamp(start[2] - dy / img_h);
    }
    if side & SIDE_B != 0 {
        p[3] = clamp(start[3] + dy / img_h);
    }
    p
}

#[cfg(test)]
mod drag_tests {
    use super::*;
    const IMG: (f32, f32, f32, f32) = (100.0, 100.0, 400.0, 400.0); // centre (300, 300)

    #[test]
    fn picks_side_from_press_position() {
        assert_eq!(side_at(120.0, 300.0, IMG), SIDE_L); // left part of the image
        assert_eq!(side_at(40.0, 310.0, IMG), SIDE_L); // outside, left
        assert_eq!(side_at(480.0, 280.0, IMG), SIDE_R);
        assert_eq!(side_at(300.0, 110.0, IMG), SIDE_T);
        assert_eq!(side_at(310.0, 560.0, IMG), SIDE_B);
        assert_eq!(side_at(60.0, 60.0, IMG), SIDE_L | SIDE_T); // past the top-left corner
        assert_eq!(side_at(560.0, 560.0, IMG), SIDE_R | SIDE_B);
    }

    #[test]
    fn dragging_outward_grows_inward_shrinks() {
        let z = [0.0; 4];
        // Pull the left side 100 px further left on a 400 px wide image → +25 %.
        assert_eq!(drag_pads(z, SIDE_L, -100.0, 0.0, 400.0, 400.0), [0.25, 0.0, 0.0, 0.0]);
        // Pulling it back past the start clamps at 0.
        assert_eq!(drag_pads([0.25, 0.0, 0.0, 0.0], SIDE_L, 200.0, 0.0, 400.0, 400.0), [0.0; 4]);
        // Right and bottom grow with positive deltas; corner moves both.
        assert_eq!(drag_pads(z, SIDE_R | SIDE_B, 40.0, 80.0, 400.0, 400.0), [0.0, 0.1, 0.0, 0.2]);
        // Vertical motion does not affect a left drag, and growth caps at 100 %.
        assert_eq!(drag_pads(z, SIDE_L, -900.0, 300.0, 400.0, 400.0), [1.0, 0.0, 0.0, 0.0]);
    }
}

/// Largest growth per side in one outpaint pass, as a fraction of the current image.
/// Tested: one big pass made the model draw the old image as a framed picture or redraw
/// the subject; growing ~15 % at a time continued the scene naturally.
pub const OUTPAINT_MAX_STEP: f32 = 0.15;

/// Splits a total expansion into passes, each growing the *current* image by at most
/// `max_step` per side. Returns per-pass pads relative to that pass's input.
pub fn plan_outpaint_passes(total: Pads, max_step: f32) -> Vec<Pads> {
    let target_w = 1.0 + total[0] + total[1];
    let target_h = 1.0 + total[2] + total[3];
    let (mut cur_w, mut cur_h) = (1.0f32, 1.0f32);
    let mut passes = Vec::new();
    while (cur_w < target_w - 1e-3 || cur_h < target_h - 1e-3) && passes.len() < 16 {
        // Remaining growth is split between the sides in the original proportion.
        let side = |a: f32, b: f32, cur: f32, target: f32| -> (f32, f32) {
            if a + b <= 0.0 || cur >= target - 1e-3 {
                return (0.0, 0.0);
            }
            // Growth this pass (fraction of cur), capped so the larger side stays ≤ max_step.
            let grow = (target / cur - 1.0).min(max_step * (a + b) / a.max(b));
            (grow * a / (a + b), grow * b / (a + b))
        };
        let (l, r) = side(total[0], total[1], cur_w, target_w);
        let (t, b) = side(total[2], total[3], cur_h, target_h);
        cur_w *= 1.0 + l + r;
        cur_h *= 1.0 + t + b;
        passes.push([l, r, t, b]);
    }
    passes
}

#[cfg(test)]
mod outpaint_plan_tests {
    use super::*;

    fn grown(passes: &[Pads]) -> (f32, f32) {
        passes.iter().fold((1.0, 1.0), |(w, h), p| (w * (1.0 + p[0] + p[1]), h * (1.0 + p[2] + p[3])))
    }

    #[test]
    fn passes_reach_the_target_in_small_steps() {
        for total in [
            [0.55, 0.55, 0.4, 0.4],
            [0.25, 0.25, 0.25, 0.25],
            [0.389, 0.389, 0.0, 0.0],
            [0.6, 0.0, 0.0, 0.0],
            [0.5, 0.1, 0.0, 0.3], // asymmetric
        ] {
            let passes = plan_outpaint_passes(total, OUTPAINT_MAX_STEP);
            let (w, h) = grown(&passes);
            assert!((w - (1.0 + total[0] + total[1])).abs() < 0.01, "{total:?} width {w}");
            assert!((h - (1.0 + total[2] + total[3])).abs() < 0.01, "{total:?} height {h}");
            for p in &passes {
                assert!(p.iter().all(|s| *s <= OUTPAINT_MAX_STEP + 1e-4), "{p:?} too big");
            }
        }
        // Only the requested sides grow.
        assert!(plan_outpaint_passes([0.6, 0.0, 0.0, 0.0], 0.15).iter().all(|p| p[1] == 0.0 && p[2] == 0.0 && p[3] == 0.0));
        assert!(plan_outpaint_passes([0.0; 4], 0.15).is_empty());
    }
}

/// Largest canvas-area growth per reference-guided outpaint pass. Tested on Turbo Q4:
/// 1:1→16:9 (×1.8), 3:4→3× wide (×3) and 3:4→tall (×1.6) all continued the scene in one
/// pass.
pub const GUIDED_MAX_AREA: f32 = 3.0;
/// The cap when the new area surrounds the picture on three or four sides. A grey border
/// all round reads like a mat: one ×4 pass left it flat, and a ×2.1 pass came back as a
/// framed picture; ×1.6 passes (three for a 2× zoom-out) came out clean.
pub const GUIDED_MAX_AREA_ENCLOSED: f32 = 1.6;

/// Splits a total expansion into reference-guided passes of equal size, each growing the
/// canvas area by at most `max_area` (or `GUIDED_MAX_AREA_ENCLOSED` when 3+ sides grow).
/// Equal passes (rather than one big pass and a sliver) keep every step small; the sides
/// keep their proportions. Pads are per pass, relative to that pass's input.
pub fn plan_guided_passes(total: Pads, max_area: f32) -> Vec<Pads> {
    let max_area = if total.iter().filter(|p| **p > 0.0).count() >= 3 {
        max_area.min(GUIDED_MAX_AREA_ENCLOSED)
    } else {
        max_area
    };
    let (tw, th) = (1.0 + total[0] + total[1], 1.0 + total[2] + total[3]);
    let area = tw * th;
    if area <= 1.0 + 1e-4 {
        return Vec::new();
    }
    let n = ((area.ln() / max_area.ln()) - 1e-4).ceil().clamp(1.0, 8.0) as i32;
    let (gw, gh) = (tw.powf(1.0 / n as f32), th.powf(1.0 / n as f32));
    let split = |a: f32, b: f32, g: f32| if a + b > 0.0 { ((g - 1.0) * a / (a + b), (g - 1.0) * b / (a + b)) } else { (0.0, 0.0) };
    let (l, r) = split(total[0], total[1], gw);
    let (t, b) = split(total[2], total[3], gh);
    vec![[l, r, t, b]; n as usize]
}

#[cfg(test)]
mod guided_plan_tests {
    use super::*;

    fn grown(passes: &[Pads]) -> (f32, f32) {
        passes.iter().fold((1.0f32, 1.0f32), |(w, h), p| (w * (1.0 + p[0] + p[1]), h * (1.0 + p[2] + p[3])))
    }

    #[test]
    fn one_pass_up_to_the_cap_then_equal_splits() {
        assert_eq!(plan_guided_passes([0.39, 0.39, 0.0, 0.0], GUIDED_MAX_AREA).len(), 1);
        assert_eq!(plan_guided_passes([1.0, 1.0, 0.0, 0.0], GUIDED_MAX_AREA).len(), 1);
        // 2× zoom-out on all sides: three equal ×1.59 passes.
        let zoom = plan_guided_passes([0.5, 0.5, 0.5, 0.5], GUIDED_MAX_AREA);
        assert_eq!(zoom.len(), 3);
        let (w, h) = grown(&zoom);
        assert!((w - 2.0).abs() < 1e-3 && (h - 2.0).abs() < 1e-3, "{w} {h}");
        for p in &zoom {
            assert!((1.0 + p[0] + p[1]) * (1.0 + p[2] + p[3]) <= GUIDED_MAX_AREA_ENCLOSED + 1e-3);
            assert!((p[0] - p[1]).abs() < 1e-6, "sides keep their proportion");
            assert_eq!(*p, zoom[0], "equal passes");
        }
        // The 1.5× zoom-out preset: two equal passes, not a big one and a sliver.
        let preset = plan_guided_passes([0.25, 0.25, 0.25, 0.25], GUIDED_MAX_AREA);
        assert_eq!(preset.len(), 2);
        let (w, _) = grown(&preset);
        assert!((w - 1.5).abs() < 1e-3);
        // Three sides (the case that hung around for minutes): also two equal passes.
        let three = plan_guided_passes([0.4, 0.4, 0.25, 0.0], GUIDED_MAX_AREA);
        assert_eq!(three.len(), 2);
        let (w, h) = grown(&three);
        assert!((w - 1.8).abs() < 1e-3 && (h - 1.25).abs() < 1e-3 && three[0][3] == 0.0);
        assert!(plan_guided_passes([0.0; 4], GUIDED_MAX_AREA).is_empty());
    }
}

/// Canvas size after padding (in source pixels).
pub fn padded_size(w: u32, h: u32, p: Pads) -> (u32, u32) {
    (((w as f32) * (1.0 + p[0] + p[1])).round() as u32, ((h as f32) * (1.0 + p[2] + p[3])).round() as u32)
}

#[derive(Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum StepMode {
    /// The distilled 8-step schedule the Turbo model was trained for.
    Turbo,
    /// Plain step count with sd.cpp's automatic flow schedule.
    Custom,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct GenParams {
    pub mode: GenMode,
    /// Img2img denoising strength (0..1); higher = further from the input image.
    pub strength: f32,
    /// In image modes, take the output aspect ratio from the first input image.
    pub follow_input_aspect: bool,
    pub prompt: String,
    pub negative_prompt: String,
    pub aspect: usize,
    pub size: usize,
    pub custom_size: bool,
    pub width: u32,
    pub height: u32,
    pub seed: u64,
    pub random_seed: bool,
    pub count: u32,
    pub cfg: f32,
    pub step_mode: StepMode,
    pub steps: u32,
    pub live_preview: bool,
    /// Spectrum step cache: ~23% faster sampling, small numerical change.
    pub fast_mode: bool,
    /// Partial-edit strength; 0.8–0.9 kept edges clean in tests (1.0 bled into hair).
    pub inpaint_strength: f32,
    pub loras: Vec<crate::lora::LoraSel>,
    /// Treat each prompt line as a separate prompt.
    pub batch: bool,
    /// Outpaint padding (left, right, top, bottom) as fractions of the input size.
    pub outpaint: Pads,
    /// Gallery shows large thumbnails (fewer per row).
    pub gallery_large: bool,
}

impl Default for GenParams {
    fn default() -> Self {
        Self {
            mode: GenMode::Text,
            strength: 0.6,
            follow_input_aspect: true,
            prompt: String::new(),
            negative_prompt: String::new(),
            aspect: 0,
            size: 1,
            custom_size: false,
            width: 1024,
            height: 1024,
            seed: 42,
            random_seed: true,
            count: 1,
            cfg: 1.0,
            step_mode: StepMode::Turbo,
            steps: 8,
            live_preview: true,
            fast_mode: false,
            inpaint_strength: 0.85,
            loras: Vec::new(),
            batch: false,
            outpaint: [0.375, 0.375, 0.0, 0.0],
            gallery_large: false,
        }
    }
}

pub struct Aspect {
    pub label: &'static str,
    pub hint_ko: &'static str,
    pub hint_en: &'static str,
    pub w: f32,
    pub h: f32,
}

/// Five clearly different shapes (3:2 and 2:3 were dropped: they looked like 4:3 / 3:4).
pub const ASPECTS: [Aspect; 5] = [
    Aspect { label: "1:1", hint_ko: "정사각", hint_en: "Square", w: 1.0, h: 1.0 },
    Aspect { label: "4:3", hint_ko: "가로", hint_en: "Landscape", w: 4.0, h: 3.0 },
    Aspect { label: "3:4", hint_ko: "세로", hint_en: "Portrait", w: 3.0, h: 4.0 },
    Aspect { label: "16:9", hint_ko: "와이드", hint_en: "Wide", w: 16.0, h: 9.0 },
    Aspect { label: "9:16", hint_ko: "세로 영상", hint_en: "Vertical video", w: 9.0, h: 16.0 },
];

impl Aspect {
    /// Shape name in the UI language.
    pub fn hint(&self) -> &'static str {
        if crate::i18n::en() { self.hint_en } else { self.hint_ko }
    }
}

/// Aspect index from saved settings: the removed 3:2 (5) and 2:3 (6) map to their nearest
/// remaining shapes, 4:3 and 3:4.
pub fn normalize_aspect(i: usize) -> usize {
    match i {
        5 => 1,
        6 => 2,
        i => i.min(ASPECTS.len() - 1),
    }
}

#[cfg(test)]
mod aspect_tests {
    #[test]
    fn removed_aspects_map_to_nearest_shape() {
        assert_eq!(super::ASPECTS[super::normalize_aspect(5)].label, "4:3"); // was 3:2
        assert_eq!(super::ASPECTS[super::normalize_aspect(6)].label, "3:4"); // was 2:3
        assert_eq!(super::normalize_aspect(3), 3);
        assert_eq!(super::normalize_aspect(99), super::ASPECTS.len() - 1);
    }
}

/// (label, megapixels)
pub const SIZES: [(&str, f32); 3] = [("빠름", 0.5), ("표준", 1.0), ("고화질", 1.6)];
const SIZE_LABELS_EN: [&str; 3] = ["Fast", "Standard", "High res"];

/// Labels of `SIZES` in the UI language.
pub fn size_labels() -> Vec<&'static str> {
    if crate::i18n::en() { SIZE_LABELS_EN.to_vec() } else { SIZES.iter().map(|s| s.0).collect() }
}

impl GenParams {
    /// Final output dimensions, snapped to multiples of 32 as Qwen-Image requires.
    pub fn dimensions(&self) -> (u32, u32) {
        self.dimensions_for(None)
    }

    /// Output size; `input` is the first input image's size, used in image modes
    /// when "follow input aspect" is on.
    pub fn dimensions_for(&self, input: Option<(u32, u32)>) -> (u32, u32) {
        if self.custom_size {
            return (snap32(self.width), snap32(self.height));
        }
        let (aw, ah) = match input {
            Some((w, h)) if self.mode != GenMode::Text && self.follow_input_aspect && w > 0 && h > 0 => {
                (w as f32, h as f32)
            }
            _ => {
                let a = &ASPECTS[normalize_aspect(self.aspect)];
                (a.w, a.h)
            }
        };
        let mp = SIZES[self.size.min(SIZES.len() - 1)].1 * 1024.0 * 1024.0;
        let h = (mp * ah / aw).sqrt();
        let w = h * aw / ah;
        (snap32(w.round() as u32), snap32(h.round() as u32))
    }
}

pub fn snap32(v: u32) -> u32 {
    (((v + 16) / 32) * 32).clamp(256, 2048)
}
