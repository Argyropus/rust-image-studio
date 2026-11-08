//! Main window: prompt controls (left), canvas (center), gallery (right).

use crate::config::{
    ASPECTS, GenMode, GenParams, INT8_SOURCE, ModelVariant, SIZES, Settings, StepMode, VISION_SOURCE, snap32,
};
use crate::inputs::{self, InputImage, Loaded, MAX_INPUTS};
use crate::download::{self, DlEvent, DlTask};
use crate::engine::{Engine, EngineEvent, EngineState, GenJob};
use crate::history::{History, HistoryEntry};
use crate::theme::{self, *};
use egui::{
    Align, Align2, Color32, ColorImage, Context, CursorIcon, FontId, Frame, Id, Key, KeyboardShortcut,
    Layout, Margin, Modifiers, Order, Panel, Pos2, Rect, RichText, ScrollArea, Sense, Stroke,
    TextEdit, TextureHandle, TextureOptions, Ui, Vec2, vec2,
};
use std::collections::{HashMap, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::time::Instant;

// Korean examples written in the order that tested best:
// subject → details → background → lighting/mood → camera or art style.
const EXAMPLES: [&str; 9] = [
    // Step 1 of the tested product-shot workflow: get the pose from text, swap the case in edit mode.
    "스튜디오에서 휴대폰을 오른손에 들고 얼굴 옆에서 휴대폰 뒷면을 카메라를 향해 보여주는 30대 여성 모델, 흰 블라우스, 어깨까지 오는 단발머리, 밝은 회색 배경, 상품 촬영 조명",
    "해 질 녘 한옥 카페, 처마 끝에 걸린 따뜻한 한지 등불, 돌길 위로 내리는 가랑비, 시네마틱 조명, 35mm 필름 사진",
    "황금빛 해 질 녘 라벤더 들판에 웅크린 붉은 여우, 부드러운 수채화 일러스트, 따뜻한 색감",
    "호두나무 책상 위 도자기 머그잔 제품 사진, 아침 햇살, 얕은 피사계 심도, 깔끔한 배경",
    "폭포가 흐르는 작은 공중 섬 도시, 아이소메트릭 디오라마, 아주 정교한 디테일, 스튜디오 조명",
    "주름진 얼굴의 노어부 인물 사진, 극적인 렘브란트 조명, 검은 배경",
    "밤 시장 포장마차, 노란 간판에 \"맛있는 집\"이라는 한글이 선명하게 보인다, 피어오르는 김, 전구 줄 조명",
    "봄날 강변을 따라 핀 벚꽃 수채화, 부드러운 붓 터치, 흰 종이 질감",
    "카페 입구의 나무 간판 클로즈업. 간판 문구: \"행복한 하루\". 따뜻한 오후 햇살",
];

const VARIATION_EXAMPLES: [&str; 4] = [
    "같은 장면을 따뜻한 수채화 그림으로, 부드러운 붓 터치, 흰 종이 질감",
    "같은 장면을 부드러운 파스텔 톤 3D 클레이 렌더로, 둥근 형태, 스튜디오 조명",
    "같은 장면을 흑백 필름 사진으로, 거친 입자, 높은 대비",
    "같은 장면을 크레용으로 그린 아이 그림처럼, 알록달록한 색, 종이 질감",
];

const EDIT_EXAMPLES: [&str; 7] = [
    // Step 2: ref 1 = model already holding a phone, ref 2 = case image (tested: pose kept, case swapped).
    "1번 이미지 속 휴대폰의 케이스를 2번 이미지의 케이스 디자인과 똑같이 바꿔줘. 인물, 자세, 배경은 그대로 유지해.",
    "배경을 야자수가 있는 햇살 가득한 해변으로 바꿔줘. 인물과 구도는 그대로 유지해.",
    "전체를 종이 공예(페이퍼 컷아웃) 그림으로 바꿔줘. 구도는 그대로 유지해.",
    "간판 문구를 \"오늘도 맑음\"으로 바꿔줘. 나머지는 그대로 유지해.",
    "인물에게 빨간 목도리를 둘러줘. 얼굴과 배경은 그대로 유지해.",
    "시간대를 노을 지는 저녁으로 바꿔줘. 하늘은 주황빛, 조명은 따뜻하게.",
    "배경을 깔끔한 흰색 스튜디오로 바꿔줘. 제품은 그대로 유지해.",
];

/// Prompt-writing tips verified with this model (see README).
const PROMPT_TIPS: [(&str, &str); 6] = [
    (
        "제품을 든 모델 컷 (2단계)",
        "① 텍스트 생성으로 휴대폰을 손에 든 자세를 먼저 만들고 ② 🖼 입력으로 → 2번에 케이스 이미지 추가 → \"1번 이미지 속 휴대폰의 케이스를 2번 이미지의 케이스 디자인과 똑같이 바꿔줘\". 한 번에 합성하면 손에 들지 않고 옷 위에 붙어 버립니다. 편집 때는 ⚡ 빠른 모드를 끄세요.",
    ),
    ("자세히 쓰기", "주제 → 세부 묘사 → 배경 → 조명·분위기 → 촬영 방식/화풍 순서로 쓰면 잘 따라옵니다."),
    ("짧은 단어만 쓰지 않기", "\"고양이\"보다 \"창가 햇살 아래 통통한 치즈 고양이, 털 결이 선명한 클로즈업, 35mm 필름 사진\"이 훨씬 좋습니다."),
    ("한글 글씨 넣기", "물체를 먼저 쓰고 문구는 큰따옴표로: 나무 간판에 \"행복한 하루\"라는 한글 글씨가 크게 쓰여 있다. 문구는 짧을수록 정확합니다."),
    ("표정·동작이 안 나올 때", "시드를 바꿔 여러 장 뽑거나, 결과를 편집 모드에 넣고 \"고양이가 눈을 감게 해줘\"처럼 지시하세요."),
    ("편집 지시", "바꿀 것과 유지할 것을 함께 쓰세요: \"배경을 해변으로 바꿔줘. 인물은 그대로 유지해.\""),
];

/// Expand prompt when the source has no scene description of its own. Positive words
/// only: tested negatives such as "액자나 테두리 없음" made the model draw frames.
use crate::engine::OUTPAINT_DEFAULT;

/// Modes in bar order: (mode, label, tooltip with its Ctrl+number shortcut).
const MODES: [(GenMode, (&str, &str), (&str, &str)); 5] = [
    (GenMode::Text, ("텍스트", "Text"), ("텍스트로 생성 (Ctrl+1)", "Generate from text (Ctrl+1)")),
    (GenMode::Variation, ("변형", "Variation"), ("입력 이미지 변형 (Ctrl+2)", "Vary the input image (Ctrl+2)")),
    (GenMode::Edit, ("편집", "Edit"), ("지시문으로 편집 (Ctrl+3)", "Edit with an instruction (Ctrl+3)")),
    (GenMode::Inpaint, ("부분수정", "Inpaint"), ("칠한 부분만 수정 (Ctrl+4)", "Repaint only the painted area (Ctrl+4)")),
    (GenMode::Outpaint, ("확장", "Expand"), ("바깥으로 넓히기 (Ctrl+5)", "Widen outward (Ctrl+5)")),
];

const OUTPAINT_EXAMPLES: [&str; 3] = [
    OUTPAINT_DEFAULT,
    "넓은 밝은 회색 스튜디오 배경, 부드러운 조명, 상품 촬영",
    "탁 트인 야외 풍경, 맑은 하늘, 같은 시간대의 햇살",
];

const INPAINT_EXAMPLES: [&str; 4] = [
    "초록 몬스테라 화분이 있는 밝은 카페 배경",
    "깔끔한 흰색 스튜디오 배경, 부드러운 그림자",
    "따뜻한 나무 테이블 위, 자연광",
    "벚꽃이 흐릿하게 보이는 봄날 야외 배경, 부드러운 햇살",
];

/// "🪄 자연스럽게": edit-mode instruction that re-lights a pasted-looking product.
/// Tested against mask repainting: with the product image as ref 2 this kept the design
/// while fixing edges, gloss and shading; mask repainting either changed the design
/// (Turbo's schedule starts at high noise) or left the pasted edges.
const HARMONIZE_WITH_REF: &str = "1번 이미지의 휴대폰 케이스가 실제 사진처럼 자연스럽게 보이도록 조명, 반사광, 그림자, \
     가장자리 음영을 장면에 맞춰줘. 케이스의 그림과 색상은 2번 이미지와 똑같이 유지하고, 인물, 손, 배경도 그대로 유지해.";
const HARMONIZE_ALONE: &str = "1번 이미지의 휴대폰 케이스가 실제 사진처럼 자연스럽게 보이도록 조명, 반사광, 그림자, \
     가장자리 음영을 장면에 맞춰줘. 케이스의 그림과 색상, 인물, 손, 배경은 그대로 유지해.";

const TEXT_TEMPLATE: &str = "간판에 \"여기에 문구\"라는 한글 글씨가 크게 쓰여 있다";

// English counterparts of the tables above (same order of ideas, written for English prompts).
const EXAMPLES_EN: [&str; 9] = [
    "A woman in her thirties in a studio holding a phone in her right hand next to her face, showing the back of the phone to the camera, white blouse, shoulder-length bob, light grey background, product photography lighting",
    "A hanok cafe at dusk, warm paper lanterns hanging from the eaves, light rain falling on a stone path, cinematic lighting, 35mm film photo",
    "A red fox curled up in a lavender field at golden hour, soft watercolor illustration, warm colors",
    "Product photo of a ceramic mug on a walnut desk, morning sunlight, shallow depth of field, clean background",
    "A tiny floating island city with waterfalls, isometric diorama, highly detailed, studio lighting",
    "Portrait of an old fisherman with a wrinkled face, dramatic Rembrandt lighting, black background",
    "A night-market food stall with a glowing yellow sign that clearly reads \"GOOD EATS\", rising steam, string lights",
    "Watercolor of cherry blossoms along a river on a spring day, soft brush strokes, white paper texture",
    "Close-up of a wooden sign at a cafe entrance. Sign text: \"HAPPY DAY\". Warm afternoon sunlight",
];

const VARIATION_EXAMPLES_EN: [&str; 4] = [
    "The same scene as a warm watercolor painting, soft brush strokes, white paper texture",
    "The same scene as a soft pastel 3D clay render, rounded shapes, studio lighting",
    "The same scene as a black-and-white film photo, coarse grain, high contrast",
    "The same scene like a child's crayon drawing, colorful, paper texture",
];

const EDIT_EXAMPLES_EN: [&str; 7] = [
    "Change the case of the phone in image 1 to exactly the case design in image 2. Keep the person, pose and background unchanged.",
    "Change the background to a sunny beach with palm trees. Keep the person and composition unchanged.",
    "Turn the whole picture into a layered paper cut-out illustration. Keep the composition unchanged.",
    "Change the sign text to \"SUNNY TODAY\". Keep everything else unchanged.",
    "Put a red scarf on the person. Keep the face and background unchanged.",
    "Change the time of day to a sunset evening. Orange sky, warm lighting.",
    "Change the background to a clean white studio. Keep the product unchanged.",
];

const PROMPT_TIPS_EN: [(&str, &str); 6] = [
    (
        "Model holding a product (2 steps)",
        "① First make the pose of holding a phone with text generation, ② then 🖼 Use as input → add the case image as image 2 → \"Change the case of the phone in image 1 to exactly the case design in image 2\". Compositing in one go tends to stick the case onto the clothes instead of the hand. Turn ⚡ Fast mode off for edits.",
    ),
    ("Be detailed", "Subject → details → background → lighting/mood → camera or art style works best."),
    ("Avoid single words", "\"cat\" is much worse than \"a chubby ginger cat under window sunlight, close-up with crisp fur, 35mm film photo\"."),
    ("Text in the image", "Name the object first, then put the words in double quotes: a wooden sign with the words \"HAPPY DAY\" written in large letters. Shorter text is more accurate."),
    ("Expression or pose won't come out", "Try several seeds, or put the result into Edit mode and say \"make the cat close its eyes\"."),
    ("Edit instructions", "Say what to change and what to keep: \"Change the background to a beach. Keep the person unchanged.\""),
];

const OUTPAINT_EXAMPLES_EN: [&str; 3] = [
    crate::engine::OUTPAINT_DEFAULT_EN,
    "A wide light grey studio background, soft lighting, product photography",
    "An open outdoor landscape, clear sky, sunlight at the same time of day",
];

const INPAINT_EXAMPLES_EN: [&str; 4] = [
    "A bright cafe background with a green monstera plant",
    "A clean white studio background, soft shadows",
    "On a warm wooden table, natural light",
    "A spring outdoor background with blurred cherry blossoms, soft sunlight",
];

const HARMONIZE_WITH_REF_EN: &str = "Make the phone case in image 1 look natural like a real photo by matching its lighting, \
     reflections, shadows and edge shading to the scene. Keep the case artwork and colors exactly as in image 2, \
     and keep the person, hands and background unchanged.";
const HARMONIZE_ALONE_EN: &str = "Make the phone case in image 1 look natural like a real photo by matching its lighting, \
     reflections, shadows and edge shading to the scene. Keep the case artwork and colors, the person, hands and \
     background unchanged.";

const TEXT_TEMPLATE_EN: &str = "a sign with the words \"YOUR TEXT\" written in large letters";

/// The Korean or English one of two tables, by the UI language.
fn lang<T: ?Sized>(ko: &'static T, en: &'static T) -> &'static T {
    if crate::i18n::en() { en } else { ko }
}

struct Toast {
    text: String,
    color: Color32,
    born: Instant,
}

struct Active {
    id: u64,
    mode: GenMode,
    seed: u64,
    prompt: String,
    started: Option<Instant>,
    step: u32,
    total: u32,
    preview: Option<TextureHandle>,
    /// (current, total) outpaint pass.
    pass: Option<(u32, u32)>,
    /// Latest engine note (next outpaint pass, redraw) shown in the progress bar.
    note: Option<String>,
}

/// In-progress drag of an outpaint frame handle (scale/position frozen at drag start).
struct OutpaintDrag {
    side: u8,
    /// Started anywhere on the preview (not on an edge handle).
    free: bool,
    img: Rect,
    s: f32,
    start: crate::config::Pads,
    origin: Pos2,
}

struct Queued {
    id: u64,
    seed: u64,
    mode: GenMode,
}

#[derive(Default)]
struct DlState {
    running: bool,
    current: Option<&'static str>,
    done: u64,
    total: u64,
    error: Option<String>,
    cancel: Option<Arc<AtomicBool>>,
    /// The current file is a zip being extracted.
    extracting: bool,
}

/// `t!` with a different glyph off Windows: a few symbols that Windows' fonts have are in
/// none of the fonts bundled for Linux (egui's emoji fonts), so Linux gets a look-alike
/// instead of a box. Arguments: Windows (ko, en), then other systems (ko, en).
macro_rules! os_t {
    ($ko:literal, $en:literal, $ko_other:literal, $en_other:literal $(,)?) => {
        if cfg!(windows) { t!($ko, $en) } else { t!($ko_other, $en_other) }
    };
}

/// Close / clear mark (see `os_t!`).
const GLYPH_CLOSE: &str = if cfg!(windows) { "✕" } else { "×" };

/// Off Windows the UI fonts (Nanum Gothic and egui's emoji fonts) have no circled
/// numbers, so the prompt tips' "①/②" steps become "1)/2)" there.
fn plain_glyphs(s: &str) -> std::borrow::Cow<'_, str> {
    if cfg!(windows) || !s.contains(['①', '②']) {
        return s.into();
    }
    s.replace('①', "1)").replace('②', "2)").into()
}

pub struct StudioApp {
    settings: Settings,
    params: GenParams,
    engine: Engine,
    engine_state: EngineState,
    logs: VecDeque<String>,

    history: History,
    thumbs: HashMap<u64, TextureHandle>,
    /// (entry id, is full-size, decoded image or None when it could not be read)
    img_tx: Sender<(u64, bool, Option<ColorImage>)>,
    img_rx: Receiver<(u64, bool, Option<ColorImage>)>,
    selected: Option<u64>,
    full: Option<(u64, TextureHandle)>,
    full_loading: Option<u64>,
    /// Selected image whose file could not be read (shown as an error, not retried).
    full_failed: Option<u64>,

    active: Option<Active>,
    queue: Vec<Queued>,
    next_job: u64,
    recent_secs: VecDeque<(GenMode, f32)>,

    inputs: Vec<InputImage>,
    input_tx: Sender<Loaded>,
    input_rx: Receiver<Loaded>,
    engine_vision: bool,

    /// Brush mask for partial edits (matches the first input image's aspect).
    mask: Option<crate::mask::MaskEditor>,
    /// Canvas shows the mask editor instead of the selected result.
    mask_editing: bool,
    op_drag: Option<OutpaintDrag>,
    /// Where the source image sits in the expand preview (self-test drag script uses it).
    preview_img: Option<Rect>,
    /// Self-test drag script progress (frame counter).
    drag_test_step: Option<u32>,
    lora_files: Vec<String>,
    /// PNG with an embedded generation record that was just dropped (ask what to do).
    pending_drop: Option<(PathBuf, crate::metadata::Restored)>,
    dl_is_lora: bool,
    /// The running download is the first-run setup batch (engine, models, extras).
    dl_setup: bool,
    /// The user stopped the setup download: don't restart it on its own this session.
    setup_stopped: bool,
    /// The setup screen was waiting for an engine the user provides (Linux: self-built
    /// sd-server): start it as soon as it shows up.
    awaiting_engine: bool,
    /// Files the setup batch couldn't fetch (it goes on with the rest).
    setup_errors: Vec<String>,
    /// Window size sanity check done (see `fix_tiny_window`).
    window_checked: bool,
    /// `ctx.input().time` of the last resize `fix_tiny_window` sent.
    window_resized_at: f64,
    /// Shortcut/tips window (F1 or the ? button).
    show_help: bool,
    /// Expand-mode prompt filled from the input image's record (may be replaced freely).
    auto_prompt: Option<String>,
    /// Photo descriptions from the captioner (request id, result).
    caption_tx: Sender<(u64, Result<String, String>)>,
    caption_rx: Receiver<(u64, Result<String, String>)>,
    caption_seq: u64,
    /// Request id of the captioner process running now, if any (one at a time: two would
    /// not fit in VRAM together).
    captioning: Option<u64>,
    /// Captioner availability (llama.cpp + model files), re-checked every few seconds
    /// instead of on every frame.
    caption_ok: std::cell::Cell<Option<(Instant, bool)>>,
    /// Thumbnails of the built-in phone-case sample (made on first show).
    sample_thumbs: Option<[egui::TextureHandle; 2]>,
    /// A description is wanted but waits for the image engine to go idle (they share VRAM).
    caption_wanted: bool,
    /// Generate was pressed during a description: start it when the description is done.
    generate_after_caption: bool,
    egui_ctx: Context,
    /// LoRA being downloaded; switched on when the download finishes.
    dl_lora_file: Option<&'static str>,
    /// Multiple-Angles LoRA camera picker: azimuth index (30° steps), elevation index, close-up.
    angle_az: usize,
    angle_el: usize,
    angle_close: bool,

    toasts: Vec<Toast>,
    show_settings: bool,
    settings_draft: Option<Settings>,
    show_logs: bool,
    viewer: bool,
    zoom: f32,
    pan: Vec2,
    gallery_filter: String,
    confirm_delete: Option<u64>,
    example_idx: usize,
    autotest: Option<crate::autotest::AutoTest>,

    dl: DlState,
    dl_tx: Sender<DlEvent>,
    dl_rx: Receiver<DlEvent>,
}

