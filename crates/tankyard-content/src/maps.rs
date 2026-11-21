//! Original arena presets; normalized geometry respects server size overrides.
use serde::{Deserialize, Serialize};
use std::collections::{HashSet, VecDeque};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum MapProcedure {
    #[default]
    None,
    Maze,
    Dense,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SpawnRegion {
    /// Zero means an unteamed spawn region; one through four select a faction.
    pub team: u8,
    pub rect: [f32; 4],
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TeamBase {
    pub team: u8,
    pub position: [f32; 2],
}

/// The furniture look of a block. `Map` follows the arena's own style.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BlockStyle {
    #[default]
    Map,
    Slate,
    Garden,
    Circuit,
    Bastion,
    Relay,
    Orrery,
    Sand,
}
impl BlockStyle {
    pub const ALL: [Self; 8] = [
        Self::Map,
        Self::Slate,
        Self::Garden,
        Self::Circuit,
        Self::Bastion,
        Self::Relay,
        Self::Orrery,
        Self::Sand,
    ];
    pub fn name(self) -> &'static str {
        match self {
            Self::Map => "Arena default",
            Self::Slate => "Slate",
            Self::Garden => "Hedge",
            Self::Circuit => "Circuit",
            Self::Bastion => "Rampart",
            Self::Relay => "Relay",
            Self::Orrery => "Orrery",
            Self::Sand => "Sandstone",
        }
    }
}

/// A solid block: a wall with its own look, colour and group.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MapBlock {
    /// Normalized x, y, width and height, like `walls`.
    pub rect: [f32; 4],
    #[serde(default)]
    pub style: BlockStyle,
    #[serde(default)]
    pub color: Option<[u8; 3]>,
    /// Zero for none; 1 to 32 lets triggers show and hide it.
    #[serde(default)]
    pub group: u8,
}

/// What a pad does to tanks over it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ZoneKind {
    /// Faster movement.
    Speed,
    /// Slower movement.
    Mud,
    /// Health regenerates.
    Repair,
    /// Health drains; it can destroy a tank.
    Hazard,
    /// Incoming damage is reduced.
    Shelter,
    /// Carries tanks along `angle`.
    Conveyor,
    /// Sends tanks to `exit`.
    Portal,
    // Newer kinds go last: the wire format numbers variants in order.
    /// Flings tanks along `angle`; they coast on after leaving it.
    Launch,
    // Added in 1.3.0.
    /// Pulls tanks toward its center, harder the closer they get.
    Pull,
    /// Slippery: tanks speed up, turn and stop slowly.
    Ice,
}
impl ZoneKind {
    pub const ALL: [Self; 10] = [
        Self::Speed,
        Self::Mud,
        Self::Ice,
        Self::Repair,
        Self::Hazard,
        Self::Shelter,
        Self::Conveyor,
        Self::Launch,
        Self::Pull,
        Self::Portal,
    ];
    pub fn name(self) -> &'static str {
        match self {
            Self::Speed => "Speed pad",
            Self::Mud => "Mud",
            Self::Repair => "Repair pad",
            Self::Hazard => "Hazard",
            Self::Shelter => "Shelter",
            Self::Conveyor => "Conveyor",
            Self::Portal => "Portal",
            Self::Launch => "Launch pad",
            Self::Pull => "Gravity well",
            Self::Ice => "Ice",
        }
    }
    /// Whether `angle` sets which way it acts.
    pub fn aims(self) -> bool {
        matches!(self, Self::Conveyor | Self::Launch)
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MapZone {
    pub kind: ZoneKind,
    pub rect: [f32; 4],
    /// 0.25 to 2; 1 is the standard effect.
    #[serde(default = "unit")]
    pub strength: f32,
    #[serde(default)]
    pub group: u8,
    /// Conveyor or launch direction in degrees: 0 right, 90 down.
    #[serde(default)]
    pub angle: f32,
    /// Where a portal sends tanks, normalized like positions.
    #[serde(default)]
    pub exit: [f32; 2],
}
fn unit() -> f32 {
    1.
}
/// World units per second a conveyor of strength 1 adds to a tank's motion.
pub const CONVEYOR_SPEED: f32 = 150.;
/// World units per second a launch pad of strength 1 flings a tank at.
pub const LAUNCH_SPEED: f32 = 900.;
/// World units per second a gravity well of strength 1 pulls a tank at its
/// edge; the pull doubles toward the center.
pub const PULL_SPEED: f32 = 130.;
/// How much grip a tank keeps on ice of strength 1, as a fraction.
pub const ICE_GRIP: f32 = 0.22;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DecorKind {
    /// A flat floor panel.
    #[default]
    Tile,
    /// Diagonal warning stripes.
    Stripes,
    /// A soft round glow.
    Light,
    /// An outlined circle.
    Ring,
    /// A floor arrow, pointing along `rotation`.
    Arrow,
    /// Painted text.
    Label,
    /// A particle emitter; its look is `effect`.
    Emitter,
    // Added in 1.3.0.
    /// A filled circle or oval.
    Disc,
    /// A triangle pointing along `rotation`.
    Triangle,
    /// A six-sided plate.
    Hexagon,
    /// A cone of light shining along `rotation`; it lights up dark maps.
    Spotlight,
    /// A shaft of light or a laser line along the rectangle's length.
    Beam,
    /// Floor grid lines, like a tiled floor or a blueprint.
    Grid,
}
impl DecorKind {
    pub const ALL: [Self; 13] = [
        Self::Tile,
        Self::Stripes,
        Self::Grid,
        Self::Disc,
        Self::Triangle,
        Self::Hexagon,
        Self::Ring,
        Self::Arrow,
        Self::Label,
        Self::Light,
        Self::Spotlight,
        Self::Beam,
        Self::Emitter,
    ];
    pub fn name(self) -> &'static str {
        match self {
            Self::Tile => "Floor tile",
            Self::Stripes => "Stripes",
            Self::Light => "Light",
            Self::Ring => "Ring",
            Self::Arrow => "Arrow",
            Self::Label => "Text",
            Self::Emitter => "Particles",
            Self::Disc => "Disc",
            Self::Triangle => "Triangle",
            Self::Hexagon => "Hexagon",
            Self::Spotlight => "Spotlight",
            Self::Beam => "Beam",
            Self::Grid => "Grid",
        }
    }
    /// Whether it gives off light that shows through a map's darkness.
    pub fn lights(self) -> bool {
        matches!(self, Self::Light | Self::Spotlight | Self::Beam)
    }
}

/// How a decoration responds to tanks nearby. Each player sees it respond to
/// their own tank, so a roof can clear for whoever drives under it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DecorReact {
    #[default]
    None,
    /// Fades away as a tank comes near, like a roof or tree canopy.
    Fade,
    /// Fades in as a tank comes near, like a hidden sign.
    Appear,
    /// Swells as a tank comes near.
    Grow,
    /// Brightens as a tank comes near.
    Glow,
}
impl DecorReact {
    pub const ALL: [Self; 5] = [Self::None, Self::Fade, Self::Appear, Self::Grow, Self::Glow];
    pub fn name(self) -> &'static str {
        match self {
            Self::None => "Nothing",
            Self::Fade => "Fade away",
            Self::Appear => "Appear",
            Self::Grow => "Grow",
            Self::Glow => "Glow",
        }
    }
    /// How strongly it responds to a tank `distance` world units from its
    /// edge, 0 to 1, when it responds within `reach`.
    pub fn closeness(distance: f32, reach: f32) -> f32 {
        let reach = reach.max(1.);
        let t = (1. - distance.max(0.) / reach).clamp(0., 1.);
        t * t * (3. - 2. * t)
    }
}
/// How near a tank must come, in world units, before decor responds,
/// unless the decoration sets its own.
pub const REACT_REACH: f32 = 220.;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ParticleShape {
    #[default]
    Dot,
    Square,
    Spark,
    Ring,
    Triangle,
    /// A dot with a soft edge, for smoke, mist and glows.
    Soft,
    /// A five-pointed star.
    Star,
}
impl ParticleShape {
    pub const ALL: [Self; 7] = [
        Self::Dot,
        Self::Soft,
        Self::Square,
        Self::Spark,
        Self::Ring,
        Self::Triangle,
        Self::Star,
    ];
    pub fn name(self) -> &'static str {
        match self {
            Self::Dot => "Dot",
            Self::Soft => "Soft",
            Self::Square => "Square",
            Self::Spark => "Spark",
            Self::Ring => "Ring",
            Self::Triangle => "Triangle",
            Self::Star => "Star",
        }
    }
}

/// Where in its rectangle an emitter releases particles.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EmitArea {
    /// Anywhere in the rectangle.
    #[default]
    Fill,
    /// Anywhere in the ellipse inside it.
    Round,
    /// Along its outline.
    Edge,
    /// From its center.
    Point,
}
impl EmitArea {
    pub const ALL: [Self; 4] = [Self::Fill, Self::Round, Self::Edge, Self::Point];
    pub fn name(self) -> &'static str {
        match self {
            Self::Fill => "Fill",
            Self::Round => "Round",
            Self::Edge => "Edge",
            Self::Point => "Center",
        }
    }
}

