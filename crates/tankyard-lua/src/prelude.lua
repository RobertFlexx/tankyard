-- Tank Yard script API (version 2). Loaded before every pack script.
-- `native` is supplied by the host; world access exists only during events.
local native, triggers, defining = ...
local type, error, pairs, ipairs, tostring, floor = type, error, pairs, ipairs, tostring, math.floor

local tankyard = { api = 2, state = {} }
local handlers, timers = {}, {}
local definitions = {
  classes = {}, maps = {}, textures = {}, rules = nil, mode_name = nil,
  menu = {}, theme = {}, text = {},
}

local function check(kind, value, call, what)
  if type(value) ~= kind then
    error(("tankyard.%s: %s must be a %s, got %s"):format(call, what, kind, type(value)), 3)
  end
  return value
end
local function optional(kind, value, call, what)
  if value ~= nil then check(kind, value, call, what) end
  return value
end

-- Events ------------------------------------------------------------------

function tankyard.on(event, handler)
  check("string", event, "on", "event")
  check("function", handler, "on", "handler")
  if not triggers[event] then
    error(("tankyard.on: unknown event %q"):format(event), 2)
  end
  local list = handlers[event]
  if not list then
    list = {}
    handlers[event] = list
  end
  if #list >= 32 then error("tankyard.on: at most 32 handlers per event", 2) end
  list[#list + 1] = handler
end

function tankyard.every(ticks, handler)
  check("number", ticks, "every", "interval")
  check("function", handler, "every", "handler")
  ticks = floor(ticks)
  if ticks < 1 or ticks > 216000 then error("tankyard.every: interval must be 1..216000 ticks", 2) end
  if #timers >= 64 then error("tankyard.every: at most 64 timers", 2) end
  timers[#timers + 1] = { ticks = ticks, handler = handler }
end

-- World queries -------------------------------------------------------------

local function read(call, ...)
  local reader = native.read
  if not reader then
    error(("tankyard.%s() is only available inside event handlers"):format(call), 3)
  end
  return reader(call, ...)
end

function tankyard.tick() return read("tick") end
function tankyard.time() return read("time") end
function tankyard.arena() return read("arena") end
function tankyard.tanks() return read("tanks") end
function tankyard.players() return read("players") end
function tankyard.tank(id) return read("tank", check("number", id, "tank", "id")) end
function tankyard.bosses() return read("bosses") end
function tankyard.walls() return read("walls") end
function tankyard.objectives() return read("objectives") end
function tankyard.classes() return read("classes") end
function tankyard.class(id) return read("class", check("string", id, "class", "class id")) end
function tankyard.base(team) return read("base", check("number", team, "base", "team")) end
function tankyard.shapes(x, y, radius)
  return read("shapes", check("number", x, "shapes", "x"), check("number", y, "shapes", "y"),
    check("number", radius, "shapes", "radius"))
end
function tankyard.projectiles(x, y, radius)
  return read("projectiles", check("number", x, "projectiles", "x"),
    check("number", y, "projectiles", "y"), check("number", radius, "projectiles", "radius"))
end

-- Nearest living tank to a point, optionally excluding a team or an ID.
function tankyard.nearest(x, y, options)
  check("number", x, "nearest", "x")
  check("number", y, "nearest", "y")
  options = optional("table", options, "nearest", "options") or {}
  local best, best_distance = nil, math.huge
  for _, tank in ipairs(tankyard.tanks()) do
    if tank.alive and tank.id ~= options.exclude and (options.enemy_of == nil
        or tank.team == 0 or tank.team ~= options.enemy_of) and (options.team == nil
        or tank.team == options.team) and not (options.humans and tank.bot) then
      local dx, dy = tank.x - x, tank.y - y
      local distance = dx * dx + dy * dy
      if distance < best_distance then
        best, best_distance = tank, distance
      end
    end
  end
  return best, best and math.sqrt(best_distance) or nil
end

function tankyard.distance(a, b)
  check("table", a, "distance", "first point")
  check("table", b, "distance", "second point")
  local dx, dy = (a.x or 0) - (b.x or 0), (a.y or 0) - (b.y or 0)
  return math.sqrt(dx * dx + dy * dy)
end

-- Commands -------------------------------------------------------------------

local function emit(call, ...)
  local emitter = native.emit
  if not emitter then
    error(("tankyard.%s() is only available inside event handlers"):format(call), 3)
  end
  emitter(call, ...)
end
local function id(call, value) return check("number", value, call, "tank id") end
local function number(call, value, what) return check("number", value, call, what) end

function tankyard.heal(tank, amount) emit("heal", id("heal", tank), number("heal", amount, "amount")) end
function tankyard.damage(tank, amount) emit("damage", id("damage", tank), number("damage", amount, "amount")) end
function tankyard.kill(tank) emit("kill", id("kill", tank)) end
function tankyard.award(tank, xp, score)
  emit("award", id("award", tank), number("award", xp or 0, "xp"), number("award", score or 0, "score"))
end
function tankyard.set_class(tank, class) emit("set_class", id("set_class", tank), check("string", class, "set_class", "class id")) end
function tankyard.set_team(tank, team) emit("set_team", id("set_team", tank), number("set_team", team, "team")) end
function tankyard.set_level(tank, level) emit("set_level", id("set_level", tank), number("set_level", level, "level")) end
function tankyard.raise_stat(tank, stat)
  if type(stat) ~= "number" and type(stat) ~= "string" then
    error("tankyard.raise_stat: stat must be a number 1..8 or a stat name", 2)
  end
  emit("raise_stat", id("raise_stat", tank), stat)
end
function tankyard.set_scales(tank, scales)
  check("table", scales, "set_scales", "scales")
  emit("set_scales", id("set_scales", tank), scales.speed, scales.reload, scales.damage)
end
function tankyard.set_armor(tank, multiplier) emit("set_armor", id("set_armor", tank), number("set_armor", multiplier, "multiplier")) end
function tankyard.set_shield(tank, seconds) emit("set_shield", id("set_shield", tank), number("set_shield", seconds, "seconds")) end
function tankyard.set_protection(tank, seconds) emit("set_protection", id("set_protection", tank), number("set_protection", seconds, "seconds")) end
function tankyard.push(tank, vx, vy) emit("push", id("push", tank), number("push", vx, "x velocity"), number("push", vy, "y velocity")) end
function tankyard.set_velocity(tank, vx, vy)
  emit("set_velocity", id("set_velocity", tank), number("set_velocity", vx, "x velocity"), number("set_velocity", vy, "y velocity"))
end
function tankyard.teleport(tank, x, y) emit("teleport", id("teleport", tank), number("teleport", x, "x"), number("teleport", y, "y")) end
function tankyard.fire(tank, shot)
  shot = optional("table", shot, "fire", "shot") or {}
  emit("fire", id("fire", tank), shot.speed or 520, shot.damage or 20, shot.radius or 8,
    shot.lifetime or 2, shot.angle or 0)
end
function tankyard.control(tank, input)
  check("table", input, "control", "input")
  emit("control", id("control", tank), input.x or 0, input.y or 0, input.aim, input.fire == true,
    input.ability == true)
end
function tankyard.spawn_shape(x, y, shape)
  shape = optional("table", shape, "spawn_shape", "shape") or {}
  emit("spawn_shape", number("spawn_shape", x, "x"), number("spawn_shape", y, "y"),
    shape.sides or 4, shape.health or 60, shape.xp or 25)
end
function tankyard.spawn_boss(name) emit("spawn_boss", check("string", name, "spawn_boss", "boss name")) end
function tankyard.clear_projectiles(tank) emit("clear_projectiles", tank or 0) end
function tankyard.announce(text) emit("announce", tostring(text)) end
function tankyard.finish_round(team, reason)
  emit("finish_round", optional("number", team, "finish_round", "team") or 0,
    reason == nil and "Round over" or tostring(reason))
end
function tankyard.set_score(tank, score) emit("set_score", id("set_score", tank), number("set_score", score, "score")) end
function tankyard.set_team_score(team, score)
  emit("set_team_score", number("set_team_score", team, "team"), number("set_team_score", score, "score"))
end

-- A color as "#rrggbb" or { r, g, b } (0-255), packed into one number.
local function hud_color(value, what)
  if value == nil then return nil end
  if type(value) == "string" then
    local hex = value:match("^#?(%x%x%x%x%x%x)$")
    if not hex then error(("tankyard.hud: %s must be \"#rrggbb\" or { r, g, b }"):format(what), 3) end
    return tonumber(hex, 16)
  end
  check("table", value, "hud", what)
  local packed = 0
  for index, key in ipairs({ "r", "g", "b" }) do
    local channel = value[index] or value[key]
    if type(channel) ~= "number" or channel < 0 or channel > 255 then
      error(("tankyard.hud: %s needs red, green and blue from 0 to 255"):format(what), 3)
    end
    packed = packed * 256 + math.floor(channel)
  end
  return packed
end
-- Show, change or (with a nil widget) remove a HUD widget, on one tank's
-- screen or, without a tank, on everyone's.
function tankyard.hud(widget_id, widget, tank)
  check("string", widget_id, "hud", "widget id")
  optional("number", tank, "hud", "tank id")
  if widget == nil then
    emit("hud", tank or 0, widget_id)
    return
  end
  check("table", widget, "hud", "widget")
  local text = widget.text
  emit("hud", tank or 0, widget_id, text ~= nil and tostring(text) or "",
    optional("number", widget.x, "hud", "x"), optional("number", widget.y, "hud", "y"),
    optional("string", widget.align, "hud", "align"), optional("number", widget.size, "hud", "size"),
    hud_color(widget.color, "color"), optional("number", widget.progress, "hud", "progress"),
    hud_color(widget.bar_color, "bar_color"), optional("number", widget.width, "hud", "width"))
end

-- Definitions (load phase only; ignored while a round is running) ------------

local function define(call, list, value, limit)
  check("table", value, call, "definition")
  if defining then
    if #list >= limit then error(("tankyard.%s: too many definitions"):format(call), 3) end
    list[#list + 1] = value
  end
  return value
end
function tankyard.define_class(class) return define("define_class", definitions.classes, class, 1024) end
function tankyard.define_map(map) return define("define_map", definitions.maps, map, 256) end
function tankyard.texture(texture_id, file)
  check("string", texture_id, "texture", "texture id")
  check("string", file, "texture", "file")
  if defining then
    if #definitions.textures >= 16 then error("tankyard.texture: at most 16 textures", 2) end
    definitions.textures[#definitions.textures + 1] = { id = texture_id, file = file }
  end
  return texture_id
end
function tankyard.set_rules(rules)
  check("table", rules, "set_rules", "rules")
  if defining then definitions.rules = rules end
end
function tankyard.mode_name(name)
  check("string", name, "mode_name", "name")
  if defining then definitions.mode_name = name end
end

-- The main menu, interface colors and wording on the player's machine. Later
-- calls add to earlier ones.
local function merge(call, target, value)
  check("table", value, call, "settings")
  if defining then
    for key, setting in pairs(value) do target[key] = setting end
  end
end
function tankyard.menu(menu) merge("menu", definitions.menu, menu) end
function tankyard.theme(theme) merge("theme", definitions.theme, theme) end
function tankyard.text(text) merge("text", definitions.text, text) end

-- Editable copies of the catalog the pack loads into (built-in plus earlier packs).
local base_class, base_map = __tankyard_base_class, __tankyard_base_map
__tankyard_base_class, __tankyard_base_map = nil, nil
function tankyard.base_class(class_id)
  return base_class(check("string", class_id, "base_class", "class id"))
end
function tankyard.base_map(map_id)
  return base_map(check("string", map_id, "base_map", "map id"))
end

tankyard.stats = { "health", "regen", "body", "speed", "penetration", "damage", "reload", "movement" }

-- Host side ------------------------------------------------------------------

local host = {}
function host.dispatch(event_name, event)
  local list = handlers[event_name]
  if list then
    for index = 1, #list do list[index](event) end
  end
  if event_name == "tick" then
    local tick = event.tick
    for index = 1, #timers do
      local timer = timers[index]
      if tick % timer.ticks == 0 then timer.handler(tick) end
    end
  end
end
function host.events()
  local names = {}
  for name in pairs(handlers) do names[#names + 1] = name end
  if #timers > 0 then names[#names + 1] = "tick" end
  return names
end
function host.definitions() return definitions end
-- One tank's query table. A constructor with constant keys builds it far
-- faster than the host setting each field.
function host.tank(id, name, class, team, x, y, vx, vy, aim, radius, health, max_health,
    level, xp, score, kills, deaths, alive, bot, points, shield, protection, speed_scale,
    reload_scale, damage_scale, in_base, s1, s2, s3, s4, s5, s6, s7, s8)
  return {
    id = id, name = name, class = class, team = team, x = x, y = y, vx = vx, vy = vy,
    aim = aim, radius = radius, health = health, max_health = max_health, level = level,
    xp = xp, score = score, kills = kills, deaths = deaths, alive = alive, bot = bot,
    points = points, shield = shield, protection = protection, speed_scale = speed_scale,
    reload_scale = reload_scale, damage_scale = damage_scale, in_base = in_base,
    stats = { s1, s2, s3, s4, s5, s6, s7, s8 },
  }
end

_G.tankyard = tankyard
return host
