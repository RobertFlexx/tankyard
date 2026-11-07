-- Sprinkler: a new tier-2 tank defined from Lua. It extends Machine Gun's
-- branch, so the edited parent is defined too; both stay sandboxed data.

local parent = tankyard.base_class("spray")
table.insert(parent.upgrades, "sprinkler")
tankyard.define_class(parent)

local sprinkler = tankyard.base_class("fan")
sprinkler.id = "sprinkler"
sprinkler.name = "Sprinkler"
sprinkler.description = "Three oscillating barrels hose down a wide cone in front of the hull."
sprinkler.strengths = "Covers wide lanes and punishes groups at close range."
sprinkler.weaknesses = "Short effective range; spread wastes shots at distance."
sprinkler.parents = { "spray" }
sprinkler.upgrades = {}
for index, barrel in ipairs(sprinkler.barrels) do
  barrel.spread = 0.35 + index * 0.05
  barrel.pattern = "Spin"
end
tankyard.define_class(sprinkler)

-- Close-range kills with a Sprinkler refund some health.
tankyard.on("kill", function(event)
  local hunter = tankyard.tank(event.actor)
  if hunter and hunter.class == "sprinkler" then
    tankyard.heal(hunter.id, hunter.max_health * 0.12)
  end
end)