/// A particle effect, drawn by clients only. Every field has a bounded range
/// so a map cannot ask for more particles than a client can afford.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ParticleEffect {
    pub shape: ParticleShape,
    /// Particles per second, 0.2 to 60.
    pub rate: f32,
    /// Seconds each particle lives, 0.1 to 8.
    pub lifetime: f32,
    /// Launch speed in world units per second, 0 to 800.
    pub speed: f32,
    /// Launch direction in degrees, 0 pointing right, 90 down.
    pub direction: f32,
    /// Spread around the direction in degrees, 0 to 360.
    pub spread: f32,
    /// Constant downward pull, -800 to 800; negative floats upward.
    pub gravity: f32,
    /// Speed lost per second as a fraction, 0 to 8.
    pub drag: f32,
    /// Start and end size in world units, 1 to 120.
    pub size: f32,
    pub end_size: f32,
    pub color: [u8; 3],
    pub end_color: [u8; 3],
    /// Fade out over the particle's life.
    pub fade: bool,
    /// Spin in degrees per second, -1080 to 1080.
    pub spin: f32,
    /// Add their light to what is beneath them, like embers or sparks.
    pub glow: bool,
    /// Sideways drift in world units, 0 to 200, for smoke and fireflies.
    pub wobble: f32,
    /// How much each particle flickers, 0 to 1.
    pub twinkle: f32,
    pub area: EmitArea,
}
impl Default for ParticleEffect {
    fn default() -> Self {
        Self {
            shape: ParticleShape::Dot,
            rate: 8.,
            lifetime: 1.6,
            speed: 60.,
            direction: 270.,
            spread: 50.,
            gravity: 0.,
            drag: 0.6,
            size: 9.,
            end_size: 3.,
            color: [255, 200, 90],
            end_color: [230, 90, 60],
            fade: true,
            spin: 0.,
            glow: false,
            wobble: 0.,
            twinkle: 0.,
            area: EmitArea::Fill,
        }
    }
}
impl ParticleEffect {
    pub fn validate(&self) -> Result<(), &'static str> {
        if range(self.rate, 0.2, 60.)
            && range(self.lifetime, 0.1, 8.)
            && range(self.speed, 0., 800.)
            && self.direction.is_finite()
            && range(self.spread, 0., 360.)
            && range(self.gravity, -800., 800.)
            && range(self.drag, 0., 8.)
            && range(self.size, 1., 120.)
            && range(self.end_size, 0., 120.)
            && range(self.spin, -1080., 1080.)
            && range(self.wobble, 0., 200.)
            && range(self.twinkle, 0., 1.)
            && self.rate * self.lifetime <= 240.
        {
            Ok(())
        } else {
            Err("particle effect values are out of range")
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MapDecor {
    pub kind: DecorKind,
    pub rect: [f32; 4],
    #[serde(default = "decor_color")]
    pub color: [u8; 3],
    /// 0 is invisible, 255 opaque.
    #[serde(default = "opaque")]
    pub alpha: u8,
    /// Degrees; arrows point this way and text and stripes turn with it.
    #[serde(default)]
    pub rotation: f32,
    /// Brightness pulses per second, 0 to 4.
    #[serde(default)]
    pub pulse: f32,
    /// Degrees per second, -720 to 720.
    #[serde(default)]
    pub spin: f32,
    /// Unsteady brightness, like a flame or a failing lamp, 0 to 1.
    #[serde(default)]
    pub flicker: f32,
    /// Drawn above tanks instead of on the floor.
    #[serde(default)]
    pub above: bool,
    #[serde(default)]
    pub text: String,
    #[serde(default)]
    pub effect: Option<ParticleEffect>,
    #[serde(default)]
    pub group: u8,
    /// How it responds to tanks nearby.
    #[serde(default)]
    pub react: DecorReact,
    /// How near, in world units, a tank must come for it to respond, 0 to
    /// 2000; 0 uses [`REACT_REACH`].
    #[serde(default)]
    pub reach: f32,
}
impl MapDecor {
    pub fn react_reach(&self) -> f32 {
        if self.reach > 0. {
            self.reach
        } else {
            REACT_REACH
        }
    }
}
fn decor_color() -> [u8; 3] {
    [120, 160, 190]
}
fn opaque() -> u8 {
    255
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TriggerWhen {
    /// Repeats every `seconds`, starting `offset` seconds into the round.
    #[default]
    Every,
    /// When a living tank enters `rect`; `seconds` is the cooldown.
    Enter,
    /// Once, `seconds` after the round starts.
    Start,
    /// When a living tank leaves `rect`; `seconds` is the cooldown.
    Exit,
    /// When another trigger sends a signal on channel `source`.
    Signal,
    /// When counter `source` reaches `threshold`, from either side.
    Counter,
    // Newer kinds go last: the wire format numbers variants in order.
    /// Every `seconds` while at least `threshold` tanks are inside `rect`.
    Occupied,
    /// When the last tank inside `rect` leaves or is destroyed; `seconds` is
    /// the cooldown.
    Cleared,
    /// When a tank is destroyed inside `rect`; `seconds` is the cooldown.
    Destroyed,
    /// Whenever counter `source` changes value.
    Changed,
    /// When group `source` is shown after being hidden.
    Shown,
    /// When group `source` is hidden after being shown.
    Hidden,
    /// When a boss is defeated: boss `source` of [`BOSS_IDS`], counting from
    /// 1, or any boss for 0.
    Defeated,
    /// When a motion of group `source` ends: a Move, Move to, Turn or Fade,
    /// the trip back included.
    Arrived,
    // Added in 1.3.0.
    /// When the blocks of group `source` start to overlap `rect`, such as a
    /// crate pushed onto a plate; `seconds` is the cooldown.
    Touch,
    /// When the blocks of group `source` stop overlapping `rect`; `seconds`
    /// is the cooldown.
    Release,
    // Added in 2.0.0.
    /// Whenever a counter or group its conditions look at changes, and once
    /// as the round starts, so a gate's result is right from the first tick.
    Inputs,
}
impl TriggerWhen {
    pub const ALL: [Self; 17] = [
        Self::Every,
        Self::Enter,
        Self::Exit,
        Self::Start,
        Self::Signal,
        Self::Counter,
        Self::Occupied,
        Self::Cleared,
        Self::Destroyed,
        Self::Changed,
        Self::Inputs,
        Self::Shown,
        Self::Hidden,
        Self::Defeated,
        Self::Arrived,
        Self::Touch,
        Self::Release,
    ];
    pub fn name(self) -> &'static str {
        match self {
            Self::Every => "Every",
            Self::Enter => "On enter",
            Self::Exit => "On exit",
            Self::Start => "At start",
            Self::Signal => "On signal",
            Self::Counter => "On counter",
            Self::Occupied => "While inside",
            Self::Cleared => "On empty",
            Self::Destroyed => "On destroy",
            Self::Changed => "On change",
            Self::Shown => "On shown",
            Self::Hidden => "On hidden",
            Self::Defeated => "On boss down",
            Self::Arrived => "On arrive",
            Self::Touch => "On touch",
            Self::Release => "On release",
            Self::Inputs => "On input change",
        }
    }
    /// Whether it watches a group's blocks over its rectangle.
    pub fn watches_blocks(self) -> bool {
        matches!(self, Self::Touch | Self::Release)
    }
    /// Whether the trigger watches its rectangle for tanks.
    pub fn watches_area(self) -> bool {
        matches!(
            self,
            Self::Enter | Self::Exit | Self::Occupied | Self::Cleared | Self::Destroyed
        )
    }
    /// Whether something else in the map wakes it, rather than time or an area.
    pub fn is_event(self) -> bool {
        matches!(
            self,
            Self::Signal
                | Self::Counter
                | Self::Changed
                | Self::Inputs
                | Self::Shown
                | Self::Hidden
                | Self::Defeated
                | Self::Arrived
        )
    }
    /// Whether `source` names a group it watches.
    pub fn watches_group(self) -> bool {
        matches!(
            self,
            Self::Shown | Self::Hidden | Self::Arrived | Self::Touch | Self::Release
        )
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TriggerAction {
    #[default]
    Toggle,
    Show,
    Hide,
    /// Shows `text` to everyone in the arena.
    Announce,
    /// Brings the `boss` named boss into the arena at `rect`.
    Boss,
    /// Fires every `Signal` trigger listening on channel `group`.
    Signal,
    /// Adds `amount` to counter `group`.
    Count,
    /// Spawns `amount` shapes inside `rect`.
    Shapes,
    // Newer kinds go last: the wire format numbers variants in order.
    /// Sets counter `group` to `amount`.
    Set,
    /// Signals one channel picked at random from `group` to
    /// `group + amount - 1`.
    Random,
    /// Restores `amount` percent of maximum health to the tanks in `rect`.
    Heal,
    /// Deals `amount` percent of maximum health to the tanks in `rect`.
    Hurt,
    /// Gives `amount` points to each tank in `rect`.
    Award,
    /// Changes counter `group` by `math` with `amount`, or with counter
    /// `operand` when that is set.
    Calculate,
    /// Raises the `map_event` script event named `text`, for script packs.
    Script,
    /// Signals the next channel of `group` to `group + amount - 1` in turn,
    /// starting again after the last.
    Sequence,
    /// Starts the triggers of group `group` (every trigger for 0) afresh:
    /// waiting delays are dropped, Once per round ones can act again, timers
    /// count from now and sequences start at their first channel.
    Reset,
    /// Glides group `group` by `travel` over `tween` seconds.
    Move,
    /// Glides group `group` to `travel` from where the map places it.
    Place,
    /// Turns group `group`'s decorations by `amount` degrees.
    Turn,
    /// Fades group `group`'s decorations to `amount` percent opacity.
    Fade,
    // Added in 1.1.0.
    /// Mixes `color` into group `group`'s decorations at `amount` percent.
    Tint,
    /// Draws group `group`'s decorations at `amount` percent of their size.
    Scale,
    /// Plays the `style` animation on group `group`'s decorations, `amount`
    /// in size at `speed` cycles a second; 0 settles it.
    Animate,
    /// Halts every motion and animation of group `group` where it is.
    Stop,
    /// Shakes the view of the noticed tanks in `rect`, `amount` percent hard
    /// for `tween` seconds.
    Shake,
    /// Plays `sound` at `rect`, at `amount` percent volume.
    Sound,
    // Added in 1.3.0.
    /// Shoves body `group` toward `angle` at `amount` world units a second.
    Shove,
    /// Flings the noticed tanks in `rect` toward `angle` at `amount` world
    /// units a second.
    Fling,
    /// Throws the noticed tanks in `rect` away from its center at `amount`
    /// world units a second, or pulls them in when `amount` is negative.
    Blast,
    /// Zooms the views of the noticed tanks in `rect` to `amount` percent
    /// for `tween` seconds: below 100 shows more of the arena.
    Zoom,
    /// Changes the arena's lighting over `tween` seconds: `amount` percent
    /// darkness, tinted `color`.
    Light,
    /// Sends group `group` along path `amount`, counting from 1.
    Path,
    /// Shows the trigger's `ui` widget in slot `group` on the screens its
    /// `ui` names, replacing what that slot showed.
    Ui,
    /// Takes slot `group`'s widget (every map widget for 0) off the screens
    /// its `ui` names, or everyone's without one.
    HideUi,
    // Added in 2.0.0.
    /// Sets counter `group` to `amount` while the gate is open and to 0
    /// while it is shut: a logic gate whose result is a number. The gate
    /// never stops it acting.
    Output,
}
impl TriggerAction {
    pub const ALL: [Self; 36] = [
        Self::Toggle,
        Self::Show,
        Self::Hide,
        Self::Move,
        Self::Place,
        Self::Path,
        Self::Turn,
        Self::Fade,
        Self::Tint,
        Self::Scale,
        Self::Animate,
        Self::Stop,
        Self::Shove,
        Self::Fling,
        Self::Blast,
        Self::Shake,
        Self::Zoom,
        Self::Light,
        Self::Sound,
        Self::Announce,
        Self::Ui,
        Self::HideUi,
        Self::Boss,
        Self::Signal,
        Self::Random,
        Self::Sequence,
        Self::Count,
        Self::Set,
        Self::Calculate,
        Self::Output,
        Self::Shapes,
        Self::Heal,
        Self::Hurt,
        Self::Award,
        Self::Script,
        Self::Reset,
    ];
    pub fn name(self) -> &'static str {
        match self {
            Self::Toggle => "Toggle group",
            Self::Show => "Show group",
            Self::Hide => "Hide group",
            Self::Announce => "Announce",
            Self::Boss => "Boss arrives",
            Self::Signal => "Send signal",
            Self::Count => "Add to counter",
            Self::Shapes => "Spawn shapes",
            Self::Set => "Set counter",
            Self::Random => "Random signal",
            Self::Heal => "Repair tanks",
            Self::Hurt => "Damage tanks",
            Self::Award => "Award points",
            Self::Calculate => "Calculate",
            Self::Script => "Script event",
            Self::Sequence => "Sequence",
            Self::Reset => "Reset triggers",
            Self::Move => "Move group",
            Self::Place => "Move group to",
            Self::Turn => "Turn decor",
            Self::Fade => "Fade decor",
            Self::Tint => "Tint decor",
            Self::Scale => "Scale decor",
            Self::Animate => "Animate decor",
            Self::Stop => "Stop group",
            Self::Shake => "Shake screen",
            Self::Sound => "Play sound",
            Self::Shove => "Shove body",
            Self::Fling => "Fling tanks",
            Self::Blast => "Blast",
            Self::Zoom => "Zoom view",
            Self::Light => "Lighting",
            Self::Path => "Play path",
            Self::Ui => "Show UI",
            Self::HideUi => "Hide UI",
            Self::Output => "Gate to counter",
        }
    }
    /// Whether it shows or hides a widget, `group` naming its slot.
    pub fn uses_ui(self) -> bool {
        matches!(self, Self::Ui | Self::HideUi)
    }
    /// Whether `group` names a map group, as opposed to a channel or counter.
    pub fn targets_group(self) -> bool {
        matches!(
            self,
            Self::Toggle
                | Self::Show
                | Self::Hide
                | Self::Move
                | Self::Place
                | Self::Turn
                | Self::Fade
                | Self::Tint
                | Self::Scale
                | Self::Animate
                | Self::Stop
                | Self::Shove
                | Self::Path
        )
    }
    /// Whether it shows or hides group `group`.
    pub fn switches_group(self) -> bool {
        matches!(self, Self::Toggle | Self::Show | Self::Hide)
    }
    /// Whether it plays a motion, using `tween` and `easing`.
    pub fn animates(self) -> bool {
        matches!(
            self,
            Self::Move
                | Self::Place
                | Self::Turn
                | Self::Fade
                | Self::Tint
                | Self::Scale
                | Self::Animate
                | Self::Light
        )
    }
    /// Whether `angle` sets which way it pushes.
    pub fn aims(self) -> bool {
        matches!(self, Self::Shove | Self::Fling)
    }
    /// Whether it only changes how decorations look.
    pub fn decor_only(self) -> bool {
        matches!(
            self,
            Self::Turn | Self::Fade | Self::Tint | Self::Scale | Self::Animate
        )
    }
    /// Whether it moves the group, using `travel`.
    pub fn moves(self) -> bool {
        matches!(self, Self::Move | Self::Place)
    }
    /// Whether `group` names a counter.
    pub fn targets_counter(self) -> bool {
        matches!(
            self,
            Self::Count | Self::Set | Self::Calculate | Self::Output
        )
    }
    /// Whether it acts on the tanks standing in the trigger's area.
    pub fn affects_tanks(self) -> bool {
        matches!(
            self,
            Self::Heal
                | Self::Hurt
                | Self::Award
                | Self::Shake
                | Self::Fling
                | Self::Blast
                | Self::Zoom
        )
    }
    /// Whether it sends on a run of `amount` channels from `group`.
    pub fn sends_range(self) -> bool {
        matches!(self, Self::Random | Self::Sequence)
    }
    /// The signal channels it can send on.
    pub fn channels(self, group: u8, amount: i32) -> std::ops::RangeInclusive<u8> {
        match self {
            Self::Signal => group..=group,
            Self::Random | Self::Sequence => {
                let last = (i64::from(group) + i64::from(amount.max(1)) - 1)
                    .clamp(0, i64::from(MAP_CHANNELS));
                group..=last as u8
            }
            #[allow(clippy::reversed_empty_ranges)]
            _ => 1..=0,
        }
    }
}

/// A sound a Play sound trigger makes. Each is synthesised by the game, so a
/// map carries no audio.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MapSound {
    /// A bright, rising two-note chime.
    #[default]
    Chime,
    /// One clear, ringing bell.
    Bell,
    /// A short warning siren.
    Alarm,
    /// A mechanical click and clunk, for levers and doors.
    Switch,
    /// A rush of air, for launches and passing things.
    Whoosh,
    /// A deep, rolling rumble.
    Rumble,
    /// Three quick rising tones, for a win or a reward.
    Fanfare,
    /// A soft, low pulse, like a heartbeat or a distant drum.
    Thump,
}
impl MapSound {
    pub const ALL: [Self; 8] = [
        Self::Chime,
        Self::Bell,
        Self::Alarm,
        Self::Switch,
        Self::Whoosh,
        Self::Rumble,
        Self::Fanfare,
        Self::Thump,
    ];
    pub fn name(self) -> &'static str {
        match self {
            Self::Chime => "Chime",
            Self::Bell => "Bell",
            Self::Alarm => "Alarm",
            Self::Switch => "Switch",
            Self::Whoosh => "Whoosh",
            Self::Rumble => "Rumble",
            Self::Fanfare => "Fanfare",
            Self::Thump => "Thump",
        }
    }
    /// Its place in [`MapSound::ALL`], as events carry it.
    pub fn index(self) -> u8 {
        Self::ALL.iter().position(|&s| s == self).unwrap_or(0) as u8
    }
    pub fn from_index(index: u8) -> Option<Self> {
        Self::ALL.get(usize::from(index)).copied()
    }
}

/// How Calculate changes its counter with the other value.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MathOp {
    #[default]
    Add,
    Subtract,
    Multiply,
    /// Rounds down; dividing by zero changes nothing.
    Divide,
    /// Never negative; by zero changes nothing.
    Remainder,
    Lowest,
    Highest,
    /// Takes the other value.
    Copy,
    /// A whole number from 0 up to one less than the other value.
    Random,
    /// 1 when the counter is 0, otherwise 0; the other value is unused.
    Flip,
    // Added in 1.7.0.
    /// Keeps the bits set in both.
    And,
    /// Keeps the bits set in either.
    Or,
    /// Keeps the bits set in just one of them.
    Xor,
    /// Doubles the counter as many times as the other value says.
    ShiftLeft,
    /// Halves the counter, rounding down, as many times as the other value
    /// says.
    ShiftRight,
    /// The counter to the power of the other value; a negative power
    /// changes nothing.
    Power,
}
impl MathOp {
    pub const ALL: [Self; 16] = [
        Self::Add,
        Self::Subtract,
        Self::Multiply,
        Self::Divide,
        Self::Remainder,
        Self::Lowest,
        Self::Highest,
        Self::Copy,
        Self::Random,
        Self::Flip,
        Self::And,
        Self::Or,
        Self::Xor,
        Self::ShiftLeft,
        Self::ShiftRight,
        Self::Power,
    ];
    pub fn name(self) -> &'static str {
        match self {
            Self::Add => "Add",
            Self::Subtract => "Subtract",
            Self::Multiply => "Multiply",
            Self::Divide => "Divide",
            Self::Remainder => "Remainder",
            Self::Lowest => "Lowest",
            Self::Highest => "Highest",
            Self::Copy => "Copy",
            Self::Random => "Random",
            Self::Flip => "Flip 0/1",
            Self::And => "Bits AND",
            Self::Or => "Bits OR",
            Self::Xor => "Bits XOR",
            Self::ShiftLeft => "Shift left",
            Self::ShiftRight => "Shift right",
            Self::Power => "Power",
        }
    }
    /// Whether the result depends on the other value.
    pub fn uses_operand(self) -> bool {
        self != Self::Flip
    }
    /// The counter's new value, or `None` to leave it. `roll` is a random
    /// fraction in `[0, 1)`, used only by `Random`.
    pub fn apply(self, value: i32, other: i32, roll: f32) -> Option<i32> {
        let (a, b) = (i64::from(value), i64::from(other));
        let result = match self {
            Self::Add => a + b,
            Self::Subtract => a - b,
            Self::Multiply => a * b,
            Self::Divide if b == 0 => return None,
            Self::Divide => {
                let q = a / b;
                if a % b != 0 && (a < 0) != (b < 0) {
                    q - 1
                } else {
                    q
                }
            }
            Self::Remainder if b == 0 => return None,
            Self::Remainder => a.rem_euclid(b),
            Self::Lowest => a.min(b),
            Self::Highest => a.max(b),
            Self::Copy => b,
            Self::Random if b < 1 => return None,
            Self::Random => ((f64::from(roll.clamp(0., 1.)) * b as f64).floor() as i64).min(b - 1),
            Self::Flip => i64::from(value == 0),
            Self::And => a & b,
            Self::Or => a | b,
            Self::Xor => a ^ b,
            // Counters hold 31 bits with the sign, so 62 places reach the
            // limit from any value but 0; saturating keeps the sign.
            Self::ShiftLeft => a.saturating_mul(1 << b.clamp(0, 62)),
            Self::ShiftRight => a >> b.clamp(0, 63),
            Self::Power if b < 0 => return None,
            Self::Power => a.saturating_pow(b.min(64) as u32),
        };
        Some(result.clamp(-i64::from(COUNTER_LIMIT), i64::from(COUNTER_LIMIT)) as i32)
    }
}

