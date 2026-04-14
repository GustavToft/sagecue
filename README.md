# sagecue

Real-time TUI monitor for ML pipeline executions on AWS SageMaker.

**sagecue** gives you a live cue on your SageMaker pipelines — steps, logs, metrics, and job details streaming straight to your terminal. Built in Rust with [ratatui](https://ratatui.rs).

## Features

- **Pipeline browser** — Browse all SageMaker pipelines in your account
- **Execution browser** — List and select from recent pipeline executions, color-coded by status
- **Live step tracking** — Watch pipeline steps progress in real-time with auto-follow on the active step
- **Log streaming** — Stream CloudWatch logs per step with scrollable history and auto-scroll
- **Metrics panel** — Final metrics and time-series from SageMaker Experiments rendered as sparklines, with per-step selection
- **Pipeline control** — Stop or retry executions directly from the TUI
- **Desktop notifications** — Native macOS/Linux alerts on completion or failure, with a background watcher that keeps notifying after you leave the monitoring view
- **Job enrichment** — See instance types, secondary status, and failure reasons pulled from SageMaker job details
- **Background polling** — Async 5-second refresh cycle

## Quick Start

```bash
# Build
cargo build --release

# Monitor the latest execution
sagecue --latest

# Select from recent executions
sagecue

# Custom pipeline and region
sagecue --pipeline my-pipeline --region us-east-1
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

| Key       | Action                                      |
|-----------|---------------------------------------------|
| `↑` `↓`  | Navigate executions                         |
| `Enter`   | Monitor execution                           |
| `N`       | Start a new execution (parameter overrides) |

### Monitoring

| Key       | Action                                         |
|-----------|------------------------------------------------|
| `↑` `↓`  | Select step                                    |
| `Tab`     | Switch between Logs and Metrics tabs           |
| `j` `k`  | Scroll logs / move metrics cursor              |
| `g`       | Jump to top of logs                            |
| `G`       | Jump to end of logs, re-enable follow          |
| `Space`   | Toggle selected metric (Metrics tab)           |
| `a`       | Toggle all metrics (Metrics tab)               |
| `n`       | Toggle desktop notifications                   |
| `S`       | Stop the running execution                     |
| `R`       | Retry the current execution                    |

## Requirements

- Rust 2021 edition
- AWS credentials configured — we recommend [Granted](https://docs.commonfate.io/granted/getting-started) for assuming roles:
  ```bash
  assume <your-profile>
  sagecue
  ```
  Standard credential sources (environment variables, `~/.aws/credentials`, IAM roles) also work.
- Access to SageMaker and CloudWatch Logs APIs

---

## Improvements

- [ ] `sagecue run` — start a pipeline execution from the shell (with optional parameter overrides)
- [ ] `sagecue status` — one-liner showing latest execution status (exit code reflects pass/fail)
- [ ] `sagecue status --watch` — poll until execution completes, with desktop notification on finish/failure
- [x] Metrics panel — show final metrics from `DescribeTrainingJob`
- [x] Time-series from SageMaker Experiments (`batch_get_metrics`) for epoch/step trends
- [x] Sparkline/chart widgets for metric trends using ratatui built-ins
- [x] Per-step metric selection and toggling in a dedicated tab
- [x] Poll metrics on the same async interval as step status
- [ ] Run comparison — select two executions and diff their metrics/params side by side
- [ ] Artifact browser — list models/files a run produced without digging through S3
- [x] Start a new execution (with parameter overrides)
- [ ] Friendlier AWS error messages — raw `ValidationException` / SDK errors are hard to read; extract the human-readable reason and hide the wire-format noise
- [ ] Selective execution — start a run that executes only a chosen subset of steps (`SelectiveExecutionConfig`), reusing prior artifacts for the rest
- [ ] Execution display name / description on start — let users label runs ("debugging X") instead of relying on the UUID
- [ ] Filter executions by status in the browser (`ListPipelineExecutions` status filter) — usable history on pipelines with hundreds of runs
- [ ] Pipeline definition viewer — render the exact JSON that ran for a given execution via `DescribePipelineDefinitionForExecution`, as a new monitoring tab
- [ ] Model package browser — for `RegisterModel` steps, surface `DescribeModelPackage` status, metrics, and S3 artifacts
- [ ] Tagging on pipelines/executions (`AddTags`/`DeleteTags`) for cost allocation and lifecycle workflows
- [x] Stop a running execution
- [x] Retry a failed execution
- [ ] Action picker UI — confirmation dialog before destructive operations
- [x] Invocation feedback in status bar (invoking / success / error)
- [x] Desktop notification when a long-running execution finishes or fails
- [x] Background watcher — keep getting notified after leaving the monitoring view
- [x] Toggleable at runtime and via `--notify` flag
- [x] macOS native notifications via `osascript` / `notify-send` on Linux
- [ ] Configurable notification rules (notify on failure only, always, never)
- [ ] Config file (TOML) — pipelines, region, notification prefs
- [x] Generic SageMaker pipeline support (auto-discover steps)
- [ ] `cargo install sagecue` via crates.io
- [x] CI/CD with GitHub Actions (build, test, release binaries)
