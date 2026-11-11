-- Workshop Toolkit: Arena Workshop tools written against the editor plugin
-- API. Every tool edits the map it is handed; the workshop checks the result
-- and keeps it as one undo step.

local function copy(value)
  if type(value) ~= "table" then
    return value
  end
  local out = {}
  for key, inner in pairs(value) do
    out[key] = copy(inner)
  end
  return out
end

-- Walls are bare rectangles; most other objects keep theirs in `rect`.
local function rect_of(list, object)
  if list == "walls" then
    return object
  end
  return object.rect
end

local function inside(r)
  return r[1] >= 0 and r[2] >= 0 and r[1] + r[3] <= 1 and r[2] + r[4] <= 1
end

-- The selection's objects that have a rectangle, with their lists.
local function selected(map, ctx)
  local out = {}
  for _, entry in ipairs(ctx.selection) do
    local object = map[entry.list] and map[entry.list][entry.index]
    if object and entry.list ~= "team_bases" and entry.list ~= "objective_positions" then
      out[#out + 1] = { list = entry.list, object = object }
    end
  end
  return out
end

editor.tool {
  id = "repeat",
  name = "Repeat",
  description = "Copies the selection several times, each copy one step further along.",
  params = {
    { id = "copies", name = "Copies", type = "integer", min = 1, max = 40, default = 4 },
    { id = "dx", name = "Step across", type = "number", min = -2000, max = 2000, step = 10, default = 160 },
    { id = "dy", name = "Step down", type = "number", min = -2000, max = 2000, step = 10, default = 0 },
  },
  run = function(map, params, ctx)
    local chosen = selected(map, ctx)
    if #chosen == 0 then
      ctx.say("Select something to repeat first.")
      return
    end
    local placed = 0
    for n = 1, params.copies do
      local nx, ny = n * params.dx / map.width, n * params.dy / map.height
      for _, item in ipairs(chosen) do
        local object = copy(item.object)
        local r = rect_of(item.list, object)
        r[1], r[2] = r[1] + nx, r[2] + ny
        if item.list == "zones" and object.kind == "portal" then
          object.exit = { object.exit[1] + nx, object.exit[2] + ny }
        end
        if inside(r) then
          table.insert(map[item.list], object)
          ctx.select(item.list, #map[item.list])
          placed = placed + 1
        end
      end
    end
    ctx.say(("Placed %d copies."):format(placed))
  end,
}

editor.tool {
  id = "ring",
  name = "Ring of blocks",
  description = "Blocks in a circle around the last place you clicked on the canvas.",
  params = {
    { id = "count", name = "Blocks", type = "integer", min = 3, max = 64, default = 12 },
    { id = "radius", name = "Radius", type = "number", min = 40, max = 3000, step = 10, default = 360 },
    { id = "size", name = "Block size", type = "number", min = 10, max = 400, step = 10, default = 80 },
    { id = "style", name = "Style", type = "choice", options = { "map", "slate", "garden", "circuit", "bastion", "relay", "orrery", "sand" }, default = "slate" },
  },
  run = function(map, params, ctx)
    local cx, cy = ctx.pointer.x, ctx.pointer.y
    local half = params.size / 2
    for i = 1, params.count do
      local angle = (i - 1) / params.count * math.pi * 2
      local x = cx + math.cos(angle) * params.radius - half
      local y = cy + math.sin(angle) * params.radius - half
      local r = editor.rect(map, x, y, params.size, params.size)
      if inside(r) then
        table.insert(map.blocks, { rect = r, style = params.style })
        ctx.select("blocks", #map.blocks)
      end
    end
  end,
}

editor.tool {
  id = "frame",
  name = "Arena frame",
  description = "Lines the arena's edges with blocks, leaving a doorway in the middle of each side.",
  params = {
    { id = "thickness", name = "Thickness", type = "number", min = 20, max = 400, step = 10, default = 60 },
    { id = "door", name = "Doorway", type = "number", min = 0, max = 2000, step = 10, default = 320 },
    { id = "style", name = "Style", type = "choice", options = { "map", "slate", "garden", "circuit", "bastion", "relay", "orrery", "sand" }, default = "bastion" },
  },
  run = function(map, params, ctx)
    local t, door = params.thickness, params.door
    local w, h = map.width, map.height
    local function add(x, y, bw, bh)
      if bw > 1 and bh > 1 then
        table.insert(map.blocks, { rect = editor.rect(map, x, y, bw, bh), style = params.style })
        ctx.select("blocks", #map.blocks)
      end
    end
    local across = (w - door) / 2
    local down = (h - 2 * t - door) / 2
    for _, y in ipairs({ 0, h - t }) do
      add(0, y, across, t)
      add(w - across, y, across, t)
    end
    for _, x in ipairs({ 0, w - t }) do
      add(x, t, t, down)
      add(x, h - t - down, t, down)
    end
  end,
}

editor.tool {
  id = "snap",
  name = "Snap to grid",
  description = "Moves the selection onto the editor's grid, and optionally rounds sizes too.",
  params = {
    { id = "sizes", name = "Round sizes too", type = "toggle", default = true },
  },
  run = function(map, params, ctx)
    local grid = ctx.grid
    local function snap(value, scale, least)
      local cells = math.floor(value * scale / grid + 0.5)
      return math.max(cells, least) * grid / scale
    end
    local moved = 0
    for _, item in ipairs(selected(map, ctx)) do
      local r = rect_of(item.list, item.object)
      r[1], r[2] = snap(r[1], map.width, 0), snap(r[2], map.height, 0)
      if params.sizes then
        r[3], r[4] = snap(r[3], map.width, 1), snap(r[4], map.height, 1)
      end
      r[1] = math.min(r[1], 1 - r[3])
      r[2] = math.min(r[2], 1 - r[4])
      moved = moved + 1
    end
    ctx.say(("Snapped %d objects to the %d-unit grid."):format(moved, grid))
  end,
}

editor.tool {
  id = "quad",
  name = "Four-way mirror",
  description = "Copies the selection into the other three quarters of the arena, mirrored.",
  run = function(map, params, ctx)
    local chosen = selected(map, ctx)
    for _, flip in ipairs({ { true, false }, { false, true }, { true, true } }) do
      for _, item in ipairs(chosen) do
        local object = copy(item.object)
        local r = rect_of(item.list, object)
        if flip[1] then
          r[1] = 1 - r[1] - r[3]
        end
        if flip[2] then
          r[2] = 1 - r[2] - r[4]
        end
        if item.list == "zones" then
          if object.kind == "conveyor" then
            local a = object.angle
            if flip[1] then a = 180 - a end
            if flip[2] then a = -a end
            object.angle = a % 360
          elseif object.kind == "portal" then
            if flip[1] then object.exit[1] = 1 - object.exit[1] end
            if flip[2] then object.exit[2] = 1 - object.exit[2] end
          end
        end
        table.insert(map[item.list], object)
        ctx.select(item.list, #map[item.list])
      end
    end
  end,
}

-- A new tank is 44 units across and the heaviest hulls about 70.
local SMALLEST, LARGEST = 46, 70
-- Edges this close touch: rectangles round-trip through fractions.
local TOUCH = 0.5
-- Solids are sorted into cells this big, so each meets only its neighbours.
local CELL = 128

-- Indices of the solids filed in the cells the area x0..x1, y0..y1 covers,
-- each once.
local function nearby(grid, x0, y0, x1, y1)
  local found, seen = {}, {}
  for cx = math.floor(x0 / CELL), math.floor(x1 / CELL) do
    for cy = math.floor(y0 / CELL), math.floor(y1 / CELL) do
      for _, i in ipairs(grid[cx .. "," .. cy] or {}) do
        if not seen[i] then
          seen[i] = true
          found[#found + 1] = i
        end
      end
    end
  end
  return found
end

editor.check {
  id = "narrow_gaps",
  name = "Narrow gaps",
  run = function(map)
    local solids = {}
    local function add(list, index, r)
      local x, y = r[1] * map.width, r[2] * map.height
      solids[#solids + 1] = {
        list = list, index = index,
        x0 = x, y0 = y, x1 = x + r[3] * map.width, y1 = y + r[4] * map.height,
      }
    end
    for i, wall in ipairs(map.walls) do
      add("walls", i, wall)
    end
    for i, block in ipairs(map.blocks) do
      if block.group == 0 or block.group == nil then
        add("blocks", i, block.rect)
      end
    end
    local grid = {}
    for i, s in ipairs(solids) do
      for cx = math.floor(s.x0 / CELL), math.floor(s.x1 / CELL) do
        for cy = math.floor(s.y0 / CELL), math.floor(s.y1 / CELL) do
          local key = cx .. "," .. cy
          grid[key] = grid[key] or {}
          table.insert(grid[key], i)
        end
      end
    end
    -- Whether a third solid stands in the space between two others.
    local function filled(x0, y0, x1, y1, a, b)
      for _, c in ipairs(nearby(grid, x0, y0, x1, y1)) do
        local s = solids[c]
        if c ~= a and c ~= b and s.x0 < x1 - TOUCH and s.x1 > x0 + TOUCH
          and s.y0 < y1 - TOUCH and s.y1 > y0 + TOUCH then
          return true
        end
      end
      return false
    end
    local found = {}
    for a, sa in ipairs(solids) do
      local around = nearby(grid, sa.x0 - LARGEST, sa.y0 - LARGEST, sa.x1 + LARGEST, sa.y1 + LARGEST)
      for _, b in ipairs(around) do
        local sb = solids[b]
        local gx = math.max(sb.x0 - sa.x1, sa.x0 - sb.x1)
        local gy = math.max(sb.y0 - sa.y1, sa.y0 - sb.y1)
        -- Side by side with overlapping spans, and room between them.
        local gap, x0, y0, x1, y1
        if b <= a then
          gap = nil
        elseif gy < -TOUCH and gx > TOUCH then
          gap = gx
          x0, x1 = math.min(sa.x1, sb.x1), math.max(sa.x0, sb.x0)
          y0, y1 = math.max(sa.y0, sb.y0), math.min(sa.y1, sb.y1)
        elseif gx < -TOUCH and gy > TOUCH then
          gap = gy
          x0, x1 = math.max(sa.x0, sb.x0), math.min(sa.x1, sb.x1)
          y0, y1 = math.min(sa.y1, sb.y1), math.max(sa.y0, sb.y0)
        end
        if gap and gap < LARGEST and not filled(x0, y0, x1, y1, a, b) then
          local who = gap < SMALLEST and "any tank" or "heavy tanks"
          found[#found + 1] = {
            text = ("A %d-unit gap is too narrow for %s; close it or widen it."):format(math.floor(gap + 0.5), who),
            list = sa.list,
            index = sa.index,
          }
          if #found >= 8 then
            return found
          end
        end
      end
    end
    return found
  end,
}
