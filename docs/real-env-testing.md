# Real Environment Testing

## Principle

Mock and protocol tests are necessary, but they are not enough to validate anti-spin behavior.

`muldex-cli` is intended to become part of the real programming environment, so human-runnable tests in real workspaces are required.

## Current real-env entrypoint

Use:

```bash
muldex decide-workspace --workspace <path> --objective "<task>"
```

Optional flags:

- `--objective-file <path>`
- `--mode <build|plan|other>`
- `--no-progress-iterations <n>`
- `--post-compaction`
- `--recoverable-failure`
- `--print-request`

## Example commands

Healthy continuation:

```bash
muldex decide-workspace --workspace D:\workspaces\muldex --objective "Continue implementing the CLI"
```

Simulated post-compaction stall:

```bash
muldex decide-workspace --workspace D:\workspaces\muldex --objective "Continue the task" --post-compaction --no-progress-iterations 2
```

Simulated recoverable failure:

```bash
muldex decide-workspace --workspace D:\workspaces\muldex --objective "Retry the failed step" --recoverable-failure
```

## Acceptance intent

Real-environment testing should verify:

- the CLI can be run against an actual workspace path
- humans can see the request and the resulting decision
- anti-spin behavior is understandable from the output
- future integration with upstream runtime signals can replace simulated flags without changing the CLI contract