impl StudioApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        theme::install(&cc.egui_ctx);
        // Self-test (QIS_FRESH): start from defaults, as on a new PC, ignoring saved settings.
        let fresh = std::env::var_os("QIS_SHOT_DIR").is_some() && std::env::var_os("QIS_FRESH").is_some();
        let mut settings: Settings = cc
            .storage
            .filter(|_| !fresh)
            .and_then(|s| eframe::get_value(s, "settings"))
            .unwrap_or_default();
        // Prompts the guard refuses are never kept in the history, including old saved ones.
        settings.recent_prompts.retain(|p| !crate::safety::blocked(p));
        // If saved paths went stale (folder moved), fall back to the bundled defaults.
        if !settings.runtime_ok() {
            settings.runtime_dir = Settings::default().runtime_dir;
        }
        if !settings.missing_models().is_empty() && Settings::default().missing_models().is_empty() {
            let d = Settings::default();
            settings.diffusion_model = d.diffusion_model;
            settings.text_encoder = d.text_encoder;
            settings.vae = d.vae;
        }
        let params: GenParams = cc
            .storage
            .filter(|_| !fresh)
            .and_then(|s| eframe::get_value(s, "params"))
            .unwrap_or_default();

        if std::env::var_os("QIS_SHOT_DIR").is_some() {
            // Self-test only: pick the model variant from the environment.
            if let Ok(v) = std::env::var("QIS_VARIANT") {
                settings.variant = if v == "int8" { ModelVariant::Int8 } else { ModelVariant::Q4 };
            }
            // Self-test only: engine arguments (e.g. a placement that fits a small test VM).
            if let Ok(a) = std::env::var("QIS_EXTRA_ARGS") {
                settings.extra_args = a;
            }
        }
        if std::env::var_os("QIS_SHOT_DIR").is_some() {
            if let Ok(l) = std::env::var("QIS_LANG") {
                settings.english = l == "en";
            }
        }
        crate::i18n::set_english(settings.english);
        let engine = Engine::spawn(cc.egui_ctx.clone());
        let history = History::load(&settings.output_dir);
        let (img_tx, img_rx) = channel();
        let (dl_tx, dl_rx) = channel();
        let (input_tx, input_rx) = channel();
        let (caption_tx, caption_rx) = channel();
        let selected = history.entries.first().map(|e| e.id);

        let mut app = Self {
            settings,
            params,
            engine,
            engine_state: EngineState::Stopped,
            logs: VecDeque::new(),
            history,
            thumbs: HashMap::new(),
            img_tx,
            img_rx,
            selected,
            full: None,
            full_loading: None,
            full_failed: None,
            active: None,
            queue: Vec::new(),
            next_job: 1,
            recent_secs: VecDeque::new(),
            inputs: Vec::new(),
            input_tx,
            input_rx,
            engine_vision: false,
            mask: None,
            mask_editing: false,
            op_drag: None,
            preview_img: None,
            drag_test_step: (std::env::var_os("QIS_SHOT_DIR").is_some() && std::env::var_os("QIS_DRAG_TEST").is_some())
                .then_some(0),
            lora_files: Vec::new(),
            pending_drop: None,
            dl_is_lora: false,
            dl_setup: false,
            setup_stopped: false,
            awaiting_engine: false,
            setup_errors: Vec::new(),
            window_checked: false,
            window_resized_at: f64::NEG_INFINITY,
            show_help: false,
            auto_prompt: None,
            caption_tx,
            caption_rx,
            caption_seq: 0,
            captioning: None,
            caption_ok: std::cell::Cell::new(None),
            sample_thumbs: None,
            caption_wanted: false,
            generate_after_caption: false,
            egui_ctx: cc.egui_ctx.clone(),
            dl_lora_file: None,
            angle_az: 2,
            angle_el: 0,
            angle_close: false,
            toasts: Vec::new(),
            show_settings: false,
            settings_draft: None,
            show_logs: false,
            viewer: false,
            zoom: 1.0,
            pan: Vec2::ZERO,
            gallery_filter: String::new(),
            confirm_delete: None,
            autotest: crate::autotest::AutoTest::from_env(),
            example_idx: 0,
            dl: DlState::default(),
            dl_tx,
            dl_rx,
        };
        app.lora_files = crate::lora::scan(&app.settings.lora_dir());
        // Self-test only: open straight into the expand preview with this image.
        if let (Some(_), Ok(path)) = (std::env::var_os("QIS_SHOT_DIR"), std::env::var("QIS_PREVIEW_INPUT")) {
            if let Ok(img) = image::open(&path) {
                app.params.mode = GenMode::Outpaint;
                app.inputs = vec![inputs::make_input(&cc.egui_ctx, "preview".into(), img.to_rgba8())];
                app.params.outpaint = [0.3, 0.1, 0.2, 0.0];
                app.mask_editing = true;
            }
        }
        app.params.aspect = crate::config::normalize_aspect(app.params.aspect);
        // Self-test only: load a file through the normal input path in expand mode.
        if let (Some(_), Ok(path)) = (std::env::var_os("QIS_SHOT_DIR"), std::env::var("QIS_LOAD_INPUT")) {
            app.params.mode = GenMode::Outpaint;
            app.params.prompt.clear();
            app.inputs.clear();
            inputs::load_file(path.into(), app.input_tx.clone(), cc.egui_ctx.clone());
        }
        // Self-test only: show an in-progress job (gallery placeholder + progress bar).
        if std::env::var_os("QIS_SHOT_DIR").is_some() && std::env::var_os("QIS_FAKE_ACTIVE").is_some() {
            app.active = Some(Active {
                id: 9999,
                mode: GenMode::Text,
                seed: 1,
                prompt: String::new(),
                started: Some(Instant::now()),
                step: 3,
                total: 8,
                preview: None,
                pass: None,
                note: None,
            });
        }
        // Self-test only: open with the help window showing.
        if std::env::var_os("QIS_SHOT_DIR").is_some() && std::env::var_os("QIS_HELP").is_some() {
            app.show_help = true;
        }
        app.load_thumbnails(&cc.egui_ctx);
        // A UI-only self-test (drag script) needs no engine, so it can run beside the app.
        if app.drag_test_step.is_none() {
            app.start_engine_if_possible();
        }
        app
    }

    // ------------------------------------------------------------------ helpers

    fn toast(&mut self, text: impl Into<String>, color: Color32) {
        self.toasts.push(Toast { text: text.into(), color, born: Instant::now() });
    }

    fn start_engine_if_possible(&mut self) {
        if std::env::var_os("QIS_SHOT_DIR").is_some() && std::env::var_os("QIS_DL_MIRROR").is_some() {
            return; // download self-test: the mirror's model files are stand-ins
        }
        if self.settings.core_ready() {
            self.engine_vision = self.settings.vision_ok();
            self.engine.start(&self.settings);
            self.engine_state = EngineState::Loading { since: Instant::now() };
        }
    }

    fn load_thumbnails(&mut self, ctx: &Context) {
        let items: Vec<(u64, PathBuf)> = self
            .history
            .entries
            .iter()
            .filter(|e| !self.thumbs.contains_key(&e.id))
            .map(|e| (e.id, e.file.clone()))
            .collect();
        let tx = self.img_tx.clone();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            for (id, path) in items {
                // Cached JPEG thumbnail when available (decoded once and cached otherwise).
                if let Some(thumb) = crate::engine::load_thumb(&path) {
                    let _ = tx.send((id, false, Some(thumb)));
                    ctx.request_repaint();
                }
            }
        });
    }

    fn request_full(&mut self, ctx: &Context, id: u64) {
        if self.full.as_ref().is_some_and(|(fid, _)| *fid == id) || self.full_loading == Some(id) {
            return;
        }
        let Some(path) = self.history.get(id).map(|e| e.file.clone()) else { return };
        self.full_loading = Some(id);
        let tx = self.img_tx.clone();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            // Always answer, so a failed read doesn't leave the canvas spinning forever.
            let img = std::fs::read(&path).ok().and_then(|b| crate::engine::decode_png(&b).ok());
            let _ = tx.send((id, true, img));
            ctx.request_repaint();
        });
    }

    /// Restarts the engine so it loads the vision module. Refuses while work is in
    /// flight, because the restart would kill the running generation.
    fn restart_for_vision(&mut self) -> bool {
        if self.active.is_some() || !self.queue.is_empty() {
            self.toast(t!("비전 모듈을 불러오려면 엔진을 다시 시작해야 합니다. 진행 중인 생성이 끝난 뒤 다시 눌러 주세요.", "Loading the vision module needs an engine restart. Press again when the current generation is done."), AMBER);
            return false;
        }
        self.toast(t!("비전 모듈을 불러오기 위해 엔진을 다시 시작합니다", "Restarting the engine to load the vision module"), ACCENT_HOVER);
        self.start_engine_if_possible();
        true
    }

    /// Prompt for expanding an image. With the vision module the source is shown to the
    /// model, so a direction instruction suffices; otherwise the prompt has to describe
    /// the scene (the source's own prompt when it was a scene description).
    fn outpaint_prompt(&self, source: Option<&HistoryEntry>) -> String {
        // The image's own scene description continued the scene best in tests;
        // edit/inpaint prompts and old expand instructions aren't scene descriptions.
        match source {
            Some(e)
                if matches!(e.mode, GenMode::Text | GenMode::Variation | GenMode::Outpaint)
                    && !e.prompt.starts_with("1번 이미지")
                    && !e.prompt.contains("image 1") =>
            {
                e.prompt.clone()
            }
            _ => crate::engine::outpaint_default().to_owned(),
        }
    }

    fn engine_usable(&self) -> bool {
        matches!(self.engine_state, EngineState::Ready | EngineState::Loading { .. })
    }

    fn generate(&mut self) {
        // The photo description holds the GPU for ~20 s: start right after it (with the
        // description it is making as the scene).
        if self.captioning.is_some() {
            if !self.generate_after_caption {
                self.generate_after_caption = true;
                self.toast(t!("사진 설명이 끝나면 바로 생성합니다 (약 20초)", "Generating as soon as the photo description is done (about 20 s)"), ACCENT_HOVER);
            }
            return;
        }
        if self.params.mode == GenMode::Outpaint {
            if self.params.outpaint.iter().all(|p| *p <= 0.0) {
                self.toast(t!("확장할 방향을 하나 이상 정해 주세요 (왼쪽 패널의 빠른 선택)", "Pick at least one direction to expand (quick presets on the left panel)"), AMBER);
                return;
            }
            if self.params.prompt.trim().is_empty() {
                self.params.prompt = self.outpaint_prompt(None);
            }
            // The reference image needs the vision module loaded in the engine.
        }
        let prompt = self.params.prompt.trim().to_owned();
        if prompt.is_empty() {
            self.toast(t!("프롬프트를 입력해 주세요", "Please enter a prompt"), AMBER);
            return;
        }
        if !self.engine_usable() {
            self.toast(t!("엔진이 준비되지 않았습니다. 상단 상태를 확인하세요.", "The engine is not ready. Check the status at the top."), RED);
            return;
        }
        let mode = self.params.mode;
        // One prompt, or one per line in batch mode (hidden, so off, when expanding).
        let prompts: Vec<String> = if self.params.batch && mode != GenMode::Outpaint {
            prompt.lines().map(str::trim).filter(|l| !l.is_empty()).map(str::to_owned).collect()
        } else {
            vec![prompt.clone()]
        };
        if prompts.iter().any(|p| crate::safety::blocked(p)) {
            self.toast(
                if crate::i18n::en() {
                    "This prompt can't be generated: it conflicts with the content policy (protection of minors)."
                } else {
                    crate::safety::BLOCK_MESSAGE
                },
                RED,
            );
            return;
        }
        if mode != GenMode::Text && self.inputs.is_empty() {
            self.toast(t!("입력 이미지를 먼저 넣어 주세요 (끌어다 놓기 · 붙여넣기 · 갤러리)", "Add an input image first (drag and drop · paste · gallery)"), AMBER);
            return;
        }
        let mask = if mode == GenMode::Inpaint {
            match &self.mask {
                Some(m) if !m.is_empty() => Some(Arc::new(m.to_gray())),
                _ => {
                    self.toast(t!("바꿀 부분을 브러시로 먼저 칠해 주세요", "Paint the area to change with the brush first"), AMBER);
                    self.mask_editing = true;
                    return;
                }
            }
        } else {
            None
        };
        let mut loras: Vec<(String, f32)> = self
            .params
            .loras
            .iter()
            .filter(|l| l.enabled && self.lora_files.contains(&l.file))
            .map(|l| (l.file.clone(), l.weight))
            .collect();
        // Few-step schedules start at sigma 1.0 and sd.cpp keeps ceil(steps × strength) of
        // them, so they would re-noise the whole input. Skip them where the input must survive.
        if matches!(mode, GenMode::Variation | GenMode::Inpaint) && loras.iter().any(|(f, _)| crate::lora::is_speed(f)) {
            loras.retain(|(f, _)| !crate::lora::is_speed(f));
            self.toast(t!("변형·부분 수정에서는 고속 LoRA를 쓰지 않고 일반 8단계로 생성합니다", "Speed LoRAs are not used for variation/inpaint; generating with the normal 8 steps"), TEXT_DIM);
        }
        // Outpainting shows the padded canvas to the model as an edit reference when the
        // vision module is there (one pass, far better continuity); otherwise the old
        // multi-pass inpaint.
        let guided = mode == GenMode::Outpaint && self.settings.vision_ok();
        if mode == GenMode::Outpaint && !guided {
            self.toast(t!("비전 모듈이 없어 기본 방식으로 확장합니다. 받으면 훨씬 자연스럽게 확장됩니다.", "No vision module: expanding the basic way. Download it for much more natural expansion."), AMBER);
        }
        // Edit-pair LoRAs only make sense with an edit instruction and reference image.
        if mode != GenMode::Edit {
            loras.retain(|(f, _)| !crate::lora::is_edit_only(f));
        }
        // Expanding runs without LoRAs: none were tested for it, and the speed ones left
        // borders blank or drew the subject twice.
        if mode == GenMode::Outpaint {
            loras.clear();
        }
        let angles = loras.iter().any(|(f, _)| crate::lora::kind(f) == crate::lora::Kind::Angles);
        let prompts: Vec<String> = if angles && !prompts.iter().all(|p| p.trim_start().starts_with("<mva>")) {
            self.params.prompt = crate::lora::angles_prompt(self.angle_az, self.angle_el, self.angle_close);
            self.params.batch = false;
            vec![self.params.prompt.clone()]
        } else {
            prompts
        };
        let (w, h) = self.out_dims();
        let sigmas = loras.iter().find_map(|(f, _)| crate::lora::speed_sigmas(f, w, h));
        if (mode == GenMode::Edit || guided) && !self.engine_vision {
            if self.settings.vision_ok() {
                if !self.restart_for_vision() {
                    return;
                }
            } else {
                self.toast(t!("편집 모드에는 비전 모듈이 필요합니다. 입력 이미지 카드에서 받으세요.", "Edit mode needs the vision module. Download it from the input image card."), AMBER);
                return;
            }
        }
        for p in prompts.iter().rev() {
            self.settings.remember_prompt(p);
        }
        let inputs: Vec<Arc<image::RgbaImage>> = match mode {
            GenMode::Text => Vec::new(),
            GenMode::Variation | GenMode::Inpaint | GenMode::Outpaint => {
                self.inputs.iter().take(1).map(|i| i.rgba.clone()).collect()
            }
            GenMode::Edit => self.inputs.iter().map(|i| i.rgba.clone()).collect(),
        };
        let strength = if mode == GenMode::Inpaint { self.params.inpaint_strength } else { self.params.strength };
        for prompt in &prompts {
        for i in 0..self.params.count.max(1) {
            let seed = if self.params.random_seed {
                rand::random::<u32>() as u64
            } else {
                self.params.seed + i as u64
            };
            let id = self.next_job;
            self.next_job += 1;
            self.engine.enqueue(GenJob {
                id,
                prompt: prompt.clone(),
                negative_prompt: self.params.negative_prompt.trim().to_owned(),
                width: w,
                height: h,
                seed,
                // Expanding always runs the tested setup: Turbo 8 steps at CFG 1.
                cfg: if mode == GenMode::Outpaint { 1.0 } else { self.params.cfg },
                step_mode: if mode == GenMode::Outpaint { StepMode::Turbo } else { self.params.step_mode },
                steps: if mode == GenMode::Outpaint { 8 } else { self.params.steps },
                live_preview: self.params.live_preview,
                // The step cache guesses steps; outpainting needs every step to stay seamless.
                fast: self.params.fast_mode && mode != GenMode::Outpaint,
                output_dir: self.settings.output_dir.clone(),
                mode,
                guided,
                strength,
                inputs: inputs.clone(),
                mask: mask.clone(),
                loras: loras.clone(),
                sigmas: sigmas.clone(),
                outpaint: self.params.outpaint,
            });
            self.queue.push(Queued { id, seed, mode });
        }
        }
        if self.params.random_seed {
            // show the seed that was used last, so "고정" reproduces it
            if let Some(q) = self.queue.last() {
                self.params.seed = q.seed;
            }
        }
    }

    fn cancel_all(&mut self) {
        for q in &self.queue {
            self.engine.cancel(q.id);
        }
        if let Some(a) = &self.active {
            self.engine.cancel(a.id);
        }
    }

    fn avg_secs_for(&self, mode: GenMode) -> Option<f32> {
        let v: Vec<f32> = self.recent_secs.iter().filter(|(m, _)| *m == mode).map(|(_, s)| *s).collect();
        (!v.is_empty()).then(|| v.iter().sum::<f32>() / v.len() as f32)
    }

    fn avg_secs(&self) -> Option<f32> {
        self.avg_secs_for(self.params.mode)
    }

    /// Switches between Q4 and INT8 weights and restarts the engine.
    fn switch_variant(&mut self, ctx: &Context, v: ModelVariant) {
        if v == self.settings.variant {
            return;
        }
        if self.active.is_some() || !self.queue.is_empty() {
            self.toast(t!("생성이 끝난 뒤에 모델을 바꿀 수 있습니다", "You can switch models after the current generation finishes"), AMBER);
            return;
        }
        let mut next = self.settings.clone();
        next.variant = v;
        if !next.missing_models().is_empty() {
            // File not downloaded yet: open settings with this choice so the download button shows.
            self.settings_draft = Some(next);
            self.show_settings = true;
            self.toast(t!("모델 파일이 없습니다. 설정 창에서 받을 수 있습니다.", "Model file missing. You can download it in Settings."), AMBER);
            return;
        }
        self.settings = next;
        self.engine.stop();
        self.engine_state = EngineState::Stopped;
        self.start_engine_if_possible();
        self.toast(tf!("{} 모델로 바꾸는 중… (약 40초)", "Switching to the {} model… (about 40 s)", self.settings.variant_label()), ACCENT_HOVER);
        ctx.request_repaint();
    }

    /// Enables exactly the given LoRAs (by file name) with their weights.
    fn set_loras(&mut self, wanted: &[(String, f32)]) {
        for l in &mut self.params.loras {
            l.enabled = false;
        }
        for (file, w) in wanted {
            match self.params.loras.iter_mut().find(|l| &l.file == file) {
                Some(l) => {
                    l.enabled = true;
                    l.weight = *w;
                }
                None => self.params.loras.push(crate::lora::LoraSel { file: file.clone(), weight: *w, enabled: true }),
            }
        }
    }

    /// Turns on one LoRA (full strength) and switches off the ones it conflicts with.
    fn enable_lora(&mut self, file: &str) {
        for l in &mut self.params.loras {
            if crate::lora::conflicts(&l.file, file) {
                l.enabled = false;
            }
        }
        match self.params.loras.iter_mut().find(|l| l.file == file) {
            Some(l) => l.enabled = true,
            None => self.params.loras.push(crate::lora::LoraSel { file: file.to_owned(), weight: 1.0, enabled: true }),
        }
    }

    /// Applies settings read from a dropped PNG's embedded generation record.
    fn apply_restored(&mut self, r: &crate::metadata::Restored) {
        self.params.prompt = r.prompt.clone();
        self.params.negative_prompt = r.negative_prompt.clone();
        self.params.seed = r.seed;
        self.params.random_seed = false;
        self.params.cfg = r.cfg;
        self.params.count = 1;
        self.params.batch = false;
        self.params.fast_mode = r.fast;
        let turbo = r.sigmas.len() == crate::config::TURBO_SIGMAS.len();
        let speed = r.loras.iter().any(|(f, _)| crate::lora::is_speed(f));
        if turbo || speed || r.sigmas.is_empty() && r.steps == 8 {
            self.params.step_mode = StepMode::Turbo;
        } else {
            self.params.step_mode = StepMode::Custom;
            self.params.steps = r.steps.clamp(4, 40);
        }
        self.set_loras(&r.loras);
        let missing: Vec<&String> = r.loras.iter().map(|(f, _)| f).filter(|f| !self.lora_files.contains(f)).collect();
        if let Some(s) = r.strength {
            // Made from an input image; the image itself is not stored in the PNG.
            if self.params.mode == GenMode::Inpaint {
                self.params.inpaint_strength = s;
            } else {
                self.params.mode = GenMode::Variation;
                self.params.strength = s;
            }
        } else if self.params.mode != GenMode::Edit {
            self.params.mode = GenMode::Text;
        }
        // Size: preset if it matches, otherwise custom.
        self.params.custom_size = true;
        self.params.width = r.width;
        self.params.height = r.height;
        'outer: for a in 0..ASPECTS.len() {
            for s in 0..SIZES.len() {
                let p = GenParams { aspect: a, size: s, custom_size: false, ..GenParams::default() };
                if p.dimensions() == (r.width, r.height) {
                    self.params.aspect = a;
                    self.params.size = s;
                    self.params.custom_size = false;
                    break 'outer;
                }
            }
        }
        if missing.is_empty() {
            self.toast(t!("이미지의 생성 설정을 불러왔습니다 (시드 고정)", "Loaded the image\'s generation settings (seed fixed)"), GREEN);
        } else {
            self.toast(tf!("설정을 불러왔습니다. LoRA 파일이 없습니다: {missing:?}", "Settings loaded. Missing LoRA files: {missing:?}"), AMBER);
        }
    }

    fn out_dims(&self) -> (u32, u32) {
        if self.params.mode == GenMode::Outpaint {
            if let Some(i) = self.inputs.first() {
                // Output takes the expanded canvas's shape at the chosen size preset.
                let (w, h) = i.size();
                let canvas = crate::config::padded_size(w, h, self.params.outpaint);
                let p = GenParams { custom_size: false, follow_input_aspect: true, ..self.params.clone() };
                return p.dimensions_for(Some(canvas));
            }
        }
        self.params.dimensions_for(self.inputs.first().map(|i| i.size()))
    }

    /// Mode to switch to when the user supplies an image while in text mode.
    fn default_image_mode(&self) -> GenMode {
        if self.settings.vision_ok() { GenMode::Edit } else { GenMode::Variation }
    }

    fn add_input(&mut self, ctx: &Context, name: String, rgba: image::RgbaImage) {
        if self.params.mode == GenMode::Text {
            self.params.mode = self.default_image_mode();
        }
        let input = inputs::make_input(ctx, name, rgba);
        if self.params.mode == GenMode::Outpaint {
            self.mask_editing = true; // show the expansion preview
        }
        if matches!(self.params.mode, GenMode::Variation | GenMode::Inpaint | GenMode::Outpaint) {
            self.inputs.clear();
        }
        if self.params.mode == GenMode::Inpaint {
            let (w, h) = input.size();
            self.mask = Some(crate::mask::MaskEditor::new(w, h));
            self.mask_editing = true;
        }
        if self.inputs.len() >= MAX_INPUTS {
            self.inputs.pop();
        }
        self.inputs.push(input);
    }

    fn pick_input_files(&mut self, ctx: &Context) {
        if let Some(files) = rfd::FileDialog::new()
            .add_filter(t!("이미지", "Images"), &["png", "jpg", "jpeg", "webp", "bmp"])
            .pick_files()
        {
            for f in files.into_iter().take(MAX_INPUTS) {
                inputs::load_file(f, self.input_tx.clone(), ctx.clone());
            }
        }
    }

    fn paste_input(&mut self, ctx: &Context) {
        match inputs::from_clipboard() {
            Ok((name, rgba, prompt)) => {
                self.add_loaded_input(ctx, name, rgba, prompt);
                self.toast(t!("클립보드 이미지를 입력으로 넣었습니다", "Added the clipboard image as input"), GREEN);
            }
            Err(e) => self.toast(e, AMBER),
        }
    }

    /// Adds an image from a file or the clipboard, keeping the prompt recorded in it, and
    /// refreshes the automatic expand prompt.
    fn add_loaded_input(&mut self, ctx: &Context, name: String, rgba: image::RgbaImage, record: Option<String>) {
        self.add_input(ctx, name, rgba);
        // add_input appends (dropping the last one when full), so the new image is last.
        if let Some(i) = self.inputs.last_mut() {
            i.record = record;
        }
        self.auto_outpaint_prompt(false);
    }

    /// In expand mode, fills the prompt from the (first) input image: the scene description
    /// recorded in images made by this app, or a GPU description of a photo. A prompt the
    /// user typed is never replaced; one filled automatically is swapped or cleared when
    /// another image comes in. `entering` = just switched into expand mode, where a prompt
    /// that isn't a scene (an edit instruction left over from another mode) is replaced too.
    fn auto_outpaint_prompt(&mut self, entering: bool) {
        if self.params.mode != GenMode::Outpaint || self.inputs.is_empty() {
            return;
        }
        let leftover = entering && scene_from_record(self.params.prompt.trim()).is_none();
        if !self.prompt_is_ours() && !leftover {
            return;
        }
        let recorded = self.inputs.first().and_then(|i| i.record.clone());
        match recorded.as_deref().and_then(scene_from_record) {
            Some(scene) => {
                self.params.prompt = scene.clone();
                self.auto_prompt = Some(scene);
                self.toast(t!("이미지에 저장된 장면 설명으로 프롬프트를 채웠습니다", "Filled the prompt with the scene description saved in the image"), ACCENT_HOVER);
            }
            None => {
                // No usable record (a photo, or an edit instruction): leave it empty — the
                // engine then adds a generic scene description — and describe the photo
                // in the background to fill it in.
                self.params.prompt.clear();
                self.auto_prompt = None;
                self.start_caption();
            }
        }
    }

    fn captioner(&self) -> Option<crate::caption::Captioner> {
        if !self.settings.vision_ok() {
            return None;
        }
        crate::caption::Captioner::find(&self.settings.runtime_dir, &self.settings.text_encoder, &self.settings.vision)
    }

    /// Cached `captioner().is_some()` for the UI (file checks at most every 3 s).
    fn caption_ready(&self) -> bool {
        match self.caption_ok.get() {
            Some((at, ok)) if at.elapsed() < std::time::Duration::from_secs(3) => ok,
            _ => {
                let ok = self.captioner().is_some();
                self.caption_ok.set(Some((Instant::now(), ok)));
                ok
            }
        }
    }

    /// No generation running or queued and no model loading: the GPU is free.
    fn engine_idle(&self) -> bool {
        self.active.is_none() && self.queue.is_empty() && !matches!(self.engine_state, EngineState::Loading { .. })
    }

    /// Starts describing the first input image. It runs on the GPU, so it waits while the
    /// image engine is busy and while another description runs (two at once would not fit
    /// in VRAM); the newest request is the one whose result fills the prompt.
    fn start_caption(&mut self) {
        if !self.caption_ready() || self.inputs.is_empty() {
            return;
        }
        if self.captioning.is_some() || !self.engine_idle() {
            self.caption_wanted = true;
            return;
        }
        let (Some(c), Some(input)) = (self.captioner(), self.inputs.first()) else { return };
        self.caption_wanted = false;
        self.caption_seq += 1;
        self.captioning = Some(self.caption_seq);
        c.spawn(self.caption_seq, input.rgba.clone(), self.caption_tx.clone(), self.egui_ctx.clone());
    }

    /// After the 한/EN switch: swap texts the app filled in itself to the new language.
    fn on_language_changed(&mut self) {
        if crate::engine::is_outpaint_default(&self.params.prompt) {
            self.params.prompt = crate::engine::outpaint_default().to_owned();
        }
    }

    /// True when the prompt may be replaced by an automatic one: empty, the generic
    /// default, or filled automatically earlier (and not edited since).
    fn prompt_is_ours(&self) -> bool {
        let current = self.params.prompt.trim();
        current.is_empty() || crate::engine::is_outpaint_default(current) || self.auto_prompt.as_deref() == Some(current)
    }

    /// "🪄 자연스럽게": re-lights a pasted-looking product in the selected image.
    /// Ref 1 = the selected result; ref 2 = the product image from the previous edit
    /// (kept so the design is copied faithfully), then generates right away.
    /// Built-in sample: ref 1 = model holding a phone, ref 2 = a case design, with the
    /// tested swap instruction, in Edit mode. The user presses Generate.
    fn load_sample(&mut self, ctx: &Context) {
        let (Some(model), Some(case)) = (decode_sample(SAMPLE_MODEL), decode_sample(SAMPLE_CASE)) else { return };
        self.set_mode(GenMode::Edit);
        self.params.batch = false;
        self.params.count = 1;
        self.params.fast_mode = false;
        for l in &mut self.params.loras {
            if crate::lora::is_speed(&l.file) || crate::lora::kind(&l.file) == crate::lora::Kind::Angles {
                l.enabled = false;
            }
        }
        self.inputs = vec![
            inputs::make_input(ctx, t!("샘플 모델", "Sample model").to_owned(), model),
            inputs::make_input(ctx, t!("샘플 케이스", "Sample case").to_owned(), case),
        ];
        self.params.prompt = lang(&EDIT_EXAMPLES[..], &EDIT_EXAMPLES_EN[..])[0].to_owned();
        self.toast(t!("샘플을 불러왔습니다. 이미지 생성을 누르면 케이스가 바뀝니다.", "Sample loaded. Press Generate to swap the case."), ACCENT_HOVER);
    }

    fn start_harmonize(&mut self, ctx: &Context, path: &Path) {
        let img = match image::open(path) {
            Ok(i) => i.to_rgba8(),
            Err(e) => {
                self.toast(tf!("이미지를 열 수 없습니다: {e}", "Can\'t open the image: {e}"), RED);
                return;
            }
        };
        // Reuse the product reference (2nd image) of the edit that made this picture.
        let product = (self.params.mode == GenMode::Edit && self.inputs.len() >= 2).then(|| self.inputs.remove(1));
        self.params.mode = GenMode::Edit;
        self.params.batch = false;
        self.params.count = 1;
        // Untested for this edit: run it with the full schedule.
        self.params.fast_mode = false;
        for l in &mut self.params.loras {
            if crate::lora::is_speed(&l.file) || crate::lora::kind(&l.file) == crate::lora::Kind::Angles {
                l.enabled = false;
            }
        }
        self.inputs.clear();
        let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        self.inputs.push(inputs::make_input(ctx, name, img));
        self.params.prompt = match product {
            Some(p) => {
                self.inputs.push(p);
                lang(HARMONIZE_WITH_REF, HARMONIZE_WITH_REF_EN).to_owned()
            }
            None => lang(HARMONIZE_ALONE, HARMONIZE_ALONE_EN).to_owned(),
        };
        self.toast(t!("케이스 디자인은 유지하고 조명·반사·음영을 자연스럽게 다듬습니다 (약 75초)", "Keeping the case design and refining light, reflections and shading (about 75 s)"), ACCENT_HOVER);
        self.generate();
    }

    /// Preview of the expanded canvas: the original in place, new area hatched.
    fn outpaint_preview(&mut self, ui: &mut Ui) {
        use crate::config::{SIDE_B as B, SIDE_L as L, SIDE_R as R, SIDE_T as T};
        let area = ui.available_rect_before_wrap().shrink(18.0);
        let top = Rect::from_min_size(area.min, vec2(area.width(), 34.0));
        let (ow, oh) = self.out_dims();
        ui.scope_builder(egui::UiBuilder::new().max_rect(top), |ui| {
            ui.horizontal_centered(|ui| {
                ui.label(bold(t!("↔ 확장 미리보기", "↔ Expand preview"), 14.0));
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if self.selected.is_some() && tool_button(ui, t!("결과 보기", "Show result"), t!("갤러리에서 선택한 이미지 보기", "Show the image selected in the gallery")).clicked() {
                        self.mask_editing = false;
                    }
                    if tool_button(ui, t!("↺ 초기화", "↺ Reset"), t!("확장 범위를 모두 0으로", "Set all expansion to 0")).clicked() {
                        self.params.outpaint = [0.0; 4];
                    }
                    // The hint takes whatever room is left and is cut with "…" instead of
                    // running under the buttons.
                    ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
                        ui.add(
                            egui::Label::new(
                                RichText::new(t!("가장자리를 잡고 바깥으로 끌기 · 모서리는 두 방향 · ◀▶ 클릭 +10% · 빗금 = 새로 그릴 곳", "Drag an edge outward · corners pull two sides · ◀▶ click +10% · hatched = new area"))
                                    .size(12.0)
                                    .color(TEXT_DIM),
                            )
                            .truncate(),
                        )
                        .on_hover_text(tf!("생성 크기 {ow}×{oh} · 화살표 우클릭은 -10%", "Output size {ow}×{oh} · right-click an arrow for -10%"));
                    });
                });
            });
        });
        ui.allocate_rect(area, Sense::hover());
        let Some(input) = self.inputs.first() else { return };
        let tex = input.tex.clone();
        let (iw, ih) = input.size();
        let (iwf, ihf) = (iw as f32, ih as f32);
        let body = Rect::from_min_max(area.min + vec2(0.0, 44.0), area.max);
        let p = self.params.outpaint;

        // While dragging, keep the scale and image position from the drag start so the
        // handle stays under the cursor; otherwise fit the whole canvas into view.
        let (img, s) = match &self.op_drag {
            Some(d) => (d.img, d.s),
            None => {
                let (cw, ch) = crate::config::padded_size(iw, ih, p);
                let frame = fit(body.shrink(34.0), vec2(cw as f32, ch as f32));
                let s = frame.width() / cw as f32;
                let img = Rect::from_min_size(frame.min + vec2(iwf * p[0] * s, ihf * p[2] * s), vec2(iwf * s, ihf * s));
                (img, s)
            }
        };
        let frame = Rect::from_min_max(
            img.min - vec2(iwf * p[0] * s, ihf * p[2] * s),
            img.max + vec2(iwf * p[1] * s, ihf * p[3] * s),
        );
        self.preview_img = Some(img);

        // Canvas: hatched new area, original image, outlines (clipped to the frame).
        let painter = ui.painter_at(body);
        painter.rect_filled(frame, 8, ACCENT_SOFT.gamma_multiply(0.5));
        let hatch = ui.painter_at(frame.intersect(body));
        let step = 14.0;
        let mut x = frame.left() - frame.height();
        while x < frame.right() {
            hatch.line_segment(
                [Pos2::new(x, frame.bottom()), Pos2::new(x + frame.height(), frame.top())],
                Stroke::new(1.0, ACCENT.gamma_multiply(0.35)),
            );
            x += step;
        }
        egui::Image::new(&tex).paint_at(ui, img);
        painter.rect_stroke(img, 0, Stroke::new(1.0, Color32::from_white_alpha(140)), egui::StrokeKind::Outside);
        painter.rect_stroke(frame, 8, Stroke::new(2.0, ACCENT), egui::StrokeKind::Inside);

        // Percent labels on the sides that grow.
        for (i, name) in [t!("왼쪽", "Left"), t!("오른쪽", "Right"), t!("위", "Top"), t!("아래", "Bottom")].iter().enumerate() {
            if p[i] <= 0.0 {
                continue;
            }
            // Skip the label when the strip is too thin to hold it (the slider shows the value).
            let room = match i {
                0 => img.left() - frame.left(),
                1 => frame.right() - img.right(),
                2 => img.top() - frame.top(),
                _ => frame.bottom() - img.bottom(),
            };
            // Full label when it fits, just the number in a narrow strip, nothing if tiny.
            let full = room >= if i < 2 { 84.0 } else { 26.0 };
            if !full && room < if i < 2 { 40.0 } else { 18.0 } {
                continue;
            }
            let name = if full { *name } else { "" };
            let pos = match i {
                0 => Pos2::new((frame.left() + img.left()) / 2.0, img.center().y),
                1 => Pos2::new((frame.right() + img.right()) / 2.0, img.center().y),
                2 => Pos2::new(img.center().x, (frame.top() + img.top()) / 2.0),
                _ => Pos2::new(img.center().x, (frame.bottom() + img.bottom()) / 2.0),
            };
            painter.text(pos, Align2::CENTER_CENTER, format!("{name} {:.0}%", p[i] * 100.0).trim(), FontId::proportional(13.0), Color32::WHITE);
        }

        // Grab anywhere: the press position picks the side (left part pulls left, a corner
        // pulls two sides); dragging outward grows, inward shrinks.
        let img_box = (img.left(), img.top(), img.width(), img.height());
        let free = ui.interact(body, Id::unique("outpaint_free"), Sense::drag());
        let active_side = match &self.op_drag {
            Some(d) => Some(d.side),
            None => free.hover_pos().map(|h| crate::config::side_at(h.x, h.y, img_box)),
        };
        if let Some(side) = active_side {
            let c = side_cursor(side);
            if free.hovered() || self.op_drag.as_ref().is_some_and(|d| d.free) {
                ui.ctx().set_cursor_icon(c);
            }
            // Highlight the edge(s) that will move.
            let hl = Stroke::new(4.0, ACCENT_HOVER);
            if side & L != 0 {
                ui.painter().line_segment([frame.left_top(), frame.left_bottom()], hl);
            }
            if side & R != 0 {
                ui.painter().line_segment([frame.right_top(), frame.right_bottom()], hl);
            }
            if side & T != 0 {
                ui.painter().line_segment([frame.left_top(), frame.right_top()], hl);
            }
            if side & B != 0 {
                ui.painter().line_segment([frame.left_bottom(), frame.right_bottom()], hl);
            }
        }
        if free.drag_started() {
            // Judge the side from where the press began, not where the drag threshold tripped.
            if let Some(origin) = ui.input(|i| i.pointer.press_origin()) {
                let side = crate::config::side_at(origin.x, origin.y, img_box);
                self.op_drag = Some(OutpaintDrag { side, free: true, img, s, start: p, origin });
            }
        }
        if free.dragged() {
            if let (Some(d), Some(now)) = (&self.op_drag, ui.input(|i| i.pointer.interact_pos())) {
                if d.free {
                    let delta = now - d.origin;
                    self.params.outpaint =
                        crate::config::drag_pads(d.start, d.side, delta.x, delta.y, iwf * d.s, ihf * d.s);
                }
            }
        }
        if free.drag_stopped() {
            self.op_drag = None;
        }

        // Drag handles: edges first, corners last so they win where they overlap.
        let e = 9.0;
        let corner = vec2(2.0 * e + 6.0, 2.0 * e + 6.0);
        let zones = [
            (L, Rect::from_min_max(Pos2::new(frame.left() - e, frame.top() + e), Pos2::new(frame.left() + e, frame.bottom() - e)), CursorIcon::ResizeHorizontal),
            (R, Rect::from_min_max(Pos2::new(frame.right() - e, frame.top() + e), Pos2::new(frame.right() + e, frame.bottom() - e)), CursorIcon::ResizeHorizontal),
            (T, Rect::from_min_max(Pos2::new(frame.left() + e, frame.top() - e), Pos2::new(frame.right() - e, frame.top() + e)), CursorIcon::ResizeVertical),
            (B, Rect::from_min_max(Pos2::new(frame.left() + e, frame.bottom() - e), Pos2::new(frame.right() - e, frame.bottom() + e)), CursorIcon::ResizeVertical),
            (L | T, Rect::from_center_size(frame.left_top(), corner), CursorIcon::ResizeNwSe),
            (R | B, Rect::from_center_size(frame.right_bottom(), corner), CursorIcon::ResizeNwSe),
            (R | T, Rect::from_center_size(frame.right_top(), corner), CursorIcon::ResizeNeSw),
            (L | B, Rect::from_center_size(frame.left_bottom(), corner), CursorIcon::ResizeNeSw),
        ];
        let pointer = ui.input(|i| i.pointer.interact_pos());
        for (side, rect, cursor) in zones {
            let r = ui.interact(rect, Id::unique(("outpaint_handle", side)), Sense::drag()).on_hover_cursor(cursor);
            if side.count_ones() == 2 {
                ui.painter().circle_filled(rect.center(), 5.0, Color32::WHITE);
                ui.painter().circle_stroke(rect.center(), 5.0, Stroke::new(1.5, ACCENT));
            }
            if r.drag_started() {
                if let Some(origin) = pointer {
                    self.op_drag = Some(OutpaintDrag { side, free: false, img, s, start: p, origin });
                }
            }
            if r.dragged() {
                ui.ctx().set_cursor_icon(cursor);
                if let (Some(d), Some(now)) = (&self.op_drag, pointer) {
                    if d.side == side && !d.free {
                        let delta = now - d.origin;
                        self.params.outpaint =
                            crate::config::drag_pads(d.start, side, delta.x, delta.y, iwf * d.s, ihf * d.s);
                    }
                }
            }
            if r.drag_stopped() {
                self.op_drag = None;
            }
        }

        // Arrow buttons just outside each side: click +10 %, right-click -10 %.
        let arrows = [
            (0usize, "◀", Pos2::new(frame.left() - 20.0, frame.center().y)),
            (1, "▶", Pos2::new(frame.right() + 20.0, frame.center().y)),
            (2, "▲", Pos2::new(frame.center().x, frame.top() - 20.0)),
            (3, "▼", Pos2::new(frame.center().x, frame.bottom() + 20.0)),
        ];
        for (i, glyph, c) in arrows {
            let rect = Rect::from_center_size(c, vec2(28.0, 28.0));
            let r = ui
                .interact(rect, Id::unique(("outpaint_arrow", i)), Sense::click())
                .on_hover_cursor(CursorIcon::PointingHand)
                .on_hover_text(t!("클릭: 이 방향으로 10% 늘리기 · 우클릭: 10% 줄이기", "Click: +10% this way · Right-click: -10%"));
            let fill = if r.hovered() { ACCENT_HOVER } else { ACCENT };
            ui.painter().circle_filled(c, 14.0, fill);
            ui.painter().text(c, Align2::CENTER_CENTER, glyph, FontId::proportional(12.0), Color32::WHITE);
            if r.clicked() {
                self.params.outpaint[i] = ((self.params.outpaint[i] + 0.1).min(1.0) * 100.0).round() / 100.0;
            }
            if r.secondary_clicked() {
                self.params.outpaint[i] = ((self.params.outpaint[i] - 0.1).max(0.0) * 100.0).round() / 100.0;
            }
        }
        if self.op_drag.is_some() && !ui.input(|i| i.pointer.any_down()) {
            self.op_drag = None; // released outside the handle
        }
    }

    /// One-click expand from the result toolbar: uses the image and its own prompt.
    fn quick_outpaint(&mut self, ctx: &Context, e: &HistoryEntry, aspect: Option<(f32, f32)>) {
        let img = match image::open(&e.file) {
            Ok(i) => i.to_rgba8(),
            Err(err) => {
                self.toast(tf!("이미지를 열 수 없습니다: {err}", "Can\'t open the image: {err}"), RED);
                return;
            }
        };
        let (w, h) = img.dimensions();
        self.params.outpaint = match aspect {
            Some((aw, ah)) => crate::config::pads_for_aspect(w, h, aw, ah),
            None => [0.25, 0.25, 0.25, 0.25],
        };
        self.params.mode = GenMode::Outpaint;
        self.params.batch = false;
        self.params.count = 1;
        // With the vision module the source is shown to the model, so a direction
        // instruction is enough. Without it the prompt must describe the scene.
        self.params.prompt = self.outpaint_prompt(Some(e));
        let name = e.file.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        self.inputs.clear();
        self.inputs.push(inputs::make_input(ctx, name, img));
        self.generate();
    }

    fn mask_canvas(&mut self, ui: &mut Ui) {
        let area = ui.available_rect_before_wrap().shrink(18.0);
        let top = Rect::from_min_size(area.min, vec2(area.width(), 34.0));
        ui.scope_builder(egui::UiBuilder::new().max_rect(top), |ui| {
            ui.horizontal_centered(|ui| {
                ui.label(bold(os_t!("🖌 바꿀 부분을 칠하세요", "🖌 Paint the area to change", "✏ 바꿀 부분을 칠하세요", "✏ Paint the area to change"), 14.0));
                ui.label(
                    RichText::new(t!("왼쪽 버튼: 칠하기 · 오른쪽 버튼: 지우기 · Ctrl+휠 또는 [ ]: 브러시 크기", "Left button: paint · Right button: erase · Ctrl+wheel or [ ]: brush size"))
                        .size(12.0)
                        .color(TEXT_DIM),
                );
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if self.selected.is_some() && tool_button(ui, t!("결과 보기", "Show result"), t!("갤러리에서 선택한 이미지 보기", "Show the image selected in the gallery")).clicked() {
                        self.mask_editing = false;
                    }
                });
            });
        });
        let Some(input) = self.inputs.first() else { return };
        let (iw, ih) = input.size();
        let tex = input.tex.clone();
        let body = Rect::from_min_max(area.min + vec2(0.0, 44.0), area.max);
        let r = fit(body, vec2(iw as f32, ih as f32));
        ui.allocate_rect(area, Sense::hover());
        ui.painter().rect_filled(r.expand(6.0), 18, Color32::from_black_alpha(60));
        egui::Image::new(&tex).corner_radius(6).paint_at(ui, r);
        let stale = self.mask.as_ref().is_none_or(|m| (m.w as f32 / m.h as f32 - iw as f32 / ih as f32).abs() > 0.01);
        if stale {
            self.mask = Some(crate::mask::MaskEditor::new(iw, ih));
        }
        if let Some(m) = self.mask.as_mut() {
            m.ui(ui, r);
        }
    }

    fn use_as_input(&mut self, ctx: &Context, path: &Path) {
        inputs::load_file(path.to_path_buf(), self.input_tx.clone(), ctx.clone());
        self.toast(t!("입력 이미지로 가져왔습니다", "Imported as input image"), ACCENT_HOVER);
    }

    fn apply_entry(&mut self, e: &HistoryEntry) {
        self.params.prompt = e.prompt.clone();
        self.params.negative_prompt = e.negative_prompt.clone();
        self.params.seed = e.seed;
        self.params.random_seed = false;
        self.params.cfg = e.cfg;
        self.params.step_mode = e.step_mode;
        self.params.steps = e.steps;
        self.params.count = 1;
        self.params.mode = e.mode;
        self.params.fast_mode = e.fast;
        self.set_loras(&e.loras);
        if e.mode == GenMode::Inpaint {
            self.params.inpaint_strength = e.strength;
        }
        if e.mode == GenMode::Variation {
            self.params.strength = e.strength;
        }
        if e.mode == GenMode::Outpaint {
            // An expand result's size comes from its expansion (and was enlarged after
            // generation): it is no size setting to restore.
            return;
        }
        let mut found = false;
        'outer: for a in 0..ASPECTS.len() {
            for s in 0..SIZES.len() {
                let p = GenParams { aspect: a, size: s, custom_size: false, ..GenParams::default() };
                if p.dimensions() == (e.width, e.height) {
                    self.params.aspect = a;
                    self.params.size = s;
                    self.params.custom_size = false;
                    found = true;
                    break 'outer;
                }
            }
        }
        if !found {
            self.params.custom_size = true;
            self.params.width = e.width;
            self.params.height = e.height;
        }
    }

    fn copy_image(&mut self, path: &Path) {
        let result = (|| -> anyhow::Result<()> {
            let img = image::open(path)?.to_rgba8();
            let mut cb = arboard::Clipboard::new()?;
            cb.set_image(arboard::ImageData {
                width: img.width() as usize,
                height: img.height() as usize,
                bytes: std::borrow::Cow::Owned(img.into_raw()),
            })?;
            Ok(())
        })();
        match result {
            Ok(()) => self.toast(t!("이미지를 클립보드에 복사했습니다", "Copied the image to the clipboard"), GREEN),
            Err(e) => self.toast(tf!("복사 실패: {e}", "Copy failed: {e}"), RED),
        }
    }

    fn save_as(&mut self, path: &Path) {
        let stem = path.file_stem().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        let Some(mut dest) = rfd::FileDialog::new()
            .set_file_name(format!("{stem}.png"))
            .add_filter(t!("PNG (원본 그대로)", "PNG (original)"), &["png"])
            .add_filter(t!("WebP (무손실, 용량 작음)", "WebP (lossless, smaller)"), &["webp"])
            .add_filter(t!("JPEG (품질 92, 블로그용)", "JPEG (quality 92, for blogs)"), &["jpg", "jpeg"])
            .save_file()
        else {
            return;
        };
        let ext = dest.extension().and_then(|e| e.to_str()).map(str::to_ascii_lowercase).unwrap_or_default();
        if !matches!(ext.as_str(), "png" | "webp" | "jpg" | "jpeg") {
            dest.set_extension("png");
        }
        match crate::inputs::export_image(path, &dest) {
            Ok(bytes) => self.toast(tf!("저장됨 ({:.1} MB): {}", "Saved ({:.1} MB): {}", bytes as f64 / 1e6, dest.display()), GREEN),
            Err(e) => self.toast(tf!("저장 실패: {e}", "Save failed: {e}"), RED),
        }
    }

    /// Shows the file in the file manager (Explorer selects it; elsewhere its folder opens).
    fn reveal(path: &Path) {
        #[cfg(windows)]
        let _ = std::process::Command::new("explorer").arg(format!("/select,{}", path.display())).spawn();
        #[cfg(not(windows))]
        if let Some(dir) = path.parent() {
            let _ = open::that_detached(dir);
        }
    }

    /// Moves the image to the Recycle Bin / Trash (restorable) and drops it from the gallery.
    fn delete_entry(&mut self, id: u64) {
        let Some(file) = self.history.get(id).map(|e| e.file.clone()) else { return };
        if file.exists() {
            if let Err(e) = trash::delete(&file) {
                self.toast(tf!("휴지통으로 옮기지 못했습니다: {e}", "Couldn\'t move to the Recycle Bin: {e}"), RED);
                return;
            }
        }
        // The cached thumbnail is derived data: delete it outright.
        if let Some(t) = crate::engine::thumb_cache_path(&file) {
            let _ = std::fs::remove_file(t);
        }
        // Keep the selection on a neighbour so arrow-key browsing continues smoothly.
        let idx = self.history.index_of(id).unwrap_or(0);
        self.history.remove(id);
        self.thumbs.remove(&id);
        if self.selected == Some(id) {
            let n = self.history.entries.len();
            self.selected = (n > 0).then(|| self.history.entries[idx.min(n - 1)].id);
        }
        if self.full.as_ref().is_some_and(|(f, _)| *f == id) {
            self.full = None;
        }
        let msg = if cfg!(windows) {
            t!("휴지통으로 이동했습니다 (Windows 휴지통에서 복원 가능)", "Moved to the Recycle Bin (restorable from Windows Recycle Bin)")
        } else {
            t!("휴지통으로 이동했습니다 (휴지통에서 복원 가능)", "Moved to the Trash (restorable from the Trash)")
        };
        self.toast(msg, TEXT_DIM);
    }

    /// First-run setup: fetches everything in `Settings::setup_items` that is missing, in
    /// order (engine, CUDA runtime, models, then the extras), going on past failures.
    fn start_setup(&mut self, ctx: &Context) {
        let tasks: Vec<DlTask> = self
            .settings
            .setup_items()
            .into_iter()
            .filter(|(s, _)| !self.settings.installed(s))
            .map(|(s, _)| DlTask { source: s, dest: self.settings.dest_for(s) })
            .collect();
        if tasks.is_empty() {
            return;
        }
        self.dl = DlState { running: true, ..Default::default() };
        self.dl_is_lora = false;
        self.dl_setup = true;
        self.setup_stopped = false;
        self.setup_errors.clear();
        self.dl.cancel = Some(download::spawn_with(tasks, self.dl_tx.clone(), ctx.clone(), true));
    }

    // ------------------------------------------------------------------ events

    fn pump(&mut self, ctx: &Context) {
        while let Ok(ev) = self.engine.events.try_recv() {
            match ev {
                EngineEvent::State(s) => {
                    match &s {
                        EngineState::Ready => self.toast(t!("엔진 준비 완료 — 이미지를 생성할 수 있습니다", "Engine ready — you can generate images"), GREEN),
                        EngineState::Error(e) => self.toast(tf!("엔진 오류: {e}", "Engine error: {e}"), RED),
                        _ => {}
                    }
                    self.engine_state = s;
                }
                EngineEvent::Log(line) => {
                    self.logs.push_back(line);
                    while self.logs.len() > 2000 {
                        self.logs.pop_front();
                    }
                }
                EngineEvent::JobStarted { id } => {
                    let (seed, mode) =
                        self.queue.iter().find(|q| q.id == id).map(|q| (q.seed, q.mode)).unwrap_or((0, GenMode::Text));
                    self.queue.retain(|q| q.id != id);
                    self.active = Some(Active {
                        id,
                        mode,
                        seed,
                        prompt: self.settings.recent_prompts.first().cloned().unwrap_or_default(),
                        started: Some(Instant::now()),
                        step: 0,
                        total: 0,
                        preview: None,
                        pass: None,
                        note: None,
                    });
                }
                EngineEvent::Progress { id, step, total, preview, pass } => {
                    if let Some(a) = self.active.as_mut().filter(|a| a.id == id) {
                        a.step = step;
                        a.total = total;
                        a.pass = pass;
                        if let Some(img) = preview {
                            match &mut a.preview {
                                Some(t) => t.set(img, TextureOptions::LINEAR),
                                None => a.preview = Some(ctx.load_texture("preview", img, TextureOptions::LINEAR)),
                            }
                        }
                    }
                }
                EngineEvent::Note { id, text } => {
                    if let Some(a) = self.active.as_mut().filter(|a| a.id == id) {
                        a.note = Some(text);
                    }
                }
                EngineEvent::JobDone { job, result, elapsed } => {
                    self.active = None;
                    self.recent_secs.push_back((job.mode, elapsed));
                    while self.recent_secs.len() > 12 {
                        self.recent_secs.pop_front();
                    }
                    let id = self.history.next_id();
                    // The saved file's own size: an expand result is enlarged after
                    // generation, so it is larger than the generation size.
                    let [fw, fh] = result.image.size;
                    self.history.entries.insert(
                        0,
                        HistoryEntry {
                            id,
                            file: result.path,
                            prompt: job.prompt,
                            negative_prompt: job.negative_prompt,
                            width: fw as u32,
                            height: fh as u32,
                            seed: job.seed,
                            cfg: job.cfg,
                            step_mode: job.step_mode,
                            steps: job.steps,
                            created: chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string(),
                            elapsed,
                            mode: job.mode,
                            strength: if matches!(job.mode, GenMode::Variation | GenMode::Inpaint) { job.strength } else { 0.0 },
                            fast: job.fast,
                            loras: job.loras.clone(),
                        },
                    );
                    self.history.save();
                    self.thumbs.insert(id, ctx.load_texture(format!("thumb{id}"), result.thumb, TextureOptions::LINEAR));
                    self.full = Some((id, ctx.load_texture("full", result.image, TextureOptions::LINEAR)));
                    self.selected = Some(id);
                    // Show the result; "🖌 마스크 편집" returns to the brush canvas.
                    self.mask_editing = false;
                    self.toast(tf!("생성 완료 · {elapsed:.1}초", "Done · {elapsed:.1} s"), GREEN);
                }
                EngineEvent::JobFailed { id, message } => {
                    self.queue.retain(|q| q.id != id);
                    if self.active.as_ref().is_some_and(|a| a.id == id) {
                        self.active = None;
                    }
                    self.toast(tf!("생성 실패: {message}", "Generation failed: {message}"), RED);
                }
                EngineEvent::JobCancelled { id } => {
                    self.queue.retain(|q| q.id != id);
                    if self.active.as_ref().is_some_and(|a| a.id == id) {
                        self.active = None;
                        self.toast(t!("생성을 취소했습니다", "Generation cancelled"), TEXT_DIM);
                    }
                }
            }
        }

        while let Ok((id, is_full, img)) = self.img_rx.try_recv() {
            if is_full {
                if self.full_loading == Some(id) {
                    self.full_loading = None;
                }
                match img {
                    Some(img) if self.selected == Some(id) => {
                        self.full = Some((id, ctx.load_texture("full", img, TextureOptions::LINEAR)));
                    }
                    Some(_) => {}
                    None => self.full_failed = Some(id),
                }
            } else if let Some(img) = img {
                self.thumbs.insert(id, ctx.load_texture(format!("thumb{id}"), img, TextureOptions::LINEAR));
            }
        }

        while let Ok((id, res)) = self.caption_rx.try_recv() {
            if self.captioning != Some(id) {
                continue;
            }
            self.captioning = None;
            if self.caption_wanted {
                // A newer image came in while this ran: its description starts next and
                // its result (not this one) fills the prompt.
                continue;
            }
            match res {
                Ok(text) if self.params.mode == GenMode::Outpaint && self.prompt_is_ours() => {
                    self.params.prompt = text.clone();
                    self.auto_prompt = Some(text);
                    self.toast(t!("사진을 보고 장면 설명을 채웠습니다", "Filled the scene description from the photo"), ACCENT_HOVER);
                }
                Ok(_) => {}
                Err(e) => self.toast(e, AMBER),
            }
            if std::mem::take(&mut self.generate_after_caption) {
                self.generate();
            }
        }
        if self.caption_wanted && self.captioning.is_none() && self.engine_idle() {
            self.start_caption();
        }

        while let Ok(res) = self.input_rx.try_recv() {
            match res {
                Ok((name, rgba, prompt)) => self.add_loaded_input(ctx, name, rgba, prompt),
                Err(e) => self.toast(tf!("이미지를 열 수 없습니다: {e}", "Can\'t open the image: {e}"), RED),
            }
        }

        while let Ok(ev) = self.dl_rx.try_recv() {
            match ev {
                DlEvent::Progress { file, done, total } => {
                    self.dl.current = Some(file);
                    self.dl.done = done;
                    self.dl.total = total;
                    self.dl.extracting = false;
                }
                DlEvent::Extracting { file } => {
                    self.dl.current = Some(file);
                    self.dl.extracting = true;
                }
                DlEvent::Finished { file } => {
                    self.dl.current = None;
                    self.dl.extracting = false;
                    if !self.dl_setup {
                        self.toast(tf!("다운로드 완료: {file}", "Download complete: {file}"), GREEN);
                    }
                }
                DlEvent::Failed { file, message } if self.dl_setup => {
                    // The setup batch goes on with the next file; a stop ends it here.
                    self.dl.current = None;
                    self.dl.extracting = false;
                    if self.dl.cancel.as_ref().is_some_and(|c| c.load(Ordering::Relaxed)) {
                        self.dl = DlState::default();
                        self.dl_setup = false;
                    } else {
                        self.setup_errors.push(format!("{file}: {message}"));
                    }
                }
                DlEvent::Failed { file, message } => {
                    self.dl.running = false;
                    self.dl.error = Some(format!("{file}: {message}"));
                    // No AllDone follows a failure: reset the kind so the next download
                    // (vision/INT8) isn't mistaken for a LoRA one.
                    self.dl_is_lora = false;
                    self.dl_lora_file = None;
                }
                DlEvent::AllDone if self.dl_setup => {
                    self.dl = DlState::default();
                    self.dl_setup = false;
                    self.lora_files = crate::lora::scan(&self.settings.lora_dir());
                    self.caption_ok.set(None);
                    if self.settings.core_ready() {
                        if !self.setup_errors.is_empty() {
                            self.toast(
                                t!("일부 추가 파일을 받지 못했습니다. 기본 생성은 쓸 수 있고, 해당 기능을 쓸 때 다시 받을 수 있습니다.", "Some extra files couldn't be downloaded. Generation works; you can fetch them again when you use those features."),
                                AMBER,
                            );
                            self.setup_errors.clear();
                        }
                        self.start_engine_if_possible();
                    }
                }
                DlEvent::AllDone => {
                    self.dl = DlState::default();
                    if std::mem::take(&mut self.dl_is_lora) {
                        // LoRAs are read per request; just rescan, no engine restart needed.
                        self.lora_files = crate::lora::scan(&self.settings.lora_dir());
                        self.engine.refresh();
                        if let Some(file) = self.dl_lora_file.take() {
                            self.enable_lora(file);
                        }
                    } else {
                        self.start_engine_if_possible();
                    }
                }
            }
        }

        if let Some(id) = self.selected.filter(|id| self.full_failed != Some(*id)) {
            if self.full.as_ref().is_none_or(|(f, _)| *f != id) {
                self.request_full(ctx, id);
            }
        }
    }

    fn handle_drops(&mut self, ctx: &Context, screen: Rect) {
        let dropped: Vec<PathBuf> = ctx.input(|i| i.raw.dropped_files.iter().map(|f| f.path().to_path_buf()).filter(|p| !p.as_os_str().is_empty()).collect());
        let mut n = 0;
        for p in dropped {
            if !inputs::is_image_path(&p) || n >= MAX_INPUTS {
                continue;
            }
            // A PNG made by this app carries its settings: ask whether to restore them.
            if n == 0 && self.pending_drop.is_none() {
                if let Some(r) = crate::metadata::read(&p) {
                    self.pending_drop = Some((p, r));
                    n += 1;
                    continue;
                }
            }
            inputs::load_file(p, self.input_tx.clone(), ctx.clone());
            n += 1;
        }
        if ctx.input(|i| !i.raw.hovered_files.is_empty()) {
            egui::Area::new(Id::unique("drop_overlay"))
                .order(Order::Foreground)
                .fixed_pos(screen.min)
                .interactable(false)
                .show(ctx, |ui| {
                    let r = screen.shrink(12.0);
                    ui.painter().rect_filled(screen, 0, Color32::from_black_alpha(170));
                    ui.painter().rect_stroke(r, 16, Stroke::new(2.0, ACCENT), egui::StrokeKind::Inside);
                    ui.painter().text(r.center() - vec2(0.0, 18.0), Align2::CENTER_CENTER, "🖼", FontId::proportional(40.0), Color32::WHITE);
                    ui.painter().text(
                        r.center() + vec2(0.0, 24.0),
                        Align2::CENTER_CENTER,
                        t!("놓으면 입력 이미지로 사용합니다", "Drop to use as the input image"),
                        FontId::new(18.0, egui::FontFamily::Name(BOLD.into())),
                        Color32::WHITE,
                    );
                });
        }
    }

    fn shortcuts(&mut self, ctx: &Context) {
        let gen_key = KeyboardShortcut::new(Modifiers::COMMAND, Key::Enter);
        let copy_key = KeyboardShortcut::new(Modifiers::COMMAND | Modifiers::SHIFT, Key::C);
        let save_key = KeyboardShortcut::new(Modifiers::COMMAND, Key::S);
        if ctx.input_mut(|i| i.consume_shortcut(&gen_key)) {
            self.generate();
        }
        if ctx.input(|i| i.key_pressed(Key::F1)) {
            self.show_help = !self.show_help;
        }
        // Ctrl+1…5: switch mode from anywhere (works while typing in the prompt too).
        for (i, key) in [Key::Num1, Key::Num2, Key::Num3, Key::Num4, Key::Num5].into_iter().enumerate() {
            if ctx.input_mut(|inp| inp.consume_shortcut(&KeyboardShortcut::new(Modifiers::COMMAND, key))) {
                self.set_mode(MODES[i].0);
            }
        }
        let path = self.selected.and_then(|id| self.history.get(id)).map(|e| e.file.clone());
        if ctx.input_mut(|i| i.consume_shortcut(&copy_key)) {
            if let Some(p) = &path {
                self.copy_image(p);
            }
        }
        if ctx.input_mut(|i| i.consume_shortcut(&save_key)) {
            if let Some(p) = &path {
                self.save_as(p);
            }
        }
        if ctx.input(|i| i.key_pressed(Key::Escape)) {
            if self.viewer {
                self.viewer = false;
            } else if self.show_settings {
                self.show_settings = false;
            }
        }
        let typing = ctx.memory(|m| m.focused().is_some());
        if !typing {
            let paste = ctx.input(|i| {
                i.events.iter().any(|e| matches!(e, egui::Event::Paste(_)))
                    || (i.modifiers.command && i.key_pressed(Key::V))
            });
            if paste {
                self.paste_input(ctx);
            }
        }
        if !typing && !self.history.entries.is_empty() {
            let (left, right, f) = ctx.input(|i| {
                (i.key_pressed(Key::ArrowLeft), i.key_pressed(Key::ArrowRight), i.key_pressed(Key::F))
            });
            let idx = self.selected.and_then(|id| self.history.index_of(id)).unwrap_or(0);
            let n = self.history.entries.len();
            // Newest is first; "left" moves to newer, "right" to older.
            if left && idx > 0 {
                self.selected = Some(self.history.entries[idx - 1].id);
            }
            if right && idx + 1 < n {
                self.selected = Some(self.history.entries[idx + 1].id);
            }
            if f && self.selected.is_some() {
                self.viewer = !self.viewer;
                self.zoom = 1.0;
                self.pan = Vec2::ZERO;
            }
            // Delete key: selected image → Recycle Bin (no dialog; it is restorable).
            if ctx.input(|i| i.key_pressed(Key::Delete)) && self.confirm_delete.is_none() {
                if let Some(id) = self.selected {
                    self.delete_entry(id);
                }
            }
        }
    }

    // ------------------------------------------------------------------ panels

    fn top_bar(&mut self, ui: &mut Ui) {
        ui.horizontal_centered(|ui| {
            ui.add_space(4.0);
            let (r, _) = ui.allocate_exact_size(vec2(26.0, 26.0), Sense::hover());
            ui.painter().rect_filled(r, 8, ACCENT);
            ui.painter().text(r.center(), Align2::CENTER_CENTER, "R", FontId::new(16.0, egui::FontFamily::Name(BOLD.into())), Color32::WHITE);
            ui.label(bold(crate::APP_NAME, 17.0).color(TEXT));
            ui.label(
                RichText::new(format!("Qwen-Image-2.1 Turbo · {}", self.settings.variant_label()))
                    .size(12.5)
                    .color(TEXT_FAINT),
            );

            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.add_space(4.0);
                // Right-to-left: the first button added sits at the far right.
                bar_group(ui, |ui| {
                    if bar_button(ui, Icon::Gear, Some(t!("설정", "Settings")), t!("모델 경로 · 메모리 옵션", "Model paths · memory options"), BarKind::Normal).clicked() {
                        self.settings_draft = Some(self.settings.clone());
                        self.show_settings = true;
                    }
                    if bar_button(ui, Icon::Log, None, t!("엔진 로그 보기", "Show engine log"), BarKind::Normal).clicked() {
                        self.show_logs = !self.show_logs;
                    }
                    if bar_button(ui, Icon::Folder, None, &tf!("출력 폴더 열기\n{}", "Open output folder\n{}", self.settings.output_dir.display()), BarKind::Normal)
                        .clicked()
                    {
                        let _ = std::fs::create_dir_all(&self.settings.output_dir);
                        let _ = open::that(&self.settings.output_dir);
                    }
                    if bar_button(ui, Icon::Help, None, t!("단축키와 사용 요령 (F1)", "Shortcuts and tips (F1)"), BarKind::Normal).clicked() {
                        self.show_help = !self.show_help;
                    }
                });
                ui.add_space(6.0);
                if theme::lang_switch(ui, &mut self.settings.english) {
                    crate::i18n::set_english(self.settings.english);
                    if let Some(d) = self.settings_draft.as_mut() {
                        d.english = self.settings.english;
                    }
                    self.on_language_changed();
                }
                ui.add_space(8.0);
                match &self.engine_state {
                    EngineState::Ready => {
                        let pending = self.queue.len() + usize::from(self.active.is_some());
                        let label = if pending > 0 { tf!("생성 중 · {pending}건", "Generating · {pending}") } else { t!("준비됨", "Ready").to_owned() };
                        status_pill(ui, GREEN, &label, self.active.is_some()).on_hover_text(t!("엔진이 켜져 있습니다", "The engine is running"));
                    }
                    EngineState::Loading { since } => {
                        status_pill(ui, AMBER, &tf!("모델 불러오는 중 · {}초", "Loading model · {} s", since.elapsed().as_secs()), true);
                    }
                    EngineState::Stopped => {
                        let r = status_pill(ui, TEXT_FAINT, t!("엔진 꺼짐 · 눌러서 시작", "Engine off · click to start"), false)
                            .on_hover_cursor(CursorIcon::PointingHand)
                            .on_hover_text(t!("엔진을 시작해 모델을 불러옵니다 (약 40초)", "Starts the engine and loads the model (about 40 s)"));
                        if r.clicked() {
                            self.start_engine_if_possible();
                        }
                    }
                    EngineState::Error(msg) => {
                        let r = status_pill(ui, RED, t!("오류 · 눌러서 재시작", "Error · click to restart"), false)
                            .on_hover_cursor(CursorIcon::PointingHand)
                            .on_hover_text(msg.clone());
                        if r.clicked() {
                            self.start_engine_if_possible();
                        }
                    }
                }
            });
        });
    }

    fn controls(&mut self, ui: &mut Ui) {
        // Pinned action area at the bottom of the panel.
        Panel::bottom("gen_area")
            .frame(Frame::new().fill(PANEL).inner_margin(Margin { left: 16, right: 16, top: 10, bottom: 16 }))
            .show_separator_line(false)
            .show(ui, |ui| self.action_area(ui));

        // Pinned mode bar at the top, so switching never needs scrolling back up.
        Panel::top("mode_bar")
            .frame(Frame::new().fill(PANEL).inner_margin(Margin { left: 16, right: 16, top: 12, bottom: 8 }))
            .show_separator_line(false)
            .show(ui, |ui| self.mode_bar(ui));

        let mut left = ScrollArea::vertical().auto_shrink(false);
        // Self-test only: start the left panel scrolled down so lower cards land in the screenshot.
        if self.autotest.is_some() {
            if let Some(y) = std::env::var("QIS_SCROLL_LEFT").ok().and_then(|v| v.parse::<f32>().ok()) {
                left = left.vertical_scroll_offset(y);
            }
        }
        left.show(ui, |ui| {
            ui.add_space(4.0);
            Frame::new().inner_margin(Margin::symmetric(16, 0)).show(ui, |ui| {
                self.mode_desc(ui);
                ui.add_space(8.0);
                if self.params.mode != GenMode::Text {
                    self.input_card(ui);
                    ui.add_space(6.0);
                }
                self.prompt_card(ui);
                ui.add_space(6.0);
                self.size_card(ui);
                ui.add_space(6.0);
                self.seed_card(ui);
                ui.add_space(6.0);
                if self.params.mode == GenMode::Outpaint {
                    // Expanding: settings that made the border fail more often (speed LoRAs,
                    // fewer steps, CFG, step cache) are fixed and hidden.
                    ui.label(
                        RichText::new(t!("확장은 실패를 줄이려고 Turbo 8단계 · CFG 1 · LoRA 끔 · 빠른 모드 끔으로 고정합니다 (해당 설정은 숨김).", "Expanding is fixed to Turbo 8 steps · CFG 1 · no LoRA · fast mode off to avoid failures (those settings are hidden)."))
                            .size(12.0)
                            .color(TEXT_FAINT),
                    );
                } else {
                    self.advanced_card(ui);
                    ui.add_space(6.0);
                    self.lora_card(ui);
                }
                ui.add_space(12.0);
            });
        });
    }

    /// Shortcuts and quick tips (F1 / the ? button).
    fn help_window(&mut self, ctx: &Context) {
        let mut open = self.show_help;
        egui::Window::new(bold(t!("단축키 · 사용 요령", "Shortcuts · Tips"), 15.0))
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .default_width(460.0)
            .anchor(Align2::CENTER_CENTER, vec2(0.0, 0.0))
            .show(ctx, |ui| {
                let row = |ui: &mut Ui, keys: &str, what: &str| {
                    ui.label(RichText::new(keys).monospace().size(12.5).color(ACCENT_HOVER));
                    ui.label(RichText::new(what).size(13.0));
                    ui.end_row();
                };
                ui.label(RichText::new(t!("단축키", "Shortcuts")).size(12.5).color(TEXT_DIM));
                egui::Grid::new("help_keys").num_columns(2).spacing([18.0, 6.0]).show(ui, |ui| {
                    row(ui, "Ctrl+Enter", t!("생성 (생성 중이면 대기열에 추가)", "Generate (adds to the queue while generating)"));
                    row(ui, "Ctrl+1 ~ 5", t!("모드 전환: 텍스트 · 변형 · 편집 · 부분수정 · 확장", "Switch mode: Text · Variation · Edit · Inpaint · Expand"));
                    row(ui, "Ctrl+V", t!("클립보드 이미지를 입력으로", "Clipboard image as input"));
                    row(ui, "← / →", t!("갤러리 이전 · 다음 이미지", "Previous · next image in the gallery"));
                    row(ui, "F", t!("크게 보기 (휠 확대 · 드래그 이동)", "Full view (wheel zoom · drag to pan)"));
                    row(ui, "Ctrl+S", t!("다른 이름으로 저장 (PNG · WebP · JPG)", "Save as (PNG · WebP · JPG)"));
                    row(ui, "Ctrl+Shift+C", t!("이미지 복사", "Copy image"));
                    row(ui, "Delete", t!("선택한 이미지를 휴지통으로", "Move the selected image to the Recycle Bin"));
                    row(ui, t!("[ ] / Ctrl+휠", "[ ] / Ctrl+wheel"), t!("부분수정 브러시 크기", "Inpaint brush size"));
                    row(ui, "F1", t!("이 창 열기 · 닫기", "Open · close this window"));
                });
                ui.add_space(12.0);
                ui.label(RichText::new(t!("요령", "Tips")).size(12.5).color(TEXT_DIM));
                for tip in [
                    t!("이미지를 창 어디로든 끌어다 놓으면 입력으로 들어갑니다. 이 앱으로 만든 PNG는 생성 설정까지 불러올 수 있습니다.", "Drop an image anywhere on the window to use it as input. PNGs made by this app also restore their generation settings."),
                    t!("갤러리 이미지를 우클릭하면 입력으로 사용 · 설정 불러오기 · 삭제를 바로 할 수 있습니다.", "Right-click a gallery image to use it as input, load its settings, or delete it."),
                    t!("왼쪽의 크기 · 시드 · 고급 · LoRA 카드는 제목을 눌러 접을 수 있고, 접어도 제목 줄에 현재 값이 보입니다.", "The size · seed · advanced · LoRA cards fold when you click their title; the title row still shows the current values."),
                    t!("확장은 미리보기에서 늘리고 싶은 쪽을 잡아 바깥으로 끌면 됩니다. 프롬프트는 비워도 됩니다.", "To expand, drag the side you want to grow outward in the preview. The prompt can stay empty."),
                ] {
                    ui.horizontal_wrapped(|ui| {
                        ui.label(RichText::new("•").color(ACCENT_HOVER));
                        ui.label(RichText::new(tip).size(12.5).color(TEXT));
                    });
                }
            });
        self.show_help = open;
    }

    /// The saved window geometry can come back as 64×64 at (0,0) (saved while minimised);
    /// eframe restores it despite the minimum size. On the first frames, put a window
    /// that small back to the default size.
    fn fix_tiny_window(&mut self, ctx: &Context) {
        if self.window_checked {
            return;
        }
        let t = ctx.input(|i| i.time);
        // On Wayland, leave the first second to the compositor: resizing while WSLg was
        // still configuring the window left it at 640×480, or dropped the connection
        // ("Broken pipe"). WSLg can still ignore the resize (seen while the engine loads;
        // the README lists it as a known issue), but it no longer crashed in 12 runs.
        let settle = if cfg!(windows) { 0.0 } else { 1.0 };
        let small = ctx
            .input(|i| i.viewport().inner_rect)
            .is_some_and(|r| r.width() < 900.0 || r.height() < 500.0);
        if small && t >= settle && t - self.window_resized_at >= 1.0 {
            // Below the minimum size: re-send the limit too, for a compositor that dropped it.
            ctx.send_viewport_cmd(egui::ViewportCommand::MinInnerSize(vec2(1100.0, 680.0)));
            ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(vec2(1480.0, 920.0)));
            if cfg!(windows) {
                // Wayland windows can't place themselves.
                ctx.send_viewport_cmd(egui::ViewportCommand::OuterPosition(Pos2::new(80.0, 60.0)));
            }
            self.window_resized_at = t;
        }
        // Keep watching for the first seconds: a Wayland compositor can settle the size
        // a moment after the first frame (WSLg was seen at 640×480, minimum ignored).
        if t > 4.0 {
            self.window_checked = true;
        } else {
            ctx.request_repaint_after(std::time::Duration::from_millis(250));
        }
    }

    /// Mode buttons in the pinned bar above the scrolling controls.
    fn mode_bar(&mut self, ui: &mut Ui) {
        let mut idx = MODES.iter().position(|m| m.0 == self.params.mode).unwrap_or(0);
        let labels = MODES.map(|m| crate::i18n::pick(&m.1));
        let tips = MODES.map(|m| crate::i18n::pick(&m.2));
        if segmented_tips(ui, &mut idx, &labels, &tips) {
            self.set_mode(MODES[idx].0);
        }
    }

    /// One-line description of the current mode, at the top of the scrolling controls.
    fn mode_desc(&self, ui: &mut Ui) {
        let desc = match self.params.mode {
            GenMode::Text => t!("프롬프트만으로 새 이미지를 만듭니다.", "Creates a new image from the prompt alone."),
            GenMode::Variation => t!("입력 이미지의 구도·색감을 바탕으로 다시 그립니다. 강도로 변화량을 조절하세요.", "Redraws based on the input image\'s layout and colors. Strength controls how much changes."),
            GenMode::Edit => t!("지시문대로 입력 이미지를 고칩니다. 배경 교체, 스타일 변경, 글자 수정 등.", "Changes the input image as instructed: replace the background, change the style, fix text, etc."),
            GenMode::Inpaint => t!("입력 이미지에서 브러시로 칠한 부분만 다시 그립니다. 나머지는 그대로 유지됩니다.", "Redraws only the area painted with the brush. The rest stays as it is."),
            GenMode::Outpaint => t!("입력 이미지의 바깥을 넓혀 장면을 이어서 그립니다. 원본 부분은 그대로 유지됩니다.", "Widens the input image and continues the scene. The original part stays as it is."),
        };
        ui.label(RichText::new(desc).size(12.5).color(TEXT_DIM));
    }

    /// Switches the generation mode and prepares the inputs/mask it needs.
    fn set_mode(&mut self, mode: GenMode) {
        if mode == self.params.mode {
            return;
        }
        self.params.mode = mode;
        if matches!(mode, GenMode::Variation | GenMode::Inpaint | GenMode::Outpaint) && self.inputs.len() > 1 {
            self.inputs.truncate(1);
        }
        if mode == GenMode::Outpaint {
            self.mask_editing = true;
            if !self.inputs.is_empty() {
                self.auto_outpaint_prompt(true);
            }
        }
        if mode == GenMode::Inpaint {
            let size = self.inputs.first().map(|i| i.size());
            let stale = match (&self.mask, size) {
                // Aspect changed (different first image) → new mask.
                (Some(m), Some((w, h))) => (m.w as f32 / m.h as f32 - w as f32 / h as f32).abs() > 0.01,
                (None, Some(_)) => true,
                _ => false,
            };
            if stale {
                let (w, h) = size.unwrap();
                self.mask = Some(crate::mask::MaskEditor::new(w, h));
            }
            self.mask_editing = true;
        }
    }

    fn input_card(&mut self, ui: &mut Ui) {
        let ctx = ui.ctx().clone();
        let edit = self.params.mode == GenMode::Edit;
        card().show(ui, |ui| {
            ui.horizontal(|ui| {
                section(ui, if edit { t!("참조 이미지", "Reference images") } else { t!("입력 이미지", "Input image") });
                if edit {
                    ui.label(RichText::new(format!("{}/{MAX_INPUTS}", self.inputs.len())).size(12.0).color(TEXT_FAINT));
                }
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.spacing_mut().item_spacing.x = 4.0;
                    if ui.small_button(t!("📋 붙여넣기", "📋 Paste")).on_hover_text(t!("클립보드 이미지 (Ctrl+V)", "Clipboard image (Ctrl+V)")).clicked() {
                        self.paste_input(&ctx);
                    }
                    if ui.small_button(t!("📂 열기", "📂 Open")).clicked() {
                        self.pick_input_files(&ctx);
                    }
                });
            });

            if self.inputs.is_empty() {
                let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 128.0), Sense::click());
                let hot = resp.hovered();
                ui.painter().rect_filled(rect, 10, if hot { WIDGET } else { INPUT });
                let stroke = Stroke::new(1.5, if hot { ACCENT } else { BORDER });
                let pts = [rect.left_top(), rect.right_top(), rect.right_bottom(), rect.left_bottom(), rect.left_top()];
                ui.painter().extend(egui::Shape::dashed_line(&pts, stroke, 6.0, 4.0));
                ui.painter().text(rect.center() - vec2(0.0, 22.0), Align2::CENTER_CENTER, "🖼", FontId::proportional(26.0), TEXT_DIM);
                ui.painter().text(rect.center() + vec2(0.0, 8.0), Align2::CENTER_CENTER, t!("이미지를 끌어다 놓거나 클릭해서 선택", "Drag an image here or click to choose"), FontId::proportional(13.5), TEXT);
                ui.painter().text(
                    rect.center() + vec2(0.0, 30.0),
                    Align2::CENTER_CENTER,
                    t!("Ctrl+V 붙여넣기 · 갤러리 우클릭 → 입력으로 사용", "Ctrl+V paste · right-click in gallery → use as input"),
                    FontId::proportional(12.0),
                    TEXT_FAINT,
                );
                if resp.on_hover_cursor(CursorIcon::PointingHand).clicked() {
                    self.pick_input_files(&ctx);
                }
            } else {
                let gap = 8.0;
                let cols = if edit { MAX_INPUTS } else { 1 };
                let tile_w = (ui.available_width() - gap * (cols as f32 - 1.0)) / cols as f32;
                let tile_h = if edit { tile_w } else { 170.0 };
                let mut remove = None;
                let mut add = false;
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = gap;
                    for (i, input) in self.inputs.iter().enumerate() {
                        let (rect, resp) = ui.allocate_exact_size(vec2(tile_w, tile_h), Sense::hover());
                        ui.painter().rect_filled(rect, 10, INPUT);
                        let img = fit(rect.shrink(4.0), input.tex.size_vec2());
                        egui::Image::new(&input.tex).corner_radius(6).paint_at(ui, img);
                        if edit {
                            let badge = Rect::from_min_size(rect.left_top() + vec2(6.0, 6.0), vec2(20.0, 20.0));
                            ui.painter().circle_filled(badge.center(), 10.0, Color32::from_black_alpha(170));
                            ui.painter().text(badge.center(), Align2::CENTER_CENTER, (i + 1).to_string(), FontId::proportional(12.0), Color32::WHITE);
                        }
                        let x = Rect::from_min_size(rect.right_top() + vec2(-28.0, 6.0), vec2(22.0, 22.0));
                        let xr = ui.interact(x, ui.scope_id().with(("rm_input", i)), Sense::click());
                        if resp.hovered() || xr.hovered() {
                            ui.painter().circle_filled(x.center(), 11.0, if xr.hovered() { RED } else { Color32::from_black_alpha(190) });
                            ui.painter().text(x.center(), Align2::CENTER_CENTER, GLYPH_CLOSE, FontId::proportional(12.0), Color32::WHITE);
                        }
                        if xr.on_hover_text(t!("빼기", "Remove")).clicked() {
                            remove = Some(i);
                        }
                        let (w, h) = input.size();
                        resp.on_hover_text(format!("{} · {w}×{h}", input.name));
                    }
                    if edit && self.inputs.len() < MAX_INPUTS {
                        let (rect, resp) = ui.allocate_exact_size(vec2(tile_w, tile_h), Sense::click());
                        let hot = resp.hovered();
                        let pts = [rect.left_top(), rect.right_top(), rect.right_bottom(), rect.left_bottom(), rect.left_top()];
                        ui.painter().extend(egui::Shape::dashed_line(&pts, Stroke::new(1.2, if hot { ACCENT } else { BORDER }), 5.0, 4.0));
                        ui.painter().text(rect.center(), Align2::CENTER_CENTER, "＋", FontId::proportional(22.0), if hot { Color32::WHITE } else { TEXT_DIM });
                        if resp.on_hover_text(t!("참조 이미지 추가", "Add reference image")).on_hover_cursor(CursorIcon::PointingHand).clicked() {
                            add = true;
                        }
                    }
                });
                if let Some(i) = remove {
                    self.inputs.remove(i);
                }
                if add {
                    self.pick_input_files(&ctx);
                }
            }

            match self.params.mode {
                GenMode::Variation => {
                    ui.add_space(4.0);
                    ui.horizontal(|ui| {
                        ui.label(t!("변형 강도", "Strength"));
                        ui.add(egui::Slider::new(&mut self.params.strength, 0.1..=1.0).step_by(0.05).fixed_decimals(2));
                        let rec = GenParams::default().strength;
                        if reset_button(ui, (self.params.strength - rec).abs() > 1e-3, &tf!("권장값 {rec:.2}으로 되돌리기", "Reset to the recommended {rec:.2}")) {
                            self.params.strength = rec;
                        }
                    });
                    // sd.cpp trims the schedule to ceil(steps × strength) (measured: 0.3→3, 0.6→5 of 8).
                    let total = if self.params.step_mode == StepMode::Turbo {
                        crate::config::TURBO_SIGMAS.len() - 1
                    } else {
                        self.params.steps as usize
                    };
                    let steps = ((total as f32 * self.params.strength) - 1e-4).ceil().max(1.0) as usize;
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(t!("◀ 원본 유지", "◀ Keep original")).size(12.0).color(TEXT_FAINT));
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            ui.label(RichText::new(t!("크게 변경 ▶", "Change a lot ▶")).size(12.0).color(TEXT_FAINT));
                            ui.label(RichText::new(tf!("약 {steps}단계", "about {steps} steps")).size(12.0).color(ACCENT_HOVER));
                        });
                    });
                }
                GenMode::Edit => {
                    if !self.settings.vision_ok() {
                        ui.add_space(4.0);
                        Frame::new().fill(AMBER.gamma_multiply(0.08)).corner_radius(8).inner_margin(Margin::same(10)).show(ui, |ui| {
                            ui.label(RichText::new(t!("편집 모드에는 비전 모듈(약 1.2GB)이 필요합니다.", "Edit mode needs the vision module (about 1.2 GB).")).size(12.5).color(AMBER));
                            if self.dl.running {
                                ui.horizontal(|ui| {
                                    ui.spinner();
                                    let pct = if self.dl.total > 0 { self.dl.done as f32 / self.dl.total as f32 } else { 0.0 };
                                    ui.add(egui::ProgressBar::new(pct).desired_height(6.0).fill(ACCENT).show_percentage());
                                });
                            } else if ui.button(t!("⬇ 비전 모듈 다운로드", "⬇ Download vision module")).clicked() {
                                self.dl = DlState { running: true, ..Default::default() };
                                self.dl_is_lora = false;
                                let task = DlTask { source: &VISION_SOURCE, dest: self.settings.vision.clone() };
                                self.dl.cancel = Some(download::spawn(vec![task], self.dl_tx.clone(), ctx.clone()));
                            }
                            if let Some(err) = &self.dl.error {
                                ui.label(RichText::new(err).size(12.0).color(RED));
                            }
                        });
                    } else {
                        ui.label(
                            RichText::new(t!("최대 3장까지 넣을 수 있습니다. 여러 장이면 \"첫 번째 이미지의 인물을 두 번째 이미지의 배경에\"처럼 지칭하세요.", "Up to 3 images. With several, refer to them like \"put the person from the first image into the background of the second\"."))
                                .size(12.0)
                                .color(TEXT_FAINT),
                        );
                        if !self.engine_vision {
                            ui.label(RichText::new(os_t!("ⓘ 첫 편집 때 엔진이 비전 모듈을 불러오며 다시 시작됩니다.", "ⓘ The engine restarts to load the vision module on the first edit.", "ℹ 첫 편집 때 엔진이 비전 모듈을 불러오며 다시 시작됩니다.", "ℹ The engine restarts to load the vision module on the first edit.")).size(12.0).color(AMBER));
                        }
                    }
                }
                GenMode::Inpaint => {
                    ui.add_space(4.0);
                    if let Some(m) = self.mask.as_mut() {
                        ui.horizontal(|ui| {
                            ui.selectable_value(&mut m.erase, false, os_t!("🖌 브러시", "🖌 Brush", "✏ 브러시", "✏ Brush"));
                            ui.selectable_value(&mut m.erase, true, os_t!("🧽 지우개", "🧽 Eraser", "✖ 지우개", "✖ Eraser"));
                            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                if ui.small_button(t!("반전", "Invert")).on_hover_text(t!("칠한 곳과 안 칠한 곳을 바꿉니다", "Swap painted and unpainted areas")).clicked() {
                                    m.invert();
                                }
                                if ui.small_button(t!("전체 지우기", "Clear all")).clicked() {
                                    m.clear();
                                }
                            });
                        });
                        ui.horizontal(|ui| {
                            ui.label(t!("브러시 크기", "Brush size"));
                            ui.add(egui::Slider::new(&mut m.brush, 4.0..=400.0).logarithmic(true).suffix(" px"));
                        });
                    }
                    ui.horizontal(|ui| {
                        ui.label(t!("수정 강도", "Strength"));
                        ui.add(egui::Slider::new(&mut self.params.inpaint_strength, 0.3..=1.0).step_by(0.05).fixed_decimals(2));
                        let rec = GenParams::default().inpaint_strength;
                        if reset_button(ui, (self.params.inpaint_strength - rec).abs() > 1e-3, &tf!("권장값 {rec:.2}로 되돌리기", "Reset to the recommended {rec:.2}")) {
                            self.params.inpaint_strength = rec;
                        }
                    });
                    ui.label(
                        RichText::new(t!("0.8~0.9 권장 · 1.0이면 칠한 경계 밖까지 크게 바뀔 수 있습니다", "0.8–0.9 recommended · at 1.0 changes can spill past the painted edge"))
                            .size(12.0)
                            .color(TEXT_FAINT),
                    );
                    if !self.inputs.is_empty() && !self.mask_editing {
                        if ui.button(os_t!("🖌 마스크 편집으로 돌아가기", "🖌 Back to mask editing", "✏ 마스크 편집으로 돌아가기", "✏ Back to mask editing")).clicked() {
                            self.mask_editing = true;
                        }
                    } else if !self.inputs.is_empty() {
                        ui.label(
                            RichText::new(t!("가운데 화면에서 바꿀 부분을 칠하세요 · 우클릭은 지우기 · Ctrl+휠 또는 [ ]로 크기 조절", "Paint the area to change in the center view · right-click erases · Ctrl+wheel or [ ] sets size"))
                                .size(12.0)
                                .color(ACCENT_HOVER),
                        );
                    }
                }
                GenMode::Outpaint => self.outpaint_controls(ui),
                GenMode::Text => {}
            }
        });
    }

    /// Expansion presets + per-side sliders for outpainting.
    fn outpaint_controls(&mut self, ui: &mut Ui) {
        ui.add_space(4.0);
        let size = self.inputs.first().map(|i| i.size());
        ui.label(RichText::new(t!("빠른 선택", "Quick presets")).size(12.5).color(TEXT_DIM));
        let presets: [(&str, &str, Option<(f32, f32)>); 5] = [
            ("↔ 16:9", t!("좌우로 넓게", "Wider (left/right)"), Some((16.0, 9.0))),
            ("↕ 9:16", t!("위아래로 길게", "Taller (top/bottom)"), Some((9.0, 16.0))),
            ("⬜ 1:1", t!("정사각형으로", "To square"), Some((1.0, 1.0))),
            ("4:3", t!("4:3으로", "To 4:3"), Some((4.0, 3.0))),
            (os_t!("⤢ 줌아웃", "⤢ Zoom out", "⛶ 줌아웃", "⛶ Zoom out"), t!("사방으로 1.5배", "1.5× on all sides"), None),
        ];
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing = vec2(4.0, 4.0);
            for (label, tip, aspect) in presets {
                if ui.small_button(label).on_hover_text(tip).clicked() {
                    self.params.outpaint = match (aspect, size) {
                        (Some((aw, ah)), Some((w, h))) => crate::config::pads_for_aspect(w, h, aw, ah),
                        _ => [0.25, 0.25, 0.25, 0.25],
                    };
                    self.mask_editing = true;
                }
            }
        });
        egui::Grid::new("outpaint_sides").num_columns(2).spacing([8.0, 4.0]).show(ui, |ui| {
            for (i, name) in [t!("왼쪽", "Left"), t!("오른쪽", "Right"), t!("위", "Top"), t!("아래", "Bottom")].iter().enumerate() {
                ui.label(*name);
                let mut pct = self.params.outpaint[i] * 100.0;
                if ui.add(egui::Slider::new(&mut pct, 0.0..=100.0).step_by(5.0).fixed_decimals(0).suffix(" %")).changed() {
                    self.params.outpaint[i] = pct / 100.0;
                    self.mask_editing = true;
                }
                ui.end_row();
            }
        });
        let guided = self.settings.vision_ok();
        if let Some((w, h)) = size {
            let (ow, oh) = self.out_dims();
            let text = if guided {
                let (fw, fh) = crate::engine::guided_final_size(w, h, self.params.outpaint, ow, oh);
                tf!("원본 {w}×{h} → 결과 약 {fw}×{fh} (원본 부분은 화질 그대로 합성)", "Original {w}×{h} → result about {fw}×{fh} (original part kept at full quality)")
            } else {
                let (cw, ch) = crate::config::padded_size(w, h, self.params.outpaint);
                tf!("캔버스 {w}×{h} → {cw}×{ch} · 생성 크기 {ow}×{oh}", "Canvas {w}×{h} → {cw}×{ch} · output size {ow}×{oh}")
            };
            ui.label(RichText::new(text).size(12.0).color(TEXT_FAINT));
        }
        if self.params.outpaint.iter().all(|p| *p <= 0.0) {
            ui.label(RichText::new(t!("확장할 방향을 하나 이상 정하세요", "Pick at least one direction to expand")).size(12.0).color(AMBER));
        }
        // Old builds filled the prompt with a reference-image instruction; replace it.
        if self.params.prompt.starts_with("1번 이미지의 장면을") {
            self.params.prompt = crate::engine::outpaint_default().to_owned();
        }
        let passes = if guided {
            crate::config::plan_guided_passes(self.params.outpaint, crate::config::GUIDED_MAX_AREA).len()
        } else {
            crate::config::plan_outpaint_passes(self.params.outpaint, crate::config::OUTPAINT_MAX_STEP).len()
        };
        let per = match (guided, self.settings.variant) {
            (true, ModelVariant::Int8) => 20,
            (true, _) => 32,
            (false, _) => 18,
        };
        if passes > 0 {
            let how = if passes == 1 { t!("한 번에 넓힙니다", "Expands in one pass").to_owned() } else { tf!("{passes}단계로 나눠 넓힙니다", "Expands in {passes} passes") };
            ui.label(
                RichText::new(tf!("{how} · 약 {}초", "{how} · about {} s", passes * per))
                    .size(12.0)
                    .color(ACCENT_HOVER),
            );
        }
        if guided {
            ui.label(
                RichText::new(t!("프롬프트는 비워 둬도 됩니다. 새로 채울 부분에 원하는 내용(예: 바다가 보이는 창가)을 쓰면 반영합니다. 배경 자체를 바꾸려면 편집 모드를 쓰세요.", "The prompt can stay empty. Describe what should fill the new area (e.g. a window with a sea view) and it will be used. To change the background itself, use Edit mode."))
                    .size(12.0)
                    .color(TEXT_FAINT),
            );
            // Measured on the same picture and seeds: a guided pass took ~30 s on Q4 and
            // ~19 s on the INT8 tensor-core model, with the same composition and detail.
            if self.settings.variant == ModelVariant::Q4 && self.settings.int8_model.is_file() {
                ui.horizontal_wrapped(|ui| {
                    ui.label(RichText::new(t!("⚡ INT8 모델이면 확장이 약 35% 빨라집니다 (화질 동일).", "⚡ The INT8 model expands about 35% faster (same quality).")).size(12.0).color(ACCENT_HOVER));
                    if ui
                        .small_button(t!("INT8로 바꾸기", "Switch to INT8"))
                        .on_hover_text(t!("엔진을 INT8 텐서코어 모델로 다시 시작합니다 (약 40초). 아래 '모델' 메뉴에서 언제든 되돌릴 수 있습니다.", "Restarts the engine with the INT8 tensor-core model (about 40 s). You can switch back anytime in the \'Model\' menu below."))
                        .clicked()
                    {
                        let ctx = ui.ctx().clone();
                        self.switch_variant(&ctx, ModelVariant::Int8);
                    }
                });
            }
        } else {
            ui.label(
                RichText::new(t!("⚠ 비전 모듈(약 1.2GB)이 없어 기본 방식으로 확장합니다. 입력 이미지 카드 위의 편집 모드에서 받으면 훨씬 자연스럽고 빠르게 확장됩니다.", "⚠ No vision module (about 1.2 GB): expanding the basic way. Download it in Edit mode (input image card) for much more natural and faster expansion."))
                    .size(12.0)
                    .color(AMBER),
            );
        }
        if !self.inputs.is_empty() && !self.mask_editing && ui.button(t!("↔ 확장 미리보기로 돌아가기", "↔ Back to expand preview")).clicked() {
            self.mask_editing = true;
        }
    }

    fn lora_card(&mut self, ui: &mut Ui) {
        let ctx = ui.ctx().clone();
        let on: Vec<String> = self
            .params
            .loras
            .iter()
            .filter(|l| l.enabled && self.lora_files.contains(&l.file))
            .map(|l| crate::lora::display_name(&l.file))
            .collect();
        let summary = match on.len() {
            0 => t!("사용 안 함", "Not used").to_owned(),
            1 => on[0].clone(),
            n => tf!("{n}개 사용 중", "{n} in use"),
        };
        fold_card(ui, "lora", "LoRA", &summary, true, |ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new(t!("models/loras 폴더의 파일", "Files in models/loras")).size(12.0).color(TEXT_FAINT));
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.spacing_mut().item_spacing.x = 4.0;
                    if ui.small_button("⟳").on_hover_text(t!("폴더 다시 읽기", "Rescan folder")).clicked() {
                        self.lora_files = crate::lora::scan(&self.settings.lora_dir());
                        self.engine.refresh();
                    }
                    if ui.small_button(t!("폴더 열기", "Open folder")).on_hover_text(t!("LoRA 파일(.safetensors)을 이 폴더에 넣으세요", "Put LoRA files (.safetensors) in this folder")).clicked() {
                        let dir = self.settings.lora_dir();
                        let _ = std::fs::create_dir_all(&dir);
                        let _ = open::that(&dir);
                    }
                });
            });

            let files = self.lora_files.clone();
            let mut turned_on: Option<String> = None;
            for file in &files {
                if !self.params.loras.iter().any(|l| &l.file == file) {
                    self.params.loras.push(crate::lora::LoraSel { file: file.clone(), weight: 1.0, enabled: false });
                }
                let sel = self.params.loras.iter_mut().find(|l| &l.file == file).unwrap();
                let was_on = sel.enabled;
                ui.horizontal(|ui| {
                    ui.checkbox(&mut sel.enabled, crate::lora::display_name(file)).on_hover_text(file.as_str());
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if reset_button(ui, sel.enabled && (sel.weight - 1.0).abs() > 1e-3, t!("기본 강도 1.00으로 되돌리기", "Reset to the default strength 1.00")) {
                            sel.weight = 1.0;
                        }
                        ui.add_enabled(
                            sel.enabled,
                            egui::DragValue::new(&mut sel.weight).range(0.0..=2.0).speed(0.01).fixed_decimals(2),
                        )
                        .on_hover_text(t!("적용 강도", "Strength"));
                    });
                });
                let on = sel.enabled;
                if on && !was_on {
                    turned_on = Some(file.clone());
                }
                if on {
                    self.lora_hint(ui, file);
                }
            }
            // Switching one on turns off the ones it can't run with (two speed schedules,
            // or "keep the frame" with "move the camera").
            if let Some(f) = turned_on {
                let mut off = Vec::new();
                for l in &mut self.params.loras {
                    if l.enabled && crate::lora::conflicts(&l.file, &f) {
                        l.enabled = false;
                        off.push(crate::lora::display_name(&l.file));
                    }
                }
                if !off.is_empty() {
                    self.toast(tf!("함께 쓸 수 없어 끔: {}", "Turned off (can\'t be combined): {}", off.join(", ")), TEXT_DIM);
                }
            }

            // Recommended LoRAs that aren't in the folder yet.
            let missing: Vec<&(&crate::config::ModelSource, (&str, &str))> =
                crate::lora::RECOMMENDED.iter().filter(|(s, _)| !files.iter().any(|f| f == s.file)).collect();
            if files.is_empty() {
                ui.label(RichText::new(t!("LoRA 파일이 없습니다.", "No LoRA files.")).size(12.5).color(TEXT_DIM));
            }
            if self.dl.running && self.dl_is_lora {
                ui.horizontal(|ui| {
                    ui.spinner();
                    let pct = if self.dl.total > 0 { self.dl.done as f32 / self.dl.total as f32 } else { 0.0 };
                    ui.add(egui::ProgressBar::new(pct).desired_height(6.0).fill(ACCENT).show_percentage());
                });
            } else if !missing.is_empty() && !self.dl.running {
                ui.add_space(4.0);
                ui.label(RichText::new(t!("추천 LoRA 받기", "Download recommended LoRAs")).size(12.0).color(TEXT_FAINT));
                for (src, desc) in missing {
                    let mb = src.approx_bytes / 1_000_000;
                    if ui.button(format!("⬇ {} ({mb}MB)", src.name())).on_hover_text(crate::i18n::pick(desc)).clicked() {
                        self.dl = DlState { running: true, ..Default::default() };
                        self.dl_is_lora = true;
                        self.dl_lora_file = Some(src.file);
                        let task = DlTask { source: src, dest: self.settings.lora_dir().join(src.file) };
                        self.dl.cancel = Some(download::spawn(vec![task], self.dl_tx.clone(), ctx.clone()));
                    }
                }
            }
        });
    }

    /// One-line guide under an enabled LoRA (plus the camera picker for Multiple-Angles).
    fn lora_hint(&mut self, ui: &mut Ui, file: &str) {
        use crate::lora::Kind;
        let hint = |ui: &mut Ui, text: &str, color: Color32| {
            ui.label(RichText::new(text).size(12.0).color(color));
        };
        let edit = self.params.mode == GenMode::Edit;
        match crate::lora::kind(file) {
            Kind::Lightning => hint(ui, t!("⚡ 4단계로 생성 · 장당 약 12초 (8단계 약 18초) · 2단계보다 디테일이 나음", "⚡ Generates in 4 steps · about 12 s per image (8 steps: about 18 s) · more detail than 2 steps"), ACCENT_HOVER),
            Kind::Consistency if edit => hint(
                ui,
                t!("📐 편집 결과의 구도·위치를 원본에 고정합니다 · 큰 물체 제거나 자세를 바꾸는 편집에는 끄세요", "📐 Keeps the edit\'s layout and positions on the original · turn off for big removals or pose changes"),
                ACCENT_HOVER,
            ),
            Kind::Angles if edit => self.angle_picker(ui),
            Kind::Consistency | Kind::Angles => {
                ui.horizontal(|ui| {
                    hint(ui, t!("편집 모드에서만 적용됩니다", "Only applies in Edit mode"), AMBER);
                    if ui.small_button(t!("편집 모드로", "To Edit mode")).clicked() {
                        self.params.mode = GenMode::Edit;
                    }
                });
            }
            Kind::Other => {}
        }
    }

    /// Camera picker for the Multiple-Angles LoRA: 12-direction dial + height + close-up.
    /// Every change rewrites the prompt in the LoRA's `<mva> …` format.
    fn angle_picker(&mut self, ui: &mut Ui) {
        use crate::lora::AZIMUTHS;
        let mut changed = false;
        Frame::new().fill(INPUT).corner_radius(10).inner_margin(Margin::same(10)).show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            ui.horizontal(|ui| {
                // Dial seen from above: the subject in the middle, its front facing down.
                let (rect, _) = ui.allocate_exact_size(vec2(132.0, 132.0), Sense::hover());
                let c = rect.center();
                let r = 50.0;
                let p = ui.painter();
                p.circle_stroke(c, r, Stroke::new(1.0, BORDER));
                p.rect_filled(Rect::from_center_size(c, vec2(22.0, 22.0)), 5, WIDGET_HOVER);
                p.line_segment([c + vec2(-6.0, 11.0), c + vec2(6.0, 11.0)], Stroke::new(2.0, ACCENT_HOVER));
                let dot = |i: usize| {
                    let a = (i as f32 * 30.0).to_radians();
                    c + vec2(a.sin(), a.cos()) * r
                };
                let sel = dot(self.angle_az);
                p.line_segment([sel, c], Stroke::new(1.5, ACCENT.gamma_multiply(0.7)));
                for i in 0..AZIMUTHS.len() {
                    let pos = dot(i);
                    let hit = Rect::from_center_size(pos, vec2(20.0, 20.0));
                    let resp = ui
                        .interact(hit, ui.scope_id().with(("az", i)), Sense::click())
                        .on_hover_cursor(CursorIcon::PointingHand)
                        .on_hover_text(crate::lora::az_label(i));
                    let on = i == self.angle_az;
                    let p = ui.painter();
                    if on {
                        p.circle_filled(pos, 8.0, ACCENT);
                        p.circle_stroke(pos, 8.0, Stroke::new(1.5, Color32::WHITE));
                    } else {
                        p.circle_filled(pos, if resp.hovered() { 7.0 } else { 5.0 }, if resp.hovered() { TEXT_DIM } else { WIDGET_HOVER });
                    }
                    if resp.clicked() {
                        self.angle_az = i;
                        changed = true;
                    }
                }
                let p = ui.painter();
                let f = FontId::proportional(10.5);
                p.text(c + vec2(0.0, r + 13.0), Align2::CENTER_CENTER, t!("정면", "Front"), f.clone(), TEXT_FAINT);
                p.text(c + vec2(0.0, -r - 13.0), Align2::CENTER_CENTER, t!("뒤", "Back"), f.clone(), TEXT_FAINT);

                ui.vertical(|ui| {
                    ui.label(bold(format!("{} · {}", crate::lora::az_label(self.angle_az), crate::lora::el_label(self.angle_el)), 13.5));
                    ui.add_space(2.0);
                    ui.label(RichText::new(t!("카메라 높이", "Camera height")).size(12.0).color(TEXT_DIM));
                    let mut el = self.angle_el;
                    if segmented(ui, &mut el, &[t!("눈높이", "Eye level"), "30°", "60°", t!("위", "Top")]) {
                        self.angle_el = el;
                        changed = true;
                    }
                    ui.add_space(2.0);
                    if ui.checkbox(&mut self.angle_close, t!("가까이 (클로즈업)", "Closer (close-up)")).changed() {
                        changed = true;
                    }
                });
            });
            ui.label(
                RichText::new(t!("입력 이미지 1장을 넣고 방향을 고르면 그 각도에서 다시 촬영합니다 · 배경은 회색 스튜디오로 바뀝니다(제품 단독컷용) · 각도가 안 바뀌면 강도를 0.8로 낮추세요", "Add one input image and pick a direction to reshoot it from that angle · the background becomes a grey studio (for product shots) · lower the strength to 0.8 if the angle doesn\'t change"))
                    .size(11.5)
                    .color(TEXT_FAINT),
            );
        });
        let wanted = crate::lora::angles_prompt(self.angle_az, self.angle_el, self.angle_close);
        if changed || !self.params.prompt.trim_start().starts_with("<mva>") && self.params.prompt.trim().is_empty() {
            self.params.prompt = wanted;
            self.params.batch = false;
        }
    }

    fn prompt_card(&mut self, ui: &mut Ui) {
        card().show(ui, |ui| {
            ui.horizontal(|ui| {
                section(ui, t!("프롬프트", "Prompt"));
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.spacing_mut().item_spacing.x = 4.0;
                    if ui.small_button(GLYPH_CLOSE).on_hover_text(t!("지우기", "Clear")).clicked() {
                        self.params.prompt.clear();
                    }
                    let recent = self.settings.recent_prompts.clone();
                    ui.add_enabled_ui(!recent.is_empty(), |ui| {
                        ui.menu_button(RichText::new(t!("🕘 최근", "🕘 Recent")).size(12.5), |ui| {
                            ui.set_max_width(400.0);
                            let mut remove = None;
                            for (i, p) in recent.iter().enumerate().take(15) {
                                ui.horizontal(|ui| {
                                    if ui.small_button(GLYPH_CLOSE).on_hover_text(t!("이 기록 삭제", "Delete this entry")).clicked() {
                                        remove = Some(i);
                                    }
                                    if ui.button(truncate(p, 60)).on_hover_text(p).clicked() {
                                        self.params.prompt = p.clone();
                                        ui.close();
                                    }
                                });
                            }
                            if let Some(i) = remove {
                                self.settings.recent_prompts.remove(i);
                                if self.settings.recent_prompts.is_empty() {
                                    ui.close();
                                }
                            }
                            ui.separator();
                            if ui.button(t!("🗑 최근 기록 모두 지우기", "🗑 Clear all recent prompts")).clicked() {
                                self.settings.recent_prompts.clear();
                                ui.close();
                            }
                        });
                    });
                    if ui
                        .small_button(t!("가 글씨", "Text"))
                        .on_hover_text(t!("이미지 속 한글 글씨 문장 틀을 끝에 붙입니다.\n\"여기에 문구\"만 원하는 글로 바꾸세요.", "Appends a sign-text template to the prompt.\nReplace \"YOUR TEXT\" with the words you want."))
                        .clicked()
                    {
                        let p = self.params.prompt.trim_end().trim_end_matches([',', '.']).to_owned();
                        let tpl = lang(TEXT_TEMPLATE, TEXT_TEMPLATE_EN);
                        self.params.prompt = if p.is_empty() { tpl.to_owned() } else { format!("{p}, {tpl}") };
                    }
                    if self.params.mode == GenMode::Outpaint && !self.inputs.is_empty() && self.caption_ready() {
                        let busy = self.captioning.is_some() || self.caption_wanted;
                        let r = ui
                            .add_enabled(!busy, egui::Button::new(RichText::new(t!("🖼 사진 설명", "🖼 Describe photo")).size(12.5)).small())
                            .on_hover_text(t!("입력 사진을 보고 장면 설명을 새로 만들어 프롬프트에 넣습니다 (GPU · 약 20초, 생성 중이면 끝난 뒤에)", "Looks at the input photo and writes a new scene description into the prompt (GPU · about 20 s, after any running generation)"));
                        if r.clicked() {
                            self.params.prompt.clear();
                            self.auto_prompt = None;
                            self.start_caption();
                        }
                    }
                    if ui.small_button(t!("✨ 예시", "✨ Example")).on_hover_text(t!("예시 프롬프트 넣기 (누를 때마다 다음 예시)", "Insert an example prompt (next one on each click)")).clicked() {
                        let list: &[&str] = match self.params.mode {
                            GenMode::Text => lang(&EXAMPLES[..], &EXAMPLES_EN[..]),
                            GenMode::Variation => lang(&VARIATION_EXAMPLES[..], &VARIATION_EXAMPLES_EN[..]),
                            GenMode::Edit => lang(&EDIT_EXAMPLES[..], &EDIT_EXAMPLES_EN[..]),
                            GenMode::Inpaint => lang(&INPAINT_EXAMPLES[..], &INPAINT_EXAMPLES_EN[..]),
                            GenMode::Outpaint => lang(&OUTPAINT_EXAMPLES[..], &OUTPAINT_EXAMPLES_EN[..]),
                        };
                        self.params.prompt = list[self.example_idx % list.len()].to_owned();
                        self.example_idx += 1;
                    }
                });
            });
            ui.add(
                TextEdit::multiline(&mut self.params.prompt)
                    .hint_text(if self.params.batch {
                        t!("한 줄에 프롬프트 하나씩 입력하세요. 위에서부터 순서대로 생성합니다.\n예:\n밤 시장 포장마차, 전구 줄 조명\n봄날 강변의 벚꽃 수채화", "One prompt per line. They are generated from top to bottom.\nExample:\nA night-market food stall, string lights\nCherry blossoms by a river in spring, watercolor")
                    } else {
                        match self.params.mode {
                        GenMode::Text => t!("만들고 싶은 이미지를 자세히 묘사하세요.\n주제 → 세부 → 배경 → 조명·분위기 → 촬영/화풍\n예: 밤 시장 포장마차, 노란 간판, 피어오르는 김, 따뜻한 전구 조명", "Describe the image you want in detail.\nSubject → details → background → lighting/mood → camera/style\nExample: A night-market food stall, yellow sign, rising steam, warm string lights"),
                        GenMode::Variation => t!("결과 이미지 전체를 묘사하세요.\n예: 같은 장면을 따뜻한 수채화 그림으로, 부드러운 붓 터치", "Describe the whole result image.\nExample: the same scene as a warm watercolor painting, soft brush strokes"),
                        GenMode::Edit => t!("바꿀 것과 유지할 것을 함께 지시하세요.\n예: 배경을 해변으로 바꿔줘. 인물은 그대로 유지해.", "Say what to change and what to keep.\nExample: change the background to a beach. Keep the person as is."),
                        GenMode::Inpaint => t!("칠한 부분에 들어갈 내용을 묘사하세요.\n예: 초록 몬스테라 화분이 있는 밝은 카페 배경", "Describe what goes into the painted area.\nExample: a bright cafe background with a green monstera plant"),
                        GenMode::Outpaint => t!("넓어진 전체 장면을 묘사하세요. 비워 두면 기본 문구로 자연스럽게 이어 그립니다.\n예: 넓은 밝은 회색 스튜디오, 부드러운 조명", "Describe the whole widened scene. Leave it empty to continue the scene with a default description.\nExample: a wide light grey studio, soft lighting"),
                    }})
                    .desired_rows(6)
                    .desired_width(f32::INFINITY)
                    .margin(Margin::same(10)),
            );
            if self.caption_wanted && self.captioning.is_none() {
                ui.horizontal(|ui| {
                    ui.spinner();
                    ui.label(RichText::new(t!("진행 중인 생성이 끝나면 사진 설명을 만듭니다 (GPU를 함께 쓰기 때문)", "The photo description starts when the current generation ends (they share the GPU)")).size(12.0).color(TEXT_DIM));
                });
            }
            if self.captioning.is_some() {
                ui.horizontal(|ui| {
                    ui.spinner();
                    ui.label(
                        RichText::new(if self.generate_after_caption {
                            t!("사진을 보고 장면 설명을 만드는 중… 끝나면 바로 생성합니다 (GPU · 약 20초)", "Describing the photo… generation starts right after (GPU · about 20 s)")
                        } else {
                            t!("사진을 보고 장면 설명을 만드는 중… (GPU · 약 20초)", "Describing the photo… (GPU · about 20 s)")
                        })
                            .size(12.0)
                            .color(ACCENT_HOVER),
                    );
                });
                ui.ctx().request_repaint_after(std::time::Duration::from_millis(300));
            }
            ui.horizontal(|ui| {
                if self.params.batch {
                    let n = self.params.prompt.lines().filter(|l| !l.trim().is_empty()).count();
                    ui.label(RichText::new(tf!("{n}개 프롬프트", "{n} prompts")).size(12.0).color(ACCENT_HOVER));
                } else {
                    ui.label(RichText::new(tf!("{}자", "{} chars", self.params.prompt.chars().count())).size(12.0).color(TEXT_FAINT));
                }
                if self.params.mode != GenMode::Outpaint {
                    ui.checkbox(&mut self.params.batch, RichText::new(t!("여러 프롬프트", "Multiple prompts")).size(12.0))
                        .on_hover_text(t!("한 줄에 프롬프트 하나씩 적으면 위에서부터 순서대로 모두 생성합니다.\n'생성 개수'는 프롬프트마다 적용됩니다.", "Write one prompt per line to generate all of them from top to bottom.\n\'Count\' applies to each prompt."));
                }
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.label(RichText::new(t!("💡 작성 요령", "💡 Tips")).size(12.0).color(ACCENT_HOVER))
                        .on_hover_cursor(CursorIcon::Help)
                        .on_hover_ui(|ui| {
                            ui.set_max_width(360.0);
                            ui.label(bold(t!("한국어 프롬프트 작성 요령", "Prompt writing tips"), 14.0));
                            ui.add_space(2.0);
                            for &(title, body) in lang(&PROMPT_TIPS[..], &PROMPT_TIPS_EN[..]) {
                                ui.label(RichText::new(title).size(13.0).color(TEXT));
                                ui.label(RichText::new(plain_glyphs(body)).size(12.5).color(TEXT_DIM));
                                ui.add_space(4.0);
                            }
                        });
                });
            });

            if self.params.mode == GenMode::Outpaint {
                return; // CFG is fixed at 1 for expanding: a negative prompt would do nothing
            }
            let cfg_on = self.params.cfg > 1.0;
            egui::CollapsingHeader::new(RichText::new(t!("네거티브 프롬프트", "Negative prompt")).size(13.0).color(TEXT_DIM))
                .id_salt("neg")
                .show(ui, |ui| {
                    ui.add(
                        TextEdit::multiline(&mut self.params.negative_prompt)
                            .hint_text(t!("피하고 싶은 요소 (예: 흐릿함, 저화질, 글자 깨짐)", "Things to avoid (e.g. blurry, low quality, broken text)"))
                            .desired_rows(2)
                            .desired_width(f32::INFINITY),
                    );
                    if !cfg_on {
                        ui.label(RichText::new(os_t!("ⓘ CFG가 1이면 네거티브 프롬프트는 적용되지 않습니다. 고급 설정에서 CFG를 올리세요.", "ⓘ With CFG 1 the negative prompt has no effect. Raise CFG in Advanced settings.", "ℹ CFG가 1이면 네거티브 프롬프트는 적용되지 않습니다. 고급 설정에서 CFG를 올리세요.", "ℹ With CFG 1 the negative prompt has no effect. Raise CFG in Advanced settings.")).size(12.0).color(AMBER));
                    }
                });
        });
    }

    fn size_card(&mut self, ui: &mut Ui) {
        let (w, h) = self.out_dims();
        if self.params.mode == GenMode::Outpaint {
            // Expanding: the shape comes from the expansion amounts, so the aspect tiles
            // (ignored here) would only contradict them. Only the pixel budget applies.
            let summary = tf!("{w}×{h} · 확장 범위로 결정", "{w}×{h} · set by expansion");
            fold_card(ui, "size", t!("비율 · 크기", "Aspect · Size"), &summary, true, |ui| {
                ui.label(
                    RichText::new(t!("확장 결과의 비율은 위의 확장 범위(빠른 선택 · 슬라이더 · 미리보기 드래그)로 정해집니다. 여기서는 생성 해상도만 고르세요.", "The result\'s aspect comes from the expansion above (quick presets · sliders · preview drag). Only pick the output resolution here."))
                        .size(12.0)
                        .color(TEXT_DIM),
                );
                ui.add_space(4.0);
                let labels = crate::config::size_labels();
                segmented(ui, &mut self.params.size, &labels);
            });
            return;
        }
        let what = if self.params.custom_size {
            t!("직접 지정", "Custom")
        } else if self.params.mode != GenMode::Text && self.params.follow_input_aspect && !self.inputs.is_empty() {
            t!("입력 비율", "Input aspect")
        } else {
            ASPECTS[crate::config::normalize_aspect(self.params.aspect)].label
        };
        let summary = format!("{w}×{h} · {what}");
        fold_card(ui, "size", t!("비율 · 크기", "Aspect · Size"), &summary, true, |ui| {
            let image_mode = self.params.mode != GenMode::Text;
            if image_mode && !self.params.custom_size {
                ui.checkbox(&mut self.params.follow_input_aspect, t!("입력 이미지 비율 따르기", "Follow the input image\'s aspect"));
            }
            let tiles_on = !self.params.custom_size && !(image_mode && self.params.follow_input_aspect && !self.inputs.is_empty());
            ui.add_enabled_ui(!self.params.custom_size, |ui| {
                // One row: the five shapes side by side, so their proportions compare at a glance.
                let gap = 6.0;
                let n = ASPECTS.len() as f32;
                let tile_w = (ui.available_width() - gap * (n - 1.0)) / n;
                ui.add_enabled_ui(tiles_on, |ui| {
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = gap;
                        for (i, a) in ASPECTS.iter().enumerate() {
                            let on = self.params.aspect == i && tiles_on;
                            let tip = format!("{} {}", a.label, a.hint());
                            if aspect_tile(ui, on, a.label, a.hint(), a.w, a.h, vec2(tile_w, 74.0)).on_hover_text(tip).clicked() {
                                self.params.aspect = i;
                            }
                        }
                    });
                });
                ui.add_space(2.0);
                let labels = crate::config::size_labels();
                segmented(ui, &mut self.params.size, &labels);
            });
            ui.checkbox(&mut self.params.custom_size, t!("직접 크기 지정", "Custom size"));
            if self.params.custom_size {
                ui.horizontal(|ui| {
                    ui.label(t!("너비", "Width"));
                    ui.add(egui::DragValue::new(&mut self.params.width).range(256..=2048).speed(32));
                    ui.label(t!("높이", "Height"));
                    ui.add(egui::DragValue::new(&mut self.params.height).range(256..=2048).speed(32));
                    if ui.small_button(if cfg!(windows) { "⇄" } else { "↔" }).on_hover_text(t!("가로/세로 바꾸기", "Swap width/height")).clicked() {
                        std::mem::swap(&mut self.params.width, &mut self.params.height);
                    }
                });
                self.params.width = snap32(self.params.width);
                self.params.height = snap32(self.params.height);
                ui.label(RichText::new(t!("32의 배수로 자동 맞춤 · 큰 해상도는 시간이 오래 걸립니다", "Snapped to multiples of 32 · large sizes take longer")).size(12.0).color(TEXT_FAINT));
            }
        });
    }

    fn seed_card(&mut self, ui: &mut Ui) {
        let seed = if self.params.random_seed { t!("랜덤 시드", "Random seed").to_owned() } else { tf!("시드 {}", "Seed {}", self.params.seed) };
        let summary = tf!("{seed} · {}장", "{seed} · ×{}", self.params.count);
        fold_card(ui, "seed", t!("시드 · 개수", "Seed · Count"), &summary, true, |ui| {
            const LABEL_W: f32 = 40.0;
            const ROW_H: f32 = 32.0;
            let row_label = |ui: &mut Ui, text: &str| {
                ui.allocate_ui_with_layout(vec2(LABEL_W, ROW_H), Layout::left_to_right(Align::Center), |ui| {
                    ui.set_min_size(vec2(LABEL_W, ROW_H));
                    ui.label(RichText::new(text).size(13.0).color(TEXT_DIM));
                });
            };

            // 시드: [랜덤 | 고정] [seed number ............] [🎲]
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 6.0;
                row_label(ui, t!("시드", "Seed"));
                let mut fixed = usize::from(!self.params.random_seed);
                ui.allocate_ui(vec2(132.0, ROW_H), |ui| {
                    if segmented(ui, &mut fixed, &[t!("랜덤", "Random"), t!("고정", "Fixed")]) {
                        self.params.random_seed = fixed == 0;
                    }
                })
                .response
                .on_hover_text(t!("랜덤: 생성할 때마다 다른 시드를 사용합니다.\n고정: 같은 시드로 같은 결과를 재현합니다.", "Random: a different seed for each generation.\nFixed: the same seed reproduces the same result."));
                let field_w = (ui.available_width() - ROW_H - 6.0).max(60.0);
                ui.add_enabled_ui(!self.params.random_seed, |ui| {
                    ui.add_sized(
                        vec2(field_w, ROW_H),
                        egui::DragValue::new(&mut self.params.seed).range(0..=u32::MAX as u64).speed(1.0),
                    )
                    .on_hover_text(t!("드래그하거나 더블클릭해서 숫자를 입력하세요", "Drag, or double-click to type a number"));
                });
                if ui
                    .add_sized(vec2(ROW_H, ROW_H), egui::Button::new(RichText::new("🎲").size(15.0)))
                    .on_hover_text(t!("새 시드를 뽑아 고정합니다", "Pick a new seed and fix it"))
                    .clicked()
                {
                    self.params.seed = rand::random::<u32>() as u64;
                    self.params.random_seed = false;
                }
            });
            ui.add_space(4.0);

            // 개수: [1][2]…[8]
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 6.0;
                row_label(ui, t!("개수", "Count"));
                const COUNTS: [&str; 8] = ["1", "2", "3", "4", "5", "6", "7", "8"];
                let mut idx = (self.params.count.clamp(1, 8) - 1) as usize;
                if segmented(ui, &mut idx, &COUNTS) {
                    self.params.count = idx as u32 + 1;
                }
            });
        });
    }

    fn advanced_card(&mut self, ui: &mut Ui) {
        let steps = if self.params.step_mode == StepMode::Turbo { t!("Turbo 8단계", "Turbo 8 steps").to_owned() } else { tf!("{}단계", "{} steps", self.params.steps) };
        let summary = format!("{steps} · CFG {:.1}", self.params.cfg);
        fold_card(ui, "advanced", t!("고급 설정", "Advanced settings"), &summary, false, |ui| {
            {
                    ui.label(RichText::new(t!("샘플링 단계", "Sampling steps")).size(13.0));
                    let mut mode = if self.params.step_mode == StepMode::Turbo { 0 } else { 1 };
                    if segmented(ui, &mut mode, &[t!("Turbo 8단계 (권장)", "Turbo 8 steps (recommended)"), t!("직접 지정", "Custom")]) {
                        self.params.step_mode = if mode == 0 { StepMode::Turbo } else { StepMode::Custom };
                    }
                    if self.params.step_mode == StepMode::Custom {
                        ui.horizontal(|ui| {
                            ui.add(egui::Slider::new(&mut self.params.steps, 4..=40).text(t!("단계", "Steps")));
                            if reset_button(ui, true, t!("공식 권장값 Turbo 8단계로 되돌리기", "Reset to the official Turbo 8 steps")) {
                                self.params.step_mode = StepMode::Turbo;
                                self.params.steps = GenParams::default().steps;
                            }
                        });
                    }
                    ui.add_space(4.0);
                    ui.horizontal(|ui| {
                        ui.label("CFG");
                        ui.add(egui::Slider::new(&mut self.params.cfg, 1.0..=8.0).step_by(0.5));
                        let rec = GenParams::default().cfg;
                        if reset_button(ui, (self.params.cfg - rec).abs() > 1e-3, t!("공식 권장값 CFG 1.0으로 되돌리기 (Turbo)", "Reset to the official CFG 1.0 (Turbo)")) {
                            self.params.cfg = rec;
                        }
                    });
                    ui.label(
                        RichText::new(t!("Turbo는 CFG 1에 맞춰져 있습니다. 올리면 네거티브 프롬프트가 적용되지만 시간이 약 2배 걸립니다.", "Turbo is tuned for CFG 1. Raising it applies the negative prompt but takes about twice as long."))
                            .size(12.0)
                            .color(TEXT_FAINT),
                    );
                    ui.checkbox(&mut self.params.live_preview, t!("생성 중 실시간 미리보기", "Live preview while generating"));
                    ui.add_space(4.0);
                    let d = GenParams::default();
                    let off = (self.params.cfg - d.cfg).abs() > 1e-3
                        || self.params.step_mode != d.step_mode
                        || (self.params.strength - d.strength).abs() > 1e-3
                        || (self.params.inpaint_strength - d.inpaint_strength).abs() > 1e-3
                        || self.params.loras.iter().any(|l| (l.weight - 1.0).abs() > 1e-3)
                        || self.params.live_preview != d.live_preview;
                    let resp = ui
                        .add_enabled(off, egui::Button::new(RichText::new(if off { t!("↺ 모두 권장값으로", "↺ All recommended") } else { os_t!("✓ 모두 권장값입니다", "✓ All recommended", "✔ 모두 권장값입니다", "✔ All recommended") }).size(13.0)))
                        .on_hover_text(
                            "Turbo 8단계 · CFG 1.0 (공식 권장)\n변형 강도 0.60 · 수정 강도 0.85 · LoRA 강도 1.00 · 실시간 미리보기 켬\n\
                             프롬프트·크기·시드는 그대로 둡니다.",
                        )
                        .on_disabled_hover_text(t!("모든 값이 권장값입니다.", "Every value is at its recommended setting."));
                    if resp.clicked() {
                        self.params.cfg = d.cfg;
                        self.params.step_mode = d.step_mode;
                        self.params.steps = d.steps;
                        self.params.strength = d.strength;
                        self.params.inpaint_strength = d.inpaint_strength;
                        self.params.live_preview = d.live_preview;
                        for l in &mut self.params.loras {
                            l.weight = 1.0;
                        }
                        self.toast(t!("권장값으로 되돌렸습니다", "Reset to recommended values"), ACCENT_HOVER);
                    }
            }
        });
    }

    fn action_area(&mut self, ui: &mut Ui) {
        let mut switch_to = None;
        ui.horizontal(|ui| {
            let tip = "Spectrum 단계 캐시: 8단계 중 일부를 예측으로 대신해 약 23% 빨라집니다.\n\
                       결과가 아주 조금 달라질 수 있습니다(눈으로는 거의 구분되지 않음).";
            if self.params.mode == GenMode::Outpaint {
                ui.label(RichText::new(t!("확장: 정밀 8단계 (빠른 모드 사용 안 함)", "Expand: precise 8 steps (no fast mode)")).size(12.5).color(TEXT_FAINT));
            } else {
            switch(ui, &mut self.params.fast_mode).on_hover_text(tip);
            let on = self.params.fast_mode;
            let label = ui
                .add(
                    egui::Label::new(RichText::new(t!("⚡ 빠른 모드", "⚡ Fast mode")).size(13.5).color(if on { Color32::WHITE } else { TEXT_DIM }))
                        .sense(Sense::click()),
                )
                .on_hover_text(tip);
            if label.clicked() {
                self.params.fast_mode = !self.params.fast_mode;
            }
            let on = self.params.fast_mode;
            ui.label(RichText::new(if on { t!("켜짐", "On") } else { t!("꺼짐", "Off") }).size(12.0).color(if on { ACCENT_HOVER } else { TEXT_FAINT }));
            }

            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                let current = self.settings.variant;
                ui.menu_button(RichText::new(tf!("모델: {} {}", "Model: {} {}", self.settings.variant_label(), if cfg!(windows) { "▾" } else { "⏷" })).size(12.0), |ui| {
                    ui.set_min_width(300.0);
                    for (v, title, desc) in [
                        (ModelVariant::Q4, t!("표준 · Q4_K_M", "Standard · Q4_K_M"), t!("RAM을 적게 씀 · 장당 약 19초", "Less RAM · about 19 s per image")),
                        (ModelVariant::Int8, t!("고화질·고속 · INT8 텐서코어", "High quality · fast · INT8 tensor core"), t!("반복 생성 장당 약 12.5초 · 원본에 더 가까운 화질", "Repeat generations about 12.5 s each · closer to the original quality")),
                    ] {
                        let mark = if v == current { "✔ " } else { "    " };
                        let r = ui.add(
                            egui::Button::new(
                                RichText::new(format!("{mark}{title}\n      {desc}")).size(13.0),
                            )
                            .frame(false),
                        );
                        if r.clicked() {
                            switch_to = Some(v);
                            ui.close();
                        }
                    }
                })
                .response
                .on_hover_text(t!("생성 모델 바꾸기 (엔진이 다시 시작됩니다)", "Switch generation model (restarts the engine)"));
            });
        });
        if let Some(v) = switch_to {
            self.switch_variant(ui.ctx(), v);
        }
        let busy = self.active.is_some() || !self.queue.is_empty();
        let ready = self.engine_usable();
        let label = if busy { t!("＋  대기열에 추가", "＋  Add to queue") } else { t!("✨  이미지 생성", "✨  Generate") };
        let n = self.params.count;
        let prompts = if self.params.batch && self.params.mode != GenMode::Outpaint {
            self.params.prompt.lines().filter(|l| !l.trim().is_empty()).count().max(1)
        } else {
            1
        };
        let label = match (prompts, n) {
            (1, 1) => label.to_owned(),
            (1, n) => tf!("{label} ({n}장)", "{label} ({n} images)"),
            (p, n) => tf!("{label} ({p}개 × {n}장)", "{label} ({p} × {n} images)"),
        };
        // When the engine can't take jobs, the button says why instead.
        let label = match &self.engine_state {
            EngineState::Stopped => t!("엔진이 꺼져 있습니다 · 상단 상태 표시를 눌러 시작", "Engine is off · click the status at the top to start").to_owned(),
            EngineState::Error(_) => t!("엔진 오류 · 상단 상태 표시를 눌러 재시작", "Engine error · click the status at the top to restart").to_owned(),
            _ => label,
        };
        if primary_button(ui, &label, 46.0, ready).on_hover_text("Ctrl + Enter").clicked() {
            self.generate();
        }
        ui.horizontal(|ui| {
            let pending = self.queue.len() + usize::from(self.active.is_some());
            if pending > 0 {
                ui.label(RichText::new(tf!("진행 {pending}건", "{pending} in progress")).size(12.5).color(TEXT_DIM));
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if ui.small_button(t!("■ 모두 취소", "■ Cancel all")).clicked() {
                        self.cancel_all();
                    }
                });
            } else {
                // What the button will make, so folded cards stay visible at a glance.
                let (w, h) = self.out_dims();
                let seed = if self.params.random_seed { t!("랜덤", "Random").to_owned() } else { tf!("시드 {}", "Seed {}", self.params.seed) };
                let loras = self.params.loras.iter().filter(|l| l.enabled && self.lora_files.contains(&l.file)).count();
                let mut parts = vec![format!("{w}×{h}"), seed];
                if loras > 0 {
                    parts.push(format!("LoRA {loras}"));
                }
                if let Some(avg) = self.avg_secs() {
                    parts.push(tf!("약 {avg:.0}초/장", "about {avg:.0} s/image"));
                }
                ui.label(RichText::new(parts.join(" · ")).size(12.0).color(TEXT_FAINT));
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.label(RichText::new("Ctrl+Enter").size(12.0).color(TEXT_FAINT).monospace());
                });
            }
        });
    }

    fn gallery(&mut self, ui: &mut Ui) {
        ui.add_space(12.0);
        ui.horizontal(|ui| {
            ui.add_space(14.0);
            ui.label(bold(t!("갤러리", "Gallery"), 15.0));
            ui.label(RichText::new(self.history.entries.len().to_string()).size(12.5).color(TEXT_FAINT));
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.add_space(8.0);
                // One toggle: the icon shows the size it switches to.
                let large = self.params.gallery_large;
                let (icon, tip) = if large { (Icon::GridSmall, t!("작은 썸네일로 보기", "Show small thumbnails")) } else { (Icon::GridLarge, t!("큰 썸네일로 보기", "Show large thumbnails")) };
                if bar_button(ui, icon, None, tip, BarKind::Normal).clicked() {
                    self.params.gallery_large = !large;
                }
            });
        });
        ui.horizontal(|ui| {
            ui.add_space(14.0);
            ui.add(
                TextEdit::singleline(&mut self.gallery_filter)
                    .hint_text(t!("🔍 프롬프트 검색", "🔍 Search prompts"))
                    .desired_width(ui.available_width() - 14.0),
            );
        });
        ui.add_space(4.0);

        let filter = self.gallery_filter.to_lowercase();
        // Indices into history (lowercasing only happens while a search is typed).
        let idx: Vec<usize> = (0..self.history.entries.len())
            .filter(|&i| filter.is_empty() || self.history.entries[i].prompt.to_lowercase().contains(&filter))
            .collect();
        let pending_n = usize::from(self.active.is_some()) + self.queue.len();
        let total = pending_n + idx.len();
        if total == 0 {
            ui.add_space(30.0);
            ui.vertical_centered(|ui| {
                ui.label(RichText::new("🖼").size(30.0).color(TEXT_FAINT));
                ui.label(RichText::new(if filter.is_empty() { t!("아직 생성한 이미지가 없습니다", "No images yet") } else { t!("검색 결과 없음", "No results") }).color(TEXT_FAINT));
            });
            return;
        }

        let gap = 8.0;
        let pad = 14.0;
        let avail = ui.available_width() - pad * 2.0;
        let min_cell = if self.params.gallery_large { 220.0 } else { 110.0 };
        let cols = ((avail + gap) / (min_cell + gap)).floor().max(1.0) as usize;
        let cell = (avail - gap * (cols as f32 - 1.0)) / cols as f32;
        let rows = total.div_ceil(cols);
        let mut actions: Vec<(u64, GalleryAction)> = Vec::new();
        // Only the rows in view are laid out and painted.
        ScrollArea::vertical().auto_shrink(false).show_rows(ui, cell + gap, rows, |ui, range| {
            ui.spacing_mut().item_spacing.y = gap;
            for row in range {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = gap;
                    ui.add_space(pad - gap);
                    for i in row * cols..((row + 1) * cols).min(total) {
                        if i < pending_n {
                            // In-flight jobs first, as placeholders.
                            pending_cell(ui, cell, if i == 0 { self.active.as_ref() } else { None });
                        } else {
                            let e = &self.history.entries[idx[i - pending_n]];
                            if let Some(a) = self.thumb_cell(ui, cell, e) {
                                actions.push((e.id, a));
                            }
                        }
                    }
                });
            }
        });
        for (id, a) in actions {
            self.gallery_action(ui.ctx(), id, a);
        }
    }

    fn thumb_cell(&self, ui: &mut Ui, cell: f32, entry: &HistoryEntry) -> Option<GalleryAction> {
        let id = entry.id;
        let (rect, resp) = ui.allocate_exact_size(vec2(cell, cell), Sense::click());
        let selected = self.selected == Some(id);
        if ui.is_rect_visible(rect) {
            ui.painter().rect_filled(rect, 10, CARD);
            if let Some(tex) = self.thumbs.get(&id) {
                let uv = cover_uv(tex.size_vec2());
                egui::Image::new(tex).uv(uv).corner_radius(10).paint_at(ui, rect);
            } else {
                // Paint only (see `pending_cell`): keep the row layout untouched.
                egui::Spinner::new().size(16.0).paint_at(ui, Rect::from_center_size(rect.center(), vec2(18.0, 18.0)));
            }
            if selected {
                ui.painter().rect_stroke(rect, 10, Stroke::new(2.5, ACCENT), egui::StrokeKind::Inside);
            } else if resp.hovered() {
                ui.painter().rect_stroke(rect, 10, Stroke::new(1.5, Color32::from_white_alpha(90)), egui::StrokeKind::Inside);
            }
        }
        let mut action = None;

        // Hover-only trash button in the top-right corner (moves the file to the Recycle Bin).
        if ui.rect_contains_pointer(rect) {
            let btn = Rect::from_center_size(rect.right_top() + vec2(-18.0, 18.0), vec2(26.0, 26.0));
            let br = ui
                .interact(btn, ui.scope_id().with(("thumb_trash", id)), Sense::click())
                .on_hover_cursor(CursorIcon::PointingHand)
                .on_hover_text(t!("휴지통으로 이동 (Delete 키)", "Move to the Recycle Bin (Delete key)"));
            let fill = if br.hovered() { Color32::from_rgb(220, 70, 70) } else { Color32::from_black_alpha(180) };
            ui.painter().circle_filled(btn.center(), 13.0, fill);
            ui.painter().text(btn.center(), Align2::CENTER_CENTER, "🗑", FontId::proportional(13.0), Color32::WHITE);
            if br.clicked() {
                return Some(GalleryAction::QuickDelete);
            }
        }

        if resp.clicked() {
            action = Some(GalleryAction::Select);
        }
        if resp.double_clicked() {
            action = Some(GalleryAction::View);
        }
        let resp = resp.on_hover_ui(|ui| {
            ui.set_max_width(320.0);
            ui.label(truncate(&entry.prompt, 200));
            ui.label(
                RichText::new(tf!("{}{}×{} · 시드 {} · {:.1}초 · {}", "{}{}×{} · seed {} · {:.1} s · {}", mode_tag(entry), entry.width, entry.height, entry.seed, entry.elapsed, entry.created))
                    .size(12.0)
                    .color(TEXT_DIM),
            );
        });
        resp.context_menu(|ui| {
            if ui.button(t!("♻ 이 설정 불러오기", "♻ Load these settings")).clicked() {
                action = Some(GalleryAction::Reuse);
                ui.close();
            }
            if ui.button(t!("🖼 입력 이미지로 사용", "🖼 Use as input image")).clicked() {
                action = Some(GalleryAction::UseInput);
                ui.close();
            }
            if ui.button(t!("🔍 크게 보기", "🔍 Full view")).clicked() {
                action = Some(GalleryAction::View);
                ui.close();
            }
            if ui.button(t!("📋 이미지 복사", "📋 Copy image")).clicked() {
                action = Some(GalleryAction::Copy);
                ui.close();
            }
            if ui.button(t!("💾 다른 이름으로 저장", "💾 Save as")).clicked() {
                action = Some(GalleryAction::SaveAs);
                ui.close();
            }
            if ui.button(t!("📂 폴더에서 보기", "📂 Show in folder")).clicked() {
                action = Some(GalleryAction::Reveal);
                ui.close();
            }
            ui.separator();
            if ui.button(RichText::new(t!("🗑 삭제", "🗑 Delete")).color(RED)).clicked() {
                action = Some(GalleryAction::Delete);
                ui.close();
            }
        });
        action
    }

    fn gallery_action(&mut self, _ctx: &Context, id: u64, a: GalleryAction) {
        let Some(entry) = self.history.get(id).cloned() else { return };
        match a {
            GalleryAction::Select => self.selected = Some(id),
            GalleryAction::View => {
                self.selected = Some(id);
                self.viewer = true;
                self.zoom = 1.0;
                self.pan = Vec2::ZERO;
            }
            GalleryAction::Reuse => {
                self.apply_entry(&entry);
                self.toast(t!("설정을 불러왔습니다 (시드 고정)", "Settings loaded (seed fixed)"), ACCENT_HOVER);
            }
            GalleryAction::Copy => self.copy_image(&entry.file),
            GalleryAction::SaveAs => self.save_as(&entry.file),
            GalleryAction::Reveal => Self::reveal(&entry.file),
            GalleryAction::Delete => self.confirm_delete = Some(id),
            GalleryAction::UseInput => self.use_as_input(_ctx, &entry.file),
            GalleryAction::QuickDelete => self.delete_entry(id),
        }
    }

    fn canvas(&mut self, ui: &mut Ui) {
        if self.dl_setup || !self.settings.core_ready() {
            self.onboarding(ui);
            self.awaiting_engine = !self.dl_setup && !self.settings.runtime_ok();
            return;
        }
        if std::mem::take(&mut self.awaiting_engine) && matches!(self.engine_state, EngineState::Stopped) {
            self.start_engine_if_possible();
        }

        // Partial edit: brush the area to repaint over the input image.
        if self.params.mode == GenMode::Inpaint && self.mask_editing && self.active.is_none() && !self.inputs.is_empty() {
            self.mask_canvas(ui);
            return;
        }
        // Expand: show where the new canvas area goes.
        if self.params.mode == GenMode::Outpaint && self.mask_editing && self.active.is_none() && !self.inputs.is_empty() {
            self.outpaint_preview(ui);
            return;
        }

        let selected = self.selected.and_then(|id| self.history.get(id).cloned());

        // Toolbar for the selected image.
        if let (Some(e), None) = (&selected, &self.active) {
            Frame::new().inner_margin(Margin { left: 18, right: 18, top: 14, bottom: 0 }).show(ui, |ui| {
                ui.vertical(|ui| {
                    ui.vertical(|ui| {
                        ui.add(egui::Label::new(RichText::new(&e.prompt).color(TEXT)).truncate());
                        ui.add(
                            egui::Label::new(
                                RichText::new(tf!("{}{}×{} · 시드 {} · {:.1}초 · {}", "{}{}×{} · seed {} · {:.1} s · {}", mode_tag(e), e.width, e.height, e.seed, e.elapsed, e.created))
                                    .size(12.0)
                                    .color(TEXT_FAINT),
                            )
                            .truncate(),
                        );
                    });
                    ui.add_space(10.0);
                    #[derive(Clone, Copy)]
                    enum Act {
                        Load,
                        Input,
                        Harmonize,
                        Mask,
                        Zoom,
                        Copy,
                        Save,
                        Reveal,
                        Delete,
                    }
                    let show_mask = self.params.mode == GenMode::Inpaint && !self.inputs.is_empty();
                    // Narrow window: drop the labels and keep the icons (tooltips still explain).
                    let compact = ui.available_width() < if show_mask { 700.0 } else { 590.0 };
                    let lbl = |t: &'static str| if compact { None } else { Some(t) };
                    let mut left_act = None;
                    let mut right_act = None;
                    let mut expand: Option<Option<(f32, f32)>> = None;
                    egui::Sides::new().show(
                        ui,
                        |ui| {
                            bar_group(ui, |ui| {
                                if bar_button(ui, Icon::Reload, lbl(t!("불러오기", "Load")), t!("프롬프트·시드·크기·모드를 왼쪽에 적용", "Apply prompt · seed · size · mode to the left panel"), BarKind::Normal)
                                    .clicked()
                                {
                                    left_act = Some(Act::Load);
                                }
                                // One-click expand: picks the direction, keeps this image's prompt, generates.
                                let r = bar_button(ui, Icon::Expand, lbl(t!("확장", "Expand")), t!("이 이미지를 넓혀서 바로 생성", "Widen this image and generate right away"), BarKind::Menu);
                                egui::Popup::menu(&r).width(220.0).show(|ui| {
                                    ui.label(RichText::new(t!("이 이미지를 넓혀서 바로 생성", "Widen this image and generate right away")).size(12.0).color(TEXT_DIM));
                                    for (label, aspect) in [
                                        (t!("↔ 좌우로 넓게 (16:9)", "↔ Wider (16:9)"), Some((16.0, 9.0))),
                                        (t!("↕ 위아래로 길게 (9:16)", "↕ Taller (9:16)"), Some((9.0, 16.0))),
                                        (t!("⬜ 정사각형으로 (1:1)", "⬜ To square (1:1)"), Some((1.0, 1.0))),
                                        (os_t!("▭ 4:3으로", "▭ To 4:3", "⬜ 4:3으로", "⬜ To 4:3"), Some((4.0, 3.0))),
                                        (os_t!("⤢ 줌 아웃 (사방 1.5배)", "⤢ Zoom out (1.5× all sides)", "⛶ 줌 아웃 (사방 1.5배)", "⛶ Zoom out (1.5× all sides)"), None),
                                    ] {
                                        if ui.button(label).clicked() {
                                            expand = Some(aspect);
                                            ui.close();
                                        }
                                    }
                                    ui.separator();
                                    if ui.button(t!("⚙ 직접 조절하기…", "⚙ Adjust manually…")).on_hover_text(t!("확장 모드에서 방향별로 조절", "Adjust each side in Expand mode")).clicked() {
                                        expand = Some(Some((-1.0, -1.0)));
                                        ui.close();
                                    }
                                });
                                if bar_button(
                                    ui,
                                    Icon::Sparkle,
                                    lbl(t!("자연스럽게", "Blend in")),
                                    t!("합성 티가 나는 휴대폰 케이스를 장면의 조명·반사·음영에 맞춰 자연스럽게 다듬습니다.\n직전 편집의 케이스 이미지(2번)를 함께 써서 디자인을 유지합니다. 누르면 바로 생성합니다.", "Re-lights a pasted-looking phone case to match the scene\'s lighting, reflections and shading.\nUses the case image (no. 2) from the previous edit to keep the design. Generates immediately."),
                                    BarKind::Normal,
                                )
                                .clicked()
                                {
                                    left_act = Some(Act::Harmonize);
                                }
                                if bar_button(ui, Icon::Image, lbl(t!("입력으로", "Use as input")), t!("이 이미지를 변형·편집의 입력으로 사용", "Use this image as the input for variation/edit"), BarKind::Normal)
                                    .clicked()
                                {
                                    left_act = Some(Act::Input);
                                }
                                if show_mask
                                    && bar_button(ui, Icon::Brush, lbl(t!("마스크 편집", "Edit mask")), t!("브러시 화면으로 돌아가기", "Back to the brush view"), BarKind::Normal)
                                        .clicked()
                                {
                                    left_act = Some(Act::Mask);
                                }
                            });
                        },
                        |ui| {
                            // Right-to-left: the first button added sits at the far right.
                            bar_group(ui, |ui| {
                                if bar_button(ui, Icon::Trash, None, t!("삭제 (Del)", "Delete (Del)"), BarKind::Danger).clicked() {
                                    right_act = Some(Act::Delete);
                                }
                                bar_divider(ui);
                                if bar_button(ui, Icon::Folder, None, t!("폴더에서 보기", "Show in folder"), BarKind::Normal).clicked() {
                                    right_act = Some(Act::Reveal);
                                }
                                if bar_button(ui, Icon::Save, None, t!("다른 이름으로 저장 (Ctrl+S)", "Save as (Ctrl+S)"), BarKind::Normal).clicked() {
                                    right_act = Some(Act::Save);
                                }
                                if bar_button(ui, Icon::Copy, None, t!("클립보드에 복사 (Ctrl+Shift+C)", "Copy to clipboard (Ctrl+Shift+C)"), BarKind::Normal).clicked() {
                                    right_act = Some(Act::Copy);
                                }
                                if bar_button(ui, Icon::Zoom, None, t!("크게 보기 (F)", "Full view (F)"), BarKind::Normal).clicked() {
                                    right_act = Some(Act::Zoom);
                                }
                            });
                        },
                    );
                    let ctx = ui.ctx().clone();
                    for act in [left_act, right_act].into_iter().flatten() {
                        match act {
                            Act::Load => {
                                self.apply_entry(e);
                                self.toast(t!("설정을 불러왔습니다 (시드 고정)", "Settings loaded (seed fixed)"), ACCENT_HOVER);
                            }
                            Act::Input => self.use_as_input(&ctx, &e.file),
                            Act::Harmonize => self.start_harmonize(&ctx, &e.file),
                            Act::Mask => self.mask_editing = true,
                            Act::Zoom => {
                                self.viewer = true;
                                self.zoom = 1.0;
                                self.pan = Vec2::ZERO;
                            }
                            Act::Copy => self.copy_image(&e.file),
                            Act::Save => self.save_as(&e.file),
                            Act::Reveal => Self::reveal(&e.file),
                            Act::Delete => self.confirm_delete = Some(e.id),
                        }
                    }
                    if let Some(aspect) = expand {
                        if aspect == Some((-1.0, -1.0)) {
                            self.params.mode = GenMode::Outpaint;
                            self.inputs.clear();
                            self.use_as_input(&ctx, &e.file);
                            self.params.prompt = self.outpaint_prompt(Some(e));
                        } else {
                            self.quick_outpaint(&ctx, e, aspect);
                        }
                    }
                });
            });
        }

        let area = ui.available_rect_before_wrap().shrink(18.0);
        ui.allocate_rect(area, Sense::hover());

        if let Some(a) = &self.active {
            let tex = a.preview.as_ref();
            let (w, h) = self.out_dims();
            let img_rect = match tex {
                Some(t) => fit(Rect::from_min_max(area.min, area.max - vec2(0.0, 76.0)), t.size_vec2()),
                None => fit(Rect::from_min_max(area.min, area.max - vec2(0.0, 76.0)), vec2(w as f32, h as f32)),
            };
            let p = ui.painter();
            p.rect_filled(img_rect, 14, CARD);
            if let Some(t) = tex {
                egui::Image::new(t).corner_radius(14).paint_at(ui, img_rect);
            } else if let (true, Some(input)) = (a.mode != GenMode::Text, self.inputs.first()) {
                let r = fit(img_rect, input.tex.size_vec2());
                egui::Image::new(&input.tex).corner_radius(14).tint(Color32::from_white_alpha(70)).paint_at(ui, r);
                shimmer(ui, img_rect);
            } else {
                shimmer(ui, img_rect);
            }
            self.progress_overlay(ui, img_rect);
            return;
        }

        match (&selected, &self.full) {
            (Some(e), Some((fid, tex))) if *fid == e.id => {
                let r = fit(area, tex.size_vec2());
                let resp = ui.interact(r, Id::unique("canvas_img"), Sense::click());
                // soft shadow
                ui.painter().rect_filled(r.expand(6.0), 18, Color32::from_black_alpha(60));
                egui::Image::new(tex).corner_radius(14).paint_at(ui, r);
                if resp.on_hover_cursor(CursorIcon::ZoomIn).on_hover_text(t!("더블클릭: 크게 보기", "Double-click: full view")).double_clicked() {
                    self.viewer = true;
                    self.zoom = 1.0;
                    self.pan = Vec2::ZERO;
                }
            }
            (Some(e), _) if self.full_failed == Some(e.id) => {
                ui.put(
                    area,
                    egui::Label::new(RichText::new(format!("이미지를 열 수 없습니다
{}", e.file.display())).color(TEXT_DIM)),
                );
            }
            (Some(_), _) => {
                ui.put(area, egui::Spinner::new().size(28.0));
            }
            (None, _) => self.empty_state(ui, area),
        }
    }

    fn progress_overlay(&self, ui: &mut Ui, img_rect: Rect) {
        let Some(a) = &self.active else { return };
        let elapsed = a.started.map(|s| s.elapsed().as_secs_f32()).unwrap_or(0.0);
        let (phase, frac) = if let Some((k, n)) = a.pass.filter(|(_, n)| *n > 1) {
            // Multi-pass expand: progress across all passes.
            let within = if a.total > 0 { a.step as f32 / a.total as f32 } else { 0.0 };
            (tf!("확장 {k}/{n}단계 · 샘플링 {} / {}", "Expand pass {k}/{n} · sampling {} / {}", a.step, a.total), ((k - 1) as f32 + within) / n as f32)
        } else if a.total == 0 {
            (
                if a.mode == GenMode::Edit { t!("참조 이미지·지시문 분석 중…", "Analyzing reference images and instruction…") } else { t!("프롬프트 인코딩 중…", "Encoding prompt…") }.to_owned(),
                0.04,
            )
        } else if a.step >= a.total {
            (t!("이미지 디코딩 중…", "Decoding image…").to_owned(), 0.97)
        } else {
            (tf!("샘플링 {} / {}", "Sampling {} / {}", a.step, a.total), 0.08 + 0.88 * a.step as f32 / a.total as f32)
        };
        let eta = self.avg_secs_for(a.mode).map(|avg| (avg - elapsed).max(0.0));

        let bar_w = img_rect.width().min(460.0).max(280.0);
        let bar_h = if a.note.is_some() { 74.0 } else { 56.0 };
        let bar = Rect::from_center_size(Pos2::new(img_rect.center().x, img_rect.bottom() + 42.0), vec2(bar_w, bar_h));
        ui.scope_builder(egui::UiBuilder::new().max_rect(bar), |ui| {
            Frame::new()
                .fill(Color32::from_rgba_unmultiplied(20, 21, 27, 235))
                .stroke(Stroke::new(1.0, BORDER))
                .corner_radius(12)
                .inner_margin(Margin::symmetric(14, 8))
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.spinner();
                        ui.label(RichText::new(phase).size(13.0));
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            let mut t = tf!("{elapsed:.0}초", "{elapsed:.0} s");
                            if let Some(eta) = eta.filter(|e| *e >= 2.0) {
                                t = tf!("{t} · 남은 시간 약 {eta:.0}초", "{t} · about {eta:.0} s left");
                            }
                            ui.label(RichText::new(t).size(12.0).color(TEXT_DIM));
                            if ui.small_button(t!("취소", "Cancel")).clicked() {
                                self.engine.cancel(a.id);
                            }
                        });
                    });
                    // A pass change or redraw restarts the step count: say so, so a bar that
                    // runs full and starts over isn't read as "finished".
                    if let Some(n) = &a.note {
                        ui.add(egui::Label::new(RichText::new(format!("↻ {n}")).size(12.0).color(ACCENT_HOVER)).truncate());
                    }
                    ui.add(egui::ProgressBar::new(frac).desired_height(5.0).fill(ACCENT).animate(false));
                });
        });
        let _ = a.seed;
        let _ = &a.prompt;
        ui.ctx().request_repaint_after(std::time::Duration::from_millis(200));
    }

    fn empty_state(&mut self, ui: &mut Ui, area: Rect) {
        let r = Rect::from_center_size(area.center(), vec2(area.width().min(520.0), 470.0));
        ui.scope_builder(egui::UiBuilder::new().max_rect(r), |ui| {
            ui.vertical_centered(|ui| {
                ui.label(RichText::new("✨").size(44.0));
                ui.label(bold(t!("무엇을 그려볼까요?", "What shall we draw?"), 24.0));
                ui.add_space(4.0);
                ui.label(RichText::new(t!("왼쪽에 프롬프트를 입력하고 이미지 생성을 누르세요.", "Type a prompt on the left and press Generate.")).color(TEXT_DIM));
                ui.add_space(14.0);
                self.sample_card(ui);
                ui.add_space(12.0);
                ui.label(RichText::new(t!("예시로 시작하기", "Start from an example")).size(12.5).color(TEXT_FAINT));
                for ex in lang(&EXAMPLES[..], &EXAMPLES_EN[..]).iter().take(3) {
                    if ui
                        .add(egui::Button::new(RichText::new(truncate(ex, 64)).size(13.0)).corner_radius(18).min_size(vec2(0.0, 32.0)))
                        .on_hover_text(*ex)
                        .clicked()
                    {
                        self.params.prompt = (*ex).to_owned();
                    }
                }
                ui.add_space(16.0);
                ui.label(RichText::new(t!("단축키 · Ctrl+Enter 생성 · ←/→ 이미지 이동 · F 크게 보기 · Ctrl+S 저장", "Shortcuts · Ctrl+Enter generate · ←/→ browse images · F full view · Ctrl+S save")).size(12.0).color(TEXT_FAINT));
            });
        });
    }

    /// "Try the sample" card on the start screen: model + case → swapped case.
    fn sample_card(&mut self, ui: &mut Ui) {
        if self.sample_thumbs.is_none() {
            let tex = |name: &str, bytes: &[u8]| {
                decode_sample(bytes).map(|img| {
                    let small = image::imageops::thumbnail(&img, 160, 160 * img.height() / img.width().max(1));
                    ui.ctx().load_texture(name, crate::engine::rgba_to_color(&small), egui::TextureOptions::LINEAR)
                })
            };
            if let (Some(a), Some(b)) = (tex("sample_model", SAMPLE_MODEL), tex("sample_case", SAMPLE_CASE)) {
                self.sample_thumbs = Some([a, b]);
            }
        }
        let Some([model, case]) = self.sample_thumbs.clone() else { return };
        let mut load = false;
        Frame::new()
            .fill(INPUT)
            .stroke(Stroke::new(1.0, BORDER))
            .corner_radius(14)
            .inner_margin(Margin::same(12))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    let h = 84.0;
                    let img = |ui: &mut Ui, t: &egui::TextureHandle| {
                        let s = t.size_vec2();
                        ui.add(egui::Image::new(t).fit_to_exact_size(vec2(h * s.x / s.y, h)).corner_radius(8));
                    };
                    img(ui, &model);
                    ui.label(RichText::new("+").size(18.0).color(TEXT_FAINT));
                    img(ui, &case);
                    ui.add_space(6.0);
                    ui.vertical(|ui| {
                        ui.label(bold(t!("샘플로 체험하기 · 폰케이스 교체", "Try the sample · swap a phone case"), 14.0));
                        ui.label(
                            RichText::new(t!("모델이 든 휴대폰의 케이스를 오른쪽 디자인으로 바꿉니다 (편집 모드).", "Puts the case design on the right onto the phone the model holds (Edit mode)."))
                                .size(12.0)
                                .color(TEXT_DIM),
                        );
                        ui.horizontal(|ui| {
                            load = ui.button(t!("샘플 불러오기", "Load sample")).clicked();
                            ui.hyperlink_to(
                                RichText::new(t!("케이스 디자인 · AKAN", "Case design · AKAN")).size(11.5).color(TEXT_FAINT),
                                "https://akan.co.kr",
                            );
                        });
                    });
                });
            });
        if load {
            let ctx = ui.ctx().clone();
            self.load_sample(&ctx);
        }
    }

    /// First-run screen: downloads the engine, the CUDA runtime, the models and the extras
    /// (vision module, recommended LoRAs, photo describer) on its own, with one progress view.
    fn onboarding(&mut self, ui: &mut Ui) {
        let items = self.settings.setup_items();
        let total: u64 = items.iter().map(|(s, _)| s.approx_bytes).sum();
        let have: u64 = items.iter().filter(|(s, _)| self.settings.installed(s)).map(|(s, _)| s.approx_bytes).sum();
        let remaining = total.saturating_sub(have);
        let base = crate::config::app_base_dir();
        let free = crate::config::free_bytes(&self.settings.models_dir());
        // About 1 GB of headroom: a zip sits next to its extracted files for a moment.
        let space_ok = free.is_none_or(|f| f >= remaining + 1_000_000_000);
        // Self-test windows never download unless pointed at a local mirror.
        let auto_ok = std::env::var_os("QIS_SHOT_DIR").is_none() || std::env::var_os("QIS_DL_MIRROR").is_some();
        if !self.dl.running && !self.setup_stopped && self.setup_errors.is_empty() && space_ok && auto_ok && remaining > 0 {
            let ctx = ui.ctx().clone();
            self.start_setup(&ctx);
        }
        let current = self.dl.current.filter(|_| self.dl_setup);
        let current_done = current
            .and_then(|f| items.iter().find(|(s, _)| s.file == f && !self.settings.installed(s)))
            .map(|(s, _)| if self.dl.extracting { s.approx_bytes } else { self.dl.done.min(s.approx_bytes) })
            .unwrap_or(0);

        let area = ui.available_rect_before_wrap();
        let r = Rect::from_center_size(area.center(), vec2(area.width().min(660.0) - 40.0, (area.height() - 40.0).min(720.0)));
        ui.scope_builder(egui::UiBuilder::new().max_rect(r), |ui| {
            card().inner_margin(Margin::same(24)).show(ui, |ui| {
                ui.label(bold(t!("처음 실행 준비", "First-run setup"), 22.0));
                ui.label(
                    RichText::new(if self.dl.running {
                        t!("필요한 파일을 자동으로 받고 있습니다. 창을 닫아도 다음 실행 때 이어받습니다.", "Downloading the files the app needs. If you close the window, it resumes next time.")
                    } else {
                        t!("이미지 생성에 필요한 엔진과 모델을 받습니다. 한 번만 받으면 됩니다.", "The app needs its engine and models. This happens only once.")
                    })
                    .color(TEXT_DIM),
                );
                ui.add_space(10.0);

                let frac = if total > 0 { (have + current_done) as f32 / total as f32 } else { 1.0 };
                ui.add(egui::ProgressBar::new(frac).desired_height(8.0).fill(ACCENT));
                ui.label(
                    RichText::new(format!("{} / {}  ·  {:.0}%", size_text(have + current_done), size_text(total), frac * 100.0))
                        .size(12.0)
                        .color(TEXT_DIM),
                );
                if !crate::config::nvidia_driver_present() {
                    ui.add_space(4.0);
                    ui.horizontal_wrapped(|ui| {
                        ui.label(
                            RichText::new(t!(
                                "NVIDIA 그래픽 드라이버가 보이지 않습니다. 이미지 생성에는 NVIDIA 그래픽카드(VRAM 8GB 이상)와 최신 드라이버가 필요합니다.",
                                "No NVIDIA graphics driver found. Image generation needs an NVIDIA GPU (8 GB+ VRAM) with a current driver."
                            ))
                            .size(12.5)
                            .color(AMBER),
                        );
                        ui.hyperlink_to(RichText::new(t!("드라이버 받기", "Get the driver")).size(12.5), "https://www.nvidia.com/Download/index.aspx");
                    });
                }
                // Linux: system libraries the downloads can't bring (the CUDA runtime comes with the engine).
                #[cfg(target_os = "linux")]
                for (what, package, required) in crate::config::missing_system_parts() {
                    ui.add_space(4.0);
                    ui.label(
                        RichText::new(if *required {
                            tf!("없는 시스템 구성 요소: {} · 이미지 생성 엔진에 필요합니다. 설치: {}", "{} is missing, so the image engine can't run. Install: {}", what, package)
                        } else {
                            tf!("없는 시스템 구성 요소: {} · 사진 설명 기능에 필요합니다. 설치: {}", "{} is missing, so photo description won't work. Install: {}", what, package)
                        })
                        .size(12.5)
                        .color(AMBER),
                    );
                }
                if !space_ok {
                    ui.add_space(4.0);
                    ui.label(
                        RichText::new(tf!(
                            "디스크 공간이 부족합니다: 약 {} 더 필요한데 남은 공간은 {}입니다. 공간을 비운 뒤 다시 시도하세요.",
                            "Not enough disk space: about {} more is needed but only {} is free. Free up space and try again.",
                            size_text(remaining + 1_000_000_000),
                            size_text(free.unwrap_or(0))
                        ))
                        .size(12.5)
                        .color(RED),
                    );
                }
                ui.add_space(8.0);

                let list_h = (ui.available_height() - 120.0).max(140.0);
                egui::ScrollArea::vertical().max_height(list_h).auto_shrink([false, true]).show(ui, |ui| {
                    for (i, (src, required)) in items.iter().enumerate() {
                        if i == 0 || (!*required && items[i - 1].1) {
                            ui.add_space(if i == 0 { 0.0 } else { 6.0 });
                            ui.label(
                                RichText::new(if *required { t!("필수", "Required") } else { t!("추가 기능 · 편집 · 확장 · 사진 설명 · LoRA", "Extras · edit · expand · photo description · LoRA") })
                                    .size(12.0)
                                    .color(TEXT_FAINT),
                            );
                        }
                        let ok = self.settings.installed(src);
                        let mut detail = format!("{} · {}", src.file, size_text(src.approx_bytes));
                        if !ok && current == Some(src.file) {
                            if self.dl.extracting {
                                detail.push_str(if src.unzip { t!(" · 압축 푸는 중…", " · extracting…") } else { t!(" · 설치 중… (권한 확인 창에서 '예')", " · installing… (choose Yes in the permission prompt)") });
                            } else if self.dl.total > 0 {
                                detail.push_str(&format!(" · {:.0}%", self.dl.done as f64 / self.dl.total as f64 * 100.0));
                            }
                        } else if !ok && self.setup_errors.iter().any(|e| e.starts_with(src.file)) {
                            detail.push_str(t!(" · 실패", " · failed"));
                        }
                        check_row(ui, ok, src.name(), &detail);
                    }
                });
                ui.add_space(10.0);

                for e in &self.setup_errors {
                    ui.label(RichText::new(tf!("다운로드 오류: {}", "Download error: {}", e)).color(RED).size(12.5));
                }
                // Linux fallback: if the engine archive can't be fetched, it can be built locally.
                #[cfg(not(windows))]
                if !self.settings.runtime_ok() && self.setup_errors.iter().any(|e| e.starts_with(crate::config::ENGINE_SOURCE.file)) {
                    engine_build_help(ui, &self.settings);
                }
                if self.dl.running {
                    ui.horizontal(|ui| {
                        ui.spinner();
                        let what = current
                            .and_then(|f| items.iter().find(|(s, _)| s.file == f))
                            .map(|(s, _)| s.name())
                            .unwrap_or("");
                        ui.label(tf!("받는 중: {}", "Downloading: {}", what));
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            if ui.button(t!("중지", "Stop")).clicked() {
                                self.setup_stopped = true;
                                if let Some(c) = &self.dl.cancel {
                                    c.store(true, Ordering::Relaxed);
                                }
                            }
                        });
                    });
                } else if remaining > 0 {
                    let label = if self.setup_errors.is_empty() {
                        tf!("⬇  다운로드 시작 (약 {})", "⬇  Start download (about {})", size_text(remaining))
                    } else {
                        tf!("⟳  다시 시도 (약 {})", "⟳  Try again (about {})", size_text(remaining))
                    };
                    if primary_button(ui, &label, 44.0, space_ok).clicked() {
                        let ctx = ui.ctx().clone();
                        self.start_setup(&ctx);
                    }
                }
                ui.add_space(6.0);
                ui.label(
                    RichText::new(tf!(
                        "받는 곳: GitHub (stable-diffusion.cpp · llama.cpp), Hugging Face  ·  저장 위치: {}",
                        "From: GitHub (stable-diffusion.cpp · llama.cpp), Hugging Face  ·  Saved in: {}",
                        base.display()
                    ))
                    .size(12.0)
                    .color(TEXT_FAINT),
                );
                if !self.dl.running && ui.link(t!("이미 받은 파일이 있다면 설정에서 경로 지정하기 →", "Already have the files? Set the paths in Settings →")).clicked() {
                    self.settings_draft = Some(self.settings.clone());
                    self.show_settings = true;
                }
            });
        });
    }

    // ------------------------------------------------------------------ overlays

    fn viewer_overlay(&mut self, ctx: &Context, screen: Rect) {
        let Some((id, tex)) = self.full.clone() else {
            self.viewer = false;
            return;
        };
        if Some(id) != self.selected {
            return;
        }
        egui::Area::new(Id::unique("viewer"))
            .order(Order::Foreground)
            .fixed_pos(screen.min)
            .show(ctx, |ui| {
                let (rect, resp) = ui.allocate_exact_size(screen.size(), Sense::click_and_drag());
                ui.painter().rect_filled(rect, 0, Color32::from_black_alpha(238));
                if resp.hovered() {
                    let scroll = ui.input(|i| i.smooth_scroll_delta.y);
                    if scroll != 0.0 {
                        let old = self.zoom;
                        self.zoom = (self.zoom * (scroll * 0.0018).exp()).clamp(0.2, 12.0);
                        // zoom around the cursor
                        if let Some(pos) = ui.input(|i| i.pointer.hover_pos()) {
                            let c = rect.center() + self.pan;
                            self.pan += (pos - c) * (1.0 - self.zoom / old);
                        }
                    }
                }
                if resp.dragged() {
                    self.pan += resp.drag_delta();
                }
                if resp.double_clicked() {
                    self.zoom = 1.0;
                    self.pan = Vec2::ZERO;
                }
                let base = fit(rect.shrink(40.0), tex.size_vec2());
                let img = Rect::from_center_size(rect.center() + self.pan, base.size() * self.zoom);
                ui.painter().image(tex.id(), img, Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)), Color32::WHITE);

                let close = Rect::from_center_size(rect.right_top() + vec2(-34.0, 34.0), vec2(36.0, 36.0));
                let cr = ui.interact(close, Id::unique("viewer_close"), Sense::click());
                ui.painter().circle_filled(close.center(), 18.0, if cr.hovered() { WIDGET_HOVER } else { WIDGET });
                ui.painter().text(close.center(), Align2::CENTER_CENTER, GLYPH_CLOSE, FontId::proportional(16.0), TEXT);
                if cr.clicked() {
                    self.viewer = false;
                }
                let px = tex.size();
                ui.painter().text(
                    rect.center_bottom() + vec2(0.0, -20.0),
                    Align2::CENTER_CENTER,
                    tf!(
                        "{}×{} · {:.0}%   ·   스크롤 확대/축소 · 드래그 이동 · 더블클릭 원래대로 · ←/→ 이전/다음 · Esc 닫기", "{}×{} · {:.0}%   ·   scroll to zoom · drag to pan · double-click to reset · ←/→ previous/next · Esc to close",
                        px[0],
                        px[1],
                        self.zoom * base.width() / px[0] as f32 * 100.0
                    ),
                    FontId::proportional(12.5),
                    TEXT_DIM,
                );
            });
    }

    fn settings_window(&mut self, ctx: &Context) {
        let mut open = self.show_settings;
        let mut apply = false;
        let Some(draft) = self.settings_draft.as_mut() else {
            self.show_settings = false;
            return;
        };
        let mut want_int8_download = false;
        egui::Window::new(bold(t!("설정", "Settings"), 16.0))
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .default_width(640.0)
            .anchor(Align2::CENTER_CENTER, Vec2::ZERO)
            .show(ctx, |ui| {
                section(ui, t!("생성 모델", "Generation model"));
                ui.horizontal(|ui| {
                    let w = (ui.available_width() - 8.0) / 2.0;
                    for (variant, title, desc) in [
                        (ModelVariant::Q4, t!("표준 · Q4_K_M (4.2GB)", "Standard · Q4_K_M (4.2 GB)"), t!("RAM을 적게 씁니다.\n장당 약 18초", "Uses less RAM.\nAbout 18 s per image")),
                        (
                            ModelVariant::Int8,
                            t!("고화질·고속 · INT8 텐서코어 (7.3GB)", "High quality · fast · INT8 tensor core (7.3 GB)"),
                            t!("단계당 39% 빠르고 원본에 더 가깝습니다.\n같은 프롬프트 반복 시 장당 약 12초,\n프롬프트를 바꾸면 약 20초", "39% faster per step and closer to the original.\nAbout 12 s per image when repeating a prompt,\nabout 20 s with a new prompt"),
                        ),
                    ] {
                        let on = draft.variant == variant;
                        let resp = Frame::new()
                            .fill(if on { ACCENT_SOFT } else { INPUT })
                            .stroke(Stroke::new(1.0, if on { ACCENT } else { BORDER }))
                            .corner_radius(10)
                            .inner_margin(Margin::same(10))
                            .show(ui, |ui| {
                                ui.set_width(w - 22.0);
                                ui.label(RichText::new(title).color(if on { Color32::WHITE } else { TEXT }));
                                ui.label(RichText::new(desc).size(12.0).color(TEXT_DIM));
                            })
                            .response
                            .interact(Sense::click())
                            .on_hover_cursor(CursorIcon::PointingHand);
                        if resp.clicked() {
                            draft.variant = variant;
                        }
                    }
                });
                if draft.variant == ModelVariant::Int8 && !draft.int8_model.is_file() {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(t!("INT8 모델 파일이 없습니다.", "INT8 model file not found.")).size(12.5).color(AMBER));
                        if self.dl.running {
                            ui.spinner();
                            let pct = if self.dl.total > 0 { self.dl.done as f32 / self.dl.total as f32 } else { 0.0 };
                            ui.add(egui::ProgressBar::new(pct).desired_width(180.0).show_percentage());
                        } else if ui.button(t!("⬇ 다운로드 (7.3GB)", "⬇ Download (7.3 GB)")).clicked() {
                            want_int8_download = true;
                        }
                    });
                }
                ui.add_space(8.0);
                section(ui, t!("파일 경로", "File paths"));
                egui::Grid::new("paths").num_columns(3).spacing([10.0, 8.0]).show(ui, |ui| {
                    path_row(ui, t!("엔진 폴더", "Engine folder"), &mut draft.runtime_dir, true, None);
                    path_row(ui, t!("Q4 모델", "Q4 model"), &mut draft.diffusion_model, false, Some(("GGUF", &["gguf", "safetensors"])));
                    path_row(ui, t!("INT8 모델", "INT8 model"), &mut draft.int8_model, false, Some(("safetensors", &["safetensors"])));
                    path_row(ui, t!("텍스트 인코더", "Text encoder"), &mut draft.text_encoder, false, Some(("GGUF", &["gguf", "safetensors"])));
                    path_row(ui, "VAE", &mut draft.vae, false, Some(("safetensors", &["safetensors", "gguf"])));
                    path_row(ui, t!("출력 폴더", "Output folder"), &mut draft.output_dir, true, None);
                });
                ui.add_space(8.0);
                section(ui, t!("메모리 · 성능", "Memory · Performance"));
                ui.label(
                    RichText::new(t!("모든 가중치는 RAM에 사본을 두고 VRAM은 6GB 한도로 씁니다. 모델을 디스크에서 다시 읽는 일이 없어 안정적입니다. INT8 모델은 RAM 16GB에 맞추려고 텍스트 인코더를 디스크에서 읽습니다.", "All weights keep a copy in RAM and VRAM is capped at 6 GB, so models are never re-read from disk. The INT8 model reads the text encoder from disk to fit in 16 GB RAM."))
                        .size(12.5)
                        .color(TEXT_DIM),
                );
                ui.checkbox(&mut draft.warmup, t!("시작할 때 예열하기 (권장)", "Warm up at startup (recommended)"))
                    .on_hover_text("엔진 준비 직후 아주 작은 이미지를 한 번 생성해 모델을 메모리에 올려 둡니다.
첫 생성이 약 36초 → 18초 수준으로 빨라집니다. 결과물에는 영향이 없습니다.");
                ui.horizontal(|ui| {
                    ui.label(t!("추가 인자", "Extra arguments"));
                    ui.add(TextEdit::singleline(&mut draft.extra_args).hint_text(t!("고급 사용자용 sd-server 인자 (예: --max-vram 7)", "sd-server arguments for advanced users (e.g. --max-vram 7)")).desired_width(f32::INFINITY));
                });
                ui.label(RichText::new(t!("경로·메모리 설정을 바꾸면 엔진이 재시작되고 모델을 다시 불러옵니다.", "Changing paths or memory settings restarts the engine and reloads the model.")).size(12.0).color(TEXT_FAINT));
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    if ui.add(egui::Button::new(RichText::new(t!("저장하고 엔진 재시작", "Save and restart engine")).color(Color32::WHITE)).fill(ACCENT)).clicked() {
                        apply = true;
                    }
                    if ui.button(t!("기본값", "Defaults")).clicked() {
                        let recent = std::mem::take(&mut draft.recent_prompts);
                        *draft = Settings::default();
                        draft.recent_prompts = recent;
                    }
                });
            });
        if want_int8_download {
            if let Some(dest) = self.settings_draft.as_ref().map(|d| d.int8_model.clone()) {
                self.dl = DlState { running: true, ..Default::default() };
                self.dl_is_lora = false;
                let task = DlTask { source: &INT8_SOURCE, dest };
                self.dl.cancel = Some(download::spawn(vec![task], self.dl_tx.clone(), ctx.clone()));
            }
        }
        if apply {
            let mut draft = self.settings_draft.take().unwrap();
            draft.recent_prompts = std::mem::take(&mut self.settings.recent_prompts);
            let out_changed = draft.output_dir != self.settings.output_dir;
            self.settings = draft;
            if out_changed {
                self.history = History::load(&self.settings.output_dir);
                self.thumbs.clear();
                self.full = None;
                self.selected = self.history.entries.first().map(|e| e.id);
                self.load_thumbnails(ctx);
            }
            self.engine.stop();
            self.engine_state = EngineState::Stopped;
            self.active = None;
            self.queue.clear();
            self.start_engine_if_possible();
            open = false;
        }
        self.show_settings = open;
        if !open {
            self.settings_draft = None;
        }
    }

    fn logs_window(&mut self, ctx: &Context) {
        let mut open = self.show_logs;
        egui::Window::new(bold(t!("엔진 로그", "Engine log"), 15.0))
            .open(&mut open)
            .default_size([760.0, 420.0])
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    if ui.button(t!("📋 전체 복사", "📋 Copy all")).clicked() {
                        ctx.copy_text(self.logs.iter().cloned().collect::<Vec<_>>().join("\n"));
                    }
                    if ui.button(t!("지우기", "Clear")).clicked() {
                        self.logs.clear();
                    }
                    if let EngineState::Error(e) = &self.engine_state {
                        ui.label(RichText::new(e).color(RED));
                    }
                });
                ScrollArea::vertical().stick_to_bottom(true).auto_shrink(false).show(ui, |ui| {
                    for l in &self.logs {
                        ui.label(RichText::new(l).monospace().size(12.0).color(TEXT_DIM));
                    }
                });
            });
        self.show_logs = open;
    }

    /// Asks what to do with a dropped PNG that carries its generation settings.
    fn drop_dialog(&mut self, ctx: &Context) {
        let Some((path, r)) = self.pending_drop.clone() else { return };
        let mut choice = None;
        egui::Modal::new(Id::unique("drop_png")).show(ctx, |ui| {
            ui.set_width(420.0);
            ui.label(bold(t!("생성 설정이 들어 있는 이미지입니다", "This image contains generation settings"), 16.0));
            ui.label(RichText::new(truncate(&r.prompt, 120)).color(TEXT));
            ui.label(
                RichText::new(tf!(
                    "{}×{} · 시드 {} · {}단계{}{}", "{}×{} · seed {} · {} steps{}{}",
                    r.width,
                    r.height,
                    r.seed,
                    r.sigmas.len().saturating_sub(1).max(1),
                    if r.fast { t!(" · ⚡빠른 모드", " · ⚡fast mode") } else { "" },
                    if r.loras.is_empty() { String::new() } else { tf!(" · LoRA {}개", " · LoRA {}", r.loras.len()) }
                ))
                .size(12.0)
                .color(TEXT_DIM),
            );
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                if ui.add(egui::Button::new(RichText::new(t!("♻ 설정 불러오기", "♻ Load settings")).color(Color32::WHITE)).fill(ACCENT)).clicked() {
                    choice = Some(0);
                }
                if ui.button(t!("🖼 입력 이미지로 사용", "🖼 Use as input image")).clicked() {
                    choice = Some(1);
                }
                if ui.button(t!("둘 다", "Both")).on_hover_text(t!("설정을 불러오고 이 이미지를 입력으로도 넣습니다", "Load the settings and also use this image as input")).clicked() {
                    choice = Some(2);
                }
                if ui.button(t!("취소", "Cancel")).clicked() {
                    choice = Some(3);
                }
            });
        });
        if ctx.input(|i| i.key_pressed(Key::Escape)) {
            choice = Some(3);
        }
        if let Some(c) = choice {
            self.pending_drop = None;
            if c == 0 || c == 2 {
                self.apply_restored(&r);
            }
            if c == 1 || c == 2 {
                inputs::load_file(path, self.input_tx.clone(), ctx.clone());
            }
        }
    }

    fn delete_dialog(&mut self, ctx: &Context) {
        let Some(id) = self.confirm_delete else { return };
        let mut close = false;
        egui::Modal::new(Id::unique("confirm_delete")).show(ctx, |ui| {
            ui.set_width(320.0);
            ui.label(bold(t!("이미지를 삭제할까요?", "Delete this image?"), 16.0));
            let note = if cfg!(windows) {
                t!("파일을 Windows 휴지통으로 옮깁니다. 휴지통에서 복원할 수 있습니다.", "Moves the file to the Windows Recycle Bin. You can restore it from there.")
            } else {
                t!("파일을 휴지통으로 옮깁니다. 휴지통에서 복원할 수 있습니다.", "Moves the file to the Trash. You can restore it from there.")
            };
            ui.label(RichText::new(note).color(TEXT_DIM));
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                if ui.add(egui::Button::new(RichText::new(t!("삭제", "Delete")).color(Color32::WHITE)).fill(Color32::from_rgb(190, 60, 60))).clicked() {
                    self.delete_entry(id);
                    close = true;
                }
                if ui.button(t!("취소", "Cancel")).clicked() {
                    close = true;
                }
            });
        });
        if close || ctx.input(|i| i.key_pressed(Key::Escape)) {
            self.confirm_delete = None;
        }
    }

    fn toasts(&mut self, ctx: &Context, screen: Rect) {
        self.toasts.retain(|t| t.born.elapsed().as_secs_f32() < 4.0);
        if self.toasts.is_empty() {
            return;
        }
        ctx.request_repaint_after(std::time::Duration::from_millis(100));
        egui::Area::new(Id::unique("toasts"))
            .order(Order::Tooltip)
            .anchor(Align2::CENTER_BOTTOM, vec2(0.0, -28.0))
            .interactable(false)
            .show(ctx, |ui| {
                ui.set_max_width(screen.width() * 0.6);
                for t in self.toasts.iter().rev().take(4).rev() {
                    let age = t.born.elapsed().as_secs_f32();
                    let alpha = ((4.0 - age) / 0.4).clamp(0.0, 1.0) * (age / 0.15).clamp(0.0, 1.0);
                    Frame::new()
                        .fill(CARD.gamma_multiply(alpha))
                        .stroke(Stroke::new(1.0, t.color.gamma_multiply(0.5 * alpha)))
                        .corner_radius(10)
                        .inner_margin(Margin::symmetric(14, 9))
                        .show(ui, |ui| {
                            ui.horizontal(|ui| {
                                let (r, _) = ui.allocate_exact_size(vec2(8.0, 8.0), Sense::hover());
                                ui.painter().circle_filled(r.center(), 4.0, t.color.gamma_multiply(alpha));
                                ui.label(RichText::new(&t.text).color(TEXT.gamma_multiply(alpha)));
                            });
                        });
                    ui.add_space(4.0);
                }
            });
    }
}

