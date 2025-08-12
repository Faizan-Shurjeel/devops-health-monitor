use std::{sync::Arc, time::Instant};

use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    routing::get,
    Json, Router,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{postgres::PgPoolOptions, FromRow, PgPool};
use tokio::{task::JoinHandle, time::{sleep, Duration}};
use tracing::{error, info, instrument};
use tower_http::{cors::{Any, CorsLayer}, trace::TraceLayer};

// Data models for API responses
#[derive(Serialize, FromRow, Clone)]
struct Target {
    id: i32,
    url: String,
}

#[derive(Serialize, FromRow)]
struct HealthCheckRecord {
    id: i32,
    target_id: i32,
    checked_at: DateTime<Utc>,
    status_code: Option<i32>,
    response_time_ms: Option<i32>,
}

// Shared application state
#[derive(Clone)]
struct AppState {
    pool: PgPool,
}

// --------- Routes ---------

#[instrument(skip(state))]
async fn list_targets(State(state): State<AppState>) -> impl IntoResponse {
    let rows = sqlx::query_as::<_, Target>(
        r#"SELECT id, url FROM targets ORDER BY id"#
    )
    .fetch_all(&state.pool)
    .await;

    match rows {
        Ok(targets) => (StatusCode::OK, Json(targets)).into_response(),
        Err(e) => {
            error!(error = %e, "failed to fetch targets");
            (StatusCode::INTERNAL_SERVER_ERROR, "DB error").into_response()
        }
    }
}

#[instrument(skip(state))]
async fn get_status(Path(target_id): Path<i32>, State(state): State<AppState>) -> impl IntoResponse {
    let rows = sqlx::query_as::<_, HealthCheckRecord>(
        r#"
        SELECT id, target_id, checked_at, status_code, response_time_ms
        FROM health_checks
        WHERE target_id = $1
        ORDER BY checked_at DESC
        LIMIT 50
        "#
    )
    .bind(target_id)
    .fetch_all(&state.pool)
    .await;

    match rows {
    Ok(recs) => (StatusCode::OK, Json(recs)).into_response(),
        Err(e) => {
            error!(error = %e, "failed to fetch health check records");
            (StatusCode::INTERNAL_SERVER_ERROR, "DB error").into_response()
        }
    }
}

// --------- Background worker ---------

/// Periodically (every 60s) fetches targets and checks their HTTP status and latency.
async fn start_background_worker(state: AppState) -> JoinHandle<()> {
    tokio::spawn(async move {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(20))
            .build()
            .expect("failed to build reqwest client");

        loop {
            if let Err(e) = tick(&state, &client).await {
                error!(error = %e, "background tick failed");
            }
            sleep(Duration::from_secs(60)).await;
        }
    })
}

#[instrument(skip(state, client))]
async fn tick(state: &AppState, client: &reqwest::Client) -> anyhow::Result<()> {
    let targets = sqlx::query_as::<_, Target>(r#"SELECT id, url FROM targets"#)
        .fetch_all(&state.pool)
        .await?;

    for t in targets {
        let start = Instant::now();
        let (status, latency_ms) = match client.get(&t.url).send().await {
            Ok(resp) => {
                let status = resp.status().as_u16() as i32;
                let _ = resp.bytes().await; // drain body to measure full latency
                (Some(status), Some(start.elapsed().as_millis() as i32))
            }
            Err(err) => {
                error!(target = %t.url, error = %err, "request failed");
                (None, None)
            }
        };

        if let Err(e) = sqlx::query(
            r#"
            INSERT INTO health_checks (target_id, status_code, response_time_ms)
            VALUES ($1, $2, $3)
            "#,
        )
        .bind(t.id)
        .bind(status)
        .bind(latency_ms)
        .execute(&state.pool)
        .await
        {
            error!(target_id = t.id, error = %e, "failed to insert health check");
        }
    }

    Ok(())
}

// --------- Shuttle entrypoint ---------

/// Shuttle entrypoint that provisions the database, builds the Axum router, and launches a background worker.
///
/// - Uses `shuttle_shared_db::Postgres` to provision or connect to a database in Shuttle.
/// - Creates a shared `sqlx::PgPool` connection pool and runs migrations/schema if provided.
/// - Spawns a Tokio task that periodically checks targets and stores results.
/// - Returns the Axum `Router` wrapped for Shuttle to run as a service.
#[shuttle_runtime::main]
async fn main(
    #[shuttle_shared_db::Postgres] pool: PgPool,
) -> shuttle_axum::ShuttleAxum {
    // Initialize structured logging
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info,tower_http=info".into()))
        .init();

    // Ensure schema exists (Shuttle also supports migrations; here we run our schema.sql on startup when needed)
    // Creating tables idempotently
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS targets (
            id SERIAL PRIMARY KEY,
            url TEXT NOT NULL UNIQUE
        );
        CREATE TABLE IF NOT EXISTS health_checks (
            id SERIAL PRIMARY KEY,
            target_id INTEGER NOT NULL REFERENCES targets(id) ON DELETE CASCADE,
            checked_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
            status_code INTEGER,
            response_time_ms INTEGER
        );
        CREATE INDEX IF NOT EXISTS idx_health_checks_target_checked_at
        ON health_checks (target_id, checked_at DESC);
        "#,
    )
    .execute(&pool)
    .await
    .map_err(|e| shuttle_runtime::CustomError::new(format!("failed to ensure schema: {e}")))?;

    // Optional: seed initial targets from `SEED_URLS` secret (comma-separated)
    if let Ok(seed) = std::env::var("SEED_URLS") {
        for url in seed.split(',').map(|s| s.trim()).filter(|s| !s.is_empty()) {
            if let Err(e) = sqlx::query("INSERT INTO targets (url) VALUES ($1) ON CONFLICT DO NOTHING")
                .bind(url)
                .execute(&pool)
                .await
            {
                error!(%url, error = %e, "failed to seed target");
            }
        }
    }

    let state = AppState { pool: pool.clone() };

    // CORS for frontend on Vercel and local dev
    let cors = CorsLayer::new().allow_origin(Any).allow_methods(Any).allow_headers(Any);

    let app = Router::new()
        .route("/api/targets", get(list_targets))
        .route("/api/status/:target_id", get(get_status))
        .with_state(state.clone())
        .layer(TraceLayer::new_for_http())
        .layer(cors);

    // Start background worker
    let _worker: JoinHandle<()> = start_background_worker(state);

    info!("service started");

    Ok(app.into())
}
