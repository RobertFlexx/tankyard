//! The Vulkan side of DLSS and its interop with the OpenGL renderer.
use super::{Mode, Status, download::Progress};
use crate::target::Target;
use ash::vk;
use macroquad::{miniquad, prelude::*};
use miniquad::gl;
use std::{
    cell::RefCell,
    ffi::{CStr, CString, c_char},
    io::{Read, Write},
    path::PathBuf,
    sync::Mutex,
};

/// Identifies Tank Yard to NGX (a custom engine needs its own GUID).
const PROJECT_ID: &CStr = c"6d1f4a72-8b3e-4c59-9e21-5a7c3d0b9f14";
const RUNTIME_COMMIT: &str = "9981c6350dc58c0ce21f024fd65e31e851c31689"; // SDK v310.9.16
#[cfg(target_os = "linux")]
const RUNTIME: (&str, &str, &str) = (
    "libnvidia-ngx-dlss.so.310.9.16",
    "lib/Linux_x86_64/rel/libnvidia-ngx-dlss.so.310.9.16",
    "30982c937afed85c96b8d313323de9a650a2eec6de1bb0f714b21750f4ce98f6",
);
#[cfg(target_os = "windows")]
const RUNTIME: (&str, &str, &str) = (
    "nvngx_dlss.dll",
    "lib/Windows_x86_64/rel/nvngx_dlss.dll",
    "d00b5b0ab29c3ca101b70c3a5ecb3a01b7ef33766a3383a28a18623083e6123e",
);
const RUNTIME_MAX_BYTES: u64 = 96 << 20;

#[cfg(target_os = "linux")]
const MEMORY_HANDLE: vk::ExternalMemoryHandleTypeFlags =
    vk::ExternalMemoryHandleTypeFlags::OPAQUE_FD;
#[cfg(target_os = "windows")]
const MEMORY_HANDLE: vk::ExternalMemoryHandleTypeFlags =
    vk::ExternalMemoryHandleTypeFlags::OPAQUE_WIN32;
#[cfg(target_os = "linux")]
const SEMAPHORE_HANDLE: vk::ExternalSemaphoreHandleTypeFlags =
    vk::ExternalSemaphoreHandleTypeFlags::OPAQUE_FD;
#[cfg(target_os = "windows")]
const SEMAPHORE_HANDLE: vk::ExternalSemaphoreHandleTypeFlags =
    vk::ExternalSemaphoreHandleTypeFlags::OPAQUE_WIN32;

// EXT_external_objects
const GL_TEXTURE_2D: u32 = 0x0DE1;
const GL_TEXTURE_MIN_FILTER: u32 = 0x2801;
const GL_TEXTURE_MAG_FILTER: u32 = 0x2800;
const GL_TEXTURE_WRAP_S: u32 = 0x2802;
const GL_TEXTURE_WRAP_T: u32 = 0x2803;
const GL_LINEAR: i32 = 0x2601;
const GL_CLAMP_TO_EDGE: i32 = 0x812F;
const GL_RGBA8: u32 = 0x8058;
const GL_RG16F: u32 = 0x822F;
const GL_TEXTURE_TILING_EXT: u32 = 0x9580;
const GL_DEDICATED_MEMORY_OBJECT_EXT: u32 = 0x9581;
const GL_OPTIMAL_TILING_EXT: i32 = 0x9584;
#[cfg(target_os = "linux")]
const GL_HANDLE_TYPE: u32 = 0x9586; // OPAQUE_FD
#[cfg(target_os = "windows")]
const GL_HANDLE_TYPE: u32 = 0x9587; // OPAQUE_WIN32
const GL_LAYOUT_GENERAL_EXT: u32 = 0x958D;
const GL_NUM_DEVICE_UUIDS_EXT: u32 = 0x9596;
const GL_DEVICE_UUID_EXT: u32 = 0x9597;
const GL_VENDOR: u32 = 0x1F00;

/// NGX create flag: motion vectors are at render resolution.
const MV_LOW_RES: i32 = 1 << 1;

#[repr(C)]
struct TyImage {
    view: vk::ImageView,
    image: vk::Image,
    format: vk::Format,
    width: u32,
    height: u32,
}

#[repr(C)]
#[derive(Default)]
struct TyStatus {
    available: i32,
    needs_driver: i32,
    min_driver_major: u32,
    min_driver_minor: u32,
    init_result: i32,
}

#[cfg(target_os = "linux")]
type WChar = u32;
#[cfg(target_os = "windows")]
type WChar = u16;

unsafe extern "C" {
    fn ty_ngx_extensions(
        instance_count: *mut u32,
        instance: *mut *const *const c_char,
        device_count: *mut u32,
        device: *mut *const *const c_char,
    ) -> i32;
    fn ty_ngx_init(
        project_id: *const c_char,
        engine_version: *const c_char,
        data_path: *const WChar,
        runtime_dir: *const WChar,
        instance: vk::Instance,
        physical: vk::PhysicalDevice,
        device: vk::Device,
        instance_proc: vk::PFN_vkGetInstanceProcAddr,
        device_proc: vk::PFN_vkGetDeviceProcAddr,
    ) -> i32;
    fn ty_dlss_status(status: *mut TyStatus);
    fn ty_dlss_optimal(
        out_width: u32,
        out_height: u32,
        quality: i32,
        width: *mut u32,
        height: *mut u32,
        min_width: *mut u32,
        min_height: *mut u32,
        max_width: *mut u32,
        max_height: *mut u32,
    ) -> i32;
    fn ty_dlss_create(
        commands: vk::CommandBuffer,
        in_width: u32,
        in_height: u32,
        out_width: u32,
        out_height: u32,
        quality: i32,
        flags: i32,
    ) -> i32;
    fn ty_dlss_evaluate(
        commands: vk::CommandBuffer,
        color: *const TyImage,
        motion: *const TyImage,
        depth: *const TyImage,
        output: *const TyImage,
        jitter_x: f32,
        jitter_y: f32,
        reset: i32,
        frame_ms: f32,
    ) -> i32;
    fn ty_dlss_release();
    fn ty_ngx_shutdown();
}

/// NGX failures all carry 0xBAD in their top bits.
fn succeeded(result: i32) -> bool {
    (result as u32 & 0xFFF0_0000) != 0xBAD0_0000
}

