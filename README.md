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

### v0.3 — MLFlow Integration

Pull training metrics from SageMaker's managed MLFlow tracking server instead of parsing CloudWatch logs.

- [ ] Add HTTP client (`reqwest`) for MLFlow REST API (`/api/2.0/mlflow/...`)
- [ ] Connect to SageMaker managed MLFlow endpoint (auto-discover or `--mlflow-url`)
- [ ] Link SageMaker steps to MLFlow runs (via job name tag or run metadata)
- [ ] New metrics panel — show loss, mAP, epoch progress alongside logs
- [ ] Sparkline/chart widgets for metric trends using ratatui built-ins
- [ ] Run comparison — select two executions and diff their metrics/params side by side
- [ ] Artifact browser — list models/files a run produced without digging through S3
- [ ] Poll metrics on the same async interval as step status

### v0.4 — Pipeline Control

Trigger operations directly from the TUI — pipeline control without leaving the terminal.

- [ ] Start a new execution (with parameter overrides)
- [ ] Stop a running execution
- [ ] Retry a failed execution
- [ ] Action picker UI — confirmation dialog before destructive operations
- [ ] Invocation feedback in status bar (invoking / success / error)

### v0.5 — Notifications

Desktop alerts so you don't have to stare at the terminal.

- [ ] Desktop notification when a long-running execution finishes or fails
- [ ] Configurable notification rules (notify on failure only, always, never)
- [ ] macOS native notifications via `osascript` / `notify-send` on Linux

### v1.0 — Standalone Release

Make fully configurable and publishable.

- [ ] Config file (TOML) — pipelines, region, MLFlow URL, notification prefs
- [x] Generic SageMaker pipeline support (auto-discover steps)
- [ ] `cargo install furnace` via crates.io
- [ ] CI/CD with GitHub Actions (build, test, release binaries)