#[derive(Clone, Copy)]
enum GalleryAction {
    Select,
    View,
    Reuse,
    Copy,
    SaveAs,
    Reveal,
    Delete,
    UseInput,
    /// Hover trash button: straight to the Recycle Bin, no dialog.
    QuickDelete,
}

impl StudioApp {
    /// Self-test only (QIS_DRAG_TEST): feeds real pointer events into egui to drag the
    /// expand preview — the left part of the image 120 px left, then the area past the
    /// bottom-right corner 60 px out diagonally — and records the padding after each.
    fn drag_test_input(&mut self, ctx: &Context, raw: &mut egui::RawInput) {
        let (Some(step), Some(img)) = (self.drag_test_step, self.preview_img) else { return };
        let ev = |pos: Pos2, pressed: bool| egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: Modifiers::NONE,
        };
        let a = Pos2::new(img.left() + img.width() * 0.1, img.center().y); // left part
        let b = img.right_bottom() + vec2(10.0, 10.0); // just past the bottom-right corner
        let record = |name: &str, p: crate::config::Pads, img: Rect| {
            if let Some(dir) = std::env::var_os("QIS_SHOT_DIR") {
                let line = format!("{name} pads={p:?} img_w={:.1} img_h={:.1}\n", img.width(), img.height());
                let path = std::path::Path::new(&dir).join("drag_result.txt");
                let old = std::fs::read_to_string(&path).unwrap_or_default();
                let _ = std::fs::write(path, old + &line);
            }
        };
        match step {
            0 => {
                self.params.outpaint = [0.0; 4];
                raw.events.push(egui::Event::PointerMoved(a));
            }
            1 => raw.events.push(ev(a, true)),
            2..=13 => raw.events.push(egui::Event::PointerMoved(a - vec2(10.0 * (step - 1) as f32, 0.0))),
            14 => raw.events.push(ev(a - vec2(120.0, 0.0), false)),
            16 => {
                record("left_drag_-120px", self.params.outpaint, img);
                raw.events.push(egui::Event::PointerMoved(b));
            }
            17 => raw.events.push(ev(b, true)),
            18..=23 => raw.events.push(egui::Event::PointerMoved(b + vec2(10.0, 10.0) * (step - 17) as f32)),
            24 => raw.events.push(ev(b + vec2(60.0, 60.0), false)),
            26 => {
                record("corner_drag_+60+60px", self.params.outpaint, img);
                self.drag_test_step = None;
                return;
            }
            _ => {}
        }
        self.drag_test_step = Some(step + 1);
        ctx.request_repaint();
    }
}

