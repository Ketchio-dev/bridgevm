#define COBJMACROS
#define WIN32_LEAN_AND_MEAN
#include <windows.h>
#include <d3d10.h>
#include <dxgi.h>
#include <stdint.h>
#include <stdio.h>
#include <string.h>

static int fail_hr(const char *step, HRESULT hr) {
  printf("BV-D3D10-FAIL step=%s hr=0x%08lx\n", step, (unsigned long)hr);
  return 1;
}

int main(int argc, char **argv) {
  ID3D10Device *device = NULL;
  ID3D10Texture2D *target = NULL;
  ID3D10Texture2D *source = NULL;
  ID3D10Texture2D *staging = NULL;
  ID3D10RenderTargetView *view = NULL;
  D3D10_TEXTURE2D_DESC target_desc = {0};
  D3D10_TEXTURE2D_DESC staging_desc;
  D3D10_MAPPED_TEXTURE2D mapped;
  uint32_t source_pixels[64 * 64];
  D3D10_SUBRESOURCE_DATA source_data = {0};
  uint32_t copy_bad = 0;
  HRESULT hr;
  int bind_target = 1;

  if (argc == 2 && strcmp(argv[1], "--unbound") == 0) {
    bind_target = 0;
  } else if (argc != 1) {
    fputs("usage: bridgevm-d3d10-smoke.exe [--unbound]\n", stderr);
    return 2;
  }
  printf("BV-D3D10-MODE target=%s\n", bind_target ? "bound" : "unbound");

  hr = D3D10CreateDevice(NULL, D3D10_DRIVER_TYPE_HARDWARE, NULL, 0,
                         D3D10_SDK_VERSION, &device);
  if (FAILED(hr)) return fail_hr("create-device", hr);

  target_desc.Width = 64;
  target_desc.Height = 64;
  target_desc.MipLevels = 1;
  target_desc.ArraySize = 1;
  target_desc.Format = DXGI_FORMAT_R8G8B8A8_UNORM;
  target_desc.SampleDesc.Count = 1;
  target_desc.Usage = D3D10_USAGE_DEFAULT;
  target_desc.BindFlags = D3D10_BIND_RENDER_TARGET;
  hr = ID3D10Device_CreateTexture2D(device, &target_desc, NULL, &target);
  if (FAILED(hr)) return fail_hr("create-target", hr);
  hr = ID3D10Device_CreateRenderTargetView(device, (ID3D10Resource *)target,
                                           NULL, &view);
  if (FAILED(hr)) return fail_hr("create-rtv", hr);

  staging_desc = target_desc;
  staging_desc.Usage = D3D10_USAGE_STAGING;
  staging_desc.BindFlags = 0;
  staging_desc.CPUAccessFlags = D3D10_CPU_ACCESS_READ;
  hr = ID3D10Device_CreateTexture2D(device, &staging_desc, NULL, &staging);
  if (FAILED(hr)) return fail_hr("create-staging", hr);

  for (uint32_t i = 0; i < 64 * 64; ++i) source_pixels[i] = 0xff332211u;
  source_data.pSysMem = source_pixels;
  source_data.SysMemPitch = 64 * 4;
  {
    D3D10_TEXTURE2D_DESC source_desc = target_desc;
    source_desc.BindFlags = 0;
    hr = ID3D10Device_CreateTexture2D(device, &source_desc, &source_data, &source);
  }
  if (FAILED(hr)) return fail_hr("create-source", hr);
  ID3D10Device_CopyResource(device, (ID3D10Resource *)staging,
                            (ID3D10Resource *)source);
  ID3D10Device_Flush(device);
  hr = ID3D10Texture2D_Map(staging, 0, D3D10_MAP_READ, 0, &mapped);
  if (FAILED(hr)) return fail_hr("map-source-copy", hr);
  {
    const uint8_t *first = (const uint8_t *)mapped.pData;
    for (uint32_t y = 0; y < 64; ++y) {
      const uint8_t *row = (const uint8_t *)mapped.pData + y * mapped.RowPitch;
      for (uint32_t x = 0; x < 64; ++x) {
        const uint8_t *pixel = row + x * 4;
        if (pixel[0] != 0x11 || pixel[1] != 0x22 ||
            pixel[2] != 0x33 || pixel[3] != 0xff) ++copy_bad;
      }
    }
    printf("BV-D3D10-COPY first=%02x%02x%02x%02x bad_pixels=%u\n",
           first[0], first[1], first[2], first[3], copy_bad);
  }
  ID3D10Texture2D_Unmap(staging, 0);

  {
    const FLOAT color[4] = {0.25f, 0.50f, 0.75f, 1.0f};
    if (bind_target)
      ID3D10Device_OMSetRenderTargets(device, 1, &view, NULL);
    ID3D10Device_ClearRenderTargetView(device, view, color);
  }
  ID3D10Device_CopyResource(device, (ID3D10Resource *)staging,
                            (ID3D10Resource *)target);
  ID3D10Device_Flush(device);
  hr = ID3D10Texture2D_Map(staging, 0, D3D10_MAP_READ, 0, &mapped);
  if (FAILED(hr)) return fail_hr("map-staging", hr);

  uint32_t bad = 0;
  uint8_t first[4] = {0, 0, 0, 0};
  for (uint32_t y = 0; y < target_desc.Height; ++y) {
    const uint8_t *row = (const uint8_t *)mapped.pData + y * mapped.RowPitch;
    for (uint32_t x = 0; x < target_desc.Width; ++x) {
      const uint8_t *pixel = row + x * 4;
      if (x == 0 && y == 0) {
        first[0] = pixel[0]; first[1] = pixel[1];
        first[2] = pixel[2]; first[3] = pixel[3];
      }
      if (pixel[0] < 63 || pixel[0] > 65 ||
          pixel[1] < 127 || pixel[1] > 129 ||
          pixel[2] < 190 || pixel[2] > 192 || pixel[3] != 255)
        ++bad;
    }
  }
  ID3D10Texture2D_Unmap(staging, 0);

  hr = ID3D10Device_GetDeviceRemovedReason(device);
  printf("BV-D3D10-DEVICE removed_reason=0x%08lx\n", (unsigned long)hr);

  printf("BV-D3D10-RESULT width=%u height=%u "
         "row_pitch=%u first=%02x%02x%02x%02x bad_pixels=%u\n",
         target_desc.Width, target_desc.Height, mapped.RowPitch, first[0],
         first[1], first[2], first[3], bad);

  ID3D10RenderTargetView_Release(view);
  ID3D10Texture2D_Release(source);
  ID3D10Texture2D_Release(staging);
  ID3D10Texture2D_Release(target);
  ID3D10Device_Release(device);
  if (copy_bad || bad) {
    puts("BV-D3D10-FAIL step=verify");
    return 1;
  }
  puts("BV-D3D10-PASS");
  return 0;
}
