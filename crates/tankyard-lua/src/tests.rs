use super::*;
use std::time::Duration;
use tankyard_core::{V2, WorldConfig};

fn info(id: &str) -> ModInfo {
    ModInfo {
        id: id.into(),
        name: id.into(),
        version: "1".into(),
        behavior_digest: 0,
    }
}

fn pack(source: &str) -> ScriptPack {
    let mut pack = ScriptPack {
        entries: vec!["main.lua".into()],
        ..Default::default()
    };
    pack.sources.insert("main.lua".into(), source.into());
    pack
}

fn world() -> World {
    World::new(
        WorldConfig {
            target_count: 0,
            spawn_protection: 0.,
            ..Default::default()
        },
        Content::builtin(),
    )
}

/// Run one round's worth of ticks with a single script; returns the world and
/// the runtime so tests can inspect effects and errors.
fn run(source: &str, ticks: usize) -> (World, ModRuntime) {
    let mut world = world();
    world.add_player("Pilot", false);
    let mut runtime = ModRuntime::default();
    runtime
        .install(Box::new(
            LuaPlugin::new(&info("test"), &pack(source), &Content::builtin(), 7).unwrap(),
        ))
        .unwrap();
    for _ in 0..ticks {
        world.step_with(|w| runtime.before_step(w));
        runtime.after_step(&mut world);
    }
    (world, runtime)
}

/// A runaway script must be stopped by the budget. If preemption ever breaks
/// (for example a JIT-compiled loop), fail instead of hanging the test suite.
fn stopped_within(source: &'static str, seconds: u64) -> String {
    let (sender, receiver) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let (_, runtime) = run(source, 3);
        let _ = sender.send(runtime.errors.join(" | "));
    });
    receiver
        .recv_timeout(Duration::from_secs(seconds))
        .unwrap_or_else(|_| panic!("script was not preempted: {source}"))
}

