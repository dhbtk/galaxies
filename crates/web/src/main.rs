pub mod chart;

use crate::chart::BirthChart;
use axum::extract::Path;
use axum::response::Response;
use axum::routing::get;
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use sweph::Ayanamsha;
use tower_http::trace::{DefaultMakeSpan, DefaultOnResponse, TraceLayer};
use tracing::Level;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::{EnvFilter, fmt};

#[derive(Clone)]
pub struct State {}

// TODO: figure out a nice way to encode this all into a single string
#[derive(Clone, Deserialize, Debug)]
pub struct BirthChartRequest {
    pub latitude: f32,
    pub longitude: f32,
    pub time: chrono::DateTime<chrono::Utc>,
}

#[derive(Serialize, Clone, Debug)]
pub struct BirthChartResponse {
    pub chart: BirthChart,
}

#[axum::debug_handler]
async fn chart(
    Path(BirthChartRequest {
        latitude,
        longitude,
        time,
    }): Path<BirthChartRequest>,
) -> Json<BirthChartResponse> {
    Json(BirthChartResponse {
        chart: BirthChart::new(latitude, longitude, time),
    })
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let env_filter =
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info,webtarot=trace"));

    let fmt_layer = fmt::layer().json().with_target(true);

    tracing_subscriber::registry()
        .with(env_filter)
        .with(fmt_layer)
        .init();

    sweph::set_sidereal_mode(Ayanamsha::DeLuce);
    let app = Router::new()
        .route("/api/v1/chart/{latitude}/{longitude}/{time}", get(chart))
        .with_state(State {})
        .layer(
            TraceLayer::new_for_http()
                .make_span_with(DefaultMakeSpan::new().level(Level::INFO))
                .on_response(DefaultOnResponse::new().level(Level::INFO)),
        );
    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await?;
    axum::serve(listener, app).await?;
    Ok(())
}
