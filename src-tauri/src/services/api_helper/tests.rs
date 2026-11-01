use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering::SeqCst};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use sha2::{Digest, Sha256};
use tokio::runtime::Handle;

use super::HelperRunReason::*;
use super::download::{self, DownloadStage, HelperDownloadProgress};
use super::runner::{Backend, BoxFuture, RunResult, Runner};
use super::*;
use crate::services::eelog::tests::tmp_dir;
use crate::services::game_memory::GameAuthzStatus;

const INTERVAL: Duration = Duration::from_secs(600);
const RETRY: Duration = Duration::from_secs(90);

// ---- reasons ----

#[test]
fn maps_the_helpers_documented_exit_codes() {
    assert_eq!(helper_failure_reason(Some(1)), GameNotRunning);
    assert_eq!(helper_failure_reason(Some(2)), AccessDenied);
    assert_eq!(helper_failure_reason(Some(3)), NotLoggedIn);
}

#[test]
fn unknown_exit_codes_are_generic_errors() {
    for code in [Some(0), Some(5), None] {
        assert_eq!(helper_failure_reason(code), Error);
    }
}

#[test]
fn poll_delay_is_fast_only_for_local_failures() {
    let i = INTERVAL.as_millis() as u64;
    assert_eq!(next_poll_delay_ms(None, i), i);
    for r in [GameNotRunning, AccessDenied, NotLoggedIn] {
        assert_eq!(next_poll_delay_ms(Some(r), i), 90_000);
    }
    for r in [ApiFailed, Error, TokenNotFound] {
        assert_eq!(next_poll_delay_ms(Some(r), i), i);
    }
    // never slower than the configured interval
    assert_eq!(next_poll_delay_ms(Some(NotLoggedIn), 30_000), 30_000);
}

#[test]
fn login_retries_only_what_a_completed_login_can_fix() {
    assert!(should_retry_after_game_login(false, None, None));
    assert!(should_retry_after_game_login(
        false,
        Some(false),
        Some(GameNotRunning)
    ));
    assert!(should_retry_after_game_login(
        false,
        Some(false),
        Some(NotLoggedIn)
    ));
    assert!(should_retry_after_game_login(
        false,
        Some(false),
        Some(TokenNotFound)
    ));
    assert!(!should_retry_after_game_login(
        false,
        Some(false),
        Some(ApiFailed)
    ));
    assert!(!should_retry_after_game_login(
        false,
        Some(false),
        Some(AccessDenied)
    ));
    assert!(!should_retry_after_game_login(false, Some(true), None));
    // queues behind an in-flight poll, which then rechecks its result
    assert!(should_retry_after_game_login(true, Some(true), None));
}

#[test]
fn native_fallback_when_the_helper_is_absent_or_misses_the_token() {
    assert!(should_use_native_fallback(false, None));
    assert!(should_use_native_fallback(true, Some(TokenNotFound)));
    assert!(should_use_native_fallback(true, Some(Error)));
    for r in [ApiFailed, AccessDenied, GameNotRunning, NotLoggedIn] {
        assert!(!should_use_native_fallback(true, Some(r)));
    }
}

#[test]
fn native_scan_status_maps_to_a_reason() {
    let dir = tmp_dir("api-helper-native");
    let logged_in = dir.join("in.log");
    std::fs::write(&logged_in, "Sys [Info]: Logging in as Tenno\n").unwrap();
    let login_screen = dir.join("out.log");
    std::fs::write(&login_screen, "still at the login screen\n").unwrap();
    let map = |s, p: Option<&Path>| authz::native_reason(&s, p);
    assert_eq!(map(GameAuthzStatus::ProcessNotFound, None), GameNotRunning);
    assert_eq!(map(GameAuthzStatus::AccessDenied, None), AccessDenied);
    assert_eq!(map(GameAuthzStatus::Unavailable, None), Error);
    assert_eq!(map(GameAuthzStatus::CrumbsNotFound, None), NotLoggedIn);
    assert_eq!(
        map(GameAuthzStatus::CrumbsNotFound, Some(&login_screen)),
        NotLoggedIn
    );
    assert_eq!(
        map(GameAuthzStatus::CrumbsNotFound, Some(&logged_in)),
        TokenNotFound
    );
    assert_eq!(
        map(GameAuthzStatus::CrumbsAmbiguous, Some(&logged_in)),
        TokenNotFound
    );
}

