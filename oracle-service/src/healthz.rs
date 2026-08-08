//! Process liveness HTTP probe (`GET /healthz`).
//!
//! Returns **200 OK** when the process is running. This does **not** assert on-chain oracle
//! rate freshness (see C-3 / `ORACLE_MAX_SILENCE_SECS` log alerts for broadcast liveness).

use axum::http::StatusCode;
use axum::routing::get;
use axum::Router;
use eyre::{eyre, Result};
use tracing::{error, info};

async fn healthz() -> StatusCode {
    StatusCode::OK
}

pub fn healthz_router() -> Router {
    Router::new().route("/healthz", get(healthz))
}

/// Bind and serve `/healthz` in a background task. Returns after the listener is ready.
pub async fn spawn_healthz_server(bind: &str) -> Result<()> {
    let listener = tokio::net::TcpListener::bind(bind)
        .await
        .map_err(|e| eyre!("HEALTHZ_BIND {bind}: {e}"))?;
    let local_addr = listener
        .local_addr()
        .map_err(|e| eyre!("healthz local_addr: {e}"))?;
    info!(
        bind = %local_addr,
        "healthz server listening (process-up only; does not check on-chain rate freshness)"
    );
    tokio::spawn(async move {
        if let Err(e) = axum::serve(listener, healthz_router()).await {
            error!(error = %e, "healthz server exited with error");
        }
    });
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use http_body_util::BodyExt;
    use tower::ServiceExt;

    #[tokio::test]
    async fn healthz_returns_200() {
        let response = healthz_router()
            .oneshot(
                axum::http::Request::builder()
                    .uri("/healthz")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = response.into_body().collect().await.unwrap().to_bytes();
        assert!(body.is_empty());
    }
}
