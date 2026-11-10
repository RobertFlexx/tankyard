//! Getting the best GPU and driver behavior out of each system.
//!
//! Before the window opens: laptops with two GPUs are steered to the
//! discrete one (Optimus and PowerXpress exports on Windows, PRIME render
//! offload on Linux), and NVIDIA's Linux driver is told to queue only one
//! frame ahead, which cuts input latency. If a launch with these changes
//! never reaches its first frame, the next launch leaves them out.
//!
//! After it opens: the GPU, driver and video memory are read for the
//! Graphics settings, the splash and the log (from Metal on a Mac), and
//! software renderers are flagged.
use crate::settings::Settings;
use macroquad::{miniquad, prelude::*};
use std::{
    cell::RefCell,
    ffi::CStr,
    path::PathBuf,
    sync::atomic::{AtomicBool, Ordering},
};

/// Asks NVIDIA Optimus and AMD switchable graphics for the discrete GPU.
/// The build exports these from the executable (see build.rs).
#[cfg(target_os = "windows")]
#[unsafe(no_mangle)]
#[used]
pub static NvOptimusEnablement: u32 = 1;
#[cfg(target_os = "windows")]
#[unsafe(no_mangle)]
#[used]
pub static AmdPowerXpressRequestHighPerformance: u32 = 1;

/// What the game is rendering on.
#[derive(Clone, Debug, Default)]
pub struct Device {
    pub vendor: String,
    pub renderer: String,
    pub version: String,
    /// Dedicated video memory, where the driver reports it.
    pub memory_mb: Option<u32>,
    /// A CPU rasterizer: the GPU driver is missing or broken.
    pub software: bool,
    /// Rendering was moved to the discrete GPU this launch.
    pub offloaded: bool,
    /// Rendering with Metal rather than OpenGL.
    pub metal: bool,
    /// The GPU shares the system's memory (Apple silicon, Intel graphics).
    pub unified_memory: bool,
}

impl Device {
    pub fn nvidia(&self) -> bool {
        self.vendor.contains("NVIDIA")
    }
    /// The GPU's name, without what drivers add to it: NVIDIA's bus and CPU
    /// features ("…/PCIe/SSE2"), Mesa's prefix and driver details ("Mesa
    /// Intel(R) UHD Graphics 630 (CFL GT2)"), macOS's " OpenGL Engine", and
    /// trademark signs.
    pub fn name(&self) -> String {
        let mut name = self.renderer.split('/').next().unwrap_or_default().trim();
        for prefix in ["Mesa DRI ", "Mesa "] {
            name = name.strip_prefix(prefix).unwrap_or(name);
        }
        name = name.strip_suffix(" OpenGL Engine").unwrap_or(name);
        if name.ends_with(')')
            && let Some(at) = name.find(" (")
        {
            name = &name[..at];
        }
        name.replace("(R)", " ")
            .replace("(TM)", " ")
            .replace("(tm)", " ")
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
    }
    /// One line for the Graphics settings.
    pub fn summary(&self) -> String {
        let details = self.details();
        if details.is_empty() {
            self.name()
        } else {
            format!("{}  ·  {details}", self.name())
        }
    }
    /// Memory, graphics API and driver, without the name.
    pub fn details(&self) -> String {
        let mut parts = Vec::new();
        if let Some(memory) = self.memory_mb {
            parts.push(if memory >= 1024 {
                format!("{:.0} GB", memory as f32 / 1024.)
            } else {
                format!("{memory} MB")
            });
        } else if self.unified_memory {
            parts.push("unified memory".into());
        }
        if self.metal {
            parts.push("Metal".into());
            return parts.join("  ·  ");
        }
        // "4.6.0 NVIDIA 580.95.05" → "OpenGL 4.6 · driver 580.95.05"
        let mut version = self.version.split_whitespace();
        if let Some(number) = version.next() {
            let number: String = number.split('.').take(2).collect::<Vec<_>>().join(".");
            let driver = version
                .last()
                .filter(|v| v.chars().next().is_some_and(|c| c.is_ascii_digit()));
            parts.push(match driver {
                Some(driver)
                    if self.version.contains("NVIDIA") || self.version.contains("Mesa") =>
                {
                    format!("OpenGL {number} · driver {driver}")
                }
                _ => format!("OpenGL {number}"),
            });
        }
        parts.join("  ·  ")
    }
}

thread_local! {
    static DEVICE: RefCell<Option<Device>> = const { RefCell::new(None) };
}
static OFFLOADED: AtomicBool = AtomicBool::new(false);

