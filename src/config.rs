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

/// The GPU interface the engine runs on. stable-diffusion.cpp publishes one build per
/// interface for the same tag; each goes in its own folder (see `Settings::engine_dir`).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize, Default)]
pub enum GpuBackend {
    /// Picked from the graphics card (`resolve_backend`).
    #[default]
    Auto,
    /// NVIDIA CUDA 12: the tested path.
    Cuda,
    /// AMD ROCm/HIP (experimental: not run on AMD hardware by us yet).
    Rocm,
    /// Vulkan: any GPU with a Vulkan driver (experimental on AMD).
    Vulkan,
}

impl GpuBackend {
    pub const ALL: [GpuBackend; 4] = [GpuBackend::Auto, GpuBackend::Cuda, GpuBackend::Rocm, GpuBackend::Vulkan];

    pub fn label(self) -> &'static str {
        match self {
            GpuBackend::Auto => t!("자동", "Automatic"),
            GpuBackend::Cuda => "NVIDIA CUDA",
            GpuBackend::Rocm => t!("AMD ROCm (실험적)", "AMD ROCm (experimental)"),
            GpuBackend::Vulkan => t!("Vulkan (실험적)", "Vulkan (experimental)"),
        }
    }

    /// Start of the device names this build lists with `sd-server --list-devices`
    /// (ggml's backend names: CUDA0, ROCm0, Vulkan0).
    pub fn device_prefix(self) -> &'static str {
        match self {
            GpuBackend::Cuda => "CUDA",
            GpuBackend::Rocm => "ROCm",
            GpuBackend::Vulkan => "Vulkan",
            GpuBackend::Auto => "",
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum GpuVendor {
    Nvidia,
    Amd,
    /// A graphics adapter from another vendor (Intel, …).
    Other,
    /// Nothing recognisable was found.
    Unknown,
}

/// The graphics card the engine will use, as far as the system tells without starting it.
#[derive(Clone, Debug)]
pub struct GpuInfo {
    pub vendor: GpuVendor,
    /// Marketing name on Windows ("AMD Radeon RX 7800 XT"); on Linux the AMD target
    /// ("AMD Radeon (gfx1101)") or empty.
    pub name: String,
    pub vram_bytes: Option<u64>,
    /// The ROCm engine build targets this GPU and, on Linux, the ROCm libraries it links
    /// against are installed.
    pub rocm_ok: bool,
}

/// The graphics card, detected once per run. NVIDIA wins when its driver is present
/// (that is the tested path); otherwise the AMD adapter with the most memory.
pub fn detect_gpu() -> &'static GpuInfo {
    static GPU: std::sync::OnceLock<GpuInfo> = std::sync::OnceLock::new();
    GPU.get_or_init(|| {
        let adapters = probe_adapters();
        let best = |vendor: GpuVendor| {
            adapters.iter().filter(|a| a.vendor == vendor).max_by_key(|a| a.vram_bytes.unwrap_or(0)).cloned()
        };
        if nvidia_driver_present() {
            let mut g = best(GpuVendor::Nvidia).unwrap_or(GpuInfo { vendor: GpuVendor::Nvidia, name: String::new(), vram_bytes: None, rocm_ok: false });
            // Linux exposes neither the name nor the VRAM size of an NVIDIA card in /sys:
            // ask the driver's tool instead.
            if g.vram_bytes.is_none() || g.name.is_empty() {
                if let Some((name, vram)) = nvidia_smi_info() {
                    if g.name.is_empty() {
                        g.name = name;
                    }
                    g.vram_bytes = g.vram_bytes.or(Some(vram));
                }
            }
            return g;
        }
        best(GpuVendor::Amd)
            .or_else(|| best(GpuVendor::Other))
            .unwrap_or(GpuInfo { vendor: GpuVendor::Unknown, name: String::new(), vram_bytes: None, rocm_ok: false })
    })
}

/// The backend `choice` means on this PC: NVIDIA → CUDA; AMD → ROCm when the ROCm build
/// supports the card (and, on Linux, ROCm is installed), else Vulkan. With no recognisable
/// GPU the CUDA build stays the default, as before AMD support.
pub fn resolve_backend(choice: GpuBackend, gpu: &GpuInfo) -> GpuBackend {
    match (choice, gpu.vendor) {
        (GpuBackend::Auto, GpuVendor::Amd) if gpu.rocm_ok => GpuBackend::Rocm,
        (GpuBackend::Auto, GpuVendor::Amd) => GpuBackend::Vulkan,
        (GpuBackend::Auto, _) => GpuBackend::Cuda,
        (c, _) => c,
    }
}

/// Windows ROCm build: whether its GPU targets (gfx1010–gfx1201) cover the card, judged
/// from the marketing name. Only the RDNA2/3/4 lines that ROCm on Windows supports well
/// are picked automatically: Radeon RX 6000/7000/9000, Radeon PRO W6000/W7000/W9000 and
/// the Ryzen AI Max "Radeon 8060S/8050S/8040S" (gfx1151). Older or integrated Radeons use
/// Vulkan unless ROCm is chosen in Settings.
#[cfg_attr(not(windows), allow(dead_code))]
pub fn rocm_windows_supported(name: &str) -> bool {
    let n = name.to_ascii_lowercase();
    if !n.contains("radeon") {
        return false;
    }
    let series_after = |marker: &str| {
        n.split(marker).skip(1).any(|rest| {
            let digits: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
            digits.len() == 4 && matches!(digits.as_bytes()[0], b'6' | b'7' | b'9')
        })
    };
    series_after("rx ") || series_after("pro w") || ["8060s", "8050s", "8040s"].iter().any(|m| n.contains(m))
}

