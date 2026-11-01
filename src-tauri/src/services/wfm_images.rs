//! warframe.market item art for the webview. The static host sits behind Cloudflare (a plain GET answers 403),
//! so the backend fetches with a clearance cookie earned lazily by `wfm_cookie`, keeps the files on disk and
//! serves them through the `wfmimg://<slug>` URI scheme. Anything that fails answers 404 and the UI falls back.

use std::collections::{HashMap, HashSet};
use std::future::Future;
use std::path::PathBuf;
use std::pin::Pin;
use std::sync::{Arc, Mutex as StdMutex};
use std::time::{Duration, Instant, SystemTime};

use reqwest::{Client, StatusCode, header};
use sha2::{Digest, Sha256};
use tauri::{AppHandle, Manager};
use tokio::sync::{Mutex, Semaphore};

use crate::db::Db;
use crate::services::cache::kv_get;
use crate::services::wfm::types::CatalogItem;

const ASSET_PREFIX: &str = "https://warframe.market/static/assets/";
const ITEMS_KEY: &str = "wfm:v2:items";
const CACHE_CAP_BYTES: u64 = 96 * 1024 * 1024;
const MAX_IMAGE_BYTES: usize = 2 * 1024 * 1024;
const CONCURRENCY: usize = 4;
const ATTEMPTS: u32 = 3;
const FAIL_TTL: Duration = Duration::from_secs(120);
const CATALOG_TTL: Duration = Duration::from_secs(60);

/// Content-addressed (by URL) files under one directory, trimmed oldest-first past the cap.
pub struct DiskCache {
    dir: PathBuf,
    cap: u64,
}

impl DiskCache {
    pub fn new(dir: PathBuf, cap: u64) -> Self {
        let _ = std::fs::create_dir_all(&dir);
        Self { dir, cap }
    }

    fn path_for(&self, url: &str) -> PathBuf {
        let hash = Sha256::digest(url.as_bytes());
        self.dir.join(format!(
            "{}.img",
            hash.iter().map(|b| format!("{b:02x}")).collect::<String>()
        ))
    }

    pub fn get(&self, url: &str) -> Option<Vec<u8>> {
        let path = self.path_for(url);
        let bytes = std::fs::read(&path).ok()?;
        // A read refreshes the mtime so eviction is least-recently-used.
        if let Ok(f) = std::fs::OpenOptions::new().write(true).open(&path) {
            let _ = f.set_modified(SystemTime::now());
        }
        Some(bytes)
    }

    pub fn put(&self, url: &str, bytes: &[u8]) {
        let path = self.path_for(url);
        let tmp = path.with_extension("part");
        if std::fs::write(&tmp, bytes).is_ok() && std::fs::rename(&tmp, &path).is_err() {
            let _ = std::fs::remove_file(&tmp);
        }
        self.evict();
    }

    /// Drops the oldest files until the directory is back under 80% of the cap.
    pub fn evict(&self) {
        let mut files: Vec<(SystemTime, u64, PathBuf)> = std::fs::read_dir(&self.dir)
            .into_iter()
            .flatten()
            .flatten()
            .filter_map(|e| {
                let m = e.metadata().ok()?;
                Some((m.modified().ok()?, m.len(), e.path()))
            })
            .collect();
        let mut total: u64 = files.iter().map(|f| f.1).sum();
        if total <= self.cap {
            return;
        }
        files.sort_by_key(|f| f.0);
        for (_, len, path) in files {
            if total <= self.cap / 10 * 8 {
                break;
            }
            if std::fs::remove_file(path).is_ok() {
                total -= len;
            }
        }
    }
}

/// Only real raster art is served, judged by magic bytes, never by what the server claims.
pub fn sniff(bytes: &[u8]) -> Option<&'static str> {
    match bytes {
        [0x89, b'P', b'N', b'G', ..] => Some("image/png"),
        [0xFF, 0xD8, 0xFF, ..] => Some("image/jpeg"),
        [
            b'R',
            b'I',
            b'F',
            b'F',
            _,
            _,
            _,
            _,
            b'W',
            b'E',
            b'B',
            b'P',
            ..,
        ] => Some("image/webp"),
        _ => None,
    }
}

type CookieFuture = Pin<Box<dyn Future<Output = Result<String, String>> + Send>>;
type CookieFn = Arc<dyn Fn() -> CookieFuture + Send + Sync>;

/// Fetch + cache pipeline, independent of Tauri so tests can point it at a local server.
pub struct Images {
    cache: DiskCache,
    client: Client,
    prefix: String,
    user_agent: String,
    cookie_source: CookieFn,
    cookie: Mutex<Option<String>>,
    permits: Semaphore,
    failed: StdMutex<HashMap<String, Instant>>,
    reported: StdMutex<HashSet<String>>,
    /// When a 403 last found no clearance cookie to earn.
    unclearable: StdMutex<Option<Instant>>,
    backoff: Duration,
}

