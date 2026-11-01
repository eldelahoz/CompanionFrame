//! cargo run --example ocr_fixtures -- <fixtures/public dir>   (manifest.json + PNGs)
//! cargo run --example ocr_fixtures -- --screen                (5 s delay, then OCR the primary screen)
//! cargo run --release --example ocr_fixtures -- --scan        (5 s delay, then scan the primary screen as a reward screen)
#[cfg(windows)]
mod imp {
    use companionframe_lib::platform::windows::{capture, ocr, reward::WindowsReader};
    use companionframe_lib::services::ocr_onnx::{PaddleModel, paddle_model};
    use companionframe_lib::services::reward_scan::{
        REFERENCE_UI_SCALE, RewardReader,
        names::PoolItem,
        onnx_reader::OnnxReader,
        slots::{match_slot, scan_frame, similarity},
    };

    /// No Windows reads: scores the Paddle model on its own.
    struct NoReader;
    impl RewardReader for NoReader {
        fn ocr_lines(&self, _: &image::RgbaImage) -> Vec<String> {
            vec![]
        }
    }

    pub fn main() {
        let arg = std::env::args().nth(1).expect("fixtures dir or --screen");
        if arg == "--screen" {
            std::thread::sleep(std::time::Duration::from_secs(5));
            let img = capture::capture(None).expect("capture");
            for l in ocr::recognize(&img).expect("ocr") {
                println!("{}", l.text);
            }
            return;
        }
        if arg == "--scan" {
            std::thread::sleep(std::time::Duration::from_secs(5));
            let img = capture::capture(None).expect("capture");
            let t = std::time::Instant::now();
            match scan_frame(&WindowsReader, &img, REFERENCE_UI_SCALE) {
                None => println!("no reward cards found"),
                Some(scan) => {
                    println!("{} card(s) in {} ms", scan.cards, t.elapsed().as_millis());
                    for s in scan.slots {
                        println!("slot {}: {:?}", s.index, s.candidates);
                    }
                }
            }
            return;
        }
        let dir = std::path::Path::new(&arg);
        // Without the runtime/models (`pnpm fetch-onnx`) the second column equals the first.
        // OCR_MODEL=latin compares the multilingual Latin recognizer against the default CH one.
        let model = match std::env::var("OCR_MODEL").as_deref() {
            Ok("latin") => PaddleModel::Latin,
            _ => PaddleModel::Ch,
        };
        println!("paddle model: {model:?}");
        let onnx = paddle_model(&std::fs::canonicalize("resources").unwrap(), model)
            .map_err(|e| println!("no ONNX: {e}"));
        // Pool = every reward item of the bundled relic data (English), like the live matcher's.
        let relics: serde_json::Value = serde_json::from_str(
            &std::fs::read_to_string("resources/game-data/wfcd/Relics.json")
                .expect("pnpm game-data"),
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
        println!("pool: {} items", pool.len());
        let manifest: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(dir.join("manifest.json")).unwrap())
                .unwrap();
        let (mut ok, mut win_ok, mut onnx_ok, mut primary_ok, mut total, mut counted) =
            (0, 0, 0, 0, 0, 0);
        println!(
            "| fixture | slot | expected | got (windows + onnx) | sim | windows alone | with onnx | onnx alone |"
        );
        println!("|---|---|---|---|---|---|---|---|");
        for f in manifest["fixtures"].as_array().unwrap() {
            let name = f["file"].as_str().unwrap();
            let img = image::open(dir.join(name)).unwrap().to_rgba8();
            let expected: Vec<&str> = f["expected"]
                .as_array()
                .unwrap()
                .iter()
                .map(|e| e.as_str().unwrap())
                .collect();
            let win_scan = scan_frame(&WindowsReader, &img, REFERENCE_UI_SCALE);
            let t = std::time::Instant::now();
            let scan = match &onnx {
                Ok(paddle) => scan_frame(
                    &OnnxReader {
                        inner: WindowsReader,
                        paddle,
                    },
                    &img,
                    REFERENCE_UI_SCALE,
                ),
                Err(_) => win_scan.clone(),
            };
            let ms = t.elapsed().as_millis();
            let cards = scan.as_ref().map_or(0, |s| s.cards);
            counted += usize::from(cards == expected.len());
            println!(
                "| {name} | cards {cards}/{} | {ms} ms | | | |",
                expected.len()
            );
            let Some(scan) = scan else {
                total += expected.len();
                continue;
            };
            let win_slots = win_scan.map(|s| s.slots).unwrap_or_default();
            let onnx_slots = match &onnx {
                Ok(paddle) => scan_frame(
                    &OnnxReader {
                        inner: NoReader,
                        paddle,
                    },
                    &img,
                    REFERENCE_UI_SCALE,
                )
                .map(|s| s.slots)
                .unwrap_or_default(),
                Err(_) => vec![],
            };
            for (slot, exp) in scan.slots.iter().zip(&expected) {
                // The matcher drops quantity prefixes ("2 X Forma Blueprint"), so compare without it.
                let want = exp.split_once(" X ").map_or(*exp, |(_, rest)| rest);
                let best = |texts: &[String]| {
                    match_slot(texts, &pool)
                        .into_iter()
                        .next()
                        .and_then(|c| c.item)
                        .map_or_else(String::new, |i| i.name)
                };
                let got = best(&slot.candidates);
                let win = win_slots
                    .get(slot.index)
                    .map_or_else(String::new, |s| best(&s.candidates));
                let alone = onnx_slots.get(slot.index).map_or_else(String::new, |s| {
                    format!("{} ({})", best(&s.candidates), s.candidates.join(" / "))
                });
                let sim = similarity(&got, want);
                let win_pass = similarity(&win, want) >= 1.0;
                let onnx_pass = alone.starts_with(&format!("{want} ("));
                // The live order (overlay::reward): Paddle alone first, Windows + Paddle only for a slot it left unread.
                let alone_best = onnx_slots
                    .get(slot.index)
                    .map_or_else(String::new, |s| best(&s.candidates));
                let primary = if alone_best.is_empty() {
                    &got
                } else {
                    &alone_best
                };
                primary_ok += usize::from(similarity(primary, want) >= 1.0);
                ok += usize::from(sim >= 1.0);
                win_ok += usize::from(win_pass);
                onnx_ok += usize::from(onnx_pass);
                let mark = |p: bool| if p { "ok" } else { "MISS" };
                println!(
                    "| {name} | {} | {exp} | {got} | {sim:.2} | {} | {} | {} {alone} |",
                    slot.index,
                    mark(win_pass),
                    mark(sim >= 1.0),
                    mark(onnx_pass)
                );
            }
            total += expected.len();
        }
        println!(
        "
score: {ok}/{total} matched to the right item with ONNX fallback ({win_ok}/{total} Windows OCR alone, {onnx_ok}/{total} ONNX alone, {primary_ok}/{total} Paddle primary + Windows fallback); card count right on {counted}/{} frames",
        manifest["fixtures"].as_array().unwrap().len()
    );
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