fn wide(path: &std::path::Path) -> Vec<WChar> {
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::ffi::OsStrExt;
        path.as_os_str().encode_wide().chain([0]).collect()
    }
    #[cfg(target_os = "linux")]
    {
        path.to_string_lossy()
            .chars()
            .map(|c| c as u32)
            .chain([0])
            .collect()
    }
}

fn dirs() -> Option<directories::ProjectDirs> {
    directories::ProjectDirs::from("org", "Tank Yard", "Tank Yard")
}

/// Where the downloaded DLSS runtime lives.
fn runtime_dir() -> PathBuf {
    dirs()
        .map(|d| d.data_dir().join("dlss"))
        .unwrap_or_else(|| PathBuf::from("dlss"))
}

/// A runtime beside the game (packagers may ship one) or downloaded.
fn runtime_present() -> Option<PathBuf> {
    let beside = std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(|dir| dir.to_path_buf()));
    beside
        .into_iter()
        .chain([runtime_dir()])
        .find(|dir| dir.join(RUNTIME.0).is_file())
}

/// The OpenGL side must be NVIDIA's driver with external memory and
/// semaphores; returns the GPU's UUID so Vulkan can pick the same one.
fn gl_interop() -> Result<Option<[u8; 16]>, String> {
    unsafe {
        let info = get_internal_gl().quad_context.info();
        if info.backend != miniquad::Backend::OpenGl || gl::is_gl2() {
            return Err("DLSS needs OpenGL 3 or newer.".into());
        }
    }
    let vendor = unsafe {
        let vendor = gl::glGetString(GL_VENDOR);
        if vendor.is_null() {
            String::new()
        } else {
            CStr::from_ptr(vendor.cast()).to_string_lossy().into_owned()
        }
    };
    if !vendor.contains("NVIDIA") {
        return Err(format!(
            "DLSS needs an NVIDIA RTX GPU; this game is running on {}.",
            if vendor.is_empty() {
                "another GPU"
            } else {
                vendor.as_str()
            }
        ));
    }
    let extensions = unsafe { crate::graphics::gl_extensions() };
    let platform = if cfg!(target_os = "windows") {
        "win32"
    } else {
        "fd"
    };
    let needed = [
        "GL_EXT_memory_object".to_owned(),
        format!("GL_EXT_memory_object_{platform}"),
        "GL_EXT_semaphore".to_owned(),
        format!("GL_EXT_semaphore_{platform}"),
    ];
    if let Some(missing) = needed.iter().find(|n| !extensions.contains(n)) {
        return Err(format!(
            "This driver lacks {missing}; update the NVIDIA driver."
        ));
    }
    if !gl::external_objects_loaded(cfg!(target_os = "linux")) {
        return Err("This driver doesn't expose OpenGL–Vulkan sharing.".into());
    }
    let mut devices = 0;
    let mut uuid = [0_u8; 16];
    unsafe {
        gl::glGetIntegerv(GL_NUM_DEVICE_UUIDS_EXT, &mut devices);
        if devices < 1 {
            return Ok(None);
        }
        gl::glGetUnsignedBytei_vEXT(GL_DEVICE_UUID_EXT, 0, uuid.as_mut_ptr());
    }
    Ok(Some(uuid))
}

#[cfg(target_os = "linux")]
type MemoryExport = ash::khr::external_memory_fd::Device;
#[cfg(target_os = "windows")]
type MemoryExport = ash::khr::external_memory_win32::Device;
#[cfg(target_os = "linux")]
type SemaphoreExport = ash::khr::external_semaphore_fd::Device;
#[cfg(target_os = "windows")]
type SemaphoreExport = ash::khr::external_semaphore_win32::Device;

struct Context {
    _entry: ash::Entry,
    instance: ash::Instance,
    physical: vk::PhysicalDevice,
    device: ash::Device,
    family: u32,
    queue: vk::Queue,
    pool: vk::CommandPool,
    commands: vk::CommandBuffer,
    fence: vk::Fence,
    memory_types: vk::PhysicalDeviceMemoryProperties,
    memory_export: MemoryExport,
    semaphore_export: SemaphoreExport,
    ngx: bool,
    status: Status,
    gpu: String,
}

thread_local! {
    /// Created on the render thread the first time DLSS is considered.
    static CONTEXT: RefCell<Option<Result<Context, String>>> = const { RefCell::new(None) };
}
static DOWNLOAD: Mutex<Option<Progress>> = Mutex::new(None);

fn with_context<T>(f: impl FnOnce(&mut Context) -> T) -> Option<T> {
    CONTEXT.with(|cell| {
        let mut cell = cell.borrow_mut();
        if cell.is_none() {
            let created = unsafe { Context::new() };
            match &created {
                Ok(context) => log::info!("DLSS: {:?} on {}", context.status, context.gpu),
                Err(error) => log::info!("DLSS unavailable: {error}"),
            }
            *cell = Some(created);
        }
        match cell.as_mut() {
            Some(Ok(context)) => Some(f(context)),
            _ => None,
        }
    })
}

pub fn status() -> Status {
    if let Some(Progress::Finished) = runtime_download() {
        // A runtime just arrived: start NGX again so it finds the runtime.
        *DOWNLOAD.lock().unwrap_or_else(|p| p.into_inner()) = None;
        with_context(|context| unsafe { context.start_ngx() });
    }
    with_context(|context| context.status.clone()).unwrap_or_else(|| {
        CONTEXT.with(|cell| match cell.borrow().as_ref() {
            Some(Err(error)) => Status::Unavailable(error.clone()),
            _ => Status::Unavailable("DLSS isn't available.".into()),
        })
    })
}

pub fn available() -> bool {
    status() == Status::Ready
}

pub fn runtime_download() -> Option<Progress> {
    DOWNLOAD.lock().unwrap_or_else(|p| p.into_inner()).clone()
}

