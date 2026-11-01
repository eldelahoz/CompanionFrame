//! Notification sound: the bundled chime or one custom clip, played with rodio on its own thread.
//!
//! The custom clip is accepted only in the bounded 24 kHz mono PCM WAV shape the picker
//! produces (never arbitrary media or paths) and is stored as `notification-sound.json`.

use std::io::{self, Cursor};
use std::path::PathBuf;
use std::sync::mpsc::{self, Sender};
use std::sync::{Arc, Mutex};

use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::services::atomic::write_atomic;

pub const MAX_SECONDS: usize = 30;
pub const SAMPLE_RATE: u32 = 24_000;
pub const MAX_WAV_BYTES: usize = 44 + SAMPLE_RATE as usize * MAX_SECONDS * 2;
const MAX_NAME_CHARS: usize = 120;
const INVALID: &str = "Invalid notification sound";

static BUNDLED: &[u8] = include_bytes!("../../../assets/notification.wav");

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, specta::Type)]
pub struct SoundUpload {
    pub name: String,
    /// Base64 of the WAV bytes.
    pub data: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct SoundAsset {
    pub name: String,
    pub data_url: String,
    /// SHA-256 of the base64 payload; changes whenever the clip does.
    pub revision: String,
}

#[derive(Clone)]
struct Stored {
    upload: SoundUpload,
    bytes: Arc<Vec<u8>>,
}

impl Stored {
    fn asset(&self) -> SoundAsset {
        SoundAsset {
            name: self.upload.name.clone(),
            data_url: format!("data:audio/wav;base64,{}", self.upload.data),
            revision: format!("{:x}", Sha256::digest(self.upload.data.as_bytes())),
        }
    }
}

fn u16_at(b: &[u8], at: usize) -> u16 {
    u16::from_le_bytes([b[at], b[at + 1]])
}

fn u32_at(b: &[u8], at: usize) -> u32 {
    u32::from_le_bytes([b[at], b[at + 1], b[at + 2], b[at + 3]])
}

/// Checks the base64 alphabet and the encoded-length bound, then decodes canonically.
fn decode_bounded(data: &str) -> Option<Vec<u8>> {
    let body = data.trim_end_matches('=');
    let padding = data.len() - body.len();
    let alphabet = !body.is_empty()
        && padding <= 2
        && body
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || c == b'+' || c == b'/');
    if !alphabet || data.len() > MAX_WAV_BYTES.div_ceil(3) * 4 {
        return None;
    }
    let bytes = STANDARD.decode(data).ok()?;
    // Canonical only: re-encoding must give back exactly what was sent.
    (STANDARD.encode(&bytes) == data).then_some(bytes)
}

fn validate(raw: &SoundUpload) -> Result<Stored, String> {
    let name = raw.name.trim();
    let bad_name = name.is_empty()
        || name.chars().count() > MAX_NAME_CHARS
        || raw.name.chars().count() > MAX_NAME_CHARS
        || name.contains(['/', '\\'])
        || name.chars().any(|c| (c as u32) < 32 || c as u32 == 127);
    let bytes = if bad_name {
        None
    } else {
        decode_bounded(&raw.data)
    };
    let Some(b) = bytes else {
        return Err(INVALID.into());
    };
    let pcm = b.len() >= 46
        && b.len() <= MAX_WAV_BYTES
        && b.len() % 2 == 0
        && &b[0..4] == b"RIFF"
        && u32_at(&b, 4) as usize == b.len() - 8
        && &b[8..16] == b"WAVEfmt "
        && u32_at(&b, 16) == 16
        && u16_at(&b, 20) == 1
        && u16_at(&b, 22) == 1
        && u32_at(&b, 24) == SAMPLE_RATE
        && u32_at(&b, 28) == SAMPLE_RATE * 2
        && u16_at(&b, 32) == 2
        && u16_at(&b, 34) == 16
        && &b[36..40] == b"data"
        && u32_at(&b, 40) as usize == b.len() - 44;
    if !pcm {
        return Err(INVALID.into());
    }
    Ok(Stored {
        upload: SoundUpload {
            name: name.to_string(),
            data: raw.data.clone(),
        },
        bytes: Arc::new(b),
    })
}

/// The custom clip on disk plus its in-memory copy. `None` outer = not read yet.
pub struct SoundStore {
    path: PathBuf,
    cached: Mutex<Option<Option<Stored>>>,
}

impl SoundStore {
    pub fn new(path: PathBuf) -> Self {
        Self {
            path,
            cached: Mutex::new(None),
        }
    }

