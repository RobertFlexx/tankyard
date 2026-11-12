-- Logic Kit: Arena Workshop tools that wire triggers together. Each tool
-- edits the map it is handed; the workshop checks the result and keeps it as
-- one undo step.

local function inside(r)
  return r[1] >= 0 and r[2] >= 0 and r[1] + r[3] <= 1 and r[2] + r[4] <= 1
end

-- A square of `size` world units centred on (x, y), pulled inside the arena.
local function square(map, x, y, size)
  local half = size / 2
  x = math.min(math.max(x - half, 0), map.width - size)
  y = math.min(math.max(y - half, 0), map.height - size)
  return editor.rect(map, x, y, size, size)
end

local function add(map, ctx, list, object)
  map[list] = map[list] or {}
  table.insert(map[list], object)
  ctx.select(list, #map[list])
end

-- The blocks in the selection, kept in their group when they share their
-- own one, or else given a free group so they open as one. Returns the
-- blocks and the group, or nil and the reason.
local function door_blocks(map, ctx)
  local door = {}
  for _, entry in ipairs(ctx.selection) do
    if entry.list == "blocks" and map.blocks[entry.index] then
      door[#door + 1] = map.blocks[entry.index]
    end
  end
  if #door == 0 then
    return nil, "Select the blocks that make the door first."
  end
  local group = door[1].group or 0
  for _, block in ipairs(door) do
    if (block.group or 0) ~= group then
      group = 0
    end
  end
  if group == 0 then
    group = editor.free_group(map)
    if not group then
      return nil, "All 32 groups are in use."
    end
    for _, block in ipairs(door) do
      block.group = group
    end
  end
  return door, group
end

editor.tool {
  id = "switch_door",
  name = "Switch door",
  description = "Turns the selected blocks into a door and places switches that open it.",
  params = {
    { id = "switches", name = "Switches", type = "integer", min = 1, max = 8, default = 3 },
    { id = "mode", name = "Opens when", type = "choice", options = { "All are pressed", "Any is pressed" }, default = "All are pressed" },
    { id = "spacing", name = "Switch spacing", type = "number", min = 120, max = 1600, step = 20, default = 360 },
    { id = "open", name = "Stays open (s, 0 for good)", type = "number", min = 0, max = 600, step = 1, default = 0 },
  },
  run = function(map, params, ctx)
    local door, group = door_blocks(map, ctx)
    if not door then
      ctx.say(group)
      return
    end
    local all = params.mode == "All are pressed"
    local counter
    if all then
      counter = editor.free_counter(map)
      if not counter then
        ctx.say("All eight counters are in use.")
        return
      end
    end
    local cx, cy = ctx.pointer.x, ctx.pointer.y
    for i = 1, params.switches do
      local x = cx + (i - (params.switches + 1) / 2) * params.spacing
      local switch = { rect = square(map, x, cy, 120), seconds = 1 }
      if all then
        switch.when, switch.action = "enter", "count"
        switch.group, switch.amount, switch.once = counter, 1, true
      else
        switch.when, switch.action = "enter", "hide"
        switch.group, switch.duration = group, params.open
      end
      add(map, ctx, "triggers", switch)
    end
    if all then
      add(map, ctx, "triggers", {
        when = "counter", action = "hide", rect = square(map, cx, cy - 160, 80),
        source = counter, threshold = params.switches, group = group, duration = params.open,
      })
      local r = editor.rect(map, math.max(cx - 160, 0), math.max(cy - 260, 0), 320, 60)
      if inside(r) then
        add(map, ctx, "decor", {
          kind = "label", rect = r, text = ("Switches {%d}/%d"):format(counter, params.switches),
        })
      end
    end
    ctx.say(("Door is group %d; %d switch%s placed."):format(group, params.switches,
      params.switches == 1 and "" or "es"))
  end,
}

editor.tool {
  id = "countdown",
  name = "Countdown",
  description = "Counts a free counter down once a second from the round's start and announces a message at zero.",
  params = {
    { id = "seconds", name = "Seconds", type = "integer", min = 3, max = 3600, default = 60 },
    { id = "message", name = "Message at zero", type = "text", limit = 96, default = "Time is up!" },
    { id = "label", name = "Show it on a label", type = "toggle", default = true },
  },
  run = function(map, params, ctx)
    local counter = editor.free_counter(map)
    if not counter then
      ctx.say("All eight counters are in use.")
      return
    end
    local message = params.message:gsub("^%s+", ""):gsub("%s+$", "")
    if message == "" then
      message = "Time is up!"
    end
    local cx, cy = ctx.pointer.x, ctx.pointer.y
    add(map, ctx, "triggers", {
      when = "start", action = "set", rect = square(map, cx, cy - 240, 80),
      seconds = 0, group = counter, amount = params.seconds,
    })
    add(map, ctx, "triggers", {
      when = "every", action = "calculate", rect = square(map, cx, cy, 80),
      seconds = 1, group = counter, math = "subtract", amount = 1,
      only_if = { kind = "counter", index = counter, compare = "above", value = 0 },
    })
    add(map, ctx, "triggers", {
      when = "counter", action = "announce", rect = square(map, cx, cy + 240, 80),
      source = counter, threshold = 0, text = message,
    })
    if params.label then
      local r = editor.rect(map, math.max(cx + 120, 0), math.max(cy - 30, 0), 160, 60)
      if inside(r) then
        add(map, ctx, "decor", { kind = "label", rect = r, text = ("{%d}"):format(counter) })
      end
    end
    ctx.say(("Counter %d counts down from %d."):format(counter, params.seconds))
  end,
}

editor.tool {
  id = "sequence",
  name = "Sequence",
  description = "Makes the selected triggers act one after another, a set time apart, when a new plate is entered.",
  params = {
    { id = "gap", name = "Seconds apart", type = "number", min = 0, max = 60, step = 0.5, default = 1 },
  },
  run = function(map, params, ctx)
    local chosen = {}
    for _, entry in ipairs(ctx.selection) do
      if entry.list == "triggers" and map.triggers[entry.index] then
        chosen[#chosen + 1] = map.triggers[entry.index]
      end
    end
    if #chosen < 2 then
      ctx.say("Select two or more triggers to run in sequence.")
      return
    end
    local channel = editor.free_channel(map)
    if not channel then
      ctx.say("All 32 signal channels are in use.")
      return
    end
    for step, trigger in ipairs(chosen) do
      trigger.when, trigger.source = "signal", channel
      trigger.delay = math.min((step - 1) * params.gap, 3600)
    end
    local cx, cy = ctx.pointer.x, ctx.pointer.y
    add(map, ctx, "triggers", {
      when = "enter", action = "signal", rect = square(map, cx, cy, 160),
      seconds = math.min((#chosen - 1) * params.gap + 1, 3600), group = channel,
    })
    ctx.say(("Entering the new plate runs %d triggers on signal %d."):format(#chosen, channel))
  end,
}

local GATES = {
  ["All are on (AND)"] = "all",
  ["Any is on (OR)"] = "any",
  ["Just one is on (XOR)"] = "one",
}

editor.tool {
  id = "lever_bank",
  name = "Lever bank",
  description = "Places levers beside the pointer and wires a gate that opens the selected blocks while the levers are set right.",
  params = {
    { id = "levers", name = "Levers", type = "integer", min = 2, max = 4, default = 3 },
    { id = "gate", name = "Door opens when", type = "choice",
      options = { "All are on (AND)", "Any is on (OR)", "Just one is on (XOR)" },
      default = "All are on (AND)" },
    { id = "spacing", name = "Lever spacing", type = "number", min = 120, max = 1200, step = 20, default = 300 },
    { id = "labels", name = "Label each lever", type = "toggle", default = true },
  },
  run = function(map, params, ctx)
    local door, group = door_blocks(map, ctx)
    if not door then
      ctx.say(group)
      return
    end
    local cx, cy = ctx.pointer.x, ctx.pointer.y
    -- Each lever flips a counter of its own; a change sends the check signal.
    local open = editor.free_channel(map)
    local tests = {}
    for i = 1, params.levers do
      local counter = editor.free_counter(map)
      if not counter or not open then
        ctx.say("There are not enough free counters or signal channels left.")
        return
      end
      local x = cx + (i - (params.levers + 1) / 2) * params.spacing
      add(map, ctx, "triggers", {
        when = "enter", action = "calculate", math = "flip", rect = square(map, x, cy, 120),
        seconds = 1, group = counter,
      })
      add(map, ctx, "triggers", {
        when = "changed", action = "signal", rect = square(map, x, cy + 140, 60),
        source = counter, group = open,
      })
      tests[#tests + 1] = { kind = "counter", index = counter, compare = "at_least", value = 1, against = 0 }
      if params.labels then
        local r = editor.rect(map, math.max(x - 70, 0), math.max(cy - 120, 0), 140, 50)
        if inside(r) then
          add(map, ctx, "decor", { kind = "label", rect = r, text = ("Lever {%d}"):format(counter) })
        end
      end
    end
    -- The gate hides the door while it holds; otherwise it sends the
    -- closing signal, which shows the door again.
    local gate = {
      when = "signal", action = "hide", rect = square(map, cx, cy - 220, 80),
      source = open, group = group, gate = GATES[params.gate] or "all",
      only_if = tests[1], also_if = {},
    }
    for i = 2, #tests do
      gate.also_if[#gate.also_if + 1] = tests[i]
    end
    add(map, ctx, "triggers", gate)
    local close = editor.free_channel(map)
    if not close then
      ctx.say("All 32 signal channels are in use.")
      return
    end
    gate.otherwise = close
    add(map, ctx, "triggers", {
      when = "signal", action = "show", rect = square(map, cx + 120, cy - 220, 80),
      source = close, group = group,
    })
    ctx.say(("Door is group %d. %d levers; it opens when %s."):format(group, params.levers,
      (params.gate:lower():gsub(" %(.*%)", ""))))
  end,
}

-- Whether `text` shows counter `n` as {n}.
local function shows(text, n)
  return text ~= nil and text:find("{" .. n .. "}", 1, true) ~= nil
end

editor.check {
  id = "unread_counters",
  name = "Unread counters",
  run = function(map)
    local first, read = {}, {}
    for index, trigger in ipairs(map.triggers or {}) do
      local action = trigger.action
      if (action == "count" or action == "set" or action == "calculate") and not first[trigger.group] then
        first[trigger.group] = index
      end
      if trigger.when == "counter" or trigger.when == "changed" then
        read[trigger.source or 0] = true
      end
      if action == "calculate" then
        read[trigger.operand or 0] = true
      end
      for _, test in ipairs(editor.conditions(trigger)) do
        if (test.kind or "counter") == "counter" then
          read[test.index or 0] = true
        end
        read[test.against or 0] = true
      end
    end
    local found = {}
    for counter = 1, 8 do
      local index = first[counter]
      local shown = false
      for _, item in ipairs(map.decor or {}) do
        shown = shown or shows(item.text, counter)
      end
      for _, trigger in ipairs(map.triggers or {}) do
        shown = shown or (trigger.action == "announce" and shows(trigger.text, counter))
      end
      if index and not read[counter] and not shown then
        found[#found + 1] = {
          text = ("Counter %d changes, but nothing reads it: no watcher, condition, label or announcement uses it. Scripts can still read it with tankyard.map_counter.")
            :format(counter),
          list = "triggers",
          index = index,
        }
      end
    end
    return found
  end,
}