/// Download the DLSS runtime from NVIDIA's SDK release, pinned by SHA-256.
pub fn start_runtime_download() {
    let mut state = DOWNLOAD.lock().unwrap_or_else(|p| p.into_inner());
    if matches!(*state, Some(Progress::Running { .. })) {
        return;
    }
    *state = Some(Progress::Running { done: 0, total: 0 });
    drop(state);
    log::info!("Downloading the DLSS runtime");
    let spawned = std::thread::Builder::new()
        .name("dlss-download".into())
        .spawn(|| {
            let result = fetch_runtime();
            *DOWNLOAD.lock().unwrap_or_else(|p| p.into_inner()) = Some(match result {
                Ok(()) => Progress::Finished,
                Err(error) => {
                    log::warn!("DLSS download failed: {error}");
                    Progress::Failed(error)
                }
            });
        });
    if spawned.is_err() {
        *DOWNLOAD.lock().unwrap_or_else(|p| p.into_inner()) =
            Some(Progress::Failed("Couldn't start the download.".into()));
    }
}

fn fetch_runtime() -> Result<(), String> {
    let url = format!(
        "https://raw.githubusercontent.com/NVIDIA/DLSS/{RUNTIME_COMMIT}/{}",
        RUNTIME.1
    );
    let directory = runtime_dir();
    std::fs::create_dir_all(&directory).map_err(|e| e.to_string())?;
    let partial = directory.join(format!("{}.partial", RUNTIME.0));
    let response = ureq::get(&url).call().map_err(|e| e.to_string())?;
    let total = response
        .headers()
        .get("content-length")
        .and_then(|v| v.to_str().ok()?.parse().ok())
        .unwrap_or(0_u64);
    let mut reader = response
        .into_body()
        .into_with_config()
        .limit(RUNTIME_MAX_BYTES)
        .reader();
    let mut file = std::fs::File::create(&partial).map_err(|e| e.to_string())?;
    let mut digest = ring::digest::Context::new(&ring::digest::SHA256);
    let mut buffer = vec![0; 1 << 16];
    let mut done = 0;
    loop {
        let read = reader.read(&mut buffer).map_err(|e| e.to_string())?;
        if read == 0 {
            break;
        }
        digest.update(&buffer[..read]);
        file.write_all(&buffer[..read]).map_err(|e| e.to_string())?;
        done += read as u64;
        *DOWNLOAD.lock().unwrap_or_else(|p| p.into_inner()) =
            Some(Progress::Running { done, total });
    }
    file.sync_all().map_err(|e| e.to_string())?;
    drop(file);
    let actual: String = digest
        .finish()
        .as_ref()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    if actual != RUNTIME.2 {
        let _ = std::fs::remove_file(&partial);
        return Err("The download didn't match NVIDIA's published runtime.".into());
    }
    std::fs::rename(&partial, directory.join(RUNTIME.0)).map_err(|e| e.to_string())
}

impl Context {
    unsafe fn new() -> Result<Self, String> {
        unsafe {
            let uuid = gl_interop()?;
            let entry = ash::Entry::load().map_err(|_| "Vulkan isn't installed.".to_owned())?;
            let (mut instance_count, mut device_count) = (0, 0);
            let (mut instance_names, mut device_names) = (std::ptr::null(), std::ptr::null());
            if !succeeded(ty_ngx_extensions(
                &mut instance_count,
                &mut instance_names,
                &mut device_count,
                &mut device_names,
            )) {
                return Err("NGX didn't list its Vulkan extensions.".into());
            }
            let list = |names: *const *const c_char, count: u32| -> Vec<CString> {
                (0..count as usize)
                    .filter_map(|i| {
                        let name = *names.add(i);
                        (!name.is_null()).then(|| CStr::from_ptr(name).to_owned())
                    })
                    .collect()
            };
            let instance_extensions = list(instance_names, instance_count);
            let mut device_extensions = list(device_names, device_count);
            for name in if cfg!(target_os = "windows") {
                [
                    c"VK_KHR_external_memory_win32",
                    c"VK_KHR_external_semaphore_win32",
                ]
            } else {
                [
                    c"VK_KHR_external_memory_fd",
                    c"VK_KHR_external_semaphore_fd",
                ]
            } {
                if !device_extensions
                    .iter()
                    .any(|known| known.as_c_str() == name)
                {
                    device_extensions.push(name.to_owned());
                }
            }
            let application = vk::ApplicationInfo::default()
                .application_name(c"Tank Yard")
                .engine_name(c"Tank Yard")
                .api_version(vk::API_VERSION_1_3);
            let pointers: Vec<_> = instance_extensions.iter().map(|n| n.as_ptr()).collect();
            let instance = entry
                .create_instance(
                    &vk::InstanceCreateInfo::default()
                        .application_info(&application)
                        .enabled_extension_names(&pointers),
                    None,
                )
                .map_err(|e| format!("Vulkan couldn't start ({e})."))?;
            Self::with_instance(entry, instance, uuid, &device_extensions)
        }
    }

