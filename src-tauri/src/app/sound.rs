//! Key sounds: the original synthesized packs from `keystage_core::synth`,
//! played through rodio on a thread of their own (the audio stream is not
//! `Send` on every platform). Off by default; while off no audio device is
//! opened at all. The pipeline asks for a sound on displayed key-down
//! messages only, which never exist while suppressed (password field,
//! excluded app, pause), so suppressed input never makes a sound.
//!
//! Playing never blocks the caller: requests go through a small bounded
//! queue and are dropped when it is full.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use crossbeam_channel::{bounded, Receiver, RecvTimeoutError, Sender, TrySendError};
use keystage_core::settings::SoundSettings;
use keystage_core::synth::{self, SoundKind, VARIANTS};
use parking_lot::Mutex;

/// Requests queued at most; more are dropped (a burst of key downs).
pub const QUEUE: usize = 64;
/// Packs kept rendered: the active one plus a few previews.
pub const BANK_CACHE: usize = 4;

/// One pack rendered: every kind, every variant.
#[derive(Clone, Debug)]
pub struct Bank {
    pub pack: String,
    sounds: HashMap<(SoundKind, u32), Arc<Vec<f32>>>,
}

impl Bank {
    /// Renders a pack. `None` for an unknown pack id.
    pub fn render(pack: &str) -> Option<Bank> {
        synth::pack(pack)?;
        let mut sounds = HashMap::new();
        for kind in SoundKind::ALL {
            for v in 0..VARIANTS {
                let s = synth::render(pack, kind, v)?;
                sounds.insert((kind, v), Arc::new(s));
            }
        }
        Some(Bank {
            pack: pack.to_string(),
            sounds,
        })
    }

    pub fn get(&self, kind: SoundKind, variant: u32) -> Option<&Arc<Vec<f32>>> {
        self.sounds.get(&(kind, variant % VARIANTS))
    }

    pub fn len(&self) -> usize {
        self.sounds.len()
    }

    pub fn is_empty(&self) -> bool {
        self.sounds.is_empty()
    }
}

/// The preview: a key down and up, then a wide key down and up.
pub const PREVIEW: [(SoundKind, u64); 4] = [
    (SoundKind::Down, 0),
    (SoundKind::Up, 95),
    (SoundKind::WideDown, 300),
    (SoundKind::WideUp, 410),
];

/// How long the device stays open after a preview while sounds are off.
pub const PREVIEW_HOLD: Duration = Duration::from_millis(1200);

/// A perceptual volume curve: the settings slider is linear, loudness is not.
pub fn gain(volume: f32) -> f32 {
    let v = if volume.is_finite() {
        volume.clamp(0.0, 1.0)
    } else {
        0.0
    };
    v * v
}

#[derive(Debug)]
enum Cmd {
    Configure {
        enabled: bool,
        pack: String,
        volume: f32,
    },
    Key(SoundKind),
    Preview(String),
    Shutdown,
}

/// The app side of the sound thread.
#[derive(Debug)]
pub struct SoundHandle {
    tx: Sender<Cmd>,
    enabled: Arc<AtomicBool>,
    error: Arc<Mutex<Option<String>>>,
    thread: Mutex<Option<thread::JoinHandle<()>>>,
}

impl SoundHandle {
    /// Starts the sound thread (no device is opened until a sound is due).
    pub fn spawn() -> std::io::Result<SoundHandle> {
        let (tx, rx) = bounded(QUEUE);
        let error = Arc::new(Mutex::new(None));
        let err2 = error.clone();
        let thread = thread::Builder::new()
            .name("keystage-sound".into())
            .spawn(move || run(rx, err2))?;
        Ok(SoundHandle {
            tx,
            enabled: Arc::new(AtomicBool::new(false)),
            error,
            thread: Mutex::new(Some(thread)),
        })
    }

    /// Applies the sound settings.
    pub fn configure(&self, s: &SoundSettings) {
        self.enabled.store(s.enabled, Ordering::Release);
        if !s.enabled {
            *self.error.lock() = None;
        }
        // Configuration must not be dropped: block briefly if the queue is full.
        let _ = self.tx.send_timeout(
            Cmd::Configure {
                enabled: s.enabled,
                pack: s.pack.clone(),
                volume: s.volume,
            },
            Duration::from_millis(200),
        );
    }

