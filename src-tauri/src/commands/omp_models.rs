use session_core::omp_models_config::{self, DiscoveredModel, OmpProviderConfig};

#[tauri::command]
pub fn omp_list_model_providers() -> Result<Vec<OmpProviderConfig>, String> {
    omp_models_config::list_providers()
}

#[tauri::command]
pub fn omp_save_model_provider(
    config: OmpProviderConfig,
    original_id: Option<String>,
) -> Result<(), String> {
    omp_models_config::save_provider(config, original_id)
}

#[tauri::command]
pub fn omp_delete_model_provider(id: String) -> Result<(), String> {
    omp_models_config::delete_provider(id)
}

#[tauri::command]
pub fn omp_disable_model_provider(id: String) -> Result<(), String> {
    omp_models_config::disable_provider(id)
}

#[tauri::command]
pub fn omp_enable_model_provider(id: String) -> Result<(), String> {
    omp_models_config::enable_provider(id)
}

#[tauri::command]
pub fn omp_preview_model_provider(
    config: OmpProviderConfig,
    original_id: Option<String>,
) -> Result<String, String> {
    omp_models_config::preview_provider(&config, original_id)
}

#[tauri::command]
pub async fn omp_refresh_models(provider: Option<String>) -> Result<Vec<DiscoveredModel>, String> {
    let models = omp_models_config::refresh_models().await?;
    Ok(
        match provider
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
        {
            Some(provider) => models
                .into_iter()
                .filter(|model| model.provider == provider)
                .collect(),
            None => models,
        },
    )
}
