# ADR-0014: Operator-Managed LLM Router Configuration

## Status

Accepted

## Context

`muldex` now has a substantial interactive shell surface.

That shell is no longer useful only to the primary developer.
It is intended for controlled broader operator trial, and that means operators must be able to configure the model endpoint without editing source files or guessing internal environment conventions.

The immediate requirement is a TUI or shell-native configuration flow for the current `llm-router` dependency.

Minimum required values are:

- router host or IP
- router port
- API key

At the same time, advanced operators must still be able to define other model providers manually through the config file without forcing the interactive shell to expose every provider-specific field directly.

`llm-router` is also the intended compatibility shim for providers whose OpenAI-compatible behavior is incomplete in practice.

## Decision

`muldex` will support operator-managed `llm-router` configuration from the interactive shell surface.

The configuration flow should not require direct file editing as the primary path.

## Configuration boundary

The minimal persisted config schema should support:

- `schema_version`
- `default_provider`
- `providers.<name>.kind`
- `providers.<name>.host` or `providers.<name>.base_url`
- `providers.<name>.port`
- `providers.<name>.api_key` or `providers.<name>.api_key_env`
- optional `providers.<name>.default_model`

Current operator-facing shell flow may still prioritize the `llm-router` entry, but the persisted schema should not be locked to a single provider forever.

## Storage direction

Configuration should be stored outside the repository in a user-scoped config location.

Intended current locations:

- Windows: `%USERPROFILE%\.muldex\config.json`
- Linux: `~/.muldex/config.json`
- macOS: `~/.muldex/config.json`

## Operator UX direction

The primary path should be inside the interactive shell.

The immediate minimal surface is expected to be a shell-native command flow such as `/config llm`.

The immediate minimal shell path can remain focused on the `llm-router` provider while other providers are added manually through the config file.

## Installer implication

The installer should explain why `llm-router` is recommended.

The intended explanation is:

- `muldex` emits requests in an OpenAI-compatible shape
- many model providers are not fully compatible with that shape in practice
- `llm-router` is the recommended compatibility layer that absorbs provider differences and format translation work

The installer should therefore ask whether the operator wants to install and use `llm-router` as the default path.

That flow should allow the operator to:

- inspect current router configuration state
- set host
- set port
- set API key
- eventually set default model

## Consequences

Positive:

- trial operators can set up the shell without hand-editing files
- endpoint configuration becomes a first-class product concern rather than hidden setup trivia
- release and install flows can reference one explicit config boundary

Negative:

- shell UX and config persistence now need to handle secret material safely
- config validation and masking become part of the operator surface

## Rejected alternatives

### Require manual file editing for all router configuration

Rejected because it is too fragile for broader operator trial.

### Require only environment variables and no persisted config

Rejected because it raises migration friction and makes repeated shell use less practical.
