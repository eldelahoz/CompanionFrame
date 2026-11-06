pub mod commands;
pub mod db;
pub mod platform;
pub mod services;
pub mod store;
mod tray;
mod window_state;

use tauri::Manager;
use tauri_specta::{Builder, collect_commands};

/// Daily log files kept; older ones are deleted when the appender starts and on each rotation.
const LOG_FILES_KEPT: usize = 7;

/// Daily file in the app log dir, plus stdout in debug builds. Keep the guard alive to flush.
fn init_logging(dir: &std::path::Path) -> tracing_appender::non_blocking::WorkerGuard {
    use tracing_subscriber::{EnvFilter, Layer, layer::SubscriberExt, util::SubscriberInitExt};

    // One filter for both outputs; third-party crates stay at info unless the env says otherwise.
    let filter = ["COMPANIONFRAME_LOG", "RUST_LOG"]
        .iter()
        .find_map(|k| std::env::var(k).ok())
        .and_then(|v| EnvFilter::try_new(v).ok())
        .unwrap_or_else(|| {
            EnvFilter::new(if cfg!(debug_assertions) {
                "info,companionframe_lib=debug"
            } else {
                "info"
            })
        });

    let appender = tracing_appender::rolling::Builder::new()
        .rotation(tracing_appender::rolling::Rotation::DAILY)
        .filename_prefix("companionframe.log")
        .max_log_files(LOG_FILES_KEPT)
        .build(dir);
    let (file, guard) = match appender {
        Ok(a) => tracing_appender::non_blocking(a),
        Err(_) => tracing_appender::non_blocking(std::io::sink()),
    };
    tracing_subscriber::registry()
        .with(filter)
        .with(
            tracing_subscriber::fmt::layer()
                .with_writer(file)
                .with_ansi(false),
        )
        .with(cfg!(debug_assertions).then(|| tracing_subscriber::fmt::layer().boxed()))
        .init();
    guard
}

