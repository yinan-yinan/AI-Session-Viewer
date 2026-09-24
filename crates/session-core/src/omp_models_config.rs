use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use serde_yml::{Mapping, Value};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::LazyLock;
use std::time::Duration;
use tokio::process::Command;
use tokio::time::timeout;

const API_FORMATS: &[&str] = &[
    "openai-completions",
    "openai-responses",
    "openai-codex-responses",
    "azure-openai-responses",
    "anthropic-messages",
    "google-generative-ai",
    "google-gemini-cli",
    "google-vertex",
];
const DISCOVERY_TYPES: &[&str] = &[
    "ollama",
    "llama.cpp",
    "lm-studio",
    "openai-models-list",
    "proxy",
    "litellm",
];
const REFRESH_TIMEOUT: Duration = Duration::from_secs(120);

static CONFIG_WRITE_LOCK: LazyLock<Mutex<()>> = LazyLock::new(|| Mutex::new(()));

fn config_write_lock() -> &'static Mutex<()> {
    &CONFIG_WRITE_LOCK
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct OmpModelConfig {
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct OmpProviderConfig {
    pub id: String,
    #[serde(default)]
    pub base_url: String,
    #[serde(default)]
    pub api_key: String,
    #[serde(default)]
    pub api: String,
    #[serde(default)]
    pub user_agent: String,
    #[serde(default)]
    pub discovery_type: String,
    #[serde(default)]
    pub models: Vec<OmpModelConfig>,
    #[serde(default = "default_enabled")]
    pub enabled: bool,
}

fn default_enabled() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DiscoveredModel {
    pub provider: String,
    pub id: String,
    pub name: String,
}

pub fn models_config_path(agent_dir: &Path) -> PathBuf {
    let yml = agent_dir.join("models.yml");
    if yml.is_file() {
        return yml;
    }
    let yaml = agent_dir.join("models.yaml");
    if yaml.is_file() {
        yaml
    } else {
        yml
    }
}
fn disabled_path(agent_dir: &Path, config_dir: &Path) -> PathBuf {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    agent_dir.hash(&mut hasher);
    config_dir
        .join("ai-session-viewer")
        .join(format!("omp-disabled-{:016x}.yml", hasher.finish()))
}

fn read_disabled(path: &Path) -> Result<Mapping, String> {
    if !path.exists() {
        return Ok(Mapping::new());
    }
    let content = fs::read_to_string(path).map_err(|_| "读取禁用供应商配置失败".to_string())?;
    let value: Value =
        serde_yml::from_str(&content).map_err(|_| "禁用供应商配置格式无效".to_string())?;
    value
        .as_mapping()
        .cloned()
        .ok_or_else(|| "禁用供应商配置格式无效".to_string())
}

fn write_disabled(path: &Path, providers: &Mapping) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|_| "创建应用配置目录失败".to_string())?;
    }
    let content = serde_yml::to_string(&Value::Mapping(providers.clone()))
        .map_err(|_| "生成禁用供应商配置失败".to_string())?;
    let temp = path.with_file_name(format!(".omp-disabled-{}.tmp", uuid::Uuid::new_v4()));
    fs::write(&temp, content).map_err(|_| "写入禁用供应商配置失败".to_string())?;
    #[cfg(windows)]
    if path.exists() {
        let previous =
            path.with_file_name(format!(".omp-disabled-{}.previous", uuid::Uuid::new_v4()));
        if let Err(error) = fs::rename(path, &previous) {
            let _ = fs::remove_file(&temp);
            return Err(format!("暂存禁用供应商配置失败: {error}"));
        }
        if let Err(error) = fs::rename(&temp, path) {
            let _ = fs::rename(&previous, path);
            let _ = fs::remove_file(&temp);
            return Err(format!("替换禁用供应商配置失败: {error}"));
        }
        let _ = fs::remove_file(previous);
        return Ok(());
    }
    if let Err(error) = fs::rename(&temp, path) {
        let _ = fs::remove_file(&temp);
        return Err(format!("替换禁用供应商配置失败: {error}"));
    }
    Ok(())
}