impl Images {
    pub fn new(
        cache: DiskCache,
        client: Client,
        prefix: &str,
        user_agent: &str,
        cookie_source: CookieFn,
    ) -> Self {
        Self {
            cache,
            client,
            prefix: prefix.to_owned(),
            user_agent: user_agent.to_owned(),
            cookie_source,
            cookie: Mutex::new(None),
            permits: Semaphore::new(CONCURRENCY),
            failed: StdMutex::default(),
            reported: StdMutex::default(),
            unclearable: StdMutex::default(),
            backoff: Duration::from_millis(400),
        }
    }

    /// Logs a failed slug or URL once per session; never ids or tokens.
    fn report(&self, what: &str, reason: &str) {
        if self.reported.lock().unwrap().insert(what.to_owned()) {
            tracing::info!("[wfm-images] {what}: {reason}");
        }
    }

    /// Image bytes and type for an asset URL; `None` for anything off-host, unreachable or not an image.
    pub async fn load(&self, url: &str) -> Option<(Vec<u8>, &'static str)> {
        if !url.starts_with(&self.prefix) || url.contains("..") {
            return None;
        }
        if let Some(bytes) = self.cache.get(url) {
            return sniff(&bytes).map(|ct| (bytes, ct));
        }
        if self
            .failed
            .lock()
            .unwrap()
            .get(url)
            .is_some_and(|t| t.elapsed() < FAIL_TTL)
        {
            return None;
        }
        let _permit = self.permits.acquire().await.ok()?;
        let loaded = match self.download(url).await {
            Ok(bytes) => sniff(&bytes)
                .map(|ct| (bytes, ct))
                .ok_or_else(|| "not an image".to_owned()),
            Err(e) => Err(e),
        }
        .map_err(|e| self.report(url.rsplit('/').next().unwrap_or(url), &e))
        .ok();
        match &loaded {
            Some((bytes, _)) => self.cache.put(url, bytes),
            None => {
                self.failed
                    .lock()
                    .unwrap()
                    .insert(url.to_owned(), Instant::now());
            }
        }
        loaded
    }

    async fn clearance(&self, refresh: bool) -> Option<String> {
        let mut slot = self.cookie.lock().await;
        if refresh {
            *slot = None;
        }
        if slot.is_none() {
            *slot = (self.cookie_source)().await.ok().filter(|c| !c.is_empty());
        }
        slot.clone()
    }

    async fn download(&self, url: &str) -> Result<Vec<u8>, String> {
        let mut refreshed = false;
        let mut cookie = self.cookie.lock().await.clone();
        for attempt in 0..ATTEMPTS {
            let mut req = self
                .client
                .get(url)
                .header(header::USER_AGENT, &self.user_agent);
            if let Some(c) = &cookie {
                req = req.header(header::COOKIE, c);
            }
            let res = req.send().await.map_err(|_| "request failed".to_owned())?;
            match res.status() {
                StatusCode::OK => {
                    let bytes = res
                        .bytes()
                        .await
                        .map_err(|_| "body read failed".to_owned())?;
                    return if bytes.len() <= MAX_IMAGE_BYTES {
                        Ok(bytes.to_vec())
                    } else {
                        Err(format!("too big ({} bytes)", bytes.len()))
                    };
                }
                // No or stale clearance: earn it once per call, then give up rather than loop on a wall.
                StatusCode::FORBIDDEN if !refreshed => {
                    refreshed = true;
                    // A recent miss stands for every URL: no clearance attempt per image, no retry spam.
                    let walled = || {
                        self.unclearable
                            .lock()
                            .unwrap()
                            .is_some_and(|t| t.elapsed() < FAIL_TTL)
                    };
                    cookie = if walled() {
                        None
                    } else {
                        self.clearance(true).await
                    };
                    if cookie.is_none() {
                        *self.unclearable.lock().unwrap() = Some(Instant::now());
                        return Err("403 and no clearance cookie".into());
                    }
                }
                s if s == StatusCode::TOO_MANY_REQUESTS || s.is_server_error() => {
                    tokio::time::sleep(self.backoff * 2u32.pow(attempt)).await;
                }
                s => return Err(format!("http {s}")),
            }
        }
        Err("retries exhausted".into())
    }
}