#[test]
fn marker_scan_handles_missing_files_and_chunk_boundaries() {
    let dir = tmp_dir("api-helper-marker");
    let write = |name: &str, body: &str| {
        let p = dir.join(name);
        std::fs::write(&p, body).unwrap();
        p
    };
    let m = authz::EE_LOG_LOGIN_MARKER;
    assert!(authz::file_contains_marker(
        &write("a", "boot\nSys [Info]: Logging in as GK\n"),
        m,
        4096
    ));
    assert!(!authz::file_contains_marker(
        &write("b", "still at the login screen\n"),
        m,
        4096
    ));
    assert!(!authz::file_contains_marker(&dir.join("nope"), "x", 4096));
    assert!(authz::file_contains_marker(
        &write("c", "abcdeMARKERfghij"),
        "MARKER",
        8
    ));
}

#[test]
fn installer_preference_is_read_leniently() {
    let dir = tmp_dir("api-helper-prefs");
    let p = dir.join(PREFERENCES_FILE);
    assert_eq!(read_auto_install_preference(&p), None);
    std::fs::write(&p, r#"{"autoInstallHelper":false}"#).unwrap();
    assert_eq!(read_auto_install_preference(&p), Some(false));
    std::fs::write(&p, r#"{"autoInstallHelper":"yes"}"#).unwrap();
    assert_eq!(read_auto_install_preference(&p), None);
    std::fs::write(&p, "not json").unwrap();
    assert_eq!(read_auto_install_preference(&p), None);
}

// ---- zip ----

fn build_zip(entries: &[(&str, &[u8])], deflate: bool) -> Vec<u8> {
    use ::zip::{CompressionMethod, ZipWriter, write::SimpleFileOptions};
    let mut w = ZipWriter::new(std::io::Cursor::new(Vec::new()));
    let method = if deflate {
        CompressionMethod::Deflated
    } else {
        CompressionMethod::Stored
    };
    for (name, body) in entries {
        w.start_file(
            *name,
            SimpleFileOptions::default().compression_method(method),
        )
        .unwrap();
        w.write_all(body).unwrap();
    }
    w.finish().unwrap().into_inner()
}

#[test]
fn zip_extracts_stored_and_deflated_entries() {
    let stored = b"\x7fELF fake binary";
    assert_eq!(
        zip::extract_single_entry(&build_zip(&[("warframe-api-helper", stored)], false)).unwrap(),
        stored
    );
    let big = b"\x7fELF ".repeat(1000);
    assert_eq!(
        zip::extract_single_entry(&build_zip(&[("warframe-api-helper", &big)], true)).unwrap(),
        big
    );
}

#[test]
fn zip_rejects_multi_entry_and_non_zip_data() {
    let two = build_zip(&[("a", b"x"), ("b", b"y")], false);
    assert!(
        zip::extract_single_entry(&two)
            .unwrap_err()
            .contains("expected exactly 1 entry")
    );
    assert!(
        zip::extract_single_entry(b"not a zip at all")
            .unwrap_err()
            .starts_with("zip:")
    );
}

// ---- polling (virtual time, mocked backend: no process, memory or network) ----

struct Mock {
    running: Mutex<Option<bool>>,
    runs: AtomicUsize,
    result: Mutex<RunResult>,
    mtime: Mutex<Option<f64>>,
}

impl Mock {
    fn new(running: Option<bool>) -> Arc<Self> {
        Arc::new(Self {
            running: Mutex::new(running),
            runs: AtomicUsize::new(0),
            result: Mutex::new((false, Some(GameNotRunning))),
            mtime: Mutex::new(None),
        })
    }

    fn runs(&self) -> usize {
        self.runs.load(SeqCst)
    }
}

impl Backend for Mock {
    fn game_running(&self) -> Option<bool> {
        *self.running.lock().unwrap()
    }

    fn inventory_mtime_ms(&self) -> Option<f64> {
        *self.mtime.lock().unwrap()
    }

    fn exe_found(&self) -> bool {
        false
    }

    fn refresh(&self) {}

    fn run_once(&self) -> BoxFuture<'_, RunResult> {
        self.runs.fetch_add(1, SeqCst);
        let r = *self.result.lock().unwrap();
        Box::pin(async move { r })
    }
}

fn runner(m: &Arc<Mock>) -> Arc<Runner> {
    Runner::new(m.clone(), Handle::current())
}

fn noop() -> Arc<dyn Fn(bool) + Send + Sync> {
    Arc::new(|_| {})
}

/// Advances virtual time and lets the spawned tasks run.
async fn advance(d: Duration) {
    // Spawned tasks register their timers on first poll, so let them run before and after.
    for _ in 0..10 {
        tokio::task::yield_now().await;
    }
    tokio::time::advance(d).await;
    for _ in 0..10 {
        tokio::task::yield_now().await;
    }
}

#[tokio::test(start_paused = true)]
async fn closed_game_settles_scheduled_runs_without_running_anything() {
    let m = Mock::new(Some(false));
    let r = runner(&m);
    r.start_polling(INTERVAL, noop());
    advance(Duration::ZERO).await;
    advance(RETRY * 4).await;
    assert_eq!(m.runs(), 0);
    let s = r.status();
    assert!(!s.running);
    assert_eq!(s.last_run_ok, Some(false));
    assert_eq!(s.last_run_reason, Some(GameNotRunning));
    r.stop_polling();
}

#[tokio::test(start_paused = true)]
async fn runs_on_the_first_poll_after_the_game_starts() {
    let m = Mock::new(Some(false));
    let r = runner(&m);
    r.start_polling(INTERVAL, noop());
    advance(RETRY).await;
    *m.running.lock().unwrap() = Some(true);
    advance(RETRY).await;
    assert_eq!(m.runs(), 1);
    r.stop_polling();
}

#[tokio::test(start_paused = true)]
async fn unknown_game_state_still_runs() {
    let m = Mock::new(None);
    let r = runner(&m);
    r.start_polling(INTERVAL, noop());
    advance(Duration::ZERO).await;
    assert_eq!(m.runs(), 1);
    r.stop_polling();
}

#[tokio::test(start_paused = true)]
async fn game_login_triggers_the_fast_fetch() {
    let m = Mock::new(Some(false));
    let r = runner(&m);
    r.start_polling(INTERVAL, noop());
    advance(Duration::ZERO).await;
    r.run_after_game_login();
    advance(Duration::from_millis(250)).await;
    assert_eq!(m.runs(), 1);
    r.stop_polling();
}

#[tokio::test(start_paused = true)]
async fn game_login_is_ignored_after_a_success() {
    let m = Mock::new(None);
    *m.result.lock().unwrap() = (true, None);
    let r = runner(&m);
    r.start_polling(INTERVAL, noop());
    advance(Duration::ZERO).await;
    r.run_after_game_login();
    advance(Duration::from_millis(250)).await;
    assert_eq!(m.runs(), 1);
    r.stop_polling();
}

#[tokio::test(start_paused = true)]
async fn direct_run_is_not_gated_by_the_game_state() {
    let m = Mock::new(Some(false));
    let r = runner(&m);
    assert!(!r.run_once().await);
    assert_eq!(m.runs(), 1);
    assert_eq!(r.status().last_run_reason, Some(GameNotRunning));
}

#[tokio::test(start_paused = true)]
async fn a_recent_inventory_defers_the_first_run_and_counts_as_a_success() {
    let m = Mock::new(None);
    let now_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs_f64()
        * 1000.0;
    *m.mtime.lock().unwrap() = Some(now_ms - 60_000.0);
    let r = runner(&m);
    r.start_polling(INTERVAL, noop());
    advance(Duration::from_secs(60)).await;
    assert_eq!(m.runs(), 0);
    assert_eq!(r.status().last_run_ok, Some(true));
    advance(Duration::from_secs(480)).await;
    assert_eq!(m.runs(), 1);
    r.stop_polling();
}

#[tokio::test(start_paused = true)]
async fn stop_polling_ends_the_schedule() {
    let m = Mock::new(None);
    let r = runner(&m);
    r.start_polling(INTERVAL, noop());
    advance(Duration::ZERO).await;
    r.stop_polling();
    advance(INTERVAL * 3).await;
    assert_eq!(m.runs(), 1);
}

// ---- local HTTP server: no real downloads or DE calls ----

/// Serves `routes` (path without query -> body) on a free port; returns the base URL and every
/// request target seen (path and query).
fn serve(routes: impl FnOnce(&str) -> Vec<(String, Vec<u8>)>) -> (String, Arc<Mutex<Vec<String>>>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let routes = routes(&base);
    let seen = Arc::new(Mutex::new(Vec::new()));
    let log = seen.clone();
    std::thread::spawn(move || {
        for mut s in listener.incoming().flatten() {
            let mut buf = [0u8; 4096];
            let n = s.read(&mut buf).unwrap_or(0);
            let target = String::from_utf8_lossy(&buf[..n])
                .split_whitespace()
                .nth(1)
                .unwrap_or("")
                .to_owned();
            log.lock().unwrap().push(target.clone());
            let path = target.split('?').next().unwrap();
            let (head, body) = match routes.iter().find(|(p, _)| p == path) {
                Some((_, b)) => ("200 OK", b.as_slice()),
                None => ("404 Not Found", &b""[..]),
            };
            let _ = s.write_all(
                format!(
                    "HTTP/1.1 {head}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    body.len()
                )
                .as_bytes(),
            );
            let _ = s.write_all(body);
        }
    });
    (base, seen)
}

fn http() -> reqwest::Client {
    reqwest::Client::new()
}

fn release_json(base: &str, tag: &str, asset: &str) -> Vec<u8> {
    format!(
        r#"{{"tag_name":"{tag}","assets":[{{"name":"{asset}","browser_download_url":"{base}/asset","size":8}}]}}"#
    )
    .into_bytes()
}

fn hex(b: &[u8]) -> String {
    format!("{:x}", Sha256::digest(b))
}

async fn run_download(
    base: &str,
    pins: &[&str],
    dir: &Path,
    windows: bool,
) -> (
    Result<std::path::PathBuf, String>,
    Vec<HelperDownloadProgress>,
) {
    let events = Mutex::new(Vec::new());
    let r = download::download(
        &http(),
        &format!("{base}/release"),
        pins,
        dir,
        windows,
        &|p| events.lock().unwrap().push(p),
    )
    .await;
    (r, events.into_inner().unwrap())
}

#[tokio::test]
async fn download_installs_a_pinned_exe_and_reports_progress() {
    let exe = b"MZ pretend helper".to_vec();
    let (base, _) = serve(|b| {
        vec![
            (
                "/release".into(),
                release_json(b, "1.1.1", "warframe-api-helper.exe"),
            ),
            ("/asset".into(), exe.clone()),
        ]
    });
    let dir = tmp_dir("api-helper-dl-ok");
    let (r, events) = run_download(&base, &[&hex(&exe)], &dir, true).await;
    assert_eq!(std::fs::read(r.unwrap()).unwrap(), exe);
    assert_eq!(events.first().unwrap().stage, DownloadStage::Resolving);
    assert!(events.iter().any(|e| e.stage == DownloadStage::Downloading));
    let last = events.last().unwrap();
    assert_eq!((last.stage, last.percent), (DownloadStage::Done, 100));
}

#[tokio::test]
async fn download_refuses_an_unpinned_hash_and_installs_nothing() {
    let (base, _) = serve(|b| {
        vec![
            (
                "/release".into(),
                release_json(b, "1.1.1", "warframe-api-helper.exe"),
            ),
            ("/asset".into(), b"MZ tampered".to_vec()),
        ]
    });
    let dir = tmp_dir("api-helper-dl-pin");
    let (r, _) = run_download(&base, &[&hex(b"something else")], &dir, true).await;
    assert!(r.unwrap_err().contains("not in pin set"));
    assert!(!dir.join(download::EXE_NAME).exists());
}

#[tokio::test]
async fn download_refuses_a_different_tag_and_a_non_executable() {
    let (base, _) = serve(|b| {
        vec![
            (
                "/release".into(),
                release_json(b, "9.9.9", "warframe-api-helper.exe"),
            ),
            ("/asset".into(), b"MZ".to_vec()),
        ]
    });
    let dir = tmp_dir("api-helper-dl-tag");
    let (r, _) = run_download(&base, &[&hex(b"MZ")], &dir, true).await;
    assert!(r.unwrap_err().contains("not the pinned"));

    let (base, _) = serve(|b| {
        vec![
            (
                "/release".into(),
                release_json(b, "1.1.1", "warframe-api-helper.exe"),
            ),
            ("/asset".into(), b"<html>not an exe".to_vec()),
        ]
    });
    let (r, _) = run_download(&base, &[&hex(b"<html>not an exe")], &dir, true).await;
    assert!(r.unwrap_err().contains("MZ header"));
}

#[tokio::test]
async fn download_unpacks_the_linux_zip_before_checking_the_pin() {
    let elf = b"\x7fELF fake".to_vec();
    let zipped = build_zip(&[("warframe-api-helper", &elf)], true);
    let (base, _) = serve(|b| {
        vec![
            ("/release".into(), release_json(b, "1.1.1", "Linux.zip")),
            ("/asset".into(), zipped),
        ]
    });
    let dir = tmp_dir("api-helper-dl-linux");
    let (r, _) = run_download(&base, &[&hex(&elf)], &dir, false).await;
    assert_eq!(std::fs::read(r.unwrap()).unwrap(), elf);
}

const AUTHZ: &str = "?accountId=0123456789abcdef01234567&nonce=424242";

fn inventory_body() -> Vec<u8> {
    br#"{"MiscItems":[{"ItemType":"/Lotus/Types/Items/MiscItems/Plastids","ItemCount":3}]}"#
        .to_vec()
}

#[tokio::test]
async fn inventory_fetch_stores_a_valid_reply_and_sends_the_authz_to_the_endpoint() {
    let (base, seen) = serve(|_| vec![("/api/inventory.php".into(), inventory_body())]);
    let dest = tmp_dir("api-helper-fetch").join("inventory.json");
    authz::fetch_inventory(&http(), &[&base], AUTHZ, &dest)
        .await
        .unwrap();
    assert_eq!(std::fs::read(&dest).unwrap(), inventory_body());
    assert_eq!(
        seen.lock().unwrap()[0],
        format!("/api/inventory.php{AUTHZ}")
    );
}

#[tokio::test]
async fn inventory_fetch_falls_back_to_the_next_host() {
    let (bad, _) = serve(|_| vec![]);
    let (good, _) = serve(|_| vec![("/api/inventory.php".into(), inventory_body())]);
    let dest = tmp_dir("api-helper-fetch-2").join("inventory.json");
    authz::fetch_inventory(&http(), &[&bad, &good], AUTHZ, &dest)
        .await
        .unwrap();
    assert!(dest.exists());
}

#[tokio::test]
async fn inventory_fetch_keeps_the_old_file_on_junk_and_never_echoes_the_authz() {
    let (base, _) = serve(|_| {
        vec![(
            "/api/inventory.php".into(),
            b"<html>maintenance</html>".to_vec(),
        )]
    });
    let dest = tmp_dir("api-helper-fetch-3").join("inventory.json");
    std::fs::write(&dest, b"old").unwrap();
    let err = authz::fetch_inventory(&http(), &[&base], AUTHZ, &dest)
        .await
        .unwrap_err();
    assert_eq!(std::fs::read(&dest).unwrap(), b"old");
    assert!(
        !err.contains("0123456789abcdef") && !err.contains("nonce"),
        "{err}"
    );

    // a connection failure's message must not carry the URL either
    let dead = TcpListener::bind("127.0.0.1:0").unwrap();
    let dead_base = format!("http://{}", dead.local_addr().unwrap());
    drop(dead);
    let err = authz::fetch_inventory(&http(), &[&dead_base], AUTHZ, &dest)
        .await
        .unwrap_err();
    assert!(
        !err.contains("accountId") && !err.contains("nonce"),
        "{err}"
    );
}

#[tokio::test]
async fn the_production_client_refuses_plain_http() {
    let (base, seen) = serve(|_| vec![("/api/inventory.php".into(), inventory_body())]);
    let dest = tmp_dir("api-helper-https").join("inventory.json");
    let client = crate::services::net::https_client(false).unwrap();
    assert!(
        authz::fetch_inventory(&client, &[&base], AUTHZ, &dest)
            .await
            .is_err()
    );
    assert!(!dest.exists());
    assert!(
        seen.lock().unwrap().is_empty(),
        "no request may leave over http"
    );
}

struct LogBuf(Arc<Mutex<Vec<u8>>>);

impl Write for LogBuf {
    fn write(&mut self, b: &[u8]) -> std::io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(b);
        Ok(b.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

/// 302 to `to` for every request.
fn redirect_server(to: String) -> String {
    let l = TcpListener::bind("127.0.0.1:0").unwrap();
    let base = format!("http://{}", l.local_addr().unwrap());
    std::thread::spawn(move || {
        for mut s in l.incoming().flatten() {
            let _ = s.read(&mut [0u8; 4096]);
            let _ = s.write_all(
                format!("HTTP/1.1 302 Found\r\nLocation: {to}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n")
                    .as_bytes(),
            );
        }
    });
    base
}

#[tokio::test]
async fn failing_requests_never_leak_the_authz_to_logs_or_errors() {
    let logs = Arc::new(Mutex::new(Vec::new()));
    let sink = logs.clone();
    let _guard = tracing::subscriber::set_default(
        tracing_subscriber::fmt()
            .with_writer(move || LogBuf(sink.clone()))
            .with_max_level(tracing::Level::TRACE)
            .finish(),
    );
    let dest = tmp_dir("api-helper-leak").join("inventory.json");
    let (junk, _) = serve(|_| vec![("/api/inventory.php".into(), b"nope".to_vec())]);
    let (missing, _) = serve(|_| vec![]);
    let (target, hits) = serve(|_| vec![("/api/inventory.php".into(), inventory_body())]);
    let redirecting = redirect_server(format!("{target}/api/inventory.php{AUTHZ}"));
    let dead = TcpListener::bind("127.0.0.1:0").unwrap();
    let dead_base = format!("http://{}", dead.local_addr().unwrap());
    drop(dead);

    let no_redirects = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .unwrap();
    let mut errors = Vec::new();
    for base in [&junk, &missing, &dead_base, &redirecting] {
        errors.push(
            authz::fetch_inventory(&no_redirects, &[base], AUTHZ, &dest)
                .await
                .unwrap_err(),
        );
    }
    // Both hosts failing: the combined path returns the last error only.
    errors.push(
        authz::fetch_inventory(&no_redirects, &[&junk, &dead_base], AUTHZ, &dest)
            .await
            .unwrap_err(),
    );

    let text = format!(
        "{}\n{}",
        errors.join("\n"),
        String::from_utf8_lossy(&logs.lock().unwrap())
    );
    for secret in ["0123456789abcdef01234567", "424242", "accountId", "nonce"] {
        assert!(!text.contains(secret), "{secret} leaked: {text}");
    }
    assert!(!dest.exists());
    assert!(
        hits.lock().unwrap().is_empty(),
        "the redirect must not be followed"
    );
}
