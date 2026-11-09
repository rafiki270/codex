use super::*;
use codex_core::config::ConfigBuilder;
use codex_login::CodexAuth;
use codex_model_provider_info::ModelProviderCapabilities;

#[tokio::test]
async fn provider_opt_in_and_opt_out_refresh_extension_tools() {
    let home = tempfile::tempdir().unwrap();
    let config = ConfigBuilder::default()
        .codex_home(home.path().to_path_buf())
        .fallback_cwd(Some(home.path().to_path_buf()))
        .build()
        .await
        .unwrap();
    let extension = ImageGenerationExtension {
        auth_manager: AuthManager::from_auth_for_testing(CodexAuth::from_api_key("test-key")),
        resolve_save_root: Arc::new(|_| None),
    };
    let session_store = ExtensionData::new("session");
    let thread_store = ExtensionData::new("thread");
    for (override_value, expected_tools) in [(None, 0), (Some(true), 1), (Some(false), 0)] {
        let mut changed = config.clone();
        changed.model_provider = ModelProviderInfo {
            name: "Custom images".to_string(),
            requires_openai_auth: false,
            capabilities: Some(ModelProviderCapabilities {
                image_generation: override_value,
                ..Default::default()
            }),
            ..Default::default()
        };
        extension.on_config_changed(&session_store, &thread_store, &config, &changed);
        assert_eq!(
            extension.tools(&session_store, &thread_store).len(),
            expected_tools
        );
    }
}
