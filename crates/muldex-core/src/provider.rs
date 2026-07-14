use std::collections::BTreeMap;

use async_trait::async_trait;
use serde::Deserialize;
use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct ProviderConfig {
    pub kind: String,
    pub host: Option<String>,
    pub port: Option<u16>,
    pub base_url: Option<String>,
    pub api_key: Option<String>,
    pub api_key_env: Option<String>,
    pub default_model: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct LegacyLlmRouterConfig {
    pub host: String,
    pub port: u16,
    pub api_key: String,
    pub default_model: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct MuldexConfig {
    pub schema_version: String,
    #[serde(default)]
    pub default_provider: Option<String>,
    #[serde(default)]
    pub providers: BTreeMap<String, ProviderConfig>,
    #[serde(default)]
    pub llm_router: Option<LegacyLlmRouterConfig>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResolvedProviderConfig {
    pub name: String,
    pub kind: String,
    pub base_url: String,
    pub api_key: Option<String>,
    pub default_model: Option<String>,
}

impl ResolvedProviderConfig {
    pub fn chat_completions_url(&self) -> String {
        let trimmed = self.base_url.trim_end_matches('/');
        if trimmed.ends_with("/chat/completions") {
            trimmed.to_string()
        } else {
            format!("{trimmed}/chat/completions")
        }
    }
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum ProviderResolutionError {
    #[error("no providers configured")]
    NoProvidersConfigured,
    #[error("provider not found: {0}")]
    ProviderNotFound(String),
    #[error("provider kind unsupported: {0}")]
    UnsupportedProviderKind(String),
    #[error("provider endpoint missing for {0}")]
    MissingEndpoint(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProviderMessageRole {
    System,
    User,
    Assistant,
    Tool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProviderToolCall {
    pub id: String,
    pub name: String,
    pub arguments_json: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProviderTurnMessage {
    pub role: ProviderMessageRole,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tool_calls: Vec<ProviderToolCall>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProviderToolSpec {
    pub name: String,
    pub description: String,
    pub input_schema: serde_json::Value,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProviderTurnRequest {
    pub model: String,
    pub messages: Vec<ProviderTurnMessage>,
    #[serde(default)]
    pub stream: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tools: Vec<ProviderToolSpec>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderAssistantTurn {
    pub content: String,
    pub tool_calls: Vec<ProviderToolCall>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderToolCallDelta {
    pub index: usize,
    pub id_fragment: Option<String>,
    pub name_fragment: Option<String>,
    pub arguments_fragment: Option<String>,
}

/// Token accounting reported by the provider for a completed turn.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct ProviderUsage {
    pub input_tokens: u64,
    pub cached_input_tokens: u64,
    pub output_tokens: u64,
    pub total_tokens: u64,
}

/// Rate-limit signals surfaced from provider response headers.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct ProviderRateLimit {
    pub limit_requests: Option<u64>,
    pub remaining_requests: Option<u64>,
    pub limit_tokens: Option<u64>,
    pub remaining_tokens: Option<u64>,
    pub reset_after_seconds: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProviderStreamEvent {
    AssistantDelta(String),
    ToolCallDelta(ProviderToolCallDelta),
    MessageComplete,
    Usage(ProviderUsage),
    RateLimit(ProviderRateLimit),
}

pub trait ProviderEventSink {
    fn push(&mut self, event: ProviderStreamEvent);
}

#[derive(Debug, thiserror::Error)]
pub enum ProviderError {
    #[error("provider http error: {0}")]
    Http(String),
    #[error("provider protocol error: {0}")]
    Protocol(String),
}

#[async_trait(?Send)]
pub trait InteractiveProvider: Send + Sync {
    async fn run_turn(
        &self,
        provider: &ResolvedProviderConfig,
        request: ProviderTurnRequest,
        sink: &mut dyn ProviderEventSink,
    ) -> Result<ProviderAssistantTurn, ProviderError>;
}

pub fn active_provider_name(config: &MuldexConfig) -> Option<String> {
    let providers = materialized_provider_map(config);
    if providers.is_empty() {
        return None;
    }

    if let Some(name) = config.default_provider.as_ref()
        && providers.contains_key(name)
    {
        return Some(name.clone());
    }

    if providers.contains_key("llm-router") {
        return Some("llm-router".to_string());
    }

    providers.keys().next().cloned()
}

pub fn resolve_provider_config(
    config: &MuldexConfig,
    requested_name: Option<&str>,
) -> Result<ResolvedProviderConfig, ProviderResolutionError> {
    let providers = materialized_provider_map(config);
    if providers.is_empty() {
        return Err(ProviderResolutionError::NoProvidersConfigured);
    }

    let provider_name = match requested_name {
        Some(name) => name.to_string(),
        None => active_provider_name(config).ok_or(ProviderResolutionError::NoProvidersConfigured)?,
    };

    let provider = providers
        .get(&provider_name)
        .ok_or_else(|| ProviderResolutionError::ProviderNotFound(provider_name.clone()))?;

    if !provider.kind.is_empty() && provider.kind != "openai-compatible" {
        return Err(ProviderResolutionError::UnsupportedProviderKind(
            provider.kind.clone(),
        ));
    }

    let Some(base_url) = resolved_base_url(provider) else {
        return Err(ProviderResolutionError::MissingEndpoint(provider_name));
    };

    let api_key = provider
        .api_key
        .clone()
        .or_else(|| provider.api_key_env.as_deref().and_then(read_env_value));

    Ok(ResolvedProviderConfig {
        name: provider_name,
        kind: if provider.kind.is_empty() {
            "openai-compatible".to_string()
        } else {
            provider.kind.clone()
        },
        base_url,
        api_key,
        default_model: provider.default_model.clone(),
    })
}

fn materialized_provider_map(config: &MuldexConfig) -> BTreeMap<String, ProviderConfig> {
    let mut providers = config.providers.clone();
    if !providers.contains_key("llm-router")
        && let Some(legacy) = config.llm_router.as_ref()
    {
        providers.insert(
            "llm-router".to_string(),
            ProviderConfig {
                kind: "openai-compatible".to_string(),
                host: Some(legacy.host.clone()),
                port: Some(legacy.port),
                base_url: None,
                api_key: Some(legacy.api_key.clone()),
                api_key_env: None,
                default_model: legacy.default_model.clone(),
            },
        );
    }
    providers
}

fn resolved_base_url(provider: &ProviderConfig) -> Option<String> {
    if let Some(base_url) = provider.base_url.as_deref() {
        return Some(base_url.trim_end_matches('/').to_string());
    }

    match (provider.host.as_deref(), provider.port) {
        (Some(host), Some(port)) => Some(format!("http://{host}:{port}/v1")),
        _ => None,
    }
}

fn read_env_value(name: &str) -> Option<String> {
    match std::env::var(name) {
        Ok(value) if !value.trim().is_empty() => Some(value),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn env_lock() -> &'static std::sync::Mutex<()> {
        static LOCK: std::sync::OnceLock<std::sync::Mutex<()>> = std::sync::OnceLock::new();
        LOCK.get_or_init(|| std::sync::Mutex::new(()))
    }

    #[test]
    fn provider_resolution_prefers_configured_default_provider() {
        let config = MuldexConfig {
            schema_version: "muldex-config-v1".to_string(),
            default_provider: Some("openai-prod".to_string()),
            providers: BTreeMap::from([
                (
                    "llm-router".to_string(),
                    ProviderConfig {
                        kind: "openai-compatible".to_string(),
                        host: Some("127.0.0.1".to_string()),
                        port: Some(3000),
                        base_url: None,
                        api_key: Some("router-key".to_string()),
                        api_key_env: None,
                        default_model: Some("gpt-router".to_string()),
                    },
                ),
                (
                    "openai-prod".to_string(),
                    ProviderConfig {
                        kind: "openai-compatible".to_string(),
                        host: None,
                        port: None,
                        base_url: Some("https://api.openai.com/v1".to_string()),
                        api_key: Some("prod-key".to_string()),
                        api_key_env: None,
                        default_model: Some("gpt-5".to_string()),
                    },
                ),
            ]),
            llm_router: None,
        };

        let resolved = resolve_provider_config(&config, None).expect("resolve provider");
        assert_eq!(resolved.name, "openai-prod");
        assert_eq!(resolved.base_url, "https://api.openai.com/v1");
        assert_eq!(resolved.default_model.as_deref(), Some("gpt-5"));
    }

    #[test]
    fn provider_resolution_falls_back_to_legacy_llm_router() {
        let config = MuldexConfig {
            schema_version: "muldex-config-v1".to_string(),
            default_provider: None,
            providers: BTreeMap::new(),
            llm_router: Some(LegacyLlmRouterConfig {
                host: "127.0.0.1".to_string(),
                port: 3000,
                api_key: "legacy-key".to_string(),
                default_model: Some("gpt-5.4".to_string()),
            }),
        };

        let resolved = resolve_provider_config(&config, None).expect("resolve legacy provider");
        assert_eq!(resolved.name, "llm-router");
        assert_eq!(resolved.base_url, "http://127.0.0.1:3000/v1");
        assert_eq!(resolved.api_key.as_deref(), Some("legacy-key"));
    }

    #[test]
    fn provider_resolution_reads_env_api_key_when_requested() {
        let _guard = env_lock().lock().expect("env lock");
        unsafe {
            std::env::set_var("MULDEX_TEST_PROVIDER_KEY", "env-secret");
        }

        let config = MuldexConfig {
            schema_version: "muldex-config-v1".to_string(),
            default_provider: Some("llm-router".to_string()),
            providers: BTreeMap::from([(
                "llm-router".to_string(),
                ProviderConfig {
                    kind: "openai-compatible".to_string(),
                    host: Some("127.0.0.1".to_string()),
                    port: Some(3000),
                    base_url: None,
                    api_key: None,
                    api_key_env: Some("MULDEX_TEST_PROVIDER_KEY".to_string()),
                    default_model: None,
                },
            )]),
            llm_router: None,
        };

        let resolved = resolve_provider_config(&config, None).expect("resolve env key");
        assert_eq!(resolved.api_key.as_deref(), Some("env-secret"));

        unsafe {
            std::env::remove_var("MULDEX_TEST_PROVIDER_KEY");
        }
    }

    #[test]
    fn provider_resolution_errors_when_unconfigured() {
        let error = resolve_provider_config(&MuldexConfig::default(), None)
            .expect_err("unconfigured provider should fail");
        assert_eq!(error, ProviderResolutionError::NoProvidersConfigured);
    }

    #[test]
    fn resolved_provider_builds_chat_completions_url_once() {
        let resolved = ResolvedProviderConfig {
            name: "llm-router".to_string(),
            kind: "openai-compatible".to_string(),
            base_url: "http://127.0.0.1:3000/v1/".to_string(),
            api_key: None,
            default_model: None,
        };

        assert_eq!(
            resolved.chat_completions_url(),
            "http://127.0.0.1:3000/v1/chat/completions"
        );
    }
}