fn private_config_path(agent_dir: &Path) -> Result<PathBuf, String> {
    let config_dir = dirs::config_dir().ok_or_else(|| "无法解析应用配置目录".to_string())?;
    Ok(disabled_path(agent_dir, &config_dir))
}

fn read_root(path: &Path) -> Result<Value, String> {
    if !path.exists() {
        return Ok(Value::Mapping(Mapping::new()));
    }
    let content = fs::read_to_string(path).map_err(|_| "读取 OMP 模型配置失败".to_string())?;
    let root: Value =
        serde_yml::from_str(&content).map_err(|_| "OMP 模型配置 YAML 格式无效".to_string())?;
    if !root.is_mapping() {
        return Err("OMP 模型配置根节点必须是对象".to_string());
    }
    Ok(root)
}

fn key(name: &str) -> String {
    name.to_string()
}
fn get<'a>(value: &'a Value, name: &str) -> Option<&'a Value> {
    value.as_mapping()?.get(name)
}
fn text(value: Option<&Value>) -> String {
    value
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string()
}

fn provider_from_value(id: &str, value: &Value) -> OmpProviderConfig {
    let models = get(value, "models")
        .and_then(Value::as_sequence)
        .into_iter()
        .flatten()
        .filter_map(|model| {
            let id = text(get(model, "id")).trim().to_string();
            if id.is_empty() {
                return None;
            }
            let name = get(model, "name")
                .and_then(Value::as_str)
                .map(str::to_string);
            Some(OmpModelConfig { id, name })
        })
        .collect();
    OmpProviderConfig {
        id: id.to_string(),
        base_url: text(get(value, "baseUrl")),
        api_key: text(get(value, "apiKey")),
        api: text(get(value, "api")),
        user_agent: text(get(value, "headers").and_then(|v| get(v, "User-Agent"))),
        discovery_type: text(get(value, "discovery").and_then(|v| get(v, "type"))),
        models,
        enabled: true,
    }
}

pub fn list_providers() -> Result<Vec<OmpProviderConfig>, String> {
    let dir = crate::provider::omp::get_agent_dir()
        .ok_or_else(|| "无法解析 OMP agent 配置目录".to_string())?;
    list_providers_at(&dir)
}

fn list_providers_at(agent_dir: &Path) -> Result<Vec<OmpProviderConfig>, String> {
    let private_path = private_config_path(agent_dir)?;
    list_providers_with_disabled_at(agent_dir, &private_path)
}

fn list_providers_with_disabled_at(
    agent_dir: &Path,
    private_path: &Path,
) -> Result<Vec<OmpProviderConfig>, String> {
    let _guard = config_write_lock().lock();
    let root = read_root(&models_config_path(agent_dir))?;
    let enabled = match get(&root, "providers") {
        Some(value) => value
            .as_mapping()
            .ok_or_else(|| "OMP providers 配置必须是对象".to_string())?,
        None => return list_disabled_only(private_path),
    };
    let mut result: Vec<_> = enabled
        .iter()
        .map(|(name, value)| provider_from_value(name, value))
        .collect();
    for (name, value) in read_disabled(private_path)? {
        let id = name.as_str();
        if !enabled.contains_key(&name) {
            let mut config = provider_from_value(id, &value);
            config.enabled = false;
            result.push(config);
        }
    }
    result.sort_by(|a, b| a.id.to_lowercase().cmp(&b.id.to_lowercase()));
    Ok(result)
}

fn list_disabled_only(private_path: &Path) -> Result<Vec<OmpProviderConfig>, String> {
    let mut result: Vec<_> = read_disabled(private_path)?
        .iter()
        .map(|(name, value)| {
            let mut config = provider_from_value(name, value);
            config.enabled = false;
            config
        })
        .collect();
    result.sort_by(|a, b| a.id.to_lowercase().cmp(&b.id.to_lowercase()));
    Ok(result)
}

