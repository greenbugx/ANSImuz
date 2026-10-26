use std::{
    f32::consts::PI,
    io::{Read, ErrorKind},
    process::{Child, Command, Stdio},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, RwLock,
    },
    thread,
};

pub const BAND_COUNT: usize = 32;

const SAMPLE_RATE: f32 = 44_100.0;
const WINDOW_SIZE: usize = 2_048;
const HOP_SIZE: usize = 512;
const MIN_FREQUENCY: f32 = 45.0;
const MAX_FREQUENCY: f32 = 16_000.0;
const BYTES_PER_STEREO_FRAME: usize = 4;

/// Captures the default output monitor through PulseAudio's compatibility interface.
///
/// On PipeWire systems this works when pipewire-pulse and pulseaudio-utils are installed.
/// The player itself remains unchanged; capture failure simply disables the live spectrum.
pub struct AudioVisualizer {
    child: Option<Child>,
    bands: Arc<RwLock<[u8; BAND_COUNT]>>,
    active: Arc<AtomicBool>,
    attempted: bool,
}

impl AudioVisualizer {
    pub fn new() -> Self {
        Self {
            child: None,
            bands: Arc::new(RwLock::new([0; BAND_COUNT])),
            active: Arc::new(AtomicBool::new(false)),
            attempted: false,
        }
    }

    pub fn has_attempted(&self) -> bool {
        self.attempted
    }

    pub fn is_active(&self) -> bool {
        self.active.load(Ordering::Acquire)
    }

    pub fn bands(&self) -> [u8; BAND_COUNT] {
        self.bands
            .read()
            .map(|bands| *bands)
            .unwrap_or([0; BAND_COUNT])
    }

    pub fn start(&mut self) -> Result<(), String> {
        if self.attempted {
            return Ok(());
        }
        self.attempted = true;

        let mut child = Command::new("parec")
            .args([
                "--device",
                "@DEFAULT_MONITOR@",
                "--raw",
                "--format=s16le",
                "--rate=44100",
                "--channels=2",
                "--client-name=ANSIMUZ",
                "--stream-name=ANSIMUZ Spectrum",
            ])
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|error| {
                format!(
                    "spectrum unavailable: install pulseaudio-utils (parec): {error}"
                )
            })?;

        let Some(stdout) = child.stdout.take() else {
            let _ = child.kill();
            let _ = child.wait();
            return Err("spectrum unavailable: could not read audio monitor".into());
        };

        let bands = Arc::clone(&self.bands);
        let active = Arc::clone(&self.active);
        active.store(true, Ordering::Release);

        if let Err(error) = thread::Builder::new()
            .name("ansimuz-spectrum".into())
            .spawn(move || capture_loop(stdout, bands, active))
        {
            self.active.store(false, Ordering::Release);
            let _ = child.kill();
            let _ = child.wait();
            return Err(format!("spectrum worker could not start: {error}"));
        }

        self.child = Some(child);
        Ok(())
    }

    pub fn stop(&mut self) {
        self.active.store(false, Ordering::Release);

        if let Some(mut child) = self.child.take() {
            let _ = child.kill();
            let _ = child.wait();
        }

        if let Ok(mut bands) = self.bands.write() {
            *bands = [0; BAND_COUNT];
        }

        self.attempted = false;
    }
}

