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
    let id = mpv
        .loadfile(wav.to_str().expect("valid path"))
        .expect("loadfile");

    let deadline = Instant::now() + Duration::from_secs(10);
    let (mut loaded, mut ended, mut saw_pos) = (false, false, false);
    while Instant::now() < deadline && !ended {
        while let Ok(Some(ev)) = mpv.poll_event() {
            if ev.load_id != id {
                continue;
            }
            match ev.event_type {
                NativeEventType::FileLoaded => loaded = true,
                NativeEventType::PropertyChange
                    if ev.property == ansimuz::playback::native_mpv::NativeProperty::TimePos =>
                {
                    saw_pos = true;
                }
                NativeEventType::EndFile => {
                    assert_eq!(
                        ev.end_reason,
                        NativeEndReason::Eof,
                        "EndFile error code: {}",
                        ev.error_code
                    );
                    ended = true;
                }
                _ => {}
            }
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    assert!(loaded, "never saw FileLoaded");
    assert!(saw_pos, "never saw TimePos property change");
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
    let first = mpv
        .loadfile(wav_a.to_str().expect("valid path"))
        .expect("loadfile a");
    let second = mpv
        .loadfile(wav_b.to_str().expect("valid path"))
        .expect("loadfile b");

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

    assert!(
        second_ended,
        "second track never reached EOF (events: {:?})",
        events
            .iter()
            .filter(|e| e.load_id == second)
            .map(|e| (e.event_type, e.end_reason, e.error_code))
            .collect::<Vec<_>>()
    );
    assert!(
        !events.iter().any(|e| e.load_id == first
            && e.event_type == NativeEventType::EndFile
            && e.end_reason == NativeEndReason::Eof),
        "first track incorrectly reported EOF after being replaced"
    );
}

#[test]
fn native_dump_stream_dumps_file() {
    let dir = std::env::temp_dir().join("ansimuz-native-smoke-dump");
    let _ = fs::create_dir_all(&dir);
    let wav_src = dir.join("src_silence.wav");
    let wav_dst = dir.join("dst_silence.wav");
    write_silence_wav(&wav_src, 1);
    if wav_dst.exists() {
        let _ = fs::remove_file(&wav_dst);
    }

    if std::env::var_os("ANSIMUZ_MPV_AO").is_none() {
        unsafe {
            std::env::set_var("ANSIMUZ_MPV_AO", "null");
        }
    }

    let res = NativeMpv::dump_stream(wav_src.to_str().expect("valid src"), &wav_dst, None);
    assert!(res.is_ok(), "dump_stream failed: {res:?}");
    assert!(wav_dst.exists(), "dump destination file does not exist");
    let meta = fs::metadata(&wav_dst).expect("read metadata");
    assert!(meta.len() > 0, "dump destination file is empty");
    let _ = fs::remove_file(&wav_src);
    let _ = fs::remove_file(&wav_dst);
}

#[test]
fn native_dump_stream_cancelled() {
    let dir = std::env::temp_dir().join("ansimuz-native-smoke-cancel");
    let _ = fs::create_dir_all(&dir);
    let wav_src = dir.join("src_long.wav");
    let wav_dst = dir.join("dst_long.wav");
    write_silence_wav(&wav_src, 5);
    if wav_dst.exists() {
        let _ = fs::remove_file(&wav_dst);
    }

    let cancel = std::sync::atomic::AtomicBool::new(true);
    let res = NativeMpv::dump_stream(
        wav_src.to_str().expect("valid src"),
        &wav_dst,
        Some(&cancel),
    );
    assert!(res.is_err(), "expected cancelled error");
    let _ = fs::remove_file(&wav_src);
    let _ = fs::remove_file(&wav_dst);
}

#[test]
fn play_while_paused_unpauses_replacement_track() {
    use ansimuz::playback::{NativeBackend, PlayerEvent};
    use std::sync::mpsc;

    if std::env::var_os("ANSIMUZ_MPV_AO").is_none() {
        unsafe {
            std::env::set_var("ANSIMUZ_MPV_AO", "null");
        }
    }

    let mut backend = match NativeBackend::new() {
        Ok(b) => b,
        Err(e) if std::env::var_os("ANSIMUZ_REQUIRE_MPV").is_none() => {
            eprintln!("skipping native backend pause test: {e}");
            return;
        }
        Err(e) => panic!("libmpv required but unavailable: {e}"),
    };

    let dir = std::env::temp_dir().join("ansimuz-native-smoke-unpause");
    let _ = fs::create_dir_all(&dir);
    let wav_a = dir.join("paused_a.wav");
    let wav_b = dir.join("replaced_b.wav");
    write_silence_wav(&wav_a, 5);
    write_silence_wav(&wav_b, 1);

    let (tx, rx) = mpsc::channel();
    backend
        .play(wav_a.to_str().expect("valid path"), 80, tx.clone())
        .expect("play first track");

    // Wait briefly so track A is active, then pause playback
    let start_deadline = Instant::now() + Duration::from_secs(3);
    while Instant::now() < start_deadline {
        let _ = backend.poll_events();
        if let Ok(PlayerEvent::Position { .. }) = rx.try_recv() {
            break;
        }
        std::thread::sleep(Duration::from_millis(20));
    }

    backend.toggle_pause().expect("pause track A");
    assert!(!backend.is_playing(), "backend should be paused");

    // Replace with track B while paused
    backend
        .play(wav_b.to_str().expect("valid path"), 80, tx.clone())
        .expect("play replacement track");
    assert!(
        backend.is_playing(),
        "replacement track must start unpaused"
    );

    // Toggling pause on replacement track must pause it immediately, not keep it unpaused
    backend.toggle_pause().expect("pause track B");
    assert!(
        !backend.is_playing(),
        "first toggle_pause after replacement must pause"
    );
    backend.toggle_pause().expect("resume track B");
    assert!(backend.is_playing(), "second toggle_pause must resume");

    // Track B must advance to natural EOF without stalling in a paused engine state
    let gen_b = backend.generation();
    let eof_deadline = Instant::now() + Duration::from_secs(10);
    let mut reached_eof = false;
    while Instant::now() < eof_deadline && !reached_eof {
        for ev in backend.poll_events().expect("poll_events") {
            if let PlayerEvent::Ended { generation } = ev
                && generation == gen_b
            {
                reached_eof = true;
            }
        }
        std::thread::sleep(Duration::from_millis(20));
    }

    assert!(
        reached_eof,
        "replacement track started while paused never reached EOF"
    );

    let _ = fs::remove_file(&wav_a);
    let _ = fs::remove_file(&wav_b);
}

#[test]
fn native_dump_stream_enforces_size_limit() {
    let dir = std::env::temp_dir().join("ansimuz-native-smoke-size-limit");
    let _ = fs::create_dir_all(&dir);
    let wav_src = dir.join("src_large.wav");
    let wav_dst = dir.join("dst_limited.wav");
    write_silence_wav(&wav_src, 5);
    let _ = fs::remove_file(&wav_dst);

    let res = NativeMpv::dump_stream_with_limits(
        wav_src.to_str().expect("valid src"),
        &wav_dst,
        16 * 1024,
        30,
        None,
    );
    let err = res.expect_err("expected size limit error");
    assert!(
        err.contains("exceeded maximum size"),
        "unexpected error: {err}"
    );

    let _ = fs::remove_file(&wav_src);
    let _ = fs::remove_file(&wav_dst);
}

#[test]
fn native_dump_stream_enforces_timeout() {
    use std::io::Write;
    use std::net::TcpListener;
    use std::sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    };

    let listener = TcpListener::bind("127.0.0.1:0").expect("bind localhost");
    let port = listener.local_addr().expect("local addr").port();
    let stop_server = Arc::new(AtomicBool::new(false));
    let stop_clone = Arc::clone(&stop_server);

    let server = std::thread::spawn(move || {
        if let Ok((mut stream, _)) = listener.accept() {
            // Stream continuous PCM silence chunks to simulate a live non-terminating audio feed
            let mut body = Vec::with_capacity(44);
            let data_len: u32 = u32::MAX - 44;
            body.extend_from_slice(b"RIFF");
            body.extend_from_slice(&u32::MAX.to_le_bytes());
            body.extend_from_slice(b"WAVEfmt ");
            body.extend_from_slice(&16u32.to_le_bytes());
            body.extend_from_slice(&1u16.to_le_bytes());
            body.extend_from_slice(&1u16.to_le_bytes());
            body.extend_from_slice(&44_100u32.to_le_bytes());
            body.extend_from_slice(&(44_100u32 * 2).to_le_bytes());
            body.extend_from_slice(&2u16.to_le_bytes());
            body.extend_from_slice(&16u16.to_le_bytes());
            body.extend_from_slice(b"data");
            body.extend_from_slice(&data_len.to_le_bytes());

            let header = "HTTP/1.1 200 OK\r\nContent-Type: audio/wav\r\nConnection: close\r\n\r\n";
            let _ = stream.write_all(header.as_bytes());
            let _ = stream.write_all(&body);
            let chunk = [0u8; 2048];
            let deadline = Instant::now() + Duration::from_secs(10);
            while !stop_clone.load(Ordering::Relaxed) && Instant::now() < deadline {
                if stream.write_all(&chunk).is_err() {
                    break;
                }
                let _ = stream.flush();
                std::thread::sleep(Duration::from_millis(25));
            }
        }
    });

    let dir = std::env::temp_dir().join("ansimuz-native-smoke-timeout");
    let _ = fs::create_dir_all(&dir);
    let wav_dst = dir.join("dst_timeout.wav");
    let _ = fs::remove_file(&wav_dst);

    let url = format!("http://127.0.0.1:{port}/live.wav");
    let started = Instant::now();
    let res = NativeMpv::dump_stream_with_limits(&url, &wav_dst, 64 * 1024 * 1024, 1, None);
    let elapsed = started.elapsed();

    stop_server.store(true, Ordering::Relaxed);
    let _ = server.join();
    let _ = fs::remove_file(&wav_dst);

    let err = res.expect_err("expected timeout error");
    assert!(err.contains("timed out"), "unexpected error: {err}");
    assert!(
        elapsed < Duration::from_secs(4),
        "timeout took too long: {elapsed:?}"
    );
}
