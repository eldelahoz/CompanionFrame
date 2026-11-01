//! DE's live PublicExport as an overlay on the bundled data, so new items do not wait for a package release.
//!
//! DE serves a tiny LZMA index of hash-suffixed manifest names, then plain-JSON manifests under `Manifest/`.
//! The trimmed result is cached in `kv_cache`; a failed refresh falls back to that copy.

use std::collections::BTreeMap;
use std::io::Cursor;

use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::db::Db;
use crate::services::cache::{kv_get, kv_put};
use crate::services::net::{self, Fetched};

pub(crate) const CACHE_KEY: &str = "public_export";
/// The image manifest is several MB.
const MAX_BYTES: usize = 64 << 20;
/// DE's item exports omit icon paths; this manifest maps uniqueName -> texture.
const IMAGE_MANIFEST: &str = "ExportManifest.json";

pub struct Source {
    pub index_url: String,
    pub manifest_base: String,
}

impl Default for Source {
    fn default() -> Self {
        Self {
            index_url: "https://content.warframe.com/PublicExport/index_en.txt.lzma".into(),
            manifest_base: "https://content.warframe.com/PublicExport/Manifest/".into(),
        }
    }
}

/// One DE manifest feeds several package exports: weapons carry railjack armaments,
/// and only `relicRewards` separates relics from arcanes in their shared file.
struct Spec {
    file: &'static str,
    array_key: &'static str,
    keys: &'static [&'static str],
}

macro_rules! plain {
    ($file:literal, $key:literal) => {
        Spec {
            file: $file,
            array_key: $key,
            keys: &[$key],
        }
    };
}

const SPECS: &[Spec] = &[
    plain!("ExportWarframes_en.json", "ExportWarframes"),
    plain!("ExportWeapons_en.json", "ExportWeapons"),
    plain!("ExportWeapons_en.json", "ExportRailjackWeapons"),
    plain!("ExportSentinels_en.json", "ExportSentinels"),
    plain!("ExportUpgrades_en.json", "ExportUpgrades"),
    plain!("ExportResources_en.json", "ExportResources"),
    Spec {
        file: "ExportRelicArcane_en.json",
        array_key: "ExportRelicArcane",
        keys: &["ExportRelics", "ExportArcanes"],
    },
    plain!("ExportRecipes_en.json", "ExportRecipes"),
    plain!("ExportGear_en.json", "ExportGear"),
    plain!("ExportKeys_en.json", "ExportKeys"),
    plain!("ExportDrones_en.json", "ExportDrones"),
    plain!("ExportFusionBundles_en.json", "ExportFusionBundles"),
    plain!("ExportCustoms_en.json", "ExportCustoms"),
    plain!("ExportFlavour_en.json", "ExportFlavour"),
];

impl Spec {
    fn target(&self, item: &Value) -> &'static str {
        match self.keys {
            [relics, _] if item["relicRewards"].is_array() => relics,
            [_, arcanes] => arcanes,
            [only] => only,
            _ => unreachable!("specs carry one or two keys"),
        }
    }
}

const KEPT_FIELDS: &[&str] = &[
    "uniqueName",
    "name",
    "description",
    "icon",
    "masteryReq",
    "primeSellingPrice",
    "tradable",
    "vaulted",
    "productCategory",
    "parentName",
    "era",
    "category",
    "defaultWeapon",
    "excludeFromCodex",
    "codexSecret",
    "resultType",
    "buildPrice",
    "buildTime",
    "num",
    "consumeOnUse",
    "ingredients",
];

type Table = Map<String, Value>;

#[derive(Clone, Default, Serialize, Deserialize)]
pub struct Overlay {
    /// Per-export item maps keyed by uniqueName, ready to merge into the item database.
    pub exports: BTreeMap<String, Table>,
    /// uniqueName -> "path!contentHash" from DE's image manifest (icon mirror fallbacks).
    pub images: Option<BTreeMap<String, String>>,
}

#[derive(Default, Serialize, Deserialize)]
struct CachePayload {
    index: BTreeMap<String, String>,
    exports: BTreeMap<String, Table>,
    images_index: Option<String>,
    images: Option<BTreeMap<String, String>>,
}

impl CachePayload {
    fn overlay(&self) -> Overlay {
        Overlay {
            exports: self.exports.clone(),
            images: self.images.clone(),
        }
    }
}

