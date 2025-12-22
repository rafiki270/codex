use chrono::SecondsFormat;
use chrono::Utc;
use codex_core::protocol::TokenUsage;
use codex_protocol::ConversationId;
use reqwest::Client;
use serde::Serialize;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;
use tracing::debug;
use tracing::warn;

const API_BASE_URL_ENV: &str = "AIRFORGE_API_BASE_URL";
const INTERNAL_CODE_ENV: &str = "INTERNAL_SHARED_CODE";
const WORKSPACE_ID_ENV: &str = "AIRFORGE_WORKSPACE_ID";
const USER_ID_ENV: &str = "AIRFORGE_USER_ID";
const SESSION_ID_ENV: &str = "AIRFORGE_SESSION_ID";
const ITEM_TYPE_ENV: &str = "AIRFORGE_USAGE_ITEM_TYPE";
const CREDITS_PER_1K_TOKENS_ENV: &str = "AIRFORGE_LLM_CREDITS_PER_1K_TOKENS";
const CREDITS_PER_1K_INPUT_ENV: &str = "AIRFORGE_LLM_CREDITS_PER_1K_INPUT_TOKENS";
const CREDITS_PER_1K_OUTPUT_ENV: &str = "AIRFORGE_LLM_CREDITS_PER_1K_OUTPUT_TOKENS";
const CREDITS_PER_1K_CACHED_INPUT_ENV: &str = "AIRFORGE_LLM_CREDITS_PER_1K_CACHED_INPUT_TOKENS";
const DEFAULT_ITEM_TYPE: &str = "llm_run";
const USAGE_SOURCE: &str = "llm";

#[derive(Clone)]
pub(crate) struct UsageReporter {
    client: Client,
    config: UsageReportingConfig,
    state: Arc<Mutex<UsageReportingState>>,
}

impl UsageReporter {
    pub(crate) fn from_env() -> Option<Self> {
        let config = match UsageReportingConfig::from_env() {
            Some(config) => config,
            None => {
                debug!(
                    "usage reporting disabled; set {API_BASE_URL_ENV}, {INTERNAL_CODE_ENV}, and {WORKSPACE_ID_ENV}"
                );
                return None;
            }
        };
        Some(Self {
            client: Client::new(),
            config,
            state: Arc::new(Mutex::new(UsageReportingState::default())),
        })
    }

    pub(crate) async fn register_conversation(
        &self,
        conversation_id: ConversationId,
        model: String,
        model_provider: String,
    ) {
        let mut state = self.state.lock().await;
        state.contexts.insert(
            conversation_id,
            UsageContext {
                model,
                model_provider,
            },
        );
    }

    pub(crate) async fn clear_turn(&self, conversation_id: ConversationId, turn_id: &str) {
        let mut state = self.state.lock().await;
        state
            .last_reported
            .remove(&TurnUsageKey::new(conversation_id, turn_id));
    }

    pub(crate) async fn report_usage(
        &self,
        conversation_id: ConversationId,
        turn_id: String,
        usage: &TokenUsage,
    ) {
        if should_skip_usage(usage) {
            return;
        }

        let snapshot = UsageSnapshot::from(usage);
        let key = TurnUsageKey::new(conversation_id, &turn_id);
        let context = {
            let mut state = self.state.lock().await;
            if state.last_reported.get(&key) == Some(&snapshot) {
                return;
            }
            state.last_reported.insert(key, snapshot);
            state.contexts.get(&conversation_id).cloned()
        };

        let payload = UsagePayload::new(&self.config, conversation_id, turn_id, usage, context);
        let client = self.client.clone();
        let url = self.config.usage_url.clone();
        let internal_code = self.config.internal_code.clone();

        tokio::spawn(async move {
            let response = client
                .post(url)
                .header("x-internal-code", internal_code)
                .json(&payload)
                .send()
                .await;

            match response {
                Ok(response) if response.status().is_success() => {}
                Ok(response) => {
                    warn!("usage reporting failed with status {}", response.status());
                }
                Err(err) => {
                    warn!("usage reporting request failed: {err}");
                }
            }
        });
    }
}

#[derive(Clone)]
struct UsageReportingConfig {
    usage_url: String,
    internal_code: String,
    workspace_id: String,
    user_id: Option<String>,
    session_id_override: Option<String>,
    item_type: String,
    input_rate: f64,
    output_rate: f64,
    cached_input_rate: f64,
}

impl UsageReportingConfig {
    fn from_env() -> Option<Self> {
        let api_base_url = read_env_var(API_BASE_URL_ENV)?;
        let internal_code = read_env_var(INTERNAL_CODE_ENV)?;
        let workspace_id = read_env_var(WORKSPACE_ID_ENV)?;

        let usage_url = format!("{}/internal/usage/llm", api_base_url.trim_end_matches('/'));
        let user_id = read_env_var(USER_ID_ENV);
        let session_id_override = read_env_var(SESSION_ID_ENV);
        let item_type =
            read_env_var(ITEM_TYPE_ENV).unwrap_or_else(|| DEFAULT_ITEM_TYPE.to_string());

        let base_rate = parse_rate(CREDITS_PER_1K_TOKENS_ENV).unwrap_or(0.0);
        let input_rate = parse_rate(CREDITS_PER_1K_INPUT_ENV).unwrap_or(base_rate);
        let output_rate = parse_rate(CREDITS_PER_1K_OUTPUT_ENV).unwrap_or(base_rate);
        let cached_input_rate = parse_rate(CREDITS_PER_1K_CACHED_INPUT_ENV).unwrap_or(0.0);

        Some(Self {
            usage_url,
            internal_code,
            workspace_id,
            user_id,
            session_id_override,
            item_type,
            input_rate,
            output_rate,
            cached_input_rate,
        })
    }
}

