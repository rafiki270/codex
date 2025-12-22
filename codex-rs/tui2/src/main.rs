use clap::Parser;
use codex_arg0::arg0_dispatch_or_else;
use codex_common::CliConfigOverrides;
use codex_core::auth::CODEX_API_KEY_ENV_VAR;
use codex_tui2::Cli;
use codex_tui2::run_main;

#[derive(Parser, Debug)]
struct TopCli {
    #[clap(flatten)]
    config_overrides: CliConfigOverrides,

    #[clap(flatten)]
    inner: Cli,
}

fn main() -> anyhow::Result<()> {
    arg0_dispatch_or_else(|codex_linux_sandbox_exe| async move {
        let top_cli = TopCli::parse();
        let mut inner = top_cli.inner;
        if let Some(token) = top_cli.config_overrides.api_token.as_deref() {
            let trimmed = token.trim();
            if trimmed.is_empty() {
                return Err(anyhow::anyhow!("--api-token must not be empty"));
            }
            // Safety: apply the override before spawning any Codex tasks.
            unsafe {
                std::env::set_var(CODEX_API_KEY_ENV_VAR, trimmed);
            }
        }
        inner
            .config_overrides
            .raw_overrides
            .splice(0..0, top_cli.config_overrides.raw_overrides);
        if inner.config_overrides.system_prompt_file.is_none() {
            inner.config_overrides.system_prompt_file = top_cli.config_overrides.system_prompt_file;
        }
        if inner.config_overrides.api_token.is_none() {
            inner.config_overrides.api_token = top_cli.config_overrides.api_token;
        }
        let exit_info = run_main(inner, codex_linux_sandbox_exe).await?;
        let token_usage = exit_info.token_usage;
        if !token_usage.is_zero() {
            println!("{}", codex_core::protocol::FinalOutput::from(token_usage),);
        }
        Ok(())
    })
}