/// How a gate compares a counter.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Compare {
    #[default]
    Equal,
    NotEqual,
    Below,
    Above,
    AtMost,
    AtLeast,
}
impl Compare {
    pub const ALL: [Self; 6] = [
        Self::Equal,
        Self::NotEqual,
        Self::Below,
        Self::Above,
        Self::AtMost,
        Self::AtLeast,
    ];
    pub fn symbol(self) -> &'static str {
        match self {
            Self::Equal => "=",
            Self::NotEqual => "\u{2260}",
            Self::Below => "<",
            Self::Above => ">",
            Self::AtMost => "\u{2264}",
            Self::AtLeast => "\u{2265}",
        }
    }
    pub fn words(self) -> &'static str {
        match self {
            Self::Equal => "is",
            Self::NotEqual => "is not",
            Self::Below => "is below",
            Self::Above => "is above",
            Self::AtMost => "is at most",
            Self::AtLeast => "is at least",
        }
    }
    pub fn holds(self, a: i32, b: i32) -> bool {
        match self {
            Self::Equal => a == b,
            Self::NotEqual => a != b,
            Self::Below => a < b,
            Self::Above => a > b,
            Self::AtMost => a <= b,
            Self::AtLeast => a >= b,
        }
    }
}

/// What a gate looks at.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TestKind {
    /// Counter `index` against `value`, or against counter `against`.
    #[default]
    Counter,
    /// Group `index` is shown.
    Shown,
    /// Group `index` is hidden.
    Hidden,
    /// How many noticed tanks are inside the trigger's area, against `value`
    /// or counter `against`.
    Inside,
    /// Whole seconds since the round started, against `value` or counter
    /// `against`.
    Time,
}
impl TestKind {
    pub const ALL: [Self; 5] = [
        Self::Counter,
        Self::Shown,
        Self::Hidden,
        Self::Inside,
        Self::Time,
    ];
    pub fn name(self) -> &'static str {
        match self {
            Self::Counter => "Counter",
            Self::Shown => "Group shown",
            Self::Hidden => "Group hidden",
            Self::Inside => "Tanks inside",
            Self::Time => "Round time",
        }
    }
    /// Whether it compares a number, rather than looking at a group.
    pub fn compares(self) -> bool {
        !matches!(self, Self::Shown | Self::Hidden)
    }
}

/// How a trigger's conditions combine into one gate.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GateMode {
    /// Every condition holds: AND.
    #[default]
    All,
    /// At least one holds: OR.
    Any,
    /// At least one fails: NAND.
    NotAll,
    /// None holds: NOR.
    NoneOf,
    /// Exactly one holds: XOR.
    One,
    // Added in 2.0.0.
    /// An even number hold, none included: XNOR. With one condition, NOT.
    Even,
    /// An odd number hold: parity, the XOR of every condition.
    Odd,
    /// More than half hold: majority.
    Most,
}
impl GateMode {
    pub const ALL: [Self; 8] = [
        Self::All,
        Self::Any,
        Self::NotAll,
        Self::NoneOf,
        Self::One,
        Self::Even,
        Self::Odd,
        Self::Most,
    ];
    pub fn name(self) -> &'static str {
        match self {
            Self::All => "All (AND)",
            Self::Any => "Any (OR)",
            Self::NotAll => "Not all (NAND)",
            Self::NoneOf => "None (NOR)",
            Self::One => "Just one (XOR)",
            Self::Even => "Even number (XNOR)",
            Self::Odd => "Odd number (parity)",
            Self::Most => "Most (majority)",
        }
    }
    /// The logic-gate name, for badges.
    pub fn short(self) -> &'static str {
        match self {
            Self::All => "AND",
            Self::Any => "OR",
            Self::NotAll => "NAND",
            Self::NoneOf => "NOR",
            Self::One => "XOR",
            Self::Even => "XNOR",
            Self::Odd => "ODD",
            Self::Most => "MAJ",
        }
    }
    /// Whether the gate opens when `passed` of `total` conditions hold.
    pub fn open(self, passed: usize, total: usize) -> bool {
        match self {
            Self::All => passed == total,
            Self::Any => passed > 0,
            Self::NotAll => passed < total,
            Self::NoneOf => passed == 0,
            Self::One => passed == 1,
            Self::Even => passed.is_multiple_of(2),
            Self::Odd => passed % 2 == 1,
            Self::Most => passed * 2 > total,
        }
    }
    /// Whether, over a single condition, it opens just when that fails: NOT.
    pub fn inverts(self) -> bool {
        !self.open(1, 1)
    }
}

/// What a gate can see when it is checked.
#[derive(Clone, Copy, Debug)]
pub struct GateState<'a> {
    /// Counter 1 first.
    pub counters: &'a [i32],
    pub hidden: u32,
    /// Seconds since the round started.
    pub time: f32,
    /// Noticed tanks inside the trigger's area.
    pub inside: usize,
}

/// Most conditions one trigger checks.
pub const TRIGGER_CONDITIONS: usize = 8;

/// A gate: when a trigger fires, it acts only if this holds at that moment.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TriggerTest {
    #[serde(default)]
    pub kind: TestKind,
    /// The counter (1 to 255) or group (1 to 32) to look at. Inside and Time
    /// look at neither, so maps may leave it out.
    #[serde(default)]
    pub index: u8,
    #[serde(default)]
    pub compare: Compare,
    #[serde(default)]
    pub value: i32,
    /// Compare with this counter (1 to 255) instead of `value`; 0 uses `value`.
    #[serde(default)]
    pub against: u8,
}
impl Default for TriggerTest {
    fn default() -> Self {
        Self {
            kind: TestKind::Counter,
            index: 1,
            compare: Compare::AtLeast,
            value: 1,
            against: 0,
        }
    }
}
impl TriggerTest {
    /// Whether the condition holds, given the counters and the hidden groups.
    pub fn passes(&self, counters: &[i32; MAP_COUNTERS as usize], hidden: u32) -> bool {
        self.holds(&GateState {
            counters,
            hidden,
            time: 0.,
            inside: 0,
        })
    }
    /// Whether the condition holds in `state`.
    pub fn holds(&self, state: &GateState) -> bool {
        let counter = |n: u8| {
            usize::from(n)
                .checked_sub(1)
                .and_then(|slot| state.counters.get(slot).copied())
        };
        let a = match self.kind {
            TestKind::Counter => match counter(self.index) {
                Some(a) => a,
                None => return false,
            },
            TestKind::Inside => state.inside.min(COUNTER_LIMIT as usize) as i32,
            TestKind::Time => state.time.max(0.).floor().min(COUNTER_LIMIT as f32) as i32,
            TestKind::Shown => return state.hidden & MapDef::group_bit(self.index) == 0,
            TestKind::Hidden => return state.hidden & MapDef::group_bit(self.index) != 0,
        };
        let b = if self.against > 0 {
            counter(self.against).unwrap_or(0)
        } else {
            self.value
        };
        self.compare.holds(a, b)
    }
}

/// Which tanks an Enter or Exit trigger notices.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TriggerWho {
    #[default]
    Anyone,
    Players,
    Bots,
    /// Only the tank that set the trigger off: the one that crossed its
    /// area, or that crossed the area or picked the choice that sent the
    /// signal it heard. Area triggers watch for any tank.
    Cause,
}
impl TriggerWho {
    pub const ALL: [Self; 4] = [Self::Anyone, Self::Players, Self::Bots, Self::Cause];
    pub fn name(self) -> &'static str {
        match self {
            Self::Anyone => "Any tank",
            Self::Players => "Players only",
            Self::Bots => "Bots only",
            Self::Cause => "Tank that set it off",
        }
    }
}

/// The map widget slots, numbered from 1.
pub const MAP_UI_SLOTS: u8 = 32;

/// Who sees a Show UI trigger's widget.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UiAudience {
    /// Every player the trigger notices, wherever they are, and players
    /// who join while it shows.
    #[default]
    Everyone,
    /// The players the trigger notices inside its area.
    Inside,
    /// The player that set the trigger off.
    Cause,
}
impl UiAudience {
    pub const ALL: [Self; 3] = [Self::Everyone, Self::Inside, Self::Cause];
    pub fn name(self) -> &'static str {
        match self {
            Self::Everyone => "Everyone",
            Self::Inside => "Players in the area",
            Self::Cause => "Player that set it off",
        }
    }
}

/// Where a Show UI trigger's widget sits.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UiPin {
    /// On the screen, where the widget's `x` and `y` put it.
    #[default]
    Screen,
    /// Over the middle of the trigger's area.
    Area,
    /// Over the tank that set the trigger off, following it.
    Cause,
}
impl UiPin {
    pub const ALL: [Self; 3] = [Self::Screen, Self::Area, Self::Cause];
    pub fn name(self) -> &'static str {
        match self {
            Self::Screen => "On the screen",
            Self::Area => "Over the area",
            Self::Cause => "Over the tank that set it off",
        }
    }
}

/// A widget a Show UI trigger puts on players' screens. Its text and title
/// fill in `{1}` to `{8}` (counters, as in announcements), `{left}` (seconds
/// before it goes), `{time}` (the round's clock), `{name}` and `{score}`
/// (the viewer's) as they change.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MapUi {
    #[serde(serialize_with = "map_widget")]
    pub widget: crate::HudWidget,
    #[serde(default, skip_serializing_if = "is_default")]
    pub to: UiAudience,
    /// Fills the bar from counter `bar` (1 to 255) out of `bar_max`; 0 keeps
    /// the widget's own progress.
    #[serde(default, skip_serializing_if = "is_default")]
    pub bar: u8,
    #[serde(default, skip_serializing_if = "is_default")]
    pub bar_max: i32,
    /// Empties the bar as the trigger's duration runs out.
    #[serde(default, skip_serializing_if = "is_default")]
    pub timer: bool,
    /// The channel each choice signals, in order; the player that picks is
    /// the tank that set it off.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub channels: Vec<u8>,
    /// Whether a menu stays open after a pick.
    #[serde(default, skip_serializing_if = "is_default")]
    pub keep: bool,
    // Added in 1.5.0, and likewise left out when unused.
    /// What the widget floats over. Servers pin the widget itself; a map
    /// widget's own `pin` stays on the screen.
    #[serde(default, skip_serializing_if = "is_default")]
    pub pin: UiPin,
}

/// Writes a map widget leaving out the fields added in 1.5.0 while they
/// are unused, so maps that do without them still load in 1.4.0. Every
/// field is named here, so a new one cannot be forgotten.
fn map_widget<S: serde::Serializer>(widget: &crate::HudWidget, out: S) -> Result<S::Ok, S::Error> {
    #[derive(Serialize)]
    struct Written<'a> {
        text: &'a String,
        x: f32,
        y: f32,
        align: crate::HudAlign,
        size: f32,
        color: Option<[u8; 3]>,
        progress: Option<f32>,
        bar_color: Option<[u8; 3]>,
        width: f32,
        style: crate::HudStyle,
        title: &'a String,
        icon: crate::HudIcon,
        panel: Option<[u8; 3]>,
        opacity: f32,
        accent: Option<[u8; 3]>,
        enter: crate::HudEnter,
        pulse: bool,
        segments: u8,
        choices: &'a Vec<String>,
        menu: u32,
        #[serde(skip_serializing_if = "is_default")]
        pin: crate::HudPin,
        #[serde(skip_serializing_if = "is_default")]
        point: bool,
        #[serde(skip_serializing_if = "is_default")]
        layout: crate::HudLayout,
        #[serde(skip_serializing_if = "is_default")]
        glow: bool,
        #[serde(skip_serializing_if = "is_default")]
        shape: crate::HudShape,
        #[serde(skip_serializing_if = "is_default")]
        border: crate::HudBorder,
        #[serde(skip_serializing_if = "is_default")]
        fill: crate::HudFill,
        #[serde(skip_serializing_if = "is_default")]
        fill_color: Option<[u8; 3]>,
        #[serde(skip_serializing_if = "is_default")]
        lettering: crate::HudLettering,
        #[serde(skip_serializing_if = "is_default")]
        caps: bool,
        #[serde(skip_serializing_if = "is_default")]
        title_color: Option<[u8; 3]>,
        #[serde(skip_serializing_if = "is_default")]
        bar_style: crate::HudBar,
        #[serde(skip_serializing_if = "is_default")]
        buttons: crate::HudButtons,
        #[serde(skip_serializing_if = "Option::is_none")]
        board: Option<WrittenBoard<'a>>,
    }
    // Cells leave out what they don't use, so boards stay readable.
    #[derive(Serialize)]
    struct WrittenBoard<'a> {
        columns: u8,
        rows: u8,
        cell: f32,
        gap: f32,
        cells: Vec<WrittenCell<'a>>,
    }
    #[derive(Serialize)]
    struct WrittenCell<'a> {
        kind: crate::HudCellKind,
        at: [u8; 2],
        #[serde(skip_serializing_if = "is_one_square")]
        span: [u8; 2],
        #[serde(skip_serializing_if = "String::is_empty")]
        text: &'a String,
        #[serde(skip_serializing_if = "is_default")]
        channel: u8,
        #[serde(skip_serializing_if = "is_default")]
        counter: u8,
        #[serde(skip_serializing_if = "is_default")]
        bit: u8,
        #[serde(skip_serializing_if = "is_default")]
        group: u8,
        #[serde(skip_serializing_if = "is_hundred")]
        max: i32,
        #[serde(skip_serializing_if = "is_default")]
        color: Option<[u8; 3]>,
        #[serde(skip_serializing_if = "is_default")]
        fill: Option<[u8; 3]>,
        #[serde(skip_serializing_if = "is_default")]
        size: f32,
        #[serde(skip_serializing_if = "is_default")]
        align: crate::HudAlign,
    }
    fn is_one_square(span: &[u8; 2]) -> bool {
        *span == [1, 1]
    }
    fn is_hundred(max: &i32) -> bool {
        *max == 100
    }
    let crate::HudWidget {
        text,
        x,
        y,
        align,
        size,
        color,
        progress,
        bar_color,
        width,
        style,
        title,
        icon,
        panel,
        opacity,
        accent,
        enter,
        pulse,
        segments,
        choices,
        menu,
        pin,
        point,
        layout,
        glow,
        shape,
        border,
        fill,
        fill_color,
        lettering,
        caps,
        title_color,
        bar_style,
        buttons,
        board,
    } = widget;
    // Servers work out whether cells are lit and how full, so maps never
    // store it.
    let board = board.as_ref().map(|board| WrittenBoard {
        columns: board.columns,
        rows: board.rows,
        cell: board.cell,
        gap: board.gap,
        cells: board
            .cells
            .iter()
            .map(|cell| WrittenCell {
                kind: cell.kind,
                at: cell.at,
                span: cell.span,
                text: &cell.text,
                channel: cell.channel,
                counter: cell.counter,
                bit: cell.bit,
                group: cell.group,
                max: cell.max,
                color: cell.color,
                fill: cell.fill,
                size: cell.size,
                align: cell.align,
            })
            .collect(),
    });
    Written {
        text,
        x: *x,
        y: *y,
        align: *align,
        size: *size,
        color: *color,
        progress: *progress,
        bar_color: *bar_color,
        width: *width,
        style: *style,
        title,
        icon: *icon,
        panel: *panel,
        opacity: *opacity,
        accent: *accent,
        enter: *enter,
        pulse: *pulse,
        segments: *segments,
        choices,
        menu: *menu,
        pin: *pin,
        point: *point,
        layout: *layout,
        glow: *glow,
        shape: *shape,
        border: *border,
        fill: *fill,
        fill_color: *fill_color,
        lettering: *lettering,
        caps: *caps,
        title_color: *title_color,
        bar_style: *bar_style,
        buttons: *buttons,
        board,
    }
    .serialize(out)
}
impl MapUi {
    /// A widget for a new Show UI trigger: a card near the top.
    pub fn starter() -> Self {
        Self {
            widget: crate::HudWidget {
                title: "Notice".into(),
                text: "Write your message here".into(),
                y: 0.18,
                style: crate::HudStyle::Card,
                icon: crate::HudIcon::Info,
                enter: crate::HudEnter::Pop,
                ..crate::HudWidget::default()
            },
            ..Self::default()
        }
    }
    /// Whether the text or title names the viewer, so each player needs a
    /// copy of their own.
    pub fn personal(&self) -> bool {
        let names = |text: &str| text.contains("{name}") || text.contains("{score}");
        let board = self.widget.board.as_ref().is_some_and(|board| {
            board
                .cells
                .iter()
                .any(|cell| cell.kind == crate::HudCellKind::Button || names(&cell.text))
        });
        !self.widget.choices.is_empty()
            || board
            || names(&self.widget.text)
            || names(&self.widget.title)
    }
    /// Whether the text, title, bar or board change while it shows.
    pub fn live(&self) -> bool {
        let fills = |text: &str| text.contains('{');
        let board = self.widget.board.as_ref().is_some_and(|board| {
            board.cells.iter().any(|cell| {
                fills(&cell.text)
                    || cell.watches()
                    || (cell.kind == crate::HudCellKind::Meter && cell.counter > 0)
            })
        });
        self.bar > 0 || self.timer || board || fills(&self.widget.text) || fills(&self.widget.title)
    }
}

