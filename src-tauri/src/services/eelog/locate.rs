//! Finds EE.log: the manual setting, then Windows `%LOCALAPPDATA%`, then Steam Proton and the
//! Wine prefixes of Heroic, Lutris, Bottles and plain Wine.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::LazyLock;
use std::time::SystemTime;

use regex::Regex;

/// Warframe's Steam app id names the Proton prefix directory.
const STEAM_APP_ID: &str = "230410";

static VDF_PATH: LazyLock<Regex> = LazyLock::new(|| Regex::new(r#""path"\s+"([^"]*)""#).unwrap());
static LUTRIS_PREFIX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?m)^[ \t]*prefix:[ \t]*(\S.*?)[ \t]*$").unwrap());

/// Best-known EE.log path, or `None` when undiscoverable. A non-blank `manual` path wins.
pub fn resolve(manual: &str) -> Option<PathBuf> {
    let manual = manual.trim();
    if !manual.is_empty() {
        return Some(manual.into());
    }
    if cfg!(windows) {
        let base = std::env::var_os("LOCALAPPDATA")?;
        return Some(Path::new(&base).join("Warframe").join("EE.log"));
    }
    discover_linux(Path::new(&std::env::var_os("HOME")?))
}

/// Every `"path"` value of a libraryfolders.vdf, without a VDF parser.
pub fn parse_steam_library_paths(vdf: &str) -> Vec<String> {
    VDF_PATH
        .captures_iter(vdf)
        .map(|c| c[1].replace("\\\\", "\\"))
        .collect()
}

fn list_dir(dir: &Path) -> Vec<PathBuf> {
    fs::read_dir(dir)
        .map(|rd| rd.flatten().map(|e| e.path()).collect())
        .unwrap_or_default()
}

fn ext_is(path: &Path, exts: &[&str]) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| exts.contains(&e))
}

fn heroic_prefixes(json: &str) -> Vec<String> {
    let Ok(serde_json::Value::Object(games)) = serde_json::from_str(json) else {
        return Vec::new();
    };
    games
        .values()
        .filter_map(|g| g.get("winePrefix")?.as_str())
        .filter(|p| !p.is_empty())
        .map(String::from)
        .collect()
}

fn lutris_prefix(yml: &str) -> Option<String> {
    let raw = LUTRIS_PREFIX.captures(yml)?[1].to_string();
    let unquoted = ["'", "\""]
        .iter()
        .find_map(|q| raw.strip_prefix(q)?.strip_suffix(q))
        .unwrap_or(&raw);
    (!unquoted.is_empty()).then(|| unquoted.to_string())
}

/// Wine prefixes of Heroic (Warframe is also on Epic), Lutris, Bottles and plain Wine.
fn launcher_prefixes(home: &Path) -> Vec<PathBuf> {
    let flatpak = |id: &str, rest: &str| home.join(".var/app").join(id).join(rest);
    let mut out = vec![home.join(".wine")];
    for dir in [
        home.join(".config/heroic/GamesConfig"),
        flatpak("com.heroicgameslauncher.hgl", "config/heroic/GamesConfig"),
    ] {
        for file in list_dir(&dir).iter().filter(|f| ext_is(f, &["json"])) {
            let text = fs::read_to_string(file).unwrap_or_default();
            out.extend(heroic_prefixes(&text).into_iter().map(PathBuf::from));
        }
    }
    out.extend(list_dir(&home.join("Games/Heroic/Prefixes/default")));
    for dir in [
        home.join(".config/lutris/games"),
        home.join(".local/share/lutris/games"),
        flatpak("net.lutris.Lutris", "config/lutris/games"),
        flatpak("net.lutris.Lutris", "data/lutris/games"),
    ] {
        for file in list_dir(&dir)
            .iter()
            .filter(|f| ext_is(f, &["yml", "yaml"]))
        {
            let text = fs::read_to_string(file).unwrap_or_default();
            out.extend(lutris_prefix(&text).map(PathBuf::from));
        }
    }
    out.extend(list_dir(&home.join(".local/share/bottles/bottles")));
    out.extend(list_dir(&flatpak(
        "com.usebottles.bottles",
        "data/bottles/bottles",
    )));
    // A relative prefix would resolve against wherever the app was started.
    out.into_iter()
        .map(|p| match p.strip_prefix("~") {
            Ok(rest) => home.join(rest),
            Err(_) => p,
        })
        .filter(|p| p.is_absolute())
        .collect()
}

