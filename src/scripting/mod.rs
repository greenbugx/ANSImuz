use std::fs;

use mlua::Lua;

pub struct LuaRuntime {
    _lua: Lua,
}

impl LuaRuntime {
    pub fn initialize() -> Result<Self, Box<dyn std::error::Error>> {
        let init_path = crate::paths::data_dir().join("lua/init.lua");
        let source = fs::read_to_string(&init_path)?;

        let lua = Lua::new();

        lua.load(&source)
            .set_name("@lua/init.lua")
            .eval::<mlua::Table>()?;

        Ok(Self { _lua: lua })
    }
}