    unsafe fn with_instance(
        entry: ash::Entry,
        instance: ash::Instance,
        uuid: Option<[u8; 16]>,
        extensions: &[CString],
    ) -> Result<Self, String> {
        unsafe {
            // Every failure below destroys the instance and returns at once.
            let fail = |instance: &ash::Instance, message: String| {
                instance.destroy_instance(None);
                Err(message)
            };
            let devices = match instance.enumerate_physical_devices() {
                Ok(devices) => devices,
                Err(e) => return fail(&instance, format!("No Vulkan GPU ({e}).")),
            };
            // The same GPU OpenGL renders on, or else the first NVIDIA one.
            let mut chosen = None;
            for &physical in &devices {
                let mut id = vk::PhysicalDeviceIDProperties::default();
                let mut properties = vk::PhysicalDeviceProperties2::default().push_next(&mut id);
                instance.get_physical_device_properties2(physical, &mut properties);
                let vendor = properties.properties.vendor_id;
                let name = properties
                    .properties
                    .device_name_as_c_str()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_default();
                let same = uuid.is_some_and(|uuid| id.device_uuid == uuid);
                if same || (uuid.is_none() && vendor == 0x10DE && chosen.is_none()) {
                    chosen = Some((physical, name));
                }
            }
            let Some((physical, gpu)) = chosen else {
                return fail(&instance, "Vulkan can't see the GPU OpenGL uses.".into());
            };
            let available = instance
                .enumerate_device_extension_properties(physical)
                .unwrap_or_default();
            if let Some(missing) = extensions.iter().find(|name| {
                !available
                    .iter()
                    .any(|e| e.extension_name_as_c_str().ok() == Some(name.as_c_str()))
            }) {
                return fail(
                    &instance,
                    format!(
                        "This GPU's Vulkan driver lacks {}.",
                        missing.to_string_lossy()
                    ),
                );
            }
            let Some(family) = instance
                .get_physical_device_queue_family_properties(physical)
                .iter()
                .position(|f| {
                    f.queue_flags
                        .contains(vk::QueueFlags::GRAPHICS | vk::QueueFlags::COMPUTE)
                })
                .map(|f| f as u32)
            else {
                return fail(&instance, "No Vulkan graphics queue.".into());
            };
            // NGX uses buffer addresses and typeless storage images where
            // the GPU supports them.
            let mut supported12 = vk::PhysicalDeviceVulkan12Features::default();
            let mut supported = vk::PhysicalDeviceFeatures2::default().push_next(&mut supported12);
            instance.get_physical_device_features2(physical, &mut supported);
            let base = supported.features;
            let mut features12 = vk::PhysicalDeviceVulkan12Features::default()
                .buffer_device_address(supported12.buffer_device_address == vk::TRUE)
                .timeline_semaphore(supported12.timeline_semaphore == vk::TRUE);
            let features = vk::PhysicalDeviceFeatures::default()
                .shader_storage_image_write_without_format(
                    base.shader_storage_image_write_without_format == vk::TRUE,
                )
                .shader_storage_image_read_without_format(
                    base.shader_storage_image_read_without_format == vk::TRUE,
                );
            let priorities = [1.0];
            let queues = [vk::DeviceQueueCreateInfo::default()
                .queue_family_index(family)
                .queue_priorities(&priorities)];
            let pointers: Vec<_> = extensions.iter().map(|n| n.as_ptr()).collect();
            let device = match instance.create_device(
                physical,
                &vk::DeviceCreateInfo::default()
                    .queue_create_infos(&queues)
                    .enabled_extension_names(&pointers)
                    .enabled_features(&features)
                    .push_next(&mut features12),
                None,
            ) {
                Ok(device) => device,
                Err(e) => return fail(&instance, format!("Vulkan device failed ({e}).")),
            };
            let queue = device.get_device_queue(family, 0);
            let made = (|| {
                let pool = device.create_command_pool(
                    &vk::CommandPoolCreateInfo::default()
                        .queue_family_index(family)
                        .flags(vk::CommandPoolCreateFlags::RESET_COMMAND_BUFFER),
                    None,
                )?;
                let commands = device.allocate_command_buffers(
                    &vk::CommandBufferAllocateInfo::default()
                        .command_pool(pool)
                        .command_buffer_count(1),
                )?[0];
                let fence = device.create_fence(
                    &vk::FenceCreateInfo::default().flags(vk::FenceCreateFlags::SIGNALED),
                    None,
                )?;
                Ok::<_, vk::Result>((pool, commands, fence))
            })();
            let (pool, commands, fence) = match made {
                Ok(made) => made,
                Err(e) => {
                    device.destroy_device(None);
                    return fail(&instance, format!("Vulkan setup failed ({e})."));
                }
            };
            let memory_types = instance.get_physical_device_memory_properties(physical);
            let mut context = Self {
                memory_export: MemoryExport::new(&instance, &device),
                semaphore_export: SemaphoreExport::new(&instance, &device),
                _entry: entry,
                instance,
                physical,
                device,
                family,
                queue,
                pool,
                commands,
                fence,
                memory_types,
                ngx: false,
                status: Status::Unavailable(String::new()),
                gpu,
            };
            context.start_ngx();
            Ok(context)
        }
    }

    /// (Re)start NGX and read whether DLSS can run.
    unsafe fn start_ngx(&mut self) {
        unsafe {
            let _ = self.device.device_wait_idle();
            if self.ngx {
                ty_ngx_shutdown();
                self.ngx = false;
            }
            let data = dirs()
                .map(|d| d.cache_dir().join("ngx"))
                .unwrap_or_else(|| PathBuf::from("ngx"));
            let _ = std::fs::create_dir_all(&data);
            let runtime = runtime_present();
            let runtime_wide = runtime.as_deref().map(wide);
            let version = CString::new(env!("CARGO_PKG_VERSION")).unwrap_or_default();
            let result = ty_ngx_init(
                PROJECT_ID.as_ptr(),
                version.as_ptr(),
                wide(&data).as_ptr(),
                runtime_wide
                    .as_ref()
                    .map_or(std::ptr::null(), |w| w.as_ptr()),
                self.instance.handle(),
                self.physical,
                self.device.handle(),
                self._entry.static_fn().get_instance_proc_addr,
                self.instance.fp_v1_0().get_device_proc_addr,
            );
            if !succeeded(result) {
                self.status = Status::Unavailable(format!(
                    "NVIDIA NGX couldn't start (0x{:08x}); update the NVIDIA driver.",
                    result as u32
                ));
                return;
            }
            self.ngx = true;
            let mut status = TyStatus::default();
            ty_dlss_status(&mut status);
            self.status = if status.available != 0 {
                Status::Ready
            } else if status.needs_driver != 0 {
                Status::Unavailable(format!(
                    "DLSS needs NVIDIA driver {}.{} or newer.",
                    status.min_driver_major, status.min_driver_minor
                ))
            } else if runtime.is_none() {
                Status::NeedsRuntime
            } else {
                Status::Unavailable(format!(
                    "This GPU can't run DLSS (0x{:08x}); it needs a GeForce RTX.",
                    status.init_result as u32
                ))
            };
        }
    }

    fn memory_type(&self, bits: u32) -> Option<u32> {
        (0..self.memory_types.memory_type_count).find(|&i| {
            bits & (1 << i) != 0
                && self.memory_types.memory_types[i as usize]
                    .property_flags
                    .contains(vk::MemoryPropertyFlags::DEVICE_LOCAL)
        })
    }

