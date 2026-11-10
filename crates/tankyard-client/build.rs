//! Links NVIDIA DLSS when the pinned SDK is present (see
//! scripts/fetch_dlss_sdk.py); otherwise the client builds without it.
use std::path::PathBuf;

fn main() {
    println!("cargo:rustc-check-cfg=cfg(tankyard_dlss)");
    let os = std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    let env = std::env::var("CARGO_CFG_TARGET_ENV").unwrap_or_default();
    // Ask Optimus and AMD switchable graphics laptops for the discrete GPU
    // (the symbols are in hardware.rs).
    if os == "windows" && env == "msvc" {
        for symbol in [
            "NvOptimusEnablement",
            "AmdPowerXpressRequestHighPerformance",
        ] {
            println!("cargo:rustc-link-arg-bin=tankyard=/EXPORT:{symbol},DATA");
        }
    }
    println!("cargo:rerun-if-env-changed=TANKYARD_DLSS_SDK");
    println!("cargo:rerun-if-changed=native/dlss.cpp");
    let arch = std::env::var("CARGO_CFG_TARGET_ARCH").unwrap_or_default();
    let library = match (os.as_str(), arch.as_str(), env.as_str()) {
        ("linux", "x86_64", "gnu") => "lib/Linux_x86_64",
        ("windows", "x86_64", "msvc") => "lib/Windows_x86_64/x64",
        _ => return,
    };
    let manifest = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());
    let sdk = std::env::var_os("TANKYARD_DLSS_SDK")
        .map(PathBuf::from)
        .unwrap_or_else(|| manifest.join("../../target/dlss-sdk"));
    let headers = sdk.join("include");
    let vulkan = sdk.join("vulkan/include");
    let library = sdk.join(library);
    println!("cargo:rerun-if-changed={}", sdk.display());
    if !headers.join("nvsdk_ngx_helpers_vk.h").is_file()
        || !vulkan.join("vulkan/vulkan.h").is_file()
    {
        return;
    }
    // The release builds for Windows link the C runtime statically, and
    // NVIDIA ships the loader built both ways.
    let static_crt = std::env::var("CARGO_CFG_TARGET_FEATURE")
        .unwrap_or_default()
        .split(',')
        .any(|feature| feature == "crt-static");
    let loader = if env != "msvc" {
        "nvsdk_ngx"
    } else if static_crt {
        "nvsdk_ngx_s"
    } else {
        "nvsdk_ngx_d"
    };
    let file = if env == "msvc" {
        format!("{loader}.lib")
    } else {
        format!("lib{loader}.a")
    };
    if !library.join(file).is_file() {
        return;
    }
    let mut build = cc::Build::new();
    build
        .cpp(true)
        .file("native/dlss.cpp")
        .include(&headers)
        .include(&vulkan)
        .warnings(false);
    if env == "msvc" {
        build.static_crt(static_crt).flag("/std:c++17");
    } else {
        build.flag("-std=c++17");
    }
    // The portable Linux build uses libc++; NGX was built against GCC's
    // libstdc++ and needs a few of its functions (see the bridge).
    let target = std::env::var("TARGET").unwrap_or_default();
    let cxx_library = std::env::var(format!("CXXSTDLIB_{}", target.replace('-', "_")))
        .or_else(|_| std::env::var("CXXSTDLIB"))
        .unwrap_or_default();
    println!("cargo:rerun-if-env-changed=CXXSTDLIB");
    println!("cargo:rerun-if-changed=native/libstdcxx_bridge.cpp");
    if env != "msvc" && cxx_library == "c++" {
        build.file("native/libstdcxx_bridge.cpp");
    }
    build.compile("tankyard_dlss");
    println!("cargo:rustc-link-search=native={}", library.display());
    println!("cargo:rustc-link-lib=static={loader}");
    if env == "msvc" {
        for system in [
            "advapi32", "user32", "shell32", "ole32", "version", "dxgi", "d3d11", "d3d12",
        ] {
            println!("cargo:rustc-link-lib=dylib={system}");
        }
    } else {
        println!("cargo:rustc-link-lib=dylib=dl");
    }
    println!("cargo:rustc-cfg=tankyard_dlss");
}