fn valid_id(id: &str) -> bool {
    let id = id.trim();
    !id.is_empty()
        && id.len() <= 128
        && !id.chars().any(char::is_control)
        && !id.contains('/')
        && !id.contains('\\')
}

fn provider_value(config: &OmpProviderConfig, old: Option<&Value>) -> Result<Value, String> {
    if !valid_id(&config.id) {
        return Err("供应商 ID 无效".to_string());
    }
    if !API_FORMATS.contains(&config.api.as_str()) {
        return Err("不支持的 OMP API 格式".to_string());
    }
    if !DISCOVERY_TYPES.contains(&config.discovery_type.as_str()) {
        return Err("不支持的 OMP discovery 类型".to_string());
    }
    if config.base_url.trim().is_empty() {
        return Err("API 地址不能为空".to_string());
    }
    if config.api_key.trim().is_empty() {
        return Err("API Key 不能为空".to_string());
    }
    let mut value = old.and_then(Value::as_mapping).cloned().unwrap_or_default();
    value.insert(
        key("baseUrl"),
        Value::String(config.base_url.trim().to_string()),
    );
    value.insert(key("apiKey"), Value::String(config.api_key.clone()));
    value.insert(key("api"), Value::String(config.api.clone()));
    let mut headers = get(old.unwrap_or(&Value::Null), "headers")
        .and_then(Value::as_mapping)
        .cloned()
        .unwrap_or_default();
    if config.user_agent.trim().is_empty() {
        headers.remove(&key("User-Agent"));
    } else {
        headers.insert(
            key("User-Agent"),
            Value::String(config.user_agent.trim().to_string()),
        );
    }
    if headers.is_empty() {
        value.remove("headers");
    } else {
        value.insert(key("headers"), Value::Mapping(headers));
    }
    let mut discovery = get(old.unwrap_or(&Value::Null), "discovery")
        .and_then(Value::as_mapping)
        .cloned()
        .unwrap_or_default();
    discovery.insert(key("type"), Value::String(config.discovery_type.clone()));
    value.insert(key("discovery"), Value::Mapping(discovery));
    let mut ids = std::collections::HashSet::new();
    let old_models = get(old.unwrap_or(&Value::Null), "models").and_then(Value::as_sequence);
    let models = config
        .models
        .iter()
        .filter_map(|model| {
            let id = model.id.trim();
            if id.is_empty() || !ids.insert(id.to_string()) {
                return None;
            }
            let mut item = old_models
                .into_iter()
                .flatten()
                .find(|old_model| get(old_model, "id").and_then(Value::as_str) == Some(id))
                .and_then(Value::as_mapping)
                .cloned()
                .unwrap_or_default();
            item.insert(key("id"), Value::String(id.to_string()));
            if let Some(name) = model
                .name
                .as_deref()
                .map(str::trim)
                .filter(|s| !s.is_empty())
            {
                item.insert(key("name"), Value::String(name.to_string()));
            } else {
                item.remove(&key("name"));
            }
            Some(Value::Mapping(item))
        })
        .collect::<Vec<_>>();
    value.insert(key("models"), Value::Sequence(models));
    Ok(Value::Mapping(value))
}

