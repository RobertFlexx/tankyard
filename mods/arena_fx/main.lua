-- Arena Effects: sounds and screen shake for the big moments of a round, and
-- effects a map can call on by name. Needs Tank Yard 1.1.0 or later.
--
-- A bell rings at the middle of the arena as each round starts, and every
-- destroyed tank lands with a thump where it fell. Each fifth kill in a row
-- plays a fanfare and gives every player's view a short jolt.
--
-- Maps call on the rest with a Script event trigger:
--   "alarm"  flashes group 1 red, makes it tremble, sounds the alarm at the
--            trigger and shakes the views of the tanks inside its area;
--   "calm"   clears the tint and settles group 1 again.

local STREAK = 5

local streaks = {}

tankyard.on("round_start", function()
  streaks = {}
  local bounds = tankyard.arena().bounds
  tankyard.play_sound("bell", bounds.x + bounds.w / 2, bounds.y + bounds.h / 2, 70)
end)

tankyard.on("kill", function(event)
  tankyard.play_sound("thump", event.x, event.y, 60)
  streaks[event.target] = nil
  if event.actor == 0 then return end -- the arena itself
  local streak = (streaks[event.actor] or 0) + 1
  streaks[event.actor] = streak
  local tank = tankyard.tank(event.actor)
  if tank and streak % STREAK == 0 then
    tankyard.play_sound("fanfare", tank.x, tank.y)
    tankyard.shake_view(nil, 25, 0.4)
    tankyard.announce(("%s has %d kills in a row"):format(tank.name, streak))
  end
end)

tankyard.on("map_event", function(event)
  if event.name == "alarm" then
    tankyard.tint_map_group(1, "#ff3030", 80, 0.2)
    tankyard.animate_map_group(1, "shake", 6, 9, 0.2)
    tankyard.play_sound("alarm", event.x, event.y)
    for _, id in ipairs(event.tanks) do
      tankyard.shake_view(id, 40, 0.8)
    end
  elseif event.name == "calm" then
    tankyard.tint_map_group(1, nil, 0, 1)
    tankyard.animate_map_group(1, "shake", 0, 0, 0.5)
  end
end)