/// What fills a map widget's `{…}` fields.
pub struct UiFill<'a> {
    /// Counter 1 first.
    pub counters: &'a [i32],
    /// Hidden groups, group 1 as bit 0, for board lights that watch them.
    pub hidden: u32,
    pub left: f32,
    pub time: f32,
    pub name: &'a str,
    /// The tank that set off the trigger, or empty when it has no tank.
    pub cause: &'a str,
    pub score: u64,
}
/// `template` with its fields filled, cut to `max` bytes. Unknown fields
/// stay as written.
pub fn fill_ui_text(template: &str, fill: &UiFill, max: usize) -> String {
    if !template.contains('{') {
        return template.to_owned();
    }
    let mut out = String::with_capacity(template.len() + 8);
    let mut rest = template;
    while let Some(open) = rest.find('{') {
        out.push_str(&rest[..open]);
        let tail = &rest[open..];
        let Some(close) = tail.find('}') else {
            out.push_str(tail);
            rest = "";
            break;
        };
        let field = &tail[1..close];
        let clock = |seconds: f32| {
            let s = seconds.max(0.).ceil() as u64;
            format!("{}:{:02}", s / 60, s % 60)
        };
        let filled = match field {
            // A widget that stays has no time left to show.
            "left" if !fill.left.is_finite() => Some(String::new()),
            "left" => Some(format!("{}", fill.left.max(0.).ceil() as u64)),
            "time" => Some(clock(fill.time)),
            "name" => Some(fill.name.to_owned()),
            "cause" => Some(fill.cause.to_owned()),
            "score" => Some(fill.score.to_string()),
            _ => counter_field(field, fill.counters),
        };
        match filled {
            Some(text) => out.push_str(&text),
            None => out.push_str(&tail[..=close]),
        }
        rest = &tail[close + 1..];
    }
    out.push_str(rest);
    crate::clip_text(&out, max)
}

/// A rule that runs during a round: toggles, timed gates, hazards that come
/// and go, signals, counters, announcements and boss arrivals.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MapTrigger {
    pub when: TriggerWhen,
    pub action: TriggerAction,
    /// The area an Enter trigger watches; where a boss arrives. Also where
    /// the editor shows the trigger.
    pub rect: [f32; 4],
    /// The group Toggle, Show and Hide act on.
    #[serde(default)]
    pub group: u8,
    #[serde(default)]
    pub seconds: f32,
    #[serde(default)]
    pub offset: f32,
    /// When above zero, the change reverts after this many seconds.
    #[serde(default)]
    pub duration: f32,
    #[serde(default)]
    pub text: String,
    #[serde(default)]
    pub boss: String,
    /// Fires at most once a round.
    #[serde(default)]
    pub once: bool,
    /// The group this trigger belongs to. While that group is hidden the
    /// trigger is switched off; group 0 is always on.
    #[serde(default)]
    pub member: u8,
    /// Which tanks area triggers and tank actions notice.
    #[serde(default)]
    pub who: TriggerWho,
    /// Area triggers and tank actions notice only this team (1 to 4); 0 is
    /// any team.
    #[serde(default)]
    pub team: u8,
    /// The channel a Signal trigger listens on, the counter a Counter or
    /// Changed trigger watches, the group a Shown or Hidden trigger watches,
    /// or the boss a Defeated trigger waits for (see [`TriggerWhen`]).
    #[serde(default)]
    pub source: u8,
    /// The value a Counter trigger waits for, or how many tanks a While
    /// inside trigger waits for.
    #[serde(default)]
    pub threshold: i32,
    /// How much Count adds (negative subtracts) or Set stores, how many
    /// shapes spawn or channels Random picks from, the percent of health
    /// Repair and Damage act on, or the points Award gives.
    #[serde(default)]
    pub amount: i32,
    /// Percent chance, 1 to 100, that the trigger acts when it fires.
    #[serde(default = "always")]
    pub chance: u8,
    /// Seconds between firing and acting.
    #[serde(default)]
    pub delay: f32,
    // Newer fields are left out when unused, so maps that do without them
    // still load in 0.15.0. Players never receive triggers (servers strip
    // them), so the binary wire format never meets these skipped fields.
    /// A gate checked when the trigger fires; it acts only if this holds.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub only_if: Option<TriggerTest>,
    /// How Calculate changes its counter.
    #[serde(default, skip_serializing_if = "is_default")]
    pub math: MathOp,
    /// Calculate uses counter `operand` (1 to 255) instead of `amount`; 0
    /// uses `amount`.
    #[serde(default, skip_serializing_if = "is_default")]
    pub operand: u8,
    // Added in 0.16.0, and likewise left out when unused.
    /// More conditions after `only_if`, up to eight in all.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub also_if: Vec<TriggerTest>,
    /// How the conditions combine.
    #[serde(default, skip_serializing_if = "is_default")]
    pub gate: GateMode,
    /// A signal channel sent instead when the gate stays shut; 0 sends none.
    #[serde(default, skip_serializing_if = "is_default")]
    pub otherwise: u8,
    // Added in 1.0.0, and likewise left out when unused.
    /// How far Move glides its group, or where Move to puts it, as a
    /// fraction of the arena's width and height.
    #[serde(default, skip_serializing_if = "is_default")]
    pub travel: [f32; 2],
    /// Seconds a motion takes; 0 is instant.
    #[serde(default, skip_serializing_if = "is_default")]
    pub tween: f32,
    /// How a motion speeds up and slows down.
    #[serde(default, skip_serializing_if = "is_default")]
    pub easing: crate::Easing,
    // Added in 1.1.0, and likewise left out when unused.
    /// The colour Tint mixes in.
    #[serde(default, skip_serializing_if = "is_default")]
    pub color: [u8; 3],
    /// The animation Animate plays.
    #[serde(default, skip_serializing_if = "is_default")]
    pub style: crate::AnimateStyle,
    /// Animate's cycles a second; 0 uses the style's own pace.
    #[serde(default, skip_serializing_if = "is_default")]
    pub speed: f32,
    /// The sound Play sound makes.
    #[serde(default, skip_serializing_if = "is_default")]
    pub sound: MapSound,
    // Added in 1.3.0, and likewise left out when unused.
    /// Which way Shove body and Fling tanks push, in degrees: 0 right, 90
    /// down.
    #[serde(default, skip_serializing_if = "is_default")]
    pub angle: f32,
    // Added in 1.4.0, and likewise left out when unused.
    /// The widget a Show UI trigger shows, and who a Hide UI trigger hides
    /// it from.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ui: Option<Box<MapUi>>,
    // Added in 1.7.0, and likewise left out when unused.
    /// Calculate changes counter `group` plus the value of this counter, for
    /// lists of counters; 0 changes counter `group` itself.
    #[serde(default, skip_serializing_if = "is_default")]
    pub target_offset: u8,
    /// Calculate reads counter `operand` plus the value of this counter; 0
    /// reads counter `operand` itself.
    #[serde(default, skip_serializing_if = "is_default")]
    pub operand_offset: u8,
    /// Zoom view stays until another Zoom view changes it, instead of for
    /// `tween` seconds.
    #[serde(default, skip_serializing_if = "is_default")]
    pub hold: bool,
    /// How far Zoom view turns the view toward the middle of the area, in
    /// percent.
    #[serde(default, skip_serializing_if = "is_default")]
    pub focus: u8,
    /// Seconds Zoom view takes to change the view, with `easing`; 0 takes
    /// half a second.
    #[serde(default, skip_serializing_if = "is_default")]
    pub ease: f32,
}
impl MapTrigger {
    /// Every signal channel acting can send on: its action's, each choice's
    /// and each board button's. The otherwise channel, sent instead of
    /// acting, is apart.
    pub fn sends(&self) -> impl Iterator<Item = u8> + '_ {
        let ui = match (&self.ui, self.action) {
            (Some(ui), TriggerAction::Ui) => Some(ui.as_ref()),
            _ => None,
        };
        let choices = ui.map_or(&[][..], |ui| ui.channels.as_slice());
        let cells = ui
            .and_then(|ui| ui.widget.board.as_ref())
            .map_or(&[][..], |board| board.cells.as_slice());
        // Channel 0 is no channel, wherever it turns up.
        self.action
            .channels(self.group, self.amount)
            .chain(choices.iter().copied())
            .chain(
                cells
                    .iter()
                    .filter(|cell| cell.kind == crate::HudCellKind::Button)
                    .map(|cell| cell.channel),
            )
            .filter(|&c| c > 0)
    }
    /// Animate's pace in cycles a second, after the style's default.
    pub fn animate_speed(&self) -> f32 {
        if self.speed > 0. {
            self.speed.min(crate::ANIMATE_SPEED)
        } else {
            self.style.default_speed()
        }
    }
}
fn always() -> u8 {
    100
}
fn is_default<T: Default + PartialEq>(value: &T) -> bool {
    *value == T::default()
}
/// Script event names: lowercase letters, digits and `_`, starting with a
/// letter, at most 32 characters.
pub fn script_event_name(name: &str) -> bool {
    let mut chars = name.chars();
    chars.next().is_some_and(|c| c.is_ascii_lowercase())
        && name.len() <= 32
        && chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
}
impl Default for MapTrigger {
    fn default() -> Self {
        Self {
            when: TriggerWhen::Every,
            action: TriggerAction::Toggle,
            rect: [0., 0., 0.05, 0.05],
            group: 1,
            seconds: 10.,
            offset: 0.,
            duration: 0.,
            text: String::new(),
            boss: String::new(),
            once: false,
            member: 0,
            who: TriggerWho::Anyone,
            team: 0,
            source: 1,
            threshold: 1,
            amount: 1,
            chance: 100,
            delay: 0.,
            only_if: None,
            math: MathOp::Add,
            operand: 0,
            also_if: Vec::new(),
            gate: GateMode::All,
            otherwise: 0,
            travel: [0., 0.],
            tween: 0.,
            easing: crate::Easing::default(),
            color: [0, 0, 0],
            style: crate::AnimateStyle::Bob,
            speed: 0.,
            sound: MapSound::Chime,
            angle: 0.,
            ui: None,
            target_offset: 0,
            operand_offset: 0,
            hold: false,
            focus: 0,
            ease: 0.,
        }
    }
}
impl MapTrigger {
    /// Every condition, in order.
    pub fn conditions(&self) -> impl Iterator<Item = &TriggerTest> {
        self.only_if.iter().chain(&self.also_if)
    }
    /// Replace the conditions; with none left, the gate settings go too.
    pub fn set_conditions(&mut self, tests: Vec<TriggerTest>) {
        let mut tests = tests.into_iter().take(TRIGGER_CONDITIONS);
        self.only_if = tests.next();
        self.also_if = tests.collect();
        if self.only_if.is_none() {
            self.gate = GateMode::All;
            self.otherwise = 0;
        }
    }
    /// The counters (into `counters`, each once) and groups (as bits) its
    /// conditions look at: what wakes an On input change trigger. Tanks
    /// inside and the round time change without telling anyone, so they
    /// only count through a counter they are compared with.
    pub fn inputs(&self, counters: &mut Vec<u8>) -> u32 {
        counters.clear();
        let mut groups = 0;
        for test in self.conditions() {
            if !test.kind.compares() {
                groups |= MapDef::group_bit(test.index);
                continue;
            }
            if test.kind == TestKind::Counter {
                counters.push(test.index);
            }
            counters.push(test.against);
        }
        counters.retain(|counter| (1..=MAP_COUNTERS).contains(counter));
        counters.sort_unstable();
        counters.dedup();
        groups
    }
    /// Whether the gate lets the trigger act in `state`. No conditions is open.
    pub fn gate_open(&self, state: &GateState) -> bool {
        let (mut passed, mut total) = (0, 0);
        for test in self.conditions() {
            total += 1;
            passed += usize::from(test.holds(state));
        }
        total == 0 || self.gate.open(passed, total)
    }
    /// Whether a condition counts the tanks inside the trigger's area.
    pub fn counts_inside(&self) -> bool {
        self.conditions().any(|test| test.kind == TestKind::Inside)
    }
}

/// The shortest period of an Every trigger, in seconds: three ticks.
pub const EVERY_LEAST: f32 = 0.05;
/// Counters triggers can add to and watch; labels show them as `{1}` to
/// `{255}`.
pub const MAP_COUNTERS: u8 = 255;
/// Counter values stay within this magnitude: 30 bits, so two of them add
/// up without overflowing and bitwise math keeps every pattern.
pub const COUNTER_LIMIT: i32 = (1 << 30) - 1;
/// Signal channels triggers and widget buttons send on, from 1.
pub const MAP_CHANNELS: u8 = 255;