impl eframe::App for StudioApp {
    fn raw_input_hook(&mut self, ctx: &Context, raw: &mut egui::RawInput) {
        self.drag_test_input(ctx, raw);
    }

    fn ui(&mut self, ui: &mut Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        let screen = ui.max_rect();
        self.fix_tiny_window(&ctx);
        self.pump(&ctx);
        self.shortcuts(&ctx);
        self.handle_drops(&ctx, screen);
        if matches!(self.engine_state, EngineState::Loading { .. }) || self.dl.running {
            ctx.request_repaint_after(std::time::Duration::from_millis(500));
        }

        Panel::top("top")
            .frame(Frame::new().fill(BG).inner_margin(Margin::symmetric(14, 10)).stroke(Stroke::new(1.0, BORDER)))
            .show_separator_line(false)
            .show(ui, |ui| self.top_bar(ui));

        Panel::left("controls")
            .frame(Frame::new().fill(PANEL))
            .default_size(380.0)
            .size_range(330.0..=520.0)
            .show(ui, |ui| self.controls(ui));

        Panel::right("gallery")
            .frame(Frame::new().fill(PANEL))
            .default_size(290.0)
            .size_range(170.0..=560.0)
            .show(ui, |ui| self.gallery(ui));

        egui::CentralPanel::no_frame()
            .frame(Frame::new().fill(BG))
            .show(ui, |ui| self.canvas(ui));

        if self.viewer {
            self.viewer_overlay(&ctx, screen);
        }
        if self.show_settings {
            self.settings_window(&ctx);
        }
        if self.show_logs {
            self.logs_window(&ctx);
        }
        if self.show_help {
            self.help_window(&ctx);
        }
        self.delete_dialog(&ctx);
        self.drop_dialog(&ctx);
        self.toasts(&ctx, screen);

        if let Some(mut t) = self.autotest.take() {
            let gen_progress = self.active.as_ref().map(|a| a.step);
            if t.wants_generation(self.engine_state == EngineState::Ready) {
                self.params.prompt = t.prompt.clone().unwrap_or_default();
                self.params.count = std::env::var("QIS_AUTOCOUNT").ok().and_then(|v| v.parse().ok()).unwrap_or(1);
                self.params.fast_mode = std::env::var("QIS_FAST").is_ok();
                self.params.mode = GenMode::Text;
                self.params.random_seed = true;
                if let Ok(path) = std::env::var("QIS_AUTOINPUT") {
                    self.params.mode = match std::env::var("QIS_AUTOMODE").as_deref() {
                        Ok("variation") => GenMode::Variation,
                        Ok("outpaint") => GenMode::Outpaint,
                        _ => GenMode::Edit,
                    };
                    if let Ok(img) = image::open(&path) {
                        self.inputs.clear();
                        let (w, h) = (img.width(), img.height());
                        self.add_input(&ctx, "auto".into(), img.to_rgba8());
                        if self.params.mode == GenMode::Outpaint {
                            self.params.outpaint = std::env::var("QIS_PADS")
                                .ok()
                                .and_then(|v| {
                                    let p: Vec<f32> = v.split(',').filter_map(|x| x.trim().parse().ok()).collect();
                                    (p.len() == 4).then(|| [p[0], p[1], p[2], p[3]])
                                })
                                .unwrap_or_else(|| crate::config::pads_for_aspect(w, h, 16.0, 9.0));
                            // The prompt is the scene description; QIS_NO_SCENE tests the
                            // empty-prompt path (instruction only).
                            if std::env::var_os("QIS_NO_SCENE").is_some() {
                                self.params.prompt.clear();
                            }
                        }
                    }
                }
                // QIS_AUTOSAMPLE: run the start-screen sample exactly as the button loads it.
                if std::env::var_os("QIS_AUTOSAMPLE").is_some() {
                    let count = self.params.count;
                    self.load_sample(&ctx);
                    self.params.count = count;
                }
                self.generate();
            }
            let idle = self.active.is_none() && self.queue.is_empty();
            // Keep the engine log next to the screenshots so a stuck run can be diagnosed.
            if self.logs.len() != t.logged {
                t.logged = self.logs.len();
                if let Some(dir) = std::env::var_os("QIS_SHOT_DIR") {
                    let text: Vec<&str> = self.logs.iter().map(|s| s.as_str()).collect();
                    let _ = std::fs::write(std::path::Path::new(&dir).join("engine_log.txt"), text.join("\n"));
                }
            }
            // Self-test of the first-run download: one shot mid-way through the CUDA zip
            // (Linux has no CUDA zip: the first model file instead).
            #[cfg(windows)]
            let shot_file = crate::config::CUDART_SOURCE.file;
            #[cfg(not(windows))]
            let shot_file = crate::config::MODEL_SOURCES[0].file;
            if self.dl_setup
                && self.dl.current == Some(shot_file)
                && self.dl.total > 0
                && self.dl.done > self.dl.total / 2
            {
                t.shoot(&ctx, "setup");
            }
            t.hold = self.captioning.is_some()
                || self.caption_wanted
                || self.dl_setup
                || (std::env::var_os("QIS_LOAD_INPUT").is_some() && self.inputs.is_empty());
            t.tick(&ctx, gen_progress, self.history.entries.len(), idle);
            self.autotest = Some(t);
        }
    }

    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        if self.autotest.is_some() {
            return; // never persist settings changed by a self-test run
        }
        eframe::set_value(storage, "settings", &self.settings);
        eframe::set_value(storage, "params", &self.params);
    }

    fn on_exit(&mut self) {
        self.engine.shutdown();
    }
}

