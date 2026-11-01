//! Runtime loading and model sessions.

use std::path::Path;
use std::sync::OnceLock;

use ort::environment::Environment;
use ort::session::Session;
use ort::session::builder::GraphOptimizationLevel;

static ENVIRONMENT: OnceLock<Environment> = OnceLock::new();

/// The process-wide environment (ort allows one): loads the DLL by absolute path and checks its
/// version against the `ort` API level. Fails (instead of panicking inside `ort`) when the DLL is
/// absent, unloadable or too old; failures are not cached.
pub fn environment(dll: &Path) -> Result<&'static Environment, String> {
    check_dll(dll)?;
    if let Some(env) = ENVIRONMENT.get() {
        return Ok(env);
    }
    let env = build_environment(dll)?;
    Ok(ENVIRONMENT.get_or_init(|| env))
}

/// Checked before the process-wide cache, so a bad path fails the same whether or not another
/// caller already loaded the runtime.
fn check_dll(dll: &Path) -> Result<(), String> {
    if !dll.is_absolute() {
        return Err(format!(
            "onnxruntime path is not absolute: {}",
            dll.display()
        ));
    }
    if !dll.is_file() {
        return Err(format!("onnxruntime.dll not found at {}", dll.display()));
    }
    Ok(())
}

fn build_environment(dll: &Path) -> Result<Environment, String> {
    ort::init_from(dll)
        .map_err(|e| e.to_string())?
        .build()
        .map_err(|e| e.to_string())
}

/// Inference runs next to the game's render thread: few threads, never all the cores.
pub fn load_model(env: &Environment, model: &Path) -> Result<Session, String> {
    let cores = std::thread::available_parallelism().map_or(2, |n| n.get());
    let build = || -> ort::Result<Session> {
        Session::builder(env)?
            .with_optimization_level(GraphOptimizationLevel::All)?
            .with_inter_threads(1)?
            .with_intra_threads(cores.saturating_sub(4).clamp(2, 4))?
            .commit_from_file(model)
    };
    build().map_err(|e| format!("{}: {e}", model.display()))
}