    /// A Vulkan image, optionally shared with OpenGL as a texture.
    unsafe fn image(
        &self,
        size: (u32, u32),
        format: vk::Format,
        gl_format: Option<u32>,
    ) -> Result<Shared, String> {
        unsafe {
            let mut external =
                vk::ExternalMemoryImageCreateInfo::default().handle_types(MEMORY_HANDLE);
            let mut info = vk::ImageCreateInfo::default()
                .image_type(vk::ImageType::TYPE_2D)
                .format(format)
                .extent(vk::Extent3D {
                    width: size.0,
                    height: size.1,
                    depth: 1,
                })
                .mip_levels(1)
                .array_layers(1)
                .samples(vk::SampleCountFlags::TYPE_1)
                .tiling(vk::ImageTiling::OPTIMAL)
                .usage(
                    vk::ImageUsageFlags::SAMPLED
                        | vk::ImageUsageFlags::STORAGE
                        | vk::ImageUsageFlags::TRANSFER_SRC
                        | vk::ImageUsageFlags::TRANSFER_DST
                        | vk::ImageUsageFlags::COLOR_ATTACHMENT,
                )
                .sharing_mode(vk::SharingMode::EXCLUSIVE)
                .initial_layout(vk::ImageLayout::UNDEFINED);
            if gl_format.is_some() {
                info = info.push_next(&mut external);
            }
            let image = self
                .device
                .create_image(&info, None)
                .map_err(|e| format!("image: {e}"))?;
            let mut shared = Shared {
                image,
                memory: vk::DeviceMemory::null(),
                view: vk::ImageView::null(),
                format,
                size,
                gl_memory: 0,
                gl_texture: 0,
            };
            let requirements = self.device.get_image_memory_requirements(image);
            let Some(kind) = self.memory_type(requirements.memory_type_bits) else {
                self.destroy(&mut shared);
                return Err("no GPU memory for DLSS images".into());
            };
            let mut dedicated = vk::MemoryDedicatedAllocateInfo::default().image(image);
            let mut export = vk::ExportMemoryAllocateInfo::default().handle_types(MEMORY_HANDLE);
            let mut allocation = vk::MemoryAllocateInfo::default()
                .allocation_size(requirements.size)
                .memory_type_index(kind)
                .push_next(&mut dedicated);
            if gl_format.is_some() {
                allocation = allocation.push_next(&mut export);
            }
            let made = (|| {
                shared.memory = self.device.allocate_memory(&allocation, None)?;
                self.device.bind_image_memory(image, shared.memory, 0)?;
                shared.view = self.device.create_image_view(
                    &vk::ImageViewCreateInfo::default()
                        .image(image)
                        .view_type(vk::ImageViewType::TYPE_2D)
                        .format(format)
                        .subresource_range(color_range()),
                    None,
                )?;
                Ok::<_, vk::Result>(())
            })();
            if let Err(e) = made {
                self.destroy(&mut shared);
                return Err(format!("image memory: {e}"));
            }
            if let Some(gl_format) = gl_format
                && let Err(error) = self.share_with_gl(&mut shared, requirements.size, gl_format)
            {
                self.destroy(&mut shared);
                return Err(error);
            }
            Ok(shared)
        }
    }

    unsafe fn share_with_gl(
        &self,
        shared: &mut Shared,
        bytes: u64,
        format: u32,
    ) -> Result<(), String> {
        unsafe {
            gl::glCreateMemoryObjectsEXT(1, &mut shared.gl_memory);
            let dedicated = 1;
            gl::glMemoryObjectParameterivEXT(
                shared.gl_memory,
                GL_DEDICATED_MEMORY_OBJECT_EXT,
                &dedicated,
            );
            #[cfg(target_os = "linux")]
            {
                let fd = self
                    .memory_export
                    .get_memory_fd(
                        &vk::MemoryGetFdInfoKHR::default()
                            .memory(shared.memory)
                            .handle_type(MEMORY_HANDLE),
                    )
                    .map_err(|e| format!("memory export: {e}"))?;
                // OpenGL takes ownership of the descriptor.
                gl::glImportMemoryFdEXT(shared.gl_memory, bytes, GL_HANDLE_TYPE, fd);
            }
            #[cfg(target_os = "windows")]
            {
                let handle = self
                    .memory_export
                    .get_memory_win32_handle(
                        &vk::MemoryGetWin32HandleInfoKHR::default()
                            .memory(shared.memory)
                            .handle_type(MEMORY_HANDLE),
                    )
                    .map_err(|e| format!("memory export: {e}"))?;
                gl::glImportMemoryWin32HandleEXT(
                    shared.gl_memory,
                    bytes,
                    GL_HANDLE_TYPE,
                    handle as *mut _,
                );
                windows_sys::Win32::Foundation::CloseHandle(handle as _);
            }
            gl::glGenTextures(1, &mut shared.gl_texture);
            gl::glBindTexture(GL_TEXTURE_2D, shared.gl_texture);
            gl::glTexParameteri(GL_TEXTURE_2D, GL_TEXTURE_TILING_EXT, GL_OPTIMAL_TILING_EXT);
            gl::glTexStorageMem2DEXT(
                GL_TEXTURE_2D,
                1,
                format,
                shared.size.0 as i32,
                shared.size.1 as i32,
                shared.gl_memory,
                0,
            );
            gl::glTexParameteri(GL_TEXTURE_2D, GL_TEXTURE_MIN_FILTER, GL_LINEAR);
            gl::glTexParameteri(GL_TEXTURE_2D, GL_TEXTURE_MAG_FILTER, GL_LINEAR);
            gl::glTexParameteri(GL_TEXTURE_2D, GL_TEXTURE_WRAP_S, GL_CLAMP_TO_EDGE);
            gl::glTexParameteri(GL_TEXTURE_2D, GL_TEXTURE_WRAP_T, GL_CLAMP_TO_EDGE);
            gl::glBindTexture(GL_TEXTURE_2D, 0);
            if gl::glGetError() != 0 {
                return Err("OpenGL couldn't use the shared image".into());
            }
            Ok(())
        }
    }

    /// Free an image's Vulkan side; OpenGL objects are freed by their owners.
    unsafe fn destroy(&self, shared: &mut Shared) {
        unsafe {
            if shared.gl_memory != 0 {
                gl::glDeleteMemoryObjectsEXT(1, &shared.gl_memory);
                shared.gl_memory = 0;
            }
            if shared.view != vk::ImageView::null() {
                self.device.destroy_image_view(shared.view, None);
            }
            if shared.image != vk::Image::null() {
                self.device.destroy_image(shared.image, None);
            }
            if shared.memory != vk::DeviceMemory::null() {
                self.device.free_memory(shared.memory, None);
            }
            shared.view = vk::ImageView::null();
            shared.image = vk::Image::null();
            shared.memory = vk::DeviceMemory::null();
        }
    }

