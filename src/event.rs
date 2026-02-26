use anyhow::Result;
use crossterm::event::{self, Event as CrosstermEvent, KeyEvent};
use std::time::Duration;
use tokio::sync::mpsc;

#[derive(Debug)]
pub enum AppEvent {
    Key(KeyEvent),
    Tick,
}

pub struct EventHandler {
    rx: mpsc::UnboundedReceiver<AppEvent>,
}

impl EventHandler {
    pub fn new(tick_rate: Duration) -> Self {
        let (tx, rx) = mpsc::unbounded_channel();

        // Spawn a blocking thread for crossterm events + tick
        let _handle = std::thread::spawn(move || {
            loop {
                // Poll with tick_rate timeout
                if event::poll(tick_rate).unwrap_or(false) {
                    if let Ok(evt) = event::read() {
                        match evt {
                            CrosstermEvent::Key(key) => {
                                if tx.send(AppEvent::Key(key)).is_err() {
                                    return;
                                }
                            }
                            _ => {}
                        }
                    }
                } else {
                    // Timeout = tick
                    if tx.send(AppEvent::Tick).is_err() {
                        return;
                    }
                }
            }
        });

        Self { rx }
    }

    pub async fn next(&mut self) -> Result<AppEvent> {
        self.rx
            .recv()
            .await
            .ok_or_else(|| anyhow::anyhow!("Event channel closed"))
    }
}
