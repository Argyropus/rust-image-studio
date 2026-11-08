//! Downloader for models, LoRAs and the engine zips (resumable, one file at a time).

use crate::config::ModelSource;
use anyhow::{Context as _, Result, bail};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::mpsc::Sender;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

pub enum DlEvent {
    Progress { file: &'static str, done: u64, total: u64 },
    /// A downloaded zip is being extracted.
    Extracting { file: &'static str },
    Finished { file: &'static str },
    Failed { file: &'static str, message: String },
    AllDone,
}

pub struct DlTask {
    pub source: &'static ModelSource,
    pub dest: PathBuf,
}

/// Downloads every task in order on a background thread. Returns a cancel flag.
/// A failure stops the batch (no `AllDone`) unless `keep_going`, which goes on with the
/// next file and still ends with `AllDone` (first-run setup: one bad extra mustn't block the rest).
pub fn spawn(tasks: Vec<DlTask>, tx: Sender<DlEvent>, ctx: egui::Context) -> Arc<AtomicBool> {
    spawn_with(tasks, tx, ctx, false)
}

pub fn spawn_with(tasks: Vec<DlTask>, tx: Sender<DlEvent>, ctx: egui::Context, keep_going: bool) -> Arc<AtomicBool> {
    let cancel = Arc::new(AtomicBool::new(false));
    let flag = cancel.clone();
    std::thread::spawn(move || {
        for t in tasks {
            let file = t.source.file;
            match fetch(&t, &tx, &ctx, &flag) {
                Ok(()) => {
                    let _ = tx.send(DlEvent::Finished { file });
                }
                Err(e) => {
                    let _ = tx.send(DlEvent::Failed { file, message: format!("{e:#}") });
                    ctx.request_repaint();
                    if !keep_going || flag.load(Ordering::Relaxed) {
                        return;
                    }
                }
            }
            ctx.request_repaint();
        }
        let _ = tx.send(DlEvent::AllDone);
        ctx.request_repaint();
    });
    cancel
}

fn fetch(t: &DlTask, tx: &Sender<DlEvent>, ctx: &egui::Context, cancel: &AtomicBool) -> Result<()> {
    if let Some(dir) = t.dest.parent() {
        std::fs::create_dir_all(dir)?;
    }
    #[cfg(windows)]
    if t.source.file == crate::config::VCREDIST_SOURCE.file && t.dest.is_file() {
        return run_vcredist(t, tx, ctx); // downloaded earlier, not installed yet
    }
    if t.source.unzip && t.dest.is_file() {
        return unpack(t, tx, ctx); // downloaded earlier, extraction didn't finish
    }
    let part = t.dest.with_extension("part");
    let have = std::fs::metadata(&part).map(|m| m.len()).unwrap_or(0);

    let agent: ureq::Agent = ureq::Agent::config_builder()
        .http_status_as_error(false)
        .build()
        .into();
    let mut req = agent.get(source_url(t.source));
    if have > 0 {
        req = req.header("Range", &format!("bytes={have}-"));
    }
    let resp = req.call().context(t!("다운로드 서버 연결 실패", "Could not connect to the download server"))?;
    let status = resp.status().as_u16();
    let (mut done, append) = match status {
        206 => (have, true),
        200 => (0, false),
        416 if have > 0 => (have, true), // already complete
        _ => bail!("HTTP {status}"),
    };
    let len: u64 = resp
        .headers()
        .get("content-length")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.parse().ok())
        .unwrap_or(0);
    let total = if status == 416 { have } else { done + len };

    let mut out = std::fs::OpenOptions::new()
        .create(true)
        .write(true)
        .append(append)
        .truncate(!append)
        .open(&part)?;
    if status != 416 {
        let mut body = resp.into_body().into_reader();
        let mut buf = vec![0u8; 1 << 20];
        let mut last = std::time::Instant::now();
        loop {
            if cancel.load(Ordering::Relaxed) {
                bail!("{}", t!("취소됨", "Cancelled"));
            }
            let n = body.read(&mut buf)?;
            if n == 0 {
                break;
            }
            out.write_all(&buf[..n])?;
            done += n as u64;
            if last.elapsed().as_millis() > 200 {
                last = std::time::Instant::now();
                let _ = tx.send(DlEvent::Progress { file: t.source.file, done, total });
                ctx.request_repaint();
            }
        }
    }
    out.flush()?;
    drop(out);
    if total > 0 && done < total {
        bail!("{}", tf!("다운로드가 중간에 끊겼습니다 ({}/{} 바이트). 다시 시도하면 이어받습니다.", "The download was cut off ({}/{} bytes). Try again to resume.", done, total));
    }
    std::fs::rename(&part, &t.dest)?;
    if t.source.unzip {
        return unpack(t, tx, ctx);
    }
    #[cfg(windows)]
    if t.source.file == crate::config::VCREDIST_SOURCE.file {
        return run_vcredist(t, tx, ctx);
    }
    Ok(())
}

/// Runs Microsoft's VC++ redistributable installer elevated (Windows shows its UAC
/// prompt), waits for it, then deletes it. Exit 3010 = installed, reboot suggested;
/// 1638 = a newer version is already there. Both count as success.
#[cfg(windows)]
fn run_vcredist(t: &DlTask, tx: &Sender<DlEvent>, ctx: &egui::Context) -> Result<()> {
    use std::os::windows::process::CommandExt;
    let _ = tx.send(DlEvent::Extracting { file: t.source.file });
    ctx.request_repaint();
    let script = format!(
        "try {{ $p = Start-Process -FilePath '{}' -ArgumentList '/install','/passive','/norestart' -Verb RunAs -Wait -PassThru; exit $p.ExitCode }} catch {{ exit 1223 }}",
        t.dest.display().to_string().replace('\'', "''")
    );
    let status = std::process::Command::new("powershell")
        .args(["-NoProfile", "-NonInteractive", "-Command", &script])
        .creation_flags(0x0800_0000) // CREATE_NO_WINDOW
        .status()
        .context("powershell")?;
    let code = status.code().unwrap_or(-1);
    if matches!(code, 0 | 3010 | 1638) || crate::config::vcredist_installed() {
        let _ = std::fs::remove_file(&t.dest);
        return Ok(());
    }
    if code == 1223 {
        bail!("{}", t!("설치 권한 확인이 취소되었습니다. 다시 시도를 누르고 '예'를 선택하세요.", "The permission prompt was declined. Press Try again and choose Yes."));
    }
    bail!("{}", tf!("Visual C++ 런타임 설치 실패 (코드 {})", "Visual C++ runtime install failed (code {})", code));
}

fn unpack(t: &DlTask, tx: &Sender<DlEvent>, ctx: &egui::Context) -> Result<()> {
    let _ = tx.send(DlEvent::Extracting { file: t.source.file });
    ctx.request_repaint();
    let dir = t.dest.parent().unwrap_or(Path::new("."));
    extract(&t.dest, dir).with_context(|| tf!("압축 풀기 실패: {}", "Could not extract {}", t.source.file))?;
    let _ = std::fs::remove_file(&t.dest);
    Ok(())
}

/// The download URL. Self-test runs may point every file at a local mirror
/// (`QIS_DL_MIRROR=http://127.0.0.1:port`, served as `<mirror>/<file name>`).
fn source_url(src: &ModelSource) -> String {
    if std::env::var_os("QIS_SHOT_DIR").is_some() {
        if let Ok(m) = std::env::var("QIS_DL_MIRROR") {
            return format!("{}/{}", m.trim_end_matches('/'), src.file);
        }
    }
    src.url.to_owned()
}

/// Extracts a .zip or .tar.gz into `dir`. Entries with unsafe paths (absolute, `..`) are
/// skipped. Every file is written under a temporary name first and renamed only after the
/// whole archive came out, so a cut-off extraction never leaves an exe that looks installed
/// next to missing DLLs / shared libraries. A .tar.gz whose entries all sit in one top
/// folder (llama.cpp's Linux builds: `llama-b12182/…`) is extracted without that folder.
fn extract(archive: &Path, dir: &Path) -> Result<()> {
    let name = archive.file_name().unwrap_or_default().to_string_lossy().to_ascii_lowercase();
    let staged = if name.ends_with(".tar.gz") || name.ends_with(".tgz") { stage_tar_gz(archive, dir)? } else { stage_zip(archive, dir)? };
    staged.finish()
}

/// Files written under temporary names, plus symlinks to create once they are in place.
#[derive(Default)]
struct Staged {
    /// (temporary file, final path, Unix permission bits from the archive)
    files: Vec<(PathBuf, PathBuf, Option<u32>)>,
    /// (link path, relative target)
    links: Vec<(PathBuf, PathBuf)>,
}

impl Staged {
    fn write(&mut self, out: PathBuf, mode: Option<u32>, data: &mut dyn Read) -> Result<()> {
        if let Some(parent) = out.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let mut name = out.file_name().unwrap_or_default().to_os_string();
        name.push(".unzipping");
        let tmp = out.with_file_name(name);
        {
            let mut f = std::fs::File::create(&tmp)?;
            std::io::copy(data, &mut f)?;
        }
        self.files.push((tmp, out, mode));
        Ok(())
    }

    fn finish(self) -> Result<()> {
        for (tmp, out, mode) in self.files {
            set_mode(&tmp, &out, mode)?;
            if out.exists() {
                let _ = std::fs::remove_file(&out);
            }
            std::fs::rename(&tmp, &out)?;
        }
        for (link, target) in self.links {
            make_link(&link, &target)?;
        }
        Ok(())
    }
}

fn stage_zip(zip_path: &Path, dir: &Path) -> Result<Staged> {
    let mut archive = zip::ZipArchive::new(std::fs::File::open(zip_path)?)?;
    let mut staged = Staged::default();
    for i in 0..archive.len() {
        let mut entry = archive.by_index(i)?;
        let Some(rel) = entry.enclosed_name() else { continue };
        let out = dir.join(rel);
        if entry.is_dir() {
            std::fs::create_dir_all(&out)?;
            continue;
        }
        if cfg!(unix) && entry.is_symlink() {
            let mut target = String::new();
            entry.read_to_string(&mut target)?;
            if let Some(t) = safe_rel(Path::new(&target)) {
                staged.links.push((out, t));
            }
            continue;
        }
        let mode = entry.unix_mode();
        staged.write(out, mode, &mut entry)?;
    }
    Ok(staged)
}

fn stage_tar_gz(path: &Path, dir: &Path) -> Result<Staged> {
    let open = || -> Result<tar::Archive<flate2::read::GzDecoder<std::fs::File>>> {
        Ok(tar::Archive::new(flate2::read::GzDecoder::new(std::fs::File::open(path)?)))
    };
    // First pass (headers only): is everything inside a single top folder?
    let mut top: Option<std::ffi::OsString> = None;
    let mut single = true;
    for entry in open()?.entries()? {
        let entry = entry?;
        let Some(rel) = safe_rel(&entry.path()?) else { continue };
        let mut parts = rel.components();
        let first = parts.next().map(|c| c.as_os_str().to_os_string());
        let nested = parts.next().is_some();
        let is_dir = entry.header().entry_type().is_dir();
        match (&top, first) {
            (_, None) => {}
            (None, Some(f)) if nested || is_dir => top = Some(f),
            (Some(t), Some(f)) if *t == f && (nested || is_dir) => {}
            _ => single = false,
        }
    }
    let strip = top.filter(|_| single);

    let mut staged = Staged::default();
    for entry in open()?.entries()? {
        let mut entry = entry?;
        let Some(mut rel) = safe_rel(&entry.path()?) else { continue };
        if let Some(t) = &strip {
            rel = rel.strip_prefix(t).map(Path::to_path_buf).unwrap_or(rel);
        }
        if rel.as_os_str().is_empty() {
            continue;
        }
        let out = dir.join(&rel);
        let kind = entry.header().entry_type();
        if kind.is_dir() {
            std::fs::create_dir_all(&out)?;
        } else if kind.is_symlink() {
            if let Some(t) = entry.link_name()?.as_deref().and_then(safe_rel) {
                staged.links.push((out, t));
            }
        } else if kind.is_file() {
            let mode = entry.header().mode().ok();
            staged.write(out, mode, &mut entry)?;
        }
        // Hard links, devices etc. are not used by the engine archives: skipped.
    }
    Ok(staged)
}

/// `p` as a relative path made only of normal components (no root, `..` or prefix).
fn safe_rel(p: &Path) -> Option<PathBuf> {
    use std::path::Component;
    let mut out = PathBuf::new();
    for c in p.components() {
        match c {
            Component::Normal(s) => out.push(s),
            Component::CurDir => {}
            _ => return None,
        }
    }
    (!out.as_os_str().is_empty()).then_some(out)
}

/// The engine/captioner programs, which must stay runnable even if an archive lost
/// their permission bits.
#[cfg(unix)]
fn is_program(out: &Path) -> bool {
    let name = out.file_name().unwrap_or_default();
    [crate::config::SERVER_EXE, crate::config::CAPTION_EXE, "sd-cli"].iter().any(|n| name == *n)
}

/// Applies the archive's permission bits (Unix): executables and shared libraries keep
/// their exec bit, and the engine programs always get one.
#[cfg(unix)]
fn set_mode(tmp: &Path, out: &Path, mode: Option<u32>) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let mut m = mode.map(|m| m & 0o777).filter(|m| *m != 0).unwrap_or(0o644);
    if is_program(out) {
        m |= 0o755;
    }
    std::fs::set_permissions(tmp, std::fs::Permissions::from_mode(m))?;
    Ok(())
}

#[cfg(not(unix))]
fn set_mode(_tmp: &Path, _out: &Path, _mode: Option<u32>) -> Result<()> {
    Ok(())
}

/// Symlinks from the archive (the Linux .so version chains), created after the files.
/// A link whose target escapes the folder was already dropped by `safe_rel`.
#[cfg(unix)]
fn make_link(link: &Path, target: &Path) -> Result<()> {
    if let Some(parent) = link.parent() {
        std::fs::create_dir_all(parent)?;
    }
    if link.symlink_metadata().is_ok() {
        let _ = std::fs::remove_file(link);
    }
    std::os::unix::fs::symlink(target, link)?;
    Ok(())
}

/// Windows: links only come in Linux archives, which this build never downloads.
#[cfg(not(unix))]
fn make_link(_link: &Path, _target: &Path) -> Result<()> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("qis_extract_{tag}_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    fn tar_gz(path: &Path, entries: &[(&str, &[u8], u32)], links: &[(&str, &str)]) {
        let gz = flate2::write::GzEncoder::new(std::fs::File::create(path).unwrap(), flate2::Compression::fast());
        let mut b = tar::Builder::new(gz);
        let mut dir = tar::Header::new_gnu();
        dir.set_entry_type(tar::EntryType::Directory);
        dir.set_mode(0o755);
        dir.set_size(0);
        b.append_data(&mut dir, "llama-b1/", std::io::empty()).unwrap();
        for (name, data, mode) in entries {
            let mut h = tar::Header::new_gnu();
            h.set_size(data.len() as u64);
            h.set_mode(*mode);
            b.append_data(&mut h, name, *data).unwrap();
        }
        for (name, target) in links {
            let mut h = tar::Header::new_gnu();
            h.set_entry_type(tar::EntryType::Symlink);
            h.set_size(0);
            b.append_link(&mut h, name, target).unwrap();
        }
        b.into_inner().unwrap().finish().unwrap();
    }

    #[test]
    fn tar_gz_strips_the_top_folder() {
        let d = scratch("strip");
        let a = d.join("pkg.tar.gz");
        tar_gz(
            &a,
            &[("llama-b1/llama-mtmd-cli", b"bin", 0o755), ("llama-b1/libmtmd.so.0.9.0", b"lib", 0o755), ("llama-b1/sub/x.txt", b"x", 0o644)],
            &[("llama-b1/libmtmd.so.0", "libmtmd.so.0.9.0")],
        );
        extract(&a, &d).unwrap();
        assert_eq!(std::fs::read(d.join("llama-mtmd-cli")).unwrap(), b"bin");
        assert_eq!(std::fs::read(d.join("sub").join("x.txt")).unwrap(), b"x");
        assert!(!d.join("llama-b1").exists());
        assert!(!d.join("llama-mtmd-cli.unzipping").exists());
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let m = std::fs::metadata(d.join("llama-mtmd-cli")).unwrap().permissions().mode();
            assert_eq!(m & 0o111, 0o111);
            assert_eq!(std::fs::read(d.join("libmtmd.so.0")).unwrap(), b"lib");
        }
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn tar_gz_keeps_mixed_top_level_and_skips_unsafe_links() {
        let d = scratch("mixed");
        let a = d.join("pkg.tgz");
        tar_gz(&a, &[("llama-b1/a", b"a", 0o644), ("b", b"b", 0o644)], &[("evil", "../../outside")]);
        extract(&a, &d).unwrap();
        assert!(d.join("llama-b1").join("a").is_file());
        assert!(d.join("b").is_file());
        assert!(d.join("evil").symlink_metadata().is_err());
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn zip_still_extracts_flat() {
        let d = scratch("zip");
        let a = d.join("pkg.zip");
        {
            let mut w = zip::ZipWriter::new(std::fs::File::create(&a).unwrap());
            let o = zip::write::SimpleFileOptions::default().unix_permissions(0o644);
            w.start_file("sd-server", o).unwrap();
            w.write_all(b"srv").unwrap();
            w.finish().unwrap();
        }
        extract(&a, &d).unwrap();
        assert_eq!(std::fs::read(d.join("sd-server")).unwrap(), b"srv");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            // The engine program gets its exec bit even when the archive dropped it.
            let m = std::fs::metadata(d.join("sd-server")).unwrap().permissions().mode();
            assert_eq!(m & 0o111, 0o111);
        }
        let _ = std::fs::remove_dir_all(&d);
    }
}