#[derive(Default)]
struct UsageReportingState {
    last_reported: HashMap<TurnUsageKey, UsageSnapshot>,
    contexts: HashMap<ConversationId, UsageContext>,
}

#[derive(Clone)]
struct UsageContext {
    model: String,
    model_provider: String,
}

#[derive(Hash, Eq, PartialEq)]
struct TurnUsageKey {
    conversation_id: ConversationId,
    turn_id: String,
}

impl TurnUsageKey {
    fn new(conversation_id: ConversationId, turn_id: &str) -> Self {
        Self {
            conversation_id,
            turn_id: turn_id.to_string(),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct UsageSnapshot {
    input_tokens: i64,
    cached_input_tokens: i64,
    output_tokens: i64,
    reasoning_output_tokens: i64,
    total_tokens: i64,
}

impl From<&TokenUsage> for UsageSnapshot {
    fn from(usage: &TokenUsage) -> Self {
        Self {
            input_tokens: usage.input_tokens,
            cached_input_tokens: usage.cached_input_tokens,
            output_tokens: usage.output_tokens,
            reasoning_output_tokens: usage.reasoning_output_tokens,
            total_tokens: usage.total_tokens,
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct UsagePayload {
    workspace_id: String,
    session_id: String,
    user_id: Option<String>,
    source: String,
    ai_brand: Option<String>,
    model: Option<String>,
    credits_used: f64,
    tokens_used: i64,
    cost_usd: Option<f64>,
    item_type: String,
    item_id: Option<String>,
    info: Option<serde_json::Value>,
    occurred_at: String,
}

impl UsagePayload {
    fn new(
        config: &UsageReportingConfig,
        conversation_id: ConversationId,
        turn_id: String,
        usage: &TokenUsage,
        context: Option<UsageContext>,
    ) -> Self {
        let tokens_used = usage.total_tokens.max(0);
        let credits_used = calculate_credits(usage, config);
        let (ai_brand, model) = context
            .map(|context| (Some(context.model_provider), Some(context.model)))
            .unwrap_or((None, None));

        let info = build_info(usage, config);

        Self {
            workspace_id: config.workspace_id.clone(),
            session_id: config
                .session_id_override
                .clone()
                .unwrap_or_else(|| conversation_id.to_string()),
            user_id: config.user_id.clone(),
            source: USAGE_SOURCE.to_string(),
            ai_brand,
            model,
            credits_used,
            tokens_used,
            cost_usd: None,
            item_type: config.item_type.clone(),
            item_id: Some(turn_id),
            info: Some(info),
            occurred_at: Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true),
        }
    }
}

fn calculate_credits(usage: &TokenUsage, config: &UsageReportingConfig) -> f64 {
    let non_cached_input = usage.non_cached_input().max(0) as f64;
    let cached_input = usage.cached_input().max(0) as f64;
    let output_tokens = usage.output_tokens.max(0) as f64;

    let input_cost = non_cached_input * config.input_rate / 1000.0;
    let cached_cost = cached_input * config.cached_input_rate / 1000.0;
    let output_cost = output_tokens * config.output_rate / 1000.0;

    (input_cost + cached_cost + output_cost).max(0.0)
}

fn build_info(usage: &TokenUsage, config: &UsageReportingConfig) -> serde_json::Value {
    let input_tokens = usage.input_tokens.max(0);
    let cached_input_tokens = usage.cached_input().max(0);
    let non_cached_input_tokens = usage.non_cached_input().max(0);
    let output_tokens = usage.output_tokens.max(0);
    let reasoning_output_tokens = usage.reasoning_output_tokens.max(0);
    let total_tokens = usage.total_tokens.max(0);
    let billable_tokens = non_cached_input_tokens + output_tokens;

    serde_json::json!({
        "inputTokens": input_tokens,
        "cachedInputTokens": cached_input_tokens,
        "nonCachedInputTokens": non_cached_input_tokens,
        "outputTokens": output_tokens,
        "reasoningOutputTokens": reasoning_output_tokens,
        "totalTokens": total_tokens,
        "billableTokens": billable_tokens,
        "creditsPer1kInputTokens": config.input_rate,
        "creditsPer1kOutputTokens": config.output_rate,
        "creditsPer1kCachedInputTokens": config.cached_input_rate,
    })
}

fn should_skip_usage(usage: &TokenUsage) -> bool {
    if usage.input_tokens <= 0
        && usage.cached_input_tokens <= 0
        && usage.output_tokens <= 0
        && usage.reasoning_output_tokens <= 0
        && usage.total_tokens <= 0
    {
        return true;
    }

    if usage.total_tokens > 0
        && usage.input_tokens == 0
        && usage.cached_input_tokens == 0
        && usage.output_tokens == 0
        && usage.reasoning_output_tokens == 0
    {
        return true;
    }

    false
}

fn read_env_var(name: &str) -> Option<String> {
    std::env::var(name)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn parse_rate(name: &str) -> Option<f64> {
    let value = read_env_var(name)?;
    match value.parse::<f64>() {
        Ok(rate) if rate.is_finite() && rate >= 0.0 => Some(rate),
        Ok(rate) => {
            warn!("{name} must be non-negative and finite, got {rate}");
            None
        }
        Err(err) => {
            warn!("{name} must be a number: {err}");
            None
        }
    }
}
