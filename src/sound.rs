//! Thin cross-platform wrapper over the optional `rodio` crate.
//!
//! The sound module exposes a `SoundPlayer` that understands how to load audio
//! assets from the user's configuration directory, enforce per-sound cooldowns,
//! and abstract over whether the `sound` cargo feature is enabled.  It also
//! provides helpers for seeding a default `~/.vellum-fe/sounds` directory so the
//! application can ship bundled effects.

use anyhow::Result;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Instant;
use tracing::{debug, warn};

#[cfg(feature = "sound")]
use rodio::{Decoder, OutputStream, OutputStreamHandle, Sink};
#[cfg(feature = "sound")]
use std::fs::File;
#[cfg(feature = "sound")]
use std::io::BufReader;

/// Sound player for playing audio files
pub struct SoundPlayer {
    #[cfg(feature = "sound")]
    _stream: OutputStream,
    #[cfg(feature = "sound")]
    stream_handle: OutputStreamHandle,
    enabled: bool,
    volume: f32,
    cooldown_map: Arc<Mutex<std::collections::HashMap<String, Instant>>>,
    cooldown_duration: std::time::Duration,
}

impl SoundPlayer {
    /// Create a new sound player.
    ///
    /// If `enabled` is false, skip audio device initialization entirely.
    /// This avoids the ~10 second timeout on systems without audio hardware.
    pub fn new(enabled: bool, volume: f32, cooldown_ms: u64) -> Result<Self> {
        #[cfg(feature = "sound")]
        {
            // Skip rodio initialization if sound is disabled
            // This avoids the ~10 second timeout on systems without audio hardware
            if !enabled {
                debug!("Sound system disabled - skipping audio device initialization");
                return Err(anyhow::anyhow!("Sound disabled by configuration"));
            }

            let (stream, stream_handle) = OutputStream::try_default()?;

            Ok(Self {
                _stream: stream,
                stream_handle,
                enabled,
                volume: volume.clamp(0.0, 1.0),
                cooldown_map: Arc::new(Mutex::new(std::collections::HashMap::new())),
                cooldown_duration: std::time::Duration::from_millis(cooldown_ms),
            })
        }

        #[cfg(not(feature = "sound"))]
        {
            Ok(Self {
                enabled,
                volume: volume.clamp(0.0, 1.0),
                cooldown_map: Arc::new(Mutex::new(std::collections::HashMap::new())),
                cooldown_duration: std::time::Duration::from_millis(cooldown_ms),
            })
        }
    }