    pub fn enabled(&self) -> bool {
        self.enabled.load(Ordering::Acquire)
    }

    /// A displayed key went down. Never blocks.
    pub fn key_down(&self, key: &str) {
        if !self.enabled() {
            return;
        }
        match self.tx.try_send(Cmd::Key(SoundKind::for_key(key, true))) {
            Ok(()) | Err(TrySendError::Full(_)) | Err(TrySendError::Disconnected(_)) => {}
        }
    }

    /// Plays the preview of a pack (also while sounds are off).
    pub fn preview(&self, pack: &str) -> Result<(), String> {
        if synth::pack(pack).is_none() {
            return Err("That sound pack does not exist.".into());
        }
        self.tx
            .try_send(Cmd::Preview(pack.to_string()))
            .map_err(|_| "The sound player is busy. Try again.".to_string())
    }

    /// Why sounds cannot play (no output device), while sounds are on.
    pub fn error(&self) -> Option<String> {
        self.error.lock().clone()
    }

    /// Stops the thread and releases the device.
    pub fn shutdown(&self) {
        let _ = self
            .tx
            .send_timeout(Cmd::Shutdown, Duration::from_millis(500));
        if let Some(t) = self.thread.lock().take() {
            let _ = t.join();
        }
    }
}

/// Picks the next take of each sound so fast typing does not repeat one.
#[derive(Debug, Default)]
pub struct Variants(AtomicU32);

impl Variants {
    pub fn next(&self) -> u32 {
        self.0.fetch_add(1, Ordering::Relaxed) % VARIANTS
    }
}

struct Player {
    sink: Option<rodio::MixerDeviceSink>,
    enabled: bool,
    pack: String,
    gain: f32,
    banks: Vec<Arc<Bank>>,
    variants: Variants,
    error: Arc<Mutex<Option<String>>>,
}

impl Player {
    fn bank(&mut self, pack: &str) -> Option<Arc<Bank>> {
        if let Some(i) = self.banks.iter().position(|b| b.pack == pack) {
            let b = self.banks.remove(i);
            self.banks.insert(0, b.clone());
            return Some(b);
        }
        let b = Arc::new(Bank::render(pack)?);
        self.banks.insert(0, b.clone());
        self.banks.truncate(BANK_CACHE);
        Some(b)
    }

    fn ensure_sink(&mut self) -> bool {
        if self.sink.is_some() {
            return true;
        }
        match rodio::DeviceSinkBuilder::open_default_sink() {
            Ok(mut sink) => {
                sink.log_on_drop(false);
                self.sink = Some(sink);
                *self.error.lock() = None;
                tracing::info!("sound output opened");
                true
            }
            Err(e) => {
                tracing::warn!(error = %e, "no sound output");
                *self.error.lock() =
                    Some("Key sounds are on, but no sound output device could be opened.".into());
                false
            }
        }
    }

    fn play(&mut self, pack: &str, kind: SoundKind, delay_ms: u64) {
        let Some(bank) = self.bank(pack) else {
            return;
        };
        let variant = self.variants.next();
        let Some(samples) = bank.get(kind, variant).cloned() else {
            return;
        };
        if !self.ensure_sink() {
            return;
        }
        let gain = self.gain;
        if let Some(sink) = &self.sink {
            use rodio::Source;
            let (Some(ch), Some(rate)) = (
                std::num::NonZero::new(1u16),
                std::num::NonZero::new(synth::SAMPLE_RATE),
            ) else {
                return;
            };
            let src = rodio::buffer::SamplesBuffer::new(ch, rate, samples.as_slice().to_vec())
                .amplify(gain)
                .delay(Duration::from_millis(delay_ms));
            sink.mixer().add(src);
        }
    }
}

