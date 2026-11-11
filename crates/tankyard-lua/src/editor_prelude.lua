-- The Arena Workshop's plugin API. Packs register tools and checks with
-- `editor.tool` and `editor.check`; the host reads them back and calls
-- `run` and `check` with a fresh copy of the map each time.
local tools, checks = {}, {}
local type, error, tostring, select = type, error, tostring, select

local function text(value, what, limit, level)
  if type(value) ~= "string" or #value == 0 or #value > limit then
    error(what .. " must be text of 1 to " .. limit .. " characters", level)
  end
end

local function registered(list, spec, kind)
  if type(spec) ~= "table" then
    error("editor." .. kind .. " takes a table", 3)
  end
  text(spec.id, kind .. " id", 48, 4)
  text(spec.name, kind .. " name", 48, 4)
  if spec.description ~= nil then
    text(spec.description, kind .. " description", 200, 4)
  end
  if type(spec.run) ~= "function" then
    error(kind .. " " .. spec.id .. " needs a run function", 3)
  end
  for _, other in ipairs(list) do
    if other.id == spec.id then
      error("two " .. kind .. "s are called " .. spec.id, 3)
    end
  end
  if #list >= 32 then
    error("a pack can add at most 32 " .. kind .. "s", 3)
  end
  list[#list + 1] = spec
end

editor = {}

function editor.tool(spec)
  registered(tools, spec, "tool")
  if spec.params ~= nil and type(spec.params) ~= "table" then
    error("tool " .. spec.id .. ": params must be a list", 2)
  end
end

function editor.check(spec)
  registered(checks, spec, "check")
end

-- Maps keep rectangles as fractions of the arena; tools usually think in
-- world units.
function editor.rect(map, x, y, w, h)
  return { x / map.width, y / map.height, w / map.width, h / map.height }
end

function editor.point(map, x, y)
  return { x / map.width, y / map.height }
end

function editor.world(map, r)
  if #r == 2 then
    return r[1] * map.width, r[2] * map.height
  end
  return r[1] * map.width, r[2] * map.height, r[3] * map.width, r[4] * map.height
end

-- The lowest unused number, or nil when every one is taken.
local function lowest_free(used, last)
  for n = 1, last do
    if not used[n] then
      return n
    end
  end
  return nil
end

-- A group (1-32) no object or trigger uses yet.
function editor.free_group(map)
  local used = {}
  for _, list in ipairs({ map.blocks, map.zones, map.decor }) do
    for _, object in ipairs(list or {}) do
      used[object.group or 0] = true
    end
  end
  for _, trigger in ipairs(map.triggers or {}) do
    used[trigger.member or 0] = true
    if trigger.action == "toggle" or trigger.action == "show" or trigger.action == "hide" then
      used[trigger.group or 0] = true
    end
  end
  for _, group in ipairs(map.hidden_groups or {}) do
    used[group] = true
  end
  return lowest_free(used, 32)
end

-- A signal channel (1-32) nothing sends or listens on yet.
function editor.free_channel(map)
  local used = {}
  for _, trigger in ipairs(map.triggers or {}) do
    if trigger.when == "signal" then
      used[trigger.source or 0] = true
    end
    if trigger.action == "signal" then
      used[trigger.group or 0] = true
    elseif trigger.action == "random" then
      for n = trigger.group or 0, (trigger.group or 0) + (trigger.amount or 1) - 1 do
        used[n] = true
      end
    end
  end
  return lowest_free(used, 32)
end

-- A counter (1-8) no trigger reads or changes yet.
function editor.free_counter(map)
  local used = {}
  for _, trigger in ipairs(map.triggers or {}) do
    if trigger.when == "counter" then
      used[trigger.source or 0] = true
    end
    if trigger.action == "count" or trigger.action == "set" or trigger.action == "calculate" then
      used[trigger.group or 0] = true
      used[trigger.operand or 0] = true
    end
    local test = trigger.only_if
    if test and (test.kind or "counter") == "counter" then
      used[test.index or 0] = true
      used[test.against or 0] = true
    end
  end
  return lowest_free(used, 8)
end

local host = {}

function host.tools()
  return tools
end

function host.checks()
  return checks
end

function host.run(index, map, params, context)
  local chosen, messages = nil, {}
  context.select = function(list, at)
    if type(list) ~= "string" or type(at) ~= "number" then
      error("select takes a list name and an index", 2)
    end
    chosen = chosen or {}
    if #chosen < 4096 then
      chosen[#chosen + 1] = { list = list, index = at }
    end
  end
  context.say = function(...)
    if #messages < 8 and select("#", ...) > 0 then
      messages[#messages + 1] = tostring((...))
    end
  end
  local result = tools[index].run(map, params, context)
  if result ~= nil then
    map = result
  end
  return map, chosen, messages
end

function host.check(index, map)
  return checks[index].run(map)
end

return host