/// GPU targets of the Linux ROCm build (from its release workflow).
#[cfg_attr(windows, allow(dead_code))]
const LINUX_ROCM_TARGETS: [&str; 27] = [
    "gfx900", "gfx906", "gfx908", "gfx90a", "gfx90c", "gfx942", "gfx950", "gfx1010", "gfx1011", "gfx1012", "gfx1030",
    "gfx1031", "gfx1032", "gfx1033", "gfx1034", "gfx1035", "gfx1036", "gfx1100", "gfx1101", "gfx1102", "gfx1103",
    "gfx1150", "gfx1151", "gfx1152", "gfx1153", "gfx1200", "gfx1201",
];

/// `gfx_target_version` from the Linux KFD topology (major·10000 + minor·100 + stepping)
/// as the target name: 110001 → "gfx1101", 90010 → "gfx90a". 0 (a CPU node) → None.
#[cfg_attr(windows, allow(dead_code))]
pub fn gfx_name(target_version: u32) -> Option<String> {
    (target_version > 0).then(|| format!("gfx{}{:x}{:x}", target_version / 10000, (target_version / 100) % 100, target_version % 100))
}

/// The PCI vendor ID in a driver's `MatchingDeviceId` ("PCI\VEN_1002&DEV_747E…").
#[cfg_attr(not(windows), allow(dead_code))]
pub fn pci_vendor(matching_id: &str) -> Option<u16> {
    let l = matching_id.to_ascii_lowercase();
    let i = l.find("ven_")?;
    u16::from_str_radix(l.get(i + 4..i + 8)?, 16).ok()
}

fn vendor_of(id: u16) -> Option<GpuVendor> {
    match id {
        0x10de => Some(GpuVendor::Nvidia),
        0x1002 => Some(GpuVendor::Amd),
        0x1414 => None, // Microsoft Basic Display / Render adapter
        _ => Some(GpuVendor::Other),
    }
}

/// Display adapters with a vendor driver, from the registry's display class
/// (DriverDesc, MatchingDeviceId, HardwareInformation.qwMemorySize).
#[cfg(windows)]
fn probe_adapters() -> Vec<GpuInfo> {
    const CLASS: &str = r"SYSTEM\CurrentControlSet\Control\Class\{4d36e968-e325-11ce-bfc1-08002be10318}";
    let mut out = Vec::new();
    for i in 0..32 {
        let key = format!(r"{CLASS}\{i:04}");
        let Some(id) = reg_string(&key, "MatchingDeviceId") else { continue };
        let Some(vendor) = pci_vendor(&id).and_then(vendor_of) else { continue };
        let name = reg_string(&key, "DriverDesc").unwrap_or_default();
        let vram_bytes = reg_u64(&key, "HardwareInformation.qwMemorySize").or_else(|| reg_u64(&key, "HardwareInformation.MemorySize"));
        let rocm_ok = vendor == GpuVendor::Amd && rocm_windows_supported(&name);
        out.push(GpuInfo { vendor, name, vram_bytes, rocm_ok });
    }
    out
}

#[cfg(windows)]
fn reg_raw(key: &str, value: &str, flags: u32) -> Option<Vec<u8>> {
    use windows_sys::Win32::System::Registry::{HKEY_LOCAL_MACHINE, RegGetValueW};
    let wide = |s: &str| s.encode_utf16().chain(std::iter::once(0)).collect::<Vec<u16>>();
    let (k, v) = (wide(key), wide(value));
    let mut buf = vec![0u8; 512];
    let mut len = buf.len() as u32;
    // SAFETY: both names are NUL-terminated; `buf`/`len` describe a writable buffer.
    let r = unsafe { RegGetValueW(HKEY_LOCAL_MACHINE, k.as_ptr(), v.as_ptr(), flags, std::ptr::null_mut(), buf.as_mut_ptr().cast(), &mut len) };
    (r == 0).then(|| {
        buf.truncate(len as usize);
        buf
    })
}

#[cfg(windows)]
fn reg_string(key: &str, value: &str) -> Option<String> {
    use windows_sys::Win32::System::Registry::RRF_RT_REG_SZ;
    let b = reg_raw(key, value, RRF_RT_REG_SZ)?;
    let w: Vec<u16> = b.chunks_exact(2).map(|c| u16::from_le_bytes([c[0], c[1]])).take_while(|c| *c != 0).collect();
    Some(String::from_utf16_lossy(&w))
}

/// A REG_QWORD, REG_DWORD or REG_BINARY number (drivers store the memory size either way).
#[cfg(windows)]
fn reg_u64(key: &str, value: &str) -> Option<u64> {
    use windows_sys::Win32::System::Registry::{RRF_RT_REG_BINARY, RRF_RT_REG_DWORD, RRF_RT_REG_QWORD};
    let b = reg_raw(key, value, RRF_RT_REG_QWORD | RRF_RT_REG_DWORD | RRF_RT_REG_BINARY)?;
    match b.len() {
        4 => Some(u32::from_le_bytes(b[..4].try_into().ok()?) as u64),
        8 => Some(u64::from_le_bytes(b[..8].try_into().ok()?)),
        _ => None,
    }
    .filter(|v| *v > 0)
}