fn write_root(agent_dir: &Path, path: &Path, root: &Value) -> Result<(), String> {
    fs::create_dir_all(agent_dir).map_err(|_| "创建 OMP 配置目录失败".to_string())?;
    if path.exists() {
        let stamp = format!(
            "{}-{}",
            chrono::Utc::now().format("%Y%m%dT%H%M%S%.3fZ"),
            uuid::Uuid::new_v4()
        );
        #[cfg(test)]
        let backup_dir = agent_dir.join(".backups").join(&stamp);
        #[cfg(not(test))]
        let backup_dir = dirs::config_dir()
            .ok_or_else(|| "无法解析应用配置目录".to_string())?
            .join("ai-session-viewer")
            .join("config-backups")
            .join(&stamp);
        fs::create_dir_all(&backup_dir).map_err(|_| "创建配置备份目录失败".to_string())?;
        fs::copy(path, backup_dir.join(path.file_name().unwrap_or_default()))
            .map_err(|_| "备份 OMP 配置失败".to_string())?;
    }
    let content = serde_yml::to_string(root).map_err(|_| "生成 OMP YAML 配置失败".to_string())?;
    let temp = path.with_file_name(format!(".models-{}.tmp", uuid::Uuid::new_v4()));
    if fs::write(&temp, content).is_err() {
        let _ = fs::remove_file(&temp);
        return Err("写入 OMP 临时配置失败".to_string());
    }
    #[cfg(windows)]
    if path.exists() {
        let previous = path.with_file_name(format!(".models-{}.previous", uuid::Uuid::new_v4()));
        if let Err(error) = fs::rename(path, &previous) {
            let _ = fs::remove_file(&temp);
            return Err(format!("暂存原 OMP 配置失败: {error}"));
        }
        if let Err(error) = fs::rename(&temp, path) {
            let _ = fs::rename(&previous, path);
            let _ = fs::remove_file(&temp);
            return Err(format!("替换 OMP 配置失败: {error}"));
        }
        let _ = fs::remove_file(previous);
        return Ok(());
    }
    if let Err(error) = fs::rename(&temp, path) {
        let _ = fs::remove_file(&temp);
        return Err(format!("替换 OMP 配置失败: {error}"));
    }
    Ok(())
}

pub fn save_provider(config: OmpProviderConfig, original_id: Option<String>) -> Result<(), String> {
    let agent_dir = crate::provider::omp::get_agent_dir()
        .ok_or_else(|| "无法解析 OMP agent 配置目录".to_string())?;
    save_provider_at(&agent_dir, config, original_id.as_deref())
}

fn save_provider_at(
    agent_dir: &Path,
    config: OmpProviderConfig,
    original_id: Option<&str>,
) -> Result<(), String> {
    let private_path = private_config_path(agent_dir)?;
    save_provider_with_disabled_at(agent_dir, &private_path, config, original_id)
}

fn save_provider_with_disabled_at(
    agent_dir: &Path,
    private_path: &Path,
    mut config: OmpProviderConfig,
    original_id: Option<&str>,
) -> Result<(), String> {
    config.id = config.id.trim().to_string();
    let _guard = config_write_lock().lock();
    let path = models_config_path(agent_dir);
    let mut root = read_root(&path)?;
    let mut disabled = read_disabled(private_path)?;
    let providers = root
        .as_mapping_mut()
        .ok_or_else(|| "OMP 模型配置根节点必须是对象".to_string())?
        .entry(key("providers"))
        .or_insert_with(|| Value::Mapping(Mapping::new()))
        .as_mapping_mut()
        .ok_or_else(|| "OMP providers 配置必须是对象".to_string())?;
    let source = original_id.unwrap_or(&config.id);
    let old_enabled = providers.get(source);
    let old_disabled = disabled.get(source);
    if original_id.is_some() && old_enabled.is_none() && old_disabled.is_none() {
        return Err("待编辑的 OMP provider 已不存在".to_string());
    }
    if (original_id.is_none() || source != config.id)
        && (providers.contains_key(&config.id) || disabled.contains_key(&config.id))
    {
        return Err("目标 OMP provider ID 已存在".to_string());
    }
    let value = provider_value(&config, old_enabled.or(old_disabled))?;
    if old_disabled.is_some() && old_enabled.is_none() {
        disabled.remove(source);
        disabled.insert(key(&config.id), value);
        write_disabled(private_path, &disabled)
    } else {
        providers.remove(source);
        providers.insert(key(&config.id), value);
        write_root(agent_dir, &path, &root)
    }
}

