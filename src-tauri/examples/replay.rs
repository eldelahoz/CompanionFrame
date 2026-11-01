//! Offline replay: real screenshots -> the real scanners -> the overlay view JSON the backend would emit.
//!
//!   cargo run --example replay -- [dir]      (dir is required, or set $CF_SCREENSHOTS)
//!
//! Per `<name>.jpg|png` it writes `target/replay/<name>.json` (`{screen, locale, events:[{event, payload}]}`)
//! and `<name>.log`; `scripts/replay/render.mjs` draws the events over the screenshot. The market is a
//! stub (no network), the inventory is empty, and the screenshots never leave their folder.
//! `CF_REPLAY_LOCALE=en` forces the game language of every image; otherwise `en` for the names in `EN`, else `fr`.
#[cfg(windows)]
mod imp {
    use std::collections::HashMap;
    use std::fmt::Write as _;
    use std::path::{Path, PathBuf};
    use std::time::{Duration, Instant};

    use companionframe_lib::platform::windows::{ocr, reward::WindowsReader};
    use companionframe_lib::services::bundled_data::BundledData;
    use companionframe_lib::services::capture::{CaptureKind, CaptureSource, Frame};
    use companionframe_lib::services::game_locale::GameLocale;
    use companionframe_lib::services::item_db::{ItemDb, Sources};
    use companionframe_lib::services::ocr_onnx::{self, paddle};
    use companionframe_lib::services::overlay::riven::{
        self, RivenCardLayout, RivenCardView, RivenOverlayStatus, RivenOverlayView,
        RivenViewLayout, SimilarScope,
    };
    use companionframe_lib::services::overlay::riven_analyzer::analyze;
    use companionframe_lib::services::relic_planner::era as planner_era;
    use companionframe_lib::services::relic_planner::{PlannerPhase, RelicPlannerEvent};
    use companionframe_lib::services::relics::{
        ItemLookup, RelicDatabase, RelicRewardItem, build, reward_items,
    };
    use companionframe_lib::services::reward_scan::counter::find_cards;
    use companionframe_lib::services::reward_scan::enrich::{Ownership, enrich};
    use companionframe_lib::services::reward_scan::era::{EraWords, set_words};
    use companionframe_lib::services::reward_scan::flow::scan_once;
    use companionframe_lib::services::reward_scan::names::{PoolItem, build_pool};
    use companionframe_lib::services::reward_scan::onnx_reader::{NoBands, OnnxReader};
    use companionframe_lib::services::reward_scan::payload::{
        RewardFailure, RewardOverlayPayload, RewardOverlayPhase, RewardOverlaySlot,
        RewardOverlayView,
    };
    use companionframe_lib::services::reward_scan::{REFERENCE_UI_SCALE, RewardReader};
    use companionframe_lib::services::riven_scan::locale::CardLabels;
    use companionframe_lib::services::riven_scan::{self, text::RivenStat};
    use companionframe_lib::services::rivens::best_attributes::GoodRolls;
    use companionframe_lib::services::rivens::data::RivenData;
    use companionframe_lib::services::wfm::contracts::{WfmContract, WfmContractAttribute};
    use image::RgbaImage;
    use serde_json::{Value, json};

    /// Captures in English; every other image is French (the user's game language).
    const EN: [&str; 1] = ["20261029141358_1"];
    /// What the app's own detectors cannot tell apart: it learns compare vs single from EE.log and the
    /// relic screen from the log tag, so the harness takes the user's labels for these captures.
    /// A detector that disagrees is logged, not hidden.
    const SCREENS: [(&str, &str); 2] = [
        ("20261030060449_1", "riven-compare"),
        ("20261030060511_1", "relic"),
    ];
    const ERA_BUDGET: Duration = Duration::from_secs(4);

    struct Still(RgbaImage);
    impl CaptureSource for Still {
        fn capture(&self) -> Result<Frame, String> {
            Ok(Frame {
                image: self.0.clone(),
                kind: CaptureKind::Fixture,
            })
        }
    }

    /// Everything a language needs, built once.
    struct World {
        locale: GameLocale,
        labels: std::sync::Arc<CardLabels>,
        pool: Vec<PoolItem>,
        rewards: HashMap<String, RelicRewardItem>,
        db: ItemDb,
        relics: RelicDatabase,
    }

    struct Ctx {
        root: PathBuf,
        bundled: BundledData,
        riven: RivenData,
        rolls: GoodRolls,
        worlds: HashMap<&'static str, World>,
    }

