//! cargo run --release --example reward_bench -- <fixtures/public dir> [reps]
//! Per-stage timings of the live scan (`scan_once`, Paddle then Windows OCR) over the fixture
//! frames; run with RUST_LOG=debug for the stages inside.
#[cfg(windows)]
mod imp {
    use std::path::Path;
    use std::time::Instant;

    use companionframe_lib::platform::windows::reward::WindowsReader;
    use companionframe_lib::services::capture::{CaptureSource, Frame};
    use companionframe_lib::services::ocr_onnx::paddle;
    use companionframe_lib::services::reward_scan::flow::scan_once;
    use companionframe_lib::services::reward_scan::names::PoolItem;
    use companionframe_lib::services::reward_scan::onnx_reader::{NoBands, OnnxReader};
    use companionframe_lib::services::reward_scan::{REFERENCE_UI_SCALE, RewardReader};

    struct Fixture(image::RgbaImage);
    impl CaptureSource for Fixture {
        fn capture(&self) -> Result<Frame, String> {
            Ok(Frame {
                image: self.0.clone(),
                kind: companionframe_lib::services::capture::CaptureKind::Fixture,
            })
        }
    }

    pub fn main() {
        tracing_subscriber::fmt()
            .with_env_filter(
                tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_default(),
            )
            .with_target(false)
            .without_time()
            .init();
        let dir = std::env::args().nth(1).expect("fixtures dir");
        let reps: usize = std::env::args().nth(2).map_or(3, |r| r.parse().unwrap());
        let dir = Path::new(&dir);
        let t = Instant::now();
        let p = paddle(&std::fs::canonicalize("resources").unwrap()).expect("paddle");
        println!("model load: {} ms", t.elapsed().as_millis());
        let relics: serde_json::Value = serde_json::from_str(
            &std::fs::read_to_string("resources/game-data/wfcd/Relics.json").unwrap(),
        )
        .unwrap();
        let mut names: Vec<String> = relics
            .as_array()
            .unwrap()
            .iter()
            .flat_map(|r| r["rewards"].as_array().into_iter().flatten())
            .filter_map(|r| r["item"]["name"].as_str().map(String::from))
            .collect();
        names.sort();
        names.dedup();
        let pool: Vec<PoolItem> = names.iter().map(|n| PoolItem::new(n, None)).collect();
        let readers: Vec<Box<dyn RewardReader + Send + Sync>> = vec![
            Box::new(OnnxReader {
                inner: NoBands,
                paddle: p,
            }),
            Box::new(OnnxReader {
                inner: WindowsReader,
                paddle: p,
            }),
        ];
        let readers: Vec<&dyn RewardReader> = readers.iter().map(|r| &**r as _).collect();
        let manifest: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(dir.join("manifest.json")).unwrap())
                .unwrap();
        for rep in 0..reps {
            for f in manifest["fixtures"].as_array().unwrap() {
                let name = f["file"].as_str().unwrap();
                let img = image::open(dir.join(name)).unwrap().to_rgba8();
                let t = Instant::now();
                let r = scan_once(&Fixture(img), &readers, &pool, REFERENCE_UI_SCALE);
                let read = r
                    .ok()
                    .flatten()
                    .map_or((0, 0), |(a, _)| (a.items.iter().flatten().count(), a.cards));
                println!(
                    "rep {rep} {name}: {} ms, {}/{} read",
                    t.elapsed().as_millis(),
                    read.0,
                    read.1
                );
            }
        }
    }
}

#[cfg(windows)]
fn main() {
    imp::main();
}

#[cfg(not(windows))]
fn main() {}