/// Linux: GPUs from `/sys/class/drm/card*/device` (vendor, VRAM); AMD targets from the KFD
/// topology, which exists only with the amdgpu kernel driver's compute support (/dev/kfd).
#[cfg(unix)]
fn probe_adapters() -> Vec<GpuInfo> {
    let read = |p: &Path| std::fs::read_to_string(p).ok().map(|s| s.trim().to_owned());
    // ROCm-capable AMD targets present on this system.
    let mut gfx: Vec<String> = Vec::new();
    if let Ok(nodes) = std::fs::read_dir("/sys/class/kfd/kfd/topology/nodes") {
        for n in nodes.flatten() {
            let props = read(&n.path().join("properties")).unwrap_or_default();
            let v = props.lines().find_map(|l| l.strip_prefix("gfx_target_version ")).and_then(|v| v.trim().parse::<u32>().ok());
            gfx.extend(v.and_then(gfx_name));
        }
    }
    let rocm_libs = || ["libamdhip64.so.7", "libhipblas.so.3", "librocblas.so.5"].iter().all(|l| can_load_library(l));
    let rocm_ok = Path::new("/dev/kfd").exists() && gfx.iter().any(|g| LINUX_ROCM_TARGETS.contains(&g.as_str())) && rocm_libs();
    let mut out = Vec::new();
    let Ok(cards) = std::fs::read_dir("/sys/class/drm") else { return out };
    for c in cards.flatten() {
        let name = c.file_name().to_string_lossy().into_owned();
        if !name.starts_with("card") || name.contains('-') {
            continue; // connectors such as card0-HDMI-A-1
        }
        let dev = c.path().join("device");
        let Some(vendor) = read(&dev.join("vendor"))
            .and_then(|v| u16::from_str_radix(v.trim_start_matches("0x"), 16).ok())
            .and_then(vendor_of)
        else {
            continue;
        };
        let vram_bytes = read(&dev.join("mem_info_vram_total")).and_then(|v| v.parse().ok());
        let amd = vendor == GpuVendor::Amd;
        let label = if amd { gfx.first().map(|g| format!("AMD Radeon ({g})")).unwrap_or_else(|| "AMD Radeon".into()) } else { String::new() };
        out.push(GpuInfo { vendor, name: label, vram_bytes, rocm_ok: amd && rocm_ok });
    }
    out
}

#[cfg(not(any(windows, unix)))]
fn probe_adapters() -> Vec<GpuInfo> {
    Vec::new()
}

/// The device to run on from `sd-server --list-devices` output ("name<TAB>description"
/// per line): the first one of `backend`, else the first that is not the CPU.
pub fn pick_device(list: &str, backend: GpuBackend) -> Option<String> {
    let names: Vec<&str> = list
        .lines()
        .filter_map(|l| l.split_once('\t').map(|(n, _)| n.trim()))
        .filter(|n| !n.is_empty() && !n.contains(' '))
        .collect();
    let prefix = backend.device_prefix();
    names
        .iter()
        .find(|n| !prefix.is_empty() && n.starts_with(prefix))
        .or_else(|| names.iter().find(|n| !n.eq_ignore_ascii_case("cpu")))
        .map(|n| n.to_string())
}

/// Name and total memory of the first NVIDIA GPU from `nvidia-smi` (installed with the driver).
fn nvidia_smi_info() -> Option<(String, u64)> {
    let mut cmd = std::process::Command::new("nvidia-smi");
    cmd.args(["--query-gpu=name,memory.total", "--format=csv,noheader,nounits"]);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }
    let out = cmd.output().ok().filter(|o| o.status.success())?;
    let line = String::from_utf8_lossy(&out.stdout).lines().next()?.to_owned();
    let (name, mib) = line.rsplit_once(',')?;
    Some((name.trim().to_owned(), mib.trim().parse::<u64>().ok()? << 20))
}

/// Whether an NVIDIA card has tensor cores, which the INT8 model's fast path needs: every
/// GeForce RTX / professional RTX card does; GeForce GTX cards (16 series and older, e.g.
/// the GTX 1660 Ti) don't. An unknown name counts as yes, as before this check.
pub fn has_tensor_cores(name: &str) -> bool {
    !name.to_ascii_lowercase().contains("gtx")
}

/// A GeForce GTX card (16 series and older, e.g. GTX 1650): no tensor cores, so sampling
/// is compute-bound, roughly 10–15× slower than an RTX 3060 Ti (a 1024² Turbo image took
/// over 500 s on a GTX 1650). These cards start on the "Fast" size with Lightning 4 steps.
/// Self-tests can pretend another card name (`QIS_FAKE_GPU_NAME`, with `QIS_SHOT_DIR`).
pub fn slow_gpu() -> bool {
    static SLOW: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *SLOW.get_or_init(|| {
        if std::env::var_os("QIS_SHOT_DIR").is_some() {
            if let Ok(name) = std::env::var("QIS_FAKE_GPU_NAME") {
                return !has_tensor_cores(&name);
            }
        }
        let g = detect_gpu();
        g.vendor == GpuVendor::Nvidia && !has_tensor_cores(&g.name)
    })
}

/// Over ~0.6 MP (above the "Fast" size) a GTX card takes minutes per image.
pub fn slow_for_gpu(w: u32, h: u32) -> bool {
    slow_gpu() && (w as u64 * h as u64) > 600_000
}

/// VRAM of the GPU the engine uses. Self-tests can pretend a smaller card
/// (`QIS_FAKE_VRAM_GB`, only together with `QIS_SHOT_DIR`).
pub fn gpu_vram_bytes() -> Option<u64> {
    if std::env::var_os("QIS_SHOT_DIR").is_some() {
        if let Some(gb) = std::env::var("QIS_FAKE_VRAM_GB").ok().and_then(|v| v.parse::<u64>().ok()) {
            return Some(gb << 30);
        }
    }
    detect_gpu().vram_bytes
}

