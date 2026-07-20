/* BridgeVM guest Vulkan draw smoke with an image-level assertion.
 *
 * Renders offscreen through the Vulkan loader in two gated stages and reads
 * the pixels back through a host-visible buffer:
 *
 *   gate C: vkCmdClearColorImage to opaque red, expect a uniform readback
 *   gate D: render pass clear to opaque blue plus a half-viewport green
 *           triangle from embedded SPIR-V, expect green top-left / blue
 *           bottom-right regions
 *
 * The same source builds for the Windows ARM64 guest (zig cc, vulkan-1.dll)
 * and for the macOS host (clang, MoltenVK via libvulkan) so the Vulkan logic
 * and shaders can be validated without a guest boot.
 *
 * Exit codes identify the failing gate:
 *   30 loader missing        34 resource setup failed
 *   31 instance failed       35 clear readback mismatch
 *   32 no physical device    36 pipeline creation failed
 *   33 device failed         37 draw readback mismatch
 */

#include <stdarg.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#define VK_NO_PROTOTYPES 1
#include <vulkan/vulkan_core.h>

#include "bridgevm-vulkan-draw-shaders.h"

#ifdef _WIN32
#include <windows.h>
#define DEFAULT_LOG_PATH "C:\\BridgeVM\\bvgpu-vulkan-draw.log"
#define DEFAULT_ICD_JSON "C:\\BridgeVM\\viogpu3d\\virtio_icd.arm64.json"
#else
#include <dlfcn.h>
#define DEFAULT_LOG_PATH "bvgpu-vulkan-draw.log"
#endif

enum {
  IMG_W = 64,
  IMG_H = 64,
  IMG_BYTES = IMG_W * IMG_H * 4,
};

static const char *log_path(void) {
  const char *override = getenv("BVGPU_VULKAN_DRAW_LOG");
  return (override && *override) ? override : DEFAULT_LOG_PATH;
}

/* Open, append, and close per line so the last completed operation survives a
 * watchdog stop, matching the vulkan probe's beacon discipline. */
static void logf_line(const char *fmt, ...) {
  va_list args;
  char line[512];

  va_start(args, fmt);
  vsnprintf(line, sizeof(line), fmt, args);
  va_end(args);

  printf("[vulkan-draw] %s\n", line);
  fflush(stdout);

  FILE *f = fopen(log_path(), "a");
  if (f) {
    fprintf(f, "[vulkan-draw] %s\n", line);
    fclose(f);
  }
}

static int fail(int code, const char *what, long detail) {
  logf_line("FAIL gate=%d what=%s detail=0x%lx", code, what, detail);
  return code;
}

static uint64_t fnv1a64(const uint8_t *data, size_t len) {
  uint64_t hash = 0xcbf29ce484222325ull;
  for (size_t i = 0; i < len; ++i) {
    hash ^= data[i];
    hash *= 0x100000001b3ull;
  }
  return hash;
}

/* ---- dynamic loader ---- */

static PFN_vkGetInstanceProcAddr load_vulkan_loader(void) {
#ifdef _WIN32
  HMODULE module = LoadLibraryW(L"vulkan-1.dll");
  if (!module) {
    logf_line("loader_load_failed win32_error=%lu", GetLastError());
    return NULL;
  }
  return (PFN_vkGetInstanceProcAddr)(void *)GetProcAddress(
      module, "vkGetInstanceProcAddr");
#else
  static const char *candidates[] = {
      "libvulkan.1.dylib",
      "/opt/homebrew/lib/libvulkan.1.dylib",
      "libvulkan.so.1",
  };
  for (size_t i = 0; i < sizeof(candidates) / sizeof(candidates[0]); ++i) {
    void *handle = dlopen(candidates[i], RTLD_NOW | RTLD_LOCAL);
    if (handle) {
      logf_line("loader=%s", candidates[i]);
      return (PFN_vkGetInstanceProcAddr)dlsym(handle, "vkGetInstanceProcAddr");
    }
  }
  logf_line("loader_load_failed dlerror=%s", dlerror());
  return NULL;
#endif
}

/* Instance/device function pointers used by the smoke. */
static PFN_vkGetInstanceProcAddr p_vkGetInstanceProcAddr;
static PFN_vkCreateInstance p_vkCreateInstance;
static PFN_vkEnumeratePhysicalDevices p_vkEnumeratePhysicalDevices;
static PFN_vkGetPhysicalDeviceProperties p_vkGetPhysicalDeviceProperties;
static PFN_vkGetPhysicalDeviceQueueFamilyProperties
    p_vkGetPhysicalDeviceQueueFamilyProperties;
static PFN_vkGetPhysicalDeviceMemoryProperties
    p_vkGetPhysicalDeviceMemoryProperties;
static PFN_vkEnumerateDeviceExtensionProperties
    p_vkEnumerateDeviceExtensionProperties;
static PFN_vkCreateDevice p_vkCreateDevice;
static PFN_vkGetDeviceProcAddr p_vkGetDeviceProcAddr;
static PFN_vkDestroyInstance p_vkDestroyInstance;