pub fn disable_provider(id: String) -> Result<(), String> {
    let agent_dir = crate::provider::omp::get_agent_dir()
        .ok_or_else(|| "无法解析 OMP agent 配置目录".to_string())?;
    let private_path = private_config_path(&agent_dir)?;
    disable_provider_at(&agent_dir, &private_path, &id)
}

fn disable_provider_at(agent_dir: &Path, private_path: &Path, id: &str) -> Result<(), String> {
    let _guard = config_write_lock().lock();
    let path = models_config_path(agent_dir);
    let mut root = read_root(&path)?;
    let providers = root
        .as_mapping_mut()
        .and_then(|map| map.get_mut("providers"))
        .and_then(Value::as_mapping_mut)
        .ok_or_else(|| "OMP provider 不存在".to_string())?;
    let value = providers
        .get(id)
        .cloned()
        .ok_or_else(|| "OMP provider 不存在".to_string())?;
    let mut disabled = read_disabled(private_path)?;
    disabled.insert(key(id), value);
    write_disabled(private_path, &disabled)?;
    providers.remove(id);
    write_root(agent_dir, &path, &root)
}

pub fn enable_provider(id: String) -> Result<(), String> {
    let agent_dir = crate::provider::omp::get_agent_dir()
        .ok_or_else(|| "无法解析 OMP agent 配置目录".to_string())?;
    let private_path = private_config_path(&agent_dir)?;
    enable_provider_at(&agent_dir, &private_path, &id)
}

fn enable_provider_at(agent_dir: &Path, private_path: &Path, id: &str) -> Result<(), String> {
    let _guard = config_write_lock().lock();
    let path = models_config_path(agent_dir);
    let mut root = read_root(&path)?;
    let mut disabled = read_disabled(private_path)?;
    let value = disabled
        .get(id)
        .cloned()
        .ok_or_else(|| "禁用供应商不存在".to_string())?;
    let providers = root
        .as_mapping_mut()
        .ok_or_else(|| "OMP 模型配置根节点必须是对象".to_string())?
        .entry(key("providers"))
        .or_insert_with(|| Value::Mapping(Mapping::new()))
        .as_mapping_mut()
        .ok_or_else(|| "OMP providers 配置必须是对象".to_string())?;
    if providers.contains_key(id) {
        return Err("OMP provider ID 已存在".to_string());
    }
    providers.insert(key(id), value);
    write_root(agent_dir, &path, &root)?;
    disabled.remove(id);
    write_disabled(private_path, &disabled)
}

pub fn delete_provider(id: String) -> Result<(), String> {
    let agent_dir = crate::provider::omp::get_agent_dir()
        .ok_or_else(|| "无法解析 OMP agent 配置目录".to_string())?;
    let private_path = private_config_path(&agent_dir)?;
    delete_provider_at(&agent_dir, &private_path, &id)
}

fn delete_provider_at(agent_dir: &Path, private_path: &Path, id: &str) -> Result<(), String> {
    let _guard = config_write_lock().lock();
    let path = models_config_path(agent_dir);
    let mut root = read_root(&path)?;
    let mut disabled = read_disabled(private_path)?;
    let enabled = root
        .as_mapping_mut()
        .and_then(|map| map.get_mut("providers"))
        .and_then(Value::as_mapping_mut)
        .and_then(|providers| providers.remove(id));
    if enabled.is_none() && !disabled.contains_key(id) {
        return Err("OMP provider 不存在".to_string());
    }
    if enabled.is_some() {
        write_root(agent_dir, &path, &root)?;
    }
    if disabled.remove(id).is_some() {
        write_disabled(private_path, &disabled)?;
    }
    Ok(())
}