fn marker() -> PathBuf {
    directories::ProjectDirs::from("org", "Tank Yard", "Tank Yard")
        .map(|dirs| dirs.cache_dir().join("gpu-tuning-pending"))
        .unwrap_or_else(|| PathBuf::from("tankyard-gpu-tuning-pending"))
}

/// Tune the driver environment. Call first thing in `main`, before any
/// thread starts or the window opens.
pub fn prepare(settings: &Settings, safe_graphics: bool) {
    if !cfg!(target_os = "linux") {
        return;
    }
    let marker = marker();
    if marker.exists() {
        // The last launch with these changes never drew a frame.
        log::warn!("Skipping GPU tuning: the last launch didn't start");
        let _ = std::fs::remove_file(&marker);
        return;
    }
    if safe_graphics {
        return;
    }
    let set = |name: &str, value: &str| {
        if std::env::var_os(name).is_none() {
            // SAFETY: called from `main` before any other thread exists.
            unsafe { std::env::set_var(name, value) };
            true
        } else {
            false
        }
    };
    let mut changed = false;
    if std::path::Path::new("/proc/driver/nvidia/version").exists() {
        // One frame queued instead of up to three: input reaches the
        // screen a frame or two sooner, at no cost at these frame rates.
        changed |= set("__GL_MaxFramesAllowed", "1");
        if !settings.battery_saver && nvidia_offload_needed() {
            let offloaded =
                set("__NV_PRIME_RENDER_OFFLOAD", "1") & set("__GLX_VENDOR_LIBRARY_NAME", "nvidia");
            set("__VK_LAYER_NV_optimus", "NVIDIA_only");
            if offloaded {
                OFFLOADED.store(true, Ordering::Relaxed);
                log::info!("Rendering on the NVIDIA GPU (PRIME render offload)");
            }
            changed |= offloaded;
        }
    }
    if changed {
        if let Some(parent) = marker.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let _ = std::fs::write(&marker, b"");
    }
}

/// The display runs on another GPU, and an NVIDIA GPU sits idle beside it.
fn nvidia_offload_needed() -> bool {
    if std::env::var_os("DRI_PRIME").is_some() {
        return false;
    }
    let Ok(cards) = std::fs::read_dir("/sys/class/drm") else {
        return false;
    };
    let mut nvidia_idle = false;
    let mut other_primary = false;
    for card in cards.flatten() {
        let name = card.file_name();
        let name = name.to_string_lossy();
        if !name
            .strip_prefix("card")
            .is_some_and(|n| n.chars().all(|c| c.is_ascii_digit()))
        {
            continue;
        }
        let device = card.path().join("device");
        let read = |file: &str| {
            std::fs::read_to_string(device.join(file))
                .map(|s| s.trim().to_owned())
                .unwrap_or_default()
        };
        let primary = read("boot_vga") == "1";
        match read("vendor").as_str() {
            "0x10de" => nvidia_idle |= !primary,
            "" => {}
            _ => other_primary |= primary,
        }
    }
    nvidia_idle && other_primary
}

/// Read the device once OpenGL is up; the window is drawing, so tuning
/// worked and the marker goes. Later calls do nothing.
pub fn probe() {
    static PROBED: AtomicBool = AtomicBool::new(false);
    if PROBED.swap(true, Ordering::Relaxed) {
        return;
    }
    let _ = std::fs::remove_file(marker());
    let device = unsafe {
        let gl = get_internal_gl();
        match gl.quad_context.info().backend {
            miniquad::Backend::OpenGl => Some(read_device()),
            miniquad::Backend::Metal => gl.quad_context.device_info().map(metal_device),
        }
    };
    let Some(device) = device else {
        return;
    };
    log::info!(
        "GPU: {} ({}), {}",
        device.renderer,
        device.vendor,
        if device.metal {
            "Metal"
        } else {
            &device.version
        }
    );
    if device.software {
        log::warn!(
            "Rendering in software on {}: install or update the GPU driver for full speed",
            device.renderer
        );
    }
    DEVICE.with(|cell| *cell.borrow_mut() = Some(device));
}

/// The Metal device, as macOS names it ("Apple M2 Pro", "AMD Radeon Pro
/// 5500M"). Its memory is given only where it's the GPU's own.
fn metal_device(info: miniquad::DeviceInfo) -> Device {
    let vendor = ["Apple", "AMD", "Intel", "NVIDIA"]
        .into_iter()
        .find(|vendor| info.name.starts_with(vendor))
        .unwrap_or_default();
    Device {
        vendor: vendor.to_owned(),
        renderer: info.name,
        memory_mb: info.memory_mb,
        metal: true,
        unified_memory: info.unified_memory,
        ..Default::default()
    }
}

