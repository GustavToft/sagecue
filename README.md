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

## Architecture

```
src/
├── main.rs         # CLI args, event loop, terminal setup
├── app.rs          # Application state and mode management
├── event.rs        # Keyboard and tick event handler
├── polling.rs      # Async background polling task
├── aws/
│   ├── client.rs   # AWS SDK client initialization
│   ├── sagemaker.rs    # SageMaker API calls
│   └── cloudwatch.rs   # CloudWatch Logs streaming
├── model/
│   ├── pipeline.rs     # Pipeline summary types
│   ├── execution.rs    # Pipeline execution types
│   ├── step.rs         # Step status types
│   └── logs.rs         # Log stream state
└── ui/
    ├── header.rs       # Execution info header
    ├── pipeline_list.rs    # Pipeline selector table
    ├── execution_list.rs   # Execution selector table
    ├── steps.rs        # Step status table
    ├── logs.rs         # Scrollable log viewer
    └── status_bar.rs   # Contextual keybinding help
```

Async event loop built on **tokio** with watch channels for command dispatch and mpsc for polling results. Terminal I/O handled by **crossterm**.

---

## Roadmap

### v0.2 — MLFlow Integration

Pull training metrics directly from MLFlow instead of parsing them from CloudWatch logs.

- [ ] Add HTTP client (`reqwest`) for MLFlow REST API (`/api/2.0/mlflow/...`)
- [ ] Link SageMaker steps to MLFlow runs (via job name tag or run metadata)
- [ ] New metrics panel — show loss, mAP, epoch progress alongside logs
- [ ] Sparkline/chart widgets for metric trends using ratatui built-ins
- [ ] `--mlflow-url` CLI arg (default `http://localhost:5000`)
- [ ] Poll metrics on the same async interval as step status

### v0.3 — Actions (Lambda Invocations)

Trigger operations directly from the TUI — pipeline control without leaving the terminal.

- [ ] Add `aws-sdk-lambda` for function invocation
- [ ] Action registry — configurable list of Lambda actions with display name, ARN, and payload template
- [ ] Action picker UI — `a` key opens action menu with confirmation dialog
- [ ] Invocation feedback in status bar (invoking / success / error)
- [ ] Built-in actions: start pipeline, stop execution, re-run failed step

### v0.4 — Standalone Release

Make fully configurable and publishable.

- [ ] Config file (TOML) — pipelines, region, MLFlow URL, actions
- [ ] Generic SageMaker pipeline support (auto-discover steps)
- [ ] `cargo install furnace` via crates.io
- [ ] CI/CD with GitHub Actions (build, test, release binaries)