static PFN_vkGetDeviceQueue p_vkGetDeviceQueue;
static PFN_vkCreateImage p_vkCreateImage;
static PFN_vkGetImageMemoryRequirements p_vkGetImageMemoryRequirements;
static PFN_vkAllocateMemory p_vkAllocateMemory;
static PFN_vkBindImageMemory p_vkBindImageMemory;
static PFN_vkCreateBuffer p_vkCreateBuffer;
static PFN_vkGetBufferMemoryRequirements p_vkGetBufferMemoryRequirements;
static PFN_vkBindBufferMemory p_vkBindBufferMemory;
static PFN_vkMapMemory p_vkMapMemory;
static PFN_vkCreateCommandPool p_vkCreateCommandPool;
static PFN_vkAllocateCommandBuffers p_vkAllocateCommandBuffers;
static PFN_vkCreateFence p_vkCreateFence;
static PFN_vkResetFences p_vkResetFences;
static PFN_vkBeginCommandBuffer p_vkBeginCommandBuffer;
static PFN_vkEndCommandBuffer p_vkEndCommandBuffer;
static PFN_vkQueueSubmit p_vkQueueSubmit;
static PFN_vkWaitForFences p_vkWaitForFences;
static PFN_vkCmdPipelineBarrier p_vkCmdPipelineBarrier;
static PFN_vkCmdClearColorImage p_vkCmdClearColorImage;
static PFN_vkCmdCopyImageToBuffer p_vkCmdCopyImageToBuffer;
static PFN_vkCreateImageView p_vkCreateImageView;
static PFN_vkCreateRenderPass p_vkCreateRenderPass;
static PFN_vkCreateFramebuffer p_vkCreateFramebuffer;
static PFN_vkCreateShaderModule p_vkCreateShaderModule;
static PFN_vkCreatePipelineLayout p_vkCreatePipelineLayout;
static PFN_vkCreateGraphicsPipelines p_vkCreateGraphicsPipelines;
static PFN_vkCmdBeginRenderPass p_vkCmdBeginRenderPass;
static PFN_vkCmdBindPipeline p_vkCmdBindPipeline;
static PFN_vkCmdDraw p_vkCmdDraw;
static PFN_vkCmdEndRenderPass p_vkCmdEndRenderPass;
static PFN_vkDeviceWaitIdle p_vkDeviceWaitIdle;

