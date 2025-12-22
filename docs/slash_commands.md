## Slash Commands

### What are slash commands?

Slash commands are special commands you can type that start with `/`.

Note: Slash commands are disabled by default in this build. Set `CODEX_DISABLE_SLASH_COMMANDS=0` to enable them.

---

### Built-in slash commands

Control Codex’s behavior during an interactive session with slash commands.

| Command         | Purpose                                                                    |
| --------------- | -------------------------------------------------------------------------- |
| `/model`        | choose what model and reasoning effort to use                              |
| `/approvals`    | choose what Codex can do without approval                                  |
| `/review`       | review my current changes and find issues                                  |
| `/new`          | start a new chat during a conversation                                     |
| `/resume`       | resume an old chat                                                         |
| `/init`         | create AGENTS.md and a blank general-prompt.md file                         |
| `/compact`      | summarize conversation to prevent hitting the context limit                |
| `/diff`         | show git diff (including untracked files)                                  |
| `/mention`      | mention a file                                                             |
| `/status`       | show current session configuration and token usage                         |
| `/mcp`          | list configured MCP tools                                                  |
| `/experimental` | open the experimental menu to enable features from our beta program        |
| `/skills`       | browse and insert skills (experimental; see [docs/skills.md](./skills.md)) |
| `/logout`       | log out of Codex                                                           |
| `/quit`         | exit Codex                                                                 |
| `/exit`         | exit Codex                                                                 |
| `/feedback`     | send logs to maintainers                                                   |

---
