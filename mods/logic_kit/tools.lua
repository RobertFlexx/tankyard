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
    local door = {}
    for _, entry in ipairs(ctx.selection) do
      if entry.list == "blocks" and map.blocks[entry.index] then
        door[#door + 1] = map.blocks[entry.index]
      end
    end
    if #door == 0 then
      ctx.say("Select the blocks that make the door first.")
      return
    end
    -- Keep the door's group if it already has its own one.
    local group = door[1].group or 0
    for _, block in ipairs(door) do
      if (block.group or 0) ~= group then
        group = 0
      end
    end
    if group == 0 then
      group = editor.free_group(map)
      if not group then
        ctx.say("All 32 groups are in use.")
        return
      end
      for _, block in ipairs(door) do
        block.group = group
      end
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

-- Channels each trigger sends on, when it sends with no delay.
local function sends(trigger)
  if (trigger.delay or 0) > 0 then
    return {}
  end
  if trigger.action == "signal" then
    return { trigger.group }
  elseif trigger.action == "random" then
    local list = {}
    for n = trigger.group, trigger.group + trigger.amount - 1 do
      list[#list + 1] = n
    end
    return list
  end
  return {}
end

editor.check {
  id = "signal_loops",
  name = "Signal loops",
  run = function(map)
    -- next[a][b]: a signal on channel a sets off one on channel b at once.
    local next = {}
    for _, trigger in ipairs(map.triggers or {}) do
      if trigger.when == "signal" then
        for _, channel in ipairs(sends(trigger)) do
          next[trigger.source] = next[trigger.source] or {}
          next[trigger.source][channel] = true
        end
      end
    end
    local function reaches(from, goal)
      local seen, stack = {}, { from }
      while #stack > 0 do
        local at = table.remove(stack)
        if at == goal then
          return true
        end
        if not seen[at] then
          seen[at] = true
          for channel in pairs(next[at] or {}) do
            stack[#stack + 1] = channel
          end
        end
      end
      return false
    end
    local found = {}
    for index, trigger in ipairs(map.triggers or {}) do
      if trigger.when == "signal" then
        for _, channel in ipairs(sends(trigger)) do
          if reaches(channel, trigger.source) then
            found[#found + 1] = {
              text = ("Signal %d sets itself off again with no delay; the chain stops after 256 steps a tick. Add a delay to slow it down.")
                :format(trigger.source),
              list = "triggers",
              index = index,
            }
            break
          end
        end
      end
      if #found >= 8 then
        break
      end
    end
    return found
  end,
}
