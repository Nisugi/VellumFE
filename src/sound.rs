use anyhow::Result;
use rodio::{Decoder, OutputStream, OutputStreamHandle, Sink};
use std::fs::File;
use std::io::BufReader;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Instant;
use tracing::{debug, warn};

/// Sound player for playing audio files
pub struct SoundPlayer {
    _stream: OutputStream,
    stream_handle: OutputStreamHandle,
    enabled: bool,
    volume: f32,
    cooldown_map: Arc<Mutex<std::collections::HashMap<String, Instant>>>,
    cooldown_duration: std::time::Duration,
}

impl SoundPlayer {
    /// Create a new sound player
    pub fn new(enabled: bool, volume: f32, cooldown_ms: u64) -> Result<Self> {
        use rodio::cpal::traits::{HostTrait, DeviceTrait};

        // Quick check for audio device before attempting full init
        // This prevents long hangs on systems with no audio hardware
        let host = rodio::cpal::default_host();
        let device = host.default_output_device();
        if device.is_none() {
            warn!("No audio output device found - sound disabled");
            anyhow::bail!("No audio output device available");
        }

        // Log the device we're using
        if let Some(ref d) = device {
            if let Ok(name) = d.name() {
                debug!("Using audio device: {}", name);
            }
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

    /// Check if a sound is on cooldown
    fn is_on_cooldown(&self, sound_id: &str) -> bool {
        let map = self.cooldown_map.lock().unwrap();
        if let Some(last_played) = map.get(sound_id) {
            last_played.elapsed() < self.cooldown_duration
        } else {
            false
        }
    }

    /// Set the cooldown for a sound
    fn set_cooldown(&self, sound_id: String) {
        let mut map = self.cooldown_map.lock().unwrap();
        map.insert(sound_id, Instant::now());
    }

    /// Play a sound file
    ///
    /// # Arguments
    /// * `path` - Path to the sound file (supports WAV, MP3, OGG, FLAC)
    /// * `volume_override` - Optional volume override for this sound (0.0 to 1.0)
    /// * `sound_id` - Identifier for cooldown tracking (usually the file path)
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
                warn!("Sound file not found: {:?} (tried extensions: mp3, wav, ogg, flac)", sounds_dir.join(filename));
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
    // Example (commented out until we have default sounds):
    // ("beep.wav", include_bytes!("../defaults/sounds/beep.wav")),
    // ("death.ogg", include_bytes!("../defaults/sounds/death.ogg")),
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
