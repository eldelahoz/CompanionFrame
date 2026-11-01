//! What one acquisition run does: the pinned helper exe first, the native game-memory read
//! (`game_memory::read_game_authz`) when the helper is missing or misses the token. Either way the
//! auth string goes only to DE's own endpoint (`authz::fetch_inventory`).

use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, UNIX_EPOCH};

use regex::Regex;
use reqwest::Client;
use zeroize::Zeroizing;

use super::download::{
    self, EXE_NAME, HelperDownloadProgress, PINNED_SHA256, RELEASE_URL, is_pinned, sha256_hex,
};
use super::runner::{Backend, BoxFuture, RunResult};
use super::{HelperRunReason, authz, helper_failure_reason, should_use_native_fallback};
use crate::services::game_memory::read_game_authz;

/// Hard kill the helper if it hasn't exited after this long. Normal runs are <5s.
const SPAWN_TIMEOUT: Duration = Duration::from_secs(60);
const INVENTORY_FILE: &str = "inventory.json";

pub struct Acquire {
    dir: PathBuf,
    exe: Mutex<Option<PathBuf>>,
    client: Client,
    /// For the inventory call, whose URL carries the authz: never follows redirects.
    authz_client: Client,
    ee_log: Box<dyn Fn() -> Option<PathBuf> + Send + Sync>,
    /// Hands a freshly fetched `inventory.json` to the inventory core.
    on_inventory: Box<dyn Fn(&Path) + Send + Sync>,
}

impl Acquire {
    pub fn new(
        dir: PathBuf,
        ee_log: Box<dyn Fn() -> Option<PathBuf> + Send + Sync>,
        on_inventory: Box<dyn Fn(&Path) + Send + Sync>,
    ) -> reqwest::Result<Self> {
        Ok(Self {
            dir,
            exe: Mutex::default(),
            client: crate::services::net::https_client(true)?,
            authz_client: crate::services::net::https_client(false)?,
            ee_log,
            on_inventory,
        })
    }

    fn inventory_path(&self) -> PathBuf {
        self.dir.join(INVENTORY_FILE)
    }

    /// The helper, only if its bytes hash to a pinned value.
    fn find_exe(&self) -> Option<PathBuf> {
        let p = self.dir.join(EXE_NAME);
        let hash = sha256_hex(&std::fs::read(&p).ok()?);
        if is_pinned(&hash, &PINNED_SHA256) {
            return Some(p);
        }
        tracing::warn!(path = %p.display(), sha256 = %hash, "refusing helper: hash not in the pin set");
        None
    }

    pub async fn download(
        &self,
        on_progress: &(dyn Fn(HelperDownloadProgress) + Sync),
    ) -> Result<(), String> {
        let path = download::download(
            &self.client,
            RELEASE_URL,
            &PINNED_SHA256,
            &self.dir,
            cfg!(windows),
            on_progress,
        )
        .await?;
        *self.exe.lock().unwrap() = Some(path);
        Ok(())
    }

    async fn fetch(&self, authz: &str) -> RunResult {
        match authz::fetch_inventory(
            &self.authz_client,
            &authz::HOSTS,
            authz,
            &self.inventory_path(),
        )
        .await
        {
            Ok(()) => {
                if let Some(data) = self.dir.parent() {
                    crate::services::profile::note_authz(data, authz);
                }
                (self.on_inventory)(&self.inventory_path());
                (true, None)
            }
            Err(_) => (false, Some(HelperRunReason::ApiFailed)),
        }
    }

    async fn classify(&self) -> HelperRunReason {
        let log = (self.ee_log)();
        tokio::task::spawn_blocking(move || authz::classify_not_logged_in(log.as_deref()))
            .await
            .unwrap_or(HelperRunReason::NotLoggedIn)
    }

