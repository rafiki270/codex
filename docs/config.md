# Configuration

For basic configuration instructions, see [this documentation](https://developers.openai.com/codex/config-basic).

For advanced configuration instructions, see [this documentation](https://developers.openai.com/codex/config-advanced).

For a full configuration reference, see [this documentation](https://developers.openai.com/codex/config-reference).

## Images through a custom provider

A Responses-compatible provider that implements the standalone Images generation
and edit endpoints can opt in to the native `image_gen` tool:

```toml
[model_providers.my_gateway.capabilities]
image_generation = true
```

This uses the provider's existing URL and authentication for `/images/generations`
and `/images/edits`. It does not change credentials or grant access at the server.
The provider must accept the native Images request and response formats. The
`image_generation` feature must also be enabled and the selected model must support
image input. An unrelated cached ChatGPT plan does not restrict this explicitly
configured custom provider.

Set the capability to `false` to disable native images for that provider. Omitting
it preserves the existing provider and authentication requirements. Providers
using ChatGPT authentication retain their account-plan checks.

## Lifecycle hooks

Admins can set top-level `allow_managed_hooks_only = true` in
`requirements.toml` to ignore user, project, and session hook configs while
still allowing managed hooks from requirements and managed config layers. This
setting is only supported in `requirements.toml`; putting it in `config.toml`
does not enable managed-hooks-only mode.