    impl Ctx {
        fn world(&mut self, code: &str) -> &World {
            let locale = GameLocale::load(code, &self.bundled);
            let key = locale.code();
            self.worlds.entry(key).or_insert_with(|| {
                let db = ItemDb::build(&Sources::load(&self.bundled, None));
                let lookup = ItemLookup {
                    db: &db,
                    locale: &locale,
                };
                let relics = build(&self.bundled.wfcd_items(&["Relics"]).unwrap(), &lookup);
                let rewards = reward_items(&relics, &lookup);
                World {
                    labels: CardLabels::load(&self.bundled, key).into(),
                    pool: build_pool(&rewards),
                    rewards: rewards.into_iter().map(|r| (r.name.clone(), r)).collect(),
                    locale,
                    db,
                    relics,
                }
            })
        }
    }

    /// A listing that carries exactly the card's stats; the price follows the stat qualities, so a
    /// better card is worth more and the compare verdict has something to compare.
    fn stub_pool(card: &RivenCardView, data: &RivenData, stats: &[RivenStat]) -> Vec<WfmContract> {
        let quality: u32 = card.stats.iter().map(|s| s.quality.unwrap_or(50)).sum();
        let (pos, neg) = riven::search_stats(
            data,
            &riven::correct_stats(data, card.weapon.as_deref(), stats),
        );
        let attrs: Vec<WfmContractAttribute> = pos
            .iter()
            .map(|u| (u, true))
            .chain(neg.iter().map(|u| (u, false)))
            .map(|(u, positive)| WfmContractAttribute {
                url_name: u.clone(),
                label: u.clone(),
                value: None,
                positive: Some(positive),
            })
            .collect();
        (0..6u32)
            .map(|i| WfmContract {
                id: format!("stub-{i}"),
                item_name: card.weapon.clone().unwrap_or_default(),
                item_id: None,
                item_url_name: None,
                weapon_url_name: None,
                riven_suffix: None,
                item_thumb: None,
                platinum: 20 + quality / 5 + i * 7,
                buyout_platinum: None,
                starting_platinum: None,
                quantity: 1,
                visible: true,
                mod_rank: Some(8),
                rerolls: None,
                mastery_level: None,
                polarity: None,
                minimal_reputation: None,
                is_direct_sell: true,
                listed_at: None,
                updated_at: None,
                note: None,
                stats: attrs.clone(),
                listing_url: "https://warframe.market/auction/stub".into(),
                source_type: None,
                seller: None,
                seller_status: Some("ingame".into()),
            })
            .collect()
    }

    fn with_analysis(card: &mut RivenCardView, data: &RivenData, stats: &[RivenStat]) {
        if card.weapon.is_none() {
            return;
        }
        let pool = stub_pool(card, data, stats);
        let (pos, neg) = riven::search_stats(
            data,
            &riven::correct_stats(data, card.weapon.as_deref(), stats),
        );
        card.similar_scope = Some(SimilarScope::All);
        card.analysis = Some(analyze(&riven::card_stats(&pos, &neg), &pool));
    }

    fn read_riven(
        ctx: &mut Ctx,
        code: &str,
        frame: &RgbaImage,
        compare: bool,
        log: &mut String,
    ) -> RivenOverlayView {
        let (root, labels) = (
            ctx.root.clone(),
            std::sync::Arc::clone(&ctx.world(code).labels),
        );
        let (data, rolls) = (&ctx.riven, &ctx.rolls);
        let src = Still(frame.clone());
        // The same steps as `riven::scan`, minus the session memory and the network.
        let (mut view, new_stats, old) = if compare {
            let r = riven::read_compare(&root, &src, data, &labels, rolls, 1, true);
            (r.new.0, r.new.1, r.old)
        } else {
            let (v, s) = riven::read_card(
                &root,
                &src,
                RivenCardLayout::Single,
                data,
                &labels,
                rolls,
                1,
            );
            (v, s, None)
        };
        let ready = view.status == RivenOverlayStatus::Ready;
        let _ = writeln!(
            log,
            "riven: status {:?}, new card {}",
            view.status,
            view.card.is_some()
        );
        let mut cur_stats = vec![];
        if compare {
            if let Some((card, stats)) = old.filter(|(c, _)| c.weapon.is_some()) {
                view.layout = RivenViewLayout::Compare;
                view.current = Some(card);
                cur_stats = stats;
                if !ready {
                    view.status = RivenOverlayStatus::Ready;
                    view.message = None;
                }
            } else {
                let _ = writeln!(log, "riven: the OLD (dimmed) card did not read");
            }
        }
        if let Some(c) = view.card.as_mut() {
            with_analysis(c, data, &new_stats);
        }
        if let Some(c) = view.current.as_mut() {
            with_analysis(c, data, &cur_stats);
        }
        if let (Some(cur), Some(new)) = (&view.current, &view.card) {
            view.verdict = riven::compare_verdict(cur, new);
        }
        view
    }

