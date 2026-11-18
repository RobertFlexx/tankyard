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

-- Every condition of a trigger: only_if, then also_if.
local function conditions(trigger)
  local list = {}
  if trigger.only_if then
    list[1] = trigger.only_if
    for _, test in ipairs(trigger.also_if or {}) do
      list[#list + 1] = test
    end
  end
  return list
end
editor.conditions = conditions

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
    local action = trigger.action
    if action == "toggle" or action == "show" or action == "hide" or action == "reset"
      or action == "move" or action == "place" or action == "turn" or action == "fade"
      or action == "tint" or action == "scale" or action == "animate" or action == "stop" then
      used[trigger.group or 0] = true
    end
    if trigger.when == "shown" or trigger.when == "hidden" or trigger.when == "arrived" then
      used[trigger.source or 0] = true
    end
    for _, test in ipairs(conditions(trigger)) do
      if test.kind == "shown" or test.kind == "hidden" then
        used[test.index or 0] = true
      end
    end
  end
  for _, group in ipairs(map.hidden_groups or {}) do
    used[group] = true
  end
  return lowest_free(used, 32)
end

-- The board cells of a Show UI trigger's widget, or none.
local function board_cells(trigger)
  local ui = trigger.action == "ui" and trigger.ui
  local board = ui and ui.widget and ui.widget.board
  return board and board.cells or {}
end

-- A signal channel (1-255) nothing sends or listens on yet.
function editor.free_channel(map)
  local used = {}
  for _, trigger in ipairs(map.triggers or {}) do
    if trigger.when == "signal" then
      used[trigger.source or 0] = true
    end
    if trigger.action == "signal" then
      used[trigger.group or 0] = true
    elseif trigger.action == "random" or trigger.action == "sequence" then
      for n = trigger.group or 0, (trigger.group or 0) + (trigger.amount or 1) - 1 do
        used[n] = true
      end
    elseif trigger.action == "ui" and trigger.ui then
      for _, channel in ipairs(trigger.ui.channels or {}) do
        used[channel] = true
      end
      for _, cell in ipairs(board_cells(trigger)) do
        if cell.kind == "button" then
          used[cell.channel or 0] = true
        end
      end
    end
    used[trigger.otherwise or 0] = true
  end
  return lowest_free(used, 255)
end

-- Marks the counters `{…}` fields in `text` name, such as {3}, {3:04} and
-- {3|2|1:4}.
local function named_counters(text, used)
  for field in (text or ""):gmatch("{([^{}]*)}") do
    local names = field:match("^([%d|]+)")
    for n in (names or ""):gmatch("%d+") do
      used[tonumber(n)] = true
    end
  end
end

-- A counter (1-255) nothing reads, changes or shows yet.
function editor.free_counter(map)
  local used = {}
  for _, decor in ipairs(map.decor or {}) do
    named_counters(decor.text, used)
  end
  for _, trigger in ipairs(map.triggers or {}) do
    if trigger.when == "counter" or trigger.when == "changed" then
      used[trigger.source or 0] = true
    end
    if trigger.action == "count" or trigger.action == "set" or trigger.action == "calculate" then
      used[trigger.group or 0] = true
      used[trigger.operand or 0] = true
      used[trigger.target_offset or 0] = true
      used[trigger.operand_offset or 0] = true
    end
    for _, test in ipairs(conditions(trigger)) do
      if (test.kind or "counter") == "counter" then
        used[test.index or 0] = true
      end
      used[test.against or 0] = true
    end
    if trigger.action == "announce" then
      named_counters(trigger.text, used)
    end
    local ui = trigger.action == "ui" and trigger.ui
    if ui then
      used[ui.bar or 0] = true
      named_counters(ui.widget and ui.widget.title, used)
      named_counters(ui.widget and ui.widget.text, used)
      for _, cell in ipairs(board_cells(trigger)) do
        used[cell.counter or 0] = true
        named_counters(cell.text, used)
      end
    end
  end
  return lowest_free(used, 255)
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
