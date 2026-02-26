# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

Furnace is a Rust TUI application for real-time monitoring of AWS SageMaker pipeline executions. It provides live step tracking, CloudWatch log streaming, and job detail enrichment in the terminal using ratatui.

## Build & Run Commands

```bash
cargo build                    # Debug build
cargo build --release          # Release build
cargo run                      # Run (pipeline/execution selection)
cargo run -- --latest          # Monitor latest execution
cargo run -- --pipeline NAME   # Skip pipeline selection
cargo run -- --region REGION   # Override region (default: eu-west-1)
cargo check                    # Type-check without building
cargo clippy                   # Lint
cargo test                     # Run tests (none currently)
```

## Architecture

**State machine flow:** `SelectPipeline → SelectExecution → Monitoring` (with back navigation)

**Async design:** Tokio runtime with three concurrent concerns:
- **Event handler** (`event.rs`): Dedicated thread reading keyboard input, sends events via mpsc channel
- **Polling task** (`polling.rs`): Background loop (5-sec interval) fetching SageMaker steps + CloudWatch logs, receives commands via watch channels, sends results via mpsc
- **Main loop** (`main.rs`): Orchestrates state updates and UI rendering at ~15fps tick rate

**Channel architecture:**
- `watch` channels: Main → Poller (execution ARN, selected step name)
- `mpsc` channels: Event handler → Main (key events), Poller → Main (poll results)

**Key modules:**
- `app.rs` — Centralized `App` struct holding all state (mode, cursors, steps, logs, job details)
- `aws/` — AWS SDK wrappers: `client.rs` (init), `sagemaker.rs` (API calls), `cloudwatch.rs` (log streaming)
- `model/` — Domain types with status enums parsed from AWS strings (`ExecutionStatus`, `StepStatus`)
- `ui/` — Ratatui rendering components, one per screen region (header, tables, logs, status bar)

**Hardcoded pipeline steps** in `app.rs` (`PIPELINE_STEPS` constant) define the expected step order for display.

**Log streaming** maintains per-step state with forward tokens for pagination, auto-discovers log streams from CloudWatch, and supports auto-scroll with manual override.

## Conventions

- Error handling via `anyhow::Result`
- CLI parsing via `clap` derive macros
- Status colors: Yellow=Executing, Green=Succeeded, Red=Failed/Stopped
- Vim-style keys (j/k) alongside arrow keys for navigation
- Terminal raw mode with alternate screen buffer; cleanup on exit
