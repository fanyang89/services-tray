//! HTTP API: service CRUD, lifecycle control and log streaming (SSE).

use crate::config::ServiceInput;
use crate::logs::LogLine;
use crate::supervisor::Supervisor;
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post, put};
use axum::{Json, Router};
use futures::stream::{self, StreamExt};
use serde::Deserialize;
use serde_json::json;
use std::convert::Infallible;
use std::sync::Arc;
use tokio_stream::wrappers::BroadcastStream;

#[derive(Clone)]
pub struct AppState {
    pub supervisor: Arc<Supervisor>,
}

pub fn router(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/api/health", get(health))
        .route("/api/services", get(list_services).post(create_service))
        .route(
            "/api/services/{id}",
            get(get_service).put(update_service).delete(delete_service),
        )
        .route("/api/services/{id}/autostart", put(set_autostart))
        .route("/api/services/{id}/start", post(start_service))
        .route("/api/services/{id}/stop", post(stop_service))
        .route("/api/services/{id}/restart", post(restart_service))
        .route("/api/services/{id}/logs", get(get_logs))
        .route("/api/services/{id}/logs/stream", get(stream_logs))
        .fallback(crate::frontend::static_handler)
        .layer(tower_http::trace::TraceLayer::new_for_http())
        .with_state(state)
}

pub struct ApiError(pub StatusCode, pub String);

impl ApiError {
    fn bad_request(msg: impl Into<String>) -> Self {
        Self(StatusCode::BAD_REQUEST, msg.into())
    }
    fn not_found(msg: impl Into<String>) -> Self {
        Self(StatusCode::NOT_FOUND, msg.into())
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (self.0, Json(json!({ "error": self.1 }))).into_response()
    }
}

async fn health() -> Json<serde_json::Value> {
    Json(json!({
        "status": "ok",
        "version": env!("CARGO_PKG_VERSION"),
    }))
}

async fn list_services(
    State(state): State<Arc<AppState>>,
) -> Json<Vec<crate::supervisor::ServiceInfo>> {
    Json(state.supervisor.list_infos().await)
}

async fn create_service(
    State(state): State<Arc<AppState>>,
    Json(input): Json<ServiceInput>,
) -> Result<(StatusCode, Json<crate::supervisor::ServiceInfo>), ApiError> {
    let svc = state
        .supervisor
        .create(input)
        .await
        .map_err(ApiError::bad_request)?;
    Ok((StatusCode::CREATED, Json(svc.info().await)))
}

async fn get_service(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<crate::supervisor::ServiceInfo>, ApiError> {
    let svc = state
        .supervisor
        .get(&id)
        .await
        .ok_or_else(|| ApiError::not_found("服务不存在"))?;
    Ok(Json(svc.info().await))
}

async fn update_service(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(input): Json<ServiceInput>,
) -> Result<Json<crate::supervisor::ServiceInfo>, ApiError> {
    let svc = state
        .supervisor
        .update(&id, input)
        .await
        .map_err(ApiError::bad_request)?;
    Ok(Json(svc.info().await))
}

async fn set_autostart(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(body): Json<serde_json::Value>,
) -> Result<Json<crate::supervisor::ServiceInfo>, ApiError> {
    let enabled = body
        .get("enabled")
        .and_then(|v| v.as_bool())
        .ok_or_else(|| ApiError::bad_request("缺少 enabled 字段"))?;
    state
        .supervisor
        .set_autostart(&id, enabled)
        .await
        .map_err(|e| {
            if e == "服务不存在" {
                ApiError::not_found(e)
            } else {
                ApiError::bad_request(e)
            }
        })
        .map(Json)
}

async fn delete_service(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    state.supervisor.delete(&id).await.map_err(|e| {
        if e == "服务不存在" {
            ApiError::not_found(e)
        } else {
            ApiError::bad_request(e)
        }
    })?;
    Ok(Json(json!({ "ok": true })))
}

macro_rules! lifecycle_handler {
    ($name:ident, $action:ident) => {
        async fn $name(
            State(state): State<Arc<AppState>>,
            Path(id): Path<String>,
        ) -> Result<Json<crate::supervisor::ServiceInfo>, ApiError> {
            let svc = state
                .supervisor
                .get(&id)
                .await
                .ok_or_else(|| ApiError::not_found("服务不存在"))?;
            svc.$action().await.map_err(ApiError::bad_request)?;
            Ok(Json(svc.info().await))
        }
    };
}

lifecycle_handler!(start_service, start);
lifecycle_handler!(stop_service, stop);
lifecycle_handler!(restart_service, restart);

#[derive(Debug, Deserialize)]
pub struct LogsQuery {
    pub tail: Option<usize>,
    pub after: Option<u64>,
}

async fn get_logs(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Query(q): Query<LogsQuery>,
) -> Result<Json<Vec<LogLine>>, ApiError> {
    let svc = state
        .supervisor
        .get(&id)
        .await
        .ok_or_else(|| ApiError::not_found("服务不存在"))?;
    let lines = svc
        .logs
        .snapshot(q.after, q.tail.unwrap_or(500).min(5000))
        .await;
    Ok(Json(lines))
}

async fn stream_logs(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Query(q): Query<LogsQuery>,
    headers: HeaderMap,
) -> Result<Sse<impl futures::Stream<Item = Result<Event, Infallible>>>, ApiError> {
    let svc = state
        .supervisor
        .get(&id)
        .await
        .ok_or_else(|| ApiError::not_found("服务不存在"))?;
    // Prefer explicit query param, then the Last-Event-ID header sent by
    // EventSource on automatic reconnect.
    let after = q.after.or_else(|| {
        headers
            .get("last-event-id")
            .and_then(|v| v.to_str().ok())
            .and_then(|s| s.parse().ok())
    });
    let tail = q.tail.unwrap_or(300).min(5000);

    let rx = svc.logs.subscribe();
    let initial = svc.logs.snapshot(after, tail).await;
    let live = BroadcastStream::new(rx).filter_map(|item| {
        std::future::ready(match item {
            Ok(line) => Some(Ok::<_, Infallible>(log_event(&line))),
            // Consumer fell behind; skip (the ring buffer still has everything).
            Err(_) => None,
        })
    });
    let stream = stream::iter(
        initial
            .into_iter()
            .map(|l| Ok::<_, Infallible>(log_event(&l))),
    )
    .chain(live);
    Ok(Sse::new(stream).keep_alive(KeepAlive::default()))
}

fn log_event(line: &LogLine) -> Event {
    Event::default()
        .id(line.seq.to_string())
        .event("log")
        .json_data(line)
        .unwrap_or_else(|_| Event::default().data("{}"))
}