pub struct Refreshed {
    pub changed: bool,
    /// Fresh overlay, the cached one after a failed fetch, or `None` with neither.
    pub overlay: Option<Overlay>,
}

/// Overlay from the last successful refresh, for use before the network answers.
pub fn load_cached(db: &Db) -> Option<Overlay> {
    read_cache(db).map(|c| c.overlay())
}

fn read_cache(db: &Db) -> Option<CachePayload> {
    serde_json::from_str(&kv_get(db, CACHE_KEY).ok()??).ok()
}

/// DE's index is plain LZMA-alone (`.lzma`) text.
pub fn lzma_decompress(bytes: &[u8]) -> Result<String, String> {
    let mut out = Vec::new();
    lzma_rs::lzma_decompress(&mut Cursor::new(bytes), &mut out)
        .map_err(|e| format!("lzma: {e}"))?;
    String::from_utf8(out).map_err(|e| e.to_string())
}

/// Base export name ("ExportWarframes_en.json") -> its hashed filename.
fn parse_index(text: &str) -> BTreeMap<String, String> {
    text.lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .map(|l| (l.split('!').next().unwrap_or(l).to_owned(), l.to_owned()))
        .collect()
}

/// Strip control chars DE leaves in its JSON, keeping the legal \t \n \r.
fn parse_de_json(bytes: &[u8]) -> Result<Value, String> {
    let text: String = String::from_utf8_lossy(bytes)
        .chars()
        .map(|c| {
            if c < ' ' && !matches!(c, '\t' | '\n' | '\r') {
                ' '
            } else {
                c
            }
        })
        .collect();
    serde_json::from_str(&text).map_err(|e| e.to_string())
}

async fn get(client: &Client, url: &str) -> Result<Vec<u8>, String> {
    match net::fetch(client, url, None, MAX_BYTES).await? {
        Fetched::Modified { body, .. } => Ok(body),
        Fetched::NotModified => Err(format!("{url}: unexpected 304")),
    }
}

async fn fetch_manifest(client: &Client, src: &Source, hashed: &str) -> Result<Value, String> {
    parse_de_json(&get(client, &format!("{}{hashed}", src.manifest_base)).await?)
}

async fn fetch_images(
    client: &Client,
    src: &Source,
    hashed: &str,
) -> Result<BTreeMap<String, String>, String> {
    let manifest = fetch_manifest(client, src, hashed).await?;
    let rows = manifest["Manifest"]
        .as_array()
        .map_or(&[][..], Vec::as_slice);
    Ok(rows
        .iter()
        .filter_map(|e| Some((e["uniqueName"].as_str()?, e["textureLocation"].as_str()?)))
        .map(|(u, t)| (u.to_owned(), t.to_owned()))
        .collect())
}

fn trim_entry(item: &Value) -> Value {
    Value::Object(
        KEPT_FIELDS
            .iter()
            .filter_map(|f| Some((f.to_string(), item.get(*f)?.clone())))
            .collect(),
    )
}

/// Refresh the overlay from DE. `bundled(export)` returns the bundled table, whose entries DE's copy skips.
/// Never fails: a network or parse error keeps the cached overlay (or none).
pub async fn refresh(
    db: &Db,
    client: &Client,
    src: &Source,
    bundled: &(dyn Fn(&str) -> Option<Table> + Sync),
) -> Refreshed {
    let previous = read_cache(db);
    match try_refresh(db, client, src, bundled, previous.as_ref()).await {
        Ok(r) => r,
        Err(e) => {
            tracing::warn!(
                "DE public export fetch failed ({e}); using {}",
                if previous.is_some() {
                    "cached overlay"
                } else {
                    "bundled data only"
                }
            );
            Refreshed {
                changed: false,
                overlay: previous.map(|p| p.overlay()),
            }
        }
    }
}