    /// Runs the helper and fetches with the auth it prints. Re-hashes at spawn time to close the
    /// gap between discovery and use.
    async fn run_helper(&self, exe: &Path) -> RunResult {
        let pinned = std::fs::read(exe).is_ok_and(|b| is_pinned(&sha256_hex(&b), &PINNED_SHA256));
        if !pinned {
            tracing::error!("helper hash changed since discovery - refusing to spawn");
            *self.exe.lock().unwrap() = None;
            return (false, Some(HelperRunReason::Error));
        }
        tracing::info!("running warframe-api-helper");
        let mut cmd = tokio::process::Command::new(exe);
        cmd.current_dir(&self.dir)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .kill_on_drop(true);
        #[cfg(windows)]
        cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
        let child = match cmd.spawn() {
            Ok(c) => c,
            Err(e) => {
                tracing::error!(error = %e, "helper spawn failed");
                return (false, Some(HelperRunReason::Error));
            }
        };
        let out = match tokio::time::timeout(SPAWN_TIMEOUT, child.wait_with_output()).await {
            Ok(Ok(o)) => o,
            Ok(Err(e)) => {
                tracing::error!(error = %e, "helper wait failed");
                return (false, Some(HelperRunReason::Error));
            }
            Err(_) => {
                tracing::warn!("helper did not exit in time - killed");
                return (false, Some(HelperRunReason::Error));
            }
        };
        // The helper prints `?accountId=...&nonce=...` to stdout/stderr, even after its own API
        // request failed, so the exit code only matters when no auth is found.
        let printed = Zeroizing::new(
            [out.stdout, out.stderr]
                .map(|b| String::from_utf8_lossy(&b).into_owned())
                .concat(),
        );
        let code = out.status.code();
        if code != Some(0) {
            tracing::warn!(?code, "helper exited with a non-zero code");
        }
        let re = Regex::new(r"(?i)\?accountId=[a-f0-9]+&nonce=\d+").unwrap();
        let Some(m) = re.find(&printed) else {
            let mapped = helper_failure_reason(code);
            let reason = if mapped == HelperRunReason::NotLoggedIn {
                self.classify().await
            } else {
                mapped
            };
            tracing::error!(?reason, "helper output did not contain auth params");
            return (false, Some(reason));
        };
        self.fetch(&Zeroizing::new(m.as_str().to_owned())).await
    }

    /// The native game-memory read. The auth never leaves this function except into `fetch`.
    async fn run_native(&self) -> RunResult {
        let read = match tokio::task::spawn_blocking(read_game_authz).await {
            Ok(r) => r,
            Err(_) => return (false, Some(HelperRunReason::Error)),
        };
        if let Some(a) = &read.authz {
            tracing::info!(hits = read.hits, "auth acquired from game memory");
            return self.fetch(&a.query()).await;
        }
        let log = (self.ee_log)();
        let status = read.status;
        let reason =
            tokio::task::spawn_blocking(move || authz::native_reason(&status, log.as_deref()))
                .await
                .unwrap_or(HelperRunReason::Error);
        match reason {
            HelperRunReason::AccessDenied => tracing::error!(
                "Warframe is likely running as administrator and cannot be read; restart it without admin rights"
            ),
            HelperRunReason::TokenNotFound => tracing::error!(
                "EE.log shows a login, so the game is logged in, but the auth token was not found in game memory"
            ),
            _ => {}
        }
        (false, Some(reason))
    }
}

impl Backend for Acquire {
    fn game_running(&self) -> Option<bool> {
        #[cfg(windows)]
        return Some(crate::platform::windows::process::warframe_running());
        #[cfg(not(windows))]
        None
    }

    fn inventory_mtime_ms(&self) -> Option<f64> {
        let m = std::fs::metadata(self.inventory_path())
            .ok()?
            .modified()
            .ok()?;
        Some(m.duration_since(UNIX_EPOCH).ok()?.as_secs_f64() * 1000.0)
    }

    fn exe_found(&self) -> bool {
        // Linux reads game memory directly, so the "helper" is always available there.
        !cfg!(windows) || self.exe.lock().unwrap().is_some()
    }

    fn refresh(&self) {
        *self.exe.lock().unwrap() = self.find_exe();
    }

    fn run_once(&self) -> BoxFuture<'_, RunResult> {
        Box::pin(async move {
            // Runs are scheduled ahead; the mode may have been switched off since.
            if !crate::services::memory_gate::allowed() {
                return (false, Some(HelperRunReason::Error));
            }
            let exe = self.exe.lock().unwrap().clone().or_else(|| self.find_exe());
            *self.exe.lock().unwrap() = exe.clone();
            if let Some(exe) = &exe {
                let (ok, reason) = self.run_helper(exe).await;
                if ok || !should_use_native_fallback(true, reason) {
                    return (ok, reason);
                }
                tracing::warn!("helper did not find the login token - trying the native scan");
            }
            self.run_native().await
        })
    }
}
