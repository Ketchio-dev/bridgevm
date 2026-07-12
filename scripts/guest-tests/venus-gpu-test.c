#include <vulkan/vulkan.h>
#include <stdio.h>
#include <string.h>
#include <stdlib.h>
#include <time.h>
#define CK(x) do { VkResult r_=(x); if (r_) { printf("BV-FB-FAIL %s=%d\n", #x, r_); return 1; } } while(0)
int main(void) {
  VkInstance inst; VkApplicationInfo ai = {VK_STRUCTURE_TYPE_APPLICATION_INFO};
  ai.apiVersion = VK_API_VERSION_1_1;
  VkInstanceCreateInfo ici = {VK_STRUCTURE_TYPE_INSTANCE_CREATE_INFO}; ici.pApplicationInfo = &ai;
  CK(vkCreateInstance(&ici, 0, &inst));
  uint32_t n = 8; VkPhysicalDevice pds[8];
  vkEnumeratePhysicalDevices(inst, &n, pds);
  VkPhysicalDevice pd = 0; VkPhysicalDeviceProperties pr;
  for (uint32_t i = 0; i < n; i++) { vkGetPhysicalDeviceProperties(pds[i], &pr);
    if (strstr(pr.deviceName, "Venus")) { pd = pds[i]; break; } }
  if (!pd) { puts("BV-FB-FAIL no-venus"); return 1; }
  float prio = 1.0f; VkDeviceQueueCreateInfo qci = {VK_STRUCTURE_TYPE_DEVICE_QUEUE_CREATE_INFO};
  qci.queueCount = 1; qci.pQueuePriorities = &prio;
  VkDeviceCreateInfo dci = {VK_STRUCTURE_TYPE_DEVICE_CREATE_INFO};
  dci.queueCreateInfoCount = 1; dci.pQueueCreateInfos = &qci;
  VkDevice dev; CK(vkCreateDevice(pd, &dci, 0, &dev));
  VkQueue q; vkGetDeviceQueue(dev, 0, 0, &q);
  VkPhysicalDeviceMemoryProperties mp; vkGetPhysicalDeviceMemoryProperties(pd, &mp);
  VkCommandPoolCreateInfo cpc = {VK_STRUCTURE_TYPE_COMMAND_POOL_CREATE_INFO};
  VkCommandPool cp; CK(vkCreateCommandPool(dev, &cpc, 0, &cp));
  VkCommandBufferAllocateInfo cba = {VK_STRUCTURE_TYPE_COMMAND_BUFFER_ALLOCATE_INFO};
  cba.commandPool = cp; cba.level = VK_COMMAND_BUFFER_LEVEL_PRIMARY; cba.commandBufferCount = 1;
  VkCommandBufferBeginInfo cbb = {VK_STRUCTURE_TYPE_COMMAND_BUFFER_BEGIN_INFO};
  VkFenceCreateInfo fci = {VK_STRUCTURE_TYPE_FENCE_CREATE_INFO};
  VkFence f; vkCreateFence(dev, &fci, 0, &f);
  VkSubmitInfo si = {VK_STRUCTURE_TYPE_SUBMIT_INFO}; si.commandBufferCount = 1;

  /* ---- correctness: FillBuffer into HOST_VISIBLE memory, read back ---- */
  VkBufferCreateInfo bci = {VK_STRUCTURE_TYPE_BUFFER_CREATE_INFO};
  bci.size = 65536; bci.usage = VK_BUFFER_USAGE_TRANSFER_DST_BIT;
  VkBuffer buf; CK(vkCreateBuffer(dev, &bci, 0, &buf));
  VkMemoryRequirements mr; vkGetBufferMemoryRequirements(dev, buf, &mr);
  uint32_t mi = UINT32_MAX;
  for (uint32_t i = 0; i < mp.memoryTypeCount; i++)
    if ((mr.memoryTypeBits & (1u<<i)) &&
        (mp.memoryTypes[i].propertyFlags & VK_MEMORY_PROPERTY_HOST_VISIBLE_BIT) &&
        (mp.memoryTypes[i].propertyFlags & VK_MEMORY_PROPERTY_HOST_COHERENT_BIT)) { mi = i; break; }
  if (mi == UINT32_MAX) { puts("BV-FB-FAIL memtype"); return 1; }
  VkMemoryAllocateInfo mai = {VK_STRUCTURE_TYPE_MEMORY_ALLOCATE_INFO};
  mai.allocationSize = mr.size; mai.memoryTypeIndex = mi;
  VkDeviceMemory mem; CK(vkAllocateMemory(dev, &mai, 0, &mem));
  vkBindBufferMemory(dev, buf, mem, 0);
  VkCommandBuffer cb; vkAllocateCommandBuffers(dev, &cba, &cb);
  vkBeginCommandBuffer(cb, &cbb);
  vkCmdFillBuffer(cb, buf, 0, 65536, 0xB1D6EB33);
  vkEndCommandBuffer(cb);
  si.pCommandBuffers = &cb;
  CK(vkQueueSubmit(q, 1, &si, f));
  CK(vkWaitForFences(dev, 1, &f, VK_TRUE, 10000000000ull));
  void *p; CK(vkMapMemory(dev, mem, 0, 65536, 0, &p));
  uint32_t *w = p, ok = 1;
  for (int i = 0; i < 16384; i++) if (w[i] != 0xB1D6EB33) { ok = 0; break; }
  if (ok) printf("BV-FILLBUFFER-OK dev=%s\n", pr.deviceName);
  else { printf("BV-FB-FAIL verify w0=%08x\n", w[0]); return 1; }
  vkUnmapMemory(dev, mem);

  /* ---- throughput: 1 GiB of fills over a 128 MiB device-local working set ---- */
  enum { BENCH_SEGMENT_BYTES = 16u<<20, BENCH_SEGMENTS = 8, BENCH_ROUNDS = 8 };
  const VkDeviceSize bench_size = (VkDeviceSize)BENCH_SEGMENT_BYTES * BENCH_SEGMENTS;
  const uint64_t bench_bytes = (uint64_t)BENCH_SEGMENT_BYTES * BENCH_SEGMENTS * BENCH_ROUNDS;
  VkBufferCreateInfo bb = {VK_STRUCTURE_TYPE_BUFFER_CREATE_INFO};
  bb.size = bench_size;
  bb.usage = VK_BUFFER_USAGE_TRANSFER_DST_BIT | VK_BUFFER_USAGE_TRANSFER_SRC_BIT;
  VkBuffer big; if (vkCreateBuffer(dev, &bb, 0, &big)) { puts("BV-BENCH-SKIP create"); goto img; }
  VkMemoryRequirements bmr; vkGetBufferMemoryRequirements(dev, big, &bmr);
  uint32_t bi = UINT32_MAX;
  for (uint32_t i = 0; i < mp.memoryTypeCount; i++)
    if ((bmr.memoryTypeBits & (1u<<i)) &&
        (mp.memoryTypes[i].propertyFlags & VK_MEMORY_PROPERTY_DEVICE_LOCAL_BIT) &&
        !(mp.memoryTypes[i].propertyFlags & VK_MEMORY_PROPERTY_HOST_VISIBLE_BIT)) { bi = i; break; }
  if (bi == UINT32_MAX)
    for (uint32_t i = 0; i < mp.memoryTypeCount; i++)
      if ((bmr.memoryTypeBits & (1u<<i)) &&
          (mp.memoryTypes[i].propertyFlags & VK_MEMORY_PROPERTY_DEVICE_LOCAL_BIT)) { bi = i; break; }
  if (bi == UINT32_MAX)
    for (uint32_t i = 0; i < mp.memoryTypeCount; i++)
      if (bmr.memoryTypeBits & (1u<<i)) { bi = i; break; }
  if (bi == UINT32_MAX) { puts("BV-BENCH-SKIP memtype"); goto img; }
  VkMemoryAllocateInfo bmai = {VK_STRUCTURE_TYPE_MEMORY_ALLOCATE_INFO};
  bmai.allocationSize = bmr.size; bmai.memoryTypeIndex = bi;
  VkDeviceMemory bmem; if (vkAllocateMemory(dev, &bmai, 0, &bmem)) { puts("BV-BENCH-SKIP"); goto img; }
  if (vkBindBufferMemory(dev, big, bmem, 0)) { puts("BV-BENCH-SKIP bind"); goto img; }
  VkCommandBuffer cb2; vkAllocateCommandBuffers(dev, &cba, &cb2);
  vkBeginCommandBuffer(cb2, &cbb);
  for (uint32_t round = 0; round < BENCH_ROUNDS; round++)
    for (uint32_t segment = 0; segment < BENCH_SEGMENTS; segment++)
      vkCmdFillBuffer(cb2, big, (VkDeviceSize)segment * BENCH_SEGMENT_BYTES,
                      BENCH_SEGMENT_BYTES, (round << 16) | segment);
  VkBufferMemoryBarrier bench_barrier = {VK_STRUCTURE_TYPE_BUFFER_MEMORY_BARRIER};
  bench_barrier.srcAccessMask = VK_ACCESS_TRANSFER_WRITE_BIT;
  bench_barrier.dstAccessMask = VK_ACCESS_TRANSFER_READ_BIT;
  bench_barrier.srcQueueFamilyIndex = bench_barrier.dstQueueFamilyIndex = VK_QUEUE_FAMILY_IGNORED;
  bench_barrier.buffer = big;
  bench_barrier.offset = (VkDeviceSize)(BENCH_SEGMENTS - 1) * BENCH_SEGMENT_BYTES;
  bench_barrier.size = BENCH_SEGMENT_BYTES;
  vkCmdPipelineBarrier(cb2, VK_PIPELINE_STAGE_TRANSFER_BIT, VK_PIPELINE_STAGE_TRANSFER_BIT,
                       0, 0, 0, 1, &bench_barrier, 0, 0);
  VkBufferCopy bench_copy = {
    .srcOffset = (VkDeviceSize)(BENCH_SEGMENTS - 1) * BENCH_SEGMENT_BYTES,
    .dstOffset = 0,
    .size = 65536,
  };
  vkCmdCopyBuffer(cb2, big, buf, 1, &bench_copy);
  vkEndCommandBuffer(cb2);
  vkResetFences(dev, 1, &f);
  struct timespec t0, t1; clock_gettime(CLOCK_MONOTONIC, &t0);
  si.pCommandBuffers = &cb2;
  if (vkQueueSubmit(q, 1, &si, f)) { puts("BV-BENCH-FAIL submit"); goto img; }
  if (vkWaitForFences(dev, 1, &f, VK_TRUE, 30000000000ull)) { puts("BV-BENCH-FAIL fence"); goto img; }
  clock_gettime(CLOCK_MONOTONIC, &t1);
  CK(vkMapMemory(dev, mem, 0, 65536, 0, &p));
  w = p; ok = 1;
  const uint32_t bench_expected = ((BENCH_ROUNDS - 1) << 16) | (BENCH_SEGMENTS - 1);
  for (int i = 0; i < 16384; i++) if (w[i] != bench_expected) { ok = 0; break; }
  if (!ok) { printf("BV-BENCH-FAIL verify w0=%08x expected=%08x\n", w[0], bench_expected); return 1; }
  vkUnmapMemory(dev, mem);
  { double sec = (t1.tv_sec - t0.tv_sec) + (t1.tv_nsec - t0.tv_nsec) / 1e9;
    VkMemoryPropertyFlags bench_flags = mp.memoryTypes[bi].propertyFlags;
    printf("BV-BENCH-OK bytes=%llu workset=%llu fills=%u mem_flags=0x%x sec=%.6f GBps=%.2f GiBps=%.2f\n",
           (unsigned long long)bench_bytes, (unsigned long long)bench_size,
           BENCH_SEGMENTS * BENCH_ROUNDS, bench_flags, sec,
           bench_bytes / 1e9 / sec, bench_bytes / 1073741824.0 / sec); }

  /* ---- throughput: 1 GiB of dependent copies between 128 MiB device-local buffers ---- */
  VkBuffer copybuf;
  if (vkCreateBuffer(dev, &bb, 0, &copybuf)) { puts("BV-COPY-BENCH-SKIP create"); goto img; }
  VkMemoryRequirements cmr; vkGetBufferMemoryRequirements(dev, copybuf, &cmr);
  if (!(cmr.memoryTypeBits & (1u << bi))) { puts("BV-COPY-BENCH-SKIP memtype"); goto img; }
  VkMemoryAllocateInfo cmai = {VK_STRUCTURE_TYPE_MEMORY_ALLOCATE_INFO};
  cmai.allocationSize = cmr.size; cmai.memoryTypeIndex = bi;
  VkDeviceMemory cmem;
  if (vkAllocateMemory(dev, &cmai, 0, &cmem)) { puts("BV-COPY-BENCH-SKIP alloc"); goto img; }
  if (vkBindBufferMemory(dev, copybuf, cmem, 0)) { puts("BV-COPY-BENCH-SKIP bind"); goto img; }

  /* Initialize the source outside the timed region. Each segment has a distinct
   * value so the final device-local -> host-visible sample proves the copy chain. */
  VkCommandBuffer cb3; vkAllocateCommandBuffers(dev, &cba, &cb3);
  vkBeginCommandBuffer(cb3, &cbb);
  for (uint32_t segment = 0; segment < BENCH_SEGMENTS; segment++)
    vkCmdFillBuffer(cb3, big, (VkDeviceSize)segment * BENCH_SEGMENT_BYTES,
                    BENCH_SEGMENT_BYTES, 0xC0000000u | segment);
  vkEndCommandBuffer(cb3);
  vkResetFences(dev, 1, &f); si.pCommandBuffers = &cb3;
  if (vkQueueSubmit(q, 1, &si, f) ||
      vkWaitForFences(dev, 1, &f, VK_TRUE, 30000000000ull)) {
    puts("BV-COPY-BENCH-FAIL init"); goto img;
  }

  VkCommandBuffer cb4; vkAllocateCommandBuffers(dev, &cba, &cb4);
  vkBeginCommandBuffer(cb4, &cbb);
  VkBufferCopy full_copy = {.srcOffset = 0, .dstOffset = 0, .size = bench_size};
  VkBufferMemoryBarrier copy_barrier = {VK_STRUCTURE_TYPE_BUFFER_MEMORY_BARRIER};
  copy_barrier.srcAccessMask = VK_ACCESS_TRANSFER_WRITE_BIT;
  copy_barrier.dstAccessMask = VK_ACCESS_TRANSFER_READ_BIT;
  copy_barrier.srcQueueFamilyIndex = copy_barrier.dstQueueFamilyIndex = VK_QUEUE_FAMILY_IGNORED;
  copy_barrier.offset = 0; copy_barrier.size = bench_size;
  for (uint32_t round = 0; round < BENCH_ROUNDS; round++) {
    VkBuffer src = (round & 1) ? copybuf : big;
    VkBuffer dst = (round & 1) ? big : copybuf;
    vkCmdCopyBuffer(cb4, src, dst, 1, &full_copy);
    copy_barrier.buffer = dst;
    vkCmdPipelineBarrier(cb4, VK_PIPELINE_STAGE_TRANSFER_BIT, VK_PIPELINE_STAGE_TRANSFER_BIT,
                         0, 0, 0, 1, &copy_barrier, 0, 0);
  }
  VkBuffer copy_result = (BENCH_ROUNDS & 1) ? copybuf : big;
  VkBufferCopy copy_sample = {
    .srcOffset = (VkDeviceSize)(BENCH_SEGMENTS - 1) * BENCH_SEGMENT_BYTES,
    .dstOffset = 0,
    .size = 65536,
  };
  vkCmdCopyBuffer(cb4, copy_result, buf, 1, &copy_sample);
  vkEndCommandBuffer(cb4);
  vkResetFences(dev, 1, &f); clock_gettime(CLOCK_MONOTONIC, &t0);
  si.pCommandBuffers = &cb4;
  if (vkQueueSubmit(q, 1, &si, f)) { puts("BV-COPY-BENCH-FAIL submit"); goto img; }
  if (vkWaitForFences(dev, 1, &f, VK_TRUE, 30000000000ull)) { puts("BV-COPY-BENCH-FAIL fence"); goto img; }
  clock_gettime(CLOCK_MONOTONIC, &t1);
  CK(vkMapMemory(dev, mem, 0, 65536, 0, &p));
  w = p; ok = 1;
  const uint32_t copy_expected = 0xC0000000u | (BENCH_SEGMENTS - 1);
  for (int i = 0; i < 16384; i++) if (w[i] != copy_expected) { ok = 0; break; }
  if (!ok) { printf("BV-COPY-BENCH-FAIL verify w0=%08x expected=%08x\n", w[0], copy_expected); return 1; }
  vkUnmapMemory(dev, mem);
  { double sec = (t1.tv_sec - t0.tv_sec) + (t1.tv_nsec - t0.tv_nsec) / 1e9;
    printf("BV-COPY-BENCH-OK bytes=%llu workset=%llu copies=%u mem_flags=0x%x sec=%.6f GBps=%.2f GiBps=%.2f\n",
           (unsigned long long)bench_bytes, (unsigned long long)bench_size,
           BENCH_ROUNDS, mp.memoryTypes[bi].propertyFlags, sec,
           bench_bytes / 1e9 / sec, bench_bytes / 1073741824.0 / sec); }

img:;
  /* ---- image: clear an OPTIMAL-tiled image, copy back, verify color ---- */
  VkImageCreateInfo ic = {VK_STRUCTURE_TYPE_IMAGE_CREATE_INFO};
  ic.imageType = VK_IMAGE_TYPE_2D; ic.format = VK_FORMAT_R8G8B8A8_UNORM;
  ic.extent.width = 128; ic.extent.height = 128; ic.extent.depth = 1;
  ic.mipLevels = 1; ic.arrayLayers = 1; ic.samples = VK_SAMPLE_COUNT_1_BIT;
  ic.tiling = VK_IMAGE_TILING_OPTIMAL;
  ic.usage = VK_IMAGE_USAGE_TRANSFER_DST_BIT | VK_IMAGE_USAGE_TRANSFER_SRC_BIT;
  VkImage img; if (vkCreateImage(dev, &ic, 0, &img)) { puts("BV-IMG-FAIL create"); return 0; }
  VkMemoryRequirements imr; vkGetImageMemoryRequirements(dev, img, &imr);
  /* OPTIMAL-tiled images must not land on host-visible (shm-imported)
   * memory — Metal rejects that at execution time (device lost). Prefer a
   * DEVICE_LOCAL, non-host-visible type like real drivers do. */
  uint32_t ii = UINT32_MAX;
  for (uint32_t i = 0; i < mp.memoryTypeCount; i++)
    if ((imr.memoryTypeBits & (1u<<i)) &&
        (mp.memoryTypes[i].propertyFlags & VK_MEMORY_PROPERTY_DEVICE_LOCAL_BIT) &&
        !(mp.memoryTypes[i].propertyFlags & VK_MEMORY_PROPERTY_HOST_VISIBLE_BIT)) { ii = i; break; }
  if (ii == UINT32_MAX)
    for (uint32_t i = 0; i < mp.memoryTypeCount; i++)
      if (imr.memoryTypeBits & (1u<<i)) { ii = i; break; }
  VkMemoryAllocateInfo imai = {VK_STRUCTURE_TYPE_MEMORY_ALLOCATE_INFO};
  imai.allocationSize = imr.size; imai.memoryTypeIndex = ii;
  VkDeviceMemory imem; vkAllocateMemory(dev, &imai, 0, &imem);
  vkBindImageMemory(dev, img, imem, 0);
  VkImageMemoryBarrier ib = {VK_STRUCTURE_TYPE_IMAGE_MEMORY_BARRIER};
  ib.srcQueueFamilyIndex = ib.dstQueueFamilyIndex = VK_QUEUE_FAMILY_IGNORED;
  ib.image = img; ib.subresourceRange.aspectMask = VK_IMAGE_ASPECT_COLOR_BIT;
  ib.subresourceRange.levelCount = 1; ib.subresourceRange.layerCount = 1;
  VkClearColorValue col = {{0.25f, 0.5f, 0.75f, 1.0f}};
  VkImageSubresourceRange rng2 = {VK_IMAGE_ASPECT_COLOR_BIT, 0, 1, 0, 1};
  /* --- submit A: layout transition + clear only --- */
  VkCommandBuffer cbA; vkAllocateCommandBuffers(dev, &cba, &cbA);
  vkBeginCommandBuffer(cbA, &cbb);
  ib.oldLayout = VK_IMAGE_LAYOUT_UNDEFINED; ib.newLayout = VK_IMAGE_LAYOUT_TRANSFER_DST_OPTIMAL;
  ib.dstAccessMask = VK_ACCESS_TRANSFER_WRITE_BIT;
  vkCmdPipelineBarrier(cbA, VK_PIPELINE_STAGE_TOP_OF_PIPE_BIT, VK_PIPELINE_STAGE_TRANSFER_BIT, 0, 0, 0, 0, 0, 1, &ib);
  vkCmdClearColorImage(cbA, img, VK_IMAGE_LAYOUT_TRANSFER_DST_OPTIMAL, &col, 1, &rng2);
  vkEndCommandBuffer(cbA);
  vkResetFences(dev, 1, &f); si.pCommandBuffers = &cbA;
  if (vkQueueSubmit(q, 1, &si, f)) { puts("BV-IMGCLEAR-FAIL submit"); return 0; }
  if (vkWaitForFences(dev, 1, &f, VK_TRUE, 8000000000ull)) { puts("BV-IMGCLEAR-FAIL fence"); return 0; }
  puts("BV-IMGCLEAR-OK");
  /* --- submit B: copy image -> shm buffer only --- */
  VkCommandBuffer cbB; vkAllocateCommandBuffers(dev, &cba, &cbB);
  vkBeginCommandBuffer(cbB, &cbb);
  ib.oldLayout = VK_IMAGE_LAYOUT_TRANSFER_DST_OPTIMAL; ib.newLayout = VK_IMAGE_LAYOUT_TRANSFER_SRC_OPTIMAL;
  ib.srcAccessMask = VK_ACCESS_TRANSFER_WRITE_BIT; ib.dstAccessMask = VK_ACCESS_TRANSFER_READ_BIT;
  vkCmdPipelineBarrier(cbB, VK_PIPELINE_STAGE_TRANSFER_BIT, VK_PIPELINE_STAGE_TRANSFER_BIT, 0, 0, 0, 0, 0, 1, &ib);
  VkBufferImageCopy bic = {0};
  bic.imageSubresource.aspectMask = VK_IMAGE_ASPECT_COLOR_BIT; bic.imageSubresource.layerCount = 1;
  bic.imageExtent.width = 128; bic.imageExtent.height = 128; bic.imageExtent.depth = 1;
  vkCmdCopyImageToBuffer(cbB, img, VK_IMAGE_LAYOUT_TRANSFER_SRC_OPTIMAL, buf, 1, &bic);
  vkEndCommandBuffer(cbB);
  vkResetFences(dev, 1, &f); si.pCommandBuffers = &cbB;
  if (vkQueueSubmit(q, 1, &si, f)) { puts("BV-IMGCOPY-FAIL submit"); return 0; }
  if (vkWaitForFences(dev, 1, &f, VK_TRUE, 8000000000ull)) { puts("BV-IMGCOPY-FAIL fence"); return 0; }
  puts("BV-IMGCOPY-OK");
  void *p2; vkMapMemory(dev, mem, 0, 65536, 0, &p2);
  unsigned char *px = p2;
  printf("BV-IMG %s r=%02x g=%02x b=%02x a=%02x\n",
         (px[0]==0x40 && px[1]==0x80 && px[2]==0xbf && px[3]==0xff) ? "OK" : "FAIL",
         px[0], px[1], px[2], px[3]);
  return 0;
}
