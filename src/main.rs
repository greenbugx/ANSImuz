use pyo3::prelude::*;
use pyo3::types::{PyList, PyModule};
use std::env;

fn main() -> PyResult<()> {
    let current_dir = env::current_dir().expect("Failed to get current directory");
    let backend_path = current_dir.join("backend");

    unsafe {
        env::set_var("PYTHONPATH", backend_path.to_string_lossy().to_string());
    }

    println!("[Rust] Initializing Python backend...");

    Python::attach(|py| {
        let bridge = PyModule::import(py, "bridge")?;

        let args = ("synthwave",);

        let result = bridge
            .getattr("search_music")?
            .call1(args)?;

        let py_results = result.cast::<PyList>()?;

        println!("[Rust] Received results from Python backend!");

        for item in py_results.iter() {
            let title: String = item.get_item("title")?.extract()?;
            let artist: String = item.get_item("artist")?.extract()?;

            println!("Found Track: {} by {}", title, artist);
        }

        Ok(())
    })
}