unsafe fn read_device() -> Device {
    use miniquad::gl::*;
    let text = |name| unsafe {
        let value = glGetString(name);
        if value.is_null() {
            String::new()
        } else {
            CStr::from_ptr(value.cast())
                .to_string_lossy()
                .trim()
                .to_owned()
        }
    };
    let (vendor, renderer, version) = (text(GL_VENDOR), text(0x1F01), text(GL_VERSION));
    let lower = renderer.to_lowercase();
    let software = [
        "llvmpipe",
        "softpipe",
        "swiftshader",
        "software rasterizer",
        "gdi generic",
        "microsoft basic render",
    ]
    .iter()
    .any(|name| lower.contains(name));
    let extensions = unsafe { crate::graphics::gl_extensions() };
    let memory_mb = unsafe {
        let mut value = [0_i32; 4];
        if extensions.iter().any(|e| e == "GL_NVX_gpu_memory_info") {
            // GPU_MEMORY_INFO_DEDICATED_VIDMEM_NVX, in KiB.
            glGetIntegerv(0x9047, value.as_mut_ptr());
            Some(value[0])
        } else if extensions.iter().any(|e| e == "GL_ATI_meminfo") {
            // TEXTURE_FREE_MEMORY_ATI: free memory now, the best AMD offers.
            glGetIntegerv(0x87FC, value.as_mut_ptr());
            Some(value[0])
        } else {
            None
        }
    };
    let _ = unsafe { glGetError() };
    Device {
        vendor,
        renderer,
        version,
        memory_mb: memory_mb.filter(|&kb| kb > 0).map(|kb| (kb / 1024) as u32),
        software,
        offloaded: OFFLOADED.load(Ordering::Relaxed),
        ..Default::default()
    }
}

pub fn device() -> Option<Device> {
    DEVICE.with(|cell| cell.borrow().clone())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn summaries_name_the_gpu_memory_and_driver() {
        let device = Device {
            vendor: "NVIDIA Corporation".into(),
            renderer: "NVIDIA GeForce RTX 2080 Ti/PCIe/SSE2".into(),
            version: "4.6.0 NVIDIA 580.95.05".into(),
            memory_mb: Some(11264),
            ..Default::default()
        };
        assert_eq!(
            device.summary(),
            "NVIDIA GeForce RTX 2080 Ti  ·  11 GB  ·  OpenGL 4.6 · driver 580.95.05"
        );
        assert!(device.nvidia());
        let mesa = Device {
            renderer: "AMD Radeon RX 6600".into(),
            version: "4.6 (Compatibility Profile) Mesa 24.2.3".into(),
            ..Default::default()
        };
        assert_eq!(
            mesa.summary(),
            "AMD Radeon RX 6600  ·  OpenGL 4.6 · driver 24.2.3"
        );
    }

    #[test]
    fn names_drop_what_drivers_add() {
        let name = |renderer: &str| {
            Device {
                renderer: renderer.into(),
                ..Default::default()
            }
            .name()
        };
        assert_eq!(
            name("Mesa Intel(R) UHD Graphics 630 (CFL GT2)"),
            "Intel UHD Graphics 630"
        );
        assert_eq!(
            name("AMD Radeon RX 6600 (radeonsi, navi23, LLVM 15.0.7, DRM 3.49, 6.1.0)"),
            "AMD Radeon RX 6600"
        );
        assert_eq!(
            name("AMD Radeon Pro 5500M OpenGL Engine"),
            "AMD Radeon Pro 5500M"
        );
        assert_eq!(
            name("Intel(R) Iris(R) Xe Graphics"),
            "Intel Iris Xe Graphics"
        );
        assert_eq!(name("AMD Radeon(TM) Graphics"), "AMD Radeon Graphics");
        assert_eq!(name("llvmpipe (LLVM 15.0.7, 256 bits)"), "llvmpipe");
        assert_eq!(name("Apple M2 Pro"), "Apple M2 Pro");
        assert_eq!(name(""), "");
    }

    #[test]
    fn macs_describe_their_metal_gpu() {
        let apple = metal_device(miniquad::DeviceInfo {
            name: "Apple M2 Pro".into(),
            unified_memory: true,
            ..Default::default()
        });
        assert_eq!(apple.vendor, "Apple");
        assert_eq!(apple.summary(), "Apple M2 Pro  ·  unified memory  ·  Metal");
        let radeon = metal_device(miniquad::DeviceInfo {
            name: "AMD Radeon Pro 5500M".into(),
            memory_mb: Some(8176),
            ..Default::default()
        });
        assert_eq!(radeon.summary(), "AMD Radeon Pro 5500M  ·  8 GB  ·  Metal");
        assert!(!radeon.nvidia());
    }
}
