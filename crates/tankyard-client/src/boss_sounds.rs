//! Layered recorded metal, stone, steam and creature performances. Each cue
//! has its own onset, weight and room tail; no cue is a bare tone or noise bed.
//! Original recordings and CC0 credits live in `assets/audio/foley`.
use crate::map_sounds::BossCue;
use crate::recorded_sounds::{Foley::*, layer, reverse};
use crate::sound_kit::{Flanger, Room, finish, in_room, limit, low_shelf, samples_in};

pub fn make(cue: BossCue) -> Vec<f32> {
    use BossCue::*;
    let (seconds, room, tail) = match cue {
        Charge => (1.05, Room::SMALL, 0.35),
        Lock => (0.4, Room::SMALL, 0.2),
        Shot | ClashIon | ClashFire => (1.7, Room::ARENA, 0.85),
        Beam => (1.35, Room::HALL, 0.45),
        Vent => (1.55, Room::SMALL, 0.3),
        Fall | WormDeath => (3.2, Room::CAVERN, 1.1),
        ClashBeam | ClashSteel | Backlash => (1.35, Room::HALL, 0.65),
        WormRumble | WormRoar | WormStrain => (2.3, Room::CAVERN, 0.8),
        WormBurst | WormTear => (2.1, Room::ARENA, 0.7),
        WormHiss | WormGurgle | WormDive | WormBrood | WormThrash => (1.5, Room::SMALL, 0.5),
        WormSnap | WormSpit | WormSpray | WormSting => (0.9, Room::SMALL, 0.3),
    };
    let mut out = vec![0.; samples_in(seconds)];
    match cue {
        Charge => {
            reverse(&mut out, MetalSpring, 0., 0.6, 0.7);
            reverse(&mut out, MetalSheet, 0.25, 0.75, 0.4);
            layer(&mut out, Steam, 0., 1.3, 0.16);
            layer(&mut out, Click, 0.78, 1.35, 0.22);
        }
        Lock => {
            layer(&mut out, MetalHit, 0., 1.9, 0.65);
            layer(&mut out, MetalHit, 0.12, 1.55, 0.55);
            layer(&mut out, Click, 0., 1., 0.3);
        }
        Shot => {
            layer(&mut out, MetalSlam, 0., 0.6, 1.);
            layer(&mut out, StoneBreak, 0., 1.4, 0.55);
            layer(&mut out, MetalSpring, 0.035, 0.8, 0.55);
            layer(&mut out, MetalFall, 0.12, 1.1, 0.3);
        }
        Beam => {
            layer(&mut out, Steam, 0., 1.35, 0.7);
            for i in 0..5 {
                layer(
                    &mut out,
                    MetalSpring,
                    i as f32 * 0.23,
                    1.1 + i as f32 * 0.07,
                    0.42,
                );
                layer(&mut out, MetalSheet, i as f32 * 0.23 + 0.07, 1.5, 0.18);
            }
            let mut flange = Flanger::new();
            for (i, sample) in out.iter_mut().enumerate() {
                let t = i as f32 / crate::sound_kit::RATE;
                *sample = flange.run(*sample, 0.001 + t * 0.002, 0.38);
            }
        }
        Vent => {
            layer(&mut out, Click, 0., 0.9, 0.6);
            layer(&mut out, Steam, 0.04, 1.2, 0.95);
            layer(&mut out, MetalFall, 0.12, 1.6, 0.17);
        }
        Fall => {
            for (at, rate, gain) in [(0., 0.6, 0.95), (0.22, 0.85, 0.8), (0.52, 0.7, 0.55)] {
                layer(&mut out, StoneBreak, at, rate, gain);
            }
            layer(&mut out, MetalSlam, 0., 0.45, 0.9);
            layer(&mut out, MetalFall, 0.22, 0.75, 0.8);
            layer(&mut out, StoneFall, 0.8, 0.65, 0.45);
        }
        ClashIon => {
            // The rail parry gets the widest, heaviest clash, with the coil
            // ringing after the steel and stone have struck.
            layer(&mut out, MetalSlam, 0., 0.52, 1.1);
            layer(&mut out, MetalHit, 0., 1.45, 0.9);
            layer(&mut out, StoneBreak, 0.008, 0.8, 0.8);
            layer(&mut out, MetalSpring, 0.025, 0.78, 0.8);
            layer(&mut out, MetalSheet, 0.1, 0.65, 0.45);
            layer(&mut out, MetalFall, 0.3, 0.85, 0.3);
        }
        ClashBeam => {
            layer(&mut out, MetalHit, 0., 1.6, 0.7);
            layer(&mut out, Steam, 0.015, 1.6, 0.7);
            layer(&mut out, MetalSpring, 0., 1.25, 0.95);
            reverse(&mut out, MetalSheet, 0.03, 1.4, 0.4);
        }
        ClashFire => {
            layer(&mut out, StoneBreak, 0., 0.75, 1.);
            layer(&mut out, MetalSlam, 0., 0.75, 0.85);
            layer(&mut out, Steam, 0.02, 1.55, 0.4);
            layer(&mut out, Crunch, 0.18, 0.7, 0.5);
            layer(&mut out, StoneFall, 0.3, 0.75, 0.55);
        }
        ClashSteel => {
            layer(&mut out, MetalHit, 0., 0.85, 1.);
            layer(&mut out, MetalHit, 0.009, 1.7, 0.6);
            layer(&mut out, MetalSheet, 0.025, 0.85, 0.7);
            layer(&mut out, MetalFall, 0.15, 1.1, 0.3);
        }
        Backlash => {
            layer(&mut out, MetalSlam, 0., 0.8, 1.);
            layer(&mut out, MetalSheet, 0.02, 0.65, 0.6);
            layer(&mut out, StoneBreak, 0.01, 1.4, 0.5);
        }
        WormRumble => {
            layer(&mut out, StoneBreak, 0., 0.38, 0.6);
            layer(&mut out, StoneFall, 0.3, 0.4, 0.7);
            layer(&mut out, StoneBreak, 0.95, 0.55, 0.45);
            layer(&mut out, Breath, 0.3, 0.5, 0.24);
        }
        WormBurst => {
            layer(&mut out, StoneBreak, 0., 0.55, 1.);
            layer(&mut out, StoneBreak, 0.08, 1.1, 0.65);
            layer(&mut out, StoneFall, 0.18, 0.6, 0.85);
            layer(&mut out, Splash, 0.03, 0.8, 0.4);
            layer(&mut out, Growl, 0.12, 0.55, 0.6);
        }
        WormRoar => {
            layer(&mut out, Growl, 0., 0.55, 0.85);
            layer(&mut out, Roar, 0.14, 0.55, 1.);
            layer(&mut out, Scream, 0.26, 0.85, 0.25);
            layer(&mut out, Breath, 1.25, 0.8, 0.3);
        }
        WormHiss => {
            layer(&mut out, Breath, 0., 0.65, 0.95);
            layer(&mut out, Steam, 0.05, 1.7, 0.2);
            layer(&mut out, Growl, 0.2, 1.3, 0.25);
        }
        WormSnap => {
            layer(&mut out, Crunch, 0., 0.7, 0.95);
            layer(&mut out, Wet, 0.008, 0.9, 0.6);
            layer(&mut out, Spit, 0.02, 1.4, 0.5);
        }
        WormSpit => {
            layer(&mut out, Spit, 0., 0.8, 1.);
            layer(&mut out, Wet, 0.05, 1.4, 0.6);
            layer(&mut out, Crunch, 0.02, 1.45, 0.35);
        }
        WormGurgle => {
            layer(&mut out, Bubble, 0., 0.6, 0.8);
            layer(&mut out, Growl, 0.08, 0.85, 0.6);
            layer(&mut out, Wet, 0.45, 0.6, 0.5);
        }
        WormSpray => {
            layer(&mut out, Splash, 0., 1., 0.8);
            layer(&mut out, Wet, 0.04, 0.9, 0.85);
            layer(&mut out, Spit, 0.12, 1.4, 0.5);
        }
        WormDive => {
            layer(&mut out, StoneFall, 0., 0.65, 0.9);
            layer(&mut out, Crunch, 0.12, 0.5, 0.55);
            layer(&mut out, StoneBreak, 0.38, 0.9, 0.5);
        }
        WormBrood => {
            for i in 0..4 {
                layer(&mut out, Wet, i as f32 * 0.19, 0.7 + i as f32 * 0.15, 0.55);
                layer(&mut out, Spit, i as f32 * 0.19 + 0.06, 1.2, 0.4);
            }
            layer(&mut out, Bubble, 0., 0.7, 0.4);
        }
        WormThrash => {
            for i in 0..5 {
                layer(
                    &mut out,
                    Crunch,
                    i as f32 * 0.17,
                    0.7 + i as f32 * 0.14,
                    0.7,
                );
            }
            layer(&mut out, StoneFall, 0.2, 0.7, 0.7);
        }
        WormSting => {
            reverse(&mut out, Crunch, 0., 1.6, 0.45);
            layer(&mut out, Crunch, 0.1, 1.6, 0.8);
            layer(&mut out, Spit, 0.09, 1.2, 0.65);
        }
        WormStrain => {
            layer(&mut out, Growl, 0., 0.5, 1.);
            layer(&mut out, Crunch, 0.6, 0.5, 0.5);
            layer(&mut out, Crunch, 1.05, 0.6, 0.6);
            layer(&mut out, Wet, 1.3, 0.65, 0.5);
        }
        WormTear => {
            layer(&mut out, Crunch, 0., 0.5, 0.9);
            layer(&mut out, Wet, 0., 0.5, 0.9);
            layer(&mut out, Splash, 0.07, 0.6, 0.6);
            layer(&mut out, Scream, 0.12, 0.65, 0.8);
            layer(&mut out, Crunch, 0.25, 0.7, 0.7);
        }
        WormDeath => {
            layer(&mut out, Roar, 0., 0.45, 1.);
            layer(&mut out, Scream, 0.15, 0.55, 0.5);
            layer(&mut out, Growl, 1.1, 0.5, 0.65);
            layer(&mut out, StoneBreak, 1.2, 0.6, 0.65);
            layer(&mut out, StoneFall, 1.6, 0.5, 0.65);
        }
    }
    master(out, room, tail)
}

/// An industrial warning: a steel impact, a reversed sheet rising into the
/// next strike, and the mechanism's real spring and steam release.
pub fn arrival() -> Vec<f32> {
    let mut out = vec![0.; samples_in(2.5)];
    for (at, rate) in [(0., 0.65), (0.74, 0.8), (1.5, 1.)] {
        layer(&mut out, MetalSlam, at, rate, 0.8);
        layer(&mut out, MetalSpring, at + 0.025, rate, 0.45);
    }
    reverse(&mut out, MetalSheet, 0.25, 0.65, 0.5);
    layer(&mut out, Steam, 0.7, 1., 0.24);
    master(out, Room::ARENA, 0.8)
}

fn master(mut out: Vec<f32>, room: Room, tail: f32) -> Vec<f32> {
    low_shelf(&mut out, 180., 0.72);
    let mut out = in_room(&out, room, tail);
    limit(&mut out, 4.);
    finish(out, 0.1)
}