#[test]
fn system_interfaces_are_absent() {
    let source = r#"
        local missing = {}
        for _, name in ipairs({ "os", "io", "debug", "ffi", "jit", "package", "dofile",
            "loadfile", "load", "loadstring", "getfenv", "setfenv", "collectgarbage",
            "newproxy", "module", "gcinfo" }) do
          if _G[name] ~= nil then missing[#missing + 1] = name end
        end
        assert(#missing == 0, "exposed: " .. table.concat(missing, ","))
        assert(string.dump == nil, "string.dump exposed")
        assert(not pcall(require, "ffi"), "required ffi")
        assert(not pcall(require, "os"), "required os")
        assert(type(coroutine.create) == "function")
        assert(type(bit.band) == "function")
        tankyard.on("tick", function() tankyard.state.ok = true end)
    "#;
    let plugin = LuaPlugin::new(&info("escape"), &pack(source), &Content::builtin(), 1);
    assert!(plugin.is_ok(), "{:?}", plugin.err());
}

#[test]
fn precompiled_chunks_are_refused() {
    // A module file containing LuaJIT bytecode must not load.
    let mut scripts = pack(
        "local ok, err = pcall(require, 'blob') assert(not ok) assert(tostring(err):find('binary') or tostring(err):find('attempt'))",
    );
    scripts
        .sources
        .insert("blob.lua".into(), "\u{1b}LJ\u{2}\u{0}garbage".into());
    LuaPlugin::new(&info("bytecode"), &scripts, &Content::builtin(), 1).unwrap();
    // Luau bytecode starts with its version byte; it must not load either.
    let bytecode = mlua::chunk::Compiler::new().compile("return 1").unwrap();
    let mut scripts = pack(
        "local ok, err = pcall(require, 'blob') assert(not ok) assert(tostring(err):find('binary'))",
    );
    scripts.sources.insert(
        "blob.lua".into(),
        String::from_utf8_lossy(&bytecode).into_owned(),
    );
    LuaPlugin::new(&info("bytecode"), &scripts, &Content::builtin(), 1).unwrap();
    let error = LuaPlugin::new(
        &info("bytecode"),
        &pack(&String::from_utf8_lossy(&bytecode)),
        &Content::builtin(),
        1,
    )
    .err()
    .unwrap();
    assert!(error.contains("binary"), "{error}");
}

#[test]
fn hot_infinite_loops_are_preempted_even_where_luajit_would_compile_them() {
    let errors = stopped_within("tankyard.on('tick', function() while true do end end)", 20);
    assert!(errors.contains("instruction budget"), "{errors}");
    let errors = stopped_within(
        "tankyard.on('tick', function() local x = 0 for i = 1, 1e12 do x = x + i % 7 end end)",
        20,
    );
    assert!(errors.contains("budget"), "{errors}");
}

#[test]
fn protected_calls_and_coroutines_cannot_swallow_the_budget() {
    let errors = stopped_within(
        "tankyard.on('tick', function() while true do pcall(function() while true do end end) end end)",
        20,
    );
    assert!(errors.contains("budget"), "{errors}");
    let errors = stopped_within(
        "tankyard.on('tick', function() while true do xpcall(function() while true do end end, function(e) return e end) end end)",
        20,
    );
    assert!(errors.contains("budget"), "{errors}");
    let errors = stopped_within(
        "tankyard.on('tick', function() while true do local co = coroutine.create(function() while true do end end) coroutine.resume(co) end end)",
        20,
    );
    assert!(errors.contains("budget"), "{errors}");
}

#[test]
fn memory_and_single_call_bombs_are_bounded() {
    let errors = stopped_within(
        "tankyard.on('tick', function() local t = {} for i = 1, 1e9 do t[i] = ('x'):rep(4096) .. i end end)",
        30,
    );
    assert!(
        errors.contains("memory") || errors.contains("budget"),
        "{errors}"
    );
    let errors = stopped_within(
        "tankyard.on('tick', function() local s = ('x'):rep(2^31) end)",
        10,
    );
    assert!(errors.contains("1 MiB"), "{errors}");
    let errors = stopped_within(
        "tankyard.on('tick', function() local s = ('a'):rep(20000) s:find('.-.-.-.-b') end)",
        10,
    );
    assert!(errors.contains("pattern"), "{errors}");
    let errors = stopped_within(
        "tankyard.on('tick', function() local t = {} for i = 1, 200000 do t[i] = i end table.sort(t) end)",
        10,
    );
    assert!(
        errors.contains("sort") || errors.contains("budget"),
        "{errors}"
    );
}

#[test]
fn a_failing_pack_is_disabled_and_ordinary_patterns_still_work() {
    let (world, runtime) = run(
        r#"
        tankyard.on("tick", function(e)
          local name = ("Pilot one"):gsub("%s+", "_")
          assert(name == "Pilot_one")
          assert(("a,b,c"):find(",", 1, true) == 2)
          if e.tick == 2 then error("boom") end
        end)
        "#,
        5,
    );
    assert_eq!(runtime.errors.len(), 1);
    assert!(runtime.errors[0].contains("boom"), "{:?}", runtime.errors);
    assert!(world.tanks[0].alive);
}

#[test]
fn scripts_change_gameplay_through_validated_commands() {
    let (world, runtime) = run(
        r#"
        local pilot
        tankyard.on("spawn", function(e) pilot = e.actor end)
        tankyard.every(2, function(tick)
          if not pilot then return end
          local me = tankyard.tank(pilot)
          assert(me and me.name == "Pilot" and me.level >= 1)
          if tick == 4 then
            tankyard.set_level(pilot, 30)
            tankyard.set_scales(pilot, { speed = 1.5 })
            tankyard.set_armor(pilot, 0.5)
            tankyard.announce("Thirty!")
            tankyard.spawn_shape(1000, 1000, { sides = 6, health = 80, xp = 40 })
          end
        end)
        "#,
        8,
    );
    assert!(runtime.errors.is_empty(), "{:?}", runtime.errors);
    let tank = &world.tanks[0];
    assert_eq!(tank.level, 30);
    assert_eq!(tank.speed_scale, 1.5);
    assert_eq!(tank.reload_scale, 1.);
    assert_eq!(world.incoming_damage.get(&tank.id), Some(&0.5));
    assert_eq!(world.match_state.announcement, "Thirty!");
    assert!(
        world
            .targets
            .iter()
            .any(|t| t.kind == tankyard_core::TargetKind::Custom(6)
                && t.pos.distance(V2::new(1000., 1000.)) < 5.)
    );
}

#[test]
fn invalid_requests_disable_the_pack_without_partial_effects() {
    let (world, runtime) = run(
        r#"
        tankyard.on("tick", function(e)
          local me = tankyard.tanks()[1]
          tankyard.set_level(me.id, 40)
          tankyard.set_class(me.id, "no_such_tank")
        end)
        "#,
        3,
    );
    assert_eq!(runtime.errors.len(), 1);
    assert_eq!(world.tanks[0].level, 1, "the failing batch is discarded");
}

#[test]
fn world_access_ends_with_the_event_and_randomness_is_seeded() {
    let source = r#"
        local saved
        tankyard.on("tick", function(e)
          if not saved then
            saved = tankyard.tanks
            tankyard.state.first = math.random()
          else
            local ok = pcall(function() return saved() end)
            assert(ok, "API wrappers stay usable inside later events")
          end
        end)
    "#;
    let (_, runtime) = run(source, 3);
    assert!(runtime.errors.is_empty(), "{:?}", runtime.errors);
    let outside = LuaPlugin::new(
        &info("outside"),
        &pack(
            "local ok, err = pcall(tankyard.tanks) assert(not ok and err:find('inside event handlers'))",
        ),
        &Content::builtin(),
        1,
    );
    assert!(outside.is_ok(), "{:?}", outside.err());
    let draw = |seed| {
        let script = Script::new(
            &info("rng"),
            &pack("x = math.random(1, 1000000)"),
            seed,
            &Content::builtin(),
            false,
        )
        .unwrap();
        script.sandbox.lua.globals().get::<i64>("x").unwrap()
    };
    assert_eq!(draw(11), draw(11));
    assert_ne!(draw(11), draw(12));
}

#[test]
fn print_output_is_bounded_and_collected() {
    let (_, mut runtime) = run(
        "tankyard.on('tick', function(e) for i = 1, 200 do print('line', i, ('x'):rep(500)) end end)",
        1,
    );
    assert!(runtime.errors.is_empty(), "{:?}", runtime.errors);
    let lines: Vec<_> = runtime.log.drain(..).collect();
    assert!(!lines.is_empty() && lines.len() <= sandbox::MAX_LOG_LINES);
    assert!(
        lines
            .iter()
            .all(|line| line.len() < 340 && line.starts_with("[test] line"))
    );
}

#[test]
fn packs_define_tanks_rules_and_modes_that_join_the_existing_tree() {
    let root = std::env::temp_dir().join(format!(
        "tankyard-lua-pack-{}-{:x}",
        std::process::id(),
        tankyard_mods::digest(b"define")
    ));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(root.join("lib")).unwrap();
    std::fs::write(
        root.join("mod.json"),
        r#"{"api_version":2,"id":"sprinklers","name":"Sprinklers","version":"1.0","scripts":["main.lua"]}"#,
    )
    .unwrap();
    std::fs::write(root.join("lib/tree.lua"), "return { parent = 'vector' }").unwrap();
    std::fs::write(
        root.join("main.lua"),
        r#"
        local tree = require("lib.tree")
        local parent = tankyard.base_class(tree.parent)
        table.insert(parent.upgrades, "sprinkler")
        tankyard.define_class(parent)
        local sprinkler = tankyard.base_class("spray")
        sprinkler.id, sprinkler.name = "sprinkler", "Sprinkler"
        sprinkler.parents, sprinkler.upgrades = { tree.parent }, {}
        sprinkler.barrels[1].spread = 0.5
        tankyard.define_class(sprinkler)
        tankyard.set_rules({ movement_multiplier = 1.25 })
        tankyard.mode_name("Sprinkler Party")
        tankyard.on("kill", function(e) tankyard.heal(e.actor, 10) end)
        "#,
    )
    .unwrap();
    let loaded = load(std::slice::from_ref(&root)).unwrap();
    let content = &loaded.bundle.content;
    assert!(content.class("sprinkler").is_some());
    assert!(
        content
            .class("vector")
            .unwrap()
            .upgrades
            .contains(&"sprinkler".to_owned())
    );
    assert_eq!(loaded.bundle.rules.movement_multiplier, 1.25);
    assert_eq!(loaded.bundle.mode_name.as_deref(), Some("Sprinkler Party"));
    let mut runtime = ModRuntime::new(&loaded);
    install(&mut runtime, &loaded, 3).unwrap();
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn every_builtin_class_and_map_round_trips_through_lua_tables() {
    let content = Content::builtin();
    let script = Script::new(&info("copies"), &pack(""), 0, &content, true).unwrap();
    let lua = &script.sandbox.lua;
    for class in &content.classes {
        let value = lua.to_value(class).unwrap();
        let back: ClassDef = lua.from_value(value).unwrap();
        assert_eq!(
            back.behavior_fingerprint(),
            class.behavior_fingerprint(),
            "{}",
            class.id
        );
    }
    for map in tankyard_content::builtin_maps() {
        let value = lua.to_value(&map).unwrap();
        let back: MapDef = lua.from_value(value).unwrap();
        assert_eq!(
            serde_json::to_value(&back).unwrap(),
            serde_json::to_value(&map).unwrap()
        );
    }
}

#[test]
fn scripts_style_the_menu_theme_and_wording() {
    let definitions = define(
        &info("styled"),
        &pack(
            r##"
            tankyard.menu({ title = "OVERCLOCK", splashes = { "Hot!", "Fresh!" } })
            tankyard.menu({ subtitle = "Rules change", showcase = "vector", ambient = false })
            tankyard.theme({ accent = "#b14cff", text = { 240, 230, 255 } })
            tankyard.text({ ["Play offline"] = "Solo run" })
            "##,
        ),
        &Content::builtin(),
    )
    .unwrap();
    let p = definitions.presentation;
    assert_eq!(p.menu.title.as_deref(), Some("OVERCLOCK"));
    assert_eq!(p.menu.subtitle.as_deref(), Some("Rules change"));
    assert_eq!(p.menu.splashes, ["Hot!", "Fresh!"]);
    assert_eq!(p.menu.ambient, Some(false));
    assert_eq!(
        p.theme.accent,
        Some(tankyard_mods::presentation::Rgb([0xb1, 0x4c, 0xff]))
    );
    assert_eq!(
        p.theme.text,
        Some(tankyard_mods::presentation::Rgb([240, 230, 255]))
    );
    assert_eq!(p.text["Play offline"], "Solo run");

    let error = define(
        &info("bad"),
        &pack(r#"tankyard.theme({ accent = "purple" })"#),
        &Content::builtin(),
    )
    .unwrap_err();
    assert!(error.contains("theme"), "{error}");
}

#[test]
fn tank_queries_match_the_world() {
    let (world, runtime) = run(
        r#"
        tankyard.on("tick", function()
          for _, tank in ipairs(tankyard.tanks()) do
            local again = tankyard.tank(tank.id)
            assert(again.name == tank.name and again.x == tank.x and #again.stats == 8)
            assert(type(tank.alive) == "boolean" and type(tank.in_base) == "boolean")
            tankyard.set_score(tank.id, 4321)
          end
        end)
        "#,
        3,
    );
    assert!(runtime.errors.is_empty(), "{:?}", runtime.errors);
    assert_eq!(world.tanks[0].score, 4321);
}

/// Packs written for the LuaJIT sandbox keep their exact results on Luau.
#[test]
fn luajit_semantics_are_preserved() {
    let source = r##"
        assert(_VERSION == "Lua 5.1")
        -- Lua 5.3 bitwise operators with LuaJIT's signed 32-bit results.
        assert((5 & 3) == 1 and (5 | 3) == 7 and (5 ~ 3) == 6 and ~5 == -6)
        assert(1 << 31 == -2147483648 and -1 >> 28 == 15 and (2^32 | 1) == 1)
        assert((5 & 3 | 8 ~ 1) == 9 and (1 << 2 + 1) == 8 and (0xff & ~0xf) == 240)
        assert(not pcall(function() return "3" | 0 end))
        assert(bit.tobit(2^32 + 5) == 5 and bit.tobit(2.5) == 2 and bit.tobit(3.5) == 4)
        assert(bit.band(-1, 0xff) == 255 and bit.bor(1, 2, 4) == 7 and bit.bxor(5, 3, 1) == 7)
        assert(bit.lshift(1, 33) == 2 and bit.arshift(-256, 4) == -16 and bit.rol(0x12345678, 8) == 0x34567812)
        assert(bit.tohex(255) == "000000ff" and bit.tohex(-1) == "ffffffff" and bit.tohex(255, -4) == "00FF")
        assert(bit.bswap(0x12345678) == 0x78563412 and bit.tobit("12") == 12)
        -- goto as loop continue.
        local sum = 0
        for i = 1, 5 do
          if i % 2 == 0 then goto continue end
          sum = sum + i
          ::continue::
        end
        assert(sum == 9)
        -- Number formatting and parsing.
        assert(tostring(0.1) == "0.1" and tostring(1e15) == "1e+15" and tostring(2^53) == "9.007199254741e+15")
        assert(tostring(-0.0) == "-0" and tostring(1/0) == "inf" and tostring(100 / 3) == "33.333333333333")
        assert(0x1p4 == 16 and 0xA.8p0 == 10.5 and 0x1P-2 == 0.25)
        assert(string.format("%s|%5.1f|%q", 1.5, 2.25, "a\r") == '1.5|  2.2|"a\\13"')
        -- Library details.
        assert(math.max(0/0, 1) == 1 and math.min(0/0, 1) == 1 and math.max(1, 0/0) ~= math.max(1, 0/0))
        assert(1 / math.max(0, -0.0) == -1/0 and 1 / math.min(-0.0, 0) == 1/0)
        local t = setmetatable({1, 2}, { __len = function() return 42 end, __tostring = function() return 5 end })
        assert(#t == 2 and tostring(t) == 5)
        assert(select(1, ("hello"):gsub("l", "%a")) == "heaao")
        local records = {}
        for i = 1, 40 do records[i] = { id = i, k = (i * 7919) % 5 } end
        table.sort(records, function(a, b) return a.k < b.k end)
        local order = {}
        for i = 1, 8 do order[i] = records[i].id end
        assert(table.concat(order, ",") == "40,25,15,30,35,10,20,5", table.concat(order, ","))
        -- Out-of-range integer arguments saturate on every CPU.
        assert(("abc"):sub(2^31) == "" and ("abc"):sub(-2^31) == "abc" and string.sub("abc", 0/0) == "abc")
        assert(string.byte("abc", 2^40) == nil and math.ldexp(1, 2^31) == math.huge and select("#", select(2^31, 1)) == 0)
        local shifted = { 10, 20, 30 }
        table.insert(shifted, 0, 5)
        assert(shifted[0] == 5 and shifted[1] == nil and shifted[4] == 30)
        -- Library tables stay writable, and overrides are honored even where
        -- Luau would otherwise call the builtin directly.
        math.floor = function() return "mine" end
        assert(math.floor(1.5) == "mine")
        function string.trim(s) return (s:gsub("^%s+", ""):gsub("%s+$", "")) end
        assert(("  x "):trim() == "x")
        require("other")
        assert(shared_counter == 1)
        bump()
        assert(shared_counter == 2)
        tankyard.on("tick", function() tankyard.state.ok = true end)
    "##;
    let mut scripts = pack(source);
    scripts.sources.insert(
        "other.lua".into(),
        "shared_counter = 1 function bump() shared_counter = shared_counter + 1 end".into(),
    );
    let mut world = world();
    world.add_player("Pilot", false);
    let mut runtime = ModRuntime::default();
    runtime
        .install(Box::new(
            LuaPlugin::new(&info("parity"), &scripts, &Content::builtin(), 7).unwrap(),
        ))
        .unwrap();
    world.step_with(|w| runtime.before_step(w));
    runtime.after_step(&mut world);
    assert!(runtime.errors.is_empty(), "{:?}", runtime.errors);
}

/// A pack that reaches the global table directly still sees every write.
#[test]
fn packs_using_the_global_table_see_every_global_write() {
    let source = r#"
        _G.answer = 1
        local function read() return answer end
        _G["answer"] = 2
        assert(read() == 2)
        rawset(_G, "math", setmetatable({ floor = function() return 7 end }, { __index = math }))
        assert(math.floor(1.5) == 7 and math.sqrt(4) == 2)
        tankyard.on("tick", function() end)
    "#;
    LuaPlugin::new(&info("globals"), &pack(source), &Content::builtin(), 1).unwrap();
}

#[test]
fn scripts_show_hud_widgets_to_everyone_or_one_player() {
    let (world, runtime) = run(
        r##"
        tankyard.on("tick", function(event)
          local players = tankyard.players()
          tankyard.hud("wave", { text = "Wave " .. 3, y = 0.1, color = "#ff8800", size = 24 })
          tankyard.hud("charge", { progress = 0.5, bar_color = { 0, 128, 255 }, align = "left", x = 0 }, players[1].id)
          tankyard.hud("gone", { text = "temporary" })
          tankyard.hud("gone", nil)
        end)
        "##,
        2,
    );
    assert!(runtime.errors.is_empty(), "{:?}", runtime.errors);
    let player = world.tanks.iter().find(|t| !t.bot).unwrap().id;
    let mine = world.hud_for(player);
    assert_eq!(mine.len(), 2);
    let wave = mine.iter().find(|w| w.text == "Wave 3").unwrap();
    assert_eq!(wave.color, Some([255, 136, 0]));
    assert_eq!((wave.y, wave.size), (0.1, 24.));
    let charge = mine.iter().find(|w| w.progress.is_some()).unwrap();
    assert_eq!(charge.bar_color, Some([0, 128, 255]));
    assert_eq!(charge.align, tankyard_mods::HudAlign::Left);
    assert_eq!(world.hud_for(player + 1000).len(), 1);

    for (source, error) in [
        (r#"tankyard.hud("Bad Id", { text = "x" })"#, "widget ids"),
        (
            r#"tankyard.hud("a", { text = "x", x = 2 })"#,
            "x must be between",
        ),
        (
            r#"tankyard.hud("a", { text = "x", color = "orange" })"#,
            "#rrggbb",
        ),
        (r#"tankyard.hud("a", {})"#, "needs text, progress"),
        (
            r#"for i = 1, 17 do tankyard.hud("w" .. i, { text = "x" }) end"#,
            "16 per player",
        ),
    ] {
        let (world, runtime) = run(&format!("tankyard.on('tick', function() {source} end)"), 1);
        let errors = runtime.errors.join(" | ");
        assert!(errors.contains(error), "{source}: {errors}");
        assert!(world.hud.is_empty(), "{source}");
    }
}

#[test]
fn map_events_reach_scripts_and_scripts_drive_map_logic() {
    use tankyard_content::{MapDef, MapTrigger, TriggerAction, TriggerWhen};
    let source = r#"
        tankyard.on("map_event", function(e)
          assert(e.name == "door", "name " .. tostring(e.name))
          assert(e.value == 1, "trigger " .. tostring(e.value))
          assert(#e.tanks == 1 and e.tanks[1] == e.actor, "tanks")
          tankyard.add_map_counter(2, 5)
          tankyard.show_map_group(7, false)
          tankyard.map_signal(4)
        end)
        tankyard.on("tick", function()
          if tankyard.map_counter(2) == 5 and not tankyard.map_group(7) then
            tankyard.announce("seen")
          end
        end)
    "#;
    let mut world = world();
    let pilot = world.add_player("Pilot", false);
    let middle = [0.4, 0.4, 0.2, 0.2];
    world.map.triggers = vec![
        MapTrigger {
            when: TriggerWhen::Start,
            action: TriggerAction::Script,
            rect: [0., 0., 1., 1.],
            seconds: 0.,
            text: "door".into(),
            ..Default::default()
        },
        MapTrigger {
            when: TriggerWhen::Signal,
            action: TriggerAction::Count,
            rect: middle,
            source: 4,
            group: 3,
            ..Default::default()
        },
    ];
    let mut runtime = ModRuntime::default();
    runtime
        .install(Box::new(
            LuaPlugin::new(&info("test"), &pack(source), &Content::builtin(), 7).unwrap(),
        ))
        .unwrap();
    for _ in 0..4 {
        world.step_with(|w| runtime.before_step(w));
        runtime.after_step(&mut world);
    }
    assert!(runtime.errors.is_empty(), "{:?}", runtime.errors);
    assert!(world.tanks.iter().any(|t| t.id == pilot));
    assert_eq!(world.match_state.counters[1..3], [5, 1]);
    assert_ne!(world.match_state.hidden_groups & MapDef::group_bit(7), 0);
    assert_eq!(world.match_state.announcement, "seen");

    for (call, error) in [
        (
            "tankyard.map_signal(33)",
            "channel must be a whole number from 1 to 32",
        ),
        (
            "tankyard.set_map_counter(0, 1)",
            "counter must be a whole number",
        ),
        (
            "tankyard.add_map_counter(1, 1.5)",
            "value must be a whole number",
        ),
        (
            "tankyard.show_map_group(2, 'yes')",
            "shown must be true or false",
        ),
        ("tankyard.map_counter(9)", "counter must be a whole number"),
        (
            "tankyard.reset_map_triggers(33)",
            "group must be a whole number from 0 to 32",
        ),
    ] {
        let (_, runtime) = run(&format!("tankyard.on('tick', function() {call} end)"), 1);
        let errors = runtime.errors.join(" | ");
        assert!(errors.contains(error), "{call}: {errors}");
    }
}

#[test]
fn scripts_rearm_once_only_map_triggers() {
    use tankyard_content::{MapTrigger, TriggerAction, TriggerWhen};
    // A once-only Start trigger counts at once; the script resets it a few
    // ticks later and it counts again.
    let source = r#"
        local rearmed = false
        tankyard.on("tick", function()
          if not rearmed and tankyard.map_counter(1) == 1 then
            rearmed = true
            tankyard.reset_map_triggers(5)
          end
        end)
    "#;
    let mut world = world();
    world.map.triggers = vec![MapTrigger {
        when: TriggerWhen::Start,
        action: TriggerAction::Count,
        rect: [0.4, 0.4, 0.2, 0.2],
        seconds: 0.,
        once: true,
        member: 5,
        group: 1,
        ..Default::default()
    }];
    let mut runtime = ModRuntime::default();
    runtime
        .install(Box::new(
            LuaPlugin::new(&info("test"), &pack(source), &Content::builtin(), 7).unwrap(),
        ))
        .unwrap();
    for _ in 0..6 {
        world.step_with(|w| runtime.before_step(w));
        runtime.after_step(&mut world);
    }
    assert!(runtime.errors.is_empty(), "{:?}", runtime.errors);
    assert_eq!(world.match_state.counters[0], 2);
}
