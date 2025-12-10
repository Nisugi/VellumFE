//! Text-to-Speech System
//!
//! Provides accessibility support through text-to-speech output.
//! Features:
//! - Cross-platform TTS (Windows SAPI, macOS AVSpeechSynthesizer, Linux Speech Dispatcher)
//! - Priority-based queue (Critical > High > Normal)
//! - Answering machine controls (Next/Previous/Pause/Mute)
//! - Per-window speech configuration
//! - Zero performance cost when disabled

use anyhow::Result;
use std::collections::VecDeque;
use std::sync::mpsc::{channel, Sender, Receiver};
use tts::Tts;

/// Events sent from TTS callbacks to the main event loop
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TtsEvent {
    UtteranceStarted,
    UtteranceEnded,
    UtteranceStopped,
}

/// Priority levels for speech events
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Priority {
    Normal = 0,   // Regular game text
    High = 1,     // Thoughts, whispers, important events
    Critical = 2, // Damage warnings, death, critical alerts
}

/// A single speech entry in the queue
#[derive(Debug, Clone)]
pub struct SpeechEntry {
    pub text: String,
    pub source_window: String,
    pub priority: Priority,
    pub spoken: bool,
}

/// Text-to-Speech manager
///
/// Manages the speech queue and TTS engine.
/// When TTS is disabled (config disabled), this is a no-op.
pub struct TtsManager {
    engine: Option<Tts>,

    /// Speech queue (sorted by priority)
    queue: VecDeque<SpeechEntry>,

    /// Current index in queue (-1 if not playing)
    current_index: Option<usize>,

    /// Current utterance being spoken (for callback correlation)

    /// Is TTS globally muted?
    muted: bool,

    /// Is TTS enabled in config?
    enabled: bool,

    /// Speech rate from config
    rate: f32,

    /// Speech volume from config
    volume: f32,

    /// Maximum queue size (prevent memory bloat)
    max_queue_size: usize,

    /// Event channel for TTS callbacks
    event_tx: Sender<TtsEvent>,
    event_rx: Receiver<TtsEvent>,

    /// Backend min/max ranges for normalization
    backend_min_rate: f32,
    backend_max_rate: f32,
    backend_min_volume: f32,
    backend_max_volume: f32,
}

impl TtsManager {
    /// Create a new TTS manager
    pub fn new(enabled: bool, rate: f32, volume: f32) -> Self {
        let (event_tx, event_rx) = channel();

        Self {
            engine: None,
            queue: VecDeque::new(),
            current_index: None,
            muted: false,
            enabled,
            rate,
            volume,
            max_queue_size: 100, // Reasonable limit
            event_tx,
            event_rx,
            // Default ranges (will be updated during initialization)
            backend_min_rate: 0.1,
            backend_max_rate: 10.0,
            backend_min_volume: 0.0,
            backend_max_volume: 1.0,
        }
    }

    /// Initialize the TTS engine (lazy initialization)
    fn ensure_initialized(&mut self) -> Result<()> {
        if self.enabled && self.engine.is_none() {
            tracing::info!("Initializing TTS engine...");
            let mut tts = Tts::default()?;

            // Query backend min/max ranges
            self.backend_min_rate = tts.min_rate();
            self.backend_max_rate = tts.max_rate();
            self.backend_min_volume = tts.min_volume();
            self.backend_max_volume = tts.max_volume();

            tracing::info!(
                "TTS backend ranges: rate={} to {}, volume={} to {}",
                self.backend_min_rate,
                self.backend_max_rate,
                self.backend_min_volume,
                self.backend_max_volume
            );

            // Normalize config values to backend ranges
            let normalized_rate = self.normalize_rate(self.rate);
            let normalized_volume = self.normalize_volume(self.volume);

            // Apply normalized settings
            let _ = tts.set_rate(normalized_rate);
            let _ = tts.set_volume(normalized_volume);
            tracing::info!(
                "TTS configured: config rate={} (normalized to {}), config volume={} (normalized to {})",
                self.rate,
                normalized_rate,
                self.volume,
                normalized_volume
            );

            // Set up callback for auto-play
            let tx = self.event_tx.clone();
            tts.on_utterance_end(Some(Box::new(move |_id| {
                let _ = tx.send(TtsEvent::UtteranceEnded);
            })))?;

            self.engine = Some(tts);
            tracing::info!("TTS engine initialized successfully with callbacks");
        }
        Ok(())
    }

    /// Normalize config rate value to backend's actual range
    fn normalize_rate(&self, config_rate: f32) -> f32 {
        // Most TTS backends use 1.0 as normal speed, so just clamp to their range
        // Config: 0.5 = slow, 1.0 = normal, 2.0 = fast
        config_rate.clamp(self.backend_min_rate, self.backend_max_rate)
    }