/// A 4 GB-class card (under 5 GiB): the engine then keeps a 3 GiB VRAM budget and decodes
/// the image in tiles. Larger cards keep the settings measured on 8 GB cards.
pub fn low_vram(vram_bytes: Option<u64>) -> bool {
    vram_bytes.is_some_and(|b| b < 5 << 30)
}

/// Under this much installed RAM (8 GB PCs, WSL's default limit) the weights don't all fit
/// in memory: the text encoder is read from disk, and 4 GB cards use the Q3_K_S image model.
pub const LOW_RAM_BYTES: u64 = 13_000_000_000;

/// Installed RAM. Self-tests can pretend a smaller PC (`QIS_FAKE_RAM_GB`, with `QIS_SHOT_DIR`).
pub fn system_ram_bytes() -> Option<u64> {
    if std::env::var_os("QIS_SHOT_DIR").is_some() {
        if let Some(gb) = std::env::var("QIS_FAKE_RAM_GB").ok().and_then(|v| v.parse::<u64>().ok()) {
            return Some(gb << 30);
        }
    }
    total_ram_bytes()
}

/// Q3_K_S instead of Q4_K_M only on a 4 GB card in a PC short of RAM: Q3_K_S measured 24%
/// slower per step (1.34 vs 1.08 s on an RTX 3060 Ti held to 4 GB) but needs 1.5 GB less
/// RAM, which keeps an 8 GB PC from paging the engine out.
pub fn wants_small_diffusion(vram: Option<u64>, ram: Option<u64>) -> bool {
    low_vram(vram) && ram.is_some_and(|r| r < LOW_RAM_BYTES)
}

/// VRAM budget (GiB) for `--max-vram` on the ROCm/Vulkan builds: the 6 GiB tested on
/// 8 GB NVIDIA cards, less on smaller cards (2 GiB kept free), never under 3.
pub fn vram_budget_gib(vram_bytes: Option<u64>) -> u32 {
    match vram_bytes {
        Some(b) => (((b + (1 << 29)) >> 30) as u32).saturating_sub(2).clamp(3, 6),
        None => 6,
    }
}

#[cfg(test)]
mod gpu_tests {
    use super::*;

    fn amd(rocm_ok: bool) -> GpuInfo {
        GpuInfo { vendor: GpuVendor::Amd, name: "AMD Radeon RX 7800 XT".into(), vram_bytes: Some(16 << 30), rocm_ok }
    }

    #[test]
    fn auto_picks_cuda_then_rocm_then_vulkan() {
        let nv = GpuInfo { vendor: GpuVendor::Nvidia, name: String::new(), vram_bytes: None, rocm_ok: false };
        assert_eq!(resolve_backend(GpuBackend::Auto, &nv), GpuBackend::Cuda);
        assert_eq!(resolve_backend(GpuBackend::Auto, &amd(true)), GpuBackend::Rocm);
        assert_eq!(resolve_backend(GpuBackend::Auto, &amd(false)), GpuBackend::Vulkan);
        let none = GpuInfo { vendor: GpuVendor::Unknown, name: String::new(), vram_bytes: None, rocm_ok: false };
        assert_eq!(resolve_backend(GpuBackend::Auto, &none), GpuBackend::Cuda);
        // A choice in Settings always wins.
        assert_eq!(resolve_backend(GpuBackend::Vulkan, &nv), GpuBackend::Vulkan);
        assert_eq!(resolve_backend(GpuBackend::Rocm, &amd(false)), GpuBackend::Rocm);
    }

    #[test]
    fn windows_rocm_support_from_the_name() {
        for yes in ["AMD Radeon RX 7900 XTX", "AMD Radeon RX 6700 XT", "AMD Radeon RX 9070 XT", "AMD Radeon PRO W7800", "AMD Radeon(TM) 8060S Graphics"] {
            assert!(rocm_windows_supported(yes), "{yes}");
        }
        for no in ["AMD Radeon RX 580 2048SP", "AMD Radeon RX 5700 XT", "AMD Radeon(TM) 780M", "AMD Radeon(TM) Graphics", "NVIDIA GeForce RTX 3060 Ti", "Radeon RX Vega 56"] {
            assert!(!rocm_windows_supported(no), "{no}");
        }
    }

    #[test]
    fn ids_and_targets() {
        assert_eq!(pci_vendor(r"pci\ven_1002&dev_747e&subsys_"), Some(0x1002));
        assert_eq!(pci_vendor(r"PCI\VEN_10DE&DEV_2489"), Some(0x10de));
        assert_eq!(pci_vendor("pci\\cc_0300"), None);
        assert_eq!(gfx_name(110001).as_deref(), Some("gfx1101"));
        assert_eq!(gfx_name(100300).as_deref(), Some("gfx1030"));
        assert_eq!(gfx_name(90010).as_deref(), Some("gfx90a"));
        assert_eq!(gfx_name(120001).as_deref(), Some("gfx1201"));
        assert_eq!(gfx_name(0), None);
        assert!(LINUX_ROCM_TARGETS.contains(&gfx_name(110000).unwrap().as_str()));
    }

    #[test]
    fn device_from_list_output() {
        // Real output of the Windows Vulkan and CUDA builds on an RTX 3060 Ti (log lines go to stderr).
        let vk = "Vulkan0\tNVIDIA GeForce RTX 3060 Ti\nCPU\tAMD Ryzen 7 5700X 8-Core Processor             \n";
        assert_eq!(pick_device(vk, GpuBackend::Vulkan).as_deref(), Some("Vulkan0"));
        assert_eq!(pick_device("CUDA0\tNVIDIA GeForce RTX 3060 Ti\nCPU\tx\n", GpuBackend::Cuda).as_deref(), Some("CUDA0"));
        assert_eq!(pick_device("ROCm0\tAMD Radeon RX 7800 XT\nCPU\tx\n", GpuBackend::Rocm).as_deref(), Some("ROCm0"));
        // The ROCm build without a usable device lists only the CPU.
        assert_eq!(pick_device("CPU\tAMD Ryzen 7 5700X\n", GpuBackend::Rocm), None);
        assert_eq!(pick_device("", GpuBackend::Vulkan), None);
    }

