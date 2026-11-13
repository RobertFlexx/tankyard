//! NVIDIA DLSS Super Resolution and DLAA on GeForce RTX GPUs.
//!
//! DLSS runs only on Vulkan or Direct3D, and the game renders with OpenGL.
//! On NVIDIA drivers the two share memory: the arena is drawn by OpenGL into
//! images that a small Vulkan device owns, DLSS reads them there and writes
//! its result into another shared image, and semaphores order the two APIs
//! on the GPU with no copies or CPU waits.
//!
//! Inputs each frame are the jittered scene, per-pixel motion and a flat
//! depth (the arena is 2D). Everything is optional at runtime: without an
//! RTX GPU, a recent driver, the SDK at build time or the DLSS runtime, the
//! game reports why and keeps using FSR 1.
use macroquad::prelude::*;

/// Why DLSS can or cannot run here, for the Graphics settings.
#[derive(Clone, Debug, PartialEq)]
pub enum Status {
    Ready,
    /// Everything but the DLSS runtime, which the player can download.
    NeedsRuntime,
    Unavailable(String),
}

/// DLSS quality modes, matching NGX's values.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    Performance = 0,
    Balanced = 1,
    Quality = 2,
    UltraPerformance = 3,
    Dlaa = 5,
}

impl Mode {
    /// The mode whose usual render scale is closest to `scale`. DLSS also
    /// accepts any scale within each mode's range, which `render_size` keeps.
    pub fn for_scale(scale: f32) -> Self {
        match scale {
            s if s >= 0.99 => Self::Dlaa,
            s if s >= 0.62 => Self::Quality,
            s if s >= 0.545 => Self::Balanced,
            s if s >= 0.42 => Self::Performance,
            _ => Self::UltraPerformance,
        }
    }
    pub fn name(self) -> &'static str {
        match self {
            Self::Performance => "Performance",
            Self::Balanced => "Balanced",
            Self::Quality => "Quality",
            Self::UltraPerformance => "Ultra Performance",
            Self::Dlaa => "DLAA",
        }
    }
}

/// Halton(2, 3) sample `index`, centered on the pixel: the sub-pixel offset
/// DLSS expects the scene to be rendered at.
// Only native DLSS builds jitter; others keep these for the tests.
#[cfg_attr(not(tankyard_dlss), allow(dead_code))]
pub fn jitter(index: u32, phases: u32) -> Vec2 {
    fn halton(mut index: u32, base: u32) -> f32 {
        let (mut result, mut fraction) = (0., 1.);
        while index > 0 {
            fraction /= base as f32;
            result += fraction * (index % base) as f32;
            index /= base;
        }
        result
    }
    let i = index % phases.max(1) + 1;
    vec2(halton(i, 2) - 0.5, halton(i, 3) - 0.5)
}

/// NVIDIA's guidance: 8 phases per output pixel covered by a render pixel.
#[cfg_attr(not(tankyard_dlss), allow(dead_code))]
pub fn phases(render: (u32, u32), output: (u32, u32)) -> u32 {
    let ratio = output.0 as f32 / render.0.max(1) as f32;
    ((8. * ratio * ratio).round() as u32).clamp(8, 72)
}

#[cfg(all(tankyard_dlss, any(target_os = "linux", target_os = "windows")))]
mod native;
#[cfg(all(tankyard_dlss, any(target_os = "linux", target_os = "windows")))]
pub use native::*;

#[cfg(not(all(tankyard_dlss, any(target_os = "linux", target_os = "windows"))))]
mod native {
    use super::*;
    pub fn status() -> Status {
        Status::Unavailable(
            if cfg!(target_vendor = "apple") {
                "DLSS needs an NVIDIA RTX GPU; Macs use MetalFX."
            } else if cfg!(target_os = "freebsd") {
                "NVIDIA ships DLSS only for Linux and Windows, so FreeBSD uses FSR 1 instead."
            } else {
                "This build of Tank Yard doesn't include DLSS."
            }
            .into(),
        )
    }
    pub fn available() -> bool {
        false
    }
    pub fn runtime_download() -> Option<super::download::Progress> {
        None
    }
    pub fn start_runtime_download() {}
    pub struct Upscaler;
    impl Upscaler {
        pub fn new() -> Option<Self> {
            None
        }
        pub fn render_size(&mut self, _: (u32, u32), _: Mode, _: f32) -> Option<(u32, u32)> {
            None
        }
        pub fn select_scene(&mut self, _: (u32, u32), _: (u32, u32), _: Mode) -> Option<Vec2> {
            None
        }
        pub fn select_motion(&mut self) -> bool {
            false
        }
        pub fn input(&mut self) -> Option<Texture2D> {
            None
        }
        pub fn upscale(&mut self, _: bool) -> Option<Texture2D> {
            None
        }
    }
}
#[cfg(not(all(tankyard_dlss, any(target_os = "linux", target_os = "windows"))))]
pub use native::*;

/// Fetching the DLSS runtime, which is too large to ship in every download.
pub mod download {
    #[derive(Clone, Debug, PartialEq)]
    #[cfg_attr(not(tankyard_dlss), allow(dead_code))]
    pub enum Progress {
        Running { done: u64, total: u64 },
        Failed(String),
        Finished,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn jitter_covers_the_pixel_evenly_and_repeats() {
        let samples: Vec<Vec2> = (0..8).map(|i| jitter(i, 8)).collect();
        assert!(samples.iter().all(|s| s.x.abs() <= 0.5 && s.y.abs() <= 0.5));
        let mean = samples.iter().copied().sum::<Vec2>() / 8.;
        assert!(mean.length() < 0.1, "{mean:?}");
        assert_eq!(jitter(3, 8), jitter(11, 8));
        assert_eq!(phases((1920, 1080), (1920, 1080)), 8);
        assert_eq!(phases((960, 540), (1920, 1080)), 32);
    }

    #[test]
    fn render_scales_pick_the_matching_quality_mode() {
        assert_eq!(Mode::for_scale(1.0), Mode::Dlaa);
        assert_eq!(Mode::for_scale(1.5), Mode::Dlaa);
        assert_eq!(Mode::for_scale(0.67), Mode::Quality);
        assert_eq!(Mode::for_scale(0.58), Mode::Balanced);
        assert_eq!(Mode::for_scale(0.5), Mode::Performance);
        assert_eq!(Mode::for_scale(0.33), Mode::UltraPerformance);
    }
}
