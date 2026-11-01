use std::sync::Mutex;

use image::RgbaImage;
use windows::Globalization::Language;
use windows::Graphics::Imaging::{BitmapAlphaMode, BitmapPixelFormat, SoftwareBitmap};
use windows::Media::Ocr::OcrEngine;
use windows::Storage::Streams::DataWriter;
use windows::core::HSTRING;

pub struct Line {
    pub text: String,
    pub words: Vec<String>,
}

/// Game locale code (`game_locale::GAME_LOCALES`) the cards are printed in.
static GAME_LANGUAGE: Mutex<&'static str> = Mutex::new("en");

/// Reads cards in this game language from now on, when Windows has its OCR pack.
pub fn set_language(code: &'static str) {
    *GAME_LANGUAGE.lock().unwrap() = code;
}

/// BCP-47 prefix of a game locale code: DE's "tc"/"zh" are Traditional/Simplified Chinese.
fn bcp47(code: &str) -> &str {
    match code {
        "tc" => "zh-Hant",
        "zh" => "zh-Hans",
        c => c,
    }
}

/// First installed recognizer tag that matches the game language ("fr" matches "fr-FR").
fn pick<'a>(installed: &'a [String], code: &str) -> Option<&'a String> {
    let want = bcp47(code).to_ascii_lowercase();
    installed.iter().find(|t| {
        let t = t.to_ascii_lowercase();
        t == want || t.starts_with(&format!("{want}-"))
    })
}

fn installed_tags() -> Vec<String> {
    OcrEngine::AvailableRecognizerLanguages()
        .map(|langs| {
            langs
                .into_iter()
                .filter_map(|l| l.LanguageTag().ok())
                .map(|t| t.to_string())
                .collect()
        })
        .unwrap_or_default()
}

/// The OCR language for the game language: (game code, installed tag or `None` when missing).
pub fn language_status() -> (&'static str, Option<String>) {
    let code = *GAME_LANGUAGE.lock().unwrap();
    (code, pick(&installed_tags(), code).cloned())
}

fn create(tag: &str) -> windows::core::Result<OcrEngine> {
    OcrEngine::TryCreateFromLanguage(&Language::CreateLanguage(&HSTRING::from(tag))?)
}

/// The game-language engine when its pack is installed, else en-US.
fn engine() -> windows::core::Result<OcrEngine> {
    match language_status() {
        (code, Some(tag)) if code != "en" => create(&tag).or_else(|_| create("en-US")),
        _ => create("en-US"),
    }
}

/// Windows capability tag of the OCR pack for a game locale. A fixed allowlist: the elevated
/// install command is only ever built from these, never from user text.
pub fn capability_tag(code: &str) -> Option<&'static str> {
    Some(match code {
        "en" => "en-US",
        "de" => "de-DE",
        "es" => "es-ES",
        "fr" => "fr-FR",
        "it" => "it-IT",
        "pl" => "pl-PL",
        "pt" => "pt-BR",
        "ru" => "ru-RU",
        "ja" => "ja-JP",
        "ko" => "ko-KR",
        "tr" => "tr-TR",
        "zh" => "zh-CN",
        "tc" => "zh-TW",
        _ => return None,
    })
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum OcrInstallOutcome {
    Installed,
    /// The user said no at the UAC prompt.
    Cancelled,
    Failed,
    /// The command finished but the recognizer never showed up.
    Timeout,
    /// No Windows OCR pack exists for the game language.
    Unsupported,
}

/// Absolute path of the system PowerShell, so the elevated launch never resolves an exe via PATH.
fn system_powershell() -> Option<HSTRING> {
    use windows::Win32::System::SystemInformation::GetSystemDirectoryW;
    let mut buf = [0u16; 260];
    let len = unsafe { GetSystemDirectoryW(Some(&mut buf)) } as usize;
    if len == 0 || len >= buf.len() {
        return None;
    }
    let dir = String::from_utf16(&buf[..len]).ok()?;
    let exe = std::path::Path::new(&dir).join(r"WindowsPowerShell\v1.0\powershell.exe");
    exe.is_file().then(|| HSTRING::from(exe.as_os_str()))
}

