# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

SageCue is a Rust TUI application for real-time monitoring of AWS SageMaker pipeline executions. It provides live step tracking, CloudWatch log streaming, training-metrics sparklines, and job detail enrichment in the terminal using ratatui. The crate and binary are named `sagecue` (lowercase).

## Build & Run Commands

```bash
cargo build                    # Debug build
cargo build --release          # Release build
cargo run                      # Run (pipeline/execution selection)
cargo run -- --latest          # Monitor latest execution
cargo run -- --pipeline NAME   # Skip pipeline selection
cargo run -- --region REGION   # Override region (default: eu-west-1)
cargo run -- --notify          # Desktop notifications on step/pipeline completion
cargo check                    # Type-check without building
cargo clippy                   # Lint
cargo test                     # Run unit tests
```

## Architecture

**State machine flow:** `SelectPipeline → SelectExecution → Monitoring` (with back navigation).

**Async design:** Tokio runtime with three concurrent concerns:
- **Event handler** (`event.rs`): Dedicated `std::thread` polling crossterm with a 250ms timeout. Emits `AppEvent::Key` on input and `AppEvent::Tick` on timeout via an mpsc channel. Never touches app state or AWS.
- **Polling task** (`polling.rs`): Tokio task on a 5-second interval. Dispatches by `PollConfig`: monitor a specific execution (steps + logs + conditional training metrics + one-shot parameters), poll an execution list, or idle. Enriches job details through a per-job cache and classifies errors into `PollError::{CredentialsExpired, Other}`.
- **Main loop** (`main.rs`): Redraws each frame, drains poll results into `App`, and translates keys into `handler::Action` side-effects. Frame cadence is bounded by the 250ms event-poll tick, not a fixed fps.

**Channel architecture:**
- `watch<PollConfig>`: Main → Poller. Fields dispatch behaviour (`execution_arn`, `selected_step`, `metrics_tab_active`, `list_pipeline_name`).
- `mpsc<()>` force channel: Main → Poller to trigger an immediate refresh after start/stop/retry.
- `mpsc<AppEvent>`: Event thread → Main (keys + ticks).
- `mpsc<PollResult>`: Poller → Main (monitoring update, execution list, or error).

**Key modules:**
- `app.rs` — `App` struct holding all state; `apply_poll_result` dispatches into typed appliers.
- `handler.rs` — Pure reducer: `(App, KeyEvent) → Action`. No async, no channels, no AWS.
- `polling.rs` — Background poll task, `PollConfig`, `PollResult`, `PollError`, `classify`.
- `event.rs` — Crossterm reader thread.
- `notify.rs` — Desktop notifications and background watchers that keep alerting after you leave the monitor view.
- `aws/` — SDK wrappers: `client.rs` (init), `sagemaker.rs` (pipeline/execution/job APIs), `cloudwatch.rs` (log streaming), `sagemaker_metrics.rs` (training-metrics time series).
- `model/` — Domain types and formatters: `execution.rs`, `step.rs`, `pipeline.rs`, `logs.rs`, `metrics.rs`, `format.rs`. Status enums (`ExecutionStatus`, `StepStatus`, `StepType`) parse from AWS strings via `FromStr` and fall back to `Unknown(String)`; `ExecutionStatus` additionally carries `Stopping`.
- `ui/` — One module per region: `header`, `pipeline_list`, `execution_list`, `steps`, `logs`, `metrics`, `status_bar`, `parameter_editor`.

**Log streaming** keeps per-step state with forward tokens, auto-discovers streams from CloudWatch, and supports auto-scroll with manual override.

## Rules of Engagement

**Layering.** Strict one-way dependencies:
- `model/` → nothing. Pure data, formatters, `FromStr` parsers.
- `aws/` → `model/`. Returns domain types; never imports `ui/` or `app`.
- `ui/` → `app` + `model/`. Must not call AWS directly.
- `handler.rs` → `app` + `model/`. Pure reducer — no `await`, no channels, no AWS (see the `handler::handle_key` docstring).
- `polling.rs` and `main.rs` are the only wiring layer where channels, AWS, and `App` meet.

**Async invariants.** The main loop must not block. AWS calls belong in `aws/*` and are invoked from (a) `spawn_poll_task`, or (b) short one-shot `.await`s in `main.rs` Action handlers between frames. When a blocking call follows state that must be visible first, force a redraw before `.await` (see the `OpenStartExecutionEditor` handler in `main.rs`). The event thread does only `crossterm::event::poll`/`read` and channel sends.

**TUI output.** While the alternate screen is active, never use `println!` / `eprintln!` / `dbg!` — they corrupt the frame. Emit diagnostics via `tracing::{debug,info,warn,error}`; they go to `/tmp/sagecue.log` (with an env filter configured in `main.rs`). Tail with `tail -f /tmp/sagecue.log`. The single `eprintln!` in `main.rs` runs only after `LeaveAlternateScreen` on fatal shutdown — new code must follow that ordering.

**Error handling.** `anyhow::Result` for all fallible ops across `aws/`, `polling`, and `main`. The two `FromStr` impls use `std::convert::Infallible` because they can't fail — unknowns fall through to `Unknown(String)`. That is intentional, not drift. Classify AWS errors with `polling::classify` so credential expiry gets the dedicated banner.

## Definition of Done

A change is done when all four commands pass locally:

```bash
cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings
cargo build
cargo test
```

CI (`.github/workflows/rust.yml`) additionally passes `--locked` to `clippy`, `build`, and `test` to catch `Cargo.lock` drift — expect to update `Cargo.lock` in the same PR when dependencies change.

## Testing

Unit tests only, colocated in `#[cfg(test)] mod tests` inside each source file. No `tests/` integration directory, no `benches/`, no examples. AWS clients are **not** mocked — tests cover pure logic: reducers (`handler.rs`, `app.rs`), formatters (`model/step.rs`, `model/format.rs`), `FromStr` parsers, error classification (`polling.rs`), and notification transition detection (`notify.rs`). When adding AWS-touching code, extract a pure helper and test that rather than introducing a mock layer.

## Conventions

- Error handling via `anyhow::Result` (except `FromStr` → `Infallible`, intentional).
- CLI parsing via `clap` derive macros.
- Status colors: Yellow=Executing, Green=Succeeded, Red=Failed/Stopped.
- Vim-style keys (j/k) alongside arrow keys for navigation.
- Terminal raw mode with alternate screen buffer; cleanup on exit.