    #[test]
    fn vram_budget() {
        assert_eq!(vram_budget_gib(None), 6);
        assert_eq!(vram_budget_gib(Some(8 << 30)), 6);
        assert_eq!(vram_budget_gib(Some(24 << 30)), 6);
        assert_eq!(vram_budget_gib(Some(6 << 30)), 4);
        assert_eq!(vram_budget_gib(Some(4 << 30)), 3);
        assert_eq!(vram_budget_gib(Some(8176 << 20)), 6); // an "8 GB" card reporting a bit less
        // Low-VRAM mode: 4 GB cards only; 6 GB and up keep the normal settings.
        assert!(low_vram(Some(4 << 30)));
        assert!(low_vram(Some(4096 << 20)));
        assert!(!low_vram(Some(6 << 30)));
        assert!(!low_vram(Some(8 << 30)));
        assert!(!low_vram(None));
        assert!(!has_tensor_cores("NVIDIA GeForce GTX 1660 Ti"));
        assert!(!has_tensor_cores("NVIDIA GeForce GTX 1080"));
        assert!(has_tensor_cores("NVIDIA GeForce RTX 3060 Ti"));
        assert!(has_tensor_cores("NVIDIA RTX A2000"));
        assert!(has_tensor_cores(""));
    }
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

/// What the Linux system lacks for parts of the app with the engine build of `backend`:
/// (what, the Ubuntu/Debian package to install, required for image generation). Our CUDA
/// archive brings its own CUDA runtime; stable-diffusion.cpp's ROCm build needs the ROCm 7
/// libraries from the system, and its Vulkan build is made on Ubuntu 26.04. Checked once
/// per run and backend.
#[cfg(target_os = "linux")]
pub fn missing_system_parts(backend: GpuBackend) -> &'static [(&'static str, &'static str, bool)] {
    type Parts = Vec<(&'static str, &'static str, bool)>;
    static MISSING: [std::sync::OnceLock<Parts>; 3] = [const { std::sync::OnceLock::new() }; 3];
    let slot = match backend {
        GpuBackend::Rocm => 1,
        GpuBackend::Vulkan => 2,
        _ => 0,
    };
    MISSING[slot].get_or_init(|| {
        let mut v = Vec::new();
        // Oldest glibc each engine build runs on (CUDA: ours, built on Ubuntu 22.04;
        // ROCm: Ubuntu 24.04; Vulkan: Ubuntu 26.04 — measured from the binaries' symbols).
        let (floor, distro) = match backend {
            GpuBackend::Rocm => ((2, 38), "Ubuntu 24.04 or newer"),
            GpuBackend::Vulkan => ((2, 43), "Ubuntu 26.04 or newer"),
            _ => ((2, 35), "Ubuntu 22.04 / Debian 12 or newer"),
        };
        if glibc_version().is_some_and(|ver| ver < floor) {
            v.push((
                match backend {
                    GpuBackend::Rocm => "glibc 2.38+",
                    GpuBackend::Vulkan => "glibc 2.43+",
                    _ => "glibc 2.35+",
                },
                distro,
                true,
            ));
        }
        if !can_load_library("libstdc++.so.6") {
            v.push(("libstdc++.so.6", "libstdc++6", true));
        }
        if backend != GpuBackend::Cuda && !can_load_library("libgomp.so.1") {
            v.push(("libgomp.so.1", "libgomp1", true));
        }
        if backend == GpuBackend::Rocm {
            if !Path::new("/dev/kfd").exists() {
                v.push(("/dev/kfd", "amdgpu kernel driver (user in the 'render' group)", true));
            }
            for (lib, pkg) in [("libamdhip64.so.7", "ROCm 7 (hip-runtime-amd)"), ("libhipblas.so.3", "ROCm 7 (hipblas)"), ("librocblas.so.5", "ROCm 7 (rocblas)")] {
                if !can_load_library(lib) {
                    v.push((lib, pkg, true));
                }
            }
        }
        if !can_load_library("libvulkan.so.1") {
            // The photo describer always uses Vulkan; the Vulkan engine needs it too.
            v.push(("libvulkan.so.1", "libvulkan1", backend == GpuBackend::Vulkan));
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

/// AMD builds of the same stable-diffusion.cpp tag. The Windows ROCm zip ships the HIP
/// runtime DLLs (amdhip64_7, amd_comgr, rocm_kpack) next to sd-server.exe, so only the
/// AMD graphics driver is needed; the Vulkan zip runs on any GPU with a Vulkan driver.
#[cfg(windows)]
pub const ENGINE_ROCM_SOURCE: ModelSource = ModelSource {
    label: "생성 엔진 (stable-diffusion.cpp, AMD ROCm)",
    label_en: "Generation engine (stable-diffusion.cpp, AMD ROCm)",
    file: "sd-master-f5df5f8-bin-win-rocm-10.1.0-x64.zip",
    url: "https://github.com/leejet/stable-diffusion.cpp/releases/download/master-1052-f5df5f8/sd-master-f5df5f8-bin-win-rocm-10.1.0-x64.zip",
    approx_bytes: 265_984_750,
    unzip: true,
};

#[cfg(windows)]
pub const ENGINE_VULKAN_SOURCE: ModelSource = ModelSource {
    label: "생성 엔진 (stable-diffusion.cpp, Vulkan)",
    label_en: "Generation engine (stable-diffusion.cpp, Vulkan)",
    file: "sd-master-f5df5f8-bin-win-vulkan-x64.zip",
    url: "https://github.com/leejet/stable-diffusion.cpp/releases/download/master-1052-f5df5f8/sd-master-f5df5f8-bin-win-vulkan-x64.zip",
    approx_bytes: 29_863_784,
    unzip: true,
};

/// Linux ROCm build (Ubuntu 24.04, files under `build/bin/` in the zip). It links the
/// system's ROCm 7 libraries (libamdhip64.so.7, libhipblas.so.3, librocblas.so.5).
#[cfg(not(windows))]
pub const ENGINE_ROCM_SOURCE: ModelSource = ModelSource {
    label: "생성 엔진 (stable-diffusion.cpp, AMD ROCm)",
    label_en: "Generation engine (stable-diffusion.cpp, AMD ROCm)",
    file: "sd-master-f5df5f8-bin-Linux-Ubuntu-24.04-x86_64-rocm-10.1.0.zip",
    url: "https://github.com/leejet/stable-diffusion.cpp/releases/download/master-1052-f5df5f8/sd-master-f5df5f8-bin-Linux-Ubuntu-24.04-x86_64-rocm-10.1.0.zip",
    approx_bytes: 298_402_307,
    unzip: true,
};

/// Linux Vulkan build. Made on Ubuntu 26.04: needs glibc 2.43 or newer.
#[cfg(not(windows))]
pub const ENGINE_VULKAN_SOURCE: ModelSource = ModelSource {
    label: "생성 엔진 (stable-diffusion.cpp, Vulkan)",
    label_en: "Generation engine (stable-diffusion.cpp, Vulkan)",
    file: "sd-master-f5df5f8-bin-Linux-Ubuntu-26.04-x86_64-vulkan.zip",
    url: "https://github.com/leejet/stable-diffusion.cpp/releases/download/master-1052-f5df5f8/sd-master-f5df5f8-bin-Linux-Ubuntu-26.04-x86_64-vulkan.zip",
    approx_bytes: 41_670_193,
    unzip: true,
};

/// The engine archive for `backend` (Auto is resolved by the caller; it maps to CUDA).
pub fn engine_source(backend: GpuBackend) -> &'static ModelSource {
    match backend {
        GpuBackend::Rocm => &ENGINE_ROCM_SOURCE,
        GpuBackend::Vulkan => &ENGINE_VULKAN_SOURCE,
        GpuBackend::Cuda | GpuBackend::Auto => &ENGINE_SOURCE,
    }
}

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
/// `llama-b12182/`, which extraction strips). Needs `libvulkan1` and the GPU driver's
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

pub const DIFFUSION_SMALL_FILE: &str = "qwen-image-2.1-turbo-Q3_K_S.gguf";

/// Image model for 4 GB cards in PCs short of RAM: Q3_K_S, 2.72 GB instead of 4.2 GB, so
/// 1.5 GB less RAM is needed.
pub const DIFFUSION_SMALL_SOURCE: ModelSource = ModelSource {
    label: "이미지 생성 모델 (Qwen-Image-2.1-Turbo Q3_K_S, 저사양 PC용)",
    label_en: "Image model (Qwen-Image-2.1-Turbo Q3_K_S, for low-end PCs)",
    file: DIFFUSION_SMALL_FILE,
    url: "https://huggingface.co/unsloth/Qwen-Image-2.1-Turbo-GGUF/resolve/main/qwen-image-2.1-turbo-Q3_K_S.gguf",
    approx_bytes: 2_724_742_976,
    unzip: false,
};

pub const TEXT_ENCODER_SMALL_FILE: &str = "Qwen3-VL-8B-Instruct-Q3_K_S.gguf";

/// Text encoder for 4 GB cards: Q3_K_S, 3.77 GB instead of 5.15 GB. Those PCs usually read
/// the text encoder from disk for every new prompt (too little RAM to keep it), so 27%
/// less to read.
pub const TEXT_ENCODER_SMALL_SOURCE: ModelSource = ModelSource {
    label: "텍스트 인코더 (Qwen3-VL-8B Q3_K_S, 4GB 그래픽카드용)",
    label_en: "Text encoder (Qwen3-VL-8B Q3_K_S, for 4 GB cards)",
    file: TEXT_ENCODER_SMALL_FILE,
    url: "https://huggingface.co/unsloth/Qwen3-VL-8B-Instruct-GGUF/resolve/main/Qwen3-VL-8B-Instruct-Q3_K_S.gguf",
    approx_bytes: 3_769_613_152,
    unzip: false,
};

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
    /// UI language: true = English (default), false = 한국어.
    pub english: bool,
    /// The user picked the language with the 한|EN switch. Versions up to 1.1.1 saved
    /// `english: false` (their Korean default) without a choice, so until this is set the
    /// app starts in English.
    pub lang_chosen: bool,
    /// GPU interface for the engine; Auto picks it from the graphics card.
    pub gpu_backend: GpuBackend,
    /// The GTX defaults ("Fast" size, Lightning 4 steps) were applied once; after that
    /// the user's own choices stay.
    pub slow_gpu_defaults: bool,
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
            english: true,
            lang_chosen: false,
            gpu_backend: GpuBackend::Auto,
            slow_gpu_defaults: false,
        }
    }
}

