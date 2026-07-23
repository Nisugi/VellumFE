//! Text-to-Speech System
//!
//! Provides accessibility support through text-to-speech output.
//! Features:
//! - Cross-platform TTS (Windows SAPI, macOS AVSpeechSynthesizer, Linux Speech Dispatcher)
//! - Chronological speech queue with answering-machine navigation
//!   (Next/Previous/Next-unread/Stop/Mute)
//! - Alert class that interrupts current speech immediately
//! - Speech-only gag patterns and pronunciation substitutions
//! - Repeated-line coalescing ("You hit!" x4 -> spoken once with a count)
//! - Zero performance cost when disabled
//!
//! Queue model: lines are spoken strictly in arrival order. New lines never
//! interrupt the current utterance - auto-play advances only when the engine
//! reports the previous utterance finished (`handle_utterance_ended`, wired
//! from the frontends' event loops via `try_recv_event`). The exception is
//! `Priority::Alert`, which stops current speech and speaks at once. Manual
//! navigation (next/previous/next-unread) always interrupts - that's the
//! user grabbing the microphone.

use anyhow::Result;
use std::collections::VecDeque;
use std::sync::mpsc::{channel, Receiver, Sender};
#[cfg(feature = "tts")]
use tts::Tts;

/// Events sent from TTS callbacks to the main event loop
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TtsEvent {
    UtteranceStarted,
    UtteranceEnded,
    UtteranceStopped,
}

/// Speech classes. `Normal` and `High` queue chronologically (`High` is kept
/// for future audio cues); `Alert` interrupts current speech and plays
/// immediately without disturbing the queue position.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Priority {
    Normal = 0,
    High = 1,
    Alert = 2,
}

/// A single speech entry in the queue
#[derive(Debug, Clone)]
pub struct SpeechEntry {
    pub text: String,
    pub source_window: String,
    pub priority: Priority,
    pub spoken: bool,
    /// How many identical consecutive lines this entry stands for.
    pub repeats: u32,
}

/// A pronunciation substitution applied to text before it is spoken.
struct SpeechSubstitution {
    pattern: regex::Regex,
    replacement: String,
}

/// Text-to-Speech manager
///
/// Manages the speech queue and TTS engine.
/// When TTS is disabled (config disabled), this is a no-op.
pub struct TtsManager {
    // Without the `tts` feature the engine field doesn't exist and
    // `ensure_initialized` never runs: every public method is a queue-only
    // no-op, which is also the behavior when TTS is disabled in config.
    #[cfg(feature = "tts")]
    engine: Option<Tts>,

    /// Speech queue in arrival order.
    queue: VecDeque<SpeechEntry>,

    /// Current index in queue (None if never navigated/spoken).
    current_index: Option<usize>,

    /// True between speak() and the engine's utterance-end/stop callback.
    speaking: bool,

    /// Is TTS globally muted?
    muted: bool,

    /// Is TTS enabled in config?
    enabled: bool,

    /// Speech rate from config (0.5 slow, 1.0 = engine normal, 3.0 = engine max)
    rate: f32,

    /// Speech volume from config (0.0 to 1.0)
    volume: f32,

    /// Preferred voice by name (engine default when None or not found).
    voice: Option<String>,

    /// Speech-only gags: lines matching any pattern are never spoken.
    gags: Vec<regex::Regex>,

    /// Pronunciation substitutions applied in order before speaking.
    substitutions: Vec<SpeechSubstitution>,

    /// Maximum queue size (prevent memory bloat)
    max_queue_size: usize,

    /// Event channel for TTS callbacks
    #[cfg_attr(not(feature = "tts"), allow(dead_code))]
    event_tx: Sender<TtsEvent>,
    event_rx: Receiver<TtsEvent>,

    /// Backend rate/volume ranges for normalization
    backend_min_rate: f32,
    backend_max_rate: f32,
    backend_normal_rate: f32,
    backend_min_volume: f32,
    backend_max_volume: f32,
}

impl TtsManager {
    /// Create a new TTS manager
    pub fn new(enabled: bool, rate: f32, volume: f32) -> Self {
        let (event_tx, event_rx) = channel();

        Self {
            #[cfg(feature = "tts")]
            engine: None,
            queue: VecDeque::new(),
            current_index: None,
            speaking: false,
            muted: false,
            enabled,
            rate,
            volume,
            voice: None,
            gags: Vec::new(),
            substitutions: Vec::new(),
            max_queue_size: 200,
            event_tx,
            event_rx,
            // Default ranges (will be updated during initialization)
            backend_min_rate: 0.1,
            backend_max_rate: 10.0,
            backend_normal_rate: 1.0,
            backend_min_volume: 0.0,
            backend_max_volume: 1.0,
        }
    }

