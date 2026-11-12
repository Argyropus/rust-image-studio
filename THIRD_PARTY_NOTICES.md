# Third-party components

Rust Image Studio's own source code is MIT-licensed (see [LICENSE](LICENSE)).
This repository does **not** contain any model weights or inference binaries. On first run the app
downloads the model files from Hugging Face and the inference engines from GitHub releases (upstream's
for Windows; this repository's own `engine-sd-master-1052-f5df5f8` release for
the Linux engine, built by `.github/workflows/linux-engine.yml`). Each keeps its own license, listed below.
Check them before any commercial use.

## Models (downloaded by the app)

| File | Source | License |
|---|---|---|
| `qwen-image-2.1-turbo-Q4_K_M.gguf` | [unsloth/Qwen-Image-2.1-Turbo-GGUF](https://huggingface.co/unsloth/Qwen-Image-2.1-Turbo-GGUF) | Qwen license of the original model, see the model card |
| `qwen_image_2.1_turbo_int8_convrot.safetensors` (optional) | [Comfy-Org/Qwen-Image-2.1](https://huggingface.co/Comfy-Org/Qwen-Image-2.1) | Qwen license of the original model, see the model card |
| `qwen_image_2.1_vae_bf16.safetensors` | [unsloth/Qwen-Image-2.1-FP8](https://huggingface.co/unsloth/Qwen-Image-2.1-FP8) | Qwen license of the original model, see the model card |
| `Qwen3-VL-8B-Instruct-UD-Q4_K_XL.gguf`, `mmproj-F16.gguf` | [unsloth/Qwen3-VL-8B-Instruct-GGUF](https://huggingface.co/unsloth/Qwen3-VL-8B-Instruct-GGUF) | See the model card |

## LoRAs (optional, downloaded from the LoRA card)

| LoRA | Source | License |
|---|---|---|
| Lightning 4-step | [lightx2v/Qwen-Image-2.1-Lightning](https://huggingface.co/lightx2v/Qwen-Image-2.1-Lightning) | See the model card |
| Edit consistency | [ausboss/Qwen-Image-2.1-Consistency-LoRA](https://huggingface.co/ausboss/Qwen-Image-2.1-Consistency-LoRA) | Qwen research license (see the model card) |
| Multiple angles v2 | [akhaliq/Qwen-Image-2.1-Multiple-Angles-LoRA](https://huggingface.co/akhaliq/Qwen-Image-2.1-Multiple-Angles-LoRA) | Apache-2.0 (see the model card) |

## Inference engines (downloaded by the user into `runtime/`)

| Component | Source | License |
|---|---|---|
| stable-diffusion.cpp `master-1052-f5df5f8` (Windows: `sd-server.exe`, `stable-diffusion.dll`, ggml DLLs from upstream's release zip; Linux: `sd-server`, `libstable-diffusion.so`, ggml libraries built from that tag by this repository's CI, unmodified; the archive includes the MIT license texts) | [leejet/stable-diffusion.cpp](https://github.com/leejet/stable-diffusion.cpp), [ggml](https://github.com/ggml-org/ggml) | MIT |
| stable-diffusion.cpp `master-1052-f5df5f8` AMD builds, only when an AMD GPU or Vulkan is used (`runtime/rocm`: upstream's Windows ROCm 10.1.0 zip or Linux Ubuntu 24.04 ROCm zip; `runtime/vulkan`: upstream's Windows or Linux Ubuntu 26.04 Vulkan zip; unmodified) | [leejet/stable-diffusion.cpp](https://github.com/leejet/stable-diffusion.cpp), [ggml](https://github.com/ggml-org/ggml) | MIT |
| AMD HIP runtime DLLs inside upstream's Windows ROCm zip (`amdhip64_7.dll`, `amd_comgr.dll`, `rocm_kpack.dll`), downloaded with that zip, not redistributed by this repository | AMD ROCm | AMD's ROCm license terms for those components (e.g. HIP runtime: MIT; Code Object Manager: University of Illinois/NCSA) |
| Microsoft Visual C++ 2015–2022 Redistributable (Windows only, installed by the first-run setup when missing; downloaded from `aka.ms/vs/17/release/vc_redist.x64.exe`) | Microsoft | Microsoft Software License Terms (Visual C++ Redistributable) |
| NVIDIA CUDA runtime (Windows: `cudart64_12.dll`, `cublas64_12.dll`, `cublasLt64_12.dll` from upstream's release; Linux: `libcudart.so.12`, `libcublas.so.12`, `libcublasLt.so.12` from CUDA 12.8, in this repository's engine release) | NVIDIA | NVIDIA CUDA Toolkit EULA (these libraries are listed as redistributable in its Attachment A) |

The NVIDIA or AMD driver itself (`libcuda.so.1`, the Vulkan ICD, on Linux the ROCm 7 libraries the ROCm build
links) is never downloaded or redistributed; it comes from the user's system. If you publish the Linux engine release from a fork, you redistribute the CUDA libraries above
and must follow the CUDA Toolkit EULA's distribution terms.

## Fonts (built into the app)

| Font | Source | License |
|---|---|---|
| Nanum Gothic Regular and Bold (`assets/fonts/`, compiled into the binary: always used on Linux, and on Windows when Malgun Gothic is missing) | [google/fonts · ofl/nanumgothic](https://github.com/google/fonts/tree/main/ofl/nanumgothic), © NHN Corporation | SIL Open Font License 1.1 ([assets/fonts/OFL.txt](assets/fonts/OFL.txt)) |

## Rust crates

The app is built from crates on crates.io (eframe/egui, image, serde, ureq, …), mostly under
MIT and/or Apache-2.0. `Cargo.lock` lists the exact versions; a full report can be generated
with [`cargo about`](https://github.com/EmbarkStudios/cargo-about).
