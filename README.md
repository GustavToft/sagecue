# furnace

Real-time TUI monitor for ML pipeline executions on AWS SageMaker.

A furnace transforms raw material through intense heat — **furnace** watches your ML pipeline transform raw data into models through compute. Built in Rust with [ratatui](https://ratatui.rs).

## Features

- **Pipeline browser** — Browse all SageMaker pipelines in your account
- **Execution browser** — List and select from recent pipeline executions, color-coded by status
- **Live step tracking** — Watch pipeline steps progress in real-time with auto-follow on the active step
- **Log streaming** — Stream CloudWatch logs per step with scrollable history and auto-scroll
- **Job enrichment** — See instance types, secondary status, and failure reasons pulled from SageMaker job details
- **Background polling** — Async 5-second refresh cycle with manual force-refresh

## Quick Start

```bash
# Build
cargo build --release

# Monitor the latest execution
furnace --latest

# Select from recent executions
furnace

# Custom pipeline and region
furnace --pipeline my-pipeline --region us-east-1
```

## Keybindings

### Global

| Key       | Action              |
|-----------|---------------------|
| `q`       | Quit                |
| `Esc`     | Back one level      |

### Pipeline Selection

| Key       | Action              |
|-----------|---------------------|
| `↑` `↓`  | Navigate pipelines  |
| `Enter`   | View executions     |

### Execution Selection

| Key       | Action              |
|-----------|---------------------|
| `↑` `↓`  | Navigate executions |
| `Enter`   | Monitor execution   |

### Monitoring

| Key       | Action                        |
|-----------|-------------------------------|
| `↑` `↓`  | Select step                   |
| `j` `k`  | Scroll logs                   |
| `g`       | Jump to top of logs           |
| `G`       | Jump to end, re-enable follow |
| `r`       | Force refresh                 |

## Requirements

- Rust 2021 edition
- AWS credentials configured — we recommend [Granted](https://docs.commonfate.io/granted/getting-started) for assuming roles:
  ```bash
  assume <your-profile>
  furnace
  ```
  Standard credential sources (environment variables, `~/.aws/credentials`, IAM roles) also work.
- Access to SageMaker and CloudWatch Logs APIs

---

## Roadmap

### v0.2 — CLI Commands

Non-TUI commands for scripting, CI, and quick access.

- [ ] `furnace run` — start a pipeline execution from the shell (with optional parameter overrides)
- [ ] `furnace status` — one-liner showing latest execution status (exit code reflects pass/fail)
- [ ] `furnace status --watch` — poll until execution completes, with desktop notification on finish/failure

### v0.3 — Training Metrics

Surface training metrics alongside logs so you don't have to grep CloudWatch.

- [x] Metrics panel — show final metrics from `DescribeTrainingJob`
- [x] Time-series from SageMaker Experiments (`batch_get_metrics`) for epoch/step trends
- [x] Sparkline/chart widgets for metric trends using ratatui built-ins
- [x] Per-step metric selection and toggling in a dedicated tab
- [x] Poll metrics on the same async interval as step status
- [ ] Run comparison — select two executions and diff their metrics/params side by side
- [ ] Artifact browser — list models/files a run produced without digging through S3

> Note: MLFlow integration was considered but dropped — metrics are pulled directly
> from the SageMaker APIs instead.

### v0.4 — Pipeline Control

Trigger operations directly from the TUI — pipeline control without leaving the terminal.

- [ ] Start a new execution (with parameter overrides)
- [x] Stop a running execution
- [x] Retry a failed execution
- [ ] Action picker UI — confirmation dialog before destructive operations
- [x] Invocation feedback in status bar (invoking / success / error)

### v0.5 — Notifications

Desktop alerts so you don't have to stare at the terminal.

- [x] Desktop notification when a long-running execution finishes or fails
- [x] Background watcher — keep getting notified after leaving the monitoring view
- [x] Toggleable at runtime and via `--notify` flag
- [x] macOS native notifications via `osascript` / `notify-send` on Linux
- [ ] Configurable notification rules (notify on failure only, always, never)

### v1.0 — Standalone Release

Make fully configurable and publishable.

- [ ] Config file (TOML) — pipelines, region, notification prefs
- [x] Generic SageMaker pipeline support (auto-discover steps)
- [ ] `cargo install furnace` via crates.io
- [ ] CI/CD with GitHub Actions (build, test, release binaries)
