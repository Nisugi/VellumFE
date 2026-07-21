//! Automation tasks that drive the game connection — command injection +
//! parser-state subscription + cancellation. The travel executor (go2's walk
//! loop) is the first task; future Lich-like features (scripted routines,
//! guild tasks) are meant to slot into the same host rather than each
//! inventing its own frontend plumbing.
//!
//! Flow per frame/network-line: AppCore builds a `TravelContext` snapshot,
//! ticks the active task, surfaces `Status` messages, and queues `Send`
//! commands; frontends drain the queue through their normal typed-command
//! path (so travel moves echo like anything the user types).

pub mod executor;
pub mod mazes;
pub mod target;

use std::collections::VecDeque;
use std::time::Instant;

pub use executor::{TravelContext, TravelEvent, TravelTask};

pub struct TravelService {
    task: Option<TravelTask>,
    /// Commands waiting for a frontend to send to the game.
    outbound: VecDeque<String>,
    /// Monotonic clock base for task timeouts.
    epoch: Instant,
    /// Room id where the last trip started (`.go2 back`).
    pub last_start_room: Option<u32>,
}

impl Default for TravelService {
    fn default() -> Self {
        TravelService {
            task: None,
            outbound: VecDeque::new(),
            epoch: Instant::now(),
            last_start_room: None,
        }
    }
}

impl TravelService {
    pub fn now_ms(&self) -> u64 {
        self.epoch.elapsed().as_millis() as u64
    }

    pub fn is_traveling(&self) -> bool {
        self.task.is_some()
    }

    pub fn task(&self) -> Option<&TravelTask> {
        self.task.as_ref()
    }

    /// Install a planned trip (replacing any active one).
    pub fn set_task(&mut self, task: TravelTask) {
        self.task = Some(task);
    }

    /// Cancel the active trip. Returns whether one was running.
    pub fn stop(&mut self) -> bool {
        self.outbound.clear();
        self.task.take().is_some()
    }

    /// Tick the active task; `Send` events are queued internally, everything
    /// else is returned for the caller to surface. A terminal event
    /// (`Arrived`/`Failed`) retires the task.
    pub fn tick(&mut self, ctx: TravelContext) -> Vec<TravelEvent> {
        let Some(task) = self.task.as_mut() else {
            return Vec::new();
        };
        let events = task.tick(ctx);
        if TravelTask::is_finished(&events) {
            self.task = None;
        }
        let mut surfaced = Vec::new();
        for event in events {
            match event {
                TravelEvent::Send(command) => self.outbound.push_back(command),
                other => surfaced.push(other),
            }
        }
        surfaced
    }

    /// Drain the commands frontends should send to the game.
    pub fn take_outbound(&mut self) -> Vec<String> {
        self.outbound.drain(..).collect()
    }
}

/// "1:04" / "0:07" — go2-style ETA formatting.
pub fn format_eta(seconds: f64) -> String {
    let total = seconds.max(0.0).round() as u64;
    format!("{}:{:02}", total / 60, total % 60)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn eta_formats_like_a_clock() {
        assert_eq!(format_eta(0.0), "0:00");
        assert_eq!(format_eta(7.4), "0:07");
        assert_eq!(format_eta(64.0), "1:04");
        assert_eq!(format_eta(-3.0), "0:00");
    }
}
