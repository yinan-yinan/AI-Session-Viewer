use axum::http::StatusCode;
use axum::response::Json;
use serde::Deserialize;
use session_core::omp_models_config::{self, DiscoveredModel, OmpProviderConfig};

pub async fn list_providers() -> Result<Json<Vec<OmpProviderConfig>>, (StatusCode, String)> {
    tokio::task::spawn_blocking(omp_models_config::list_providers)
        .await
        .map_err(|error| (StatusCode::INTERNAL_SERVER_ERROR, error.to_string()))?
        .map(Json)
        .map_err(|error| (StatusCode::INTERNAL_SERVER_ERROR, error))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveProviderBody {
    pub config: OmpProviderConfig,
    pub original_id: Option<String>,
}

pub async fn save_provider(
    Json(body): Json<SaveProviderBody>,
) -> Result<Json<()>, (StatusCode, String)> {
    tokio::task::spawn_blocking(move || {
        omp_models_config::save_provider(body.config, body.original_id)
    })
    .await
    .map_err(|error| (StatusCode::INTERNAL_SERVER_ERROR, error.to_string()))?
    .map(|()| Json(()))
    .map_err(|error| (StatusCode::BAD_REQUEST, error))
}

#[derive(Deserialize)]
pub struct ProviderIdBody {
    pub id: String,
}

pub async fn delete_provider(
    Json(body): Json<ProviderIdBody>,
) -> Result<Json<()>, (StatusCode, String)> {
    tokio::task::spawn_blocking(move || omp_models_config::delete_provider(body.id))
        .await
        .map_err(|error| (StatusCode::INTERNAL_SERVER_ERROR, error.to_string()))?
        .map(|()| Json(()))
        .map_err(|error| (StatusCode::BAD_REQUEST, error))
}

pub async fn disable_provider(
    Json(body): Json<ProviderIdBody>,
) -> Result<Json<()>, (StatusCode, String)> {
    tokio::task::spawn_blocking(move || omp_models_config::disable_provider(body.id))
        .await
        .map_err(|error| (StatusCode::INTERNAL_SERVER_ERROR, error.to_string()))?
        .map(|()| Json(()))
        .map_err(|error| (StatusCode::BAD_REQUEST, error))
}

pub async fn enable_provider(
    Json(body): Json<ProviderIdBody>,
) -> Result<Json<()>, (StatusCode, String)> {
    tokio::task::spawn_blocking(move || omp_models_config::enable_provider(body.id))
        .await
        .map_err(|error| (StatusCode::INTERNAL_SERVER_ERROR, error.to_string()))?
        .map(|()| Json(()))
        .map_err(|error| (StatusCode::BAD_REQUEST, error))
}

pub async fn preview_provider(
    Json(body): Json<SaveProviderBody>,
) -> Result<Json<String>, (StatusCode, String)> {
    tokio::task::spawn_blocking(move || {
        omp_models_config::preview_provider(&body.config, body.original_id)
    })
    .await
    .map_err(|error| (StatusCode::INTERNAL_SERVER_ERROR, error.to_string()))?
    .map(Json)
    .map_err(|error| (StatusCode::BAD_REQUEST, error))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RefreshBody {
    pub provider: Option<String>,
}

pub async fn refresh_models(
    Json(body): Json<RefreshBody>,
) -> Result<Json<Vec<DiscoveredModel>>, (StatusCode, String)> {
    let models = omp_models_config::refresh_models()
        .await
        .map_err(|error| (StatusCode::BAD_GATEWAY, error))?;
    let provider = body
        .provider
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());
    Ok(Json(match provider {
        Some(provider) => models
            .into_iter()
            .filter(|model| model.provider == provider)
            .collect(),
        None => models,
    }))
}
