#![cfg(feature = "native-mpv")]

use std::fs;
use std::path::Path;
use std::time::{Duration, Instant};

use ansimuz::playback::native_mpv::{NativeEndReason, NativeEventData, NativeEventType, NativeMpv};

// Writes a valid mono 16-bit 44.1 kHz PCM WAV file of silence
fn write_silence_wav(path: &Path, secs: u32) {
    let rate: u32 = 44_100;
    let data_len = rate * secs * 2;
    let mut b = Vec::with_capacity(44 + data_len as usize);
    b.extend_from_slice(b"RIFF");
    b.extend_from_slice(&(36 + data_len).to_le_bytes());
    b.extend_from_slice(b"WAVEfmt ");
    b.extend_from_slice(&16u32.to_le_bytes());
    b.extend_from_slice(&1u16.to_le_bytes());
    b.extend_from_slice(&1u16.to_le_bytes());
    b.extend_from_slice(&rate.to_le_bytes());
    b.extend_from_slice(&(rate * 2).to_le_bytes());
    b.extend_from_slice(&2u16.to_le_bytes());
    b.extend_from_slice(&16u16.to_le_bytes());
    b.extend_from_slice(b"data");
    b.extend_from_slice(&data_len.to_le_bytes());
    b.resize(44 + data_len as usize, 0);
    fs::write(path, b).expect("write silence wav");
}

fn new_player() -> Option<NativeMpv> {
    if std::env::var_os("ANSIMUZ_MPV_AO").is_none() {
        unsafe {
            std::env::set_var("ANSIMUZ_MPV_AO", "null");
        }
    }

    match NativeMpv::new() {
        Ok(m) => Some(m),
        Err(e) if std::env::var_os("ANSIMUZ_REQUIRE_MPV").is_none() => {
            eprintln!("skipping native mpv test: {e}");
            None
        }
        Err(e) => panic!("libmpv required but unavailable: {e}"),
    }
}

#[test]
fn plays_a_short_file_to_eof() {
    let dir = std::env::temp_dir().join("ansimuz-native-smoke");
    let _ = fs::create_dir_all(&dir);
    let wav = dir.join("silence_1s.wav");
    write_silence_wav(&wav, 1);

    let Some(mut mpv) = new_player() else { return };
    let id = mpv.loadfile(wav.to_str().expect("valid path")).expect("loadfile");

    let deadline = Instant::now() + Duration::from_secs(10);
    let (mut loaded, mut ended) = (false, false);
    while Instant::now() < deadline && !ended {
        while let Ok(Some(ev)) = mpv.poll_event() {
            if ev.load_id != id {
                continue;
            }
            match ev.event_type {
                NativeEventType::FileLoaded => loaded = true,
                NativeEventType::EndFile => {
                    assert_eq!(ev.end_reason, NativeEndReason::Eof);
                    ended = true;
                }
                _ => {}
            }
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    assert!(loaded, "never saw FileLoaded");
    assert!(ended, "never saw EndFile");
}

#[test]
fn rapid_replace_does_not_emit_eof_for_first_track() {
    let dir = std::env::temp_dir().join("ansimuz-native-smoke");
    let _ = fs::create_dir_all(&dir);
    let wav_a = dir.join("silence_5s.wav");
    let wav_b = dir.join("silence_1s_b.wav");
    write_silence_wav(&wav_a, 5);
    write_silence_wav(&wav_b, 1);

    let Some(mut mpv) = new_player() else { return };
    let first = mpv.loadfile(wav_a.to_str().expect("valid path")).expect("loadfile a");
    let second = mpv.loadfile(wav_b.to_str().expect("valid path")).expect("loadfile b");

    let mut events: Vec<NativeEventData> = Vec::new();
    let deadline = Instant::now() + Duration::from_secs(10);
    let mut second_ended = false;

    while Instant::now() < deadline && !second_ended {
        while let Ok(Some(ev)) = mpv.poll_event() {
            if ev.load_id == second
                && ev.event_type == NativeEventType::EndFile
                && ev.end_reason == NativeEndReason::Eof
            {
                second_ended = true;
            }
            events.push(ev);
        }
        std::thread::sleep(Duration::from_millis(20));
    }

    assert!(second_ended, "second track never reached EOF");
    assert!(
        !events.iter().any(|e| e.load_id == first
            && e.event_type == NativeEventType::EndFile
            && e.end_reason == NativeEndReason::Eof),
        "first track incorrectly reported EOF after being replaced"
    );
}

