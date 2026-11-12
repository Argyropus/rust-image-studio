# Rust Image Studio UI reference

A technical reference for every control, menu and option in the application. New users can cover the core workflow with [1. First launch and setup](#1-first-launch-and-setup) and [3. Control panel](#3-control-panel) alone.

**English** · [한국어](GUIDE.ko.md) · [Back to the README](../README.md)

**Contents**
[0. Interface layout](#0-interface-layout) ·
[1. First launch and setup](#1-first-launch-and-setup) ·
[2. Top bar](#2-top-bar) ·
[3. Control panel](#3-control-panel) ·
[4. Canvas](#4-canvas) ·
[5. Gallery](#5-gallery) ·
[6. Settings](#6-settings) ·
[7. Help and engine log](#7-help-and-engine-log) ·
[8. Drag and drop, clipboard](#8-drag-and-drop-clipboard) ·
[9. Keyboard shortcuts](#9-keyboard-shortcuts) ·
[10. File system layout](#10-file-system-layout) ·
[11. Notifications and troubleshooting](#11-notifications-and-troubleshooting)

---

## 0. Interface layout

```
┌──────────────────────────────────────────────────────────────────────┐
│ Top bar: app name · model │ engine status │ 한/EN │ ? 📁 log ⚙Settings │
├──────────────┬───────────────────────────────────────┬───────────────┤
│ Control panel │ Canvas                                │ Gallery       │
│ 5 modes       │ result image · progress               │ search        │
│ input image   │ inpaint brush · expand preview        │ thumbnails    │
│ prompt        │                                       │               │
│ aspect/size … │                                       │               │
│ [Generate]    │                                       │               │
└──────────────┴───────────────────────────────────────┴───────────────┘
```

- The control panel and the gallery can be resized by dragging their borders.
- Language, settings, prompt, resolution, seed, LoRA configuration and gallery thumbnail size persist on exit and are restored in the next session.
- On Linux, a few icons fall back to alternative glyphs depending on the coverage of the bundled fonts (e.g. ✕ → ×, 🖌 → ✏). Functionality is identical.

---

## 1. First launch and setup

Run `RustImageStudio.exe` from the extracted directory. The **First-run setup** screen appears and component provisioning starts automatically.

| Element | Function |
|---|---|
| Overall progress bar | Bytes received / total and percent. Closing the window is safe: the next launch **resumes**. |
| **Required** | Inference engine, CUDA runtime, image model, text encoder, VAE. On Windows the Visual C++ runtime is installed first, only if missing (one UAC approval). On an AMD Radeon (experimental) the engine is the ROCm or Vulkan build instead and there is no CUDA runtime; see [GPU](#6-settings). |
| **Extras** | Vision module (edit, expand, photo description), the 3 recommended LoRAs, the captioning engine. A failure here leaves core generation available; the component can be retried when its feature is used. |
| Per-item status | ✔ done · percent · "extracting…" · "installing…" · "failed" |
| **Stop** | Halts provisioning; it does not resume automatically during the current session. |
| **⬇ Start download** / **⟳ Try again** | Resumes provisioning after a stop or an error. Partially received files continue via HTTP Range requests. |
| Graphics card line | With the NVIDIA CUDA engine and no NVIDIA driver: a warning with **NVIDIA driver** / **AMD driver** links (the driver is the only component the app cannot install). With the ROCm or Vulkan engine: the detected card, the engine, and a note that AMD/Vulkan support is experimental and the INT8 model is NVIDIA-only. |
| Insufficient disk space (red) | The start button stays disabled until the required space + 1 GB is free. |
| **Already have the files? Set the paths in Settings →** | Point [Settings](#6-settings) at existing model files to skip re-downloading. |
| (Linux) system component check | Names the package to install if glibc, `libstdc++` or `libvulkan` is missing (for the AMD engines also `libgomp`, and for ROCm `/dev/kfd` and the ROCm 7 libraries). If the engine download fails, it offers manual build commands with a **📋 Copy commands** button. |

Once provisioning completes, the engine starts automatically and "Engine ready — you can generate images" is shown.

---

## 2. Top bar

| Element | Function |
|---|---|
| Model name | The currently loaded generation model (`Q4_K_M` or `INT8 tensor cores`). |
| **Engine status** | **Ready** (green) · **Generating · N** · **Loading model · N s** (orange) · **Engine off · click to start** (grey; click to start) · **Error · click to restart** (red; hover for details). |
| **한 / EN** | Switches the UI language instantly, including example prompts and tips. |
| **?** | Shortcuts and tips (F1). |
| 📁 folder | Opens the output directory. |
| log | Opens the engine log. Attach it when reporting an issue. |
| ⚙ **Settings** | Model paths, memory policy and more. See [6. Settings](#6-settings). |

---

## 3. Control panel

From top to bottom: **modes → input image → prompt → aspect/size → seed/count → advanced → LoRA**, with the **fast mode · model · Generate** area pinned at the bottom. Each card folds by clicking its title, and a folded card keeps summarizing its current values on the title line.

### 3.1 Generation modes

| Mode | Behavior | Shortcut |
|---|---|---|
| **Text** | Generates a new image from the prompt alone (text-to-image). | Ctrl+1 |
| **Variation** | Redraws the **whole** input picture: similar composition and colors, new details (img2img). For similar alternatives or a style change. | Ctrl+2 |
| **Edit** | Edits the input according to a natural-language instruction ("replace the background with a beach"). Up to 3 reference images; requires the vision module. | Ctrl+3 |
| **Inpaint** | Redraws **only** the brushed area and keeps the rest unchanged. For fixing one spot. | Ctrl+4 |
| **Expand** | Extends the canvas and continues the scene (outpainting). The source region keeps its native resolution. | Ctrl+5 |

- Adding an image in Text mode switches to Edit (or Variation when the vision module is absent).
- Variation, Inpaint and Expand use a single input image.

### 3.2 Input image card (hidden in Text mode)

| Element | Function |
|---|---|
| **📋 Paste** | Uses the clipboard image as input (Ctrl+V). |
| **📂 Open** | File picker (png, jpg, webp, bmp; multi-select). |
| Drop zone | Drop an image or click to choose one. Gallery right-click → **Use as input image** also works. |
| ✕ on a thumbnail | Removes that input. |
| **＋** (Edit mode) | Adds a reference image. Up to 3; the index badge (1, 2, 3) is how the prompt refers to them ("image 1", "the second image"). |

**Variation only**
- **Strength** 0.10–1.00 (default 0.60): higher values diverge further from the source. The number of steps actually run is shown below; ↺ restores the recommended value.

**Edit only**
- Without the vision module, a **⬇ Download vision module** button (about 1.2 GB) appears; the engine restarts automatically once it arrives.
- The first edit request may restart the engine once to load the vision module.

**Inpaint only**
- **🖌 Brush** / **🧽 Eraser**: paint or erase the mask.
- **Invert**: swaps masked and unmasked regions.
- **Clear all**: resets the mask.
- **Brush size** 4–400 px (default 60). Ctrl+wheel or `[` `]` over the canvas also adjust it.
- **Strength** 0.30–1.00 (default 0.85). 0.8–0.9 recommended; at 1.0 changes can bleed past the mask edge.
- **🖌 Back to mask editing**: returns from the result view to the mask editor.

**Expand only**
- **Quick presets**: `↔ 16:9` (horizontal) · `↕ 9:16` (vertical) · `⬜ 1:1` (square) · `4:3` · `⤢ Zoom out` (1.5× on all sides). If the input already has that aspect, it expands slightly on all sides.
- **Left · Right · Top · Bottom** sliders (0–100 %): per-side expansion. Also adjustable by dragging in the canvas preview.
- Resolution line: source → result resolution, number of passes and estimated time.
- **Switch to INT8**: shown when the INT8 model is present and the engine is NVIDIA CUDA; expansion runs about 35 % faster.
- **↔ Back to expand preview**: returns from the result view to the expand preview.
- To minimize failures, Expand is fixed to Turbo 8 steps · CFG 1 · LoRA off · fast mode off, so the advanced and LoRA cards are hidden.

### 3.3 Prompt card

| Element | Function |
|---|---|
| **✨ Example** | Inserts an example prompt for the current mode, cycling on each click. |
| **🖼 Describe photo** (Expand) | Captions the input image on the GPU and inserts the scene description (about 20 s). Deferred until a running generation finishes. |
| **Text** | Appends a template for rendering text inside the image. Replace `"YOUR TEXT"` with the desired wording. |
| **🕘 Recent** | Prompt history (15 shown, 30 kept). Click to reuse, ✕ next to an entry to delete it, **🗑 Clear all recent prompts** to purge the history. |
| **✕** | Clears the prompt field. |
| Prompt field | Shows mode-specific authoring guidance as placeholder text, with a character count below. |
| **Multiple prompts** | One prompt per line, generated in order. The image count applies per prompt. |
| **💡 Tips** | On hover, six prompt-writing tips validated through same-seed comparisons. |
| **Negative prompt** | Elements to exclude (blurry, low quality, …). Ineffective at CFG 1; raise CFG under Advanced settings. |

- In Expand, the prompt may be **left empty**: PNGs produced by this app supply the scene description stored in their metadata, and other photos are captioned automatically. Prompts typed by the user are never overwritten.

### 3.4 Aspect · Size card

| Element | Function |
|---|---|
| **Follow the input image's aspect** | In image-based modes (on by default), keeps the input's aspect ratio. |
| Aspect tiles | `1:1` square · `4:3` landscape · `3:4` portrait · `16:9` wide · `9:16` vertical video. |
| **Fast / Standard / High res** | Output resolution (about 0.5 / 1.0 / 1.6 megapixels). Standard by default (1024×1024 at 1:1). |
| **Custom size** | Explicit width and height (256–2048, snapped to multiples of 32). ⇄ swaps them. |

In Expand, the output aspect ratio is determined by the expansion, so only the resolution is selected here.

### 3.5 Seed · Count card

| Element | Function |
|---|---|
| **Random / Fixed** | Random draws a new seed per generation; Fixed reproduces results deterministically. The last seed remains in the field after a random run, so switching to Fixed reproduces it immediately. |
| Seed value | Drag or double-click to edit. Batches use seed, seed+1, … |
| 🎲 | Draws a new seed and fixes it. |
| **Count 1–8** | Images per request. |

### 3.6 Advanced settings card (folded by default)

| Element | Function |
|---|---|
| **Sampling steps** | **Turbo 8 steps (recommended)** or **Custom** (4–40). |
| **CFG** | 1.0–8.0 (default 1.0). The Turbo model is tuned for CFG 1. Higher values enable the negative prompt at roughly twice the generation time. |
| **Live preview while generating** | Displays intermediate results at each sampling step (on by default). |
| **↺ All recommended** | Restores steps, CFG, strengths, LoRA strength and live preview to their recommended values. Prompt, resolution and seed are kept. |

### 3.7 LoRA card

A LoRA is a lightweight adapter that adds a specific capability to the base model. Files in `models/loras` are listed here.

| Element | Function |
|---|---|
| **Open folder** | Opens the LoRA directory. Place `.safetensors` files there. |
| ⟳ | Rescans the directory. New files are usable immediately, without restarting the engine. |
| Checkbox + name | Enable/disable. The value on the right is the **strength** (0.00–2.00, default 1.00). |
| **Lightning 4-step** | 4-step sampling at about 12 s per image (8 steps: about 18 s). Excluded automatically in Variation and Inpaint. |
| **Edit consistency** | Locks the composition and positions of an edit to the source. Disable it for large object removal or pose changes. Edit mode only. |
| **Multiple angles** | Re-shoots a single input from another camera angle (background switches to a grey studio). Enables a **camera direction dial** (12 directions), **Camera height** (eye level · 30° · 60° · top) and **Closer (close-up)**, and composes the prompt from the selection. If the angle does not change, lower the strength to 0.8. Edit mode only. |
| **To Edit mode** | Shortcut shown when an edit-only LoRA is enabled in another mode. |
| **Download recommended LoRAs** | One-click download of recommended LoRAs not yet present; they are enabled automatically on arrival. |

Mutually exclusive LoRAs (two speed LoRAs, or "Edit consistency" with "Multiple angles") disable each other, with a notification.

### 3.8 Pinned bottom area: fast mode · model · Generate

| Element | Function |
|---|---|
| **⚡ Fast mode** | A Spectrum step cache replaces some of the 8 steps with predictions for about a 23 % speed-up, with deviations that are barely perceptible. Not applied in Expand. |
| **Model: … ▾** | **Standard · Q4_K_M** (minimal RAM use) or **High quality · fast · INT8 tensor core** (faster repeated generation, quality closer to the original model). Switching restarts the engine (about 40 s). Missing model files can be downloaded from Settings. Shown only with the NVIDIA CUDA engine. |
| **✨ Generate** | Submits a request (Ctrl+Enter). While generating it becomes **＋ Add to queue** for sequential processing; requests can be queued even while the model is loading. |
| Status line | Resolution · seed · LoRA count · estimated time per image. With jobs active: **N in progress** and **■ Cancel all**. |

When a request is rejected, a notification states why (missing prompt, missing input image, no inpaint mask, …).

---

## 4. Canvas

### 4.1 Start screen

- **Try the sample · swap a phone case**: **Load sample** configures a model photo, a case design and the instruction in Edit mode. Only **Generate** remains.
- **Start from an example**: three example prompts; clicking one inserts it.

### 4.2 Result view and toolbar

The prompt and metadata (mode · resolution · seed · duration · timestamp) appear above the image, with the toolbar below. In narrow windows only icons are shown; hovering reveals the names.

| Button | Function |
|---|---|
| **Load** | Applies the image's prompt, seed (fixed), resolution, mode, LoRA configuration and more to the control panel, for reproduction or fine-tuning. |
| **Expand ▾** | Expands the image and generates immediately: horizontal (16:9) · vertical (9:16) · square (1:1) · 4:3 · zoom out (1.5× all sides). **⚙ Adjust manually…** switches to Expand mode without generating. |
| **Blend in** | Re-lights a composited-looking phone case to match the scene's lighting, reflections and shading. Uses the case image (reference 2) from the previous edit to preserve the design and generates immediately (about 75 s). |
| **Use as input** | Sets the image as the input for Variation or Edit. |
| **Edit mask** (Inpaint) | Returns to the mask editor. |
| 🔍 Full view | Full-screen viewer (F). |
| Copy | Copies the image to the clipboard (Ctrl+Shift+C). |
| Save as | PNG (byte-for-byte, with generation metadata) · WebP (lossless, compact) · JPEG (quality 92, for web publishing) (Ctrl+S). |
| Show in folder | Opens the containing directory. |
| 🗑 Delete | Moves the file to the Trash after confirmation (restorable). |

Double-clicking the image also opens the full-screen viewer.

### 4.3 During generation

A preview and a progress bar show the stage (encoding prompt → sampling N/M → decoding), elapsed time and estimated time remaining. **Cancel** stops the current job only. Multi-pass expansions display "Expand pass k/n", and blank new regions are detected and regenerated automatically.

### 4.4 Inpaint mask editor

- Left-click paints the mask, right-click erases it. The masked region is shown in translucent red.
- Ctrl+wheel or `[` `]` adjusts the brush size.
- **Show result**: displays the image selected in the gallery.

### 4.5 Expand preview

- Drag an edge or corner outward to expand, inward to shrink. Dragging anywhere inside the image adjusts the nearest side.
- ◀ ▶ ▲ ▼ arrows: click for +10 % in that direction, right-click for −10 %.
- Hatched areas mark the regions to be generated, with the per-side percentage displayed.
- **↺ Reset**: sets all sides back to 0. **Show result**: switches to the result view.

### 4.6 Full-screen viewer

Wheel zooms around the cursor, drag pans, double-click resets the zoom, ←/→ navigates to the previous/next image, and Esc or ✕ in the corner closes it.

---

## 5. Gallery

| Element | Function |
|---|---|
| Count · thumbnail size button | Toggles small/large thumbnail grids. |
| **🔍 Search prompts** | Filters by substring in the prompt. |
| In-progress / waiting tiles | Listed first. |
| Thumbnail | Click to select, double-click for the full-screen viewer. Hover shows the prompt and metadata. |
| 🗑 on a thumbnail | Moves the file to the Trash **without confirmation** (same as the Delete key). Restorable from the Trash. |
| Right-click menu | ♻ Load these settings · 🖼 Use as input image · 🔍 Full view · 📋 Copy image · 💾 Save as · 📂 Show in folder · 🗑 Delete (with confirmation). |

←/→ steps through the gallery.

---

## 6. Settings

Open via ⚙ **Settings** in the top bar. Changes take effect only on **Save and restart engine**; closing with ✕ or Esc discards them.

| Item | Function |
|---|---|
| **GPU** | Engine build: **Automatic → …** (shows what it picks), **NVIDIA CUDA**, **AMD ROCm (experimental)**, **Vulkan (experimental)**. Automatic uses CUDA on NVIDIA; on AMD Radeon it uses ROCm on a supported card with the ROCm 7 libraries installed (Windows: RX 6000/7000/9000 series and `hipblas.dll` from ROCm 7; Linux: ROCm 7), otherwise Vulkan. Below: the detected card. Switching downloads that engine on save (Vulkan about 30–40 MB, ROCm about 270–300 MB) into `runtime\vulkan\` or `runtime\rocm\`. If the engine finds no GPU, an error suggests another option here. |
| **Generation model** | **Standard · Q4_K_M (4.2 GB)**: minimal RAM use. **High quality · fast · INT8 tensor core (7.3 GB)**: faster per step, quality closer to the original model. If the file is missing, a **⬇ Download (7.3 GB)** button appears. The INT8 option (and the **Model** menu next to Generate) is hidden unless the engine is NVIDIA CUDA. |
| **File paths** | Engine folder · Q4 model · INT8 model (CUDA only) · text encoder · VAE · output folder. 📂 browses; ✔ found / ✖ Missing / New folder is shown on the right. Changing the output folder rebuilds the gallery from that location. |
| **Memory · Performance** | Describes the memory placement policy (VRAM capped at 6 GB). |
| **Warm up at startup (recommended)** | Generates one low-resolution image right after the engine starts to cut first-image latency (about 36 s → 18 s). |
| **Extra arguments** | Engine arguments for advanced users (e.g. `--max-vram 7`). Leave empty under normal use. |
| **Defaults** | Restores default settings (applied on save; prompt history is kept). |
| **Save and restart engine** | Applies the changes and restarts the engine. Running jobs are cancelled. |

The UI language is switched with **한 / EN** in the top bar, not in Settings.

---

## 7. Help and engine log

- **Help (? or F1)**: shortcut table and four usage tips.
- **Engine log**: the engine process's output. **📋 Copy all** copies it to the clipboard and **Clear** empties the view. Errors are highlighted in red.
- **🖥 Copy system info**: copies a short summary for bug reports: app version, graphics card and its memory, RAM, engine, how the weights were placed, and the last image's time. It contains no paths, prompts or images.
- **New-version badge**: when a newer release exists, the top bar shows "vX.Y.Z available"; click it to open the download page. Only the executable changes, your models and settings stay. The check asks GitHub for the latest version number once at startup and can be turned off in Settings.

---

## 8. Drag and drop, clipboard

- Drop image files **anywhere in the window** to set them as input (up to 3 at once).
- Dropping a **PNG produced by this app** reports its embedded generation metadata and offers:
  - **♻ Load settings**: applies its prompt, seed, resolution, LoRA configuration and more.
  - **🖼 Use as input image**: uses the image only.
  - **Both** / **Cancel**.
- Ctrl+V outside a text field sets a clipboard image as input; inside a text field it pastes text.

---

## 9. Keyboard shortcuts

| Key | Action |
|---|---|
| Ctrl+Enter | Generate (add to queue while generating) |
| Ctrl+1 … 5 | Text · Variation · Edit · Inpaint · Expand |
| F1 | Help |
| Ctrl+V | Clipboard image as input |
| ← / → | Previous / next image in the gallery |
| F | Full-screen viewer |
| Ctrl+S | Save as |
| Ctrl+Shift+C | Copy image |
| Delete | Move the selected image to the Trash (no confirmation) |
| Esc | Close the viewer · Settings · dialogs |
| `[` / `]`, Ctrl+wheel | Inpaint brush size |

---

## 10. File system layout

All data is stored alongside the executable, so the installation is portable and can be moved or backed up as a single directory.

| Path | Contents |
|---|---|
| `runtime\` | Inference engine and CUDA runtime (`runtime\llama\` holds the captioning engine; the AMD builds go in `runtime\rocm\` and `runtime\vulkan\`) |
| `models\` | Image model, text encoder, VAE, vision module |
| `models\loras\` | LoRA weights. Rescan with ⟳ in the LoRA card after adding files. |
| `outputs\` | Generated images (`date_time_seed.png`), gallery index (`history.json`), thumbnail cache (`.thumbs`) |
| Settings | Windows `%APPDATA%\Rust Image Studio`, Linux `~/.local/share/rustimagestudio` |

Generated PNGs embed their generation metadata (prompt, seed, resolution, LoRA configuration), so dropping one back in restores the exact settings.

---

## 11. Notifications and troubleshooting

Notifications appear at the bottom center for 4 seconds. Status colors: **green = success**, **orange = warning**, **red = error**, **grey = info**.

| Symptom | Resolution |
|---|---|
| Top bar shows **Engine off** | Click the status to start the engine (about 40 s). |
| Top bar shows **Error** | Click to restart. If it recurs, collect the log via **log → 📋 Copy all** and report it. |
| "The ROCm engine needs the ROCm 7 libraries" / "…are built for another version" | Windows, **AMD ROCm** chosen: `hipblas.dll` and `rocblas.dll` from ROCm 7 were not found, or only a ROCm 6 copy was. Install ROCm 7 ([AMD's guide](https://rocm.docs.amd.com/projects/install-on-windows/en/latest/)) and restart the app, or pick **Vulkan** in **Settings → GPU**, which needs only the graphics driver. |
| "The engine (…) found no GPU to use" | The ROCm or Vulkan engine saw no usable GPU. Update the graphics driver, or pick another engine in **Settings → GPU** (e.g. Vulkan instead of ROCm). |
| Generate is disabled | The engine is off or in an error state; click the status in the top bar. |
| Edit mode does not run | The vision module is required: run **⬇ Download vision module** in the input image card. |
| "Some extra files couldn't be downloaded" | Core generation works; the component can be fetched again when its feature is used. |
| An image was deleted by mistake | Restore it from the Windows Recycle Bin (Trash on Linux). |
| Model files are already available | Specify them under **File paths** in Settings to avoid re-downloading. |

For the content policy, see the README's [Content safety](../README.md#content-safety) section.
