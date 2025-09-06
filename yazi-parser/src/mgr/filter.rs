use mlua::{ExternalError, FromLua, IntoLua, Lua, Value};
use yazi_fs::FilterCase;
use yazi_shared::{SStr, event::CmdCow};

#[derive(Debug, Default)]
pub struct FilterOpt {
    pub query: SStr,
    pub case:  FilterCase,
    pub done:  bool,
    pub deep:  bool,
    pub levels: Option<usize>,
    pub tags:   Option<SStr>,
    pub tag_all: bool,
    pub combine_or: bool,
}

impl TryFrom<CmdCow> for FilterOpt {
	type Error = anyhow::Error;

	fn try_from(mut c: CmdCow) -> Result<Self, Self::Error> {
		if let Some(opt) = c.take_any2("opt") {
			return opt;
		}

        let levels = c
            .try_take("levels")
            .and_then(|d| d.as_usize())
            .or_else(|| c.get("levels").and_then(|d| d.as_usize()));

        let tags = c
            .try_take("tags")
            .and_then(|d| d.into_string())
            .or_else(|| c.take_str("tags"));

        // Parse combine mode: default AND; allow --combine=or
        let combine_or = c
            .try_take("combine")
            .and_then(|d| d.into_string())
            .or_else(|| c.take_str("combine"))
            .map(|s| matches!(s.to_ascii_lowercase().as_str(), "or"))
            .unwrap_or(false);

        Ok(Self {
            query: c.take_first_str().unwrap_or_default(),
            case:  FilterCase::from(&*c),
            done:  c.bool("done"),
            deep:  c.bool("deep"),
            levels,
            tags,
            tag_all: c.bool("tag-all") || c.bool("tag_all"),
            combine_or,
        })
    }
}

impl FromLua for FilterOpt {
	fn from_lua(_: Value, _: &Lua) -> mlua::Result<Self> { Err("unsupported".into_lua_err()) }
}

impl IntoLua for FilterOpt {
	fn into_lua(self, _: &Lua) -> mlua::Result<Value> { Err("unsupported".into_lua_err()) }
}
