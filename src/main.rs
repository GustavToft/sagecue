mod app;
mod aws;
mod event;
mod handler;
mod model;
mod notify;
mod polling;
mod ui;

use anyhow::Result;
use clap::Parser;
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use crossterm::ExecutableCommand;
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;
use std::io;
use std::time::Duration;
use tokio::sync::{mpsc, watch};

use app::{App, AppMode, MonitorTab};
use aws::client::AwsClients;
use event::{AppEvent, EventHandler};
use handler::Action;
use model::execution::ExecutionStatus;

const REGION: &str = "eu-west-1";

#[derive(Parser)]
#[command(name = "sagecue")]
#[command(about = "TUI monitor for SageMaker pipeline executions")]
struct Cli {
    /// Skip execution selection, monitor the latest execution
    #[arg(long)]
    latest: bool,

    /// AWS region
    #[arg(long, default_value = REGION)]
    region: String,

    /// Pipeline name (skip pipeline selection screen)
    #[arg(long)]
    pipeline: Option<String>,

    /// Enable desktop notifications for step/pipeline completion
    #[arg(long)]
    notify: bool,
}

#[tokio::main]
async fn main() -> Result<()> {
    // Log to file since stdout is the TUI
    let log_file = std::fs::File::create("/tmp/sagecue.log").unwrap();
    tracing_subscriber::fmt()
        .with_writer(log_file)
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive("sagecue=debug".parse().unwrap()),
        )
        .with_ansi(false)
        .init();

    let cli = Cli::parse();

    // Init AWS clients
    let clients = AwsClients::from_env(&cli.region).await?;

    // Init terminal
    enable_raw_mode()?;
    io::stdout().execute(EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(io::stdout());
    let mut terminal = Terminal::new(backend)?;

    let result = run_app(&mut terminal, clients, &cli).await;

    // Restore terminal
    disable_raw_mode()?;
    io::stdout().execute(LeaveAlternateScreen)?;

    if let Err(ref e) = result {
        eprintln!("Error: {:#}", e);
    }

    result
}