    /// Initialize the TTS engine (lazy initialization)
    fn ensure_initialized(&mut self) -> Result<()> {
        #[cfg(feature = "tts")]
        if self.enabled && self.engine.is_none() {
            tracing::info!("Initializing TTS engine...");
            let mut tts = Tts::default()?;

            // Query backend ranges
            self.backend_min_rate = tts.min_rate();
            self.backend_max_rate = tts.max_rate();
            self.backend_normal_rate = tts.normal_rate();
            self.backend_min_volume = tts.min_volume();
            self.backend_max_volume = tts.max_volume();

            tracing::info!(
                "TTS backend ranges: rate={} to {} (normal {}), volume={} to {}",
                self.backend_min_rate,
                self.backend_max_rate,
                self.backend_normal_rate,
                self.backend_min_volume,
                self.backend_max_volume
            );

            let normalized_rate = self.normalize_rate(self.rate);
            let normalized_volume = self.normalize_volume(self.volume);
            let _ = tts.set_rate(normalized_rate);
            let _ = tts.set_volume(normalized_volume);

            // Utterance-end drives auto-play; stop keeps `speaking` honest.
            let end_tx = self.event_tx.clone();
            tts.on_utterance_end(Some(Box::new(move |_id| {
                let _ = end_tx.send(TtsEvent::UtteranceEnded);
            })))?;
            let stop_tx = self.event_tx.clone();
            let _ = tts.on_utterance_stop(Some(Box::new(move |_id| {
                let _ = stop_tx.send(TtsEvent::UtteranceStopped);
            })));

            self.engine = Some(tts);
            self.apply_voice();
            tracing::info!("TTS engine initialized successfully with callbacks");
        }
        Ok(())
    }

    /// Map the config rate (0.5..=3.0, 1.0 = normal) onto the backend's
    /// actual range, piecewise around the backend's normal rate so 1.0
    /// always means "the engine's natural speed".
    fn normalize_rate(&self, config_rate: f32) -> f32 {
        let clamped = config_rate.clamp(0.5, 3.0);
        if clamped <= 1.0 {
            // 0.5..1.0 -> min..normal
            let t = (clamped - 0.5) / 0.5;
            self.backend_min_rate + t * (self.backend_normal_rate - self.backend_min_rate)
        } else {
            // 1.0..3.0 -> normal..max
            let t = (clamped - 1.0) / 2.0;
            self.backend_normal_rate + t * (self.backend_max_rate - self.backend_normal_rate)
        }
    }

    /// Normalize config volume value to backend's actual range
    fn normalize_volume(&self, config_volume: f32) -> f32 {
        let clamped = config_volume.clamp(0.0, 1.0);
        self.backend_min_volume + clamped * (self.backend_max_volume - self.backend_min_volume)
    }

    /// Install speech-only filters. Invalid regexes are skipped with a log
    /// line (the editor validates before save; this is belt and braces).
    pub fn set_filters(&mut self, gags: &[String], substitutions: &[(String, String)]) {
        self.gags = gags
            .iter()
            .filter_map(|pattern| match regex::Regex::new(pattern) {
                Ok(re) => Some(re),
                Err(err) => {
                    tracing::warn!("TTS gag pattern '{}' invalid: {}", pattern, err);
                    None
                }
            })
            .collect();
        self.substitutions = substitutions
            .iter()
            .filter_map(|(pattern, replacement)| match regex::Regex::new(pattern) {
                Ok(re) => Some(SpeechSubstitution {
                    pattern: re,
                    replacement: replacement.clone(),
                }),
                Err(err) => {
                    tracing::warn!("TTS substitution pattern '{}' invalid: {}", pattern, err);
                    None
                }
            })
            .collect();
    }

