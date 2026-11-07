//! A LuaJIT state with nothing but computation. The trace compiler is off so
//! the instruction hook always runs; budgets therefore preempt any script,
//! including infinite loops inside coroutines or protected calls.
use mlua::{
    Function, HookTriggers, Lua, LuaOptions, LuaString, StdLib, Table, Value, VmState,
    chunk::ChunkMode,
};
use std::{
    collections::BTreeMap,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
};

/// Instructions between budget checks.
const HOOK_INTERVAL: u32 = 1_000;
pub const MEMORY_LIMIT: usize = 64 * 1024 * 1024;
/// Longest string a single library call may build.
const MAX_BUILT_STRING: usize = 1024 * 1024;
const MAX_PATTERN_SUBJECT: usize = 64 * 1024;
const MAX_SORTED: usize = 100_000;
pub const MAX_LOG_LINES: usize = 64;

#[derive(Default)]
pub(crate) struct Budget {
    used: AtomicU64,
    limit: AtomicU64,
    poisoned: AtomicBool,
    reason: std::sync::Mutex<Option<String>>,
}
impl Budget {
    pub fn begin(&self, limit: u64) {
        self.used.store(0, Ordering::Relaxed);
        self.limit.store(limit, Ordering::Relaxed);
    }
    pub fn used(&self) -> u64 {
        self.used.load(Ordering::Relaxed)
    }
    pub fn poisoned(&self) -> bool {
        self.poisoned.load(Ordering::Relaxed)
    }
    pub fn reason(&self) -> Option<String> {
        self.reason.lock().ok().and_then(|reason| reason.clone())
    }
    fn poison(&self, reason: &str) -> mlua::Error {
        self.poisoned.store(true, Ordering::Relaxed);
        if let Ok(mut slot) = self.reason.lock()
            && slot.is_none()
        {
            *slot = Some(reason.to_owned());
        }
        mlua::Error::RuntimeError(format!("tankyard: {reason}"))
    }
}

pub(crate) struct Sandbox {
    pub lua: Lua,
    pub budget: Arc<Budget>,
    pub log: Arc<std::sync::Mutex<Vec<String>>>,
}

/// SplitMix64 spreads small or similar seeds across the whole state space.
fn mix(seed: u64) -> u64 {
    let mut z = seed.wrapping_add(0x9e37_79b9_7f4a_7c15);
    z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    (z ^ (z >> 31)) | 1
}

pub(crate) fn rng_next(state: &AtomicU64) -> u64 {
    let mut x = state.load(Ordering::Relaxed);
    x ^= x << 13;
    x ^= x >> 7;
    x ^= x << 17;
    state.store(x, Ordering::Relaxed);
    x
}

impl Sandbox {
    pub fn new(seed: u64) -> Result<Self, String> {
        let lua = Lua::new_with(
            StdLib::TABLE | StdLib::STRING | StdLib::MATH | StdLib::BIT | StdLib::JIT,
            LuaOptions::new(),
        )
        .map_err(|e| e.to_string())?;
        // Compiled traces never call debug hooks; keep LuaJIT on its fast
        // interpreter so every instruction stays accountable.
        lua.load("jit.off() jit.flush()")
            .set_mode(ChunkMode::Text)
            .exec()
            .map_err(|e| e.to_string())?;
        let budget = Arc::new(Budget::default());
        budget.begin(u64::MAX);
        let hook_budget = budget.clone();
        // A global hook covers coroutines the script creates itself. mlua's
        // per-thread hooks would find no callback for such a coroutine and
        // clear LuaJIT's shared hook, silently ending budget enforcement.
        lua.set_global_hook(
            HookTriggers::new().every_nth_instruction(HOOK_INTERVAL),
            move |lua, _| {
                let used = hook_budget
                    .used
                    .fetch_add(u64::from(HOOK_INTERVAL), Ordering::Relaxed)
                    + u64::from(HOOK_INTERVAL);
                if hook_budget.poisoned() {
                    return Err(hook_budget.poison("script stopped after exceeding its budget"));
                }
                if used > hook_budget.limit.load(Ordering::Relaxed) {
                    return Err(hook_budget.poison("instruction budget exceeded"));
                }
                if lua.used_memory() > MEMORY_LIMIT {
                    return Err(hook_budget.poison("memory limit exceeded (64 MiB)"));
                }
                Ok(VmState::Continue)
            },
        )
        .map_err(|e| e.to_string())?;
        let log = Arc::new(std::sync::Mutex::new(Vec::new()));
        let sandbox = Self { lua, budget, log };
        sandbox.harden(seed).map_err(|e| e.to_string())?;
        Ok(sandbox)
    }

