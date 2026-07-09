# Installing `muldex-cli`

## Purpose

Describe the current installation skeleton for `muldex-cli` across Windows, Linux, and macOS.

This is a minimal installation layer.
It is not yet a polished installer, package-manager distribution, or signed release flow.

## Current install scripts

### Windows

- `scripts/install-muldex-windows.ps1`
- `scripts/uninstall-muldex-windows.ps1`

Current behavior:

- copies `muldex.exe` into `%USERPROFILE%\.muldex\bin` by default
- adds that directory to the user PATH if missing
- prompts whether to configure `llm-router` as the default compatibility layer
- if accepted, prompts for:
  - host or IP
  - port
  - API key
  - optional default model
- merges or updates the `llm-router` provider entry in the user config file
- runs a minimal TCP connectivity check against the configured `llm-router` endpoint
- uninstall removes the binary and removes the PATH entry

Example:

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\install-muldex-windows.ps1
```

Optional flags:

- `-UseLlmRouter`
  - configure `llm-router` without prompting
- `-SkipLlmRouterPrompt`
  - suppress the prompt and leave provider configuration for later

### Linux

- `scripts/install-muldex-linux.sh`
- `scripts/uninstall-muldex-linux.sh`

Current behavior:

- copies `muldex` into `$HOME/.local/bin` by default
- marks the binary executable
- prompts whether to configure `llm-router` as the default compatibility layer
- if accepted, prompts for host, port, API key, and optional default model
- merges or updates the `llm-router` provider entry in the user config file
- runs a minimal TCP connectivity check against the configured `llm-router` endpoint

Example:

```sh
sh ./scripts/install-muldex-linux.sh
```

### macOS

- `scripts/install-muldex-macos.sh`
- `scripts/uninstall-muldex-macos.sh`

Current behavior:

- copies `muldex` into `$HOME/.local/bin` by default
- marks the binary executable
- prompts whether to configure `llm-router` as the default compatibility layer
- if accepted, prompts for host, port, API key, and optional default model
- merges or updates the `llm-router` provider entry in the user config file
- runs a minimal TCP connectivity check against the configured `llm-router` endpoint

Example:

```sh
sh ./scripts/install-muldex-macos.sh
```

## Current limitations

These install scripts are intentionally minimal.

They do not yet provide:

- signed installers
- MSI packages
- winget / scoop / Homebrew packages
- release archive extraction logic
- config migration or data migration
- richer uninstall cleanup

## Relationship to release strategy

See also:

- `docs/release-build-strategy.md`
- `docs/interactive-shell-release-checklist.md`

## Provider configuration note

Installation does not by itself fully configure model providers.

Current installer expectation should be:

- ask whether the operator wants to install and use `llm-router`
- explain that `muldex` speaks an OpenAI-compatible request shape
- explain that many model providers are not fully OpenAI-compatible in practice
- explain that `llm-router` is the recommended compatibility layer for provider-specific translation and normalization

Current intended setup split:

- common operator path: configure `llm-router` from inside the shell with `/config llm ...`
- advanced operator path: manually edit the user config file to define additional providers

Current shell-native config commands include:

- `/config llm show`
- `/config llm test`
- `/config llm host <value>`
- `/config llm port <value>`
- `/config llm api-key <value>`
- `/config llm default-model <value>`

Current provider-switching commands include:

- `/provider show`
- `/provider list`
- `/provider use <name>`
- `/provider test`
- `/provider test <name>`

Those documents define the broader release intent and readiness gates.