/// EE.log under any Windows user of a prefix; a Proton-run prefix nests it in `pfx`.
fn logs_in_prefix(prefix: &Path) -> Vec<PathBuf> {
    [prefix.to_path_buf(), prefix.join("pfx")]
        .iter()
        .flat_map(|root| list_dir(&root.join("drive_c/users")))
        .map(|user| user.join("AppData/Local/Warframe/EE.log"))
        .filter(|p| p.exists())
        .collect()
}

fn mtime(p: &Path) -> SystemTime {
    fs::metadata(p)
        .and_then(|m| m.modified())
        .unwrap_or(SystemTime::UNIX_EPOCH)
}

fn steam_roots(home: &Path) -> [PathBuf; 5] {
    [
        home.join(".local/share/Steam"),
        home.join(".steam/steam"),
        home.join(".steam/root"),
        home.join(".var/app/com.valvesoftware.Steam/.local/share/Steam"),
        home.join("snap/steam/common/.local/share/Steam"),
    ]
}

/// The newest EE.log of any Steam library or launcher prefix; failing that, the path a fresh
/// Steam prefix will get, so a watcher can attach before the game writes it.
pub fn discover_linux(home: &Path) -> Option<PathBuf> {
    let mut libraries: Vec<PathBuf> = Vec::new();
    for root in steam_roots(home).into_iter().filter(|r| r.exists()) {
        let vdf = fs::read_to_string(root.join("steamapps/libraryfolders.vdf")).unwrap_or_default();
        libraries.push(root);
        libraries.extend(
            parse_steam_library_paths(&vdf)
                .into_iter()
                .map(PathBuf::from),
        );
    }
    // Every compatdata id, since Warframe added as a non-Steam game gets a random one.
    let mut found: Vec<PathBuf> = libraries
        .iter()
        .flat_map(|lib| list_dir(&lib.join("steamapps/compatdata")))
        .flat_map(|prefix| logs_in_prefix(&prefix))
        .collect();
    found.extend(
        launcher_prefixes(home)
            .iter()
            .flat_map(|p| logs_in_prefix(p)),
    );
    // A player who moved launchers keeps the old prefix, so the newest log wins.
    if let Some(newest) = found.into_iter().max_by_key(|p| mtime(p)) {
        return Some(newest);
    }
    libraries
        .iter()
        .map(|lib| lib.join("steamapps/compatdata").join(STEAM_APP_ID))
        .find(|prefix| prefix.exists())
        .map(|prefix| prefix.join("pfx/drive_c/users/steamuser/AppData/Local/Warframe/EE.log"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::eelog::tests::tmp_dir;

    fn log_in_prefix(prefix: &Path, user: &str) -> PathBuf {
        let p = prefix
            .join("drive_c/users")
            .join(user)
            .join("AppData/Local/Warframe/EE.log");
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        fs::write(&p, "").unwrap();
        p
    }

    #[test]
    fn vdf_paths() {
        let vdf = "\"libraryfolders\"\n{\n\t\"0\"\n\t{\n\t\t\"path\"\t\t\"/home/user/.local/share/Steam\"\n\t\t\"label\"\t\t\"\"\n\t}\n\t\"1\"\n\t{\n\t\t\"path\"\t\t\"/mnt/games/SteamLibrary\"\n\t}\n}\n";
        assert_eq!(
            parse_steam_library_paths(vdf),
            ["/home/user/.local/share/Steam", "/mnt/games/SteamLibrary"]
        );
        assert_eq!(
            parse_steam_library_paths("\"path\"\t\t\"D:\\\\SteamLibrary\""),
            ["D:\\SteamLibrary"]
        );
        assert!(parse_steam_library_paths("\"label\" \"foo\"").is_empty());
    }

    #[test]
    fn manual_path_wins_and_blank_is_ignored() {
        assert_eq!(
            resolve("/tmp/fake-ee/EE.log"),
            Some("/tmp/fake-ee/EE.log".into())
        );
        assert_ne!(resolve("   "), Some("   ".into()));
    }

    #[test]
    #[cfg(windows)]
    fn windows_uses_localappdata() {
        let base = std::env::var_os("LOCALAPPDATA").unwrap();
        assert_eq!(
            resolve(""),
            Some(Path::new(&base).join("Warframe").join("EE.log"))
        );
    }

    #[test]
    fn lutris_prefix_from_game_config() {
        let (home, prefix) = (tmp_dir("home"), tmp_dir("lutris-prefix"));
        let games = home.join(".local/share/lutris/games");
        fs::create_dir_all(&games).unwrap();
        let yml = format!(
            "game:\n  exe: x\n  prefix: '{}'\nsystem: {{}}\n",
            prefix.display()
        );
        fs::write(games.join("warframe-1700000000.yml"), yml).unwrap();
        let expected = log_in_prefix(&prefix, "player");
        assert_eq!(discover_linux(&home), Some(expected));
    }

    #[test]
    fn heroic_prefix_nested_under_pfx() {
        let home = tmp_dir("home");
        let prefix = home.join("Games/Heroic/Prefixes/Warframe Epic");
        let configs = home.join(".config/heroic/GamesConfig");
        fs::create_dir_all(&configs).unwrap();
        let json = serde_json::json!({"a1b2c3": {"winePrefix": prefix}, "version": "v0"});
        fs::write(configs.join("a1b2c3.json"), json.to_string()).unwrap();
        let expected = log_in_prefix(&prefix.join("pfx"), "steamuser");
        assert_eq!(discover_linux(&home), Some(expected));
    }

    #[test]
    fn non_steam_game_gets_a_random_compat_id() {
        let home = tmp_dir("home");
        let compat = home.join(".local/share/Steam/steamapps/compatdata/3141592653/pfx");
        let expected = log_in_prefix(&compat, "steamuser");
        assert_eq!(discover_linux(&home), Some(expected));
    }

    #[test]
    fn newest_log_wins_over_an_old_steam_prefix() {
        let home = tmp_dir("home");
        let steam = home.join(".local/share/Steam/steamapps/compatdata/230410/pfx");
        let old = log_in_prefix(&steam, "steamuser");
        fs::File::options()
            .write(true)
            .open(&old)
            .unwrap()
            .set_modified(SystemTime::UNIX_EPOCH)
            .unwrap();
        let bottle = home.join(".local/share/bottles/bottles/Warframe");
        let expected = log_in_prefix(&bottle, "player");
        assert_eq!(discover_linux(&home), Some(expected));
    }

    #[test]
    fn empty_steam_prefix_yields_the_expected_path() {
        let home = tmp_dir("home");
        let steam = home.join(".local/share/Steam");
        fs::create_dir_all(steam.join("steamapps/compatdata/230410")).unwrap();
        let guess = discover_linux(&home).unwrap();
        assert!(guess.starts_with(&steam) && guess.ends_with("Warframe/EE.log"));
        assert!(!guess.exists());
        assert_eq!(discover_linux(&tmp_dir("bare")), None);
    }

    #[test]
    fn extra_library_from_libraryfolders() {
        let (home, lib) = (tmp_dir("home"), tmp_dir("lib"));
        let steam = home.join(".local/share/Steam");
        fs::create_dir_all(steam.join("steamapps")).unwrap();
        let path = lib.display().to_string().replace('\\', "\\\\");
        let vdf = format!("\"0\"\n{{\n\"path\"\t\"{path}\"\n}}");
        fs::write(steam.join("steamapps/libraryfolders.vdf"), vdf).unwrap();
        let expected = log_in_prefix(&lib.join("steamapps/compatdata/230410/pfx"), "steamuser");
        assert_eq!(discover_linux(&home), Some(expected));
    }
}
