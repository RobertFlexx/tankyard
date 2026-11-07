-- Small 2D helpers shared by the pack's scripts.
local vector = {}

function vector.length(x, y)
  return math.sqrt(x * x + y * y)
end

function vector.toward(from_x, from_y, to_x, to_y, strength)
  local dx, dy = to_x - from_x, to_y - from_y
  local distance = vector.length(dx, dy)
  if distance < 1 then return 0, 0, distance end
  return dx / distance * strength, dy / distance * strength, distance
end

return vector
