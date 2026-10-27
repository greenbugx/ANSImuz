mod app;
mod artwork;
mod config;
mod music;
mod playback;
mod terminal;
mod ui;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    artwork::init_picker();

    let mut term = terminal::setup()?;
    let result = app::run(&mut term);
    let restore_result = terminal::restore(&mut term);

    result?;
    restore_result?;

    Ok(())
}
