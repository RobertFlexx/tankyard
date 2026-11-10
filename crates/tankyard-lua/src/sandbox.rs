//! A Luau state with nothing but computation. Luau calls an interrupt at every
//! loop back edge and every call, in its interpreter and in native code alike,
//! so budgets preempt any script, including infinite loops inside coroutines
//! or protected calls.
//!
//! The vendored Luau (see `vendor/README.md`) prints numbers, formats strings
//! and parses Lua 5.3 bitwise operators exactly as LuaJIT did, and bounds the
//! library calls that run in C (`string.rep`, pattern matching, `table.sort`).
use mlua::{
    Function, Lua, LuaOptions, StdLib, Table, Value,
    chunk::{ChunkMode, Compiler},
    ffi,
};
use std::{
    collections::{BTreeMap, BTreeSet, HashMap},
    ffi::c_int,
    hash::{Hash, Hasher},
    sync::{
        Arc, LazyLock, Mutex,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    time::{Duration, Instant},
};

pub const MEMORY_LIMIT: usize = 64 * 1024 * 1024;
/// Allocations fail past this; the interrupt stops the script well before.
const HARD_MEMORY_LIMIT: usize = MEMORY_LIMIT + 16 * 1024 * 1024;
/// Interrupts between memory checks; time is checked every eighth check.
const CHECK_INTERVAL: u64 = 256;
pub const MAX_LOG_LINES: usize = 64;

pub(crate) struct Budget {
    used: AtomicU64,
    limit: AtomicU64,
    /// Wall-clock backstop for the current allowance. Steps bound work
    /// deterministically; this only catches pathological loop bodies.
    deadline: std::sync::Mutex<Instant>,
    poisoned: AtomicBool,
    reason: std::sync::Mutex<Option<String>>,
}

impl Default for Budget {
    fn default() -> Self {
        Self {
            used: AtomicU64::new(0),
            limit: AtomicU64::new(u64::MAX),
            deadline: std::sync::Mutex::new(Instant::now() + Duration::from_secs(3600)),
            poisoned: AtomicBool::new(false),
            reason: std::sync::Mutex::new(None),
        }
    }
}

impl Budget {
    /// Start a new allowance of `limit` steps. Luau charges a step at every
    /// loop iteration and call, and every LuaJIT loop iteration or call ran
    /// at least one instruction, so any script that fit LuaJIT's instruction
    /// limits fits the same number of steps.
    pub fn begin(&self, limit: u64) {
        self.used.store(0, Ordering::Relaxed);
        self.limit.store(limit, Ordering::Relaxed);
        if let Ok(mut deadline) = self.deadline.lock() {
            let millis = 250_u64.saturating_add(limit / 2_000).min(3_600_000);
            *deadline = Instant::now() + Duration::from_millis(millis);
        }
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
    fn poison(&self, reason: &str) {
        self.poisoned.store(true, Ordering::Relaxed);
        if let Ok(mut slot) = self.reason.lock()
            && slot.is_none()
        {
            *slot = Some(reason.to_owned());
        }
    }
    fn past_deadline(&self) -> bool {
        self.deadline
            .lock()
            .map(|deadline| Instant::now() > *deadline)
            .unwrap_or(false)
    }
}

/// Raise a Lua error from the interrupt. Luau unwinds with a C++ exception,
/// which may pass through this `C-unwind` frame; nothing here needs dropping.
unsafe fn raise(state: *mut ffi::lua_State, message: &'static [u8]) -> ! {
    unsafe {
        ffi::lua_pushlstring(state, message.as_ptr().cast(), message.len());
        ffi::lua_error(state)
    }
}

unsafe extern "C-unwind" fn interrupt(state: *mut ffi::lua_State, gc: c_int) {
    // Garbage collector steps cannot survive an error.
    if gc >= 0 {
        return;
    }
    // SAFETY: `Sandbox::new` stores the budget in the main thread's data and
    // keeps an `Arc` to it in the state's app data, which outlives every
    // interrupt; dropping the state removes this callback first.
    let budget = unsafe { ffi::lua_getthreaddata(ffi::lua_mainthread(state)) } as *const Budget;
    if budget.is_null() {
        return;
    }
    let budget = unsafe { &*budget };
    let used = budget.used.load(Ordering::Relaxed) + 1;
    budget.used.store(used, Ordering::Relaxed);
    if budget.poisoned() {
        unsafe {
            raise(
                state,
                b"tankyard: script stopped after exceeding its budget",
            )
        }
    }
    if used > budget.limit.load(Ordering::Relaxed) {
        budget.poison("instruction budget exceeded");
        unsafe { raise(state, b"tankyard: instruction budget exceeded") }
    }
    if used % CHECK_INTERVAL == 0 {
        let kilobytes = unsafe { ffi::lua_gc(state, ffi::LUA_GCCOUNT, 0) };
        if kilobytes as usize > MEMORY_LIMIT / 1024 {
            budget.poison("memory limit exceeded (64 MiB)");
            unsafe { raise(state, b"tankyard: memory limit exceeded (64 MiB)") }
        }
        if used % (CHECK_INTERVAL * 8) == 0 && budget.past_deadline() {
            budget.poison("instruction budget exceeded (time limit)");
            unsafe { raise(state, b"tankyard: instruction budget exceeded (time limit)") }
        }
    }
}

pub(crate) struct Sandbox {
    pub lua: Lua,
    pub budget: Arc<Budget>,
    pub log: Arc<std::sync::Mutex<Vec<String>>>,
    bytecode: Bytecode,
    native: bool,
}

/// Compiled chunks shared by every pack state in the process, by compiler
/// options and then source, so a pack is parsed and compiled once rather
/// than every round.
static BYTECODE: LazyLock<Mutex<HashMap<u64, Sources>>> = LazyLock::new(Default::default);
/// Bytecode by source text.
type Sources = HashMap<String, Arc<Vec<u8>>>;
/// Cached sources kept before the cache starts over.
const BYTECODE_ENTRIES: usize = 512;

/// Compiles text source with one pack's options, through [`BYTECODE`].
#[derive(Clone)]
pub(crate) struct Bytecode {
    compiler: Compiler,
    options: u64,
}

impl Bytecode {
    fn new(globals: &Globals) -> Self {
        // Optimization level 2 inlines local functions and unrolls short
        // loops; debug level 1 keeps line numbers for error messages. Type
        // information guides the native code generator.
        let compiler = Compiler::new()
            .set_optimization_level(2)
            .set_debug_level(1)
            .set_type_info_level(1)
            .set_mutable_globals(globals.mutable.iter().cloned());
        let mut hasher = std::hash::DefaultHasher::new();
        globals.mutable.hash(&mut hasher);
        Self {
            compiler,
            options: hasher.finish(),
        }
    }

    /// A chunk of text source. Only text is ever compiled; a source that
    /// does not compile is loaded as text so its error reads as before.
    pub fn load<'a>(&self, lua: &'a Lua, name: &str, source: &str) -> mlua::chunk::Chunk<'a> {
        let cached = BYTECODE.lock().ok().and_then(|cache| {
            cache
                .get(&self.options)
                .and_then(|sources| sources.get(source))
                .cloned()
        });
        let bytecode = cached.or_else(|| {
            let bytecode = Arc::new(self.compiler.compile(source).ok()?);
            if let Ok(mut cache) = BYTECODE.lock() {
                if cache.values().map(HashMap::len).sum::<usize>() >= BYTECODE_ENTRIES {
                    cache.clear();
                }
                cache
                    .entry(self.options)
                    .or_default()
                    .insert(source.to_owned(), bytecode.clone());
            }
            Some(bytecode)
        });
        match bytecode {
            Some(bytecode) => lua
                .load(bytecode.as_ref().clone())
                .set_name(name.to_owned())
                .set_mode(ChunkMode::Binary),
            None => lua
                .load(source.to_owned())
                .set_name(name.to_owned())
                .set_mode(ChunkMode::Text),
        }
    }
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

/// LuaJIT's `bit` library and the functions behind the Lua 5.3 bitwise
/// operators, with LuaJIT's signed 32-bit results. `tobit` adds 2^52 + 2^51
/// and keeps the low word, exactly as LuaJIT does, so rounding and wrapping
/// of fractional and out-of-range numbers match too.
const BIT_LIBRARY: &str = r##"
local buffer_create, writef64, readi32 = buffer.create, buffer.writef64, buffer.readi32
local band32, bor32, bxor32, bnot32 = bit32.band, bit32.bor, bit32.bxor, bit32.bnot
local lshift32, rshift32, arshift32 = bit32.lshift, bit32.rshift, bit32.arshift
local lrotate32, rrotate32, byteswap32 = bit32.lrotate, bit32.rrotate, bit32.byteswap
local type, tonumber, error, select, format = type, tonumber, error, select, string.format
local scratch = buffer_create(8)

local function raw(x)
  writef64(scratch, 0, x + 6755399441055744)
  return readi32(scratch, 0)
end
local function signed(u)
  if u >= 2147483648 then return u - 4294967296 end
  return u
end
local function check(x, position, name)
  if type(x) == "number" then return raw(x) end
  local n = type(x) == "string" and tonumber(x)
  if n then return raw(n) end
  error(("bad argument #%d to '%s' (number expected, got %s)"):format(position, name,
    x == nil and "no value" or type(x)), 3)
end
local function fold(name, combine)
  return function(x, ...)
    local result = check(x, 1, name)
    for index = 1, select("#", ...) do
      result = signed(combine(result, check((select(index, ...)), index + 1, name)))
    end
    return result
  end
end

local bit = {}
function bit.tobit(x) return check(x, 1, "tobit") end
function bit.bnot(x) return signed(bnot32(check(x, 1, "bnot"))) end
bit.band = fold("band", band32)
bit.bor = fold("bor", bor32)
bit.bxor = fold("bxor", bxor32)
function bit.lshift(x, n) return signed(lshift32(check(x, 1, "lshift"), check(n, 2, "lshift") % 32)) end
function bit.rshift(x, n) return signed(rshift32(check(x, 1, "rshift"), check(n, 2, "rshift") % 32)) end
function bit.arshift(x, n) return signed(arshift32(check(x, 1, "arshift"), check(n, 2, "arshift") % 32)) end
function bit.rol(x, n) return signed(lrotate32(check(x, 1, "rol"), check(n, 2, "rol") % 32)) end
function bit.ror(x, n) return signed(rrotate32(check(x, 1, "ror"), check(n, 2, "ror") % 32)) end
function bit.bswap(x) return signed(byteswap32(check(x, 1, "bswap"))) end
function bit.tohex(x, n)
  x = check(x, 1, "tohex")
  n = n == nil and 8 or check(n, 2, "tohex")
  local digits = "%08x"
  if n < 0 then n, digits = -n, "%08X" end
  if n > 8 then n = 8 end
  local text = format(digits, x % 4294967296)
  return text:sub(9 - n)
end

-- The operators accept only numbers, as in LuaJIT.
local function operand(x)
  if type(x) ~= "number" then
    error(("attempt to perform bitwise operation on a %s value"):format(type(x)), 3)
  end
  return raw(x)
end
local ops = {}
function ops.band(a, b) return signed(band32(operand(a), operand(b))) end
function ops.bor(a, b) return signed(bor32(operand(a), operand(b))) end
function ops.bxor(a, b) return signed(bxor32(operand(a), operand(b))) end
function ops.bnot(a) return signed(bnot32(operand(a))) end
function ops.lshift(a, b) return signed(lshift32(operand(a), operand(b) % 32)) end
function ops.rshift(a, b) return signed(rshift32(operand(a), operand(b) % 32)) end
return bit, ops
"##;

/// Library tables and functions Luau's compiler may fold or call directly.
const BUILTIN_GLOBALS: [&str; 30] = [
    "math",
    "string",
    "table",
    "bit32",
    "buffer",
    "coroutine",
    "utf8",
    "bit",
    "vector",
    "assert",
    "type",
    "typeof",
    "rawget",
    "rawset",
    "rawequal",
    "rawlen",
    "unpack",
    "select",
    "tonumber",
    "tostring",
    "getmetatable",
    "setmetatable",
    "ipairs",
    "pairs",
    "next",
    "pcall",
    "xpcall",
    "error",
    "print",
    "require",
];
const LIBRARY_TABLES: [&str; 8] = [
    "math",
    "string",
    "table",
    "bit32",
    "buffer",
    "coroutine",
    "utf8",
    "bit",
];

/// How a pack's chunks are compiled; see [`compile_globals`].
pub(crate) struct Globals {
    /// Globals whose lookups and builtin calls must stay dynamic.
    pub mutable: BTreeSet<String>,
    /// Whether builtins may be called directly at run time.
    pub safe: bool,
}

/// Luau caches global lookups when a chunk loads and folds or directly calls
/// library functions. That must not change what a pack sees, so this lists
/// every global the pack assigns anywhere (`name =`, `name.field =`,
/// `function name`), every library table it uses as a value (which could
/// alias a write), and the string library when it reaches it through
/// `getmetatable("")`; those keep plain lookups. A pack that touches `_G`
/// can rebind anything, so it gets no shortcuts at all. Locals that happen
/// to be listed cost nothing.
pub(crate) fn compile_globals<'a>(sources: impl IntoIterator<Item = &'a str>) -> Globals {
    use crate::compat::Token;
    let mut names = BTreeSet::new();
    let mut safe = true;
    for source in sources {
        let tokens = crate::compat::tokens(source);
        for (index, token) in tokens.iter().enumerate() {
            let Token::Name(name) = token else {
                continue;
            };
            if *name == "_G" {
                safe = false;
                continue;
            }
            let previous = index.checked_sub(1).and_then(|i| tokens.get(i));
            // `function name...`
            if previous == Some(&Token::Keyword("function")) {
                names.insert((*name).to_owned());
                continue;
            }
            if *name == "getmetatable"
                && tokens.get(index + 1) == Some(&Token::Symbol("("))
                && tokens.get(index + 2) == Some(&Token::String)
            {
                names.insert("string".to_owned());
            }
            let after = tokens.get(index + 1);
            if LIBRARY_TABLES.contains(name)
                && previous != Some(&Token::Symbol("."))
                && previous != Some(&Token::Symbol(":"))
                && !matches!(after, Some(Token::Symbol("." | ":" | "[")))
            {
                names.insert((*name).to_owned());
                continue;
            }
            // `name =`, `name.a.b =`, `name[...] =`, `a, name =`
            let mut next = index + 1;
            let mut depth = 0_usize;
            while let Some(token) = tokens.get(next) {
                match token {
                    Token::Symbol("[") => depth += 1,
                    Token::Symbol("]") if depth > 0 => depth -= 1,
                    Token::Symbol(".") if depth == 0 => {}
                    Token::Name(_) if depth == 0 => {
                        if tokens.get(next - 1) != Some(&Token::Symbol(".")) {
                            break;
                        }
                    }
                    _ if depth > 0 => {}
                    _ => break,
                }
                next += 1;
            }
            if matches!(
                tokens.get(next),
                Some(Token::Symbol(
                    "=" | "," | "+=" | "-=" | "*=" | "/=" | "//=" | "%=" | "^=" | "..="
                ))
            ) {
                names.insert((*name).to_owned());
            }
        }
    }
    if !safe {
        names.extend(BUILTIN_GLOBALS.iter().map(|name| (*name).to_owned()));
    }
    Globals {
        mutable: names,
        safe,
    }
}