// ---------------------------------------------------------------------- free helpers

fn side_cursor(side: u8) -> CursorIcon {
    use crate::config::{SIDE_B as B, SIDE_L as L, SIDE_R as R, SIDE_T as T};
    match side {
        s if s == L | T || s == R | B => CursorIcon::ResizeNwSe,
        s if s == R | T || s == L | B => CursorIcon::ResizeNeSw,
        s if s == L || s == R => CursorIcon::ResizeHorizontal,
        _ => CursorIcon::ResizeVertical,
    }
}

/// Scene description to reuse from a prompt recorded in an image this app made: the scene
/// part of an expand prompt, or a plain description. Edit instructions ("…바꿔줘") and the
/// generic default are not scene descriptions.
fn scene_from_record(prompt: &str) -> Option<String> {
    let p = prompt.trim();
    let scene = if p.starts_with(crate::engine::OUTPAINT_GUIDE) {
        p.rsplit_once("장면 설명:").map(|(_, s)| s.trim())?
    } else {
        p
    };
    let instruction = ["해줘", "바꿔", "변경", "줘.", "주세요", "1번 이미지", "2번 이미지", "image 1", "image 2"].iter().any(|w| scene.contains(w))
        || scene.ends_with('줘');
    (!scene.is_empty() && !crate::engine::is_outpaint_default(scene) && !instruction && !scene.starts_with("<mva>")).then(|| scene.to_owned())
}