impl Settings {
    /// The GPU interface the engine runs on (`gpu_backend` with Auto resolved).
    pub fn backend(&self) -> GpuBackend {
        resolve_backend(self.gpu_backend, detect_gpu())
    }

    /// The INT8 tensor-core model needs the CUDA build's CUTLASS INT8 kernels.
    /// The INT8 model needs the CUDA engine and an NVIDIA card with tensor cores.
    pub fn int8_supported(&self) -> bool {
        self.backend() == GpuBackend::Cuda && has_tensor_cores(&detect_gpu().name)
    }

    /// Folder of the engine build for `backend`: `runtime/` for CUDA (where it always was),
    /// `runtime/rocm/` and `runtime/vulkan/` for the AMD builds, whose DLLs share names
    /// with the CUDA build's (ggml-base, stable-diffusion, …).
    pub fn engine_dir_for(&self, backend: GpuBackend) -> PathBuf {
        match backend {
            GpuBackend::Rocm => self.runtime_dir.join("rocm"),
            GpuBackend::Vulkan => self.runtime_dir.join("vulkan"),
            GpuBackend::Cuda | GpuBackend::Auto => self.runtime_dir.clone(),
        }
    }

    pub fn engine_dir(&self) -> PathBuf {
        self.engine_dir_for(self.backend())
    }

