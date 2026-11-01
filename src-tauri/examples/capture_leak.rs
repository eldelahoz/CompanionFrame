//! cargo run --example capture_leak   (from src-tauri/)
//! Captures the primary screen 20 times and fails if the process GDI object count grew.
#[cfg(windows)]
mod imp {
    use companionframe_lib::platform::windows::capture;
    use windows::Win32::System::Threading::{GR_GDIOBJECTS, GetCurrentProcess, GetGuiResources};

    fn gdi() -> u32 {
        // SAFETY: pseudo-handle of the current process.
        unsafe { GetGuiResources(GetCurrentProcess(), GR_GDIOBJECTS) }
    }

    pub fn main() {
        capture::capture(None).expect("warm-up capture");
        let before = gdi();
        for _ in 0..20 {
            let img = capture::capture(None).expect("capture");
            assert!(img.width() > 0);
        }
        let after = gdi();
        println!("GDI objects: {before} -> {after}");
        assert!(after <= before, "GDI handle leak");
    }
}

#[cfg(windows)]
fn main() {
    imp::main()
}

#[cfg(not(windows))]
fn main() {
    eprintln!("Windows only");
}