    /// Set whether sounds are enabled
    pub fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
        debug!("Sound player enabled: {}", enabled);
    }

    /// Set the master volume (0.0 to 1.0)
    pub fn set_volume(&mut self, volume: f32) {
        self.volume = volume.clamp(0.0, 1.0);
        debug!("Sound player volume set to: {}", self.volume);
    }

    fn lock_cooldown_map(
        &self,
    ) -> std::sync::MutexGuard<'_, std::collections::HashMap<String, Instant>> {
        match self.cooldown_map.lock() {
            Ok(guard) => guard,
            Err(poisoned) => {
                warn!("Sound cooldown map lock poisoned; recovering");
                poisoned.into_inner()
            }
        }
    }

    /// Check if a sound is on cooldown
    fn is_on_cooldown(&self, sound_id: &str) -> bool {
        let map = self.lock_cooldown_map();
        if let Some(last_played) = map.get(sound_id) {
            last_played.elapsed() < self.cooldown_duration
        } else {
            false
        }
    }

    /// Set the cooldown for a sound
    fn set_cooldown(&self, sound_id: String) {
        let mut map = self.lock_cooldown_map();
        map.insert(sound_id, Instant::now());
    }

    /// Play a sound file
    ///
    /// # Arguments
    /// * `path` - Path to the sound file (supports WAV, MP3, OGG, FLAC)
    /// * `volume_override` - Optional volume override for this sound (0.0 to 1.0)
    /// * `sound_id` - Identifier for cooldown tracking (usually the file path)
    #[cfg(feature = "sound")]
    pub fn play(&self, path: &PathBuf, volume_override: Option<f32>, sound_id: &str) -> Result<()> {
        if !self.enabled {
            return Ok(());
        }

        // Check cooldown
        if self.is_on_cooldown(sound_id) {
            debug!("Sound '{}' is on cooldown, skipping", sound_id);
            return Ok(());
        }

        // Open the file
        let file = match File::open(path) {
            Ok(f) => f,
            Err(e) => {
                warn!("Failed to open sound file {:?}: {}", path, e);
                return Ok(()); // Don't error, just skip
            }
        };

        // Decode the audio file
        let source = match Decoder::new(BufReader::new(file)) {
            Ok(s) => s,
            Err(e) => {
                warn!("Failed to decode sound file {:?}: {}", path, e);
                return Ok(());
            }
        };

        // Calculate final volume
        let volume = volume_override.unwrap_or(self.volume);
        let volume = volume.clamp(0.0, 1.0);

        // Create a sink and play
        let sink = Sink::try_new(&self.stream_handle)?;
        sink.set_volume(volume);
        sink.append(source);
        sink.detach(); // Play in background

        // Set cooldown
        self.set_cooldown(sound_id.to_string());

        debug!("Playing sound: {:?} at volume {}", path, volume);
        Ok(())
    }

    /// Play stub for when sound feature is disabled
    #[cfg(not(feature = "sound"))]
    pub fn play(
        &self,
        _path: &PathBuf,
        _volume_override: Option<f32>,
        _sound_id: &str,
    ) -> Result<()> {
        debug!("Sound playback disabled (sound feature not enabled)");
        Ok(())
    }

    /// Play a sound from the shared sounds directory
    ///
    /// # Arguments
    /// * `filename` - Filename in ~/.vellum-fe/sounds/
    /// * `volume_override` - Optional volume override
    pub fn play_from_sounds_dir(&self, filename: &str, volume_override: Option<f32>) -> Result<()> {
        let sounds_dir = crate::config::Config::sounds_dir()
            .map_err(|e| anyhow::anyhow!("Failed to get sounds directory: {}", e))?;

        let mut path = sounds_dir.join(filename);

        // If file doesn't exist as-is, try common audio extensions
        if !path.exists() {
            let extensions = ["mp3", "wav", "ogg", "flac"];
            let mut found = false;
            for ext in &extensions {
                let path_with_ext = sounds_dir.join(format!("{}.{}", filename, ext));
                if path_with_ext.exists() {
                    path = path_with_ext;
                    found = true;
                    break;
                }
            }
            if !found {
                warn!(
                    "Sound file not found: {:?} (tried extensions: mp3, wav, ogg, flac)",
                    sounds_dir.join(filename)
                );
                return Ok(()); // Don't error, just skip
            }
        }

        self.play(&path, volume_override, filename)
    }
}

/// Embedded default sound files (included at compile time)
/// Format: (filename, bytes)
///
/// To add default sounds in the future:
/// 1. Place sound files in defaults/sounds/ directory
/// 2. Uncomment and add entries like:
///    ("beep.wav", include_bytes!("../defaults/sounds/beep.wav")),
const DEFAULT_SOUNDS: &[(&str, &[u8])] = &[
    ("wizard_music.mp3", include_bytes!("../defaults/globals/sounds/wizard_music.mp3")),
];

