//! New-version notice: one anonymous request to GitHub at startup asks for the latest
//! release tag (nothing about the PC, the prompts or the images is sent). Off in Settings.

use std::sync::mpsc::Sender;
use std::time::Duration;

/// Where the notice sends the user.
pub const RELEASES_URL: &str = "https://github.com/hid276/rust-image-studio/releases/latest";
const API_URL: &str = "https://api.github.com/repos/hid276/rust-image-studio/releases/latest";

/// "v1.2.3" or "1.2.3" as numbers; anything else (a pre-release suffix, an engine tag) → None.
pub fn parse_version(s: &str) -> Option<(u32, u32, u32)> {
    let mut parts = s.trim().trim_start_matches('v').split('.').map(|p| p.parse::<u32>().ok());
    let v = (parts.next()??, parts.next()??, parts.next()??);
    parts.next().is_none().then_some(v)
}

/// Whether release tag `latest` is a newer version than `current`.
pub fn newer(latest: &str, current: &str) -> bool {
    matches!((parse_version(latest), parse_version(current)), (Some(l), Some(c)) if l > c)
}

/// Looks up the latest release in the background and sends its tag when it is newer than
/// this build. Offline or blocked: nothing happens. Self-tests (`QIS_SHOT_DIR`) never go
/// online; `QIS_FAKE_LATEST` stands in for the answer there.
pub fn check(tx: Sender<String>, ctx: egui::Context) {
    std::thread::spawn(move || {
        let tag = if std::env::var_os("QIS_SHOT_DIR").is_some() { std::env::var("QIS_FAKE_LATEST").ok() } else { latest_tag() };
        if let Some(tag) = tag.filter(|t| newer(t, env!("CARGO_PKG_VERSION"))) {
            let _ = tx.send(tag);
            ctx.request_repaint();
        }
    });
}

fn latest_tag() -> Option<String> {
    let agent: ureq::Agent = ureq::Agent::config_builder().timeout_global(Some(Duration::from_secs(10))).build().into();
    let mut resp = agent
        .get(API_URL)
        .header("User-Agent", concat!("RustImageStudio/", env!("CARGO_PKG_VERSION")))
        .header("Accept", "application/vnd.github+json")
        .call()
        .ok()?;
    let v: serde_json::Value = resp.body_mut().read_json().ok()?;
    v.get("tag_name")?.as_str().map(str::to_owned)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The real request (needs the network): `cargo test latest_tag_online -- --ignored`.
    #[test]
    #[ignore]
    fn latest_tag_online() {
        let tag = latest_tag().expect("GitHub answered with a release tag");
        assert!(parse_version(&tag).is_some(), "unexpected tag {tag}");
    }

    #[test]
    fn version_compare() {
        assert_eq!(parse_version("v1.2.3"), Some((1, 2, 3)));
        assert_eq!(parse_version("1.10.0"), Some((1, 10, 0)));
        assert_eq!(parse_version("engine-sd-master-1052-f5df5f8"), None);
        assert_eq!(parse_version("v1.2"), None);
        assert_eq!(parse_version("v1.2.3-beta"), None);
        assert!(newer("v1.2.3", "1.2.2"));
        assert!(newer("v1.10.0", "1.9.9"));
        assert!(!newer("v1.2.2", "1.2.2"));
        assert!(!newer("v1.2.1", "1.2.2"));
        assert!(!newer("nonsense", "1.2.2"));
        // The build's own version always parses, or no notice could ever show.
        assert!(parse_version(env!("CARGO_PKG_VERSION")).is_some());
    }
}