/// Every map counter's value, counter 1 first.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MapCounters(pub [i32; MAP_COUNTERS as usize]);
impl Default for MapCounters {
    fn default() -> Self {
        Self([0; MAP_COUNTERS as usize])
    }
}
impl std::ops::Deref for MapCounters {
    type Target = [i32];
    fn deref(&self) -> &[i32] {
        &self.0
    }
}
impl std::ops::DerefMut for MapCounters {
    fn deref_mut(&mut self) -> &mut [i32] {
        &mut self.0
    }
}
impl MapCounters {
    /// The counters `shown` names, as (counter, value) pairs, set over 0.
    pub fn from_shown(shown: &[(u8, i32)]) -> Self {
        let mut counters = Self::default();
        for &(counter, value) in shown {
            if let Some(slot) = usize::from(counter)
                .checked_sub(1)
                .and_then(|slot| counters.0.get_mut(slot))
            {
                *slot = value;
            }
        }
        counters
    }
}

/// Add the counters `text`'s fields name (see [`counter_field`]) to
/// `named`, each once, in no particular order.
pub fn counters_named(text: &str, named: &mut Vec<u8>) {
    // Every counter present, so a field fills in exactly when it names
    // counters; read as [`fill_counters`] reads.
    static EVERY: [i32; MAP_COUNTERS as usize] = [1; MAP_COUNTERS as usize];
    let mut rest = text;
    while let Some(open) = rest.find('{') {
        let after = &rest[open + 1..];
        let field = after
            .find('}')
            .filter(|&close| counter_field(&after[..close], &EVERY).is_some());
        let Some(close) = field else {
            rest = after;
            continue;
        };
        let names = after[..close].split(':').next().unwrap_or("");
        for name in names.split('|') {
            if let Ok(counter) = name.parse::<u8>()
                && !named.contains(&counter)
            {
                named.push(counter);
            }
        }
        rest = &after[close + 1..];
    }
}
/// How far one Move or Move to trigger may glide its group, in arena widths
/// or heights.
pub const TRAVEL_LIMIT: f32 = 2.;
/// Fastest average glide, in arena widths or heights a second. Easing makes
/// the middle of a motion faster; moving cover pushes tanks along with it.
pub const MOVE_SPEED_LIMIT: f32 = 1.;
/// Most degrees one Turn trigger turns.
pub const TURN_LIMIT: i32 = 3600;
/// Longest Shake screen, in seconds.
pub const SHAKE_SECONDS: f32 = 10.;
/// The shortest glide, in tenths of a second, that keeps a move of
/// `travel` within `MOVE_SPEED_LIMIT`.
pub fn least_tween(travel: [f32; 2]) -> f32 {
    let far = travel[0].abs().max(travel[1].abs());
    if far.is_finite() {
        (far / MOVE_SPEED_LIMIT * 10.).ceil() / 10.
    } else {
        0.
    }
}
/// At most this many shapes from one Spawn shapes trigger.
pub const TRIGGER_SHAPES: i32 = 24;
/// At most this many points from one Award points trigger.
pub const TRIGGER_AWARD: i32 = 100_000;
/// A While inside trigger waits for at most this many tanks.
pub const TRIGGER_CROWD: i32 = 64;

/// `text` with its counter fields (see [`counter_field`]) filled from
/// `counters`, counter 1 first.
pub fn fill_counters<'a>(text: &'a str, counters: &[i32]) -> std::borrow::Cow<'a, str> {
    if !text.contains('{') {
        return text.into();
    }
    let mut out = String::with_capacity(text.len() + 8);
    let mut rest = text;
    while let Some(open) = rest.find('{') {
        out.push_str(&rest[..open]);
        let after = &rest[open..];
        let filled = after
            .find('}')
            .and_then(|close| Some((close, counter_field(&after[1..close], counters)?)));
        match filled {
            Some((close, value)) => {
                out.push_str(&value);
                rest = &after[close + 1..];
            }
            None => {
                out.push('{');
                rest = &after[1..];
            }
        }
    }
    out.push_str(rest);
    out.into()
}

/// The text for a `{…}` field that names counters, from `counters` (counter
/// 1 first), or `None` when it names none:
///
/// - `{12}`: counter 12.
/// - `{12:,}`: with commas between groups of three digits.
/// - `{12:04}`: at least four digits, with zeros in front.
/// - `{12:b}`, `{12:b8}`: binary, optionally at least eight digits.
/// - `{12:x}`, `{12:x4}`: hexadecimal, optionally at least four digits.
/// - `{3|2|1:9}`: counters 3, 2 and 1 as nine-digit parts of one long
///   number, most significant first, as `{3|2|1:9,}` with commas. Two to
///   eight parts of one to nine digits; the first part that isn't 0 gives
///   the sign.
pub fn counter_field(field: &str, counters: &[i32]) -> Option<String> {
    let (names, spec) = match field.split_once(':') {
        // A colon needs a format after it.
        Some((_, "")) => return None,
        Some(split) => split,
        None => (field, ""),
    };
    let value = |name: &str| {
        if name.is_empty() || name.len() > 3 || name.starts_with('0') {
            return None;
        }
        let number: usize = name.parse().ok()?;
        counters.get(number.checked_sub(1)?).copied()
    };
    // Widths of one or two digits, from 1 to `most`.
    let width = |digits: &[u8], most: usize| -> Option<usize> {
        if digits.is_empty() || digits.len() > 2 || !digits.iter().all(u8::is_ascii_digit) {
            return None;
        }
        let width = digits
            .iter()
            .fold(0, |sum, &d| sum * 10 + usize::from(d - b'0'));
        (1..=most).contains(&width).then_some(width)
    };
    if names.contains('|') {
        let parts = names.split('|').map(value).collect::<Option<Vec<i32>>>()?;
        let (digits, commas) = match spec.strip_suffix(',') {
            Some(digits) => (digits, true),
            None => (spec, false),
        };
        return match digits.as_bytes() {
            &[d @ b'1'..=b'9'] if (2..=8).contains(&parts.len()) => {
                Some(joined_number(&parts, usize::from(d - b'0'), commas))
            }
            _ => None,
        };
    }
    let number = value(names)?;
    let sign = if number < 0 { "-" } else { "" };
    let magnitude = number.unsigned_abs();
    Some(match spec.as_bytes() {
        [] => number.to_string(),
        [b','] => format!("{sign}{}", digit_groups(&magnitude.to_string())),
        [b'0', digits @ ..] => {
            let w = width(digits, 20)?;
            format!("{sign}{magnitude:0w$}")
        }
        [b'b'] => format!("{sign}{magnitude:b}"),
        [b'b', digits @ ..] => {
            let w = width(digits, 32)?;
            format!("{sign}{magnitude:0w$b}")
        }
        [b'x'] => format!("{sign}{magnitude:X}"),
        [b'x', digits @ ..] => {
            let w = width(digits, 8)?;
            format!("{sign}{magnitude:0w$X}")
        }
        _ => return None,
    })
}
/// `parts`, most significant first, as one number of `digits` digits a
/// part, with commas between groups of three when `commas`.
fn joined_number(parts: &[i32], digits: usize, commas: bool) -> String {
    let negative = parts
        .iter()
        .find(|&&part| part != 0)
        .is_some_and(|&part| part < 0);
    let mut text = String::new();
    for part in parts {
        let magnitude = part.unsigned_abs();
        if !text.is_empty() {
            text.push_str(&format!("{magnitude:0digits$}"));
        } else if magnitude != 0 {
            text = magnitude.to_string();
        }
    }
    if text.is_empty() {
        text.push('0');
    }
    if commas {
        text = digit_groups(&text);
    }
    if negative {
        text.insert(0, '-');
    }
    text
}
/// ASCII `digits` with a comma between each group of three from the right.
fn digit_groups(digits: &str) -> String {
    let mut out = String::with_capacity(digits.len() + digits.len() / 3);
    for (index, digit) in digits.chars().enumerate() {
        if index > 0 && (digits.len() - index).is_multiple_of(3) {
            out.push(',');
        }
        out.push(digit);
    }
    out
}

/// Most groups a map can name; they fit one 32-bit mask in snapshots.
pub const MAP_GROUPS: u8 = 32;
/// Most walls and blocks together, pads, decorations and triggers in one
/// map.
pub const MAP_BLOCKS: usize = 2048;
pub const MAP_ZONES: usize = 512;
pub const MAP_DECOR: usize = 4096;
pub const MAP_TRIGGERS: usize = 4096;
/// Fastest Shove body, in world units a second.
pub const SHOVE_LIMIT: i32 = 2000;
/// Fastest Fling tanks or Blast, in world units a second.
pub const FLING_LIMIT: i32 = 3000;
/// Zoom view's range, in percent of the usual view.
pub const ZOOM_RANGE: (i32, i32) = (40, 250);
/// Longest Zoom view, in seconds.
pub const ZOOM_SECONDS: f32 = 60.;
/// Longest change from one Zoom view to the next, in seconds.
pub const ZOOM_EASE: f32 = 10.;
/// Stable IDs accepted by map boss triggers. Core verifies this catalog stays
/// in sync with its boss enum.
pub const BOSS_IDS: [&str; 14] = [
    "sentinel",
    "prism",
    "foundry",
    "tempest",
    "bastion",
    "eclipse",
    "guardian",
    "summoner",
    "defender",
    "fallen_overlord",
    "fallen_booster",
    "siege_engine",
    "skirmisher",
    "trapper",
];

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MapDef {
    pub id: String,
    pub name: String,
    pub description: String,
    /// Suggested dimensions. Explicit server arena dimensions take precedence.
    pub width: f32,
    pub height: f32,
    /// Recommended capacity; the server's player limit remains authoritative.
    pub capacity: u16,
    pub recommended_modes: Vec<String>,
    /// Normalized x, y, width and height, each within the unit arena rectangle.
    pub walls: Vec<[f32; 4]>,
    pub procedural: MapProcedure,
    pub objective_positions: Vec<[f32; 2]>,
    pub spawn_regions: Vec<SpawnRegion>,
    pub team_bases: Vec<TeamBase>,
    pub background: String,
    /// The look of `walls` and of blocks left at `Map`. `Map` picks one from
    /// the arena's id, as built-in arenas do.
    #[serde(default)]
    pub style: BlockStyle,
    #[serde(default)]
    pub blocks: Vec<MapBlock>,
    #[serde(default)]
    pub zones: Vec<MapZone>,
    #[serde(default)]
    pub decor: Vec<MapDecor>,
    #[serde(default)]
    pub triggers: Vec<MapTrigger>,
    /// Groups hidden when a round starts, such as gates a trigger opens.
    #[serde(default)]
    pub hidden_groups: Vec<u8>,
    // Added in 1.3.0.
    /// Groups tanks can push, such as crates and push doors.
    #[serde(default)]
    pub bodies: Vec<crate::MapBody>,
    /// Keyframed routes Play path triggers send groups along.
    #[serde(default)]
    pub paths: Vec<crate::MapPath>,
    /// How dark the arena is when a round starts, and the color of its
    /// light. Lights, spotlights and beams shine through the dark.
    #[serde(default)]
    pub ambient: MapAmbient,
    /// Names for groups, shown in the editor.
    #[serde(default)]
    pub group_names: Vec<GroupName>,
}

/// An arena's lighting.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MapAmbient {
    /// 0 is fully lit; 0.95 is nearly black.
    #[serde(default)]
    pub darkness: f32,
    /// The color the dark takes on.
    #[serde(default = "night_color")]
    pub color: [u8; 3],
}
fn night_color() -> [u8; 3] {
    [8, 10, 26]
}
impl Default for MapAmbient {
    fn default() -> Self {
        Self {
            darkness: 0.,
            color: night_color(),
        }
    }
}
/// Darkest a map may be.
pub const DARKNESS_LIMIT: f32 = 0.95;

/// A name for a group, so the editor can say "Vault door" instead of "7".
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GroupName {
    pub group: u8,
    pub name: String,
}
impl MapDef {
    /// The bit of `group` in a hidden-group mask; group 0 has none.
    pub fn group_bit(group: u8) -> u32 {
        if (1..=MAP_GROUPS).contains(&group) {
            1 << (group - 1)
        } else {
            0
        }
    }
    /// The editor's name for `group`, if it has one.
    pub fn group_name(&self, group: u8) -> Option<&str> {
        self.group_names
            .iter()
            .find(|named| named.group == group)
            .map(|named| named.name.as_str())
    }
    /// The body made of `group`'s blocks, if it is one.
    pub fn body(&self, group: u8) -> Option<&crate::MapBody> {
        self.bodies.iter().find(|body| body.group == group)
    }
    /// Groups hidden at the start of a round, as a mask.
    pub fn starting_mask(&self) -> u32 {
        self.hidden_groups
            .iter()
            .fold(0, |mask, &group| mask | Self::group_bit(group))
    }
    /// Every solid rectangle (normalized) while the groups in `hidden` are
    /// hidden: plain walls first, then blocks in order.
    pub fn solid_rects(&self, hidden: u32) -> impl Iterator<Item = [f32; 4]> + '_ {
        self.walls.iter().copied().chain(
            self.blocks
                .iter()
                .filter(move |block| Self::group_bit(block.group) & hidden == 0)
                .map(|block| block.rect),
        )
    }
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct MapCatalog {
    maps: Vec<MapDef>,
}

pub fn builtin_maps() -> Vec<MapDef> {
    maps_from_json(include_str!("../../../assets/content/maps.json"))
        .expect("compiled-in arena definitions must be valid")
}

pub fn maps_from_json(source: &str) -> Result<Vec<MapDef>, String> {
    let catalog: MapCatalog =
        serde_json::from_str(source).map_err(|error| format!("invalid map JSON: {error}"))?;
    validate_maps(&catalog.maps)?;
    Ok(catalog.maps)
}

pub fn validate_maps(maps: &[MapDef]) -> Result<(), String> {
    validate_map_catalog(maps, true)
}

/// Validate maps received from a server. These contain presentation and
/// collision data, without the server's triggers or paths. Their closed doors
/// can divide the authored geometry, so connectivity is checked on the full
/// server catalog instead. All bounds, clearance and resource checks remain.
pub fn validate_player_maps(maps: &[MapDef]) -> Result<(), String> {
    if maps
        .iter()
        .any(|map| !map.triggers.is_empty() || !map.paths.is_empty())
    {
        return Err("player maps must not contain server triggers or paths".into());
    }
    validate_map_catalog(maps, false)
}

