use color_eyre::Result;
use crossterm::event::{self, Event, KeyCode, KeyEventKind};
use std::{
    sync::mpsc::{self, Receiver, Sender, TryRecvError},
    thread::{self, JoinHandle},
};

/// Terminal events.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TermEvent {
    /// Key press event.
    KeyPress(KeyCode),
    /// Terminal resize event.
    Resize,
}

/// Terminal events handler.
#[derive(Debug)]
pub struct EventHandler {
    /// Channel to send events to the main thread.
    _tx: Sender<TermEvent>,
    /// Channel to receive events from the event thread.
    rx: Receiver<TermEvent>,
    /// The event thread.
    _thread: JoinHandle<Result<()>>,
}

impl EventHandler {
    /// Create a new [`EventHandler`].
    pub fn new() -> Self {
        let (tx, rx) = mpsc::channel();
        let _tx: Sender<TermEvent> = tx.clone();
        let _thread = thread::spawn(move || loop {
            match event::read()? {
                Event::Key(e) => {
                    // Send the event only if it is a key press.
                    if e.kind == KeyEventKind::Press {
                        tx.send(TermEvent::KeyPress(e.code))?;
                    }
                }
                Event::Resize(_, _) => {
                    tx.send(TermEvent::Resize)?;
                }
                _ => {}
            }
        });

        Self { _tx, rx, _thread }
    }

    /// Receive an event.
    pub fn recv(&self) -> Result<TermEvent> {
        Ok(self.rx.recv()?)
    }

    /// Try to receive an event.
    ///
    /// If no event is available, return `None`.
    pub fn try_recv(&self) -> Result<Option<TermEvent>> {
        match self.rx.try_recv() {
            Ok(event) => Ok(Some(event)),
            Err(TryRecvError::Empty) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }
}