fn run(rx: Receiver<Cmd>, error: Arc<Mutex<Option<String>>>) {
    let mut p = Player {
        sink: None,
        enabled: false,
        pack: synth::DEFAULT_PACK.to_string(),
        gain: gain(0.6),
        banks: Vec::new(),
        variants: Variants::default(),
        error,
    };
    // While sounds are off, a preview opens the device and this closes it
    // again once the preview has played.
    let mut close_at: Option<Instant> = None;
    loop {
        let cmd = match close_at {
            Some(t) => match rx.recv_timeout(t.saturating_duration_since(Instant::now())) {
                Ok(c) => c,
                Err(RecvTimeoutError::Timeout) => {
                    close_at = None;
                    if !p.enabled {
                        p.sink = None;
                    }
                    continue;
                }
                Err(RecvTimeoutError::Disconnected) => break,
            },
            None => match rx.recv() {
                Ok(c) => c,
                Err(_) => break,
            },
        };
        match cmd {
            Cmd::Configure {
                enabled,
                pack,
                volume,
            } => {
                p.enabled = enabled;
                p.gain = gain(volume);
                if synth::pack(&pack).is_some() {
                    p.pack = pack;
                }
                if enabled {
                    close_at = None;
                    let pack = p.pack.clone();
                    let _ = p.bank(&pack);
                    p.ensure_sink();
                } else if close_at.is_none() && p.sink.take().is_some() {
                    tracing::info!("sound output closed");
                }
            }
            Cmd::Key(kind) => {
                if p.enabled {
                    let pack = p.pack.clone();
                    p.play(&pack, kind, 0);
                }
            }
            Cmd::Preview(pack) => {
                for (kind, at) in PREVIEW {
                    p.play(&pack, kind, at);
                }
                if !p.enabled {
                    close_at = Some(Instant::now() + PREVIEW_HOLD);
                }
            }
            Cmd::Shutdown => break,
        }
    }
    tracing::info!("sound thread stopped");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_bank_holds_every_kind_and_variant() {
        let b = Bank::render(synth::DEFAULT_PACK).expect("default pack");
        assert_eq!(b.len(), 4 * VARIANTS as usize);
        for kind in SoundKind::ALL {
            for v in 0..VARIANTS {
                let s = b.get(kind, v).expect("sound");
                assert!(!s.is_empty());
                assert_eq!(s.first().copied(), Some(0.0));
            }
        }
        assert!(Arc::ptr_eq(
            b.get(SoundKind::Down, 0).expect("a"),
            b.get(SoundKind::Down, VARIANTS).expect("b")
        ));
        assert!(Bank::render("missing").is_none());
    }

    #[test]
    fn every_pack_renders() {
        for p in synth::PACKS {
            assert!(Bank::render(p.id).is_some(), "{}", p.id);
        }
    }

    #[test]
    fn gain_curve() {
        assert_eq!(gain(0.0), 0.0);
        assert_eq!(gain(1.0), 1.0);
        assert!((gain(0.5) - 0.25).abs() < 1e-6);
        assert_eq!(gain(3.0), 1.0);
        assert_eq!(gain(f32::NAN), 0.0);
    }

    #[test]
    fn variants_rotate() {
        let v = Variants::default();
        let seq: Vec<u32> = (0..VARIANTS * 2).map(|_| v.next()).collect();
        assert_eq!(seq[0], 0);
        assert_eq!(seq[VARIANTS as usize], 0);
        assert!(seq.iter().all(|x| *x < VARIANTS));
    }

    #[test]
    fn handle_is_quiet_when_off_and_rejects_unknown_packs() {
        let h = SoundHandle::spawn().expect("thread");
        assert!(!h.enabled());
        // Off: key downs are ignored before they reach the queue.
        for _ in 0..(QUEUE * 2) {
            h.key_down("A");
        }
        assert!(h.preview("missing").is_err());
        h.configure(&SoundSettings {
            enabled: false,
            pack: "clicky".into(),
            volume: 0.3,
        });
        assert!(h.error().is_none());
        h.shutdown();
        h.shutdown();
    }

    #[test]
    fn preview_order_is_down_then_up() {
        assert_eq!(PREVIEW[0].0, SoundKind::Down);
        assert!(PREVIEW.windows(2).all(|w| w[0].1 < w[1].1));
    }
}