/// Installs the OCR pack for the current game language with one elevated PowerShell command
/// (UAC prompt), then waits for the recognizer to appear. Blocks for minutes: off the UI thread.
/// The engine is created per read, so the new language is used from the next scan on.
pub fn install_pack() -> OcrInstallOutcome {
    use windows::Win32::Foundation::{CloseHandle, ERROR_CANCELLED, WAIT_OBJECT_0};
    use windows::Win32::System::Threading::{GetExitCodeProcess, WaitForSingleObject};
    use windows::Win32::UI::Shell::{
        SEE_MASK_NOASYNC, SEE_MASK_NOCLOSEPROCESS, SHELLEXECUTEINFOW, ShellExecuteExW,
    };
    use windows::Win32::UI::WindowsAndMessaging::SW_HIDE;
    use windows::core::{PCWSTR, w};

    let code = *GAME_LANGUAGE.lock().unwrap();
    let Some(tag) = capability_tag(code) else {
        return OcrInstallOutcome::Unsupported;
    };
    let Some(powershell) = system_powershell() else {
        return OcrInstallOutcome::Failed;
    };
    let params = HSTRING::from(format!(
        "-NoProfile -NonInteractive -Command Add-WindowsCapability -Online -Name \"Language.OCR~~~{tag}~0.0.1.0\" -ErrorAction Stop"
    ));
    let mut info = SHELLEXECUTEINFOW {
        cbSize: std::mem::size_of::<SHELLEXECUTEINFOW>() as u32,
        fMask: SEE_MASK_NOCLOSEPROCESS | SEE_MASK_NOASYNC,
        lpVerb: w!("runas"),
        lpFile: PCWSTR(powershell.as_ptr()),
        lpParameters: PCWSTR(params.as_ptr()),
        nShow: SW_HIDE.0,
        ..Default::default()
    };
    if let Err(e) = unsafe { ShellExecuteExW(&mut info) } {
        if e.code() == ERROR_CANCELLED.to_hresult() {
            return OcrInstallOutcome::Cancelled;
        }
        tracing::warn!("ocr pack install: {e}");
        return OcrInstallOutcome::Failed;
    }
    // The pack downloads from Windows Update: give it 15 minutes.
    let mut exit = 1u32;
    unsafe {
        let done = WaitForSingleObject(info.hProcess, 15 * 60 * 1000) == WAIT_OBJECT_0;
        if done {
            let _ = GetExitCodeProcess(info.hProcess, &mut exit);
        }
        let _ = CloseHandle(info.hProcess);
        if !done {
            return OcrInstallOutcome::Timeout;
        }
    }
    if exit != 0 {
        tracing::warn!("ocr pack install: powershell exit {exit}");
        return OcrInstallOutcome::Failed;
    }
    for _ in 0..60 {
        if pick(&installed_tags(), code).is_some() {
            return OcrInstallOutcome::Installed;
        }
        std::thread::sleep(std::time::Duration::from_secs(1));
    }
    OcrInstallOutcome::Timeout
}

/// An OCR engine can be created (a language pack is installed).
pub fn available() -> bool {
    engine().is_ok()
}

/// Windows.Media.Ocr over an RGBA buffer. Blocks on WinRT: call off the UI thread.
pub fn recognize(img: &RgbaImage) -> windows::core::Result<Vec<Line>> {
    let engine = engine()?;
    let bgra: Vec<u8> = img
        .pixels()
        .flat_map(|p| [p[2], p[1], p[0], p[3]])
        .collect();
    let writer = DataWriter::new()?;
    writer.WriteBytes(&bgra)?;
    let bitmap = SoftwareBitmap::CreateCopyWithAlphaFromBuffer(
        &writer.DetachBuffer()?,
        BitmapPixelFormat::Bgra8,
        img.width() as i32,
        img.height() as i32,
        BitmapAlphaMode::Ignore,
    )?;
    let result = engine.RecognizeAsync(&bitmap)?.join()?;
    let mut lines = Vec::new();
    for line in result.Lines()? {
        let words = line
            .Words()?
            .into_iter()
            .filter_map(|w| w.Text().ok())
            .map(|t| t.to_string())
            .collect();
        lines.push(Line {
            text: line.Text()?.to_string(),
            words,
        });
    }
    Ok(lines)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn picks_the_installed_pack_for_the_game_language() {
        let installed: Vec<String> = ["en-US", "fr-FR", "zh-Hant-TW"].map(String::from).into();
        assert_eq!(pick(&installed, "fr").map(String::as_str), Some("fr-FR"));
        assert_eq!(
            pick(&installed, "tc").map(String::as_str),
            Some("zh-Hant-TW")
        );
        assert_eq!(pick(&installed, "zh"), None);
        assert_eq!(pick(&installed, "de"), None);
        // "en" must not match a tag that only starts with the same letters.
        assert_eq!(pick(&["eng-X".into()], "en"), None);
    }

    #[test]
    fn every_capability_tag_is_a_plain_language_tag() {
        // The tags go into an elevated command line: letters and one dash only.
        for code in crate::services::game_locale::GAME_LOCALES {
            if let Some(tag) = capability_tag(code) {
                assert!(
                    tag.len() == 5 && tag.chars().all(|c| c.is_ascii_alphabetic() || c == '-'),
                    "{tag}"
                );
            }
        }
        assert_eq!(capability_tag("fr; rm"), None);
    }

    #[test]
    fn powershell_is_the_absolute_system32_one() {
        let p = system_powershell().expect("system PowerShell").to_string();
        let path = std::path::Path::new(&p);
        assert!(path.is_absolute(), "{p}");
        assert!(
            p.to_ascii_lowercase()
                .ends_with(r"\windowspowershell\v1.0\powershell.exe"),
            "{p}"
        );
    }

    #[test]
    fn lists_installed_recognizers_without_failing() {
        // Machine-dependent: only checks the WinRT call works.
        let _ = installed_tags();
        let _ = language_status();
    }
}
