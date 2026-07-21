/* BridgeVM guest D3D11 draw smoke for the DXVK rung.
 *
 * Renders a fullscreen magenta triangle offscreen through whatever
 * d3d11.dll/dxgi.dll the loader resolves — place the ARM64 DXVK DLLs next to
 * this exe to route D3D11 through DXVK onto the Venus Vulkan ICD — then reads
 * the pixels back and asserts them, mirroring bridgevm-d3d10-draw-smoke.
 * The loaded d3d11/dxgi module paths are printed so a run proves whether
 * DXVK or the inbox implementation served the API.
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
  printf("BV-D3D11-DRAW-FAIL step=%s hr=0x%08lx\n", step, (unsigned long)hr);
  return 1;
}

static void print_module_path(const char *name) {
  char path[MAX_PATH] = "<not loaded>";
  HMODULE module = GetModuleHandleA(name);
  if (module) GetModuleFileNameA(module, path, sizeof(path));
  printf("BV-D3D11-DRAW-MODULE %s=%s\n", name, path);
}

struct vertex {
  float position[2];
};

int main(void) {
  /* Default draws through a real vertex buffer + input layout; BV_DRAW_NOVB=1
   * switches to the SV_VertexID fullscreen triangle with no vertex bindings.
   * On a DXVK built with the nullDescriptor requirement relaxed, comparing the
   * two isolates whether null-binding handling breaks rasterization. */
  const int no_vertex_buffer =
      GetEnvironmentVariableA("BV_DRAW_NOVB", NULL, 0) != 0;
  static const char vs_vb_source[] =
      "float4 main(float2 position : POSITION) : SV_POSITION {"
      "  return float4(position, 0.0, 1.0);"
      "}";
  static const char vs_novb_source[] =
      "float4 main(uint id : SV_VertexID) : SV_POSITION {"
      "  float2 p = float2(float((id << 1) & 2), float(id & 2));"
      "  return float4(p * float2(2.0, 2.0) + float2(-1.0, -1.0), 0.0, 1.0);"
      "}";
  const char *vs_source = no_vertex_buffer ? vs_novb_source : vs_vb_source;
  static const char ps_source[] =
      "float4 main() : SV_TARGET { return float4(1.0, 0.0, 1.0, 1.0); }";
  static const struct vertex vertices[3] = {
      {{-1.0f, -1.0f}}, {{-1.0f, 3.0f}}, {{3.0f, -1.0f}}};

  ID3D11Device *device = NULL;
  ID3D11DeviceContext *context = NULL;
  ID3D11Texture2D *target = NULL, *staging = NULL;
  ID3D11RenderTargetView *view = NULL;
  ID3D11VertexShader *vs = NULL;
  ID3D11PixelShader *ps = NULL;
  ID3D11InputLayout *layout = NULL;
  ID3D11Buffer *vertex_buffer = NULL;
  ID3D10Blob *vs_blob = NULL, *ps_blob = NULL;
  D3D_FEATURE_LEVEL got_level = 0;
  HRESULT hr;

  static const D3D_FEATURE_LEVEL levels[] = {
      D3D_FEATURE_LEVEL_11_1,
      D3D_FEATURE_LEVEL_11_0,
      D3D_FEATURE_LEVEL_10_1,
      D3D_FEATURE_LEVEL_10_0,
  };
  hr = D3D11CreateDevice(NULL, D3D_DRIVER_TYPE_HARDWARE, NULL, 0, levels,
                         sizeof(levels) / sizeof(levels[0]), D3D11_SDK_VERSION,
                         &device, &got_level, &context);
  if (FAILED(hr)) return fail_hr("create-device", hr);
  printf("BV-D3D11-DRAW-DEVICE feature_level=0x%04x mode=%s\n", got_level,
         no_vertex_buffer ? "novb" : "vb");
  print_module_path("d3d11.dll");
  print_module_path("dxgi.dll");

  {
    IDXGIDevice *dxgi_device = NULL;
    if (SUCCEEDED(ID3D11Device_QueryInterface(device, &IID_IDXGIDevice,
                                              (void **)&dxgi_device))) {
      IDXGIAdapter *adapter = NULL;
      if (SUCCEEDED(IDXGIDevice_GetAdapter(dxgi_device, &adapter))) {
        DXGI_ADAPTER_DESC desc;
        if (SUCCEEDED(IDXGIAdapter_GetDesc(adapter, &desc))) {
          printf("BV-D3D11-DRAW-ADAPTER vendor=0x%04x device=0x%04x desc=%ls\n",
                 desc.VendorId, desc.DeviceId, desc.Description);
        }
        IDXGIAdapter_Release(adapter);
      }
      IDXGIDevice_Release(dxgi_device);
    }
  }

  HMODULE compiler_module = LoadLibraryA("d3dcompiler_47.dll");
  if (!compiler_module)
    return fail_hr("load-compiler", HRESULT_FROM_WIN32(GetLastError()));
  d3d_compile_fn compile =
      (d3d_compile_fn)GetProcAddress(compiler_module, "D3DCompile");
  if (!compile)
    return fail_hr("find-compiler", HRESULT_FROM_WIN32(GetLastError()));

  hr = compile(vs_source, strlen(vs_source), "bv-d3d11-vs", NULL, NULL, "main",
               "vs_4_0", 0, 0, &vs_blob, NULL);
  if (FAILED(hr)) return fail_hr("compile-vs", hr);
  hr = compile(ps_source, strlen(ps_source), "bv-d3d11-ps", NULL, NULL, "main",
               "ps_4_0", 0, 0, &ps_blob, NULL);
  if (FAILED(hr)) return fail_hr("compile-ps", hr);
  hr = ID3D11Device_CreateVertexShader(
      device, ID3D10Blob_GetBufferPointer(vs_blob),
      ID3D10Blob_GetBufferSize(vs_blob), NULL, &vs);
  if (FAILED(hr)) return fail_hr("create-vs", hr);
  hr = ID3D11Device_CreatePixelShader(
      device, ID3D10Blob_GetBufferPointer(ps_blob),
      ID3D10Blob_GetBufferSize(ps_blob), NULL, &ps);
  if (FAILED(hr)) return fail_hr("create-ps", hr);

  if (!no_vertex_buffer) {
    const D3D11_INPUT_ELEMENT_DESC element = {
        "POSITION", 0, DXGI_FORMAT_R32G32_FLOAT, 0, 0,
        D3D11_INPUT_PER_VERTEX_DATA, 0};
    hr = ID3D11Device_CreateInputLayout(
        device, &element, 1, ID3D10Blob_GetBufferPointer(vs_blob),
        ID3D10Blob_GetBufferSize(vs_blob), &layout);
    if (FAILED(hr)) return fail_hr("create-layout", hr);
    D3D11_BUFFER_DESC desc = {0};
    D3D11_SUBRESOURCE_DATA data = {0};
    desc.ByteWidth = sizeof(vertices);
    desc.Usage = D3D11_USAGE_IMMUTABLE;
    desc.BindFlags = D3D11_BIND_VERTEX_BUFFER;
    data.pSysMem = vertices;
    hr = ID3D11Device_CreateBuffer(device, &desc, &data, &vertex_buffer);
    if (FAILED(hr)) return fail_hr("create-vertex-buffer", hr);
  }

  D3D11_TEXTURE2D_DESC texture_desc = {0};
  texture_desc.Width = 64;
  texture_desc.Height = 64;
  texture_desc.MipLevels = 1;
  texture_desc.ArraySize = 1;
  texture_desc.Format = DXGI_FORMAT_R8G8B8A8_UNORM;
  texture_desc.SampleDesc.Count = 1;
  texture_desc.Usage = D3D11_USAGE_DEFAULT;
  texture_desc.BindFlags = D3D11_BIND_RENDER_TARGET;
  hr = ID3D11Device_CreateTexture2D(device, &texture_desc, NULL, &target);
  if (FAILED(hr)) return fail_hr("create-target", hr);
  hr = ID3D11Device_CreateRenderTargetView(device, (ID3D11Resource *)target,
                                           NULL, &view);
  if (FAILED(hr)) return fail_hr("create-rtv", hr);
  {
    D3D11_TEXTURE2D_DESC staging_desc = texture_desc;
    staging_desc.Usage = D3D11_USAGE_STAGING;
    staging_desc.BindFlags = 0;
    staging_desc.CPUAccessFlags = D3D11_CPU_ACCESS_READ;
    hr = ID3D11Device_CreateTexture2D(device, &staging_desc, NULL, &staging);
  }
  if (FAILED(hr)) return fail_hr("create-staging", hr);

  const FLOAT black[4] = {0, 0, 0, 1};
  D3D11_VIEWPORT viewport = {0, 0, 64, 64, 0.0f, 1.0f};
  ID3D11DeviceContext_OMSetRenderTargets(context, 1, &view, NULL);
  ID3D11DeviceContext_RSSetViewports(context, 1, &viewport);
  ID3D11DeviceContext_IASetPrimitiveTopology(
      context, D3D11_PRIMITIVE_TOPOLOGY_TRIANGLELIST);
  if (!no_vertex_buffer) {
    UINT stride = sizeof(struct vertex), offset = 0;
    ID3D11DeviceContext_IASetInputLayout(context, layout);
    ID3D11DeviceContext_IASetVertexBuffers(context, 0, 1, &vertex_buffer,
                                           &stride, &offset);
  }
  ID3D11DeviceContext_VSSetShader(context, vs, NULL, 0);
  ID3D11DeviceContext_PSSetShader(context, ps, NULL, 0);
  ID3D11DeviceContext_ClearRenderTargetView(context, view, black);
  ID3D11DeviceContext_Draw(context, 3, 0);
  ID3D11DeviceContext_CopyResource(context, (ID3D11Resource *)staging,
                                   (ID3D11Resource *)target);

  ID3D11Query *event_query = NULL;
  {
    D3D11_QUERY_DESC query_desc = {D3D11_QUERY_EVENT, 0};
    hr = ID3D11Device_CreateQuery(device, &query_desc, &event_query);
  }
  if (FAILED(hr)) return fail_hr("create-event-query", hr);
  ID3D11DeviceContext_End(context, (ID3D11Asynchronous *)event_query);
  ID3D11DeviceContext_Flush(context);
  {
    BOOL gpu_done = FALSE;
    DWORD start_ms = GetTickCount();
    DWORD waited_ms = 0;
    HRESULT query_hr = S_FALSE;
    while (waited_ms < 10000) {
      query_hr = ID3D11DeviceContext_GetData(
          context, (ID3D11Asynchronous *)event_query, &gpu_done,
          sizeof(gpu_done), 0);
      if (query_hr == S_OK && gpu_done) break;
      if (FAILED(query_hr)) break;
      Sleep(50);
      waited_ms = GetTickCount() - start_ms;
    }
    printf("BV-D3D11-DRAW-EVENT hr=0x%08lx done=%d waited_ms=%lu\n",
           (unsigned long)query_hr, gpu_done ? 1 : 0,
           (unsigned long)waited_ms);
  }
  ID3D11Query_Release(event_query);

  D3D11_MAPPED_SUBRESOURCE mapped;
  hr = ID3D11DeviceContext_Map(context, (ID3D11Resource *)staging, 0,
                               D3D11_MAP_READ, 0, &mapped);
  if (FAILED(hr)) return fail_hr("map-staging", hr);

  uint32_t magenta = 0, bad = 0;
  uint8_t center[4] = {0};
  for (uint32_t y = 0; y < 64; ++y) {
    const uint8_t *row = (const uint8_t *)mapped.pData + y * mapped.RowPitch;
    for (uint32_t x = 0; x < 64; ++x) {
      const uint8_t *pixel = row + x * 4;
      if (x == 32 && y == 32) {
        for (uint32_t i = 0; i < 4; ++i) center[i] = pixel[i];
      }
      if (pixel[0] >= 254 && pixel[1] <= 1 && pixel[2] >= 254 &&
          pixel[3] >= 254)
        ++magenta;
      else
        ++bad;
    }
  }
  ID3D11DeviceContext_Unmap(context, (ID3D11Resource *)staging, 0);

  hr = ID3D11Device_GetDeviceRemovedReason(device);
  printf("BV-D3D11-DRAW-RESULT center=%02x%02x%02x%02x magenta_pixels=%u "
         "bad_pixels=%u removed_reason=0x%08lx\n",
         center[0], center[1], center[2], center[3], magenta, bad,
         (unsigned long)hr);

  if (vertex_buffer) ID3D11Buffer_Release(vertex_buffer);
  if (layout) ID3D11InputLayout_Release(layout);
  ID3D11PixelShader_Release(ps);
  ID3D11VertexShader_Release(vs);
  ID3D10Blob_Release(ps_blob);
  ID3D10Blob_Release(vs_blob);
  ID3D11RenderTargetView_Release(view);
  ID3D11Texture2D_Release(staging);
  ID3D11Texture2D_Release(target);
  ID3D11DeviceContext_Release(context);
  ID3D11Device_Release(device);
  FreeLibrary(compiler_module);

  if (magenta < 4000 || bad > 96) {
    puts("BV-D3D11-DRAW-FAIL step=verify");
    return 1;
  }
  puts("BV-D3D11-DRAW-PASS");
  return 0;
}