    fn harden(&self, seed: u64) -> mlua::Result<()> {
        let lua = &self.lua;
        let globals = lua.globals();
        for name in [
            "jit",
            "dofile",
            "loadfile",
            "load",
            "loadstring",
            "getfenv",
            "setfenv",
            "collectgarbage",
            "gcinfo",
            "newproxy",
            "module",
            "require",
            "print",
        ] {
            globals.raw_set(name, Value::Nil)?;
        }
        let string: Table = globals.get("string")?;
        string.raw_set("dump", Value::Nil)?;
        // Single library calls run in C, outside the instruction hook; bound
        // the ones that could otherwise allocate or backtrack without limit.
        let rep: Function = string.get("rep")?;
        string.raw_set(
            "rep",
            lua.create_function(
                move |_, (text, count, separator): (LuaString, i64, Option<LuaString>)| {
                    let count = count.max(0) as usize;
                    let separator_len = separator.as_ref().map_or(0, |s| s.as_bytes().len());
                    let size = text
                        .as_bytes()
                        .len()
                        .saturating_mul(count)
                        .saturating_add(separator_len.saturating_mul(count.saturating_sub(1)));
                    if size > MAX_BUILT_STRING {
                        return Err(mlua::Error::RuntimeError(
                            "string.rep result exceeds 1 MiB".into(),
                        ));
                    }
                    rep.call::<LuaString>((text, count, separator))
                },
            )?,
        )?;
        for name in ["find", "match", "gmatch", "gsub"] {
            let original: Function = string.get(name)?;
            string.raw_set(
                name,
                lua.create_function(move |_, args: mlua::MultiValue| {
                    pattern_guard(&args)?;
                    original.call::<mlua::MultiValue>(args)
                })?,
            )?;
        }
        let table: Table = globals.get("table")?;
        let sort: Function = table.get("sort")?;
        table.raw_set(
            "sort",
            lua.create_function(move |_, (list, comparator): (Table, Option<Function>)| {
                if list.raw_len() > MAX_SORTED {
                    return Err(mlua::Error::RuntimeError(
                        "table.sort is limited to 100000 elements".into(),
                    ));
                }
                sort.call::<()>((list, comparator))
            })?,
        )?;
        // Deterministic per-round randomness instead of the C library's.
        let state = Arc::new(AtomicU64::new(mix(seed)));
        let math: Table = globals.get("math")?;
        let random_state = state.clone();
        math.raw_set(
            "random",
            lua.create_function(move |_, (low, high): (Option<i64>, Option<i64>)| {
                let unit = (rng_next(&random_state) >> 11) as f64 / (1_u64 << 53) as f64;
                Ok(match (low, high) {
                    (None, _) => Value::Number(unit),
                    (Some(high), None) if high >= 1 => {
                        Value::Integer(1 + (unit * high as f64) as i64)
                    }
                    (Some(low), Some(high)) if high >= low => {
                        Value::Integer(low + (unit * ((high - low) as f64 + 1.)) as i64)
                    }
                    _ => return Err(mlua::Error::RuntimeError("interval is empty".into())),
                })
            })?,
        )?;
        let seed_state = state;
        math.raw_set(
            "randomseed",
            lua.create_function(move |_, value: f64| {
                seed_state.store(mix(value.to_bits()), Ordering::Relaxed);
                Ok(())
            })?,
        )?;
        let log = self.log.clone();
        globals.raw_set(
            "print",
            lua.create_function(move |lua, values: mlua::MultiValue| {
                let mut line = String::new();
                for (index, value) in values.into_iter().enumerate() {
                    if index > 0 {
                        line.push('\t');
                    }
                    match lua.coerce_string(value.clone())? {
                        Some(text) => line.push_str(&text.to_string_lossy()),
                        None => line.push_str(value.type_name()),
                    }
                    if line.len() > 300 {
                        break;
                    }
                }
                let line: String = line
                    .chars()
                    .filter(|c| !c.is_control() || *c == '\t')
                    .take(300)
                    .collect();
                if let Ok(mut log) = log.lock()
                    && log.len() < MAX_LOG_LINES
                {
                    log.push(line);
                }
                Ok(())
            })?,
        )?;
        // Budget errors must reach the host even through protected calls.
        let budget = self.budget.clone();
        globals.raw_set(
            "__tankyard_poisoned",
            lua.create_function(move |_, ()| Ok(budget.poisoned()))?,
        )?;
        lua.load(
            r#"
            local raw_pcall, raw_xpcall, raw_resume = pcall, xpcall, coroutine.resume
            local poisoned, error, select = __tankyard_poisoned, error, select
            __tankyard_poisoned = nil
            local function guard(...)
                if poisoned() then error("tankyard: script stopped after exceeding its budget", 0) end
                return ...
            end
            pcall = function(f, ...) return guard(raw_pcall(f, ...)) end
            xpcall = function(f, handler, ...) return guard(raw_xpcall(f, handler, ...)) end
            coroutine.resume = function(co, ...) return guard(raw_resume(co, ...)) end
            "#,
        )
        .set_name("=sandbox")
        .set_mode(ChunkMode::Text)
        .exec()?;
        Ok(())
    }

