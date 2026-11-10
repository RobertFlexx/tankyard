// Tank Yard's bridge to NVIDIA DLSS Super Resolution (NGX, Vulkan).
//
// Compiled only when scripts/fetch_dlss_sdk.py has fetched the pinned SDK,
// whose headers and loader library are NVIDIA's and stay outside this
// repository. Everything here is Tank Yard's own code; the Rust side owns
// the Vulkan device and images and calls these functions on one thread.
#include <vulkan/vulkan.h>

#include "nvsdk_ngx_helpers.h"
#include "nvsdk_ngx_helpers_vk.h"

namespace {
NVSDK_NGX_Parameter* capabilities = nullptr;
NVSDK_NGX_Handle* feature = nullptr;
VkDevice device = VK_NULL_HANDLE;
}  // namespace

struct TyImage {
    VkImageView view;
    VkImage image;
    VkFormat format;
    unsigned width;
    unsigned height;
};

struct TyStatus {
    int available;
    int needs_driver;
    unsigned min_driver_major;
    unsigned min_driver_minor;
    int init_result;
};

static NVSDK_NGX_Resource_VK resource(const TyImage* image, bool write) {
    VkImageSubresourceRange range = {VK_IMAGE_ASPECT_COLOR_BIT, 0, 1, 0, 1};
    return NVSDK_NGX_Create_ImageView_Resource_VK(image->view, image->image, range, image->format,
                                                  image->width, image->height, write);
}