fn validate_map_catalog(maps: &[MapDef], check_connectivity: bool) -> Result<(), String> {
    if maps.is_empty() || maps.len() > 256 {
        return Err("a map catalog needs between 1 and 256 arenas".into());
    }
    // Wire size alone does not bound geometric work: a compact catalog can
    // request hundreds of large navigation grids against thousands of walls.
    // Reject the whole catalog before starting any connectivity traversal.
    let mut geometry_work = 0_u64;
    for map in maps {
        if !range(map.width, 900., 16000.)
            || !range(map.height, 700., 16000.)
            || map.walls.len() + map.blocks.len() > 2048
        {
            return Err(format!(
                "{}: invalid dimensions or excessive geometry",
                map.id
            ));
        }
        geometry_work += (map.width / 80.).ceil() as u64
            * (map.height / 80.).ceil() as u64
            * (map.walls.len() + map.blocks.len()).max(1) as u64;
        if geometry_work > 16_000_000 {
            return Err("map catalog exceeds the geometry validation budget".into());
        }
    }
    let mut ids = HashSet::new();
    for map in maps {
        if map.id.is_empty()
            || map.id.len() > 48
            || !map
                .id
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_')
            || !ids.insert(map.id.as_str())
        {
            return Err(format!("invalid or duplicate map identifier {}", map.id));
        }
        if map.name.trim().is_empty()
            || map.name.len() > 64
            || map.name.chars().any(char::is_control)
            || map.description.trim().is_empty()
            || map.description.len() > 2048
            || map.description.chars().any(char::is_control)
        {
            return Err(format!("{}: invalid arena display text", map.id));
        }
        if !range(map.width, 900., 16000.)
            || !range(map.height, 700., 16000.)
            || !(2..=128).contains(&map.capacity)
        {
            return Err(format!("{}: invalid dimensions or capacity", map.id));
        }
        if map.recommended_modes.is_empty() || map.recommended_modes.len() > 14 {
            return Err(format!(
                "{}: missing or oversized mode recommendations",
                map.id
            ));
        }
        let mut modes = HashSet::new();
        for mode in &map.recommended_modes {
            let canonical = mode.replace('-', "_");
            if !matches!(
                canonical.as_str(),
                "ffa"
                    | "two_teams"
                    | "four_teams"
                    | "maze"
                    | "domination"
                    | "survival"
                    | "tag"
                    | "infection"
                    | "king_of_the_hill"
                    | "mothership"
                    | "capture_core"
                    | "boss_rush"
                    | "deathmatch"
                    | "sandbox"
                    | "random_events"
                    | "coop_pve"
            ) || !modes.insert(canonical)
            {
                return Err(format!(
                    "{}: invalid or duplicate mode reference {mode}",
                    map.id
                ));
            }
        }
        if map.walls.len() > 2048
            || map.objective_positions.len() > 32
            || map.spawn_regions.len() > 32
            || map.team_bases.len() > 4
        {
            return Err(format!("{}: excessive arena geometry", map.id));
        }
        for &wall in &map.walls {
            validate_rect(wall).map_err(|error| format!("{} wall: {error}", map.id))?;
        }
        validate_map_features(map)?;
        for &position in &map.objective_positions {
            validate_point(position).map_err(|error| format!("{} objective: {error}", map.id))?;
            if !point_clear(position[0] * map.width, position[1] * map.height, map, 42.) {
                return Err(format!("{}: objective overlaps a wall or boundary", map.id));
            }
        }
        if map.spawn_regions.is_empty() {
            return Err(format!("{}: arena needs a spawn region", map.id));
        }
        for region in &map.spawn_regions {
            validate_rect(region.rect).map_err(|error| format!("{} spawn: {error}", map.id))?;
            let [x, y, w, h] = region.rect;
            if region.team > 4
                || w * map.width < 80.
                || h * map.height < 80.
                || !point_clear(
                    (x + w * 0.5) * map.width,
                    (y + h * 0.5) * map.height,
                    map,
                    37.,
                )
            {
                return Err(format!(
                    "{}: spawn region lacks hull clearance or has invalid team",
                    map.id
                ));
            }
        }
        let mut teams = HashSet::new();
        for base in &map.team_bases {
            validate_point(base.position).map_err(|error| format!("{} base: {error}", map.id))?;
            if !(1..=4).contains(&base.team) || !teams.insert(base.team) {
                return Err(format!("{}: invalid or duplicate team base", map.id));
            }
            if !point_clear(
                base.position[0] * map.width,
                base.position[1] * map.height,
                map,
                75.,
            ) {
                return Err(format!("{}: team base lacks clearance", map.id));
            }
            if !map.spawn_regions.iter().any(|region| {
                let [x, y, w, h] = region.rect;
                region.team == base.team
                    && base.position[0] >= x
                    && base.position[0] <= x + w
                    && base.position[1] >= y
                    && base.position[1] <= y + h
            }) {
                return Err(format!(
                    "{}: team base has no matching spawn region",
                    map.id
                ));
            }
        }
        if !matches!(
            map.background.as_str(),
            "slate" | "teal" | "sand" | "violet"
        ) {
            return Err(format!(
                "{}: unknown background palette {}",
                map.id, map.background
            ));
        }
        // Procedural generators validate their generated geometry in core. This
        // pass verifies the complete static geometry declared by every preset.
        if check_connectivity {
            validate_connectivity(map)?;
        }
    }
    Ok(())
}

/// Blocks, pads, decor, triggers and groups: counts, ranges and text.
pub fn validate_map_features(map: &MapDef) -> Result<(), String> {
    let id = &map.id;
    if map.walls.len() + map.blocks.len() > MAP_BLOCKS
        || map.zones.len() > MAP_ZONES
        || map.decor.len() > MAP_DECOR
        || map.triggers.len() > MAP_TRIGGERS
        || map.hidden_groups.len() > usize::from(MAP_GROUPS)
    {
        return Err(format!(
            "{id}: at most {MAP_BLOCKS} blocks, {MAP_ZONES} pads, {MAP_DECOR} decorations and {MAP_TRIGGERS} triggers"
        ));
    }
    if map.bodies.len() > usize::from(MAP_GROUPS)
        || map.paths.len() > crate::MAP_PATHS
        || map.group_names.len() > usize::from(MAP_GROUPS)
    {
        return Err(format!("{id}: too many bodies, paths or group names"));
    }
    for (index, body) in map.bodies.iter().enumerate() {
        body.validate()
            .map_err(|error| format!("{id}: body {}: {error}", index + 1))?;
        if map.bodies[..index].iter().any(|b| b.group == body.group) {
            return Err(format!("{id}: group {} is two bodies", body.group));
        }
    }
    for (index, path) in map.paths.iter().enumerate() {
        path.validate()
            .map_err(|error| format!("{id}: path {}: {error}", index + 1))?;
    }
    if !range(map.ambient.darkness, 0., DARKNESS_LIMIT) {
        return Err(format!("{id}: darkness must be 0 to {}", DARKNESS_LIMIT));
    }
    for (index, named) in map.group_names.iter().enumerate() {
        if !(1..=MAP_GROUPS).contains(&named.group)
            || named.name.trim().is_empty()
            || !display_text(&named.name, 24)
            || map.group_names[..index]
                .iter()
                .any(|n| n.group == named.group)
        {
            return Err(format!(
                "{id}: each group 1 to {MAP_GROUPS} has at most one name of up to 24 characters"
            ));
        }
    }
    let group = |group: u8| group <= MAP_GROUPS;
    for block in &map.blocks {
        validate_rect(block.rect).map_err(|error| format!("{id} block: {error}"))?;
        if !group(block.group) {
            return Err(format!("{id}: block group must be 0 to {MAP_GROUPS}"));
        }
    }
    for (index, zone) in map.zones.iter().enumerate() {
        let name = format!("{id}: {} {}", zone.kind.name().to_lowercase(), index + 1);
        validate_rect(zone.rect).map_err(|error| format!("{name}: {error}"))?;
        if !range(zone.strength, 0.25, 2.) {
            return Err(format!("{name}: strength must be 0.25 to 2"));
        }
        if !group(zone.group) {
            return Err(format!("{name}: group must be 0 to {MAP_GROUPS}"));
        }
        if !zone.angle.is_finite() {
            return Err(format!("{name}: direction must be a number"));
        }
        if zone.kind == ZoneKind::Portal {
            validate_point(zone.exit).map_err(|error| format!("{name} exit: {error}"))?;
            if !portal_exit_clear(map, zone.exit) {
                return Err(format!(
                    "{name}: its exit is inside cover or too near the edge"
                ));
            }
        }
    }
    let mut emitters = 0;
    for decor in &map.decor {
        validate_rect(decor.rect).map_err(|error| format!("{id} decoration: {error}"))?;
        if !decor.rotation.is_finite()
            || !range(decor.pulse, 0., 4.)
            || !range(decor.spin, -720., 720.)
            || !range(decor.flicker, 0., 1.)
            || !group(decor.group)
            || !display_text(&decor.text, 64)
            || !range(decor.reach, 0., 2000.)
        {
            return Err(format!("{id}: decoration settings are out of range"));
        }
        if let Some(effect) = &decor.effect {
            effect.validate().map_err(|e| format!("{id}: {e}"))?;
        }
        if decor.kind == DecorKind::Emitter {
            emitters += 1;
            decor
                .effect
                .unwrap_or_default()
                .validate()
                .map_err(|error| format!("{id}: {error}"))?;
        }
    }
    if emitters > 48 {
        return Err(format!("{id}: at most 48 particle emitters"));
    }
    for (index, trigger) in map.triggers.iter().enumerate() {
        // A Path with no path to play or a Shove on a group that isn't a
        // body does nothing; the workshop points them out while building.
        validate_trigger(trigger)
            .map_err(|error| format!("{id}: trigger {}: {error}", index + 1))?;
    }
    if map
        .hidden_groups
        .iter()
        .any(|&g| !(1..=MAP_GROUPS).contains(&g))
    {
        return Err(format!("{id}: hidden groups must be 1 to {MAP_GROUPS}"));
    }
    Ok(())
}

/// Whether `counter` names a counter, or is 0 for none.
fn counter_or_none(counter: u8) -> bool {
    counter == 0 || (1..=MAP_COUNTERS).contains(&counter)
}
/// Whether `channel` names a signal channel, or is 0 for none.
fn channel_or_none(channel: u8) -> bool {
    channel == 0 || (1..=MAP_CHANNELS).contains(&channel)
}

/// One trigger's settings, with a reason a map author can act on.
fn validate_ui(ui: &MapUi) -> Result<(), String> {
    if !ui.widget.valid() {
        return Err(format!(
            "give the widget text, a title, a bar or choices; text up to {} bytes, a title up to {}, up to {} choices of {} bytes, sizes 10 to 48",
            crate::HUD_TEXT,
            crate::HUD_TITLE,
            crate::HUD_CHOICES,
            crate::HUD_CHOICE
        ));
    }
    if ui.widget.menu != 0 {
        return Err("leave the widget's menu number to the game".into());
    }
    if ui.widget.pinned() {
        return Err("pin a map widget with the trigger's pin, not the widget's".into());
    }
    if !counter_or_none(ui.bar) || (ui.bar > 0 && !(1..=COUNTER_LIMIT).contains(&ui.bar_max)) {
        return Err(format!(
            "fill the bar from a counter from 1 to {MAP_COUNTERS}, out of 1 to {COUNTER_LIMIT}"
        ));
    }
    let cells = ui.widget.board.iter().flat_map(|board| &board.cells);
    if ui.channels.len() > ui.widget.choices.len()
        || !ui.channels.iter().copied().all(channel_or_none)
        || !cells
            .clone()
            .all(|cell| channel_or_none(cell.channel) && counter_or_none(cell.counter))
    {
        return Err(format!(
            "each choice signals a channel from 1 to {MAP_CHANNELS}, or 0 for none"
        ));
    }
    if ui.widget.board.is_some() && (!ui.widget.choices.is_empty() || ui.bar > 0 || ui.timer) {
        return Err(
            "a board shows its title and cells: leave out the text, bar and choices".into(),
        );
    }
    Ok(())
}

