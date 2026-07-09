# Interactive Shell Guide

## Purpose

Describe the current `muldex` interactive shell as it exists now.

This guide is for operators and testers who want to use the shell directly rather than the lower-level daemon and client commands.

## Start the shell

From the workspace root:

```powershell
cargo run -p muldex-cli --
```

You can also seed the first prompt immediately:

```powershell
cargo run -p muldex-cli -- "summarize current runtime state"
```

If you build the binary first:

```powershell
cargo build -p muldex-cli
target\debug\muldex.exe
target\debug\muldex.exe "summarize current runtime state"
```

## Shell model

Current shell behavior:

- default `muldex` launch enters the interactive shell
- shell state is session-oriented rather than one-shot
- messages, prompt history, and runtime state are persisted per interactive shell session
- `muldex` keeps one active session and can create or resume others from inside the shell

## Session commands

Supported session slash commands:

- `/new`
  - create a new interactive shell session
- `/sessions`
  - list resumable shell sessions
- `/resume`
  - resume the active persisted shell session
- `/resume <id>`
  - resume a specific persisted shell session

## Provider switching

Current provider commands:

- `/provider`
- `/provider show`
- `/provider list`
- `/provider use <name>`
- `/provider test`
- `/provider test <name>`

Current intended split:

- use shell commands to inspect and switch the active provider
- use `/config llm ...` to edit the common `llm-router` path
- use the config file directly for richer non-router provider definitions

Current provider testing behavior:

- `/provider test`
  - tests the current default provider
- `/provider test <name>`
  - tests a named provider
- current test path is a low-dependency TCP connectivity check rather than a full provider-specific API request

## LLM configuration

Current shell-native configuration path focuses on the `llm-router` provider.

Supported commands:

- `/config llm`
- `/config llm show`
- `/config llm test`
- `/config llm host <value>`
- `/config llm port <value>`
- `/config llm api-key <value>`
- `/config llm default-model <value>`

Current shell behavior:

- if `llm-router` config is missing, the shell prints a startup hint
- `/status` shows whether the router is configured and displays masked key state
- `/config llm test` performs a low-dependency connectivity check against the configured router host and port

Advanced note:

- the config file is intended to support manual definition of additional providers beyond `llm-router`
- the shell-native flow is currently focused on the `llm-router` entry first

Example advanced config shape:

```json
{
  "schema_version": "muldex-config-v1",
  "default_provider": "openai-prod",
  "providers": {
    "llm-router": {
      "kind": "openai-compatible",
      "host": "127.0.0.1",
      "port": 3000,
      "api_key": "...",
      "default_model": "gpt-5"
    },
    "openai-prod": {
      "kind": "openai-compatible",
      "base_url": "https://api.openai.com/v1",
      "api_key_env": "OPENAI_API_KEY",
      "default_model": "gpt-5"
    }
  }
}
```

## Runtime-linked slash commands

These commands are not shell-only placeholders anymore.
They update runtime-backed state.

- `/status`
  - show runtime phase, cycle, model, approval mode, compaction state, and recent outcome
- `/model`
  - show active model
- `/model <name>`
  - set active model in runtime continuation state
- `/approval`
  - show active approval mode
- `/approval <mode>`
  - set approval mode
  - accepted modes now include:
    - `manual`
    - `ask`
    - `on-request`
    - `never`
    - `auto`
    - `unless-trusted`
- `/compact`
  - request post-compaction state in the runtime
- `/help`
  - show shell help
- `/exit`
  - leave the shell

## Composer shortcuts

### General editing

- `Left` / `Right`
  - move by character
- `Home` / `End`
  - jump to line boundaries
- `Backspace`
  - delete previous character
- `Ctrl+U`
  - clear the whole input buffer
- `Esc`
  - normally clears the input buffer

### Word-level editing

- `Alt+Left`
  - move to previous word boundary
- `Alt+Right`
  - move to next word boundary
- `Ctrl+W`
  - delete previous word

### Multiline compose

- `Ctrl+J`
  - insert newline
- `Enter`
  - submit the current input

Current multiline rule:

- slash-command parsing only applies to a single-line slash input
- if the input contains multiple lines, it is treated as a prompt rather than a slash command

## Slash picker behavior

When the first line of the composer starts with `/`, the shell opens a slash picker hint area.

Current picker behavior:

- matching commands are shown inline in the shell view
- `Up` / `Down` move the active slash candidate
- `Tab` applies the selected slash candidate
- `Enter` also applies the selected slash candidate if the current first line is still only a prefix
- first `Esc` hides the picker without clearing the typed slash input
- second `Esc` clears the input buffer

## Prompt history and reverse search

### History recall

Outside slash-picker mode:

- `Up`
  - restore earlier submitted prompts
- `Down`
  - move toward newer prompts
- moving back past the newest recalled prompt restores the original draft

History rules:

- empty prompts are not stored
- consecutive duplicate prompts are deduped

### Reverse search

- `Ctrl+R`
  - activate reverse history search using the current draft as query
- press `Ctrl+R` again
  - move to an earlier matching prompt
- type while search is active
  - refine the query
- `Backspace` while search is active
  - widen the query
- `Esc` while search is active
  - restore the original draft and leave search mode

Current search view exposes:

- active query text
- total match count
- current matching history entry

## Rendering modes

### TTY mode

When stdout is a real terminal, `muldex` uses a stable redraw shell view.

The shell shows:

- session header
- recent message log
- slash picker hints when relevant
- reverse search status when active
- prompt line

### Plain mode

When stdin or stdout is not a real terminal, the shell stays line-oriented.
This is intentional so smoke tests and scripted usage remain stable.

You can also force plain mode explicitly:

```powershell
$env:MULDEX_FORCE_PLAIN_SHELL = "1"
cargo run -p muldex-cli --
```

You can force the TTY-style render branch for diagnostics even under piped test usage:

```powershell
$env:MULDEX_FORCE_TTY_RENDER = "1"
cargo run -p muldex-cli --
```

## Known current limits

Current shell is still a compatibility-oriented shell, not a full Codex TUI replacement.

Important current limits:

- slash picker is inline text, not a richer popup or pane widget
- reverse history search is interactive but still minimal compared with mature shell search UIs
- multiline compose exists, but there is no full multi-line editor widget yet
- PTY/ConPTY automation remains partially environment-sensitive on Windows
- compatibility covers a focused subset of Codex shell behavior, not the full upstream slash and shortcut surface

## Validate current shell behavior

From the workspace root:

```powershell
cargo test -p muldex-cli
```

For broader regression coverage:

```powershell
cargo test
```