impl Default for AudioVisualizer {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for AudioVisualizer {
    fn drop(&mut self) {
        self.stop();
    }
}

fn capture_loop(
    mut stdout: impl Read,
    bands: Arc<RwLock<[u8; BAND_COUNT]>>,
    active: Arc<AtomicBool>,
) {
    let frequencies = std::array::from_fn::<_, BAND_COUNT, _>(|band| {
        let ratio = MAX_FREQUENCY / MIN_FREQUENCY;
        let lower = MIN_FREQUENCY * ratio.powf(band as f32 / BAND_COUNT as f32);
        let upper = MIN_FREQUENCY * ratio.powf((band + 1) as f32 / BAND_COUNT as f32);
        (lower, (lower * upper).sqrt(), upper)
    });

    let window: Vec<f32> = (0..WINDOW_SIZE)
        .map(|index| {
            0.5 - 0.5 * (2.0 * PI * index as f32 / (WINDOW_SIZE - 1) as f32).cos()
        })
        .collect();

    // Goertzel probes three logarithmically spaced frequencies per band. That is enough
    // for a compact terminal spectrum without adding another runtime audio dependency.
    let mut buffer = vec![0_u8; HOP_SIZE * BYTES_PER_STEREO_FRAME];
    let mut ring = vec![0.0_f32; WINDOW_SIZE];
    let mut write_pos = 0;
    let mut filled = 0;
    let mut since_analysis = 0;
    let mut previous = [0.0_f32; BAND_COUNT];

    while active.load(Ordering::Acquire) {
        match stdout.read_exact(&mut buffer) {
            Ok(()) => {}
            Err(error) if error.kind() == ErrorKind::Interrupted => continue,
            Err(_) => break,
        }

        for frame in buffer.chunks_exact(BYTES_PER_STEREO_FRAME) {
            let left = i16::from_le_bytes([frame[0], frame[1]]) as f32;
            let right = i16::from_le_bytes([frame[2], frame[3]]) as f32;
            ring[write_pos] = (left + right) / 65_536.0;
            write_pos = (write_pos + 1) % WINDOW_SIZE;
            filled = (filled + 1).min(WINDOW_SIZE);
            since_analysis += 1;
        }

        if filled < WINDOW_SIZE || since_analysis < HOP_SIZE {
            continue;
        }
        since_analysis = 0;

        let samples: Vec<f32> = (0..WINDOW_SIZE)
            .map(|offset| ring[(write_pos + offset) % WINDOW_SIZE])
            .collect();

        let mut latest = [0_u8; BAND_COUNT];
        for (band, probes) in frequencies.iter().enumerate() {
            let amplitude = [
                goertzel_amplitude(&samples, &window, probes.0),
                goertzel_amplitude(&samples, &window, probes.1),
                goertzel_amplitude(&samples, &window, probes.2),
            ]
            .into_iter()
            .fold(0.0_f32, f32::max);

            let db = 20.0 * amplitude.max(0.000_001).log10();
            let target = if db < -55.0 {
                0.0
            } else {
                ((db + 55.0) / 55.0 * 100.0).clamp(0.0, 100.0)
            };

            // Fast attack with a slower release makes the bars follow kicks without
            // flickering down to zero between frames.
            previous[band] = if target > previous[band] {
                previous[band] + (target - previous[band]) * 0.68
            } else {
                previous[band] + (target - previous[band]) * 0.20
            };
            latest[band] = previous[band].round().clamp(0.0, 100.0) as u8;
        }

        if let Ok(mut shared) = bands.write() {
            *shared = latest;
        }
    }

    active.store(false, Ordering::Release);
    if let Ok(mut shared) = bands.write() {
        *shared = [0; BAND_COUNT];
    }
}

fn goertzel_amplitude(samples: &[f32], window: &[f32], frequency: f32) -> f32 {
    let omega = 2.0 * PI * frequency / SAMPLE_RATE;
    let coefficient = 2.0 * omega.cos();
    let mut previous = 0.0_f32;
    let mut previous2 = 0.0_f32;

    for (sample, window_value) in samples.iter().zip(window.iter()) {
        let current = sample * window_value + coefficient * previous - previous2;
        previous2 = previous;
        previous = current;
    }

    let power = (previous * previous + previous2 * previous2
        - coefficient * previous * previous2)
        .max(0.0);

    (power.sqrt() * 2.0 / samples.len() as f32).clamp(0.0, 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn silence_has_no_spectrum() {
        let samples = vec![0.0_f32; WINDOW_SIZE];
        let window = vec![1.0_f32; WINDOW_SIZE];
        assert_eq!(goertzel_amplitude(&samples, &window, 440.0), 0.0);
    }

    #[test]
    fn detects_a_tone_near_the_probe_frequency() {
        let samples: Vec<f32> = (0..WINDOW_SIZE)
            .map(|index| {
                (2.0 * PI * 440.0 * index as f32 / SAMPLE_RATE).sin() * 0.5
            })
            .collect();
        let window = vec![1.0_f32; WINDOW_SIZE];
        assert!(goertzel_amplitude(&samples, &window, 440.0) > 0.2);
    }
}