    /// A semaphore Vulkan and OpenGL both signal or wait on.
    unsafe fn semaphore(&self) -> Result<(vk::Semaphore, u32), String> {
        unsafe {
            let mut export =
                vk::ExportSemaphoreCreateInfo::default().handle_types(SEMAPHORE_HANDLE);
            let semaphore = self
                .device
                .create_semaphore(
                    &vk::SemaphoreCreateInfo::default().push_next(&mut export),
                    None,
                )
                .map_err(|e| format!("semaphore: {e}"))?;
            let mut gl_semaphore = 0;
            gl::glGenSemaphoresEXT(1, &mut gl_semaphore);
            #[cfg(target_os = "linux")]
            let exported = self
                .semaphore_export
                .get_semaphore_fd(
                    &vk::SemaphoreGetFdInfoKHR::default()
                        .semaphore(semaphore)
                        .handle_type(SEMAPHORE_HANDLE),
                )
                .map(|fd| gl::glImportSemaphoreFdEXT(gl_semaphore, GL_HANDLE_TYPE, fd));
            #[cfg(target_os = "windows")]
            let exported = self
                .semaphore_export
                .get_semaphore_win32_handle(
                    &vk::SemaphoreGetWin32HandleInfoKHR::default()
                        .semaphore(semaphore)
                        .handle_type(SEMAPHORE_HANDLE),
                )
                .map(|handle| {
                    gl::glImportSemaphoreWin32HandleEXT(
                        gl_semaphore,
                        GL_HANDLE_TYPE,
                        handle as *mut _,
                    );
                    windows_sys::Win32::Foundation::CloseHandle(handle as _);
                });
            if let Err(e) = exported {
                gl::glDeleteSemaphoresEXT(1, &gl_semaphore);
                self.device.destroy_semaphore(semaphore, None);
                return Err(format!("semaphore export: {e}"));
            }
            Ok((semaphore, gl_semaphore))
        }
    }

    /// Run `record` in the command buffer and submit it, optionally ordered
    /// after and before OpenGL.
    unsafe fn submit(
        &self,
        wait: Option<vk::Semaphore>,
        signal: Option<vk::Semaphore>,
        record: impl FnOnce(vk::CommandBuffer) -> Result<(), String>,
    ) -> Result<(), String> {
        unsafe {
            let device = &self.device;
            device
                .wait_for_fences(&[self.fence], true, 1_000_000_000)
                .map_err(|e| format!("fence: {e}"))?;
            device
                .reset_command_buffer(self.commands, vk::CommandBufferResetFlags::empty())
                .map_err(|e| e.to_string())?;
            device
                .begin_command_buffer(
                    self.commands,
                    &vk::CommandBufferBeginInfo::default()
                        .flags(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT),
                )
                .map_err(|e| e.to_string())?;
            let recorded = record(self.commands);
            device
                .end_command_buffer(self.commands)
                .map_err(|e| e.to_string())?;
            recorded?;
            device
                .reset_fences(&[self.fence])
                .map_err(|e| e.to_string())?;
            let waits: Vec<_> = wait.into_iter().collect();
            let stages = vec![vk::PipelineStageFlags::ALL_COMMANDS; waits.len()];
            let signals: Vec<_> = signal.into_iter().collect();
            let commands = [self.commands];
            device
                .queue_submit(
                    self.queue,
                    &[vk::SubmitInfo::default()
                        .wait_semaphores(&waits)
                        .wait_dst_stage_mask(&stages)
                        .command_buffers(&commands)
                        .signal_semaphores(&signals)],
                    self.fence,
                )
                .map_err(|e| format!("submit: {e}"))
        }
    }
}

impl Drop for Context {
    fn drop(&mut self) {
        unsafe {
            let _ = self.device.device_wait_idle();
            if self.ngx {
                ty_ngx_shutdown();
            }
            self.device.destroy_fence(self.fence, None);
            self.device.destroy_command_pool(self.pool, None);
            self.device.destroy_device(None);
            self.instance.destroy_instance(None);
        }
    }
}

fn color_range() -> vk::ImageSubresourceRange {
    vk::ImageSubresourceRange::default()
        .aspect_mask(vk::ImageAspectFlags::COLOR)
        .level_count(1)
        .layer_count(1)
}

struct Shared {
    image: vk::Image,
    memory: vk::DeviceMemory,
    view: vk::ImageView,
    format: vk::Format,
    size: (u32, u32),
    gl_memory: u32,
    gl_texture: u32,
}

impl Shared {
    fn ty(&self) -> TyImage {
        TyImage {
            view: self.view,
            image: self.image,
            format: self.format,
            width: self.size.0,
            height: self.size.1,
        }
    }
}

/// Everything sized for one render and output resolution.
struct Frame {
    render: (u32, u32),
    output: (u32, u32),
    mode: Mode,
    color: Shared,
    motion: Shared,
    depth: Shared,
    result: Shared,
    scene: Target,
    motion_target: Target,
    texture: Texture2D,
    /// Signalled by OpenGL when the inputs are drawn.
    drawn: (vk::Semaphore, u32),
    /// Signalled by Vulkan when the result is ready.
    upscaled: (vk::Semaphore, u32),
    /// Images still in their initial layout.
    fresh: bool,
}

