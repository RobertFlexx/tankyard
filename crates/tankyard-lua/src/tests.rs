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