/// The command list shared by the app and `examples/export_bindings.rs`.
pub fn specta_builder() -> Builder<tauri::Wry> {
    Builder::<tauri::Wry>::new()
        .typ::<services::api_helper::download::HelperDownloadProgress>()
        .typ::<services::mission_rewards::MissionRewardsPayload>()
        .typ::<services::trackers::TradeDetected>()
        .typ::<services::eelog_parsers::arbi::ArbiLive>()
        .typ::<services::overlay::trade::TradeOverlayView>()
        .typ::<services::reward_scan::payload::RewardOverlayView>()
        .typ::<services::wfm::catalog::SetLookup>()
        .typ::<services::wfm::my_orders::CachedOrders>()
        .typ::<services::jobs::JobUpdate>()
        .typ::<services::overlay::riven::RivenOverlayView>()
        .typ::<services::relic_planner::RelicPlannerEvent>()
        .commands(collect_commands![
            commands::app_info,
            commands::get_settings,
            commands::set_settings,
            services::zoom::set_zoom,
            services::wfm::session::wfm_sign_in,
            services::wfm::session::wfm_sign_out,
            services::wfm::session::wfm_session_status,
            services::wfm::webview_login::wfm_sign_in_webview,
            services::wfm::webview_login::wfm_sign_in_webview_cancel,
            services::item_db::get_item_database,
            services::item_db::set_game_locale,
            services::item_db::ocr_language_status,
            services::item_db::ocr_language_install,
            services::item_db::game_language_detected,
            services::eelog::eelog_status,
            services::api_helper::helper_get_status,
            services::api_helper::helper_run_now,
            services::api_helper::helper_download,
            services::inventory::commands::get_inventory,
            services::inventory::commands::get_inventory_status,
            services::inventory::commands::import_inventory_path,
            services::inventory::commands::clear_inventory,
            services::inventory::commands::inventory_apply_pick,
            services::inventory::commands::open_inventory_file,
            services::inventory::commands::open_alecaframe_inventory_file,
            services::inventory::commands::find_alecaframe_cache,
            services::inventory::commands::export_inventory,
            services::relics::get_relic_database,
            services::relics::get_relic_data_info,
            services::spawn_nodes::spawn_nodes_get,
            services::drop_data::drop_search,
            services::drop_data::drop_item_sources,
            services::world_state::get_world_state,
            services::mastery::get_mastery_progress,
            services::profile::get_codex_scans,
            services::profile::profile_get_account,
            services::profile::profile_set_account,
            services::profile::personal::get_personal_profile,
            services::profile::personal::get_personal_loadouts,
            services::ledger::ledger_get_range,
            services::ledger::ledger_add,
            services::ledger::ledger_edit,
            services::ledger::ledger_undo_batch,
            services::ledger::ledger_list_batches,
            services::ledger::gdpr_preview_file,
            services::ledger::gdpr_apply,
            services::trackers::trackers_status,
            services::wfm::catalog::get_wfm_items,
            services::wfm::catalog::wfm_item,
            services::wfm::riven_items::wfm_riven_weapons,
            services::wfm::prices::wfm_price,
            services::wfm::prices::wfm_prices_bulk,
            services::wfm::prices::wfm_prices_load,
            services::wfm::prices::wfm_stats_history,
            services::wfm::live::wfm_presence_get,
            services::wfm::live::wfm_presence_set,
            services::wfm::live::wfm_presence_set_options,
            services::wfm::live::wfm_ws_status,
            services::wfm::my_orders::wfm_orders_my,
            services::wfm::my_orders::wfm_orders_my_cached,
            services::wfm::my_orders::wfm_orders_refresh,
            services::wfm::my_orders::wfm_orders_set_all_visible,
            services::wfm::my_orders::wfm_order_create,
            services::wfm::my_orders::wfm_order_update,
            services::wfm::my_orders::wfm_order_delete,
            services::wfm::my_orders::wfm_order_close,
            services::wfm::orders::wfm_orders_item,
            services::notify::notif_get_state,
            services::notify::notif_set_webhook,
            services::notify::notif_clear_webhook,
            services::notify::notif_test_webhook,
            services::notify::notif_set_source,
            services::notify::notif_set_game_gate,
            services::notify::notif_set_discord_ping,
            services::notify::notif_log_get,
            services::notify::notif_log_remove,
            services::notify::notif_log_clear,
            services::notify::notif_sound_get,
            services::notify::notif_sound_set,
            services::notify::notif_sound_reset,
            services::notify::notif_sound_set_prefs,
            services::notify::notif_sound_preview,
            services::world_state::notify::world_notify_get,
            services::world_state::notify::world_notify_set,
            services::market_alerts::market_alerts_get,
            services::market_alerts::market_alerts_set,
            services::market_alerts::market_alerts_delete,
            services::market_alerts::market_alerts_set_enabled,
            services::market_alerts::market_alerts_clear_cooldown,
            services::market_alerts::market_alerts_test,
            services::market_alerts::market_alerts_hits_get,
            services::market_alerts::market_alerts_hits_clear,
            services::market_alerts::market_alerts_status,
            services::market_alerts::market_alerts_export,
            services::market_alerts::market_alerts_import,
            services::runs::arbi_runs_get,
            services::runs::arbi_run_get,
            services::runs::arbi_run_set_vitus,
            services::runs::run_set_tags,
            services::runs::run_set_notes,
            services::runs::run_delete_log,
            services::runs::run_delete,
            services::runs::arbi_import_log,
            services::rivens::riven_best_attributes,
            services::rivens::rivens_get,
            services::rivens::rivens_good_rolls_info,
            services::rivens::rivens_refresh_good_rolls,
            services::rivens::rivens_decoded,
            services::rivens::rivens_set_note,
            services::rivens::rivens_pin,
            services::rivens::rivens_delete,
            services::runs::pt_runs_get,
            services::runs::pt_run_get,
            services::runs::pt_import_log,
            services::wfm::contracts::wfm_contracts_my,
            services::wfm::riven_search::wfm_riven_search,
            services::wfm::riven_search::wfm_riven_auction_create,
            services::wfm::riven_search::wfm_riven_auction_update,
            services::wfm::riven_search::wfm_riven_auction_close,
            services::wfm::reviews::wfm_send_plus_rep,
            services::wfm::reviews::wfm_my_reputation,
            services::jobs::commands::jobs_start_orders_visibility,
            services::jobs::commands::jobs_start_orders_delete,
            services::jobs::commands::jobs_start_reprice,
            services::jobs::commands::jobs_stop,
            services::jobs::commands::jobs_retry_failed,
            services::jobs::commands::jobs_continue,
            services::jobs::commands::jobs_dismiss,
            services::jobs::commands::jobs_list,
            services::workbench::commands::workbench_get_state,
            services::workbench::commands::workbench_preview_plan,
            services::workbench::commands::workbench_execute_plan,
            services::workbench::commands::workbench_cancel_run,
            services::workbench::commands::workbench_ack_override,
            services::workbench::commands::workbench_reconcile,
            services::workbench::commands::workbench_resolve_review,
            services::workbench::commands::workbench_match_trade,
            services::workbench::commands::workbench_close_match,
            services::arbi_schedule::arbi_schedule_get,
            services::arbi_schedule::arbi_schedule_set_occurrence,
            services::arbi_schedule::arbi_schedule_set_favorite,
            services::arbi_schedule::arbi_schedule_set_lead,
            services::mission_rewards::mission_rewards_page,
            services::mission_rewards::mission_rewards_set_tracking,
            services::stats::stats_get_session,
            services::stats::stats_get_daily,
            services::stats::stats_import,
            services::stats::stats_reset,
            services::capture::capture_game_window_info,
            services::overlay::overlay_show,
            services::overlay::overlay_hide,
            services::overlay::overlay_set_interactive,
            services::overlay::overlay_set_regions,
            services::overlay::overlay_move_mode,
            services::overlay::trade::trade_notification_dismiss,
            services::overlay::trade::trade_notification_close_match,
            services::overlay::trade::dev_trade_sample,
            services::overlay::reward::reward_overlay_close,
            services::overlay::reward::reward_scan_debug,
            services::overlay::riven::riven_overlay_scan,
            services::overlay::riven::riven_overlay_apply_label,
            services::overlay::riven::riven_session_start,
            services::overlay::riven::riven_session_roll_done,
            services::overlay::riven::riven_session_end,
            services::overlay::layout::overlay_layout_get,
            services::overlay::layout::overlay_layout_save,
            services::overlay::layout::overlay_layout_reset,
            services::overlay::placement::overlay_monitors,
            services::overlay::placement::overlay_placement_detect,
            services::overlay::placement::overlay_placement_get,
            services::overlay::placement::overlay_placement_save,
            services::overlay::placement::overlay_placement_save_scale,
            services::overlay::placement::overlay_placement_reset,
            services::onboarding::onboarding_get,
            services::onboarding::onboarding_set_setup_done,
            services::onboarding::onboarding_set_tour_seen,
            services::relic_planner::relic_planner_trigger,
            services::relic_planner::relic_planner_close,
            services::hotkeys::hotkey_capture,
            services::ocr_onnx::ocr_onnx_status,
            window_state::window_minimize,
            window_state::window_toggle_maximize,
            window_state::window_close,
            services::popout::popout_open,
        ])
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // Two rustls backends are compiled in (ring via the updater, aws-lc-rs via reqwest), so rustls
    // cannot pick one by itself and panics on the first TLS connection: choose it before any.
    let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();
    let builder = specta_builder();
    #[cfg(debug_assertions)]
    builder
        .export(
            specta_typescript::Typescript::default(),
            "../src/bindings.ts",
        )
        .expect("failed to export typescript bindings");

    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _, _| {
            if let Some(w) = app.get_webview_window("main") {
                let _ = w.show();
                let _ = w.unminimize();
                let _ = w.set_focus();
            }
        }))
        .register_asynchronous_uri_scheme_protocol("wfmimg", |ctx, req, reply| {
            services::wfm_images::respond(ctx.app_handle().clone(), req, reply)
        })
        .register_asynchronous_uri_scheme_protocol("gameimg", |ctx, req, reply| {
            services::wfm_images::respond_game(ctx.app_handle().clone(), req, reply)
        })
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .invoke_handler(builder.invoke_handler())
        .on_window_event(window_state::on_event)
        .setup(move |app| {
            let path = app.path();
            let guard = init_logging(&path.app_log_dir()?);
            app.manage(guard);
            let data = path.app_data_dir()?;
            std::fs::create_dir_all(&data)?;
            let store = store::Store::load(data.join("settings.json"));
            services::memory_gate::apply(&store.get());
            services::scan_captures::init(&path.app_log_dir()?, &store.get());
            app.manage(store);
            app.manage(db::open(&data.join("companionframe.db"))?);
            app.manage(services::gdpr_import::GdprStaging::default());
            app.manage(services::wfm::session::Session::new(
                std::sync::Arc::new(services::wfm::session::KeyringStore),
                services::wfm::session::device_id(&data),
            )?);
            services::wfm_images::setup(app)?;
            services::wfm::live::setup(app);
            app.manage(services::wfm::prices::Prices::default());
            app.manage(services::wfm::my_orders::MyOrdersCache::default());
            services::wfm::my_orders::start_poller(app.handle().clone());
            services::item_db::setup(app);
            services::mastery::setup(app)?;
            services::eelog::start(app.handle());
            services::trackers::setup(app.handle());
            services::inventory::commands::setup(app)?;
            services::api_helper::setup(app)?;
            services::relics::setup(app);
            app.manage(services::spawn_nodes::SpawnNodes::default());
            services::drop_data::setup(app);
            services::notify::setup(app)?;
            services::market_alerts::setup(app);
            services::world_state::setup(app);
            app.manage(services::wfm::contracts::Contracts::default());
            app.manage(services::wfm::riven_search::SearchCache::default());
            services::rivens::setup(app);
            services::arbi_schedule::setup(app);
            services::jobs::commands::setup(app);
            services::workbench::commands::setup(app);
            services::stats::setup(app);
            app.manage(window_state::WindowState::new(&data));
            window_state::restore(app.handle());
            tray::setup(app)?;
            #[cfg(windows)]
            services::overlay::setup(app)?;
            services::hotkeys::setup(app)?;
            services::profile::start_auto_refresh(app.handle());
            tracing::info!(data_dir = %data.display(), "started");
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