fn parse_models_json(bytes: &[u8]) -> Result<Vec<DiscoveredModel>, String> {
    let value: serde_json::Value =
        serde_json::from_slice(bytes).map_err(|_| "OMP 模型刷新返回无效 JSON".to_string())?;
    let entries = value
        .get("models")
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| "OMP 模型刷新结果缺少 models 列表".to_string())?;
    let mut models = entries
        .iter()
        .filter_map(|entry| {
            let id = entry.get("id")?.as_str()?.trim();
            let provider = entry.get("provider")?.as_str()?;
            if id.is_empty() {
                return None;
            }
            let name = entry
                .get("name")
                .and_then(serde_json::Value::as_str)
                .filter(|name| !name.trim().is_empty())
                .unwrap_or(id);
            Some(DiscoveredModel {
                provider: provider.to_string(),
                id: id.to_string(),
                name: name.to_string(),
            })
        })
        .collect::<Vec<_>>();
    models.sort_by(|a, b| a.provider.cmp(&b.provider).then(a.id.cmp(&b.id)));
    Ok(models)
}

pub async fn refresh_models() -> Result<Vec<DiscoveredModel>, String> {
    run_models_command(true).await
}

pub async fn list_available_models() -> Result<Vec<DiscoveredModel>, String> {
    run_models_command(false).await
}

async fn run_models_command(force_refresh: bool) -> Result<Vec<DiscoveredModel>, String> {
    let action = if force_refresh { "refresh" } else { "ls" };
    let cli = crate::cli::find_cli("omp")?;
    let mut command = {
        #[cfg(windows)]
        {
            let path = Path::new(&cli);
            if path
                .extension()
                .is_some_and(|ext| ext.eq_ignore_ascii_case("cmd"))
            {
                let path = path.to_string_lossy();
                if path.chars().any(|c| {
                    c.is_control() || matches!(c, '"' | '&' | '|' | '<' | '>' | '^' | '%' | '!')
                }) {
                    return Err("OMP CLI 路径包含 Windows shell 不安全字符".to_string());
                }
                let line = format!(r#""{}" models {} --json""#, path, action);
                let mut command = Command::new("cmd.exe");
                command.args(["/D", "/S", "/C"]).arg(line);
                command
            } else {
                let mut command = Command::new(cli);
                command.args(["models", action, "--json"]);
                command
            }
        }
        #[cfg(not(windows))]
        {
            let mut command = Command::new(cli);
            command.args(["models", action, "--json"]);
            command
        }
    };
    command
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.as_std_mut().creation_flags(0x08000000);
    }
    let output = timeout(REFRESH_TIMEOUT, command.output())
        .await
        .map_err(|_| "OMP 模型命令超时".to_string())?
        .map_err(|_| "无法启动 OMP 模型命令".to_string())?;
    if !output.status.success() {
        return Err("OMP 模型列表获取失败；请检查 OMP 配置和日志".to_string());
    }
    parse_models_json(&output.stdout)
}

