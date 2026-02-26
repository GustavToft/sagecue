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

use app::{App, AppMode};
use aws::client::AwsClients;
use event::{AppEvent, EventHandler};
use handler::Action;

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

    /// Enable desktop notifications for step/pipeline completion
    #[arg(long)]
    notify: bool,
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
    app.notifications_enabled = cli.notify;
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
                    let step_name = app.enter_monitoring(&arn);
                    arn_tx.send(arn)?;
                    step_tx.send(step_name)?;
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
        terminal.draw(|f| ui::draw(f, &app))?;

        // Drain poll results (non-blocking)
        while let Ok(result) = poll_result_rx.try_recv() {
            app.apply_poll_result(result);
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
                    }
                    Action::StartMonitoring { arn, step_name } => {
                        arn_tx.send(arn)?;
                        step_tx.send(step_name)?;
                        let _ = force_tx.send(());
                    }
                    Action::ForceRefresh => {
                        let _ = force_tx.send(());
                    }
                    Action::StepChanged { step_name } => {
                        let _ = step_tx.send(step_name);
                    }
                    Action::ToggleNotifications => {
                        app.toggle_notifications();
                    }
                    Action::BackToExecutions { pipeline_name } => {
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
                        }
                    }
                }
            }
            AppEvent::Tick => {}
        }

        if app.should_quit {
            break;
        }
    }

    Ok(())
}
