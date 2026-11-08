//! Generation history, stored as `history.json` next to the output images.

use crate::config::{GenMode, StepMode};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Clone, Serialize, Deserialize)]
pub struct HistoryEntry {
    pub id: u64,
    pub file: PathBuf,
    pub prompt: String,
    #[serde(default)]
    pub negative_prompt: String,
    pub width: u32,
    pub height: u32,
    pub seed: u64,
    pub cfg: f32,
    pub step_mode: StepMode,
    pub steps: u32,
    pub created: String,
    pub elapsed: f32,
    #[serde(default)]
    pub mode: GenMode,
    #[serde(default)]
    pub strength: f32,
    #[serde(default)]
    pub fast: bool,
    #[serde(default)]
    pub loras: Vec<(String, f32)>,
}

pub struct History {
    /// Newest first.
    pub entries: Vec<HistoryEntry>,
    dir: PathBuf,
}

impl History {
    pub fn load(dir: &Path) -> Self {
        let entries: Vec<HistoryEntry> = std::fs::read(dir.join("history.json"))
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default();
        let entries = entries.into_iter().filter(|e| e.file.is_file()).collect();
        Self { entries, dir: dir.to_path_buf() }
    }

    pub fn save(&self) {
        let _ = std::fs::create_dir_all(&self.dir);
        if let Ok(json) = serde_json::to_vec_pretty(&self.entries) {
            let tmp = self.dir.join("history.json.tmp");
            if std::fs::write(&tmp, json).is_ok() {
                let _ = std::fs::rename(tmp, self.dir.join("history.json"));
            }
        }
    }

    pub fn next_id(&self) -> u64 {
        self.entries.iter().map(|e| e.id).max().unwrap_or(0) + 1
    }

    pub fn get(&self, id: u64) -> Option<&HistoryEntry> {
        self.entries.iter().find(|e| e.id == id)
    }

    pub fn index_of(&self, id: u64) -> Option<usize> {
        self.entries.iter().position(|e| e.id == id)
    }

    pub fn remove(&mut self, id: u64) -> Option<HistoryEntry> {
        let i = self.index_of(id)?;
        let e = self.entries.remove(i);
        self.save();
        Some(e)
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn old_motion_entries_still_load_as_edit() {
        let json = r#"[{"id":1,"file":"x.png","prompt":"p","width":8,"height":8,"seed":1,"cfg":1.0,
            "step_mode":"Turbo","steps":8,"created":"","elapsed":1.0,"mode":"Motion"}]"#;
        let v: Vec<super::HistoryEntry> = serde_json::from_str(json).expect("parses");
        assert!(v[0].mode == crate::config::GenMode::Edit);
    }
}
