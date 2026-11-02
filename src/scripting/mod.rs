use mlua::Lua;

pub struct LuaRuntime {
    _lua: Lua,
}

impl LuaRuntime {
    pub fn initialize() -> Self {
        Self {
            _lua: Lua::new()
        }
    }
}