impl Sandbox {
    /// `native` compiles loaded chunks to machine code; worth it for states
    /// that run every tick, not for the one-off definition pass.
    pub fn new(seed: u64, globals: &Globals, native: bool) -> Result<Self, String> {
        let lua = Lua::new_with(
            StdLib::COROUTINE
                | StdLib::TABLE
                | StdLib::STRING
                | StdLib::MATH
                | StdLib::BIT
                | StdLib::UTF8
                | StdLib::BUFFER,
            LuaOptions::new(),
        )
        .map_err(|e| e.to_string())?;
        // iOS forbids executable memory for apps, so scripts stay interpreted.
        let native = native && cfg!(not(target_os = "ios"));
        lua.enable_jit(native);
        lua.set_memory_limit(HARD_MEMORY_LIMIT)
            .map_err(|e| e.to_string())?;
        let bytecode = Bytecode::new(globals);
        lua.set_compiler(bytecode.compiler.clone());
        let budget = Arc::new(Budget::default());
        lua.set_app_data(budget.clone());
        let pointer = Arc::as_ptr(&budget) as *mut std::ffi::c_void;
        // SAFETY: the app data above keeps the budget alive as long as the
        // state. mlua keeps its own data in the callbacks' `userdata` and
        // never uses thread data.
        unsafe {
            lua.exec_raw::<()>((), |state| {
                ffi::lua_setthreaddata(ffi::lua_mainthread(state), pointer);
                (*ffi::lua_callbacks(state)).interrupt = Some(interrupt);
            })
            .map_err(|e| e.to_string())?;
        }
        let log = Arc::new(std::sync::Mutex::new(Vec::new()));
        let sandbox = Self {
            lua,
            budget,
            log,
            bytecode,
            native,
        };
        sandbox.harden(seed).map_err(|e| e.to_string())?;
        if globals.safe {
            // Builtins such as `math.floor` become direct calls and global
            // lookups are cached. Library tables stay writable, as on LuaJIT;
            // a pack that rebinds a library sees its names left uncached.
            unsafe {
                sandbox
                    .lua
                    .exec_raw::<()>((), |state| {
                        ffi::lua_setsafeenv(state, ffi::LUA_GLOBALSINDEX, 1);
                    })
                    .map_err(|e| e.to_string())?;
            }
        }
        Ok(sandbox)
    }

