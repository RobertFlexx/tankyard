-- Gravity Wells: three wells orbit the arena. Tanks inside a well's reach are
-- pulled toward its center, harder the closer they get, and every few seconds
-- a well spawns a valuable hexagon at its core.
local vector = require("lib.vector")

local WELLS, REACH, PULL = 3, 520, 26

local function well_position(index, tick)
  local arena = tankyard.arena()
  local b = arena.bounds
  local angle = tick / 60 * 0.12 + index * (math.pi * 2 / WELLS)
  local cx, cy = b.x + b.w / 2, b.y + b.h / 2
  return cx + math.cos(angle) * b.w * 0.3, cy + math.sin(angle) * b.h * 0.28
end

tankyard.every(2, function(tick)
  for index = 1, WELLS do
    local wx, wy = well_position(index, tick)
    for _, tank in ipairs(tankyard.tanks()) do
      if tank.alive and not tank.in_base then
        local px, py, distance = vector.toward(tank.x, tank.y, wx, wy, 1)
        if distance < REACH then
          local strength = PULL * (1 - distance / REACH)
          tankyard.push(tank.id, px * strength, py * strength)
        end
      end
    end
  end
end)

tankyard.every(6 * 60, function(tick)
  local index = math.floor(tick / (6 * 60)) % WELLS + 1
  local wx, wy = well_position(index, tick)
  tankyard.spawn_shape(wx, wy, { sides = 6, health = 420, xp = 900 })
end)