    /// Enqueue a speech event. Chronological: never reorders, never
    /// interrupts. Auto-plays only if nothing is currently speaking.
    /// `Priority::Alert` interrupts and speaks immediately instead.
    pub fn enqueue(&mut self, entry: SpeechEntry) {
        if !self.enabled || self.muted {
            return;
        }

        // Speech-only gags.
        if self.gags.iter().any(|re| re.is_match(&entry.text)) {
            return;
        }

        // Pronunciation substitutions.
        let mut entry = entry;
        for sub in &self.substitutions {
            if let std::borrow::Cow::Owned(replaced) =
                sub.pattern.replace_all(&entry.text, sub.replacement.as_str())
            {
                entry.text = replaced;
            }
        }

        if entry.priority == Priority::Alert {
            // Alerts jump the line entirely: speak now, don't queue.
            if let Err(err) = self.speak_text_now(&entry.text) {
                tracing::warn!("TTS alert failed: {}", err);
            }
            return;
        }

        // Coalesce identical consecutive unspoken lines (combat spam).
        if let Some(last) = self.queue.back_mut() {
            if !last.spoken && last.text == entry.text {
                last.repeats = last.repeats.saturating_add(1);
                return;
            }
        }

        // Prevent queue from growing unbounded
        if self.queue.len() >= self.max_queue_size {
            tracing::warn!(
                "TTS queue full ({} entries), dropping oldest",
                self.max_queue_size
            );
            self.queue.pop_front();
            // Adjust current_index since we removed from the front
            if let Some(current) = self.current_index {
                self.current_index = current.checked_sub(1);
            }
        }

        self.queue.push_back(entry);

        // Kick off playback only when idle - the utterance-end callback
        // chains the rest of the queue.
        if !self.speaking {
            if let Err(err) = self.auto_play_next() {
                tracing::warn!("TTS auto-play failed: {}", err);
            }
        }
    }

    /// Speak arbitrary text immediately, interrupting anything current.
    /// Used for alerts and the settings panel's test button.
    pub fn speak_text_now(&mut self, text: &str) -> Result<()> {
        if !self.enabled || self.muted {
            return Ok(());
        }
        self.ensure_initialized()?;
        #[cfg(feature = "tts")]
        if let Some(ref mut engine) = self.engine {
            let _ = engine.stop();
            engine.speak(text, false)?;
            self.speaking = true;
        }
        Ok(())
    }