    fn harden(&self, seed: u64) -> mlua::Result<()> {
        let lua = &self.lua;
        let globals = lua.globals();
        for name in [
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
        // LuaJIT reported Lua 5.1; scripts that check it see the same.
        globals.raw_set("_VERSION", "Lua 5.1")?;
        let (bit, ops): (Table, Table) = self.bytecode.load(lua, "=bit", BIT_LIBRARY).eval()?;
        globals.raw_set("bit", bit)?;
        globals.raw_set("__tankyard_bitops", ops)?;
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
        self.bytecode
            .load(
                lua,
                "=sandbox",
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
            .exec()?;
        Ok(())
    }

    /// Install a `require` that resolves only modules inside the pack.
    pub fn install_require(&self, sources: Arc<BTreeMap<String, String>>) -> mlua::Result<()> {
        let lua = &self.lua;
        let cache = lua.create_table()?;
        lua.set_named_registry_value("tankyard_modules", cache)?;
        let bytecode = self.bytecode.clone();
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
            let source =
                crate::compat::luau_source(&file, &source).map_err(mlua::Error::RuntimeError)?;
            cache.raw_set(name.as_str(), false)?;
            let value: Value = bytecode
                .load(lua, &format!("@{file}"), &source)
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
        let source = crate::compat::luau_source(name, source).map_err(mlua::Error::RuntimeError)?;
        self.bytecode
            .load(&self.lua, &format!("@{name}"), &source)
            .exec()
    }

    /// Load host source (the prelude) through the shared bytecode cache.
    pub fn host_chunk<'a>(&'a self, name: &str, source: &str) -> mlua::chunk::Chunk<'a> {
        self.bytecode.load(&self.lua, name, source)
    }

    /// Load host source without compiling it to machine code; see
    /// [`Sandbox::compile_native`].
    pub fn interpreted<R>(&self, f: impl FnOnce() -> R) -> R {
        self.lua.enable_jit(false);
        let result = f();
        self.lua.enable_jit(self.native);
        result
    }

    /// Compile chosen host functions to machine code, when this state uses it.
    pub fn compile_native(&self, functions: &[&Function]) -> mlua::Result<()> {
        if !self.native {
            return Ok(());
        }
        for function in functions {
            // SAFETY: the function is on the stack for the call; native
            // compilation only attaches code to its prototypes.
            unsafe {
                self.lua.exec_raw::<()>(*function, |state| {
                    if ffi::luau_codegen_supported() != 0 {
                        ffi::luau_codegen_compile(state, -1);
                    }
                })?;
            }
        }
        Ok(())
    }

    pub fn drain_log(&self) -> Vec<String> {
        self.log
            .lock()
            .map(|mut log| std::mem::take(&mut *log))
            .unwrap_or_default()
    }
}
