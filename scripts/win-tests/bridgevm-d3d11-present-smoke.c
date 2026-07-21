/* BridgeVM guest D3D11 windowed present smoke for the DXVK rung.
 *
 * Creates a real HWND, a legacy DXGI swapchain on it, renders magenta into
 * the backbuffer, verifies the pixels with a pre-present readback, then
 * Presents a run of frames.  With the ARM64 DXVK DLLs beside the exe this
 * exercises the full present pipeline: DXVK -> VkSwapchainKHR -> Mesa win32
 * WSI (CPU image + GDI blit) -> HWND, with acquire semaphores completing
 * through the Venus fd==-1 / ImportSemaphoreResourceMESA path.
 *
 * Runs headless-safe: the window is created but never required to be
 * visible, so the SYSTEM/session-0 firstboot context can execute it.
 */

#define COBJMACROS
#define WIN32_LEAN_AND_MEAN
#define INITGUID
#include <windows.h>
#include <d3d11.h>
#include <dxgi.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

typedef HRESULT(WINAPI *d3d_compile_fn)(
    LPCVOID, SIZE_T, LPCSTR, const void *, void *, LPCSTR,
    LPCSTR, UINT, UINT, ID3D10Blob **, ID3D10Blob **);

static int fail_hr(const char *step, HRESULT hr) {
  printf("BV-D3D11-PRESENT-FAIL step=%s hr=0x%08lx\n", step,
         (unsigned long)hr);
  return 1;
}

static LRESULT CALLBACK wnd_proc(HWND hwnd, UINT msg, WPARAM wp, LPARAM lp) {
  return DefWindowProcA(hwnd, msg, wp, lp);
}