    /// Normalize config volume value to backend's actual range
    fn normalize_volume(&self, config_volume: f32) -> f32 {
        // Config volume is expected to be 0.0 to 1.0
        // Map it directly to backend's range
        let clamped = config_volume.clamp(0.0, 1.0);
        self.backend_min_volume + clamped * (self.backend_max_volume - self.backend_min_volume)
    }

    /// Enqueue a speech event
    pub fn enqueue(&mut self, entry: SpeechEntry) {
        if !self.enabled || self.muted {
            return;
        }

        // Prevent queue from growing unbounded
        if self.queue.len() >= self.max_queue_size {
            tracing::warn!("TTS queue full ({} entries), dropping oldest", self.max_queue_size);
            self.queue.pop_front();

            // Adjust current_index since we removed from the front
            if let Some(current) = self.current_index {
                if current > 0 {
                    self.current_index = Some(current - 1);
                } else {
                    // Was pointing at the dropped message
                    self.current_index = None;
                }
            }
        }

        // Insert based on priority (higher priority first)
        let insert_pos = self.queue
            .iter()
            .position(|e| e.priority < entry.priority)
            .unwrap_or(self.queue.len());

        self.queue.insert(insert_pos, entry);

        // Adjust current_index if insertion happened before current position
        // (This only happens for high/critical priority messages jumping the queue)
        if let Some(current) = self.current_index {
            if insert_pos <= current {
                self.current_index = Some(current + 1);
                tracing::debug!("High-priority message inserted at {} - adjusted current_index from {} to {}", insert_pos, current, current + 1);
            }
        }

        // Queue silently - user has full manual control with prev/next/next_unread
        // Messages will auto-play via callbacks when user manually navigates
        tracing::debug!("Queued message at index {} (total: {})", insert_pos, self.queue.len());
    }

    /// Speak the next item in the queue (sequential, includes read messages)
    pub fn speak_next(&mut self) -> Result<()> {
        if !self.enabled || self.muted {
            return Ok(());
        }

        self.ensure_initialized()?;

        // Navigate sequentially (like pressing next on an answering machine)
        let next_index = if let Some(current) = self.current_index {
            if current + 1 < self.queue.len() {
                Some(current + 1)
            } else {
                None // At end of queue
            }
        } else {
            // If no current position, start from beginning
            if !self.queue.is_empty() {
                Some(0)
            } else {
                None
            }
        };

        if let Some(index) = next_index {
            self.speak_at_index(index, true)?; // Interrupt for manual navigation
        } else {
            tracing::debug!("At end of TTS queue");
        }

        Ok(())
    }

    /// Speak the previous item in the queue
    pub fn speak_previous(&mut self) -> Result<()> {
        if !self.enabled || self.muted {
            return Ok(());
        }

        self.ensure_initialized()?;

        // Find the previous entry (spoken or not)
        let prev_index = if let Some(current) = self.current_index {
            if current > 0 {
                Some(current - 1)
            } else {
                None
            }
        } else {
            // If nothing playing, start from end
            if !self.queue.is_empty() {
                Some(self.queue.len() - 1)
            } else {
                None
            }
        };

        if let Some(index) = prev_index {
            self.speak_at_index(index, true)?; // Interrupt for manual navigation
        }

        Ok(())
    }

    /// Skip to the next unread (unspoken) message in the queue
    /// If no current position, jumps to LATEST (highest index) unread message
    pub fn speak_next_unread(&mut self) -> Result<()> {
        if !self.enabled || self.muted {
            return Ok(());
        }

        self.ensure_initialized()?;

        // Find the next unspoken entry
        let next_index = if let Some(current) = self.current_index {
            // From current position, search forward for next unread
            (current + 1..self.queue.len())
                .find(|&i| !self.queue[i].spoken)
        } else {
            // No current position - jump to LATEST unread (highest index)
            (0..self.queue.len())
                .rev()  // Search backwards from end
                .find(|&i| !self.queue[i].spoken)
        };

        if let Some(index) = next_index {
            self.speak_at_index(index, true)?; // Interrupt for manual navigation
        } else {
            tracing::debug!("No more unread entries in TTS queue");
        }

        Ok(())
    }

    /// Auto-play the next unread item (called from utterance_end callback)
    /// Does NOT interrupt - only plays if there's an unspoken item waiting
    pub fn auto_play_next(&mut self) -> Result<()> {
        if !self.enabled || self.muted {
            return Ok(());
        }

        // Find next unspoken entry
        let next_index = if let Some(current) = self.current_index {
            (current + 1..self.queue.len())
                .find(|&i| !self.queue[i].spoken)
        } else {
            (0..self.queue.len())
                .find(|&i| !self.queue[i].spoken)
        };

        if let Some(index) = next_index {
            // Auto-play - don't interrupt (nothing should be playing)
            self.speak_at_index(index, false)?;
            tracing::debug!("Auto-playing next TTS entry at index {}", index);
        }

        Ok(())
    }