/// Slugs worth trying for `slug`: callers derive them from item names, which drift from
/// warframe.market's (part with or without "_blueprint", a weapon whose art lives on its set).
pub fn slug_candidates(slug: &str) -> Vec<String> {
    let mut c = vec![slug.to_owned()];
    for suffix in ["_blueprint", "_relic"] {
        match slug.strip_suffix(suffix) {
            Some(base) => c.push(base.to_owned()),
            None => c.push(format!("{slug}{suffix}")),
        }
    }
    c.push(format!("{slug}_set"));
    c
}

/// slug -> thumb URL, from the catalog cache the market already keeps.
pub fn thumb_map(items: Vec<CatalogItem>) -> HashMap<String, String> {
    items
        .into_iter()
        .filter_map(|i| Some((i.slug, i.thumb.or(i.icon)?)))
        .collect()
}

fn read_catalog(db: &Db) -> Vec<CatalogItem> {
    #[derive(serde::Deserialize)]
    struct Stored {
        items: Vec<CatalogItem>,
    }
    kv_get(db, ITEMS_KEY)
        .ok()
        .flatten()
        .and_then(|s| serde_json::from_str::<Stored>(&s).ok())
        .map(|s| s.items)
        .unwrap_or_default()
}

#[derive(Default)]
struct ThumbIndex {
    map: HashMap<String, String>,
    loaded: Option<Instant>,
}

pub struct WfmImages {
    images: Images,
    index: StdMutex<ThumbIndex>,
}

impl WfmImages {
    /// Rereads the catalog cache when a slug is unknown and the copy is older than a minute.
    fn thumb_url(&self, db: &Db, slug: &str) -> Option<String> {
        let mut index = self.index.lock().unwrap();
        let stale = index.loaded.is_none_or(|t| t.elapsed() > CATALOG_TTL);
        if !index.map.contains_key(slug) && stale {
            index.map = thumb_map(read_catalog(db));
            index.loaded = Some(Instant::now());
        }
        slug_candidates(slug)
            .iter()
            .find_map(|c| index.map.get(c).cloned())
    }
}

pub fn setup(app: &tauri::App) -> Result<(), Box<dyn std::error::Error>> {
    let dir = app.path().app_cache_dir()?.join("wfm-images");
    let handle = app.handle().clone();
    let cookie_source: CookieFn = Arc::new(move || {
        let h: AppHandle = handle.clone();
        Box::pin(async move { crate::services::wfm_cookie::asset_clearance_header(&h).await })
    });
    let images = Images::new(
        DiskCache::new(dir, CACHE_CAP_BYTES),
        Client::builder()
            .https_only(true)
            .connect_timeout(Duration::from_secs(10))
            .timeout(Duration::from_secs(20))
            .build()?,
        ASSET_PREFIX,
        crate::services::wfm_cookie::CLEARANCE_UA,
        cookie_source,
    );
    app.manage(WfmImages {
        images,
        index: StdMutex::default(),
    });
    Ok(())
}

