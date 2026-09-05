//! Service manager library.
//!
//! The supervisor, HTTP API and embedded frontend are exposed as a library so
//! future frontends (e.g. a macOS/Windows tray app) can host the exact same
//! manager inside their own process.

pub mod api;
pub mod config;
pub mod frontend;
pub mod logs;
pub mod supervisor;

pub use supervisor::{ServiceInfo, Status, Supervisor};

use std::path::Path;
use std::sync::Arc;

/// Load the supervisor from `config_path` and build the axum application.
///
/// Returns the router together with the supervisor handle so the caller can
/// trigger a clean shutdown of all managed services on exit.
pub async fn build_app(config_path: &Path) -> anyhow::Result<(axum::Router, Arc<Supervisor>)> {
    let supervisor = Supervisor::load(config_path).await?;
    let app = api::router(Arc::new(api::AppState {
        supervisor: supervisor.clone(),
    }));
    Ok((app, supervisor))
}
