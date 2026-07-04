# Claude Code Comparison

## Purpose

Use Claude Code as a practical benchmark for capability coverage so `muldex` does not accidentally harden around a too-narrow kernel.

This is not a product-clone goal. It is a boundary-audit goal.

## Public capability surfaces worth comparing

From public Claude Code docs, the important capability areas are:

- terminal-first interactive coding
- structured tool use and command execution
- MCP integration
- instructions, skills, and hooks
- sub-agents and background agents
- remote and multi-surface session continuity
- scheduling and recurring task execution
- custom agent construction through an SDK
- cross-surface session mobility

## What matters to `muldex`

The following are the capability areas that most affect kernel architecture.

### 1. Capability registry

Why it matters:

- Claude Code treats MCP, skills, hooks, and sub-agents as normal runtime surfaces.
- `muldex` must not hide these in scattered side channels.

Required `muldex` response:

- capability registry snapshot in core protocol

### 2. Agent and session mobility

Why it matters:

- background agents, remote control, and multi-surface continuity require stable session and task identity.

Required `muldex` response:

- keep session/thread identity explicit in orchestration requests
- keep continuation decisions replayable and inspectable

### 3. Long-running and scheduled execution

Why it matters:

- recurring or background work changes continuation semantics and wakeup rules.

Required `muldex` response:

- execution-mode descriptors
- wakeup policy that is not tied only to foreground interactive use

### 4. Custom agent composition

Why it matters:

- the kernel must not assume only one built-in agent personality or orchestration loop.

Required `muldex` response:

- explicit orchestrator trait
- explicit capability-aware routing

### 5. Multimodal and browser-adjacent workflows

Why it matters:

- future media and external context workflows will need model routing and bounded context packaging.

Required `muldex` response:

- modality-aware model routing
- media and hyperframe context envelopes

## Capability gaps to avoid

`muldex` should avoid baking in assumptions that would later block:

- only one active agent style
- only foreground interactive turns
- only text-plus-image inputs
- only one default model
- only local, non-remote session surfaces
- only implicit MCP/skill availability

## Current `muldex` direction after this comparison

Already aligned:

- Rust-first kernel
- orchestration boundary
- multimodal model selection direction
- audio/video/hyperframe context direction
- MCP/skills/ADP becoming first-class capability layers

Still needs explicit protocol support:

- execution-mode descriptors
- remote/background/session mobility descriptors
- richer capability registry in Rust types
- scheduling-aware wakeup semantics

## Gemini CLI comparison notes

Public Gemini CLI capability surfaces reinforce several boundary requirements already visible from Claude Code:

- multimodal generation and analysis from PDFs, images, and sketches
- explicit model selection
- long context awareness
- non-interactive and streaming JSON modes
- checkpointing and session resume
- MCP integration
- extensions and custom commands
- sandboxing and trusted-folder semantics

What Gemini CLI adds weight to:

### 1. Output mode descriptors

Why it matters:

- headless JSON and stream-JSON are first-class usage patterns

Required `muldex` response:

- execution and output mode awareness should be part of protocol and routing boundaries

### 2. Checkpointing and resume

Why it matters:

- resumable sessions must not be an afterthought if the runtime is expected to survive long or multimodal tasks

Required `muldex` response:

- session identity and replayability remain first-class

### 3. Document-derived context

Why it matters:

- PDFs and rich documents are clearly in scope for modern coding agents

Required `muldex` response:

- document-derived context should be treated as another bounded media/context path, not bolted on later

### 4. Explicit model switching

Why it matters:

- a CLI with multimodal ambitions should not bind itself to a single static default model assumption

Required `muldex` response:

- modality-aware model routing and capability descriptors in the kernel protocol