    /// Install a `require` that resolves only modules inside the pack.
    pub fn install_require(&self, sources: Arc<BTreeMap<String, String>>) -> mlua::Result<()> {
        let lua = &self.lua;
        let cache = lua.create_table()?;
        lua.set_named_registry_value("tankyard_modules", cache)?;
        let require = lua.create_function(move |lua, name: String| {
            if name.is_empty()
                || name.len() > 96
                || !name
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'.' | b'-'))
            {
                return Err(mlua::Error::RuntimeError(format!(
                    "invalid module name {name:?}"
                )));
            }
            let cache: Table = lua.named_registry_value("tankyard_modules")?;
            match cache.raw_get::<Value>(name.as_str())? {
                Value::Boolean(false) => {
                    return Err(mlua::Error::RuntimeError(format!(
                        "module {name:?} requires itself"
                    )));
                }
                Value::Nil => {}
                loaded => return Ok(loaded),
            }
            let path = name.replace('.', "/");
            let (file, source) = [format!("{path}.lua"), format!("{path}/init.lua")]
                .into_iter()
                .find_map(|file| sources.get(&file).map(|source| (file, source.clone())))
                .ok_or_else(|| {
                    mlua::Error::RuntimeError(format!(
                        "module {name:?} is not part of this pack (only pack modules can be required)"
                    ))
                })?;
            cache.raw_set(name.as_str(), false)?;
            let value: Value = lua
                .load(source)
                .set_name(format!("@{file}"))
                .set_mode(ChunkMode::Text)
                .set_environment(lua.globals())
                .call(name.as_str())?;
            let value = if value.is_nil() {
                Value::Boolean(true)
            } else {
                value
            };
            cache.raw_set(name.as_str(), value.clone())?;
            Ok(value)
        })?;
        lua.globals().raw_set("require", require)
    }

    /// Run text source in the sandbox's globals. Precompiled chunks are refused.
    pub fn run(&self, name: &str, source: &str) -> mlua::Result<()> {
        self.lua
            .load(source)
            .set_name(format!("@{name}"))
            .set_mode(ChunkMode::Text)
            .set_environment(self.lua.globals())
            .exec()
    }

    pub fn drain_log(&self) -> Vec<String> {
        self.log
            .lock()
            .map(|mut log| std::mem::take(&mut *log))
            .unwrap_or_default()
    }
}

/// Lua patterns backtrack in C. Allow ordinary matching while refusing
/// combinations of long subjects and many quantifiers.
fn pattern_guard(args: &mlua::MultiValue) -> mlua::Result<()> {
    let subject = args.front().and_then(|value| match value {
        Value::String(text) => Some(text.as_bytes().len()),
        _ => None,
    });
    let pattern = args.get(1).and_then(|value| match value {
        Value::String(text) => Some(text.as_bytes().to_vec()),
        _ => None,
    });
    let (Some(length), Some(pattern)) = (subject, pattern) else {
        return Ok(());
    };
    if length > MAX_PATTERN_SUBJECT || pattern.len() > 512 {
        return Err(mlua::Error::RuntimeError(
            "pattern subject exceeds 64 KiB or pattern exceeds 512 bytes".into(),
        ));
    }
    let mut quantifiers = 0_u32;
    let mut escaped = false;
    for &byte in &pattern {
        if escaped {
            escaped = false;
        } else if byte == b'%' {
            escaped = true;
        } else if matches!(byte, b'*' | b'+' | b'-') {
            quantifiers += 1;
        }
    }
    let cost = (length.max(2) as f64).powi(quantifiers.min(8) as i32);
    if quantifiers > 1 && cost > 5e7 {
        return Err(mlua::Error::RuntimeError(
            "pattern is too expensive for this subject; split the string first".into(),
        ));
    }
    Ok(())
}