    fn read_reward(
        ctx: &mut Ctx,
        code: &str,
        frame: &RgbaImage,
        log: &mut String,
    ) -> RewardOverlayView {
        let (root, world) = (ctx.root.clone(), ctx.world(code));
        let view = |phase, presentation, failure| RewardOverlayView {
            phase,
            presentation,
            failure,
            auto_close_ms: 120_000,
            token: 1,
        };
        let failed = |f| view(RewardOverlayPhase::Failed, None, Some(f));
        let Ok(p) = paddle(&root) else {
            let _ = writeln!(log, "reward: no ONNX recognizer");
            return failed(RewardFailure::NothingRead);
        };
        // Paddle first, Windows OCR plus Paddle as the second vote, like `overlay::reward::readers`.
        let first = OnnxReader {
            inner: NoBands,
            paddle: p,
        };
        let second = OnnxReader {
            inner: WindowsReader,
            paddle: p,
        };
        let readers: [&dyn RewardReader; 2] = [&first, &second];
        let scanned = scan_once(
            &Still(frame.clone()),
            &readers,
            &world.pool,
            REFERENCE_UI_SCALE,
        );
        let (attempt, debug) = match scanned {
            Ok(Some(x)) => x,
            Ok(None) => {
                let _ = writeln!(log, "reward: no card bar found");
                return failed(RewardFailure::NotRewardScreen);
            }
            Err(e) => {
                let _ = writeln!(log, "reward: {e}");
                return failed(RewardFailure::CaptureUnavailable);
            }
        };
        for (i, c) in debug.slots.iter().enumerate() {
            let _ = writeln!(log, "reward: slot {i} reads {c:?}");
        }
        let _ = writeln!(
            log,
            "reward: {} card(s), matched {:?}",
            attempt.cards, attempt.items
        );
        let names: Vec<&String> = attempt.items.iter().flatten().collect();
        let cards: Vec<_> = names.iter().filter_map(|n| world.rewards.get(*n)).collect();
        if names.len() != attempt.cards || cards.len() != names.len() {
            return failed(RewardFailure::NothingRead);
        }
        let own = Ownership::default();
        let mut slots: Vec<Option<RewardOverlaySlot>> = cards
            .iter()
            .map(|r| {
                Some(RewardOverlaySlot {
                    item: enrich(r, &world.db, &own, None),
                    price: None,
                    set_price: None,
                })
            })
            .collect();
        let count = slots.len() as u8;
        slots.resize(4, None);
        view(
            RewardOverlayPhase::Ready,
            Some(RewardOverlayPayload { count, slots }),
            None,
        )
    }

    fn read_era(frame: &RgbaImage, label_only: bool) -> planner_era::EraDetection {
        if !ocr::available() {
            return Default::default();
        }
        planner_era::detect(&WindowsReader, frame, label_only, ERA_BUDGET)
    }

    /// The raw text of the relic screen's filter-tab crop (same fractions as `relic_planner::era`).
    fn label_text(frame: &RgbaImage) -> Vec<String> {
        let (w, h) = (frame.width() as f64, frame.height() as f64);
        let crop = image::imageops::crop_imm(
            frame,
            (w * 0.02) as u32,
            (h * 0.085) as u32,
            (w * 0.19) as u32,
            (h * 0.058) as u32,
        )
        .to_image();
        ocr::recognize(&crop)
            .map(|l| l.into_iter().map(|l| l.text).collect())
            .unwrap_or_default()
    }

