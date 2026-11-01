//! Where scanners get their pixels: the live game window.
use image::RgbaImage;
use serde::Serialize;
use specta::Type;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum CaptureKind {
    /// Cropped to the game's client rect.
    Window,
    /// Whole primary monitor: the game window was not found.
    Screen,
    Fixture,
}

pub struct Frame {
    pub image: RgbaImage,
    pub kind: CaptureKind,
}

pub trait CaptureSource: Send + Sync {
    fn capture(&self) -> Result<Frame, String>;
}

/// Live capture: BitBlt of the monitor(s) cropped to Warframe's client rect, like WFHelper.
/// No fallback to another API on failure (WFHelper: it can return stale MPO frames); when the
/// window is not found it reads the primary monitor instead.
#[cfg(windows)]
pub struct GameWindow;

#[cfg(windows)]
impl CaptureSource for GameWindow {
    fn capture(&self) -> Result<Frame, String> {
        use crate::platform::windows::{capture, game_window};
        let rect =
            game_window::find().and_then(|g| capture::clip(g.client, capture::virtual_screen()));
        let image = capture::capture(rect).map_err(|e| e.to_string())?;
        Ok(Frame {
            image,
            kind: if rect.is_some() {
                CaptureKind::Window
            } else {
                CaptureKind::Screen
            },
        })
    }
}

#[derive(Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct GameWindowInfo {
    pub found: bool,
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
    pub dpi: u32,
    /// What a capture produced right now: `window`, `screen`, or `error`.
    pub source: String,
    /// The captured frame is (nearly) all black: BitBlt did not read the game.
    pub black: bool,
    pub error: Option<String>,
}

#[cfg(windows)]
fn game_window_info() -> GameWindowInfo {
    use crate::platform::windows::{capture, game_window};
    let win = game_window::find();
    let (x, y, width, height, dpi) = win
        .as_ref()
        .map(|g| (g.client.x, g.client.y, g.client.w, g.client.h, g.dpi))
        .unwrap_or_default();
    let (source, black, error) = match GameWindow.capture() {
        Ok(f) => {
            let black = capture::looks_black(&f.image);
            if black {
                tracing::warn!("capture: frame is black ({:?})", f.kind);
            }
            let source = match f.kind {
                CaptureKind::Window => "window",
                _ => "screen",
            };
            (source.to_string(), black, None)
        }
        Err(e) => ("error".to_string(), false, Some(e)),
    };
    GameWindowInfo {
        found: win.is_some(),
        x,
        y,
        width,
        height,
        dpi,
        source,
        black,
        error,
    }
}

/// Where Warframe is and whether a capture of it comes out black (manual-test aid).
#[tauri::command]
#[specta::specta]
pub async fn capture_game_window_info() -> Result<GameWindowInfo, String> {
    #[cfg(windows)]
    {
        // GDI blocks, so keep it off the async runtime threads.
        tauri::async_runtime::spawn_blocking(game_window_info)
            .await
            .map_err(|e| e.to_string())
    }
    #[cfg(not(windows))]
    Err("windows only".into())
}