int main(void) {
  static const char vs_source[] =
      "float4 main(float2 position : POSITION) : SV_POSITION {"
      "  return float4(position, 0.0, 1.0);"
      "}";
  static const char ps_source[] =
      "float4 main() : SV_TARGET { return float4(1.0, 0.0, 1.0, 1.0); }";
  static const float vertices[3][2] = {
      {-1.0f, -1.0f}, {-1.0f, 3.0f}, {3.0f, -1.0f}};
  HRESULT hr;

  WNDCLASSA wc = {0};
  wc.lpfnWndProc = wnd_proc;
  wc.hInstance = GetModuleHandleA(NULL);
  wc.lpszClassName = "BridgeVMPresentSmoke";
  if (!RegisterClassA(&wc))
    return fail_hr("register-class", HRESULT_FROM_WIN32(GetLastError()));
  /* BV_PRESENT_DEMO=1 runs a longer, visible presentation for scanout
   * capture: the window is shown on the desktop and frames present with
   * vsync pacing, so the host-side ramfb samples of the composited desktop
   * catch the magenta client area. */
  const int demo = GetEnvironmentVariableA("BV_PRESENT_DEMO", NULL, 0) != 0;
  HWND hwnd = CreateWindowExA(0, wc.lpszClassName, "BridgeVM present smoke",
                              WS_OVERLAPPEDWINDOW, 40, 40, 320, 240, NULL,
                              NULL, wc.hInstance, NULL);
  if (!hwnd)
    return fail_hr("create-window", HRESULT_FROM_WIN32(GetLastError()));
  if (demo) {
    ShowWindow(hwnd, SW_SHOWNORMAL);
    UpdateWindow(hwnd);
  }

  DXGI_SWAP_CHAIN_DESC scd = {0};
  scd.BufferDesc.Width = 320;
  scd.BufferDesc.Height = 240;
  scd.BufferDesc.Format = DXGI_FORMAT_R8G8B8A8_UNORM;
  scd.BufferDesc.RefreshRate.Numerator = 60;
  scd.BufferDesc.RefreshRate.Denominator = 1;
  scd.SampleDesc.Count = 1;
  scd.BufferUsage = DXGI_USAGE_RENDER_TARGET_OUTPUT;
  scd.BufferCount = 2;
  scd.OutputWindow = hwnd;
  scd.Windowed = TRUE;
  scd.SwapEffect = DXGI_SWAP_EFFECT_DISCARD;

  static const D3D_FEATURE_LEVEL levels[] = {
      D3D_FEATURE_LEVEL_11_1,
      D3D_FEATURE_LEVEL_11_0,
      D3D_FEATURE_LEVEL_10_0,
  };
  ID3D11Device *device = NULL;
  ID3D11DeviceContext *context = NULL;
  IDXGISwapChain *swapchain = NULL;
  D3D_FEATURE_LEVEL got_level = 0;
  hr = D3D11CreateDeviceAndSwapChain(
      NULL, D3D_DRIVER_TYPE_HARDWARE, NULL, 0, levels,
      sizeof(levels) / sizeof(levels[0]), D3D11_SDK_VERSION, &scd, &swapchain,
      &device, &got_level, &context);
  if (FAILED(hr)) return fail_hr("create-device-swapchain", hr);
  printf("BV-D3D11-PRESENT-DEVICE feature_level=0x%04x\n", got_level);
  {
    char path[MAX_PATH] = "<not loaded>";
    HMODULE module = GetModuleHandleA("d3d11.dll");
    if (module) GetModuleFileNameA(module, path, sizeof(path));
    printf("BV-D3D11-PRESENT-MODULE d3d11.dll=%s\n", path);
  }

  ID3D11Texture2D *backbuffer = NULL;
  hr = IDXGISwapChain_GetBuffer(swapchain, 0, &IID_ID3D11Texture2D,
                                (void **)&backbuffer);
  if (FAILED(hr)) return fail_hr("get-backbuffer", hr);
  ID3D11RenderTargetView *view = NULL;
  hr = ID3D11Device_CreateRenderTargetView(device, (ID3D11Resource *)backbuffer,
                                           NULL, &view);
  if (FAILED(hr)) return fail_hr("create-rtv", hr);

  HMODULE compiler_module = LoadLibraryA("d3dcompiler_47.dll");
  if (!compiler_module)
    return fail_hr("load-compiler", HRESULT_FROM_WIN32(GetLastError()));
  d3d_compile_fn compile =
      (d3d_compile_fn)GetProcAddress(compiler_module, "D3DCompile");
  if (!compile)
    return fail_hr("find-compiler", HRESULT_FROM_WIN32(GetLastError()));

  ID3D10Blob *vs_blob = NULL, *ps_blob = NULL;
  hr = compile(vs_source, strlen(vs_source), "bv-present-vs", NULL, NULL,
               "main", "vs_4_0", 0, 0, &vs_blob, NULL);
  if (FAILED(hr)) return fail_hr("compile-vs", hr);
  hr = compile(ps_source, strlen(ps_source), "bv-present-ps", NULL, NULL,
               "main", "ps_4_0", 0, 0, &ps_blob, NULL);
  if (FAILED(hr)) return fail_hr("compile-ps", hr);
  ID3D11VertexShader *vs = NULL;
  ID3D11PixelShader *ps = NULL;
  hr = ID3D11Device_CreateVertexShader(
      device, ID3D10Blob_GetBufferPointer(vs_blob),
      ID3D10Blob_GetBufferSize(vs_blob), NULL, &vs);
  if (FAILED(hr)) return fail_hr("create-vs", hr);
  hr = ID3D11Device_CreatePixelShader(
      device, ID3D10Blob_GetBufferPointer(ps_blob),
      ID3D10Blob_GetBufferSize(ps_blob), NULL, &ps);
  if (FAILED(hr)) return fail_hr("create-ps", hr);
  const D3D11_INPUT_ELEMENT_DESC element = {
      "POSITION", 0, DXGI_FORMAT_R32G32_FLOAT, 0, 0,
      D3D11_INPUT_PER_VERTEX_DATA, 0};
  ID3D11InputLayout *layout = NULL;
  hr = ID3D11Device_CreateInputLayout(
      device, &element, 1, ID3D10Blob_GetBufferPointer(vs_blob),
      ID3D10Blob_GetBufferSize(vs_blob), &layout);
  if (FAILED(hr)) return fail_hr("create-layout", hr);
  ID3D11Buffer *vertex_buffer = NULL;
  {
    D3D11_BUFFER_DESC desc = {0};
    D3D11_SUBRESOURCE_DATA data = {0};
    desc.ByteWidth = sizeof(vertices);
    desc.Usage = D3D11_USAGE_IMMUTABLE;
    desc.BindFlags = D3D11_BIND_VERTEX_BUFFER;
    data.pSysMem = vertices;
    hr = ID3D11Device_CreateBuffer(device, &desc, &data, &vertex_buffer);
    if (FAILED(hr)) return fail_hr("create-vertex-buffer", hr);
  }

  ID3D11Texture2D *staging = NULL;
  {
    D3D11_TEXTURE2D_DESC desc;
    ID3D11Texture2D_GetDesc(backbuffer, &desc);
    desc.Usage = D3D11_USAGE_STAGING;
    desc.BindFlags = 0;
    desc.CPUAccessFlags = D3D11_CPU_ACCESS_READ;
    desc.MiscFlags = 0;
    hr = ID3D11Device_CreateTexture2D(device, &desc, NULL, &staging);
  }
  if (FAILED(hr)) return fail_hr("create-staging", hr);

  const FLOAT black[4] = {0, 0, 0, 1};
  D3D11_VIEWPORT viewport = {0, 0, 320, 240, 0.0f, 1.0f};
  UINT stride = sizeof(vertices[0]), offset = 0;
  const int frames = demo ? 900 : 30;
  uint32_t magenta = 0, bad = 0;
  for (int frame = 0; frame < frames; ++frame) {
    ID3D11DeviceContext_OMSetRenderTargets(context, 1, &view, NULL);
    ID3D11DeviceContext_RSSetViewports(context, 1, &viewport);
    ID3D11DeviceContext_IASetPrimitiveTopology(
        context, D3D11_PRIMITIVE_TOPOLOGY_TRIANGLELIST);
    ID3D11DeviceContext_IASetInputLayout(context, layout);
    ID3D11DeviceContext_IASetVertexBuffers(context, 0, 1, &vertex_buffer,
                                           &stride, &offset);
    ID3D11DeviceContext_VSSetShader(context, vs, NULL, 0);
    ID3D11DeviceContext_PSSetShader(context, ps, NULL, 0);
    ID3D11DeviceContext_ClearRenderTargetView(context, view, black);
    ID3D11DeviceContext_Draw(context, 3, 0);

    if (frame == 0) {
      /* Assert the backbuffer content once before the first present. */
      ID3D11DeviceContext_CopyResource(context, (ID3D11Resource *)staging,
                                       (ID3D11Resource *)backbuffer);
      D3D11_MAPPED_SUBRESOURCE mapped;
      hr = ID3D11DeviceContext_Map(context, (ID3D11Resource *)staging, 0,
                                   D3D11_MAP_READ, 0, &mapped);
      if (FAILED(hr)) return fail_hr("map-staging", hr);
      for (uint32_t y = 0; y < 240; ++y) {
        const uint8_t *row =
            (const uint8_t *)mapped.pData + y * mapped.RowPitch;
        for (uint32_t x = 0; x < 320; ++x) {
          const uint8_t *pixel = row + x * 4;
          if (pixel[0] >= 254 && pixel[1] <= 1 && pixel[2] >= 254 &&
              pixel[3] >= 254)
            ++magenta;
          else
            ++bad;
        }
      }
      ID3D11DeviceContext_Unmap(context, (ID3D11Resource *)staging, 0);
      printf("BV-D3D11-PRESENT-BACKBUFFER magenta_pixels=%u bad_pixels=%u\n",
             magenta, bad);
    }

    hr = IDXGISwapChain_Present(swapchain, demo ? 1 : 0, 0);
    if (FAILED(hr)) {
      printf("BV-D3D11-PRESENT-FAIL step=present frame=%d hr=0x%08lx\n", frame,
             (unsigned long)hr);
      return 1;
    }
    if (demo) {
      MSG msg;
      while (PeekMessageA(&msg, NULL, 0, 0, PM_REMOVE)) {
        TranslateMessage(&msg);
        DispatchMessageA(&msg);
      }
      Sleep(15);
    }
  }
  printf("BV-D3D11-PRESENT-FRAMES presented=%d\n", frames);

  hr = ID3D11Device_GetDeviceRemovedReason(device);
  printf("BV-D3D11-PRESENT-RESULT removed_reason=0x%08lx\n",
         (unsigned long)hr);
  if (FAILED(hr)) return fail_hr("device-removed", hr);
  if (magenta < 320 * 240 - 100) {
    puts("BV-D3D11-PRESENT-FAIL step=verify");
    return 1;
  }
  puts("BV-D3D11-PRESENT-PASS");
  return 0;
}
