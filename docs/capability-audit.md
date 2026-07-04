# Capability Audit

## Purpose

This audit lists model and runtime capabilities that should be represented early enough in `muldex` to avoid later kernel reshaping.

The goal is not to implement every capability now. The goal is to reserve the right boundaries so future support does not require tearing open core orchestration or transport layers.

## Capabilities that should already be first-class

### 1. Input modalities

- text
- image
- audio
- video-derived context
- hyperframes
- document-derived context

### 2. Output modalities

- plain text
- structured JSON
- tool calls
- reasoning summary
- citations and references
- patch or diff style outputs
- generated media outputs when needed

### 3. Reasoning controls

- adjustable reasoning effort
- summary-only reasoning access
- raw reasoning availability when supported
- no-reasoning or low-cost mode

### 4. Structured output controls

- schema-constrained output
- JSON-schema support
- required-key enforcement
- tool-argument structure guarantees

### 5. Tool-use semantics

- native tool calling
- parallel tool calls
- tool choice control
- resumable tool-use sequences

### 6. Execution modes

- streaming
- synchronous interactive turns
- background or long-running execution
- resumable execution
- batch or queued execution

### 7. Context and memory behavior

- long-context support
- prompt caching awareness
- citation support
- native session memory or state carry-over
- explicit post-compaction state and cache-impact tracking
- invoked-skill preservation across compaction or resume

### 8. Capability surfaces beyond the base model

- MCP tools and resources
- skills
- Agent Data Protocol
- retrieval or knowledge connectors
- generative media backends such as ComfyUI and Seedance
- ASR backends
- alignment backends

### 9. Agent-mode and subagent surfaces

- built-in agent modes such as `build` and `plan`
- role-specific permission and edit policies
- general-purpose subagents for search or decomposition
- explicit mode-transition state such as plan-exit or auto-exit guidance

### 10. Surface mobility

- terminal-first operation
- desktop surface
- remote or detached execution surfaces
- consistent session identity across surfaces

## What must be represented in the kernel now

- modality-aware model selection
- capability registry snapshot
- reasoning and structured-output capability descriptors
- execution-mode descriptors
- media and hyperframe context types
- MCP, skill, and ADP capability descriptors
- agent-mode descriptors
- subagent capability descriptors
- surface and session mobility descriptors
- media-generation capability descriptors and generated-artifact lifecycle concepts
- ASR and alignment capability descriptors

## What can stay implementation-specific for now

- exact sidecar wire format details
- provider-specific feature flags
- retrieval engine internals
- codec or transcription backend choices
- keyframe extraction algorithm choice