extern "C" {

// Instance and device extensions NGX needs; the strings are NGX's own.
int ty_ngx_extensions(unsigned* instance_count, const char*** instance, unsigned* device_count,
                      const char*** device_extensions) {
    return (int)NVSDK_NGX_VULKAN_RequiredExtensions(instance_count, instance, device_count,
                                                    device_extensions);
}

int ty_ngx_init(const char* project_id, const char* engine_version, const wchar_t* data_path,
                const wchar_t* runtime_dir, VkInstance instance, VkPhysicalDevice physical,
                VkDevice logical, PFN_vkGetInstanceProcAddr instance_proc,
                PFN_vkGetDeviceProcAddr device_proc) {
    const wchar_t* paths[1] = {runtime_dir};
    NVSDK_NGX_FeatureCommonInfo info = {};
    info.PathListInfo.Path = paths;
    info.PathListInfo.Length = runtime_dir ? 1 : 0;
    info.LoggingInfo.MinimumLoggingLevel = NVSDK_NGX_LOGGING_LEVEL_OFF;
    NVSDK_NGX_Result result = NVSDK_NGX_VULKAN_Init_with_ProjectID(
        project_id, NVSDK_NGX_ENGINE_TYPE_CUSTOM, engine_version, data_path, instance, physical,
        logical, instance_proc, device_proc, &info, NVSDK_NGX_Version_API);
    if (NVSDK_NGX_FAILED(result)) {
        return (int)result;
    }
    device = logical;
    return (int)NVSDK_NGX_VULKAN_GetCapabilityParameters(&capabilities);
}

void ty_dlss_status(TyStatus* status) {
    *status = TyStatus{};
    if (!capabilities) {
        return;
    }
    NVSDK_NGX_Parameter_GetI(capabilities, NVSDK_NGX_Parameter_SuperSampling_Available,
                             &status->available);
    NVSDK_NGX_Parameter_GetI(capabilities, NVSDK_NGX_Parameter_SuperSampling_NeedsUpdatedDriver,
                             &status->needs_driver);
    NVSDK_NGX_Parameter_GetUI(capabilities,
                              NVSDK_NGX_Parameter_SuperSampling_MinDriverVersionMajor,
                              &status->min_driver_major);
    NVSDK_NGX_Parameter_GetUI(capabilities,
                              NVSDK_NGX_Parameter_SuperSampling_MinDriverVersionMinor,
                              &status->min_driver_minor);
    NVSDK_NGX_Parameter_GetI(capabilities, NVSDK_NGX_Parameter_SuperSampling_FeatureInitResult,
                             &status->init_result);
}

// The render sizes DLSS accepts for an output size and quality mode.
int ty_dlss_optimal(unsigned out_width, unsigned out_height, int quality, unsigned* width,
                    unsigned* height, unsigned* min_width, unsigned* min_height,
                    unsigned* max_width, unsigned* max_height) {
    if (!capabilities) {
        return (int)NVSDK_NGX_Result_FAIL_NotInitialized;
    }
    float sharpness = 0.f;
    return (int)NGX_DLSS_GET_OPTIMAL_SETTINGS(
        capabilities, out_width, out_height, (NVSDK_NGX_PerfQuality_Value)quality, width, height,
        max_width, max_height, min_width, min_height, &sharpness);
}

void ty_dlss_release(void) {
    if (feature) {
        NVSDK_NGX_VULKAN_ReleaseFeature(feature);
        feature = nullptr;
    }
}

// Record DLSS creation into `commands`, replacing any earlier feature.
int ty_dlss_create(VkCommandBuffer commands, unsigned in_width, unsigned in_height,
                   unsigned out_width, unsigned out_height, int quality, int flags) {
    if (!capabilities) {
        return (int)NVSDK_NGX_Result_FAIL_NotInitialized;
    }
    ty_dlss_release();
    NVSDK_NGX_DLSS_Create_Params params = {};
    params.Feature.InWidth = in_width;
    params.Feature.InHeight = in_height;
    params.Feature.InTargetWidth = out_width;
    params.Feature.InTargetHeight = out_height;
    params.Feature.InPerfQualityValue = (NVSDK_NGX_PerfQuality_Value)quality;
    params.InFeatureCreateFlags = flags;
    return (int)NGX_VULKAN_CREATE_DLSS_EXT1(device, commands, 1, 1, &feature, capabilities,
                                            &params);
}

// Record one upscale into `commands`. Images must already be in GENERAL
// layout; jitter and motion are in render pixels.
int ty_dlss_evaluate(VkCommandBuffer commands, const TyImage* color, const TyImage* motion,
                     const TyImage* depth, const TyImage* output, float jitter_x,
                     float jitter_y, int reset, float frame_ms) {
    if (!feature) {
        return (int)NVSDK_NGX_Result_FAIL_FeatureNotFound;
    }
    NVSDK_NGX_Resource_VK in_color = resource(color, false);
    NVSDK_NGX_Resource_VK in_motion = resource(motion, false);
    NVSDK_NGX_Resource_VK in_depth = resource(depth, false);
    NVSDK_NGX_Resource_VK out_color = resource(output, true);
    NVSDK_NGX_VK_DLSS_Eval_Params params = {};
    params.Feature.pInColor = &in_color;
    params.Feature.pInOutput = &out_color;
    params.pInDepth = &in_depth;
    params.pInMotionVectors = &in_motion;
    params.InJitterOffsetX = jitter_x;
    params.InJitterOffsetY = jitter_y;
    params.InRenderSubrectDimensions.Width = color->width;
    params.InRenderSubrectDimensions.Height = color->height;
    params.InReset = reset;
    // Motion is in render pixels, y down the screen. The images arrive from
    // OpenGL bottom-up, so y runs the other way in them.
    params.InMVScaleX = 1.f;
    params.InMVScaleY = -1.f;
    params.InFrameTimeDeltaInMsec = frame_ms;
    return (int)NGX_VULKAN_EVALUATE_DLSS_EXT(commands, feature, capabilities, &params);
}

void ty_ngx_shutdown(void) {
    ty_dlss_release();
    if (capabilities) {
        NVSDK_NGX_VULKAN_DestroyParameters(capabilities);
        capabilities = nullptr;
    }
    if (device) {
        NVSDK_NGX_VULKAN_Shutdown1(device);
        device = VK_NULL_HANDLE;
    }
}

}  // extern "C"
