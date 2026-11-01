use image::RgbaImage;
use windows::Win32::Graphics::Gdi::{
    BI_RGB, BITMAPINFO, BITMAPINFOHEADER, BitBlt, CreateCompatibleBitmap, CreateCompatibleDC,
    DIB_RGB_COLORS, DeleteDC, DeleteObject, GetDC, GetDIBits, HBITMAP, HDC, ReleaseDC, SRCCOPY,
    SelectObject,
};
use windows::Win32::UI::WindowsAndMessaging::{
    GetSystemMetrics, SM_CXSCREEN, SM_CXVIRTUALSCREEN, SM_CYSCREEN, SM_CYVIRTUALSCREEN,
    SM_XVIRTUALSCREEN, SM_YVIRTUALSCREEN,
};

/// Anything smaller than this is a bad read, not a game window (WFHelper's floor).
pub const MIN_W: i32 = 320;
pub const MIN_H: i32 = 240;

/// Pixel rect in virtual-screen coordinates.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}

fn fail() -> windows::core::Error {
    windows::core::Error::from_hresult(windows::core::HRESULT(-1))
}

/// The whole virtual desktop (every monitor).
pub fn virtual_screen() -> Rect {
    // SAFETY: plain metrics queries.
    unsafe {
        Rect {
            x: GetSystemMetrics(SM_XVIRTUALSCREEN),
            y: GetSystemMetrics(SM_YVIRTUALSCREEN),
            w: GetSystemMetrics(SM_CXVIRTUALSCREEN),
            h: GetSystemMetrics(SM_CYVIRTUALSCREEN),
        }
    }
}

/// `r` clipped to `bounds`; `None` when what is left is below the minimum window size.
pub fn clip(r: Rect, bounds: Rect) -> Option<Rect> {
    let x = r.x.max(bounds.x);
    let y = r.y.max(bounds.y);
    let w = (r.x + r.w).min(bounds.x + bounds.w) - x;
    let h = (r.y + r.h).min(bounds.y + bounds.h) - y;
    (w >= MIN_W && h >= MIN_H).then_some(Rect { x, y, w, h })
}

/// True when a sparse grid of pixels is all (near) black: the signature of BitBlt failing to read
/// the surface. A genuinely black loading screen reads the same, so callers only warn.
pub fn looks_black(img: &RgbaImage) -> bool {
    let (w, h) = img.dimensions();
    if w == 0 || h == 0 {
        return true;
    }
    (0..48).all(|j| {
        (0..48).all(|i| {
            let p = img.get_pixel(i * (w - 1) / 47, j * (h - 1) / 47);
            p[0].max(p[1]).max(p[2]) <= 6
        })
    })
}

/// BitBlt from the *screen* DC (post-DWM composition, so borderless/DX12 is read as shown).
/// `rect` is in virtual-screen pixels; `None` is the primary monitor.
pub fn capture(rect: Option<Rect>) -> windows::core::Result<RgbaImage> {
    let r = rect.unwrap_or_else(|| {
        // SAFETY: plain metrics queries.
        let (w, h) = unsafe { (GetSystemMetrics(SM_CXSCREEN), GetSystemMetrics(SM_CYSCREEN)) };
        Rect { x: 0, y: 0, w, h }
    });
    if r.w <= 0 || r.h <= 0 {
        return Err(fail());
    }
    // SAFETY: every GDI handle is released before returning, on all paths.
    unsafe {
        let src = GetDC(None);
        if src.is_invalid() {
            return Err(fail());
        }
        let dc = CreateCompatibleDC(Some(src));
        let bmp = CreateCompatibleBitmap(src, r.w, r.h);
        let res = if dc.is_invalid() || bmp.is_invalid() {
            Err(fail())
        } else {
            let old = SelectObject(dc, bmp.into());
            let res = read_pixels(dc, bmp, src, r);
            SelectObject(dc, old);
            res
        };
        if !bmp.is_invalid() {
            let _ = DeleteObject(bmp.into());
        }
        if !dc.is_invalid() {
            let _ = DeleteDC(dc);
        }
        ReleaseDC(None, src);
        res
    }
}

/// SAFETY: `dc` has `bmp` selected and all three handles are live.
unsafe fn read_pixels(
    dc: HDC,
    bmp: HBITMAP,
    src: HDC,
    Rect { x, y, w, h }: Rect,
) -> windows::core::Result<RgbaImage> {
    // SAFETY: upheld by the caller; `px` is sized for w*h 32-bit pixels.
    unsafe {
        BitBlt(dc, 0, 0, w, h, Some(src), x, y, SRCCOPY)?;
        let mut bi = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: w,
                biHeight: -h, // top-down
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0,
                ..Default::default()
            },
            ..Default::default()
        };
        let mut px = vec![0u8; (w as usize) * (h as usize) * 4];
        let rows = GetDIBits(
            dc,
            bmp,
            0,
            h as u32,
            Some(px.as_mut_ptr().cast()),
            &mut bi,
            DIB_RGB_COLORS,
        );
        if rows == 0 {
            return Err(fail());
        }
        for p in px.as_chunks_mut::<4>().0 {
            p.swap(0, 2); // BGRA -> RGBA
            p[3] = 255;
        }
        RgbaImage::from_raw(w as u32, h as u32, px).ok_or_else(fail)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SCREEN: Rect = Rect {
        x: -1920,
        y: 0,
        w: 4480,
        h: 1440,
    };

    #[test]
    fn clip_keeps_a_window_inside() {
        let r = Rect {
            x: 100,
            y: 50,
            w: 1280,
            h: 720,
        };
        assert_eq!(clip(r, SCREEN), Some(r));
    }

    #[test]
    fn clip_trims_to_the_virtual_screen() {
        let r = Rect {
            x: -2100,
            y: -40,
            w: 1000,
            h: 800,
        };
        let want = Rect {
            x: -1920,
            y: 0,
            w: 820,
            h: 760,
        };
        assert_eq!(clip(r, SCREEN), Some(want));
    }

    #[test]
    fn clip_rejects_tiny_or_offscreen() {
        let at = |x, w, h| Rect { x, y: 0, w, h };
        assert_eq!(clip(at(0, 319, 900), SCREEN), None);
        assert_eq!(clip(at(0, 900, 239), SCREEN), None);
        assert_eq!(clip(at(9000, 900, 900), SCREEN), None);
    }

    // Client rects are already physical pixels, so 100/125/150 % scale (1080p/1440p/2160p
    // fullscreen) must come through unchanged.
    #[test]
    fn clip_is_exact_for_common_scales() {
        for (w, h) in [(1920, 1080), (2560, 1440), (3840, 2160)] {
            let screen = Rect { x: 0, y: 0, w, h };
            assert_eq!(clip(screen, screen), Some(screen));
        }
    }

    #[test]
    fn black_detection() {
        let black = RgbaImage::from_pixel(640, 360, image::Rgba([0, 0, 0, 255]));
        assert!(looks_black(&black));
        let mut lit = black.clone();
        lit.put_pixel(0, 0, image::Rgba([200, 180, 90, 255])); // grid corner is sampled
        assert!(!looks_black(&lit));
        assert!(looks_black(&RgbaImage::new(0, 0)));
    }
}
