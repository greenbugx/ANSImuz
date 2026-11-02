use ansimuz::{app, artwork, scripting, terminal};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let _lua_runtime = scripting::LuaRuntime::initialize()?;

    artwork::init_picker();

    let mut term = terminal::setup()?;
    let result = app::run(&mut term);
    let restore_result = terminal::restore(&mut term);

    result?;
    restore_result?;

    Ok(())
}