impl Frame {
    unsafe fn new(
        context: &Context,
        render: (u32, u32),
        output: (u32, u32),
        mode: Mode,
    ) -> Result<Self, String> {
        unsafe {
            let mut made: Vec<Shared> = Vec::new();
            let mut semaphores = Vec::new();
            let result = (|| {
                made.push(context.image(render, vk::Format::R8G8B8A8_UNORM, Some(GL_RGBA8))?);
                made.push(context.image(render, vk::Format::R16G16_SFLOAT, Some(GL_RG16F))?);
                made.push(context.image(render, vk::Format::R32_SFLOAT, None)?);
                made.push(context.image(output, vk::Format::R8G8B8A8_UNORM, Some(GL_RGBA8))?);
                semaphores.push(context.semaphore()?);
                semaphores.push(context.semaphore()?);
                Ok::<_, String>(())
            })();
            if let Err(error) = result {
                for mut shared in made {
                    if shared.gl_texture != 0 {
                        gl::glDeleteTextures(1, &shared.gl_texture);
                    }
                    context.destroy(&mut shared);
                }
                for (semaphore, gl_semaphore) in semaphores {
                    gl::glDeleteSemaphoresEXT(1, &gl_semaphore);
                    context.device.destroy_semaphore(semaphore, None);
                }
                return Err(error);
            }
            let mut images = made.into_iter();
            let (color, motion, depth, result) = (
                images.next().unwrap(),
                images.next().unwrap(),
                images.next().unwrap(),
                images.next().unwrap(),
            );
            let adopted = (
                Target::from_raw_gl(render, color.gl_texture),
                Target::from_raw_gl(render, motion.gl_texture),
                crate::target::adopt_gl_texture(output, result.gl_texture),
            );
            let (Some(scene), Some(motion_target), Some(texture)) = adopted else {
                return Err("OpenGL couldn't adopt the shared images".into());
            };
            let texture = Texture2D::from_miniquad_texture(texture);
            Ok(Self {
                render,
                output,
                mode,
                color,
                motion,
                depth,
                result,
                scene,
                motion_target,
                texture,
                drawn: semaphores[0],
                upscaled: semaphores[1],
                fresh: true,
            })
        }
    }

    unsafe fn free(mut self, context: &Context) {
        unsafe {
            let _ = context.device.device_wait_idle();
            ty_dlss_release();
            // The targets delete their OpenGL textures; the result's is ours.
            drop(self.scene);
            drop(self.motion_target);
            gl::glDeleteTextures(1, &self.result.gl_texture);
            for shared in [
                &mut self.color,
                &mut self.motion,
                &mut self.depth,
                &mut self.result,
            ] {
                context.destroy(shared);
            }
            for (semaphore, gl_semaphore) in [self.drawn, self.upscaled] {
                gl::glDeleteSemaphoresEXT(1, &gl_semaphore);
                context.device.destroy_semaphore(semaphore, None);
            }
        }
    }

    /// Layout and ownership changes around DLSS. Shared images come from
    /// OpenGL in GENERAL layout and go back the same way.
    unsafe fn barriers(&self, context: &Context, commands: vk::CommandBuffer, acquire: bool) {
        unsafe {
            let shared = [&self.color, &self.motion, &self.result];
            let mut barriers: Vec<_> = shared
                .iter()
                .map(|image| {
                    let (src, dst) = if acquire {
                        (vk::QUEUE_FAMILY_EXTERNAL, context.family)
                    } else {
                        (context.family, vk::QUEUE_FAMILY_EXTERNAL)
                    };
                    vk::ImageMemoryBarrier::default()
                        .src_access_mask(vk::AccessFlags::MEMORY_WRITE)
                        .dst_access_mask(
                            vk::AccessFlags::MEMORY_READ | vk::AccessFlags::MEMORY_WRITE,
                        )
                        .old_layout(vk::ImageLayout::GENERAL)
                        .new_layout(vk::ImageLayout::GENERAL)
                        .src_queue_family_index(src)
                        .dst_queue_family_index(dst)
                        .image(image.image)
                        .subresource_range(color_range())
                })
                .collect();
            if acquire && self.fresh {
                barriers.push(
                    vk::ImageMemoryBarrier::default()
                        .dst_access_mask(vk::AccessFlags::TRANSFER_WRITE)
                        .old_layout(vk::ImageLayout::UNDEFINED)
                        .new_layout(vk::ImageLayout::GENERAL)
                        .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                        .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                        .image(self.depth.image)
                        .subresource_range(color_range()),
                );
            }
            context.device.cmd_pipeline_barrier(
                commands,
                vk::PipelineStageFlags::ALL_COMMANDS,
                vk::PipelineStageFlags::ALL_COMMANDS,
                vk::DependencyFlags::empty(),
                &[],
                &[],
                &barriers,
            );
            if acquire && self.fresh {
                // The arena is flat: one depth everywhere.
                context.device.cmd_clear_color_image(
                    commands,
                    self.depth.image,
                    vk::ImageLayout::GENERAL,
                    &vk::ClearColorValue { float32: [0.; 4] },
                    &[color_range()],
                );
                let after = vk::ImageMemoryBarrier::default()
                    .src_access_mask(vk::AccessFlags::TRANSFER_WRITE)
                    .dst_access_mask(vk::AccessFlags::SHADER_READ)
                    .old_layout(vk::ImageLayout::GENERAL)
                    .new_layout(vk::ImageLayout::GENERAL)
                    .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                    .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                    .image(self.depth.image)
                    .subresource_range(color_range());
                context.device.cmd_pipeline_barrier(
                    commands,
                    vk::PipelineStageFlags::TRANSFER,
                    vk::PipelineStageFlags::ALL_COMMANDS,
                    vk::DependencyFlags::empty(),
                    &[],
                    &[],
                    &[after],
                );
            }
        }
    }
}

/// One scaler's DLSS state: images, the NGX feature and jitter sequence.
pub struct Upscaler {
    frame: Option<Frame>,
    index: u32,
    jitter: Vec2,
    last: f64,
    failed: bool,
}

impl Upscaler {
    pub fn new() -> Option<Self> {
        available().then(|| Self {
            frame: None,
            index: 0,
            jitter: Vec2::ZERO,
            last: get_time(),
            failed: false,
        })
    }

    /// The render size for `output` in `mode`: `scale` of the output, kept
    /// within what DLSS accepts for that mode (DLAA renders at full size).
    pub fn render_size(
        &mut self,
        output: (u32, u32),
        mode: Mode,
        scale: f32,
    ) -> Option<(u32, u32)> {
        if self.failed {
            return None;
        }
        if mode == Mode::Dlaa {
            return Some(output);
        }
        let (mut w, mut h, mut min_w, mut min_h, mut max_w, mut max_h) = (0, 0, 0, 0, 0, 0);
        let ok = with_context(|_| unsafe {
            succeeded(ty_dlss_optimal(
                output.0,
                output.1,
                mode as i32,
                &mut w,
                &mut h,
                &mut min_w,
                &mut min_h,
                &mut max_w,
                &mut max_h,
            ))
        })?;
        if !ok || w == 0 || h == 0 {
            return None;
        }
        let want = (
            (output.0 as f32 * scale).round() as u32,
            (output.1 as f32 * scale).round() as u32,
        );
        let clamp = |value: u32, low: u32, high: u32, fallback: u32| {
            if low == 0 || high < low {
                fallback
            } else {
                value.clamp(low, high)
            }
        };
        Some((
            clamp(want.0, min_w, max_w, w),
            clamp(want.1, min_h, max_h, h),
        ))
    }