#[cfg(test)]
mod scene_tests {
    use super::scene_from_record;

    #[test]
    fn picks_scene_descriptions_only() {
        let scene = "카페 창가 테이블에 앉은 여성, 따뜻한 자연광";
        assert_eq!(scene_from_record(scene).as_deref(), Some(scene));
        let expanded = format!("{} 장면 설명: {scene}", crate::engine::OUTPAINT_GUIDE);
        assert_eq!(scene_from_record(&expanded).as_deref(), Some(scene));
        let generic = format!("{} 장면 설명: {}", crate::engine::OUTPAINT_GUIDE, crate::engine::OUTPAINT_DEFAULT);
        assert_eq!(scene_from_record(&generic), None);
        assert_eq!(scene_from_record("배경을 해변으로 바꿔줘"), None);
        assert_eq!(scene_from_record("<mva> back view, eye-level shot"), None);
    }
}

fn mode_tag(e: &HistoryEntry) -> String {
    let fast = if e.fast { t!("⚡빠른 모드 · ", "⚡Fast · ") } else { "" };
    let mode = match e.mode {
        GenMode::Text => String::new(),
        GenMode::Variation => tf!("변형 {:.2} · ", "Variation {:.2} · ", e.strength),
        GenMode::Edit => t!("편집 · ", "Edit · ").to_owned(),
        GenMode::Inpaint => tf!("부분 수정 {:.2} · ", "Inpaint {:.2} · ", e.strength),
        GenMode::Outpaint => t!("확장 · ", "Expand · ").to_owned(),
    };
    let lora = if e.loras.is_empty() { String::new() } else { tf!("LoRA {}개 · ", "{} LoRA · ", e.loras.len()) };
    format!("{fast}{mode}{lora}")
}