    fn load(&self) -> Option<Stored> {
        let mut cached = self.cached.lock().unwrap();
        if let Some(settled) = cached.as_ref() {
            return settled.clone();
        }
        let too_big = std::fs::metadata(&self.path)
            .map(|m| m.len() as usize > MAX_WAV_BYTES.div_ceil(3) * 4 + 1024)
            .unwrap_or(false);
        let read = if too_big {
            Ok(None)
        } else {
            std::fs::read(&self.path).map(Some)
        };
        let text = match read {
            Ok(text) => text,
            Err(e) if e.kind() == io::ErrorKind::NotFound => None,
            // A locked file is retried on the next notification.
            Err(_) => return None,
        };
        let stored = text
            .and_then(|t| serde_json::from_slice::<SoundUpload>(&t).ok())
            .and_then(|u| validate(&u).ok());
        *cached = Some(stored.clone());
        stored
    }

    pub fn get(&self) -> Option<SoundAsset> {
        self.load().map(|s| s.asset())
    }

    /// The clip to play: the custom one, else the bundled chime.
    pub fn bytes(&self) -> Arc<Vec<u8>> {
        self.load()
            .map(|s| s.bytes)
            .unwrap_or_else(|| Arc::new(BUNDLED.to_vec()))
    }

    /// On a failed write the previous clip stays in place.
    pub fn save(&self, raw: &SoundUpload) -> Result<SoundAsset, String> {
        let stored = validate(raw)?;
        let json = serde_json::to_vec(&stored.upload).map_err(|e| e.to_string())?;
        write_atomic(&self.path, &json).map_err(|e| e.to_string())?;
        let asset = stored.asset();
        *self.cached.lock().unwrap() = Some(Some(stored));
        Ok(asset)
    }

    pub fn reset(&self) {
        let _ = std::fs::remove_file(&self.path);
        *self.cached.lock().unwrap() = Some(None);
    }
}

struct Play {
    bytes: Arc<Vec<u8>>,
    volume: f32,
}

/// Owns the audio thread; the output device opens per clip, so an unplugged device only costs
/// the next sound.
pub struct Player {
    tx: Sender<Play>,
}

impl Player {
    pub fn spawn() -> Self {
        let (tx, rx) = mpsc::channel::<Play>();
        std::thread::Builder::new()
            .name("notification-sound".into())
            .spawn(move || {
                while let Ok(mut next) = rx.recv() {
                    // A burst of notifications plays the newest only.
                    while let Ok(newer) = rx.try_recv() {
                        next = newer;
                    }
                    if let Err(e) = play_blocking(next) {
                        tracing::warn!("notification sound failed: {e}");
                    }
                }
            })
            .expect("spawn notification sound thread");
        Self { tx }
    }

    pub fn play(&self, bytes: Arc<Vec<u8>>, volume: f32) {
        let _ = self.tx.send(Play { bytes, volume });
    }
}