    /// Speak the entry at a specific index
    /// interrupt: if true, stops current speech before speaking (for manual navigation)
    fn speak_at_index(&mut self, index: usize, interrupt: bool) -> Result<()> {
        if let Some(entry) = self.queue.get_mut(index) {
            if let Some(ref mut engine) = self.engine {
                tracing::debug!("Speaking [{}]: {}", entry.source_window, entry.text);

                // For manual navigation, interrupt current speech
                // For auto-play, let it queue (though the tts crate may not truly queue)
                if interrupt {
                    let _ = engine.stop();
                }

                // Speak the text
                match engine.speak(&entry.text, false)? {
                    Some(_) => {
                        entry.spoken = true;
                        self.current_index = Some(index);
                        tracing::debug!("Speaking at index {}", index);
                    }
                    None => {
                        tracing::warn!("TTS speak() returned no UtteranceId");
                        entry.spoken = true;
                        self.current_index = Some(index);
                    }
                }
            }
        }

        Ok(())
    }

    /// Stop current speech (does NOT change current_index position)
    pub fn stop(&mut self) -> Result<()> {
        if let Some(ref mut engine) = self.engine {
            engine.stop()?;
        }

        // Don't change current_index - this preserves position for next/previous navigation
        Ok(())
    }

    /// Toggle mute
    pub fn toggle_mute(&mut self) {
        self.muted = !self.muted;

        if self.muted {
            tracing::info!("TTS muted");
            let _ = self.stop();
        } else {
            tracing::info!("TTS unmuted");
        }
    }

    /// Clear the queue
    pub fn clear_queue(&mut self) {
        self.queue.clear();
        self.current_index = None;
        tracing::debug!("TTS queue cleared");
    }

    /// Get current queue size
    pub fn queue_size(&self) -> usize {
        self.queue.len()
    }

    /// Check if muted
    pub fn is_muted(&self) -> bool {
        self.muted
    }

    /// Check if enabled
    pub fn is_enabled(&self) -> bool {
        self.enabled
    }

    /// Set enabled state
    pub fn set_enabled(&mut self, enabled: bool) {
        if self.enabled != enabled {
            self.enabled = enabled;
            if !enabled {
                let _ = self.stop();
                self.clear_queue();
            }
        }
    }

    /// Increase speech rate by 0.1
    pub fn increase_rate(&mut self) -> Result<()> {
        self.rate = (self.rate + 0.1).min(2.0); // Cap at 2.0
        let normalized = self.normalize_rate(self.rate);

        if let Some(ref mut engine) = self.engine {
            engine.set_rate(normalized)?;
            tracing::info!("TTS rate increased to {} (normalized: {})", self.rate, normalized);
        }
        Ok(())
    }

    /// Decrease speech rate by 0.1
    pub fn decrease_rate(&mut self) -> Result<()> {
        self.rate = (self.rate - 0.1).max(0.5); // Don't go below 0.5
        let normalized = self.normalize_rate(self.rate);

        if let Some(ref mut engine) = self.engine {
            engine.set_rate(normalized)?;
            tracing::info!("TTS rate decreased to {} (normalized: {})", self.rate, normalized);
        }
        Ok(())
    }

    /// Increase volume by 0.1
    pub fn increase_volume(&mut self) -> Result<()> {
        self.volume = (self.volume + 0.1).min(1.0); // Cap at 1.0
        let normalized = self.normalize_volume(self.volume);

        if let Some(ref mut engine) = self.engine {
            engine.set_volume(normalized)?;
            tracing::info!("TTS volume increased to {} (normalized: {})", self.volume, normalized);
        }
        Ok(())
    }

    /// Decrease volume by 0.1
    pub fn decrease_volume(&mut self) -> Result<()> {
        self.volume = (self.volume - 0.1).max(0.0); // Don't go below 0.0
        let normalized = self.normalize_volume(self.volume);

        if let Some(ref mut engine) = self.engine {
            engine.set_volume(normalized)?;
            tracing::info!("TTS volume decreased to {} (normalized: {})", self.volume, normalized);
        }
        Ok(())
    }

    /// Try to receive a TTS event from the callback channel (non-blocking)
    pub fn try_recv_event(&self) -> Result<TtsEvent, std::sync::mpsc::TryRecvError> {
        self.event_rx.try_recv()
    }

}