    /// The engine executable: `sd-server(.exe)` in `engine_dir`. On Linux a self-built
    /// `sd-server` on PATH is used when that folder has none.
    pub fn server_exe(&self) -> PathBuf {
        let local = self.engine_dir().join(SERVER_EXE);
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
            TEXT_ENCODER_SMALL_FILE => self.models_dir().join(TEXT_ENCODER_SMALL_FILE),
            DIFFUSION_SMALL_FILE => self.models_dir().join(DIFFUSION_SMALL_FILE),
            VISION_FILE => self.vision.clone(),
            INT8_FILE => self.int8_model.clone(),
            _ => self.vae.clone(),
        }
    }

    /// Diffusion weights for the selected variant.
    pub fn active_diffusion(&self) -> PathBuf {
        match self.variant {
            ModelVariant::Q4 if self.small_diffusion() => self.model_path_for(DIFFUSION_SMALL_FILE),
            ModelVariant::Q4 => self.diffusion_model.clone(),
            ModelVariant::Int8 => self.int8_model.clone(),
        }
    }

    /// 4 GB cards in PCs short of RAM run the Q3_K_S image model instead of Q4_K_M
    /// (`wants_small_diffusion`), unless the user picked their own file.
    pub fn small_diffusion(&self) -> bool {
        wants_small_diffusion(gpu_vram_bytes(), system_ram_bytes())
            && self.diffusion_model.file_name().is_some_and(|n| n == DIFFUSION_FILE)
    }

    /// 4 GB cards run the smaller text encoder, unless the user picked their own file.
    pub fn small_text_encoder(&self) -> bool {
        low_vram(gpu_vram_bytes()) && self.text_encoder.file_name().is_some_and(|n| n == TEXT_ENCODER_FILE)
    }

    /// The text encoder the engine loads.
    pub fn active_text_encoder(&self) -> PathBuf {
        if self.small_text_encoder() {
            self.model_path_for(TEXT_ENCODER_SMALL_FILE)
        } else {
            self.text_encoder.clone()
        }
    }

    /// The three model files the engine needs: diffusion (selected variant), text encoder, VAE.
    fn required_models(&self) -> [&'static ModelSource; 3] {
        let diffusion: &'static ModelSource = match self.variant {
            ModelVariant::Q4 if self.small_diffusion() => &DIFFUSION_SMALL_SOURCE,
            ModelVariant::Q4 => &MODEL_SOURCES[0],
            ModelVariant::Int8 => &INT8_SOURCE,
        };
        let te = if self.small_text_encoder() { &TEXT_ENCODER_SMALL_SOURCE } else { &MODEL_SOURCES[1] };
        [diffusion, te, &MODEL_SOURCES[2]]
    }

    /// Required files for the selected variant that are not on disk yet.
    pub fn missing_models(&self) -> Vec<&'static ModelSource> {
        self.required_models().into_iter().filter(|m| !self.model_path_for(m.file).is_file()).collect()
    }

    pub fn variant_label(&self) -> &'static str {
        match self.variant {
            ModelVariant::Q4 if self.small_diffusion() => "Q3_K_S",
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
            f if f == ENGINE_ROCM_SOURCE.file => self.engine_dir_for(GpuBackend::Rocm).join(f),
            f if f == ENGINE_VULKAN_SOURCE.file => self.engine_dir_for(GpuBackend::Vulkan).join(f),
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
            f if f == engine_source(self.backend()).file => self.server_exe().is_file(),
            f if f == ENGINE_SOURCE.file => self.runtime_dir.join(SERVER_EXE).is_file(),
            f if f == ENGINE_ROCM_SOURCE.file => self.engine_dir_for(GpuBackend::Rocm).join(SERVER_EXE).is_file(),
            f if f == ENGINE_VULKAN_SOURCE.file => self.engine_dir_for(GpuBackend::Vulkan).join(SERVER_EXE).is_file(),
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
    /// Required: the engine for this GPU (+ the CUDA runtime on NVIDIA) + the three model
    /// files. Then the extras that the edit/expand/describe features and the LoRA card use.
    /// On Linux the CUDA engine and runtime come from this repository's own release (see
    /// `linux_engine_release!`); Windows also installs the Visual C++ runtime first.
    pub fn setup_items(&self) -> Vec<(&'static ModelSource, bool)> {
        let mut v: Vec<(&'static ModelSource, bool)> = Vec::new();
        #[cfg(windows)]
        v.push((&VCREDIST_SOURCE, true));
        let backend = self.backend();
        v.push((engine_source(backend), true));
        if backend == GpuBackend::Cuda {
            v.push((&CUDART_SOURCE, true));
        }
        v.extend(self.required_models().into_iter().map(|m| (m, true)));
        v.push((&VISION_SOURCE, false));
        v.extend(crate::lora::RECOMMENDED.iter().map(|(s, _)| (*s, false)));
        v.push((&CAPTION_SOURCE, false));
        v
    }

    /// Off CUDA the INT8 model can't run: fall back to Q4 (the choice is hidden there).
    pub fn fit_to_gpu(&mut self) {
        if self.variant == ModelVariant::Int8 && !self.int8_supported() {
            self.variant = ModelVariant::Q4;
        }
    }

    /// Engine (+ CUDA runtime on NVIDIA) and models are all present: the app can generate.
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

/// Physical memory installed (Windows) or available to this system (Linux, incl. a WSL VM's limit).
#[cfg(windows)]
pub fn total_ram_bytes() -> Option<u64> {
    use windows_sys::Win32::System::SystemInformation::{GlobalMemoryStatusEx, MEMORYSTATUSEX};
    let mut m: MEMORYSTATUSEX = unsafe { std::mem::zeroed() };
    m.dwLength = std::mem::size_of::<MEMORYSTATUSEX>() as u32;
    // SAFETY: `m` is a valid MEMORYSTATUSEX with dwLength set, as the API requires.
    (unsafe { GlobalMemoryStatusEx(&mut m) } != 0).then_some(m.ullTotalPhys)
}

#[cfg(target_os = "linux")]
pub fn total_ram_bytes() -> Option<u64> {
    let mut i: libc::sysinfo = unsafe { std::mem::zeroed() };
    // SAFETY: `i` is a valid out-pointer for sysinfo(2).
    if unsafe { libc::sysinfo(&mut i) } != 0 {
        return None;
    }
    #[allow(clippy::unnecessary_cast)] // field widths differ between libc targets
    Some(i.totalram as u64 * i.mem_unit as u64)
}

#[cfg(not(any(windows, target_os = "linux")))]
pub fn total_ram_bytes() -> Option<u64> {
    None
}

/// Physical memory free for a new process right now (Windows `ullAvailPhys`, Linux
/// `MemAvailable`): RAM the browser and other apps aren't using.
#[cfg(windows)]
pub fn available_ram_bytes() -> Option<u64> {
    use windows_sys::Win32::System::SystemInformation::{GlobalMemoryStatusEx, MEMORYSTATUSEX};
    let mut m: MEMORYSTATUSEX = unsafe { std::mem::zeroed() };
    m.dwLength = std::mem::size_of::<MEMORYSTATUSEX>() as u32;
    // SAFETY: `m` is a valid MEMORYSTATUSEX with dwLength set, as the API requires.
    (unsafe { GlobalMemoryStatusEx(&mut m) } != 0).then_some(m.ullAvailPhys)
}

#[cfg(target_os = "linux")]
pub fn available_ram_bytes() -> Option<u64> {
    let info = std::fs::read_to_string("/proc/meminfo").ok()?;
    let kb: u64 = info.lines().find_map(|l| l.strip_prefix("MemAvailable:"))?.trim().trim_end_matches("kB").trim().parse().ok()?;
    Some(kb * 1024)
}

#[cfg(not(any(windows, target_os = "linux")))]
pub fn available_ram_bytes() -> Option<u64> {
    None
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
        .find(|d| ["", "vulkan", "rocm"].iter().any(|sub| d.join("runtime").join(sub).join(SERVER_EXE).is_file()))
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

    #[test]
    fn slow_gpu_defaults_pick_fast_size_and_lightning() {
        let mut p = super::GenParams::default();
        p.size = 2;
        p.loras.push(crate::lora::LoraSel { file: crate::lora::LIGHTNING_FILE.into(), weight: 0.5, enabled: false });
        p.apply_slow_gpu_defaults();
        assert_eq!(p.dimensions(), (736, 736));
        assert_eq!(p.loras.len(), 1);
        assert!(p.loras[0].enabled && (p.loras[0].weight - 1.0).abs() < 1e-6);
        assert!(super::has_tensor_cores("NVIDIA GeForce RTX 3050"));
        assert!(!super::has_tensor_cores("NVIDIA GeForce GTX 1650"));
    }

    #[test]
    fn q3_image_model_only_on_4gb_card_short_of_ram() {
        use super::wants_small_diffusion as small;
        let gb = |n: u64| Some(n << 30);
        assert!(small(gb(4), gb(8)));
        assert!(!small(gb(4), gb(16)), "enough RAM: Q4 is faster");
        assert!(!small(gb(8), gb(8)), "8 GB card keeps Q4");
        assert!(!small(gb(4), None), "unknown RAM keeps Q4");
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
    /// Defaults for a GTX card: the "Fast" size (736×736 square) and Lightning 4 steps,
    /// about 4× less work than 1024² in 8 steps.
    pub fn apply_slow_gpu_defaults(&mut self) {
        self.size = 0;
        self.custom_size = false;
        let file = crate::lora::LIGHTNING_FILE;
        for l in &mut self.loras {
            if crate::lora::conflicts(&l.file, file) {
                l.enabled = false;
            }
        }
        match self.loras.iter_mut().find(|l| l.file == file) {
            Some(l) => {
                l.enabled = true;
                l.weight = 1.0;
            }
            None => self.loras.push(crate::lora::LoraSel { file: file.to_owned(), weight: 1.0, enabled: true }),
        }
    }

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