async fn try_refresh(
    db: &Db,
    client: &Client,
    src: &Source,
    bundled: &(dyn Fn(&str) -> Option<Table> + Sync),
    previous: Option<&CachePayload>,
) -> Result<Refreshed, String> {
    let index = parse_index(&lzma_decompress(&get(client, &src.index_url).await?)?);
    let mut files: Vec<&str> = SPECS.iter().map(|s| s.file).collect();
    files.dedup();
    if !files.iter().any(|f| index.contains_key(*f)) {
        return Err(format!(
            "index lists none of the {} expected manifests",
            files.len()
        ));
    }
    // A cache from before manifests were keyed by file name has no ".json" keys: refetch everything.
    let prev_index = previous
        .map(|p| &p.index)
        .filter(|i| i.keys().any(|k| k.ends_with(".json")));

    let mut next_index = BTreeMap::new();
    let mut exports: BTreeMap<String, Table> = BTreeMap::new();
    let mut bundled_tables: BTreeMap<&str, Option<Table>> = BTreeMap::new();
    let mut changed = false;

    for file in files {
        let Some(hashed) = index.get(file) else {
            continue;
        };
        next_index.insert(file.to_owned(), hashed.clone());
        let specs = SPECS.iter().filter(|s| s.file == file);

        if prev_index.and_then(|i| i.get(file)) == Some(hashed) {
            for key in specs.flat_map(|s| s.keys) {
                if let Some(kept) = previous.and_then(|p| p.exports.get(*key)) {
                    exports.insert((*key).to_owned(), kept.clone());
                }
            }
            continue;
        }

        let manifest = fetch_manifest(client, src, hashed).await?;
        for spec in specs {
            let Some(rows) = manifest[spec.array_key].as_array() else {
                continue;
            };
            for raw in rows {
                let Some(unique) = raw["uniqueName"].as_str() else {
                    continue;
                };
                let key = spec.target(raw);
                let table = bundled_tables.entry(key).or_insert_with(|| bundled(key));
                if table.as_ref().is_some_and(|t| t.contains_key(unique)) {
                    continue;
                }
                exports
                    .entry(key.to_owned())
                    .or_default()
                    .insert(unique.to_owned(), trim_entry(raw));
            }
        }
        changed = true;
    }

    let mut images = previous.and_then(|p| p.images.clone());
    let mut images_index = previous.and_then(|p| p.images_index.clone());
    if let Some(hashed) = index.get(IMAGE_MANIFEST)
        && (images_index.as_ref() != Some(hashed) || images.is_none() || changed)
    {
        let full = fetch_images(client, src, hashed).await?;
        // The manifest is ~20k rows; only the overlay entries are ever looked up.
        images = Some(
            exports
                .values()
                .flat_map(Table::keys)
                .filter_map(|u| Some((u.clone(), full.get(u)?.clone())))
                .collect(),
        );
        images_index = Some(hashed.clone());
        changed = true;
    }
    if let Some(images) = &images {
        for item in exports.values_mut().flat_map(Table::values_mut) {
            if item.get("icon").is_some_and(|i| !i.is_null()) {
                continue;
            }
            let texture = item["uniqueName"].as_str().and_then(|u| images.get(u));
            if let Some(path) = texture.and_then(|t| t.split('!').next()) {
                item["icon"] = path.into();
            }
        }
    }

    // A manifest DE stops listing changes the cache without setting `changed`.
    let dropped = previous.is_some_and(|p| p.index.keys().any(|f| !next_index.contains_key(f)));
    let payload = CachePayload {
        index: next_index,
        exports,
        images_index,
        images,
    };
    if changed || dropped || previous.is_none() {
        let json = serde_json::to_string(&payload).map_err(|e| e.to_string())?;
        kv_put(db, CACHE_KEY, &json).map_err(|e| e.to_string())?;
    }
    Ok(Refreshed {
        changed,
        overlay: Some(payload.overlay()),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::mem_db;

    use serde_json::json;
    use std::collections::HashMap;
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::sync::{Arc, Mutex};

    const SUIT: &str = "/Lotus/Powersuits/Test/TestSuit";
    const SUIT_TEXTURE: &str = "/Lotus/Interface/Icons/Test/TestSuit.png!00_abc";
    const NEW_MOD: &str = "/Lotus/Upgrades/Mods/Pistol/Event/Nightwave/TestOnlyAugmentMod";
    const NEW_MOD_TEXTURE: &str =
        "/Lotus/Interface/Cards/Images/Nightwave/TestOnlyAugmentMod.jpg!00_def";
    // Stands in for a mod the bundled package already carries.
    const BUNDLED_MOD: &str = "/Lotus/Upgrades/Mods/Warframe/AvatarSlideBoostMod";
    const NEW_RELIC: &str = "/Lotus/Types/Game/Projections/TestVoidProjectionA";
    const NEW_ARCANE: &str = "/Lotus/Upgrades/CosmeticEnhancers/Test/TestArcane";

    const INDEX: &[&str] = &[
        "ExportWarframes_en.json!00_wf",
        "ExportWeapons_en.json!00_wp",
        "ExportSentinels_en.json!00_se",
        "ExportUpgrades_en.json!00_up",
        "ExportRelicArcane_en.json!00_ra",
        "ExportManifest.json!00_img",
    ];

    fn compress(text: &str) -> Vec<u8> {
        let mut out = Vec::new();
        lzma_rs::lzma_compress(&mut Cursor::new(text.as_bytes()), &mut out).unwrap();
        out
    }

    /// Path -> body routes (editable between refreshes) and a per-path hit counter.
    struct Server {
        base: String,
        routes: Arc<Mutex<HashMap<String, Vec<u8>>>>,
        hits: Arc<Mutex<HashMap<String, usize>>>,
    }

    impl Server {
        fn start(index: &[&str]) -> Self {
            let listener = TcpListener::bind("127.0.0.1:0").unwrap();
            let base = format!("http://{}", listener.local_addr().unwrap());
            let routes: Arc<Mutex<HashMap<String, Vec<u8>>>> = Arc::default();
            let hits: Arc<Mutex<HashMap<String, usize>>> = Arc::default();
            let (r, h) = (routes.clone(), hits.clone());
            std::thread::spawn(move || {
                for mut s in listener.incoming().flatten() {
                    let mut buf = [0u8; 2048];
                    let n = s.read(&mut buf).unwrap_or(0);
                    let req = String::from_utf8_lossy(&buf[..n]).into_owned();
                    let path = req.split_whitespace().nth(1).unwrap_or("").to_owned();
                    *h.lock().unwrap().entry(path.clone()).or_default() += 1;
                    let body = r.lock().unwrap().get(&path).cloned();
                    let head = match &body {
                        Some(b) => format!("HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", b.len()),
                        None => "HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".into(),
                    };
                    let _ = s.write_all(head.as_bytes());
                    let _ = s.write_all(&body.unwrap_or_default());
                }
            });
            let server = Self { base, routes, hits };
            server.set_index(index);
            let json = |v: Value| v.to_string().into_bytes();
            server.put("/Manifest/ExportWarframes_en.json!00_wf", json(json!({"ExportWarframes": [{"uniqueName": SUIT, "name": "Test Suit", "masteryReq": 0}]})));
            server.put(
                "/Manifest/ExportWeapons_en.json!00_wp",
                json(json!({"ExportWeapons": []})),
            );
            server.put(
                "/Manifest/ExportSentinels_en.json!00_se",
                json(json!({"ExportSentinels": []})),
            );
            server.put("/Manifest/ExportUpgrades_en.json!00_up", json(json!({"ExportUpgrades": [
                {"uniqueName": NEW_MOD, "name": "Test Only Augment", "rarity": "RARE", "levelStats": [{"stats": ["a"]}, {"stats": ["b"]}]},
                {"uniqueName": BUNDLED_MOD, "name": "Bundled Already"}]})));
            server.put("/Manifest/ExportRelicArcane_en.json!00_ra", json(json!({"ExportRelicArcane": [
                {"uniqueName": NEW_RELIC, "name": "Test A1 Relic", "relicRewards": [{"rewardName": "x"}]},
                {"uniqueName": NEW_ARCANE, "name": "Test Arcane"}]})));
            server.put("/Manifest/ExportManifest.json!00_img", json(json!({"Manifest": [
                {"uniqueName": SUIT, "textureLocation": SUIT_TEXTURE},
                {"uniqueName": NEW_MOD, "textureLocation": NEW_MOD_TEXTURE},
                {"uniqueName": BUNDLED_MOD, "textureLocation": "/Lotus/Interface/Bundled.png!00_zzz"}]})));
            server
        }

        fn put(&self, path: &str, body: Vec<u8>) {
            self.routes.lock().unwrap().insert(path.into(), body);
        }

        fn set_index(&self, lines: &[&str]) {
            self.put("/index", compress(&lines.join("\n")));
        }

        fn hits(&self, path: &str) -> usize {
            self.hits.lock().unwrap().get(path).copied().unwrap_or(0)
        }

        fn source(&self) -> Source {
            Source {
                index_url: format!("{}/index", self.base),
                manifest_base: format!("{}/Manifest/", self.base),
            }
        }
    }

    fn bundled(export: &str) -> Option<Table> {
        (export == "ExportUpgrades").then(|| {
            let mut t = Table::new();
            t.insert(BUNDLED_MOD.into(), json!({}));
            t
        })
    }

    async fn run(db: &Db, server: &Server) -> Refreshed {
        refresh(db, &net::client().unwrap(), &server.source(), &bundled).await
    }

    #[test]
    fn decompresses_a_real_xz_lzma_index() {
        let text =
            lzma_decompress(include_bytes!("../../tests/fixtures/index_en.txt.lzma")).unwrap();
        let index = parse_index(&text);
        assert_eq!(
            index["ExportWarframes_en.json"],
            "ExportWarframes_en.json!00_wf"
        );
        assert_eq!(index.len(), 3);
    }

    #[test]
    fn strips_control_chars_from_de_json() {
        let v = parse_de_json(b"{\"a\":\"x\x01y\",\n\"b\":1}").unwrap();
        assert_eq!(v["a"], "x y");
    }

    #[tokio::test]
    async fn fills_icons_and_keeps_full_texture_location() {
        let (db, server) = (mem_db(), Server::start(INDEX));
        let r = run(&db, &server).await;
        assert!(r.changed);
        let overlay = r.overlay.unwrap();
        assert_eq!(
            overlay.exports["ExportWarframes"][SUIT]["icon"],
            "/Lotus/Interface/Icons/Test/TestSuit.png"
        );
        assert_eq!(overlay.images.unwrap()[SUIT], SUIT_TEXTURE);
    }

    #[tokio::test]
    async fn skips_unchanged_manifests_on_the_next_refresh() {
        let (db, server) = (mem_db(), Server::start(INDEX));
        run(&db, &server).await;
        let r = run(&db, &server).await;
        assert!(!r.changed);
        assert_eq!(server.hits("/index"), 2);
        assert_eq!(server.hits("/Manifest/ExportWarframes_en.json!00_wf"), 1);
        assert_eq!(server.hits("/Manifest/ExportManifest.json!00_img"), 1);
        assert_eq!(
            r.overlay.unwrap().exports["ExportWarframes"][SUIT]["icon"],
            "/Lotus/Interface/Icons/Test/TestSuit.png"
        );
    }

    #[tokio::test]
    async fn carries_new_mods_trimmed_and_skips_bundled_ones() {
        let (db, server) = (mem_db(), Server::start(INDEX));
        let overlay = run(&db, &server).await.overlay.unwrap();
        let mods = &overlay.exports["ExportUpgrades"];
        assert_eq!(mods[NEW_MOD]["name"], "Test Only Augment");
        assert_eq!(
            mods[NEW_MOD]["icon"],
            "/Lotus/Interface/Cards/Images/Nightwave/TestOnlyAugmentMod.jpg"
        );
        // Per-rank stat tables are the bulk of DE's export and nothing reads them.
        assert!(mods[NEW_MOD].get("levelStats").is_none());
        assert!(!mods.contains_key(BUNDLED_MOD));
        assert!(!overlay.images.unwrap().contains_key(BUNDLED_MOD));
    }

    #[tokio::test]
    async fn splits_the_shared_relic_and_arcane_manifest() {
        let (db, server) = (mem_db(), Server::start(INDEX));
        let e = run(&db, &server).await.overlay.unwrap().exports;
        assert_eq!(e["ExportRelics"][NEW_RELIC]["name"], "Test A1 Relic");
        assert_eq!(e["ExportArcanes"][NEW_ARCANE]["name"], "Test Arcane");
        assert!(!e["ExportRelics"].contains_key(NEW_ARCANE));
        assert!(!e["ExportArcanes"].contains_key(NEW_RELIC));
    }

    #[tokio::test]
    async fn refetches_when_the_cache_predates_file_keyed_indexes() {
        let (db, server) = (mem_db(), Server::start(INDEX));
        let stale = json!({"index": {"ExportWarframes": "ExportWarframes_en.json!00_wf"},
            "exports": {"ExportWarframes": {SUIT: {"uniqueName": SUIT, "name": "Stale Suit"}}},
            "images_index": null, "images": null});
        kv_put(&db, CACHE_KEY, &stale.to_string()).unwrap();
        // An offline start still sees the last-good items, stale names and all.
        assert_eq!(
            load_cached(&db).unwrap().exports["ExportWarframes"][SUIT]["name"],
            "Stale Suit"
        );
        let overlay = run(&db, &server).await.overlay.unwrap();
        assert_eq!(server.hits("/Manifest/ExportWarframes_en.json!00_wf"), 1);
        assert_eq!(
            overlay.exports["ExportWarframes"][SUIT]["name"],
            "Test Suit"
        );
    }

    #[tokio::test]
    async fn keeps_the_cached_overlay_when_the_index_lists_none_of_des_manifests() {
        let (db, server) = (mem_db(), Server::start(INDEX));
        run(&db, &server).await;
        let cached = kv_get(&db, CACHE_KEY).unwrap();
        server.set_index(&["SomeRenamedExport_en.json!00_zz"]);
        let r = run(&db, &server).await;
        assert!(!r.changed);
        assert_eq!(
            r.overlay.unwrap().exports["ExportWarframes"][SUIT]["name"],
            "Test Suit"
        );
        assert_eq!(kv_get(&db, CACHE_KEY).unwrap(), cached);
    }

    #[tokio::test]
    async fn falls_back_to_the_cache_when_the_network_is_down() {
        let (db, server) = (mem_db(), Server::start(INDEX));
        run(&db, &server).await;
        let offline = Source {
            index_url: "http://127.0.0.1:1/index".into(),
            manifest_base: "http://127.0.0.1:1/".into(),
        };
        let r = refresh(&db, &net::client().unwrap(), &offline, &bundled).await;
        assert!(!r.changed);
        assert!(r.overlay.unwrap().exports.contains_key("ExportWarframes"));
        // With no cache either there is simply no overlay.
        let r = refresh(&mem_db(), &net::client().unwrap(), &offline, &bundled).await;
        assert!(r.overlay.is_none());
    }

    #[tokio::test]
    async fn leaves_the_cache_untouched_when_nothing_changed() {
        let (db, server) = (mem_db(), Server::start(INDEX));
        run(&db, &server).await;
        let updated = |db: &Db| {
            db.0.lock()
                .unwrap()
                .query_row(
                    "SELECT updated_at, value FROM kv_cache WHERE key = ?1",
                    [CACHE_KEY],
                    |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?)),
                )
                .unwrap()
        };
        db.0.lock()
            .unwrap()
            .execute("UPDATE kv_cache SET updated_at = 1", [])
            .unwrap();
        let before = updated(&db);
        run(&db, &server).await;
        // A rewrite would carry a new updated_at stamp.
        assert_eq!(updated(&db), before);
    }

    #[tokio::test]
    async fn rewrites_a_cache_it_cannot_read() {
        let (db, server) = (mem_db(), Server::start(INDEX));
        kv_put(&db, CACHE_KEY, "{ not json").unwrap();
        run(&db, &server).await;
        let written: Value =
            serde_json::from_str(&kv_get(&db, CACHE_KEY).unwrap().unwrap()).unwrap();
        assert_eq!(
            written["exports"]["ExportWarframes"][SUIT]["name"],
            "Test Suit"
        );
    }

    #[tokio::test]
    async fn rewrites_the_cache_when_de_stops_listing_a_manifest() {
        let (db, server) = (mem_db(), Server::start(INDEX));
        run(&db, &server).await;
        let without: Vec<_> = INDEX
            .iter()
            .copied()
            .filter(|l| !l.starts_with("ExportSentinels"))
            .collect();
        server.set_index(&without);
        assert!(!run(&db, &server).await.changed);
        let written: Value =
            serde_json::from_str(&kv_get(&db, CACHE_KEY).unwrap().unwrap()).unwrap();
        assert!(written["index"].get("ExportSentinels_en.json").is_none());
        assert_eq!(
            written["exports"]["ExportWarframes"][SUIT]["name"],
            "Test Suit"
        );
    }
}
