mod app;
mod aws;
mod event;
mod model;
mod polling;
mod ui;

use anyhow::Result;
use clap::Parser;
use crossterm::event::{KeyCode, KeyModifiers};
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use crossterm::ExecutableCommand;
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;
use std::io;
use std::time::Duration;
use tokio::sync::{mpsc, watch};

use app::{App, AppMode};
use aws::client::AwsClients;
use event::{AppEvent, EventHandler};

const REGION: &str = "eu-west-1";

#[derive(Parser)]
#[command(name = "pipeline-monitor")]
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
}

#[tokio::main]
async fn main() -> Result<()> {
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
    let mut events = EventHandler::new(Duration::from_millis(250));

    // Channels for polling task
    let (arn_tx, arn_rx) = watch::channel(String::new());
    let (step_tx, step_rx) = watch::channel(String::new());
    let (poll_result_tx, mut poll_result_rx) = mpsc::unbounded_channel();
    let (force_tx, force_rx) = mpsc::unbounded_channel();

    // Spawn background polling task
    let _poll_handle = polling::spawn_poll_task(
        clients.clone(),
        arn_rx,
        step_rx,
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
                    start_monitoring(&mut app, &arn, &arn_tx, &step_tx)?;
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
        // Draw
        terminal.draw(|f| ui::draw(f, &app))?;

        // Drain poll results (non-blocking)
        while let Ok(result) = poll_result_rx.try_recv() {
            app.execution = Some(result.execution);
            app.update_steps(result.steps);
            app.maybe_follow_executing_step();

            // Update log cache
            if let (Some(step_name), Some(stream_state)) =
                (result.log_step_name, result.log_stream_state)
            {
                app.log_viewer
                    .per_step_cache
                    .insert(step_name, stream_state);
            }
        }

        // Handle events
        match events.next().await? {
            AppEvent::Key(key) => {
                // Ctrl+C always quits
                if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c')
                {
                    break;
                }

                // q always quits
                if key.code == KeyCode::Char('q') {
                    break;
                }

                match app.mode {
                    AppMode::SelectPipeline => match key.code {
                        KeyCode::Esc => break,
                        KeyCode::Up => app.pipeline_cursor_up(),
                        KeyCode::Down => app.pipeline_cursor_down(),
                        KeyCode::Enter => {
                            if let Some(name) = app.selected_pipeline_name() {
                                let name = name.to_string();
                                app.selected_pipeline_name = Some(name.clone());
                                app.mode = AppMode::SelectExecution;
                                app.loading = true;
                                app.execution_cursor = 0;
                                app.error_message = None;

                                match aws::sagemaker::list_executions(
                                    &clients.sagemaker,
                                    &name,
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
                            }
                        }
                        _ => {}
                    },
                    AppMode::SelectExecution => match key.code {
                        KeyCode::Esc => {
                            if cli.pipeline.is_none() {
                                app.mode = AppMode::SelectPipeline;
                                app.error_message = None;
                            } else {
                                break;
                            }
                        }
                        KeyCode::Up => app.execution_cursor_up(),
                        KeyCode::Down => app.execution_cursor_down(),
                        KeyCode::Enter => {
                            if let Some(arn) = app.selected_execution_arn() {
                                let arn = arn.to_string();
                                start_monitoring(&mut app, &arn, &arn_tx, &step_tx)?;
                                let _ = force_tx.send(());
                            }
                        }
                        _ => {}
                    },
                    AppMode::Monitoring => match key.code {
                        KeyCode::Esc => {
                            app.mode = AppMode::SelectExecution;
                            app.execution_cursor = 0;
                            // Refresh execution list
                            if let Some(ref pipeline_name) = app.selected_pipeline_name {
                                if let Ok(execs) = aws::sagemaker::list_executions(
                                    &clients.sagemaker,
                                    pipeline_name,
                                    20,
                                )
                                .await
                                {
                                    app.executions = execs;
                                }
                            }
                        }
                        KeyCode::Up => {
                            app.select_step_up();
                            let _ = step_tx.send(app.selected_step_name().unwrap_or_default().to_string());
                        }
                        KeyCode::Down => {
                            app.select_step_down();
                            let _ = step_tx.send(app.selected_step_name().unwrap_or_default().to_string());
                        }
                        KeyCode::Char('j') => {
                            let name = app.selected_step_name().unwrap_or_default().to_string();
                            app.log_viewer.scroll_down(&name, 3);
                        }
                        KeyCode::Char('k') => {
                            app.log_viewer.scroll_up(3);
                        }
                        KeyCode::Char('G') => {
                            let name = app.selected_step_name().unwrap_or_default().to_string();
                            app.log_viewer.jump_to_end(&name);
                        }
                        KeyCode::Char('g') => {
                            app.log_viewer.jump_to_start();
                        }
                        KeyCode::Char('r') => {
                            let _ = force_tx.send(());
                        }
                        _ => {}
                    },
                }
            }
            AppEvent::Tick => {
                // Just triggers a redraw (for updated timers etc.)
            }
        }

        if app.should_quit {
            break;
        }
    }

    Ok(())
}

fn start_monitoring(
    app: &mut App,
    arn: &str,
    arn_tx: &watch::Sender<String>,
    step_tx: &watch::Sender<String>,
) -> Result<()> {
    app.mode = AppMode::Monitoring;
    app.auto_follow = true;
    app.selected_step = 0;
    app.log_viewer = model::logs::LogViewerState::new();

    // Tell poll task which execution and step to monitor
    arn_tx.send(arn.to_string())?;
    step_tx.send(app.selected_step_name().unwrap_or_default().to_string())?;

    Ok(())
}
