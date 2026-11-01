//! Downloads the pinned `warframe-api-helper` release and refuses anything whose SHA-256 is not
//! in the pin set. The helper runs with the user's rights next to their game, so the pin is the
//! whole trust story: bump it only after auditing a new release.

use std::path::{Path, PathBuf};

use reqwest::Client;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use super::zip::extract_single_entry;
use crate::services::atomic::write_atomic;

pub const RELEASE_TAG: &str = "1.1.1";
pub const RELEASE_URL: &str =
    "https://api.github.com/repos/Sainan/warframe-api-helper/releases/tags/1.1.1";
pub const EXE_NAME: &str = if cfg!(windows) {
    "warframe-api-helper.exe"
} else {
    "warframe-api-helper"
};

/// Accepted helper binary SHA-256s (lowercase hex); anything else is refused.
pub const PINNED_SHA256: [&str; 2] = [
    // 1.1.1 (tag on 'senpai' branch) - verified 2026-04-18
    "3f883abb1226c9da6d6cb9c2d6675d3daa6b321a192583c646ef8c45cbd5b8f6",
    // 1.1.1 Linux.zip inner `warframe-api-helper` ELF - user-approved pin 2026-07-21
    "971b9c00cb95a8cabfc0c3f2ce9d6422f6380c3d83dc380fb9a9df18535b1a5a",
];

const MAX_RELEASE_METADATA_BYTES: usize = 1024 * 1024;
const MAX_DOWNLOAD_BYTES: usize = 128 * 1024 * 1024;

pub fn sha256_hex(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

pub fn is_pinned(hash: &str, pins: &[&str]) -> bool {
    pins.iter().any(|p| p.eq_ignore_ascii_case(hash))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "lowercase")]
pub enum DownloadStage {
    Resolving,
    Downloading,
    Done,
    Error,
}

/// Payload of the `helper-download-progress` event.
#[derive(Clone, Debug, PartialEq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct HelperDownloadProgress {
    pub stage: DownloadStage,
    /// 0-100.
    pub percent: u32,
    pub bytes_received: f64,
    pub bytes_total: f64,
    pub error: Option<String>,
}

impl HelperDownloadProgress {
    pub fn stage(stage: DownloadStage) -> Self {
        Self {
            stage,
            percent: 0,
            bytes_received: 0.0,
            bytes_total: 0.0,
            error: None,
        }
    }

    pub fn error(mut self, message: String) -> Self {
        self.error = Some(message);
        self
    }
}

#[derive(Deserialize)]
struct Release {
    tag_name: String,
    assets: Vec<Asset>,
}

#[derive(Deserialize)]
struct Asset {
    name: String,
    browser_download_url: String,
    size: u64,
}

async fn get_capped(
    client: &Client,
    url: &str,
    max: usize,
    mut on_chunk: impl FnMut(usize, Option<u64>),
) -> Result<Vec<u8>, String> {
    let mut res = client
        .get(url)
        .send()
        .await
        .map_err(|e| e.without_url().to_string())?;
    if !res.status().is_success() {
        return Err(format!("HTTP {}", res.status()));
    }
    let total = res.content_length();
    if total.is_some_and(|t| t > max as u64) {
        return Err(format!("download exceeds {} MiB", max / 1024 / 1024));
    }
    let mut body = Vec::new();
    while let Some(chunk) = res.chunk().await.map_err(|e| e.without_url().to_string())? {
        if body.len() + chunk.len() > max {
            return Err(format!("download exceeds {} MiB", max / 1024 / 1024));
        }
        body.extend_from_slice(&chunk);
        on_chunk(body.len(), total);
    }
    Ok(body)
}

fn magic_ok(binary: &[u8], windows: bool) -> bool {
    if windows {
        binary.starts_with(b"MZ")
    } else {
        binary.starts_with(&[0x7f, b'E', b'L', b'F'])
    }
}

/// Fetches the release at `release_url`, checks its tag, downloads the platform asset, verifies
/// it is a native executable whose hash is in `pins`, and installs it atomically as
/// `<dir>/<EXE_NAME>`.
pub async fn download(
    client: &Client,
    release_url: &str,
    pins: &[&str],
    dir: &Path,
    windows: bool,
    on_progress: &(dyn Fn(HelperDownloadProgress) + Sync),
) -> Result<PathBuf, String> {
    on_progress(HelperDownloadProgress::stage(DownloadStage::Resolving));
    let meta = get_capped(client, release_url, MAX_RELEASE_METADATA_BYTES, |_, _| {})
        .await
        .map_err(|e| format!("GitHub release lookup failed: {e}"))?;
    let release: Release =
        serde_json::from_slice(&meta).map_err(|e| format!("bad release metadata: {e}"))?;
    if release.tag_name != RELEASE_TAG {
        return Err(format!(
            "release tag {} is not the pinned {RELEASE_TAG}",
            release.tag_name
        ));
    }
    let asset = release
        .assets
        .iter()
        .find(|a| {
            let n = a.name.to_lowercase();
            if windows {
                n.ends_with(".exe")
            } else {
                n.starts_with("linux") && n.ends_with(".zip")
            }
        })
        .ok_or_else(|| {
            format!(
                "No {} asset found in release",
                if windows { ".exe" } else { "Linux .zip" }
            )
        })?;

    tracing::info!(asset = %asset.name, size = asset.size, "downloading helper");
    let total = asset.size as f64;
    let last_pct = std::sync::atomic::AtomicU32::new(u32::MAX);
    let bytes = get_capped(
        client,
        &asset.browser_download_url,
        MAX_DOWNLOAD_BYTES,
        |received, header_total| {
            let t = header_total.map_or(total, |t| t as f64);
            let percent = if t > 0.0 {
                ((received as f64 / t) * 100.0).round().min(100.0) as u32
            } else {
                0
            };
            // One event per percent, not per network chunk.
            if last_pct.swap(percent, std::sync::atomic::Ordering::Relaxed) != percent {
                on_progress(HelperDownloadProgress {
                    stage: DownloadStage::Downloading,
                    percent,
                    bytes_received: received as f64,
                    bytes_total: t,
                    error: None,
                });
            }
        },
    )
    .await
    .map_err(|e| format!("Download failed: {e}"))?;

    let binary = if windows {
        bytes
    } else {
        extract_single_entry(&bytes)?
    };
    if !magic_ok(&binary, windows) {
        return Err(if windows {
            "Downloaded file is not a valid PE executable (missing MZ header)".into()
        } else {
            "Downloaded file is not a valid ELF executable".into()
        });
    }
    let hash = sha256_hex(&binary);
    if !is_pinned(&hash, pins) {
        return Err(format!(
            "Refusing helper: SHA-256 {hash} not in pin set. Upstream release may have changed - \
             bump PINNED_SHA256 after audit."
        ));
    }
    tracing::info!(sha256 = %hash, "helper pin verified");

    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let dest = dir.join(EXE_NAME);
    write_atomic(&dest, &binary).map_err(|e| e.to_string())?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&dest, std::fs::Permissions::from_mode(0o755))
            .map_err(|e| e.to_string())?;
    }
    on_progress(HelperDownloadProgress {
        stage: DownloadStage::Done,
        percent: 100,
        bytes_received: total,
        bytes_total: total,
        error: None,
    });
    Ok(dest)
}