pub fn validate_trigger(trigger: &MapTrigger) -> Result<(), String> {
    validate_rect(trigger.rect)?;
    let seconds = match trigger.when {
        TriggerWhen::Every => range(trigger.seconds, EVERY_LEAST, 3600.),
        TriggerWhen::Start => range(trigger.seconds, 0., 86400.),
        TriggerWhen::Occupied => range(trigger.seconds, 0.25, 3600.),
        _ => range(trigger.seconds, 0., 3600.),
    };
    if !seconds {
        return Err(match trigger.when {
            TriggerWhen::Every => format!("repeat every {EVERY_LEAST} to 3600 seconds"),
            TriggerWhen::Start => "start within a day of the round".into(),
            TriggerWhen::Occupied => "repeat every 0.25 to 3600 seconds".into(),
            _ => "cooldown must be 0 to 3600 seconds".into(),
        });
    }
    match trigger.when {
        TriggerWhen::Signal if !(1..=MAP_CHANNELS).contains(&trigger.source) => {
            return Err(format!("listen on a channel from 1 to {MAP_CHANNELS}"));
        }
        TriggerWhen::Counter if !(1..=MAP_COUNTERS).contains(&trigger.source) => {
            return Err(format!("watch a counter from 1 to {MAP_COUNTERS}"));
        }
        TriggerWhen::Counter if !(-COUNTER_LIMIT..=COUNTER_LIMIT).contains(&trigger.threshold) => {
            return Err(format!("wait for a value within ±{COUNTER_LIMIT}"));
        }
        TriggerWhen::Occupied if !(1..=TRIGGER_CROWD).contains(&trigger.threshold) => {
            return Err(format!("wait for 1 to {TRIGGER_CROWD} tanks inside"));
        }
        TriggerWhen::Changed if !(1..=MAP_COUNTERS).contains(&trigger.source) => {
            return Err(format!("watch a counter from 1 to {MAP_COUNTERS}"));
        }
        when if when.watches_group() && !(1..=MAP_GROUPS).contains(&trigger.source) => {
            return Err(format!("watch a group from 1 to {MAP_GROUPS}"));
        }
        TriggerWhen::Inputs
            if {
                let mut counters = Vec::new();
                trigger.inputs(&mut counters) == 0 && counters.is_empty()
            } =>
        {
            return Err("add a condition on a counter or group for it to watch".into());
        }
        TriggerWhen::Defeated if usize::from(trigger.source) > BOSS_IDS.len() => {
            return Err(format!(
                "wait for boss 1 to {}, or 0 for any boss",
                BOSS_IDS.len()
            ));
        }
        _ => {}
    }
    match trigger.action {
        action if action.targets_group() && !(1..=MAP_GROUPS).contains(&trigger.group) => {
            return Err(format!("choose a target group from 1 to {MAP_GROUPS}"));
        }
        TriggerAction::Announce if trigger.text.trim().is_empty() => {
            return Err("write the announcement".into());
        }
        TriggerAction::Boss if !BOSS_IDS.contains(&trigger.boss.as_str()) => {
            return Err(format!("unknown boss {:?}", trigger.boss));
        }
        TriggerAction::Signal if !(1..=MAP_CHANNELS).contains(&trigger.group) => {
            return Err(format!("send on a channel from 1 to {MAP_CHANNELS}"));
        }
        TriggerAction::Count
            if !(1..=MAP_COUNTERS).contains(&trigger.group)
                || !(-COUNTER_LIMIT..=COUNTER_LIMIT).contains(&trigger.amount) =>
        {
            return Err(format!(
                "add within ±{COUNTER_LIMIT} to a counter from 1 to {MAP_COUNTERS}"
            ));
        }
        TriggerAction::Set | TriggerAction::Output
            if !(1..=MAP_COUNTERS).contains(&trigger.group)
                || !(-COUNTER_LIMIT..=COUNTER_LIMIT).contains(&trigger.amount) =>
        {
            return Err(format!(
                "set a counter from 1 to {MAP_COUNTERS} within ±{COUNTER_LIMIT}"
            ));
        }
        TriggerAction::Random | TriggerAction::Sequence
            if !(1..=MAP_CHANNELS).contains(&trigger.group)
                || !(2..=i32::from(MAP_CHANNELS)).contains(&trigger.amount)
                || i32::from(trigger.group) + trigger.amount - 1 > i32::from(MAP_CHANNELS) =>
        {
            return Err(format!(
                "pick from 2 or more channels, all within 1 to {MAP_CHANNELS}"
            ));
        }
        TriggerAction::Heal | TriggerAction::Hurt if !(1..=100).contains(&trigger.amount) => {
            return Err("act on 1 to 100 percent of health".into());
        }
        TriggerAction::Award if !(1..=TRIGGER_AWARD).contains(&trigger.amount) => {
            return Err(format!("award 1 to {TRIGGER_AWARD} points"));
        }
        TriggerAction::Shapes if !(1..=TRIGGER_SHAPES).contains(&trigger.amount) => {
            return Err(format!("spawn 1 to {TRIGGER_SHAPES} shapes"));
        }
        TriggerAction::Calculate
            if !counter_or_none(trigger.group)
                || (trigger.group == 0 && trigger.target_offset == 0)
                || !counter_or_none(trigger.operand)
                || !(-COUNTER_LIMIT..=COUNTER_LIMIT).contains(&trigger.amount) =>
        {
            return Err(format!(
                "calculate on counters from 1 to {MAP_COUNTERS} with a number within ±{COUNTER_LIMIT}"
            ));
        }
        TriggerAction::Script if !script_event_name(trigger.text.trim()) => {
            return Err(
                "name the script event with lowercase letters, digits and _ (32 at most)".into(),
            );
        }
        TriggerAction::Move | TriggerAction::Place
            if !trigger
                .travel
                .iter()
                .all(|v| range(*v, -TRAVEL_LIMIT, TRAVEL_LIMIT)) =>
        {
            return Err(format!(
                "move by at most {TRAVEL_LIMIT} arena widths or heights"
            ));
        }
        TriggerAction::Move | TriggerAction::Place
            if trigger.tween > 0.
                && trigger.travel[0].abs().max(trigger.travel[1].abs()) / trigger.tween
                    > MOVE_SPEED_LIMIT =>
        {
            return Err(format!(
                "glide more slowly: at most {MOVE_SPEED_LIMIT} arena widths a second"
            ));
        }
        TriggerAction::Turn if !(-TURN_LIMIT..=TURN_LIMIT).contains(&trigger.amount) => {
            return Err(format!("turn by at most {TURN_LIMIT} degrees"));
        }
        TriggerAction::Fade if !(0..=100).contains(&trigger.amount) => {
            return Err("fade to 0 to 100 percent".into());
        }
        TriggerAction::Tint if !(0..=100).contains(&trigger.amount) => {
            return Err("tint at 0 to 100 percent strength".into());
        }
        TriggerAction::Scale
            if !(0..=(crate::SCALE_LIMIT * 100.) as i32).contains(&trigger.amount) =>
        {
            return Err(format!(
                "scale to 0 to {} percent",
                (crate::SCALE_LIMIT * 100.) as i32
            ));
        }
        TriggerAction::Animate
            if !(0..=trigger.style.limit()).contains(&trigger.amount)
                || !range(trigger.speed, 0., crate::ANIMATE_SPEED) =>
        {
            return Err(format!(
                "{} by 0 to {}{}, at most {} times a second",
                trigger.style.name().to_lowercase(),
                trigger.style.limit(),
                trigger.style.unit(),
                crate::ANIMATE_SPEED
            ));
        }
        TriggerAction::Shake
            if !(1..=100).contains(&trigger.amount)
                || !range(trigger.tween, 0.1, SHAKE_SECONDS) =>
        {
            return Err(format!(
                "shake at 1 to 100 percent for 0.1 to {SHAKE_SECONDS} seconds"
            ));
        }
        TriggerAction::Sound if !(1..=100).contains(&trigger.amount) => {
            return Err("play at 1 to 100 percent volume".into());
        }
        TriggerAction::Shove if !(1..=SHOVE_LIMIT).contains(&trigger.amount) => {
            return Err(format!("shove at 1 to {SHOVE_LIMIT} units a second"));
        }
        TriggerAction::Fling if !(1..=FLING_LIMIT).contains(&trigger.amount) => {
            return Err(format!("fling at 1 to {FLING_LIMIT} units a second"));
        }
        TriggerAction::Blast if trigger.amount == 0 || trigger.amount.abs() > FLING_LIMIT => {
            return Err(format!(
                "blast at 1 to {FLING_LIMIT} units a second, or pull in with a negative amount"
            ));
        }
        // A zoom that holds lasts until the next, whatever its length says.
        TriggerAction::Zoom
            if !(ZOOM_RANGE.0..=ZOOM_RANGE.1).contains(&trigger.amount)
                || (!trigger.hold && !range(trigger.tween, 0.1, ZOOM_SECONDS)) =>
        {
            return Err(format!(
                "zoom to {} to {} percent for 0.1 to {ZOOM_SECONDS} seconds, or until the next zoom",
                ZOOM_RANGE.0, ZOOM_RANGE.1
            ));
        }
        TriggerAction::Light if !(0..=(DARKNESS_LIMIT * 100.) as i32).contains(&trigger.amount) => {
            return Err(format!(
                "darken by 0 to {} percent",
                (DARKNESS_LIMIT * 100.) as i32
            ));
        }
        TriggerAction::Path if !(1..=crate::MAP_PATHS as i32).contains(&trigger.amount) => {
            return Err(format!("play a path from 1 to {}", crate::MAP_PATHS));
        }
        TriggerAction::Reset if trigger.group > MAP_GROUPS => {
            return Err(format!(
                "reset a group from 1 to {MAP_GROUPS}, or 0 for every trigger"
            ));
        }
        TriggerAction::Ui if !(1..=MAP_UI_SLOTS).contains(&trigger.group) => {
            return Err(format!("show UI in a slot from 1 to {MAP_UI_SLOTS}"));
        }
        TriggerAction::HideUi if trigger.group > MAP_UI_SLOTS => {
            return Err(format!(
                "hide UI slot 1 to {MAP_UI_SLOTS}, or 0 for every slot"
            ));
        }
        TriggerAction::Ui if trigger.ui.is_none() => {
            return Err("design the widget to show".into());
        }
        _ => {}
    }
    // Other actions keep a design they no longer use, for switching back.
    if trigger.action == TriggerAction::Ui
        && let Some(ui) = &trigger.ui
    {
        validate_ui(ui)?;
    }
    if trigger.only_if.is_none() && !trigger.also_if.is_empty() {
        return Err("more conditions need a first one".into());
    }
    if trigger.conditions().count() > TRIGGER_CONDITIONS {
        return Err(format!("check at most {TRIGGER_CONDITIONS} conditions"));
    }
    for test in trigger.conditions() {
        let fits = match test.kind {
            TestKind::Counter => {
                (1..=MAP_COUNTERS).contains(&test.index)
                    && counter_or_none(test.against)
                    && (-COUNTER_LIMIT..=COUNTER_LIMIT).contains(&test.value)
            }
            TestKind::Inside | TestKind::Time => {
                counter_or_none(test.against)
                    && (-COUNTER_LIMIT..=COUNTER_LIMIT).contains(&test.value)
            }
            TestKind::Shown | TestKind::Hidden => (1..=MAP_GROUPS).contains(&test.index),
        };
        if !fits {
            return Err(format!(
                "a condition looks at a counter from 1 to {MAP_COUNTERS} (within ±{COUNTER_LIMIT}) or a group from 1 to {MAP_GROUPS}"
            ));
        }
    }
    if !channel_or_none(trigger.otherwise) {
        return Err(format!(
            "send the otherwise signal on a channel from 1 to {MAP_CHANNELS}, or 0 for none"
        ));
    }
    if !counter_or_none(trigger.target_offset) || !counter_or_none(trigger.operand_offset) {
        return Err(format!(
            "offset by a counter from 1 to {MAP_COUNTERS}, or 0 for none"
        ));
    }
    if trigger.focus > 100 || !range(trigger.ease, 0., ZOOM_EASE) {
        return Err(format!(
            "turn the view 0 to 100 percent toward the area, over 0 to {ZOOM_EASE} seconds"
        ));
    }
    if !range(trigger.offset, 0., 3600.)
        || !range(trigger.duration, 0., 3600.)
        || !range(trigger.delay, 0., 3600.)
    {
        return Err("offset, delay and restore time must be 0 to 3600 seconds".into());
    }
    if !(1..=100).contains(&trigger.chance) {
        return Err("chance must be 1 to 100 percent".into());
    }
    if !range(trigger.tween, 0., crate::TWEEN_SECONDS) {
        return Err(format!(
            "a motion takes 0 to {} seconds",
            crate::TWEEN_SECONDS
        ));
    }
    if !trigger.angle.is_finite() {
        return Err("direction must be a number".into());
    }
    if trigger.member > MAP_GROUPS || trigger.team > 4 {
        return Err(format!(
            "its own group must be 0 to {MAP_GROUPS} and its team 0 to 4"
        ));
    }
    if !display_text(&trigger.text, 96) {
        return Err("text is limited to 96 characters on one line".into());
    }
    Ok(())
}

fn display_text(text: &str, limit: usize) -> bool {
    text.chars().count() <= limit && !text.chars().any(char::is_control)
}

fn range(value: f32, min: f32, max: f32) -> bool {
    value.is_finite() && value >= min && value <= max
}

fn validate_point(position: [f32; 2]) -> Result<(), &'static str> {
    if position.into_iter().all(|v| range(v, 0., 1.)) {
        Ok(())
    } else {
        Err("positions must be finite normalized coordinates")
    }
}

fn validate_rect([x, y, w, h]: [f32; 4]) -> Result<(), &'static str> {
    if range(x, 0., 1.)
        && range(y, 0., 1.)
        && range(w, 0.001, 1.)
        && range(h, 0.001, 1.)
        && x + w <= 1.00001
        && y + h <= 1.00001
    {
        Ok(())
    } else {
        Err("rectangles must be finite, positive and contained in the unit arena")
    }
}

/// Whether a portal exit at normalized `exit` has room for a tank.
pub fn portal_exit_clear(map: &MapDef, exit: [f32; 2]) -> bool {
    validate_point(exit).is_ok() && point_clear(exit[0] * map.width, exit[1] * map.height, map, 40.)
}

/// Groups whose blocks don't stay put: bodies tanks push, and groups a
/// trigger hides, toggles or moves. A door like that may close a room at
/// the start without cutting the arena in two.
fn movable_groups(map: &MapDef) -> u32 {
    let bodies = map
        .bodies
        .iter()
        .fold(0, |mask, body| mask | MapDef::group_bit(body.group));
    map.triggers
        .iter()
        .filter(|t| {
            matches!(
                t.action,
                TriggerAction::Hide
                    | TriggerAction::Toggle
                    | TriggerAction::Move
                    | TriggerAction::Place
                    | TriggerAction::Path
                    | TriggerAction::Shove
            )
        })
        .fold(bodies, |mask, t| mask | MapDef::group_bit(t.group))
}

fn point_clear(x: f32, y: f32, map: &MapDef, radius: f32) -> bool {
    clear_of(x, y, map, radius, map.starting_mask())
}

/// Whether a circle at `x`, `y` misses the walls and the blocks of groups
/// not in `hidden`.
fn clear_of(x: f32, y: f32, map: &MapDef, radius: f32, hidden: u32) -> bool {
    if x < radius || y < radius || x > map.width - radius || y > map.height - radius {
        return false;
    }
    map.solid_rects(hidden).all(|[wx, wy, w, h]| {
        let (wx, wy, w, h) = (
            wx * map.width,
            wy * map.height,
            w * map.width,
            h * map.height,
        );
        let nearest_x = x.clamp(wx, wx + w);
        let nearest_y = y.clamp(wy, wy + h);
        (x - nearest_x).powi(2) + (y - nearest_y).powi(2) >= radius * radius
    })
}

fn segment_clear(a: [f32; 2], b: [f32; 2], map: &MapDef, hidden: u32) -> bool {
    map.solid_rects(hidden).all(|[x, y, w, h]| {
        let mins = [x * map.width - 30., y * map.height - 30.];
        let maxs = [(x + w) * map.width + 30., (y + h) * map.height + 30.];
        let mut entry: f32 = 0.;
        let mut exit: f32 = 1.;
        for axis in 0..2 {
            let delta = b[axis] - a[axis];
            if delta.abs() < 0.001 {
                if a[axis] < mins[axis] || a[axis] > maxs[axis] {
                    return true;
                }
            } else {
                let first = (mins[axis] - a[axis]) / delta;
                let second = (maxs[axis] - a[axis]) / delta;
                entry = entry.max(first.min(second));
                exit = exit.min(first.max(second));
                if entry > exit {
                    return true;
                }
            }
        }
        false
    })
}