async fn run_app(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    clients: AwsClients,
    cli: &Cli,
) -> Result<()> {
    let mut app = App::new();
    app.notifications_enabled = cli.notify;
    let mut events = EventHandler::new(Duration::from_millis(250));

    // Track the current monitoring ARN for spawning background watchers
    let mut current_monitoring_arn: Option<String> = None;

    // Background notification watchers
    struct BackgroundWatcher {
        arn: String,
        handle: tokio::task::JoinHandle<()>,
    }
    let mut watchers: Vec<BackgroundWatcher> = Vec::new();

    // Channels for polling task
    let (config_tx, config_rx) = watch::channel(polling::PollConfig::default());
    let (poll_result_tx, mut poll_result_rx) = mpsc::unbounded_channel();
    let (force_tx, force_rx) = mpsc::unbounded_channel();

    // Spawn background polling task
    let _poll_handle = polling::spawn_poll_task(
        clients.clone(),
        config_rx,
        poll_result_tx,
        force_rx,
    );

    // Initial load depends on CLI args
    if let Some(ref pipeline_name) = cli.pipeline {
        // --pipeline provided: skip pipeline selection, go straight to executions
        app.selected_pipeline_name = Some(pipeline_name.clone());
        app.mode = AppMode::SelectExecution;

        match aws::sagemaker::list_executions(&clients.sagemaker, pipeline_name, 20).await {
            Ok(execs) => {
                app.executions = execs;
                app.loading = false;

                // --latest: auto-select first execution
                if cli.latest && !app.executions.is_empty() {
                    let arn = app.executions[0].arn.clone();
                    let step_name = app.enter_monitoring(&arn);
                    current_monitoring_arn = Some(arn.clone());
                    config_tx.send_modify(|c| {
                        c.execution_arn = arn;
                        c.selected_step = step_name;
                        c.metrics_tab_active = false;
                        c.list_pipeline_name = String::new();
                    });
                } else {
                    // Stay on SelectExecution: poll the list in the background.
                    config_tx.send_modify(|c| {
                        c.execution_arn = String::new();
                        c.list_pipeline_name = pipeline_name.clone();
                    });
                }
            }
            Err(e) => {
                app.error_message = Some(format!("{:#}", e));
                app.loading = false;
            }
        }
    } else {
        // No --pipeline: load pipeline list
        match aws::sagemaker::list_pipelines(&clients.sagemaker).await {
            Ok(pipelines) => {
                app.pipelines = pipelines;
                app.loading = false;
            }
            Err(e) => {
                app.error_message = Some(format!("{:#}", e));
                app.loading = false;
            }
        }
    }

    // Main event loop
    loop {
        terminal.draw(|f| ui::draw(f, &mut app))?;

        // Drain poll results (non-blocking)
        while let Ok(result) = poll_result_rx.try_recv() {
            app.apply_poll_result(result);
        }

        // Sync poller config with current app state
        if app.mode == AppMode::Monitoring {
            let step = app.selected_step_name().unwrap_or_default().to_string();
            let want_metrics = app.active_tab == MonitorTab::Metrics;
            let current = config_tx.borrow();
            if current.selected_step != step || current.metrics_tab_active != want_metrics {
                drop(current);
                config_tx.send_modify(|c| {
                    c.selected_step = step;
                    c.metrics_tab_active = want_metrics;
                });
            }
        }

        match events.next().await? {
            AppEvent::Key(key) => {
                match handler::handle_key(&mut app, key, cli.pipeline.is_some()) {
                    Action::None => {}
                    Action::Quit => break,
                    Action::LoadExecutions { pipeline_name } => {
                        match aws::sagemaker::list_executions(
                            &clients.sagemaker,
                            &pipeline_name,
                            20,
                        )
                        .await
                        {
                            Ok(execs) => {
                                app.executions = execs;
                                app.loading = false;
                            }
                            Err(e) => {
                                app.error_message = Some(format!("{:#}", e));
                                app.loading = false;
                            }
                        }
                        // Turn on background polling for this pipeline's
                        // execution list while the user is on the screen.
                        config_tx.send_modify(|c| {
                            c.execution_arn = String::new();
                            c.list_pipeline_name = pipeline_name.clone();
                        });
                    }
                    Action::StartMonitoring { arn, step_name } => {
                        // Abort any existing background watcher for this ARN
                        watchers.retain(|w| {
                            if w.arn == arn {
                                w.handle.abort();
                                false
                            } else {
                                true
                            }
                        });
                        current_monitoring_arn = Some(arn.clone());
                        app.background_watcher_count = watchers.len();
                        config_tx.send_modify(|c| {
                            c.execution_arn = arn;
                            c.selected_step = step_name;
                            c.metrics_tab_active = false;
                            c.list_pipeline_name = String::new();
                        });
                        let _ = force_tx.send(());
                    }
                    Action::StopPipeline => {
                        if let Some(ref arn) = current_monitoring_arn {
                            let is_executing = app
                                .execution
                                .as_ref()
                                .map(|e| e.status == ExecutionStatus::Executing)
                                .unwrap_or(false);
                            if is_executing {
                                match aws::sagemaker::stop_pipeline_execution(
                                    &clients.sagemaker,
                                    arn,
                                )
                                .await
                                {
                                    Ok(()) => {
                                        let _ = force_tx.send(());
                                    }
                                    Err(e) => {
                                        app.error_message = Some(format!("{:#}", e));
                                    }
                                }
                            }
                        }
                    }
                    Action::RestartPipeline => {
                        let pipeline_name = app
                            .execution
                            .as_ref()
                            .and_then(|e| e.pipeline_arn.as_deref())
                            .and_then(|arn| arn.rsplit('/').next())
                            .map(|s| s.to_string());

                        if let Some(name) = pipeline_name {
                            match aws::sagemaker::start_pipeline_execution(
                                &clients.sagemaker,
                                &name,
                            )
                            .await
                            {
                                Ok(new_arn) => {
                                    let step_name = app.enter_monitoring(&new_arn);
                                    current_monitoring_arn = Some(new_arn.clone());
                                    config_tx.send_modify(|c| {
                                        c.execution_arn = new_arn;
                                        c.selected_step = step_name;
                                        c.metrics_tab_active = false;
                                        c.list_pipeline_name = String::new();
                                    });
                                    let _ = force_tx.send(());
                                }
                                Err(e) => {
                                    app.error_message = Some(format!("{:#}", e));
                                }
                            }
                        }
                    }
                    Action::StepChanged { step_name } => {
                        config_tx.send_modify(|c| {
                            c.selected_step = step_name;
                        });
                    }
                    Action::ToggleNotifications => {
                        app.toggle_notifications();
                    }
                    Action::BackToPipelines => {
                        // Stop polling the execution list and clear any
                        // stale poll error banner.
                        config_tx.send_modify(|c| {
                            c.execution_arn = String::new();
                            c.list_pipeline_name = String::new();
                        });
                        app.last_poll_error = None;
                    }
                    Action::BackToExecutions { pipeline_name } => {
                        // Spawn background watcher if notifications are enabled
                        if app.notifications_enabled {
                            if let Some(ref arn) = current_monitoring_arn {
                                let handle = notify::spawn_background_watcher(
                                    clients.clone(),
                                    arn.clone(),
                                    pipeline_name.clone(),
                                    app.steps.clone(),
                                    app.execution.clone(),
                                );
                                watchers.push(BackgroundWatcher {
                                    arn: arn.clone(),
                                    handle,
                                });
                            }
                        }
                        current_monitoring_arn = None;
                        app.background_watcher_count = watchers.len();

                        if !pipeline_name.is_empty() {
                            if let Ok(execs) = aws::sagemaker::list_executions(
                                &clients.sagemaker,
                                &pipeline_name,
                                20,
                            )
                            .await
                            {
                                app.executions = execs;
                            }
                            config_tx.send_modify(|c| {
                                c.execution_arn = String::new();
                                c.list_pipeline_name = pipeline_name.clone();
                            });
                        }
                    }
                }
            }
            AppEvent::Tick => {
                // Clean up finished background watchers
                watchers.retain(|w| !w.handle.is_finished());
                app.background_watcher_count = watchers.len();
            }
        }

        if app.should_quit {
            break;
        }
    }

    Ok(())
}