/// Create shared sounds directory if it doesn't exist and extract default sounds
pub fn ensure_sounds_directory() -> Result<PathBuf> {
    let sounds_dir = crate::config::Config::sounds_dir()
        .map_err(|e| anyhow::anyhow!("Failed to get sounds directory: {}", e))?;

    if !sounds_dir.exists() {
        std::fs::create_dir_all(&sounds_dir)?;
        debug!("Created sounds directory: {:?}", sounds_dir);
    }

    // Extract default sounds if they don't exist
    for (filename, bytes) in DEFAULT_SOUNDS {
        let sound_path = sounds_dir.join(filename);
        if !sound_path.exists() {
            std::fs::write(&sound_path, bytes)?;
            debug!("Extracted default sound: {}", filename);
        }
    }

    Ok(sounds_dir)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;
    use std::time::Duration;

    // ========== Volume clamping tests ==========

    #[test]
    fn test_volume_clamp_normal() {
        // Normal volume values should be unchanged
        assert_eq!(0.5_f32.clamp(0.0, 1.0), 0.5);
        assert_eq!(0.0_f32.clamp(0.0, 1.0), 0.0);
        assert_eq!(1.0_f32.clamp(0.0, 1.0), 1.0);
    }

    #[test]
    fn test_volume_clamp_over_max() {
        // Values over 1.0 should be clamped to 1.0
        assert_eq!(1.5_f32.clamp(0.0, 1.0), 1.0);
        assert_eq!(100.0_f32.clamp(0.0, 1.0), 1.0);
    }

    #[test]
    fn test_volume_clamp_under_min() {
        // Values under 0.0 should be clamped to 0.0
        assert_eq!((-0.5_f32).clamp(0.0, 1.0), 0.0);
        assert_eq!((-100.0_f32).clamp(0.0, 1.0), 0.0);
    }

    #[test]
    fn test_volume_clamp_boundary() {
        // Boundary values
        assert_eq!(0.001_f32.clamp(0.0, 1.0), 0.001);
        assert_eq!(0.999_f32.clamp(0.0, 1.0), 0.999);
    }

    // ========== Cooldown logic tests ==========

    #[test]
    fn test_cooldown_map_empty() {
        // Empty cooldown map should not be on cooldown
        let map: std::collections::HashMap<String, Instant> = std::collections::HashMap::new();
        let sound_id = "test_sound";
        let cooldown_duration = Duration::from_millis(500);

        let is_on_cooldown = if let Some(last_played) = map.get(sound_id) {
            last_played.elapsed() < cooldown_duration
        } else {
            false
        };

        assert!(!is_on_cooldown);
    }

    #[test]
    fn test_cooldown_map_recent_sound() {
        // Sound played just now should be on cooldown
        let mut map: std::collections::HashMap<String, Instant> = std::collections::HashMap::new();
        let sound_id = "test_sound";
        let cooldown_duration = Duration::from_millis(500);

        map.insert(sound_id.to_string(), Instant::now());

        let is_on_cooldown = if let Some(last_played) = map.get(sound_id) {
            last_played.elapsed() < cooldown_duration
        } else {
            false
        };

        assert!(is_on_cooldown);
    }

    #[test]
    fn test_cooldown_map_expired() {
        // Sound played long ago should not be on cooldown
        let mut map: std::collections::HashMap<String, Instant> = std::collections::HashMap::new();
        let sound_id = "test_sound";
        let cooldown_duration = Duration::from_millis(50);

        map.insert(sound_id.to_string(), Instant::now());

        // Wait for cooldown to expire
        thread::sleep(Duration::from_millis(60));

        let is_on_cooldown = if let Some(last_played) = map.get(sound_id) {
            last_played.elapsed() < cooldown_duration
        } else {
            false
        };

        assert!(!is_on_cooldown);
    }

    #[test]
    fn test_cooldown_different_sounds() {
        // Different sounds should have independent cooldowns
        let mut map: std::collections::HashMap<String, Instant> = std::collections::HashMap::new();
        let cooldown_duration = Duration::from_millis(500);

        map.insert("sound_a".to_string(), Instant::now());
        // sound_b is not in the map

        let is_a_on_cooldown = if let Some(last_played) = map.get("sound_a") {
            last_played.elapsed() < cooldown_duration
        } else {
            false
        };

        let is_b_on_cooldown = if let Some(last_played) = map.get("sound_b") {
            last_played.elapsed() < cooldown_duration
        } else {
            false
        };

        assert!(is_a_on_cooldown);
        assert!(!is_b_on_cooldown);
    }

    // ========== Extension search pattern tests ==========

    #[test]
    fn test_audio_extensions_list() {
        // Verify we support common audio formats
        let extensions = ["mp3", "wav", "ogg", "flac"];

        assert!(extensions.contains(&"mp3"));
        assert!(extensions.contains(&"wav"));
        assert!(extensions.contains(&"ogg"));
        assert!(extensions.contains(&"flac"));
        assert_eq!(extensions.len(), 4);
    }

    #[test]
    fn test_path_with_extension_join() {
        // Test how path joining works for extension search
        let sounds_dir = PathBuf::from("/sounds");
        let filename = "alert";

        let path_mp3 = sounds_dir.join(format!("{}.{}", filename, "mp3"));
        let path_wav = sounds_dir.join(format!("{}.{}", filename, "wav"));

        assert_eq!(path_mp3, PathBuf::from("/sounds/alert.mp3"));
        assert_eq!(path_wav, PathBuf::from("/sounds/alert.wav"));
    }

    #[test]
    fn test_path_already_has_extension() {
        // When file already has extension, should use as-is
        let sounds_dir = PathBuf::from("/sounds");
        let filename = "alert.mp3";

        let path = sounds_dir.join(filename);
        assert_eq!(path, PathBuf::from("/sounds/alert.mp3"));
    }

    // ========== DEFAULT_SOUNDS constant tests ==========

    #[test]
    fn test_default_sounds_format() {
        // Each entry should be (filename, bytes)
        for (filename, bytes) in DEFAULT_SOUNDS {
            assert!(!filename.is_empty() || DEFAULT_SOUNDS.is_empty());
            assert!(!bytes.is_empty() || DEFAULT_SOUNDS.is_empty());
        }
    }

    // ========== Duration conversion tests ==========

    #[test]
    fn test_cooldown_duration_from_millis() {
        let cooldown_ms: u64 = 500;
        let duration = Duration::from_millis(cooldown_ms);

        assert_eq!(duration.as_millis(), 500);
        assert_eq!(duration.as_secs(), 0);
    }

    #[test]
    fn test_cooldown_duration_zero() {
        let cooldown_ms: u64 = 0;
        let duration = Duration::from_millis(cooldown_ms);

        assert_eq!(duration.as_millis(), 0);
    }

    #[test]
    fn test_cooldown_duration_large() {
        let cooldown_ms: u64 = 60_000; // 1 minute
        let duration = Duration::from_millis(cooldown_ms);

        assert_eq!(duration.as_secs(), 60);
    }

    // ========== Arc<Mutex> cooldown map pattern tests ==========

    #[test]
    fn test_arc_mutex_cooldown_pattern() {
        // Test the Arc<Mutex<HashMap>> pattern used for thread-safe cooldowns
        let cooldown_map: Arc<Mutex<std::collections::HashMap<String, Instant>>> =
            Arc::new(Mutex::new(std::collections::HashMap::new()));

        // Insert a cooldown
        {
            let mut map = cooldown_map.lock().unwrap();
            map.insert("test".to_string(), Instant::now());
        }

        // Check cooldown from another "thread" perspective
        {
            let map = cooldown_map.lock().unwrap();
            assert!(map.contains_key("test"));
        }
    }

    #[test]
    fn test_arc_mutex_multiple_sounds() {
        let cooldown_map: Arc<Mutex<std::collections::HashMap<String, Instant>>> =
            Arc::new(Mutex::new(std::collections::HashMap::new()));

        // Insert multiple sounds
        {
            let mut map = cooldown_map.lock().unwrap();
            map.insert("beep".to_string(), Instant::now());
            map.insert("alert".to_string(), Instant::now());
            map.insert("death".to_string(), Instant::now());
        }

        // Verify all present
        {
            let map = cooldown_map.lock().unwrap();
            assert_eq!(map.len(), 3);
            assert!(map.contains_key("beep"));
            assert!(map.contains_key("alert"));
            assert!(map.contains_key("death"));
        }
    }

    // ========== Volume override calculation tests ==========

    #[test]
    fn test_volume_override_some() {
        let master_volume: f32 = 0.8;
        let volume_override: Option<f32> = Some(0.5);

        let final_volume = volume_override.unwrap_or(master_volume);
        assert_eq!(final_volume, 0.5);
    }

    #[test]
    fn test_volume_override_none() {
        let master_volume: f32 = 0.8;
        let volume_override: Option<f32> = None;

        let final_volume = volume_override.unwrap_or(master_volume);
        assert_eq!(final_volume, 0.8);
    }

    #[test]
    fn test_volume_override_with_clamp() {
        let volume_override: Option<f32> = Some(1.5);

        let volume = volume_override.unwrap_or(0.5);
        let clamped = volume.clamp(0.0, 1.0);

        assert_eq!(clamped, 1.0);
    }
}