fn validate_connectivity(map: &MapDef) -> Result<(), String> {
    // Only cover that stays put all round can cut the arena in two.
    let hidden = map.starting_mask() | movable_groups(map);
    let cols = (map.width / 80.).ceil() as usize;
    let rows = (map.height / 80.).ceil() as usize;
    let center = |index: usize| {
        [
            (index % cols) as f32 * 80. + 40.,
            (index / cols) as f32 * 80. + 40.,
        ]
    };
    let free = (0..cols * rows)
        .map(|index| {
            let [x, y] = center(index);
            // Keep samples strictly outside the link test's expanded walls.
            // Exact tangency must not create an isolated line of "free" cells.
            clear_of(x, y, map, 37., hidden)
        })
        .collect::<Vec<_>>();
    let Some(start) = free.iter().position(|&valid| valid) else {
        return Err(format!("{}: no navigable arena cells", map.id));
    };
    let mut visited = vec![false; free.len()];
    let mut queue = VecDeque::from([start]);
    visited[start] = true;
    while let Some(index) = queue.pop_front() {
        let (x, y) = (index % cols, index / cols);
        for (nx, ny) in [
            (x.wrapping_sub(1), y),
            (x + 1, y),
            (x, y.wrapping_sub(1)),
            (x, y + 1),
        ] {
            if nx >= cols || ny >= rows {
                continue;
            }
            let next = ny * cols + nx;
            if !visited[next]
                && free[next]
                && segment_clear(center(index), center(next), map, hidden)
            {
                visited[next] = true;
                queue.push_back(next);
            }
        }
    }
    if free
        .iter()
        .zip(&visited)
        .any(|(&free, &seen)| free && !seen)
    {
        return Err(format!(
            "{}: static arena has disconnected navigable regions",
            map.id
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counter_validation_rejects_extreme_integers_without_overflowing() {
        for value in [i32::MIN, -COUNTER_LIMIT - 1, COUNTER_LIMIT + 1, i32::MAX] {
            let mut trigger = MapTrigger {
                when: TriggerWhen::Counter,
                threshold: value,
                ..Default::default()
            };
            assert!(validate_trigger(&trigger).is_err());
            trigger.when = TriggerWhen::Start;
            trigger.amount = value;
            for action in [
                TriggerAction::Count,
                TriggerAction::Set,
                TriggerAction::Calculate,
            ] {
                trigger.action = action;
                assert!(validate_trigger(&trigger).is_err());
            }
            trigger.action = TriggerAction::Toggle;
            trigger.only_if = Some(TriggerTest {
                index: 1,
                value,
                ..Default::default()
            });
            assert!(validate_trigger(&trigger).is_err());
        }
        for value in [-COUNTER_LIMIT, COUNTER_LIMIT] {
            let trigger = MapTrigger {
                when: TriggerWhen::Counter,
                action: TriggerAction::Set,
                threshold: value,
                amount: value,
                only_if: Some(TriggerTest {
                    index: 1,
                    value,
                    ..Default::default()
                }),
                ..Default::default()
            };
            validate_trigger(&trigger).unwrap();
        }
        let random = MapTrigger {
            action: TriggerAction::Random,
            group: 2,
            amount: i32::MAX,
            ..Default::default()
        };
        assert!(validate_trigger(&random).is_err());
    }

    #[test]
    fn compact_catalogs_cannot_request_unbounded_geometry_validation_work() {
        let mut map = builtin_maps().remove(0);
        map.width = 16000.;
        map.height = 16000.;
        map.walls = vec![[0.45, 0.45, 0.001, 0.001]; 2048];
        assert!(
            validate_maps(&[map.clone()])
                .unwrap_err()
                .contains("budget")
        );
        map.walls.truncate(64);
        let maps: Vec<_> = (0..8)
            .map(|index| {
                let mut copy = map.clone();
                copy.id = format!("arena_{index}");
                copy
            })
            .collect();
        assert!(validate_maps(&maps).unwrap_err().contains("budget"));
    }

    #[test]
    fn every_builtin_arena_has_valid_clear_connected_geometry() {
        let maps = builtin_maps();
        assert_eq!(maps.len(), 9);
        validate_maps(&maps).unwrap();
        assert!(maps.iter().any(|map| map.procedural == MapProcedure::Maze));
        assert!(maps.iter().any(|map| map.procedural == MapProcedure::Dense));
        assert_eq!(
            maps.iter()
                .find(|map| map.id == "domination")
                .unwrap()
                .objective_positions
                .len(),
            3
        );
    }

    #[test]
    fn malformed_references_geometry_and_sealed_maps_fail() {
        let mut maps = builtin_maps();
        maps[0].recommended_modes = vec!["imaginary".into()];
        assert!(validate_maps(&maps).is_err());
        let mut maps = builtin_maps();
        maps[0].width = f32::INFINITY;
        assert!(validate_maps(&maps).is_err());
        let mut maps = builtin_maps();
        maps[0].walls = vec![[0.49, 0., 0.02, 1.]];
        maps[0].spawn_regions[0].rect = [0.02, 0.02, 0.2, 0.2];
        assert!(validate_maps(&maps).unwrap_err().contains("disconnected"));
        // A door that can open doesn't seal the arena: a body tanks push,
        // or a group a trigger hides.
        let sealed = || {
            let mut maps = builtin_maps();
            maps[0].blocks.push(MapBlock {
                rect: [0.49, 0., 0.02, 1.],
                style: Default::default(),
                color: None,
                group: 2,
            });
            maps[0].spawn_regions[0].rect = [0.02, 0.02, 0.2, 0.2];
            maps
        };
        assert!(
            validate_maps(&sealed())
                .unwrap_err()
                .contains("disconnected")
        );
        let mut maps = sealed();
        maps[0].bodies.push(crate::MapBody {
            group: 2,
            ..Default::default()
        });
        assert!(validate_maps(&maps).is_ok());
        let mut maps = sealed();
        maps[0].triggers.push(MapTrigger {
            when: TriggerWhen::Signal,
            action: TriggerAction::Hide,
            group: 2,
            ..Default::default()
        });
        assert!(validate_maps(&maps).is_ok());
        let mut maps = builtin_maps();
        maps.push(maps[0].clone());
        assert!(validate_maps(&maps).unwrap_err().contains("duplicate"));
    }

    #[test]
    fn map_catalog_roundtrips_through_the_public_loader() {
        let catalog = MapCatalog {
            maps: builtin_maps(),
        };
        let json = serde_json::to_string(&catalog).unwrap();
        let maps = maps_from_json(&json).unwrap();
        assert_eq!(maps.len(), 9);
    }

    #[test]
    fn newer_logic_round_trips_and_older_maps_keep_their_shape() {
        let test = |kind, index| TriggerTest {
            kind,
            index,
            ..Default::default()
        };
        let mut t = MapTrigger {
            when: TriggerWhen::Signal,
            action: TriggerAction::Signal,
            ..Default::default()
        };
        let plain = serde_json::to_value(&t).unwrap();
        for key in ["also_if", "gate", "otherwise"] {
            assert!(plain.get(key).is_none(), "{key} is left out when unused");
        }
        t.set_conditions(vec![test(TestKind::Counter, 1), test(TestKind::Shown, 3)]);
        t.gate = GateMode::One;
        t.otherwise = 4;
        validate_trigger(&t).unwrap();
        let json = serde_json::to_string(&t).unwrap();
        let back: MapTrigger = serde_json::from_str(&json).unwrap();
        assert_eq!(back, t);
        assert!(json.contains("\"gate\":\"one\""), "{json}");
        t.set_conditions(Vec::new());
        assert_eq!(
            (t.gate, t.otherwise),
            Default::default(),
            "no gate, no settings"
        );
    }

    #[test]
    fn gates_reject_impossible_conditions() {
        let base = MapTrigger::default();
        let mut too_many = base.clone();
        too_many.only_if = Some(TriggerTest::default());
        too_many.also_if = vec![TriggerTest::default(); TRIGGER_CONDITIONS];
        assert!(validate_trigger(&too_many).is_err());
        let mut orphan = base.clone();
        orphan.also_if = vec![TriggerTest::default()];
        assert!(
            validate_trigger(&orphan).is_err(),
            "more conditions need a first"
        );
        let mut channel = base.clone();
        channel.otherwise = MAP_CHANNELS;
        validate_trigger(&channel).unwrap();
        let mut sequence = MapTrigger {
            action: TriggerAction::Sequence,
            group: MAP_CHANNELS - 1,
            amount: 3,
            ..base.clone()
        };
        assert!(
            validate_trigger(&sequence).is_err(),
            "runs past the last channel"
        );
        sequence.group = MAP_CHANNELS - 2;
        validate_trigger(&sequence).unwrap();
        let boss = MapTrigger {
            when: TriggerWhen::Defeated,
            source: BOSS_IDS.len() as u8 + 1,
            ..base
        };
        assert!(validate_trigger(&boss).is_err());
    }

    #[test]
    fn time_and_crowd_conditions_need_no_index() {
        // As the modding guide writes them.
        for source in [
            r#"{ "kind": "time", "compare": "at_least", "value": 300 }"#,
            r#"{ "kind": "inside", "compare": "at_least", "value": 2 }"#,
        ] {
            let trigger = MapTrigger {
                only_if: Some(serde_json::from_str(source).expect(source)),
                ..Default::default()
            };
            validate_trigger(&trigger).expect(source);
        }
        // Counters and groups still have to say which.
        let trigger = MapTrigger {
            only_if: Some(serde_json::from_str(r#"{ "kind": "counter", "value": 3 }"#).unwrap()),
            ..Default::default()
        };
        assert!(validate_trigger(&trigger).is_err());
    }

    #[test]
    fn gate_modes_count_conditions_like_logic_gates() {
        let table = [
            (GateMode::All, [false, false, false, true]),
            (GateMode::Any, [false, true, true, true]),
            (GateMode::NotAll, [true, true, true, false]),
            (GateMode::NoneOf, [true, false, false, false]),
            (GateMode::One, [false, true, true, false]),
            (GateMode::Even, [true, false, false, true]),
            (GateMode::Odd, [false, true, true, false]),
            (GateMode::Most, [false, false, false, true]),
        ];
        for (mode, outputs) in table {
            for (inputs, expected) in [(0, 2), (1, 2), (1, 2), (2, 2)].iter().zip(outputs) {
                assert_eq!(
                    mode.open(inputs.0, inputs.1),
                    expected,
                    "{mode:?} {inputs:?}"
                );
            }
        }
        // Over three: parity and majority make a full adder's sum and carry.
        for passed in 0..=3 {
            assert_eq!(GateMode::Odd.open(passed, 3), passed % 2 == 1);
            assert_eq!(GateMode::Most.open(passed, 3), passed >= 2);
        }
        let not: Vec<GateMode> = GateMode::ALL.into_iter().filter(|g| g.inverts()).collect();
        assert_eq!(not, [GateMode::NotAll, GateMode::NoneOf, GateMode::Even]);
    }

    #[test]
    fn input_watchers_need_a_counter_or_group_to_watch() {
        let mut t = MapTrigger {
            when: TriggerWhen::Inputs,
            action: TriggerAction::Output,
            group: 3,
            amount: 1,
            ..Default::default()
        };
        assert!(validate_trigger(&t).is_err(), "nothing to watch");
        t.set_conditions(vec![TriggerTest {
            kind: TestKind::Time,
            value: 5,
            ..Default::default()
        }]);
        assert!(validate_trigger(&t).is_err(), "time changes unseen");
        t.set_conditions(vec![TriggerTest {
            kind: TestKind::Time,
            against: 7,
            ..Default::default()
        }]);
        validate_trigger(&t).unwrap();
        let mut counters = Vec::new();
        assert_eq!(t.inputs(&mut counters), 0);
        assert_eq!(counters, [7]);
        t.set_conditions(vec![
            TriggerTest {
                index: 4,
                against: 2,
                ..Default::default()
            },
            TriggerTest {
                kind: TestKind::Hidden,
                index: 3,
                ..Default::default()
            },
            TriggerTest {
                index: 2,
                ..Default::default()
            },
        ]);
        assert_eq!(t.inputs(&mut counters), 0b100);
        assert_eq!(counters, [2, 4], "each counter once, in order");
        t.group = 0;
        assert!(validate_trigger(&t).is_err(), "the result needs a counter");
    }

    #[test]
    fn senders_name_the_channels_they_reach() {
        assert_eq!(TriggerAction::Signal.channels(4, 9), 4..=4);
        assert_eq!(TriggerAction::Sequence.channels(4, 3), 4..=6);
        assert!(TriggerAction::Count.channels(4, 3).is_empty());
        for action in [TriggerAction::Random, TriggerAction::Sequence] {
            assert_eq!(action.channels(4, i32::MAX), 4..=MAP_CHANNELS);
            assert_eq!(action.channels(4, i32::MIN), 4..=4);
            assert_eq!(
                action.channels(MAP_CHANNELS, i32::MAX),
                MAP_CHANNELS..=MAP_CHANNELS
            );
        }
    }

    #[test]
    fn map_widgets_write_newer_fields_only_when_used() {
        let mut ui = MapUi::starter();
        let plain = serde_json::to_string(&ui).expect("writes");
        for newer in ["\"pin\"", "\"point\"", "\"layout\"", "\"glow\""] {
            assert!(!plain.contains(newer), "{newer} in {plain}");
        }
        assert_eq!(serde_json::from_str::<MapUi>(&plain).expect("reads"), ui);
        ui.pin = UiPin::Cause;
        ui.widget.point = true;
        ui.widget.layout = crate::HudLayout::Grid;
        ui.widget.glow = true;
        let styled = serde_json::to_string(&ui).expect("writes");
        assert_eq!(serde_json::from_str::<MapUi>(&styled).expect("reads"), ui);
        assert!(validate_ui(&ui).is_ok());
        ui.widget.pin = crate::HudPin::Tank(3);
        assert!(validate_ui(&ui).is_err(), "maps pin with the trigger");
    }

    #[test]
    fn counter_fields_pad_group_join_and_change_base() {
        let mut counters = [0i32; MAP_COUNTERS as usize];
        counters[0] = 1234567;
        counters[1] = -42;
        counters[2] = 5;
        counters[3] = 12;
        counters[4] = 345;
        counters[254] = 9;
        let field = |f: &str| counter_field(f, &counters);
        assert_eq!(field("1").as_deref(), Some("1234567"));
        assert_eq!(field("1:,").as_deref(), Some("1,234,567"));
        assert_eq!(field("2:,").as_deref(), Some("-42"));
        assert_eq!(field("2:05").as_deref(), Some("-00042"));
        assert_eq!(field("3:b").as_deref(), Some("101"));
        assert_eq!(field("3:b8").as_deref(), Some("00000101"));
        assert_eq!(field("4:x2").as_deref(), Some("0C"));
        assert_eq!(field("255").as_deref(), Some("9"));
        // Parts of one long number, most significant first.
        assert_eq!(field("3|4|5:4").as_deref(), Some("500120345"));
        assert_eq!(field("3|4|5:4,").as_deref(), Some("500,120,345"));
        assert_eq!(field("6|5:4").as_deref(), Some("345"), "leading zero parts");
        assert_eq!(
            field("6|2|3:4").as_deref(),
            Some("-420005"),
            "sign from the first"
        );
        for wrong in [
            "", "0", "01", "256", "1:", "1:y", "1:0", "1:021", "1:b33", "1:x9", "3|4", "3:4|5",
            "1|2:0", "x",
        ] {
            assert_eq!(field(wrong), None, "{wrong}");
        }
        assert_eq!(
            fill_counters("{3}/{1:,} {name} {{4} {9", &counters),
            "5/1,234,567 {name} {12 {9"
        );
        let mut named = Vec::new();
        counters_named("{3}/{1:,} {name} {{4} {3|5:4} {300} {9", &mut named);
        named.sort_unstable();
        assert_eq!(named, [1, 3, 4, 5]);
    }

    #[test]
    fn new_math_works_on_bits_and_powers_and_stays_in_range() {
        let apply = |op: MathOp, a: i32, b: i32| op.apply(a, b, 0.);
        assert_eq!(apply(MathOp::And, 0b1100, 0b1010), Some(0b1000));
        assert_eq!(apply(MathOp::Or, 0b1100, 0b1010), Some(0b1110));
        assert_eq!(apply(MathOp::Xor, 0b1100, 0b1010), Some(0b0110));
        assert_eq!(apply(MathOp::ShiftLeft, 3, 4), Some(48));
        assert_eq!(apply(MathOp::ShiftRight, 48, 4), Some(3));
        assert_eq!(apply(MathOp::ShiftRight, -1, 4), Some(-1), "rounds down");
        assert_eq!(apply(MathOp::ShiftLeft, 1, 200), Some(COUNTER_LIMIT));
        assert_eq!(apply(MathOp::Power, 10, 4), Some(10_000));
        assert_eq!(apply(MathOp::Power, -2, 3), Some(-8));
        assert_eq!(apply(MathOp::Power, 7, 0), Some(1));
        assert_eq!(apply(MathOp::Power, 10, 40), Some(COUNTER_LIMIT));
        assert_eq!(apply(MathOp::Power, 2, -1), None, "no fractions");
        assert_eq!(MathOp::ALL.len(), 16);
        for op in MathOp::ALL {
            for (a, b) in [
                (COUNTER_LIMIT, COUNTER_LIMIT),
                (-COUNTER_LIMIT, -COUNTER_LIMIT),
                (7, 0),
            ] {
                if let Some(value) = op.apply(a, b, 0.5) {
                    assert!(value.abs() <= COUNTER_LIMIT, "{op:?} {a} {b}");
                }
            }
        }
    }
}
