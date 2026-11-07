-- Bounty Hunt: every 40 seconds the top scorer becomes the bounty. The bounty
-- takes less damage and regains a shield, but whoever brings them down earns
-- a large reward. Works on any free-for-all or team mode.

tankyard.mode_name("Bounty Hunt")

local INTERVAL = 40 * 60 -- ticks
local REWARD_XP, REWARD_SCORE = 6000, 2500

local state = tankyard.state
state.bounty = nil

local function choose()
  local best
  for _, tank in ipairs(tankyard.tanks()) do
    if tank.alive and tank.score > 0 and (not best or tank.score > best.score) then
      best = tank
    end
  end
  if state.bounty and (not best or best.id ~= state.bounty) then
    tankyard.set_armor(state.bounty, 1) -- the old bounty returns to normal
  end
  state.bounty = best and best.id or nil
  if best then
    tankyard.set_armor(best.id, 0.75)
    tankyard.set_shield(best.id, 4)
    tankyard.announce(("Bounty on %s — take them down!"):format(best.name))
  end
end

tankyard.every(INTERVAL, choose)

tankyard.every(10 * 60, function()
  -- A short shield every ten seconds keeps the bounty in the fight.
  if state.bounty then
    local target = tankyard.tank(state.bounty)
    if target and target.alive then tankyard.set_shield(target.id, 1.5) end
  end
end)

tankyard.on("kill", function(event)
  if event.target ~= state.bounty or event.actor == 0 then return end
  local hunter = tankyard.tank(event.actor)
  tankyard.award(event.actor, REWARD_XP, REWARD_SCORE)
  tankyard.announce(("%s claimed the bounty!"):format(hunter and hunter.name or "Someone"))
  state.bounty = nil
end)

tankyard.on("round_start", function() state.bounty = nil end)