#define LOAD_INSTANCE_FN(instance, name)                                       \
  do {                                                                         \
    p_##name = (PFN_##name)p_vkGetInstanceProcAddr(instance, #name);           \
    if (!p_##name) {                                                           \
      logf_line("missing_instance_fn=%s", #name);                              \
      return 0;                                                                \
    }                                                                          \
  } while (0)

#define LOAD_DEVICE_FN(device, name)                                           \
  do {                                                                         \
    p_##name = (PFN_##name)p_vkGetDeviceProcAddr(device, #name);               \
    if (!p_##name) {                                                           \
      logf_line("missing_device_fn=%s", #name);                                \
      return 0;                                                                \
    }                                                                          \
  } while (0)

static int load_instance_fns(VkInstance instance) {
  LOAD_INSTANCE_FN(instance, vkEnumeratePhysicalDevices);
  LOAD_INSTANCE_FN(instance, vkGetPhysicalDeviceProperties);
  LOAD_INSTANCE_FN(instance, vkGetPhysicalDeviceQueueFamilyProperties);
  LOAD_INSTANCE_FN(instance, vkGetPhysicalDeviceMemoryProperties);
  LOAD_INSTANCE_FN(instance, vkEnumerateDeviceExtensionProperties);
  LOAD_INSTANCE_FN(instance, vkCreateDevice);
  LOAD_INSTANCE_FN(instance, vkGetDeviceProcAddr);
  LOAD_INSTANCE_FN(instance, vkDestroyInstance);
  return 1;
}

static int load_device_fns(VkDevice device) {
  LOAD_DEVICE_FN(device, vkGetDeviceQueue);
  LOAD_DEVICE_FN(device, vkCreateImage);
  LOAD_DEVICE_FN(device, vkGetImageMemoryRequirements);
  LOAD_DEVICE_FN(device, vkAllocateMemory);
  LOAD_DEVICE_FN(device, vkBindImageMemory);
  LOAD_DEVICE_FN(device, vkCreateBuffer);
  LOAD_DEVICE_FN(device, vkGetBufferMemoryRequirements);
  LOAD_DEVICE_FN(device, vkBindBufferMemory);
  LOAD_DEVICE_FN(device, vkMapMemory);
  LOAD_DEVICE_FN(device, vkCreateCommandPool);
  LOAD_DEVICE_FN(device, vkAllocateCommandBuffers);
  LOAD_DEVICE_FN(device, vkCreateFence);
  LOAD_DEVICE_FN(device, vkResetFences);
  LOAD_DEVICE_FN(device, vkBeginCommandBuffer);
  LOAD_DEVICE_FN(device, vkEndCommandBuffer);
  LOAD_DEVICE_FN(device, vkQueueSubmit);
  LOAD_DEVICE_FN(device, vkWaitForFences);
  LOAD_DEVICE_FN(device, vkCmdPipelineBarrier);
  LOAD_DEVICE_FN(device, vkCmdClearColorImage);
  LOAD_DEVICE_FN(device, vkCmdCopyImageToBuffer);
  LOAD_DEVICE_FN(device, vkCreateImageView);
  LOAD_DEVICE_FN(device, vkCreateRenderPass);
  LOAD_DEVICE_FN(device, vkCreateFramebuffer);
  LOAD_DEVICE_FN(device, vkCreateShaderModule);
  LOAD_DEVICE_FN(device, vkCreatePipelineLayout);
  LOAD_DEVICE_FN(device, vkCreateGraphicsPipelines);
  LOAD_DEVICE_FN(device, vkCmdBeginRenderPass);
  LOAD_DEVICE_FN(device, vkCmdBindPipeline);
  LOAD_DEVICE_FN(device, vkCmdDraw);
  LOAD_DEVICE_FN(device, vkCmdEndRenderPass);
  LOAD_DEVICE_FN(device, vkDeviceWaitIdle);
  return 1;
}

static uint32_t find_memory_type(const VkPhysicalDeviceMemoryProperties *props,
                                 uint32_t type_bits,
                                 VkMemoryPropertyFlags required) {
  for (uint32_t i = 0; i < props->memoryTypeCount; ++i) {
    if ((type_bits & (1u << i)) &&
        (props->memoryTypes[i].propertyFlags & required) == required) {
      return i;
    }
  }
  return UINT32_MAX;
}

static int pixel_is(const uint8_t *p, uint8_t r, uint8_t g, uint8_t b,
                    uint8_t a) {
  /* R8G8B8A8_UNORM writes of exact 0.0/1.0 components must round-trip, but
   * tolerate one code of quantization slack. */
  return abs((int)p[0] - r) <= 1 && abs((int)p[1] - g) <= 1 &&
         abs((int)p[2] - b) <= 1 && abs((int)p[3] - a) <= 1;
}

static int submit_and_wait(VkDevice device, VkQueue queue,
                           VkCommandBuffer cmd, VkFence fence,
                           const char *label) {
  VkSubmitInfo submit = {
      .sType = VK_STRUCTURE_TYPE_SUBMIT_INFO,
      .commandBufferCount = 1,
      .pCommandBuffers = &cmd,
  };
  VkResult result = p_vkQueueSubmit(queue, 1, &submit, fence);
  logf_line("%s_submit_result=%d", label, result);
  if (result != VK_SUCCESS) return 0;
  result = p_vkWaitForFences(device, 1, &fence, VK_TRUE, 30ull * 1000000000ull);
  logf_line("%s_fence_result=%d", label, result);
  if (result != VK_SUCCESS) return 0;
  return p_vkResetFences(device, 1, &fence) == VK_SUCCESS;
}

static void copy_image_to_buffer(VkCommandBuffer cmd, VkImage image,
                                 VkBuffer buffer) {
  VkBufferImageCopy region = {
      .imageSubresource =
          {
              .aspectMask = VK_IMAGE_ASPECT_COLOR_BIT,
              .layerCount = 1,
          },
      .imageExtent = {IMG_W, IMG_H, 1},
  };
  p_vkCmdCopyImageToBuffer(cmd, image, VK_IMAGE_LAYOUT_TRANSFER_SRC_OPTIMAL,
                           buffer, 1, &region);
}

int main(void) {
  /* Truncate the log so every run reads as one bounded record. */
  FILE *f = fopen(log_path(), "w");
  if (f) fclose(f);
  logf_line("begin log=%s", log_path());

#ifdef _WIN32
  /* Pin the Venus ICD so an unrelated software ICD cannot mask a package
   * failure, matching bvgpu-vulkan-probe.ps1. */
  if (!getenv("VK_DRIVER_FILES")) {
    DWORD attrs = GetFileAttributesA(DEFAULT_ICD_JSON);
    if (attrs != INVALID_FILE_ATTRIBUTES &&
        !(attrs & FILE_ATTRIBUTE_DIRECTORY)) {
      _putenv("VK_DRIVER_FILES=" DEFAULT_ICD_JSON);
    }
  }
#endif
  logf_line("driver_files=%s",
            getenv("VK_DRIVER_FILES") ? getenv("VK_DRIVER_FILES") : "<unset>");

  p_vkGetInstanceProcAddr = load_vulkan_loader();
  if (!p_vkGetInstanceProcAddr) return fail(30, "loader", 0);
  p_vkCreateInstance = (PFN_vkCreateInstance)p_vkGetInstanceProcAddr(
      NULL, "vkCreateInstance");
  if (!p_vkCreateInstance) return fail(30, "vkCreateInstance_symbol", 0);

  VkApplicationInfo app = {
      .sType = VK_STRUCTURE_TYPE_APPLICATION_INFO,
      .pApplicationName = "BridgeVM Venus draw smoke",
      .apiVersion = VK_MAKE_API_VERSION(0, 1, 1, 0),
  };
  VkInstanceCreateInfo instance_info = {
      .sType = VK_STRUCTURE_TYPE_INSTANCE_CREATE_INFO,
      .pApplicationInfo = &app,
  };
  VkInstance instance = VK_NULL_HANDLE;
  VkResult result = p_vkCreateInstance(&instance_info, NULL, &instance);
  if (result == VK_ERROR_INCOMPATIBLE_DRIVER) {
    /* MoltenVK on the macOS host is a portability implementation and needs
     * opt-in enumeration; harmless and unused in the guest. */
    static const char *portability_ext =
        VK_KHR_PORTABILITY_ENUMERATION_EXTENSION_NAME;
    instance_info.flags = VK_INSTANCE_CREATE_ENUMERATE_PORTABILITY_BIT_KHR;
    instance_info.enabledExtensionCount = 1;
    instance_info.ppEnabledExtensionNames = &portability_ext;
    result = p_vkCreateInstance(&instance_info, NULL, &instance);
    logf_line("instance_portability_retry_result=%d", result);
  }
  logf_line("create_instance_result=%d", result);
  if (result != VK_SUCCESS || instance == VK_NULL_HANDLE)
    return fail(31, "create_instance", result);
  if (!load_instance_fns(instance)) return fail(31, "instance_fns", 0);

  uint32_t device_count = 0;
  result = p_vkEnumeratePhysicalDevices(instance, &device_count, NULL);
  logf_line("enumerate_physical_devices_result=%d count=%u", result,
            device_count);
  if (result != VK_SUCCESS || device_count < 1)
    return fail(32, "enumerate_physical_devices", result);
  VkPhysicalDevice physical = VK_NULL_HANDLE;
  device_count = 1;
  result = p_vkEnumeratePhysicalDevices(instance, &device_count, &physical);
  if ((result != VK_SUCCESS && result != VK_INCOMPLETE) ||
      physical == VK_NULL_HANDLE)
    return fail(32, "select_physical_device", result);

  VkPhysicalDeviceProperties props;
  p_vkGetPhysicalDeviceProperties(physical, &props);
  logf_line("device_name=%s vendor=0x%x device=0x%x api=0x%x",
            props.deviceName, props.vendorID, props.deviceID,
            props.apiVersion);

  uint32_t family_count = 0;
  p_vkGetPhysicalDeviceQueueFamilyProperties(physical, &family_count, NULL);
  VkQueueFamilyProperties families[16];
  if (family_count > 16) family_count = 16;
  p_vkGetPhysicalDeviceQueueFamilyProperties(physical, &family_count,
                                             families);
  uint32_t graphics_family = UINT32_MAX;
  for (uint32_t i = 0; i < family_count; ++i) {
    if (families[i].queueFlags & VK_QUEUE_GRAPHICS_BIT) {
      graphics_family = i;
      break;
    }
  }
  logf_line("graphics_queue_family=%u of %u", graphics_family, family_count);
  if (graphics_family == UINT32_MAX)
    return fail(33, "graphics_queue_family", 0);

  /* VK_KHR_portability_subset must be enabled when advertised. */
  const char *device_exts[1];
  uint32_t device_ext_count = 0;
  {
    uint32_t ext_count = 0;
    p_vkEnumerateDeviceExtensionProperties(physical, NULL, &ext_count, NULL);
    VkExtensionProperties *exts =
        calloc(ext_count ? ext_count : 1, sizeof(*exts));
    if (exts) {
      p_vkEnumerateDeviceExtensionProperties(physical, NULL, &ext_count, exts);
      for (uint32_t i = 0; i < ext_count; ++i) {
        if (strcmp(exts[i].extensionName, "VK_KHR_portability_subset") == 0) {
          device_exts[device_ext_count++] = "VK_KHR_portability_subset";
          break;
        }
      }
      free(exts);
    }
  }

  float queue_priority = 1.0f;
  VkDeviceQueueCreateInfo queue_info = {
      .sType = VK_STRUCTURE_TYPE_DEVICE_QUEUE_CREATE_INFO,
      .queueFamilyIndex = graphics_family,
      .queueCount = 1,
      .pQueuePriorities = &queue_priority,
  };
  VkDeviceCreateInfo device_info = {
      .sType = VK_STRUCTURE_TYPE_DEVICE_CREATE_INFO,
      .queueCreateInfoCount = 1,
      .pQueueCreateInfos = &queue_info,
      .enabledExtensionCount = device_ext_count,
      .ppEnabledExtensionNames = device_ext_count ? device_exts : NULL,
  };
  VkDevice device = VK_NULL_HANDLE;
  result = p_vkCreateDevice(physical, &device_info, NULL, &device);
  logf_line("create_device_result=%d portability_subset=%u", result,
            device_ext_count);
  if (result != VK_SUCCESS || device == VK_NULL_HANDLE)
    return fail(33, "create_device", result);
  if (!load_device_fns(device)) return fail(33, "device_fns", 0);

  VkQueue queue = VK_NULL_HANDLE;
  p_vkGetDeviceQueue(device, graphics_family, 0, &queue);
  if (queue == VK_NULL_HANDLE) return fail(33, "get_device_queue", 0);

  VkPhysicalDeviceMemoryProperties mem_props;
  p_vkGetPhysicalDeviceMemoryProperties(physical, &mem_props);

  /* Offscreen render target. */
  VkImageCreateInfo image_info = {
      .sType = VK_STRUCTURE_TYPE_IMAGE_CREATE_INFO,
      .imageType = VK_IMAGE_TYPE_2D,
      .format = VK_FORMAT_R8G8B8A8_UNORM,
      .extent = {IMG_W, IMG_H, 1},
      .mipLevels = 1,
      .arrayLayers = 1,
      .samples = VK_SAMPLE_COUNT_1_BIT,
      .tiling = VK_IMAGE_TILING_OPTIMAL,
      .usage = VK_IMAGE_USAGE_COLOR_ATTACHMENT_BIT |
               VK_IMAGE_USAGE_TRANSFER_DST_BIT |
               VK_IMAGE_USAGE_TRANSFER_SRC_BIT,
      .initialLayout = VK_IMAGE_LAYOUT_UNDEFINED,
  };
  VkImage image = VK_NULL_HANDLE;
  result = p_vkCreateImage(device, &image_info, NULL, &image);
  if (result != VK_SUCCESS) return fail(34, "create_image", result);

  VkMemoryRequirements image_reqs;
  p_vkGetImageMemoryRequirements(device, image, &image_reqs);
  uint32_t image_type = find_memory_type(
      &mem_props, image_reqs.memoryTypeBits,
      VK_MEMORY_PROPERTY_DEVICE_LOCAL_BIT);
  if (image_type == UINT32_MAX)
    image_type = find_memory_type(&mem_props, image_reqs.memoryTypeBits, 0);
  if (image_type == UINT32_MAX) return fail(34, "image_memory_type", 0);
  VkMemoryAllocateInfo image_alloc = {
      .sType = VK_STRUCTURE_TYPE_MEMORY_ALLOCATE_INFO,
      .allocationSize = image_reqs.size,
      .memoryTypeIndex = image_type,
  };
  VkDeviceMemory image_memory = VK_NULL_HANDLE;
  result = p_vkAllocateMemory(device, &image_alloc, NULL, &image_memory);
  if (result != VK_SUCCESS) return fail(34, "allocate_image_memory", result);
  result = p_vkBindImageMemory(device, image, image_memory, 0);
  if (result != VK_SUCCESS) return fail(34, "bind_image_memory", result);

  /* Host-visible readback buffer. */
  VkBufferCreateInfo buffer_info = {
      .sType = VK_STRUCTURE_TYPE_BUFFER_CREATE_INFO,
      .size = IMG_BYTES,
      .usage = VK_BUFFER_USAGE_TRANSFER_DST_BIT,
  };
  VkBuffer buffer = VK_NULL_HANDLE;
  result = p_vkCreateBuffer(device, &buffer_info, NULL, &buffer);
  if (result != VK_SUCCESS) return fail(34, "create_buffer", result);
  VkMemoryRequirements buffer_reqs;
  p_vkGetBufferMemoryRequirements(device, buffer, &buffer_reqs);
  uint32_t buffer_type = find_memory_type(
      &mem_props, buffer_reqs.memoryTypeBits,
      VK_MEMORY_PROPERTY_HOST_VISIBLE_BIT |
          VK_MEMORY_PROPERTY_HOST_COHERENT_BIT);
  if (buffer_type == UINT32_MAX) return fail(34, "buffer_memory_type", 0);
  VkMemoryAllocateInfo buffer_alloc = {
      .sType = VK_STRUCTURE_TYPE_MEMORY_ALLOCATE_INFO,
      .allocationSize = buffer_reqs.size,
      .memoryTypeIndex = buffer_type,
  };
  VkDeviceMemory buffer_memory = VK_NULL_HANDLE;
  result = p_vkAllocateMemory(device, &buffer_alloc, NULL, &buffer_memory);
  if (result != VK_SUCCESS) return fail(34, "allocate_buffer_memory", result);
  result = p_vkBindBufferMemory(device, buffer, buffer_memory, 0);
  if (result != VK_SUCCESS) return fail(34, "bind_buffer_memory", result);
  void *mapped = NULL;
  result = p_vkMapMemory(device, buffer_memory, 0, VK_WHOLE_SIZE, 0, &mapped);
  if (result != VK_SUCCESS || !mapped) return fail(34, "map_memory", result);

  VkCommandPoolCreateInfo pool_info = {
      .sType = VK_STRUCTURE_TYPE_COMMAND_POOL_CREATE_INFO,
      .queueFamilyIndex = graphics_family,
  };
  VkCommandPool pool = VK_NULL_HANDLE;
  result = p_vkCreateCommandPool(device, &pool_info, NULL, &pool);
  if (result != VK_SUCCESS) return fail(34, "create_command_pool", result);
  VkCommandBufferAllocateInfo cmd_info = {
      .sType = VK_STRUCTURE_TYPE_COMMAND_BUFFER_ALLOCATE_INFO,
      .commandPool = pool,
      .level = VK_COMMAND_BUFFER_LEVEL_PRIMARY,
      .commandBufferCount = 2,
  };
  VkCommandBuffer cmds[2] = {VK_NULL_HANDLE, VK_NULL_HANDLE};
  result = p_vkAllocateCommandBuffers(device, &cmd_info, cmds);
  if (result != VK_SUCCESS) return fail(34, "allocate_command_buffers", result);
  VkFenceCreateInfo fence_info = {
      .sType = VK_STRUCTURE_TYPE_FENCE_CREATE_INFO,
  };
  VkFence fence = VK_NULL_HANDLE;
  result = p_vkCreateFence(device, &fence_info, NULL, &fence);
  if (result != VK_SUCCESS) return fail(34, "create_fence", result);
  logf_line("resources_ready image_bytes=%d", IMG_BYTES);

  /* ---- gate C: clear + readback ---- */
  {
    VkCommandBufferBeginInfo begin = {
        .sType = VK_STRUCTURE_TYPE_COMMAND_BUFFER_BEGIN_INFO,
        .flags = VK_COMMAND_BUFFER_USAGE_ONE_TIME_SUBMIT_BIT,
    };
    result = p_vkBeginCommandBuffer(cmds[0], &begin);
    if (result != VK_SUCCESS) return fail(35, "clear_begin_cmd", result);

    VkImageSubresourceRange range = {
        .aspectMask = VK_IMAGE_ASPECT_COLOR_BIT,
        .levelCount = 1,
        .layerCount = 1,
    };
    VkImageMemoryBarrier to_dst = {
        .sType = VK_STRUCTURE_TYPE_IMAGE_MEMORY_BARRIER,
        .dstAccessMask = VK_ACCESS_TRANSFER_WRITE_BIT,
        .oldLayout = VK_IMAGE_LAYOUT_UNDEFINED,
        .newLayout = VK_IMAGE_LAYOUT_TRANSFER_DST_OPTIMAL,
        .srcQueueFamilyIndex = VK_QUEUE_FAMILY_IGNORED,
        .dstQueueFamilyIndex = VK_QUEUE_FAMILY_IGNORED,
        .image = image,
        .subresourceRange = range,
    };
    p_vkCmdPipelineBarrier(cmds[0], VK_PIPELINE_STAGE_TOP_OF_PIPE_BIT,
                           VK_PIPELINE_STAGE_TRANSFER_BIT, 0, 0, NULL, 0, NULL,
                           1, &to_dst);
    VkClearColorValue red = {.float32 = {1.0f, 0.0f, 0.0f, 1.0f}};
    p_vkCmdClearColorImage(cmds[0], image,
                           VK_IMAGE_LAYOUT_TRANSFER_DST_OPTIMAL, &red, 1,
                           &range);
    VkImageMemoryBarrier to_src = {
        .sType = VK_STRUCTURE_TYPE_IMAGE_MEMORY_BARRIER,
        .srcAccessMask = VK_ACCESS_TRANSFER_WRITE_BIT,
        .dstAccessMask = VK_ACCESS_TRANSFER_READ_BIT,
        .oldLayout = VK_IMAGE_LAYOUT_TRANSFER_DST_OPTIMAL,
        .newLayout = VK_IMAGE_LAYOUT_TRANSFER_SRC_OPTIMAL,
        .srcQueueFamilyIndex = VK_QUEUE_FAMILY_IGNORED,
        .dstQueueFamilyIndex = VK_QUEUE_FAMILY_IGNORED,
        .image = image,
        .subresourceRange = range,
    };
    p_vkCmdPipelineBarrier(cmds[0], VK_PIPELINE_STAGE_TRANSFER_BIT,
                           VK_PIPELINE_STAGE_TRANSFER_BIT, 0, 0, NULL, 0, NULL,
                           1, &to_src);
    copy_image_to_buffer(cmds[0], image, buffer);
    result = p_vkEndCommandBuffer(cmds[0]);
    if (result != VK_SUCCESS) return fail(35, "clear_end_cmd", result);
    if (!submit_and_wait(device, queue, cmds[0], fence, "clear"))
      return fail(35, "clear_submit", 0);

    const uint8_t *pixels = (const uint8_t *)mapped;
    uint32_t red_pixels = 0;
    for (uint32_t i = 0; i < IMG_W * IMG_H; ++i) {
      if (pixel_is(pixels + i * 4, 255, 0, 0, 255)) ++red_pixels;
    }
    logf_line("clear_readback red_pixels=%u expected=%u checksum=0x%016llx",
              red_pixels, IMG_W * IMG_H,
              (unsigned long long)fnv1a64(pixels, IMG_BYTES));
    if (red_pixels != IMG_W * IMG_H) return fail(35, "clear_assert", red_pixels);
    logf_line("gate_clear=PASS");
  }

  /* ---- gate D: render pass draw + readback ---- */
  VkImageViewCreateInfo view_info = {
      .sType = VK_STRUCTURE_TYPE_IMAGE_VIEW_CREATE_INFO,
      .image = image,
      .viewType = VK_IMAGE_VIEW_TYPE_2D,
      .format = VK_FORMAT_R8G8B8A8_UNORM,
      .subresourceRange =
          {
              .aspectMask = VK_IMAGE_ASPECT_COLOR_BIT,
              .levelCount = 1,
              .layerCount = 1,
          },
  };
  VkImageView view = VK_NULL_HANDLE;
  result = p_vkCreateImageView(device, &view_info, NULL, &view);
  if (result != VK_SUCCESS) return fail(36, "create_image_view", result);

  VkAttachmentDescription attachment = {
      .format = VK_FORMAT_R8G8B8A8_UNORM,
      .samples = VK_SAMPLE_COUNT_1_BIT,
      .loadOp = VK_ATTACHMENT_LOAD_OP_CLEAR,
      .storeOp = VK_ATTACHMENT_STORE_OP_STORE,
      .stencilLoadOp = VK_ATTACHMENT_LOAD_OP_DONT_CARE,
      .stencilStoreOp = VK_ATTACHMENT_STORE_OP_DONT_CARE,
      .initialLayout = VK_IMAGE_LAYOUT_UNDEFINED,
      .finalLayout = VK_IMAGE_LAYOUT_TRANSFER_SRC_OPTIMAL,
  };
  VkAttachmentReference color_ref = {
      .attachment = 0,
      .layout = VK_IMAGE_LAYOUT_COLOR_ATTACHMENT_OPTIMAL,
  };
  VkSubpassDescription subpass = {
      .pipelineBindPoint = VK_PIPELINE_BIND_POINT_GRAPHICS,
      .colorAttachmentCount = 1,
      .pColorAttachments = &color_ref,
  };
  VkSubpassDependency dependency = {
      .srcSubpass = 0,
      .dstSubpass = VK_SUBPASS_EXTERNAL,
      .srcStageMask = VK_PIPELINE_STAGE_COLOR_ATTACHMENT_OUTPUT_BIT,
      .dstStageMask = VK_PIPELINE_STAGE_TRANSFER_BIT,
      .srcAccessMask = VK_ACCESS_COLOR_ATTACHMENT_WRITE_BIT,
      .dstAccessMask = VK_ACCESS_TRANSFER_READ_BIT,
  };
  VkRenderPassCreateInfo pass_info = {
      .sType = VK_STRUCTURE_TYPE_RENDER_PASS_CREATE_INFO,
      .attachmentCount = 1,
      .pAttachments = &attachment,
      .subpassCount = 1,
      .pSubpasses = &subpass,
      .dependencyCount = 1,
      .pDependencies = &dependency,
  };
  VkRenderPass render_pass = VK_NULL_HANDLE;
  result = p_vkCreateRenderPass(device, &pass_info, NULL, &render_pass);
  if (result != VK_SUCCESS) return fail(36, "create_render_pass", result);

  VkFramebufferCreateInfo fb_info = {
      .sType = VK_STRUCTURE_TYPE_FRAMEBUFFER_CREATE_INFO,
      .renderPass = render_pass,
      .attachmentCount = 1,
      .pAttachments = &view,
      .width = IMG_W,
      .height = IMG_H,
      .layers = 1,
  };
  VkFramebuffer framebuffer = VK_NULL_HANDLE;
  result = p_vkCreateFramebuffer(device, &fb_info, NULL, &framebuffer);
  if (result != VK_SUCCESS) return fail(36, "create_framebuffer", result);

  VkShaderModuleCreateInfo vert_info = {
      .sType = VK_STRUCTURE_TYPE_SHADER_MODULE_CREATE_INFO,
      .codeSize = sizeof(bridgevm_draw_vert_spv),
      .pCode = bridgevm_draw_vert_spv,
  };
  VkShaderModule vert_module = VK_NULL_HANDLE;
  result = p_vkCreateShaderModule(device, &vert_info, NULL, &vert_module);
  if (result != VK_SUCCESS) return fail(36, "create_vert_module", result);
  VkShaderModuleCreateInfo frag_info = {
      .sType = VK_STRUCTURE_TYPE_SHADER_MODULE_CREATE_INFO,
      .codeSize = sizeof(bridgevm_draw_frag_spv),
      .pCode = bridgevm_draw_frag_spv,
  };
  VkShaderModule frag_module = VK_NULL_HANDLE;
  result = p_vkCreateShaderModule(device, &frag_info, NULL, &frag_module);
  if (result != VK_SUCCESS) return fail(36, "create_frag_module", result);

  VkPipelineLayoutCreateInfo layout_info = {
      .sType = VK_STRUCTURE_TYPE_PIPELINE_LAYOUT_CREATE_INFO,
  };
  VkPipelineLayout layout = VK_NULL_HANDLE;
  result = p_vkCreatePipelineLayout(device, &layout_info, NULL, &layout);
  if (result != VK_SUCCESS) return fail(36, "create_pipeline_layout", result);

  VkPipelineShaderStageCreateInfo stages[2] = {
      {
          .sType = VK_STRUCTURE_TYPE_PIPELINE_SHADER_STAGE_CREATE_INFO,
          .stage = VK_SHADER_STAGE_VERTEX_BIT,
          .module = vert_module,
          .pName = "main",
      },
      {
          .sType = VK_STRUCTURE_TYPE_PIPELINE_SHADER_STAGE_CREATE_INFO,
          .stage = VK_SHADER_STAGE_FRAGMENT_BIT,
          .module = frag_module,
          .pName = "main",
      },
  };
  VkPipelineVertexInputStateCreateInfo vertex_input = {
      .sType = VK_STRUCTURE_TYPE_PIPELINE_VERTEX_INPUT_STATE_CREATE_INFO,
  };
  VkPipelineInputAssemblyStateCreateInfo input_assembly = {
      .sType = VK_STRUCTURE_TYPE_PIPELINE_INPUT_ASSEMBLY_STATE_CREATE_INFO,
      .topology = VK_PRIMITIVE_TOPOLOGY_TRIANGLE_LIST,
  };
  VkViewport viewport = {
      .width = IMG_W,
      .height = IMG_H,
      .maxDepth = 1.0f,
  };
  VkRect2D scissor = {.extent = {IMG_W, IMG_H}};
  VkPipelineViewportStateCreateInfo viewport_state = {
      .sType = VK_STRUCTURE_TYPE_PIPELINE_VIEWPORT_STATE_CREATE_INFO,
      .viewportCount = 1,
      .pViewports = &viewport,
      .scissorCount = 1,
      .pScissors = &scissor,
  };
  VkPipelineRasterizationStateCreateInfo raster = {
      .sType = VK_STRUCTURE_TYPE_PIPELINE_RASTERIZATION_STATE_CREATE_INFO,
      .polygonMode = VK_POLYGON_MODE_FILL,
      .cullMode = VK_CULL_MODE_NONE,
      .frontFace = VK_FRONT_FACE_COUNTER_CLOCKWISE,
      .lineWidth = 1.0f,
  };
  VkPipelineMultisampleStateCreateInfo multisample = {
      .sType = VK_STRUCTURE_TYPE_PIPELINE_MULTISAMPLE_STATE_CREATE_INFO,
      .rasterizationSamples = VK_SAMPLE_COUNT_1_BIT,
  };
  VkPipelineColorBlendAttachmentState blend_attachment = {
      .colorWriteMask = VK_COLOR_COMPONENT_R_BIT | VK_COLOR_COMPONENT_G_BIT |
                        VK_COLOR_COMPONENT_B_BIT | VK_COLOR_COMPONENT_A_BIT,
  };
  VkPipelineColorBlendStateCreateInfo blend = {
      .sType = VK_STRUCTURE_TYPE_PIPELINE_COLOR_BLEND_STATE_CREATE_INFO,
      .attachmentCount = 1,
      .pAttachments = &blend_attachment,
  };
  VkGraphicsPipelineCreateInfo pipeline_info = {
      .sType = VK_STRUCTURE_TYPE_GRAPHICS_PIPELINE_CREATE_INFO,
      .stageCount = 2,
      .pStages = stages,
      .pVertexInputState = &vertex_input,
      .pInputAssemblyState = &input_assembly,
      .pViewportState = &viewport_state,
      .pRasterizationState = &raster,
      .pMultisampleState = &multisample,
      .pColorBlendState = &blend,
      .layout = layout,
      .renderPass = render_pass,
  };
  VkPipeline pipeline = VK_NULL_HANDLE;
  result = p_vkCreateGraphicsPipelines(device, VK_NULL_HANDLE, 1,
                                       &pipeline_info, NULL, &pipeline);
  logf_line("create_graphics_pipeline_result=%d", result);
  if (result != VK_SUCCESS || pipeline == VK_NULL_HANDLE)
    return fail(36, "create_graphics_pipeline", result);

  {
    memset(mapped, 0, IMG_BYTES);
    VkCommandBufferBeginInfo begin = {
        .sType = VK_STRUCTURE_TYPE_COMMAND_BUFFER_BEGIN_INFO,
        .flags = VK_COMMAND_BUFFER_USAGE_ONE_TIME_SUBMIT_BIT,
    };
    result = p_vkBeginCommandBuffer(cmds[1], &begin);
    if (result != VK_SUCCESS) return fail(37, "draw_begin_cmd", result);
    VkClearValue clear_blue = {.color = {.float32 = {0.0f, 0.0f, 1.0f, 1.0f}}};
    VkRenderPassBeginInfo pass_begin = {
        .sType = VK_STRUCTURE_TYPE_RENDER_PASS_BEGIN_INFO,
        .renderPass = render_pass,
        .framebuffer = framebuffer,
        .renderArea = {.extent = {IMG_W, IMG_H}},
        .clearValueCount = 1,
        .pClearValues = &clear_blue,
    };
    p_vkCmdBeginRenderPass(cmds[1], &pass_begin, VK_SUBPASS_CONTENTS_INLINE);
    p_vkCmdBindPipeline(cmds[1], VK_PIPELINE_BIND_POINT_GRAPHICS, pipeline);
    p_vkCmdDraw(cmds[1], 3, 1, 0, 0);
    p_vkCmdEndRenderPass(cmds[1]);
    copy_image_to_buffer(cmds[1], image, buffer);
    result = p_vkEndCommandBuffer(cmds[1]);
    if (result != VK_SUCCESS) return fail(37, "draw_end_cmd", result);
    if (!submit_and_wait(device, queue, cmds[1], fence, "draw"))
      return fail(37, "draw_submit", 0);

    const uint8_t *pixels = (const uint8_t *)mapped;
    uint32_t green_pixels = 0;
    uint32_t blue_pixels = 0;
    uint32_t other_pixels = 0;
    for (uint32_t i = 0; i < IMG_W * IMG_H; ++i) {
      const uint8_t *p = pixels + i * 4;
      if (pixel_is(p, 0, 255, 0, 255)) {
        ++green_pixels;
      } else if (pixel_is(p, 0, 0, 255, 255)) {
        ++blue_pixels;
      } else {
        ++other_pixels;
      }
    }
    const uint8_t *inside = pixels + ((8 * IMG_W) + 8) * 4;
    const uint8_t *outside = pixels + ((56 * IMG_W) + 56) * 4;
    logf_line("draw_readback green=%u blue=%u other=%u inside=%u,%u,%u,%u "
              "outside=%u,%u,%u,%u checksum=0x%016llx",
              green_pixels, blue_pixels, other_pixels, inside[0], inside[1],
              inside[2], inside[3], outside[0], outside[1], outside[2],
              outside[3], (unsigned long long)fnv1a64(pixels, IMG_BYTES));
    /* The half-viewport triangle must cover the inside probe with the
     * fragment color and leave the outside probe at the clear color.  The
     * exact covered count may vary by one diagonal row of edge rules, so
     * assert regions and a sane split rather than a single exact count. */
    if (!pixel_is(inside, 0, 255, 0, 255))
      return fail(37, "draw_inside_pixel", inside[1]);
    if (!pixel_is(outside, 0, 0, 255, 255))
      return fail(37, "draw_outside_pixel", outside[2]);
    if (other_pixels != 0) return fail(37, "draw_other_pixels", other_pixels);
    uint32_t total = IMG_W * IMG_H;
    if (green_pixels < total / 4 || green_pixels > (total * 3) / 4)
      return fail(37, "draw_green_share", green_pixels);
    logf_line("gate_draw=PASS");
  }

  p_vkDeviceWaitIdle(device);
  logf_line("success");
  return 0;
}
