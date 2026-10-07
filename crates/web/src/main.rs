pub mod chart;
mod state;
mod entity;
pub mod object;
pub mod birthplace;

use crate::chart::{BirthChart, BirthChartRepository};
use axum::extract::Path;
use axum::routing::get;
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use state::State;
use sweph::Ayanamsha;
use tower_http::trace::{DefaultMakeSpan, DefaultOnResponse, TraceLayer};
use tracing::Level;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::{EnvFilter, fmt};
use crate::birthplace::{Birthplace, BirthplaceRepository};

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

async fn chart(
    Path(BirthChartRequest {
        latitude,
        longitude,
        time,
    }): Path<BirthChartRequest>,
    chart_repository: BirthChartRepository,
) -> Json<BirthChartResponse> {
    Json(BirthChartResponse {
        chart: chart_repository.calculate(latitude, longitude, time).await.unwrap(),
    })
}

async fn search(
    Path(query): Path<String>,
    birthplace_repository: BirthplaceRepository,
) -> Json<Vec<Birthplace>> {
    Json(birthplace_repository.search(query).await.unwrap())
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let env_filter =
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("debug,webtarot=trace"));

    let fmt_layer = fmt::layer().json().with_target(true);

    tracing_subscriber::registry()
        .with(env_filter)
        .with(fmt_layer)
        .init();

    sweph::set_sidereal_mode(Ayanamsha::DeLuce);
    let app = Router::new()
        .route("/api/v1/chart/{latitude}/{longitude}/{time}", get(chart))
        .route("/api/v1/search/{query}", get(search))
        .with_state(state::State::new().await)
        .nest_service("/images", tower_http::services::ServeDir::new("catalog-images"))
        .layer(
            TraceLayer::new_for_http()
                .make_span_with(DefaultMakeSpan::new().level(Level::INFO))
                .on_response(DefaultOnResponse::new().level(Level::INFO)),
        );
    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await?;
    axum::serve(listener, app).await?;
    Ok(())
}