/// Handler of the `wfmimg` scheme: the path is the catalog slug.
pub fn respond(
    app: AppHandle,
    request: tauri::http::Request<Vec<u8>>,
    reply: tauri::UriSchemeResponder,
) {
    let slug = request.uri().path().trim_matches('/').to_owned();
    tauri::async_runtime::spawn(async move {
        let found = async {
            let state = app.try_state::<WfmImages>()?;
            let Some(url) = state.thumb_url(&app.state::<Db>(), &slug) else {
                state.images.report(&slug, "unknown slug");
                return None;
            };
            state.images.load(&url).await
        }
        .await;
        let response = match found {
            Some((bytes, ct)) => tauri::http::Response::builder()
                .header("Content-Type", ct)
                .header("Cache-Control", "max-age=86400")
                .header("Access-Control-Allow-Origin", "*")
                .body(bytes),
            None => tauri::http::Response::builder()
                .status(404)
                .body(Vec::new()),
        };
        reply.respond(response.expect("static response parts"));
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    const PNG: &[u8] = &[0x89, b'P', b'N', b'G', 1, 2, 3];

    fn tmp() -> PathBuf {
        std::env::temp_dir().join(format!("cf-img-{}", uuid::Uuid::new_v4()))
    }

    fn item(slug: &str, thumb: Option<&str>) -> CatalogItem {
        serde_json::from_value(serde_json::json!({
            "id": slug, "slug": slug, "name": slug, "thumb": thumb, "icon": null, "maxRank": null, "gameRef": null
        }))
        .unwrap()
    }

    #[test]
    fn slug_maps_to_thumb_and_skips_art_less_items() {
        let m = thumb_map(vec![
            item("serration", Some("https://x/a.png")),
            item("bare", None),
        ]);
        assert_eq!(
            m.get("serration").map(String::as_str),
            Some("https://x/a.png")
        );
        assert!(!m.contains_key("bare"));
    }

    #[test]
    fn candidates_bridge_name_drift() {
        let c = slug_candidates("ash_prime_chassis_blueprint");
        assert!(c.contains(&"ash_prime_chassis".to_owned()));
        assert!(slug_candidates("soma_prime").contains(&"soma_prime_set".to_owned()));
        assert!(slug_candidates("lith_a1").contains(&"lith_a1_relic".to_owned()));
        assert!(slug_candidates("lith_a1_relic").contains(&"lith_a1".to_owned()));
        assert_eq!(slug_candidates("serration")[0], "serration");
    }

    #[test]
    fn cache_path_is_a_stable_hash_and_roundtrips() {
        let c = DiskCache::new(tmp(), 1 << 20);
        assert_eq!(c.path_for("u"), c.path_for("u"));
        assert_ne!(c.path_for("u"), c.path_for("v"));
        c.put("u", PNG);
        assert_eq!(c.get("u").unwrap(), PNG);
        assert!(c.get("v").is_none());
    }

    #[test]
    fn eviction_drops_the_least_recently_used() {
        let c = DiskCache::new(tmp(), 100);
        c.put("old", &[0; 50]);
        std::thread::sleep(Duration::from_millis(30));
        c.put("mid", &[0; 40]);
        std::thread::sleep(Duration::from_millis(30));
        c.get("old"); // touched: now the newest
        std::thread::sleep(Duration::from_millis(30));
        c.put("new", &[0; 30]); // 120 > 100: trims the oldest down to 80
        assert!(c.get("mid").is_none());
        assert!(c.get("old").is_some());
        assert!(c.get("new").is_some());
    }

    #[test]
    fn only_images_pass_the_sniff() {
        assert_eq!(sniff(PNG), Some("image/png"));
        assert_eq!(sniff(b"<html>challenge</html>"), None);
    }

    /// 403 without the clearance cookie, the PNG with it; counts requests.
    async fn server(hits: Arc<AtomicUsize>) -> String {
        let l = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = l.local_addr().unwrap();
        tokio::spawn(async move {
            loop {
                let (mut s, _) = l.accept().await.unwrap();
                let hits = hits.clone();
                tokio::spawn(async move {
                    let mut buf = vec![0; 4096];
                    let n = s.read(&mut buf).await.unwrap_or(0);
                    hits.fetch_add(1, Ordering::SeqCst);
                    let req = String::from_utf8_lossy(&buf[..n]).to_lowercase();
                    let ok = req.contains("cf_clearance=ok");
                    let head = if ok {
                        format!(
                            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                            PNG.len()
                        )
                    } else {
                        "HTTP/1.1 403 Forbidden\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
                            .into()
                    };
                    let _ = s.write_all(head.as_bytes()).await;
                    if ok {
                        let _ = s.write_all(PNG).await;
                    }
                });
            }
        });
        format!("http://{addr}/static/assets/")
    }

    fn images(prefix: &str, cookie: &'static str) -> Images {
        let source: CookieFn = Arc::new(move || Box::pin(async move { Ok(cookie.to_owned()) }));
        Images::new(
            DiskCache::new(tmp(), 1 << 20),
            Client::new(),
            prefix,
            "test",
            source,
        )
    }

    #[tokio::test]
    async fn a_403_earns_the_cookie_then_the_file_is_cached() {
        let hits = Arc::new(AtomicUsize::new(0));
        let prefix = server(hits.clone()).await;
        let imgs = images(&prefix, "cf_clearance=ok");
        let url = format!("{prefix}a.png");
        assert_eq!(imgs.load(&url).await.unwrap().1, "image/png");
        let first = hits.load(Ordering::SeqCst);
        assert_eq!(first, 2); // 403 without the cookie, 200 with it
        assert!(imgs.load(&url).await.is_some());
        assert_eq!(hits.load(Ordering::SeqCst), first); // served from disk
    }

    #[tokio::test]
    async fn no_clearance_falls_back_and_is_not_retried_at_once() {
        let hits = Arc::new(AtomicUsize::new(0));
        let prefix = server(hits.clone()).await;
        let imgs = images(&prefix, "cf_clearance=nope");
        let url = format!("{prefix}a.png");
        assert!(imgs.load(&url).await.is_none());
        let n = hits.load(Ordering::SeqCst);
        assert!(imgs.load(&url).await.is_none());
        assert_eq!(hits.load(Ordering::SeqCst), n);
    }

    #[tokio::test]
    async fn off_host_urls_are_refused() {
        let imgs = images("https://warframe.market/static/assets/", "x");
        assert!(imgs.load("https://evil.example/a.png").await.is_none());
        assert!(
            imgs.load("https://warframe.market/static/assets/../x")
                .await
                .is_none()
        );
    }
}