    /// Speak the next item in the queue (sequential, includes read messages)
    pub fn speak_next(&mut self) -> Result<()> {
        if !self.enabled || self.muted {
            return Ok(());
        }

        self.ensure_initialized()?;

        // Navigate sequentially (like pressing next on an answering machine)
        let next_index = match self.current_index {
            Some(current) if current + 1 < self.queue.len() => Some(current + 1),
            Some(_) => None, // At end of queue
            None if !self.queue.is_empty() => Some(0),
            None => None,
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

        let prev_index = match self.current_index {
            Some(current) if current > 0 => Some(current - 1),
            Some(_) => None,
            None if !self.queue.is_empty() => Some(self.queue.len() - 1),
            None => None,
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

        let next_index = if let Some(current) = self.current_index {
            (current + 1..self.queue.len()).find(|&i| !self.queue[i].spoken)
        } else {
            // No current position - jump to LATEST unread (highest index)
            (0..self.queue.len()).rev().find(|&i| !self.queue[i].spoken)
        };

        if let Some(index) = next_index {
            self.speak_at_index(index, true)?; // Interrupt for manual navigation
        } else {
            tracing::debug!("No more unread entries in TTS queue");
        }

        Ok(())
    }

    /// Play the next unread item without interrupting. Called when idle
    /// (from `enqueue`) and when an utterance finishes.
    pub fn auto_play_next(&mut self) -> Result<()> {
        if !self.enabled || self.muted {
            return Ok(());
        }

        self.ensure_initialized()?;

        let next_index = if let Some(current) = self.current_index {
            (current + 1..self.queue.len()).find(|&i| !self.queue[i].spoken)
        } else {
            (0..self.queue.len()).find(|&i| !self.queue[i].spoken)
        };

        if let Some(index) = next_index {
            self.speak_at_index(index, false)?;
            tracing::debug!("Auto-playing next TTS entry at index {}", index);
        }

        Ok(())
    }

    /// Utterance finished naturally: chain the next unread entry.
    /// Call from the event loop on `TtsEvent::UtteranceEnded`.
    pub fn handle_utterance_ended(&mut self) {
        self.speaking = false;
        if let Err(err) = self.auto_play_next() {
            tracing::warn!("TTS auto-play after utterance end failed: {}", err);
        }
    }

    /// Utterance was stopped (interrupt/mute): just clear the speaking flag.
    pub fn handle_utterance_stopped(&mut self) {
        self.speaking = false;
    }

    /// Speak the entry at a specific index
    /// interrupt: if true, stops current speech before speaking (for manual navigation)
    #[cfg_attr(not(feature = "tts"), allow(unused_variables))]
    fn speak_at_index(&mut self, index: usize, interrupt: bool) -> Result<()> {
        let Some(entry) = self.queue.get(index) else {
            return Ok(());
        };
        // Coalesced spam speaks once with the count appended.
        let text = if entry.repeats > 1 {
            format!("{}, {} times", entry.text, entry.repeats)
        } else {
            entry.text.clone()
        };

        #[cfg(feature = "tts")]
        if let Some(ref mut engine) = self.engine {
            tracing::debug!("Speaking [{}]: {}", entry.source_window, text);

            if interrupt {
                let _ = engine.stop();
            }

            engine.speak(&text, false)?;
            self.speaking = true;
        }
        if let Some(entry) = self.queue.get_mut(index) {
            entry.spoken = true;
        }
        self.current_index = Some(index);

        Ok(())
    }

    /// Stop current speech (does NOT change current_index position)
    pub fn stop(&mut self) -> Result<()> {
        #[cfg(feature = "tts")]
        if let Some(ref mut engine) = self.engine {
            engine.stop()?;
        }
        self.speaking = false;

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

    /// Check if an utterance is in flight
    pub fn is_speaking(&self) -> bool {
        self.speaking
    }

    /// Current config-space rate (0.5..=3.0).
    pub fn rate(&self) -> f32 {
        self.rate
    }

    /// Current config-space volume (0.0..=1.0).
    pub fn volume(&self) -> f32 {
        self.volume
    }

    /// Preferred voice name, if one was chosen.
    pub fn voice_name(&self) -> Option<&str> {
        self.voice.as_deref()
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

    /// Set the config-space rate directly (0.5..=3.0) and apply it.
    #[cfg_attr(not(feature = "tts"), allow(unused_variables))]
    pub fn set_rate(&mut self, rate: f32) -> Result<()> {
        self.rate = rate.clamp(0.5, 3.0);
        let normalized = self.normalize_rate(self.rate);
        #[cfg(feature = "tts")]
        if let Some(ref mut engine) = self.engine {
            engine.set_rate(normalized)?;
        }
        Ok(())
    }

    /// Set the config-space volume directly (0.0..=1.0) and apply it.
    #[cfg_attr(not(feature = "tts"), allow(unused_variables))]
    pub fn set_volume(&mut self, volume: f32) -> Result<()> {
        self.volume = volume.clamp(0.0, 1.0);
        let normalized = self.normalize_volume(self.volume);
        #[cfg(feature = "tts")]
        if let Some(ref mut engine) = self.engine {
            engine.set_volume(normalized)?;
        }
        Ok(())
    }

    /// Increase speech rate by 0.1
    pub fn increase_rate(&mut self) -> Result<()> {
        let rate = self.rate;
        self.set_rate(rate + 0.1)?;
        tracing::info!("TTS rate increased to {}", self.rate);
        Ok(())
    }

    /// Decrease speech rate by 0.1
    pub fn decrease_rate(&mut self) -> Result<()> {
        let rate = self.rate;
        self.set_rate(rate - 0.1)?;
        tracing::info!("TTS rate decreased to {}", self.rate);
        Ok(())
    }

    /// Increase volume by 0.1
    pub fn increase_volume(&mut self) -> Result<()> {
        let volume = self.volume;
        self.set_volume(volume + 0.1)?;
        tracing::info!("TTS volume increased to {}", self.volume);
        Ok(())
    }

    /// Decrease volume by 0.1
    pub fn decrease_volume(&mut self) -> Result<()> {
        let volume = self.volume;
        self.set_volume(volume - 0.1)?;
        tracing::info!("TTS volume decreased to {}", self.volume);
        Ok(())
    }

    /// Names of the voices the backend offers (empty when the engine is off,
    /// uninitialized, or the platform doesn't enumerate voices).
    pub fn available_voices(&mut self) -> Vec<String> {
        #[cfg(feature = "tts")]
        {
            if self.enabled {
                let _ = self.ensure_initialized();
            }
            if let Some(ref engine) = self.engine {
                if let Ok(voices) = engine.voices() {
                    return voices.iter().map(|voice| voice.name()).collect();
                }
            }
        }
        Vec::new()
    }

    /// Choose a voice by name (None reverts to the engine default on the
    /// next restart; engines have no universal "reset to default").
    pub fn set_voice_by_name(&mut self, name: Option<String>) {
        self.voice = name;
        self.apply_voice();
    }

    /// Apply the preferred voice to the live engine, if both exist.
    fn apply_voice(&mut self) {
        #[cfg(feature = "tts")]
        if let (Some(engine), Some(wanted)) = (self.engine.as_mut(), self.voice.as_deref()) {
            match engine.voices() {
                Ok(voices) => {
                    if let Some(voice) = voices.iter().find(|v| v.name() == wanted) {
                        if let Err(err) = engine.set_voice(voice) {
                            tracing::warn!("TTS set_voice '{}' failed: {}", wanted, err);
                        }
                    } else {
                        tracing::warn!("TTS voice '{}' not found; keeping default", wanted);
                    }
                }
                Err(err) => tracing::warn!("TTS voices() failed: {}", err),
            }
        }
    }

    /// Try to receive a TTS event from the callback channel (non-blocking)
    pub fn try_recv_event(&self) -> Result<TtsEvent, std::sync::mpsc::TryRecvError> {
        self.event_rx.try_recv()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(text: &str) -> SpeechEntry {
        SpeechEntry {
            text: text.to_string(),
            source_window: "main".to_string(),
            priority: Priority::Normal,
            spoken: false,
            repeats: 1,
        }
    }

    fn manager() -> TtsManager {
        let mut tts = TtsManager::new(true, 1.0, 1.0);
        // Pretend an utterance is in flight: enqueue only auto-plays when
        // idle, so this keeps tests from initializing (and audibly using)
        // the real OS speech engine.
        tts.speaking = true;
        tts
    }

    #[test]
    fn queue_is_chronological() {
        let mut tts = manager();
        tts.enqueue(SpeechEntry {
            priority: Priority::High,
            ..entry("high one")
        });
        tts.enqueue(entry("normal two"));
        tts.enqueue(SpeechEntry {
            priority: Priority::High,
            ..entry("high three")
        });
        let texts: Vec<&str> = tts.queue.iter().map(|e| e.text.as_str()).collect();
        assert_eq!(texts, ["high one", "normal two", "high three"]);
    }

    #[test]
    fn identical_consecutive_lines_coalesce() {
        let mut tts = manager();
        tts.enqueue(entry("You hit!"));
        tts.enqueue(entry("You hit!"));
        tts.enqueue(entry("You hit!"));
        tts.enqueue(entry("You miss!"));
        assert_eq!(tts.queue.len(), 2);
        assert_eq!(tts.queue[0].repeats, 3);
        assert_eq!(tts.queue[1].repeats, 1);
    }

    #[test]
    fn gagged_lines_are_not_queued() {
        let mut tts = manager();
        tts.set_filters(&["^You feel fully rested".to_string()], &[]);
        tts.enqueue(entry("You feel fully rested."));
        tts.enqueue(entry("A rolton bleats."));
        assert_eq!(tts.queue.len(), 1);
        assert_eq!(tts.queue[0].text, "A rolton bleats.");
    }

    #[test]
    fn substitutions_rewrite_text() {
        let mut tts = manager();
        tts.set_filters(
            &[],
            &[("Wehnimer's".to_string(), "Wenimers".to_string())],
        );
        tts.enqueue(entry("You arrive in Wehnimer's Landing."));
        assert_eq!(tts.queue[0].text, "You arrive in Wenimers Landing.");
    }

    #[test]
    fn invalid_filter_patterns_are_skipped() {
        let mut tts = manager();
        tts.set_filters(
            &["[unclosed".to_string(), "valid".to_string()],
            &[("(bad".to_string(), "x".to_string())],
        );
        assert_eq!(tts.gags.len(), 1);
        assert!(tts.substitutions.is_empty());
    }

    #[test]
    fn disabled_manager_queues_nothing() {
        let mut tts = TtsManager::new(false, 1.0, 1.0);
        tts.enqueue(entry("hello"));
        assert_eq!(tts.queue_size(), 0);
    }

    #[test]
    fn queue_overflow_drops_oldest_and_fixes_index() {
        let mut tts = manager();
        tts.max_queue_size = 3;
        tts.enqueue(entry("one"));
        tts.enqueue(entry("two"));
        tts.enqueue(entry("three"));
        tts.current_index = Some(1);
        tts.enqueue(entry("four"));
        assert_eq!(tts.queue.len(), 3);
        assert_eq!(tts.queue[0].text, "two");
        assert_eq!(tts.current_index, Some(0));
    }

    #[test]
    fn rate_normalization_is_piecewise_around_normal() {
        let mut tts = manager();
        tts.backend_min_rate = 0.5;
        tts.backend_max_rate = 6.0;
        tts.backend_normal_rate = 1.0;
        assert_eq!(tts.normalize_rate(1.0), 1.0);
        assert_eq!(tts.normalize_rate(0.5), 0.5);
        assert_eq!(tts.normalize_rate(3.0), 6.0);
        // Halfway up the fast zone lands halfway between normal and max.
        assert_eq!(tts.normalize_rate(2.0), 3.5);
    }
}
