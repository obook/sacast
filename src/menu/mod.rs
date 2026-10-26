//! menu/
//! Terminal menu (ratatui): pick a receiver found on the network, adjust the stream settings,
//! follow the running cast. This file holds the menu state and the key loop; settings.rs,
//! live.rs, draw.rs and panel.rs hold the rest.
//!
//! Author: O. Booklage
//! Date: October 2026
//! Licence: MIT

mod draw;
mod live;
mod panel;
mod settings;
#[cfg(test)]
mod tests;

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::Arc;
use std::time::Duration;

use ratatui::crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
use ratatui::widgets::ListState;
use ratatui::DefaultTerminal;

use crate::device::{discovery, CastDevice};
use live::Live;
pub use settings::Settings;
use settings::{change, AUDIO_DELAY_ROW, ROWS};

enum Found {
    Device(CastDevice),
    Done,
}

/// What the user asked for.
#[derive(Debug)]
pub enum Action {
    /// Cast to this device, stopping the current cast first.
    Cast(CastDevice),
    Stop,
    /// Choose the screen again in the portal dialog.
    PickScreen,
    Quit,
}

/// Progress of the running cast, sent by its thread.
pub enum Status {
    Info(String),
    Error(String),
    Ended,
    // ponytail: sent by no one until the sender is written; it reconnects and drops frames.
    #[allow(dead_code)]
    Reconnecting,
    /// Bytes sent so far, to measure the real bitrate.
    Served(ByteCounter),
    /// Frames dropped by the capture since the start.
    Dropped(Arc<AtomicU64>),
    /// Frames dropped because the network did not take them in time.
    #[allow(dead_code)]
    NetworkDropped(Arc<AtomicU64>),
    /// The screen picker got a new screen: the running cast restarts on it.
    ScreenPicked,
}

pub type ByteCounter = Box<dyn Fn() -> u64 + Send>;

pub struct Menu {
    devices: BTreeMap<String, CastDevice>,
    device: ListState,
    row: ListState,
    on_settings: bool,
    scanning: bool,
    tick: usize,
    tx: Sender<Found>,
    rx: Receiver<Found>,
    status: Status,
    /// Id of the device being cast to.
    casting: Option<String>,
    status_tx: Sender<Status>,
    status_rx: Receiver<Status>,
    live: Live,
}

impl Menu {
    pub fn new() -> Self {
        let mut menu = Self::idle();
        menu.scan();
        menu
    }

    /// A menu that does not search the network yet.
    fn idle() -> Self {
        let (tx, rx) = mpsc::channel();
        let (status_tx, status_rx) = mpsc::channel();
        Self {
            devices: BTreeMap::new(),
            device: ListState::default().with_selected(Some(0)),
            row: ListState::default().with_selected(Some(0)),
            on_settings: false,
            scanning: false,
            tick: 0,
            tx,
            rx,
            status: Status::Info("Choose a receiver, then Enter".into()),
            casting: None,
            status_tx,
            status_rx,
            live: Live::default(),
        }
    }

    /// Channel for a new cast thread to report its progress. Replaces the previous one, so a
    /// stopped cast's late messages (its "ended") are dropped instead of ending the new cast.
    pub fn status_sender(&mut self) -> Sender<Status> {
        (self.status_tx, self.status_rx) = mpsc::channel();
        self.status_tx.clone()
    }

    /// Current channel, for messages that are not from a cast (screen picker).
    pub fn current_sender(&self) -> Sender<Status> {
        self.status_tx.clone()
    }

    pub fn set_status(&mut self, status: Status) {
        match status {
            Status::Reconnecting => self.live.reconnects += 1,
            Status::Served(counter) => self.live.served = Some((counter, None)),
            Status::Dropped(counter) => self.live.dropped = Some(counter),
            Status::NetworkDropped(counter) => self.live.network_dropped = Some(counter),
            // Handled in next(), which returns actions.
            Status::ScreenPicked => {}
            Status::Info(_) => self.status = status,
            Status::Error(_) | Status::Ended => {
                self.casting = None;
                self.live.started = None;
                self.status = status;
            }
        }
    }

    /// Change the selected setting by hand.
    fn change_setting(&mut self, s: &mut Settings, dir: isize) {
        let row = self.row.selected().unwrap_or(0);
        change(s, row, dir);
        // Not live: the offsets froze the image in sccast, and a restart may ask for the
        // receiver's password again.
        if row == AUDIO_DELAY_ROW && self.casting.is_some() {
            self.status = Status::Info(format!("Audio delay {} ms: applies at the next cast", s.audio_delay));
        }
    }

    /// Mark the receiver being cast to; a new cast starts new stats.
    pub fn set_casting(&mut self, id: Option<String>) {
        if id.is_some() {
            self.live = Live::starting();
        }
        self.casting = id;
    }