fn play_blocking(play: Play) -> Result<(), String> {
    let sink = rodio::DeviceSinkBuilder::open_default_sink().map_err(|e| e.to_string())?;
    let player = rodio::play(sink.mixer(), Cursor::new(play.bytes.as_ref().clone()))
        .map_err(|e| e.to_string())?;
    player.set_volume(play.volume);
    player.sleep_until_end();
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wav(size: usize) -> Vec<u8> {
        let samples = (size - 44) / 2;
        let mut b = Vec::with_capacity(size);
        b.extend(b"RIFF");
        b.extend(((size - 8) as u32).to_le_bytes());
        b.extend(b"WAVEfmt ");
        b.extend(16u32.to_le_bytes());
        b.extend(1u16.to_le_bytes());
        b.extend(1u16.to_le_bytes());
        b.extend(SAMPLE_RATE.to_le_bytes());
        b.extend((SAMPLE_RATE * 2).to_le_bytes());
        b.extend(2u16.to_le_bytes());
        b.extend(16u16.to_le_bytes());
        b.extend(b"data");
        b.extend(((samples * 2) as u32).to_le_bytes());
        b.resize(size, 0);
        b
    }

    fn upload_of(bytes: &[u8]) -> SoundUpload {
        SoundUpload {
            name: "My chime.wav".into(),
            data: STANDARD.encode(bytes),
        }
    }

    fn upload() -> SoundUpload {
        upload_of(&wav(48))
    }

    fn store() -> (SoundStore, PathBuf) {
        let dir = std::env::temp_dir().join(format!("cf-sound-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("notification-sound.json");
        (SoundStore::new(path.clone()), path)
    }

    #[test]
    fn persists_canonical_pcm_and_restores_it_after_a_fresh_load() {
        let (s, path) = store();
        let saved = s
            .save(&SoundUpload {
                name: " My chime.wav ".into(),
                ..upload()
            })
            .unwrap();
        assert_eq!(saved.name, "My chime.wav");
        assert_eq!(
            saved.data_url,
            format!("data:audio/wav;base64,{}", upload().data)
        );
        assert_eq!(saved.revision.len(), 64);
        assert!(saved.revision.bytes().all(|c| c.is_ascii_hexdigit()));
        let on_disk: SoundUpload = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        assert_eq!(on_disk, upload());
        assert_eq!(SoundStore::new(path).get(), Some(saved));
    }

    #[test]
    fn rejects_an_invalid_upload_without_replacing_the_saved_clip() {
        let (s, path) = store();
        let saved = s.save(&upload()).unwrap();
        let persisted = std::fs::read(&path).unwrap();
        let too_long = upload_of(&wav(MAX_WAV_BYTES + 2));
        let invalid = [
            SoundUpload {
                name: "../chime.wav".into(),
                ..upload()
            },
            SoundUpload {
                name: "C:\\chime.wav".into(),
                ..upload()
            },
            SoundUpload {
                name: "bad\nname.wav".into(),
                ..upload()
            },
            SoundUpload {
                name: " ".into(),
                ..upload()
            },
            SoundUpload {
                name: "a".repeat(121),
                ..upload()
            },
            SoundUpload {
                data: "file:///sound.wav".into(),
                ..upload()
            },
            SoundUpload {
                data: "bm90IGEgd2F2".into(),
                ..upload()
            },
            SoundUpload {
                data: format!("{}\n", upload().data),
                ..upload()
            },
            SoundUpload {
                data: String::new(),
                ..upload()
            },
            too_long,
        ];
        for bad in invalid {
            assert_eq!(s.save(&bad).unwrap_err(), INVALID, "{:?}", bad.name);
            assert_eq!(s.get(), Some(saved.clone()));
            assert_eq!(std::fs::read(&path).unwrap(), persisted);
        }
    }

    #[test]
    fn rejects_a_mismatched_pcm_header_at_every_checked_byte() {
        for offset in [4, 16, 20, 22, 24, 28, 32, 34, 40] {
            let (s, _) = store();
            let mut bytes = wav(48);
            bytes[offset] ^= 1;
            assert_eq!(
                s.save(&upload_of(&bytes)).unwrap_err(),
                INVALID,
                "byte {offset}"
            );
            assert_eq!(s.get(), None);
        }
    }

    #[test]
    fn accepts_the_maximum_duration_and_changes_revision_when_samples_change() {
        let (s, _) = store();
        let previous = s.save(&upload()).unwrap();
        let mut bytes = wav(MAX_WAV_BYTES);
        bytes[44..46].copy_from_slice(&150i16.to_le_bytes());
        let saved = s.save(&upload_of(&bytes)).unwrap();
        assert_ne!(saved.revision, previous.revision);
    }

    #[test]
    fn retains_the_old_clip_when_persistence_fails() {
        let (s, path) = store();
        let previous = s.save(&upload()).unwrap();
        let persisted = std::fs::read(&path).unwrap();
        // A directory where the file belongs makes the atomic rename fail.
        let blocked = SoundStore::new(path.parent().unwrap().to_path_buf());
        assert!(blocked.save(&upload()).is_err());
        assert_eq!(s.get(), Some(previous));
        assert_eq!(std::fs::read(&path).unwrap(), persisted);
    }

    #[test]
    fn resets_to_the_bundled_chime_and_stays_empty_after_reload() {
        let (s, path) = store();
        s.save(&upload()).unwrap();
        s.reset();
        assert_eq!(s.get(), None);
        assert!(!path.exists());
        assert_eq!(*s.bytes(), BUNDLED);
        assert_eq!(SoundStore::new(path).get(), None);
    }

    #[test]
    fn falls_back_to_the_bundled_sound_for_missing_corrupt_invalid_or_oversized_storage() {
        let cases: [Option<Vec<u8>>; 4] = [
            None,
            Some(b"{".to_vec()),
            Some(
                serde_json::to_vec(&SoundUpload {
                    data: "bad".into(),
                    ..upload()
                })
                .unwrap(),
            ),
            Some(vec![b' '; MAX_WAV_BYTES * 2]),
        ];
        for case in cases {
            let (s, path) = store();
            if let Some(bytes) = case {
                std::fs::write(&path, bytes).unwrap();
            }
            assert_eq!(s.get(), None);
            assert_eq!(*s.bytes(), BUNDLED);
        }
    }

    #[test]
    fn a_missing_file_settles_but_an_unreadable_one_is_retried() {
        let (s, path) = store();
        assert_eq!(s.get(), None);
        std::fs::write(&path, serde_json::to_vec(&upload()).unwrap()).unwrap();
        assert_eq!(s.get(), None, "a settled miss is not re-read");

        // A directory in place of the file reads as an error other than NotFound.
        let (s, path) = store();
        std::fs::create_dir(&path).unwrap();
        assert_eq!(s.get(), None);
        std::fs::remove_dir(&path).unwrap();
        std::fs::write(&path, serde_json::to_vec(&upload()).unwrap()).unwrap();
        assert_eq!(s.get().unwrap().name, "My chime.wav");
    }

    #[test]
    fn the_bundled_chime_decodes() {
        assert!(rodio::Decoder::try_from(Cursor::new(BUNDLED.to_vec())).is_ok());
    }
}
