//! Reads the generation record sd.cpp embeds in output PNGs
//! (`tEXt` keyword `parameters`: "<WebUI infotext>, SDCPP: <native JSON>").

use serde_json::Value;
use std::path::Path;

#[derive(Clone, Debug)]
pub struct Restored {
    pub prompt: String,
    pub negative_prompt: String,
    pub width: u32,
    pub height: u32,
    pub seed: u64,
    pub cfg: f32,
    /// Custom sigmas as recorded (Turbo = 9 values).
    pub sigmas: Vec<f32>,
    pub steps: u32,
    pub fast: bool,
    /// Present when the image was made from an init image (variation / partial edit).
    pub strength: Option<f32>,
    /// LoRA file names (without directories) and weights.
    pub loras: Vec<(String, f32)>,
}

/// Returns the native JSON record of a PNG made by sd.cpp, if any.
pub fn read_record(path: &Path) -> Option<Value> {
    record_from_bytes(&std::fs::read(path).ok()?)
}

/// `read_record` on file contents already in memory.
pub fn record_from_bytes(data: &[u8]) -> Option<Value> {
    if data.len() < 8 || &data[..8] != b"\x89PNG\r\n\x1a\n" {
        return None;
    }
    let mut i = 8;
    while i + 12 <= data.len() {
        let len = u32::from_be_bytes(data[i..i + 4].try_into().ok()?) as usize;
        let kind = &data[i + 4..i + 8];
        let body = data.get(i + 8..i + 8 + len)?;
        if kind == b"tEXt" || kind == b"iTXt" {
            if let Some(nul) = body.iter().position(|b| *b == 0) {
                if &body[..nul] == b"parameters" {
                    let mut text = &body[nul + 1..];
                    if kind == b"iTXt" {
                        // compression flag, method, language\0, translated keyword\0
                        text = text.get(2..)?;
                        for _ in 0..2 {
                            let n = text.iter().position(|b| *b == 0)?;
                            text = &text[n + 1..];
                        }
                    }
                    let text = String::from_utf8_lossy(text);
                    let json = text.rsplit_once("SDCPP: ").map(|(_, j)| j)?;
                    return serde_json::from_str(json.trim()).ok();
                }
            }
        }
        if kind == b"IEND" {
            break;
        }
        i += 12 + len;
    }
    None
}

pub fn read(path: &Path) -> Option<Restored> {
    from_record(read_record(path)?)
}

/// Just the prompt recorded in file contents already in memory.
pub fn prompt_from_bytes(data: &[u8]) -> Option<String> {
    record_from_bytes(data)?["prompt"].as_str().map(str::to_owned)
}

fn from_record(v: Value) -> Option<Restored> {
    let prompt = v["prompt"].as_str()?.to_owned();
    let sp = &v["sample_params"];
    let sigmas: Vec<f32> = sp["custom_sigmas"]
        .as_array()
        .map(|a| a.iter().filter_map(|x| x.as_f64()).map(|x| x as f32).collect())
        .unwrap_or_default();
    let seed = match &v["seed"] {
        Value::String(s) => s.parse().unwrap_or(0),
        x => x.as_u64().unwrap_or(0),
    };
    let loras = v["loras"]
        .as_array()
        .map(|a| {
            a.iter()
                .filter_map(|l| {
                    let p = l["path"].as_str()?;
                    // Split on both separators: a picture made on Windows (`D:\…\x.safetensors`)
                    // must also restore its LoRAs on Linux, and the other way round.
                    let name = p.rsplit(['/', '\\']).next().filter(|n| !n.is_empty())?.to_owned();
                    Some((name, l["multiplier"].as_f64().unwrap_or(1.0) as f32))
                })
                .collect()
        })
        .unwrap_or_default();
    Some(Restored {
        prompt,
        negative_prompt: v["negative_prompt"].as_str().unwrap_or("").to_owned(),
        width: v["width"].as_u64().unwrap_or(1024) as u32,
        height: v["height"].as_u64().unwrap_or(1024) as u32,
        seed,
        cfg: sp["guidance"]["txt_cfg"].as_f64().unwrap_or(1.0) as f32,
        steps: sp["sample_steps"].as_u64().unwrap_or(8) as u32,
        sigmas,
        fast: v["cache"]["mode"].as_str() == Some("spectrum"),
        strength: v["strength"].as_f64().map(|s| s as f32),
        loras,
    })
}

#[cfg(test)]
mod tests {
    #[test]
    fn reads_sdcpp_parameters_chunk() {
        // Minimal PNG: signature + one tEXt chunk + IEND (CRCs are not checked by the reader).
        let json = r#"{"prompt":"고양이","negative_prompt":"","width":896,"height":1184,"seed":"1694619703",
            "sample_params":{"sample_steps":8,"custom_sigmas":[1.0,0.5,0.0],"guidance":{"txt_cfg":1.0}},
            "cache":{"mode":"spectrum"},"loras":[{"path":"D:\\m\\loras\\dmd2.safetensors","multiplier":0.8}]}"#;
        let text = format!("parameters\0고양이\nSteps: 8, SDCPP: {json}");
        let mut png = b"\x89PNG\r\n\x1a\n".to_vec();
        png.extend((text.len() as u32).to_be_bytes());
        png.extend(b"tEXt");
        png.extend(text.as_bytes());
        png.extend([0u8; 4]);
        png.extend([0, 0, 0, 0]);
        png.extend(b"IEND");
        png.extend([0u8; 4]);
        let p = std::env::temp_dir().join("qis_meta_test.png");
        std::fs::write(&p, png).unwrap();
        let r = super::read(&p).expect("parsed");
        assert_eq!(r.prompt, "고양이");
        assert_eq!((r.width, r.height, r.seed), (896, 1184, 1694619703));
        assert!(r.fast);
        assert_eq!(r.sigmas.len(), 3);
        assert_eq!(r.loras, vec![("dmd2.safetensors".to_owned(), 0.8)]);
        let _ = std::fs::remove_file(p);
    }
}
