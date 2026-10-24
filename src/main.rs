use pyo3::prelude::*;
use pyo3::types::{PyList, PyModule};
use std::env;

mod player;

fn main() -> PyResult<()> {
    let current_dir = env::current_dir().expect("Failed to get current directory");
    let backend_path = current_dir.join("backend");

    unsafe {
        env::set_var("PYTHONPATH", backend_path.to_string_lossy().to_string());
    }

    println!("[Rust] Initializing Python backend...");

    let mut target_stream_url = String::new();

    Python::attach(|py| -> PyResult<()> {
        let bridge = PyModule::import(py, "bridge")?;
        let args = ("synthwave",);
        let result = bridge
            .getattr("search_music")?
            .call1(args)?;
        let py_results = result.cast::<PyList>()?;

        println!("[Rust] Received results from Python backend!");

        // let mut player = player::AudioPlayer::new();

        if let Some(first_item) = py_results.iter().next() {
            let title: String = first_item.get_item("title")?.extract()?;
            let artist: String = first_item.get_item("artist")?.extract()?;
            let stream_url: String = first_item.get_item("stream_url")?.extract()?;

            println!("Streaming Track: {} by {}", title, artist);
            println!("[Rust] Stream URL: {}", stream_url);

            // player.play(&stream_url);

            // std::thread::sleep(std::time::Duration::from_secs(30)); // play for 10 seconds for test
            // player.stop();

            target_stream_url = stream_url;
        }

        Ok(())
    })?;

    if !target_stream_url.is_empty() {
        println!("\n[Rust] Spawning headless player process...");
        let mut player = player::AudioPlayer::new();
        
        player.play(&target_stream_url);

        std::thread::sleep(std::time::Duration::from_secs(30)); 

        println!("[Rust] Stopping music playback.");
        player.stop();
    } else {
        println!("[Rust] Error: No stream URL was extracted.");
    }

    Ok(())
}
