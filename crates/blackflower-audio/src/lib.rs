//! Client-side spatial audio for plugin-published game events.
//!
//! The plugin publishes events carrying an opaque sound id and a world
//! position; the client plays each as a one-shot on a per-event spatial track,
//! panned and attenuated relative to a listener placed at the local player's
//! camera. Sounds are loaded by id (file stem) from a directory, so adding a
//! sound is just dropping a file in — no recompile of this engine. All failures
//! (no audio device, missing files) are non-fatal — the game runs silently.

use anyhow::Context;
use blackflower_math::{Quat, Vec3};
use kira::{
    AudioManager, AudioManagerSettings, DefaultBackend, Tween, listener::ListenerHandle,
    sound::static_sound::StaticSoundData, track::SpatialTrackBuilder,
};
use tracing::{info, warn};

/// Directory scanned for `*.wav` sounds, keyed by file stem (the id the plugin
/// references).
const SOUNDS_DIR: &str = "assets/sounds";

/// Owns the kira audio manager, a single listener, and the loaded sounds. Lives
/// on the client tick thread (which sees both the events and the local player's
/// transform).
pub struct AudioEngine {
    manager: AudioManager,
    listener: ListenerHandle,
    /// `(id, data)` pairs loaded from [`SOUNDS_DIR`]; looked up by id. A short
    /// linear scan — there are only a handful of sounds.
    sounds: Vec<(String, StaticSoundData)>,
}

impl AudioEngine {
    /// Initialize the audio backend, place a listener at the origin, and load
    /// every `*.wav` in the sounds directory. Returns an error only if the audio
    /// device itself can't be opened; a missing/unreadable directory just yields
    /// no sounds (logged).
    pub fn new() -> anyhow::Result<Self> {
        let mut manager = AudioManager::<DefaultBackend>::new(AudioManagerSettings::default())
            .context("opening audio device")?;
        let listener = manager
            .add_listener(Vec3::ZERO, Quat::IDENTITY)
            .context("adding audio listener")?;
        let sounds = load_sounds(SOUNDS_DIR);
        Ok(Self {
            manager,
            listener,
            sounds,
        })
    }

    /// Move the listener to the local player's camera each tick: `position` is
    /// the eye, `orientation` the facing (unrotated = looking down -Z).
    pub fn set_listener(&mut self, position: Vec3, orientation: Quat) {
        self.listener.set_position(position, Tween::default());
        self.listener.set_orientation(orientation, Tween::default());
    }

    /// Play the sound with id `sound` at a world `position`. Unknown ids are a
    /// no-op. Spawns a transient spatial track that outlives this handle until
    /// the sound finishes.
    pub fn play(&mut self, sound: &str, position: Vec3) {
        let Some((_, data)) = self.sounds.iter().find(|(id, _)| id == sound) else {
            return;
        };
        let data = data.clone();
        let builder = SpatialTrackBuilder::new().persist_until_sounds_finish(true);
        let mut track = match self
            .manager
            .add_spatial_sub_track(&self.listener, position, builder)
        {
            Ok(track) => track,
            Err(error) => {
                warn!(%error, "audio: spatial track limit reached; dropping sound");
                return;
            }
        };
        if let Err(error) = track.play(data) {
            warn!(%error, "audio: failed to play sound");
        }
    }
}

/// Load every `*.wav` in `dir` as `(file_stem, data)`. Missing dir or unreadable
/// files are logged and skipped.
fn load_sounds(dir: &str) -> Vec<(String, StaticSoundData)> {
    let mut sounds = Vec::new();
    let entries = match std::fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(error) => {
            warn!(dir, %error, "audio: sounds directory unavailable; running silent");
            return sounds;
        }
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().is_none_or(|ext| ext != "wav") {
            continue;
        }
        let Some(id) = path.file_stem().and_then(|s| s.to_str()).map(str::to_owned) else {
            continue;
        };
        match StaticSoundData::from_file(&path) {
            Ok(data) => {
                info!(id, "audio: loaded sound");
                sounds.push((id, data));
            }
            Err(error) => warn!(path = %path.display(), %error, "audio: failed to load sound"),
        }
    }
    sounds
}