    /// Redraw before a blocking step (portal dialog, stopping).
    pub fn draw_now(&mut self, terminal: &mut DefaultTerminal, s: &Settings) -> std::io::Result<()> {
        terminal.draw(|f| self.draw(f, s)).map(drop)
    }

    /// Browse in a thread; devices show up as they answer.
    fn scan(&mut self) {
        if self.scanning {
            return;
        }
        self.scanning = true;
        // Devices gone since the last scan (unplugged, new address) must not stay listed.
        self.devices.clear();
        let tx = self.tx.clone();
        std::thread::spawn(move || {
            // Stop early once the menu is gone (send fails).
            let result = discovery::browse(Duration::from_secs(12), |d| tx.send(Found::Device(d)).is_err());
            if let Err(e) = result {
                tracing::warn!("discovery failed: {e:#}");
            }
            let _ = tx.send(Found::Done);
        });
    }

    /// Devices sorted by name, as listed.
    fn list(&self) -> Vec<&CastDevice> {
        let mut list: Vec<_> = self.devices.values().collect();
        list.sort_by_key(|d| d.name.to_lowercase());
        list
    }

    /// Draw and handle keys until the user asks for something. `quit` is set by SIGTERM.
    pub fn next(&mut self, terminal: &mut DefaultTerminal, s: &mut Settings, quit: &AtomicBool) -> anyhow::Result<Action> {
        loop {
            if quit.load(Ordering::SeqCst) {
                return Ok(Action::Quit);
            }
            while let Ok(found) = self.rx.try_recv() {
                match found {
                    // Keyed by id: a device answers several times.
                    Found::Device(d) => drop(self.devices.insert(d.id.clone(), d)),
                    Found::Done => self.scanning = false,
                }
            }
            while let Ok(status) = self.status_rx.try_recv() {
                // A receiver gone from the list since: Enter recasts it by hand.
                if let Status::ScreenPicked = status {
                    if let Some(device) = self.casting.as_ref().and_then(|id| self.devices.get(id)) {
                        return Ok(Action::Cast(device.clone()));
                    }
                    continue;
                }
                self.set_status(status);
            }
            if self.live.started.is_some() {
                self.live.sample();
            }
            terminal.draw(|f| self.draw(f, s))?;
            self.tick += 1;
            // About once a minute: a frozen menu shows in the log as the end of these lines.
            if self.tick.is_multiple_of(600) {
                tracing::info!(casting = self.casting.is_some(), "menu loop alive");
            }
            if !event::poll(Duration::from_millis(100))? {
                continue;
            }
            let Event::Key(key) = event::read()? else { continue };
            if key.kind != KeyEventKind::Press {
                continue;
            }
            tracing::info!(key = ?key.code, "key");
            let count = if self.on_settings { ROWS.len() } else { self.devices.len() };
            let state = if self.on_settings { &mut self.row } else { &mut self.device };
            match key.code {
                // Raw mode turns Ctrl+C into a key, not a signal.
                KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => return Ok(Action::Quit),
                KeyCode::Char('q') | KeyCode::Esc => return Ok(Action::Quit),
                KeyCode::Char('s') if self.casting.is_some() => return Ok(Action::Stop),
                KeyCode::Char('p') => return Ok(Action::PickScreen),
                KeyCode::Up | KeyCode::Char('k') if count > 0 => {
                    state.select(Some(state.selected().unwrap_or(0).checked_sub(1).unwrap_or(count - 1)));
                }
                KeyCode::Down | KeyCode::Char('j') if count > 0 => {
                    state.select(Some((state.selected().unwrap_or(0) + 1) % count));
                }
                // Long device lists (a school network): jump by pages or to the ends.
                KeyCode::PageDown if count > 0 => state.select(Some((state.selected().unwrap_or(0) + 10).min(count - 1))),
                KeyCode::PageUp => state.select(Some(state.selected().unwrap_or(0).saturating_sub(10))),
                KeyCode::Home => state.select(Some(0)),
                KeyCode::End if count > 0 => state.select(Some(count - 1)),
                KeyCode::Tab | KeyCode::BackTab => self.on_settings = !self.on_settings,
                // A change by hand becomes the user's choice.
                KeyCode::Left | KeyCode::Char('h') if self.on_settings => self.change_setting(s, -1),
                KeyCode::Right | KeyCode::Char('l') if self.on_settings => self.change_setting(s, 1),
                KeyCode::Char('r') => self.scan(),
                KeyCode::Enter => {
                    let list = self.list();
                    if let Some(device) = self.device.selected().and_then(|i| list.get(i)) {
                        return Ok(Action::Cast((*device).clone()));
                    }
                }
                _ => {}
            }
        }
    }
}