    /// Draw the scene into DLSS's input at `render` size, jittered. Returns
    /// false if DLSS can't run, so the caller uses another upscaler.
    pub fn select_scene(
        &mut self,
        render: (u32, u32),
        output: (u32, u32),
        mode: Mode,
    ) -> Option<Vec2> {
        if self.failed {
            return None;
        }
        let stale = self
            .frame
            .as_ref()
            .is_none_or(|f| f.render != render || f.output != output || f.mode != mode);
        if stale {
            let rebuilt = with_context(|context| unsafe {
                if let Some(old) = self.frame.take() {
                    old.free(context);
                }
                let mut frame = Frame::new(context, render, output, mode)?;
                let created = context.submit(None, None, |commands| {
                    let result = ty_dlss_create(
                        commands,
                        render.0,
                        render.1,
                        output.0,
                        output.1,
                        mode as i32,
                        MV_LOW_RES,
                    );
                    if succeeded(result) {
                        Ok(())
                    } else {
                        Err(format!("DLSS couldn't start (0x{:08x})", result as u32))
                    }
                });
                if let Err(error) = created {
                    frame.free(context);
                    return Err(error);
                }
                let _ = context.device.device_wait_idle();
                frame.fresh = true;
                Ok(frame)
            });
            match rebuilt {
                Some(Ok(frame)) => {
                    log::info!(
                        "DLSS {}: {}x{} to {}x{}",
                        mode.name(),
                        render.0,
                        render.1,
                        output.0,
                        output.1
                    );
                    self.frame = Some(frame);
                    self.index = 0;
                }
                Some(Err(error)) => {
                    log::warn!("DLSS disabled: {error}");
                    self.failed = true;
                    return None;
                }
                None => {
                    self.failed = true;
                    return None;
                }
            }
        }
        let frame = self.frame.as_mut()?;
        self.index = self.index.wrapping_add(1);
        self.jitter = super::jitter(self.index, super::phases(render, output));
        // Jitter is in render pixels; the camera works in window units.
        let units = vec2(
            screen_width() / render.0 as f32,
            screen_height() / render.1 as f32,
        );
        frame.scene.select_offset(self.jitter * units);
        Some(self.jitter)
    }

    /// Draw motion vectors next, into DLSS's motion input.
    pub fn select_motion(&mut self) -> bool {
        match &mut self.frame {
            Some(frame) => {
                frame.motion_target.select();
                true
            }
            None => false,
        }
    }

    /// The scene as drawn, before upscaling.
    pub fn input(&mut self) -> Option<Texture2D> {
        self.frame
            .as_mut()
            .map(|frame| frame.scene.texture().clone())
    }

    /// Upscale what was drawn. `reset` drops DLSS's history, after a cut.
    pub fn upscale(&mut self, reset: bool) -> Option<Texture2D> {
        let frame = self.frame.as_mut()?;
        let now = get_time();
        let frame_ms = ((now - self.last) * 1000.).clamp(0., 250.) as f32;
        self.last = now;
        let textures = [
            frame.color.gl_texture,
            frame.motion.gl_texture,
            frame.result.gl_texture,
        ];
        let layouts = [GL_LAYOUT_GENERAL_EXT; 3];
        unsafe {
            // Draws queued for the inputs go first.
            get_internal_gl().flush();
            gl::glSignalSemaphoreEXT(
                frame.drawn.1,
                0,
                std::ptr::null(),
                3,
                textures.as_ptr(),
                layouts.as_ptr(),
            );
            gl::glFlush();
        }
        let jitter = self.jitter;
        let fresh = frame.fresh;
        let submitted = with_context(|context| unsafe {
            context.submit(Some(frame.drawn.0), Some(frame.upscaled.0), |commands| {
                frame.barriers(context, commands, true);
                let result = ty_dlss_evaluate(
                    commands,
                    &frame.color.ty(),
                    &frame.motion.ty(),
                    &frame.depth.ty(),
                    &frame.result.ty(),
                    // OpenGL hands the images over bottom-up, so DLSS sees
                    // the screen's y axis reversed (motion too; see the shim).
                    jitter.x,
                    -jitter.y,
                    i32::from(reset || fresh),
                    frame_ms,
                );
                frame.barriers(context, commands, false);
                if succeeded(result) {
                    Ok(())
                } else {
                    Err(format!("DLSS failed (0x{:08x})", result as u32))
                }
            })
        });
        match submitted {
            Some(Ok(())) => {
                frame.fresh = false;
                unsafe {
                    gl::glWaitSemaphoreEXT(
                        frame.upscaled.1,
                        0,
                        std::ptr::null(),
                        3,
                        textures.as_ptr(),
                        layouts.as_ptr(),
                    );
                }
                Some(frame.texture.clone())
            }
            Some(Err(error)) => {
                // The semaphore OpenGL signalled was never waited on; start
                // over with fresh objects next time, or give up.
                log::warn!("DLSS disabled: {error}");
                self.failed = true;
                None
            }
            None => None,
        }
    }
}

impl Drop for Upscaler {
    fn drop(&mut self) {
        if let Some(frame) = self.frame.take() {
            with_context(|context| unsafe { frame.free(context) });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Fetches about 60 MB from NVIDIA; run with `--ignored` and
    /// XDG_DATA_HOME pointing somewhere disposable.
    #[test]
    #[ignore]
    fn the_runtime_downloads_and_matches_its_pinned_digest() {
        let path = runtime_dir().join(RUNTIME.0);
        let _ = std::fs::remove_file(&path);
        fetch_runtime().unwrap();
        assert!(path.is_file());
        assert_eq!(runtime_present(), Some(runtime_dir()));
        assert!(matches!(
            *DOWNLOAD.lock().unwrap(),
            Some(Progress::Running { done, total }) if done == total && total > 50 << 20
        ));
    }
}