fn truncate(s: &str, n: usize) -> String {
    let s = s.replace('\n', " ");
    if s.chars().count() <= n { s } else { format!("{}…", s.chars().take(n).collect::<String>()) }
}

/// Largest rect with the given aspect that fits inside `area`, centered.
fn fit(area: Rect, size: Vec2) -> Rect {
    if size.x <= 0.0 || size.y <= 0.0 {
        return area;
    }
    let s = (area.width() / size.x).min(area.height() / size.y);
    Rect::from_center_size(area.center(), size * s)
}

/// UV rect that crops a texture to a centered square ("object-fit: cover").
fn cover_uv(size: Vec2) -> Rect {
    if size.x > size.y {
        let f = size.y / size.x;
        Rect::from_min_max(Pos2::new((1.0 - f) / 2.0, 0.0), Pos2::new((1.0 + f) / 2.0, 1.0))
    } else {
        let f = size.x / size.y;
        Rect::from_min_max(Pos2::new(0.0, (1.0 - f) / 2.0), Pos2::new(1.0, (1.0 + f) / 2.0))
    }
}

fn shimmer(ui: &mut Ui, rect: Rect) {
    let t = ui.input(|i| i.time) as f32;
    let x = rect.left() + (t * 0.6).fract() * (rect.width() + 200.0) - 100.0;
    let band = Rect::from_min_max(Pos2::new((x - 80.0).max(rect.left()), rect.top()), Pos2::new((x + 80.0).min(rect.right()), rect.bottom()));
    if band.width() > 0.0 {
        ui.painter().rect_filled(band, 0, Color32::from_white_alpha(6));
    }
    ui.painter().text(rect.center(), Align2::CENTER_CENTER, "🎨", FontId::proportional(40.0), TEXT_FAINT);
    ui.ctx().request_repaint();
}