pub fn preview_provider(
    config: &OmpProviderConfig,
    original_id: Option<String>,
) -> Result<String, String> {
    let agent_dir = crate::provider::omp::get_agent_dir()
        .ok_or_else(|| "无法解析 OMP agent 配置目录".to_string())?;
    let path = models_config_path(&agent_dir);
    let root = read_root(&path)?;
    let old = original_id.as_deref().and_then(|id| {
        get(&root, "providers").and_then(|providers| providers.as_mapping()?.get(id))
    });
    let provider = provider_value(config, old)?;
    let mut providers = Mapping::new();
    providers.insert(config.id.trim().to_string(), provider);
    let mut preview = Mapping::new();
    preview.insert("providers".to_string(), Value::Mapping(providers));
    serde_yml::to_string(&Value::Mapping(preview)).map_err(|_| "生成 OMP YAML 预览失败".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir() -> PathBuf {
        let path = std::env::temp_dir().join(format!("omp-model-config-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&path).unwrap();
        path
    }

    fn sample() -> OmpProviderConfig {
        OmpProviderConfig {
            id: "test-provider".into(),
            base_url: "https://example.test/v1".into(),
            api_key: "TEST_KEY".into(),
            enabled: true,
            api: "openai-responses".into(),
            user_agent: "viewer-test".into(),
            discovery_type: "openai-models-list".into(),
            models: vec![OmpModelConfig {
                id: "model-x".into(),
                name: Some("Model X".into()),
            }],
        }
    }

    #[test]
    fn parses_and_orders_models_by_provider_and_id() {
        let output = br#"{"models":[{"provider":"zeta","id":"b","name":"Bee"},{"provider":"alpha","id":"c","name":"See"},{"provider":"alpha","id":"a","name":"Aye"}]}"#;
        let models = parse_models_json(output).unwrap();
        assert_eq!(
            models,
            vec![
                DiscoveredModel {
                    provider: "alpha".into(),
                    id: "a".into(),
                    name: "Aye".into()
                },
                DiscoveredModel {
                    provider: "alpha".into(),
                    id: "c".into(),
                    name: "See".into()
                },
                DiscoveredModel {
                    provider: "zeta".into(),
                    id: "b".into(),
                    name: "Bee".into()
                },
            ]
        );
    }

    #[test]
    fn save_preserves_other_provider_and_unmanaged_provider_and_model_fields() {
        let dir = temp_dir();
        fs::write(dir.join("models.yaml"), "providers:\n  other:\n    api: openai-responses\n    customField: keep-me\n  test-provider:\n    extra: preserve-me\n    models:\n      - id: model-x\n        contextWindow: 64000\n        customModelFlag: retain\n").unwrap();
        save_provider_at(&dir, sample(), Some("test-provider")).unwrap();
        let parsed: Value =
            serde_yml::from_str(&fs::read_to_string(dir.join("models.yaml")).unwrap()).unwrap();
        let providers = get(&parsed, "providers").unwrap();
        let other = providers.as_mapping().unwrap().get("other").unwrap();
        let configured = providers
            .as_mapping()
            .unwrap()
            .get("test-provider")
            .unwrap();
        assert_eq!(
            get(other, "customField").and_then(Value::as_str),
            Some("keep-me")
        );
        assert_eq!(
            get(configured, "extra").and_then(Value::as_str),
            Some("preserve-me")
        );
        let model = get(configured, "models")
            .and_then(Value::as_sequence)
            .unwrap()
            .first()
            .unwrap();
        assert_eq!(
            get(model, "contextWindow").and_then(Value::as_u64),
            Some(64000)
        );
        assert_eq!(
            get(model, "customModelFlag").and_then(Value::as_str),
            Some("retain")
        );
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn malformed_config_is_not_overwritten() {
        let dir = temp_dir();
        let path = dir.join("models.yml");
        fs::write(&path, "providers: [broken").unwrap();
        assert!(save_provider_at(&dir, sample(), None).is_err());
        assert_eq!(fs::read_to_string(&path).unwrap(), "providers: [broken");
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn disable_enable_and_delete_keep_states_separate() {
        let dir = temp_dir();
        let private_path = dir.join("disabled.yml");
        save_provider_with_disabled_at(&dir, &private_path, sample(), None).unwrap();
        assert!(list_providers_with_disabled_at(&dir, &private_path).unwrap()[0].enabled);

        disable_provider_at(&dir, &private_path, "test-provider").unwrap();
        let disabled = list_providers_with_disabled_at(&dir, &private_path).unwrap();
        assert_eq!(disabled.len(), 1);
        assert!(!disabled[0].enabled);
        let models =
            serde_yml::from_str::<Value>(&fs::read_to_string(dir.join("models.yml")).unwrap())
                .unwrap();
        assert!(get(&models, "providers")
            .and_then(Value::as_mapping)
            .unwrap()
            .is_empty());

        enable_provider_at(&dir, &private_path, "test-provider").unwrap();
        assert!(list_providers_with_disabled_at(&dir, &private_path).unwrap()[0].enabled);
        delete_provider_at(&dir, &private_path, "test-provider").unwrap();
        assert!(list_providers_with_disabled_at(&dir, &private_path)
            .unwrap()
            .is_empty());
        fs::remove_dir_all(dir).unwrap();
    }
}