    pub fn main() {
        let dir = std::env::args()
            .nth(1)
            .or_else(|| std::env::var("CF_SCREENSHOTS").ok())
            .unwrap_or_else(|| {
                eprintln!(
                    "replay: pass the screenshots folder as an argument or set CF_SCREENSHOTS"
                );
                std::process::exit(2)
            });
        let dir = Path::new(&dir);
        if !dir.is_dir() {
            println!("replay: {} is not there, skipping", dir.display());
            return;
        }
        let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
        let out = manifest.join("target/replay");
        std::fs::create_dir_all(&out).unwrap();
        let root = manifest.join("resources");
        let bundled = BundledData::new(root.join("game-data"));
        let mut ctx = Ctx {
            riven: RivenData::load(&bundled),
            rolls: GoodRolls::new(vec![], None),
            bundled,
            root,
            worlds: HashMap::new(),
        };
        let mut shots: Vec<PathBuf> = std::fs::read_dir(dir)
            .unwrap()
            .flatten()
            .map(|e| e.path())
            .filter(|p| {
                matches!(
                    p.extension().and_then(|e| e.to_str()),
                    Some("jpg" | "jpeg" | "png")
                )
            })
            .collect();
        shots.sort();
        let forced = std::env::var("CF_REPLAY_LOCALE").ok();
        for path in shots {
            let stem = path.file_stem().unwrap().to_string_lossy().into_owned();
            let code = forced.clone().unwrap_or_else(|| {
                if EN.contains(&stem.as_str()) {
                    "en"
                } else {
                    "fr"
                }
                .into()
            });
            let frame = match image::open(&path) {
                Ok(i) => i.to_rgba8(),
                Err(e) => {
                    println!("{stem}: unreadable ({e})");
                    continue;
                }
            };
            let started = Instant::now();
            let mut log = format!(
                "{stem} {}x{} game language {code}\n",
                frame.width(),
                frame.height()
            );
            // Same game-wide state the app sets when the language is applied.
            let world = ctx.world(&code);
            let locale = world.locale.clone();
            set_words(EraWords::build(|k| {
                Some(locale.localize(Some(k), "")).filter(|s| !s.is_empty())
            }));
            ocr_onnx::set_language(locale.code());
            ocr::set_language(locale.code());

            // Screen detection: what each of the app's detectors says about the frame.
            let (lang, pack) = ocr::language_status();
            let _ = writeln!(
                log,
                "windows ocr: available {}, game language {lang}, pack {pack:?}",
                ocr::available()
            );
            let label = label_text(&frame);
            let _ = writeln!(log, "detect: filter-label crop reads {label:?}");
            let era = read_era(&frame, true);
            let (reward_cards, scale) = find_cards(&frame, REFERENCE_UI_SCALE);
            let located = riven_scan::locate_cards(&frame);
            let dimmed = riven_scan::locate_dimmed(&frame, &located);
            let _ = writeln!(log, "detect: relic filter label {era:?}");
            let _ = writeln!(
                log,
                "detect: reward cards {reward_cards} (ui scale {scale})"
            );
            let _ = writeln!(log, "detect: riven cards {located:?}, dimmed {dimmed:?}");
            let detected = if era.era.is_some()
                && era.candidate.as_deref() == Some(planner_era::FILTER_LABEL)
            {
                "relic"
            } else if reward_cards > 0 {
                "reward"
            } else if !located.is_empty() && dimmed.is_some() {
                "riven-compare"
            } else if !located.is_empty() {
                "riven-single"
            } else {
                "unknown"
            };
            let hint = SCREENS.iter().find(|(n, _)| *n == stem).map(|(_, s)| *s);
            let screen = hint.unwrap_or(detected);
            let _ = writeln!(
                log,
                "screen: detected {detected}, used {screen}{}",
                if screen == detected {
                    ""
                } else {
                    " (labelled)"
                }
            );

            let events: Vec<Value> = match screen {
                "relic" => {
                    let full = read_era(&frame, false);
                    let _ = writeln!(log, "relic: era {full:?}");
                    let ev = RelicPlannerEvent {
                        token: 1,
                        phase: PlannerPhase::Ready,
                        era: full.era.clone(),
                        confidence: full.confidence,
                        ocr_unavailable: !ocr::available(),
                    };
                    let db = serde_json::to_value(&ctx.world(&code).relics).unwrap();
                    std::fs::write(out.join("relic-db.json"), db.to_string()).unwrap();
                    vec![json!({"event": "relic-planner", "payload": ev})]
                }
                "reward" => {
                    let v = read_reward(&mut ctx, &code, &frame, &mut log);
                    vec![json!({"event": "reward-overlay", "payload": v})]
                }
                "riven-single" | "riven-compare" => {
                    let v =
                        read_riven(&mut ctx, &code, &frame, screen == "riven-compare", &mut log);
                    vec![json!({"event": "riven-overlay", "payload": v})]
                }
                _ => vec![],
            };
            let _ = writeln!(log, "done in {:?}", started.elapsed());
            let doc = json!({"image": stem, "locale": code, "screen": screen, "events": events});
            std::fs::write(
                out.join(format!("{stem}.json")),
                serde_json::to_string_pretty(&doc).unwrap(),
            )
            .unwrap();
            std::fs::write(out.join(format!("{stem}.log")), &log).unwrap();
            print!("{log}");
        }
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