fn pending_cell(ui: &mut Ui, cell: f32, active: Option<&Active>) {
    let (rect, _) = ui.allocate_exact_size(vec2(cell, cell), Sense::hover());
    let p = ui.painter();
    p.rect_filled(rect, 10, CARD);
    p.rect_stroke(rect, 10, Stroke::new(1.0, BORDER), egui::StrokeKind::Inside);
    match active {
        Some(a) => {
            if let Some(t) = &a.preview {
                egui::Image::new(t).uv(cover_uv(t.size_vec2())).corner_radius(10).tint(Color32::from_white_alpha(170)).paint_at(ui, rect);
            }
            let frac = if a.total > 0 { a.step as f32 / a.total as f32 } else { 0.0 };
            let bar = Rect::from_min_size(rect.left_bottom() + vec2(8.0, -12.0), vec2((cell - 16.0) * frac, 4.0));
            ui.painter().rect_filled(Rect::from_min_size(bar.min, vec2(cell - 16.0, 4.0)), 2, Color32::from_white_alpha(25));
            ui.painter().rect_filled(bar, 2, ACCENT);
            // Paint only: `ui.put` would move the row's cursor to the spinner's edge, and the
            // next thumbnail was then drawn over half of this cell.
            egui::Spinner::new()
                .size(20.0)
                .color(Color32::WHITE)
                .paint_at(ui, Rect::from_center_size(rect.center(), vec2(24.0, 24.0)));
        }
        None => {
            p.text(rect.center(), Align2::CENTER_CENTER, t!("대기 중", "Waiting"), FontId::proportional(12.5), TEXT_FAINT);
        }
    }
}

/// Linux fallback shown when the engine archive couldn't be downloaded: how to build
/// sd-server locally. One placed in the engine folder or on PATH is picked up on its own
/// (checked every couple of seconds).
#[cfg(not(windows))]
fn engine_build_help(ui: &mut Ui, settings: &Settings) {
    let runtime = settings.runtime_dir.display().to_string();
    let cmds = linux_build_commands(&runtime);
    ui.add_space(4.0);
    Frame::new().fill(WIDGET).corner_radius(8).inner_margin(Margin::same(10)).show(ui, |ui| {
        ui.label(
            RichText::new(tf!(
                "엔진을 받지 못했다면 직접 빌드할 수도 있습니다. NVIDIA 드라이버, CUDA Toolkit 12, cmake, g++, git을 설치한 뒤 터미널에서 실행하세요. {} 또는 PATH에 sd-server가 생기면 자동으로 찾습니다.",
                "If the engine can't be downloaded, you can build it. With the NVIDIA driver, CUDA Toolkit 12, cmake, g++ and git installed, run in a terminal. An sd-server in {} or on PATH is found automatically.",
                runtime
            ))
            .size(12.0)
            .color(TEXT_DIM),
        );
        ui.add_space(4.0);
        ui.label(RichText::new(&cmds).monospace().size(11.5).color(TEXT));
        if ui.small_button(t!("📋 명령 복사", "📋 Copy commands")).clicked() {
            ui.ctx().copy_text(cmds.clone());
        }
    });
    ui.add_space(6.0);
    ui.ctx().request_repaint_after(std::time::Duration::from_secs(2));
}

/// Shell commands that build the tested sd.cpp release with CUDA and install sd-server.
#[cfg(not(windows))]
fn linux_build_commands(runtime: &str) -> String {
    // Single-quoted for the shell (a ' inside becomes '\'').
    let rt = format!("'{}'", runtime.replace('\'', r"'\''"));
    [
        "git clone --recursive --branch master-1052-f5df5f8 https://github.com/leejet/stable-diffusion.cpp".to_owned(),
        "cd stable-diffusion.cpp".to_owned(),
        "cmake -B build -DSD_CUDA=ON -DCMAKE_BUILD_TYPE=Release".to_owned(),
        "cmake --build build --config Release -j --target sd-server".to_owned(),
        format!("mkdir -p {rt} && cp build/bin/sd-server {rt}/"),
    ]
    .join("\n")
}

fn check_row(ui: &mut Ui, ok: bool, label: &str, detail: &str) {
    ui.horizontal(|ui| {
        let (r, _) = ui.allocate_exact_size(vec2(22.0, 22.0), Sense::hover());
        ui.painter().circle_filled(r.center(), 10.0, if ok { GREEN.gamma_multiply(0.18) } else { WIDGET });
        ui.painter().text(r.center(), Align2::CENTER_CENTER, if ok { "✔" } else { "•" }, FontId::proportional(12.0), if ok { GREEN } else { TEXT_DIM });
        ui.vertical(|ui| {
            ui.spacing_mut().item_spacing.y = 0.0;
            ui.label(RichText::new(label).color(TEXT));
            ui.label(RichText::new(detail).size(12.0).color(TEXT_FAINT));
        });
    });
}

fn path_row(ui: &mut Ui, label: &str, path: &mut PathBuf, dir: bool, filter: Option<(&str, &[&str])>) {
    let ok = if dir { path.is_dir() } else { path.is_file() };
    ui.label(label);
    let mut s = path.display().to_string();
    if ui.add(TextEdit::singleline(&mut s).desired_width(400.0)).changed() {
        *path = PathBuf::from(s);
    }
    ui.horizontal(|ui| {
        if ui.button("📂").on_hover_text(t!("찾아보기", "Browse")).clicked() {
            let mut d = rfd::FileDialog::new();
            if let Some(parent) = if dir { Some(path.as_path()) } else { path.parent() } {
                if parent.is_dir() {
                    d = d.set_directory(parent);
                }
            }
            let picked = if dir {
                d.pick_folder()
            } else {
                if let Some((name, exts)) = filter {
                    d = d.add_filter(name, exts);
                }
                d.pick_file()
            };
            if let Some(p) = picked {
                *path = p;
            }
        }
        ui.label(if ok { RichText::new("✔").color(GREEN) } else if dir { RichText::new(t!("새 폴더", "New folder")).color(TEXT_FAINT).size(12.0) } else { RichText::new(t!("✖ 없음", "✖ Missing")).color(RED) });
    });
    ui.end_row();
}

/// "1.2 GB" / "360 MB".
fn size_text(bytes: u64) -> String {
    if bytes >= 1_000_000_000 { format!("{:.1} GB", bytes as f64 / 1e9) } else { format!("{:.0} MB", bytes as f64 / 1e6) }
}

/// Built-in sample pictures (start screen): a model holding a phone and a case design.
const SAMPLE_MODEL: &[u8] = include_bytes!("../assets/samples/model.webp");
const SAMPLE_CASE: &[u8] = include_bytes!("../assets/samples/case.webp");

fn decode_sample(bytes: &[u8]) -> Option<image::RgbaImage> {
    image::load_from_memory(bytes).ok().map(|i| i.to_rgba8())
}
