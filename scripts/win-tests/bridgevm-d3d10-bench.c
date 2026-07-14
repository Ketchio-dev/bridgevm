#define COBJMACROS
#define WIN32_LEAN_AND_MEAN
#include <windows.h>
#include <d3d10.h>
#include <d3dcompiler.h>
#include <dxgi.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

/* Offscreen, present-free D3D10 throughput benchmark.
 *
 * WinSAT's D3D assessments report a flat vsync/present-coupled figure on
 * BridgeVM (every subtest 42.00 F/s), so they measure the presentation
 * cadence rather than the renderer. This benchmark renders to an offscreen
 * 1280x800 target with no swapchain: per frame it clears, then issues
 * BV_BENCH_DRAWS instanced draws (BV_BENCH_INSTANCES quads each) with a
 * per-draw constant-buffer update, flushes, and after the timed frames
 * fences GPU completion with a staging copy + map before reading the clock.
 * The per-draw UpdateSubresource deliberately exercises the guest->host
 * buffer-upload path (the one the first-draw fix added a host flush to), so
 * regressions there show up directly in draws/s. */

struct vertex {
  float position[2];
};

struct params {
  float xform[4];
  float tint[4];
};

typedef HRESULT(WINAPI *d3d_compile_fn)(
    LPCVOID, SIZE_T, LPCSTR, const D3D_SHADER_MACRO *, ID3DInclude *, LPCSTR,
    LPCSTR, UINT, UINT, ID3DBlob **, ID3DBlob **);

static int fail_hr(const char *step, HRESULT hr) {
  printf("BV-D3D10-BENCH-FAIL step=%s hr=0x%08lx\n", step, (unsigned long)hr);
  return 1;
}

static HRESULT compile_shader(d3d_compile_fn compile, const char *source,
                              const char *entry, const char *profile,
                              ID3DBlob **shader) {
  ID3DBlob *errors = NULL;
  HRESULT hr = compile(source, strlen(source), "bridgevm-bench", NULL, NULL,
                       entry, profile, 0, 0, shader, &errors);
  if (errors) {
    printf("BV-D3D10-BENCH-COMPILER %.*s\n",
           (int)ID3D10Blob_GetBufferSize(errors),
           (const char *)ID3D10Blob_GetBufferPointer(errors));
    ID3D10Blob_Release(errors);
  }
  return hr;
}

static int env_int(const char *name, int fallback, int lo, int hi) {
  char buf[16] = {0};
  if (GetEnvironmentVariableA(name, buf, sizeof(buf))) {
    int parsed = atoi(buf);
    if (parsed >= lo && parsed <= hi) return parsed;
  }
  return fallback;
}

int main(void) {
  const int frames = env_int("BV_BENCH_FRAMES", 300, 10, 100000);
  const int warmup = env_int("BV_BENCH_WARMUP", 30, 0, 10000);
  const int draws = env_int("BV_BENCH_DRAWS", 100, 1, 10000);
  const int instances = env_int("BV_BENCH_INSTANCES", 64, 1, 65536);
  const int width = 1280, height = 800;

  static const char vs_source[] =
      "cbuffer Params : register(b0) { float4 xform; float4 tint; }"
      "struct VSOut { float4 pos : SV_POSITION; float2 uv : TEXCOORD0; };"
      "VSOut main(float2 pos : POSITION) {"
      "  VSOut o;"
      "  o.pos = float4(pos * xform.zw + xform.xy, 0.0, 1.0);"
      "  o.uv = pos;"
      "  return o;"
      "}";
  static const char ps_source[] =
      "cbuffer Params : register(b0) { float4 xform; float4 tint; }"
      "float4 main(float4 sp : SV_POSITION, float2 uv : TEXCOORD0) : SV_TARGET {"
      "  float acc = 0.0;"
      "  [unroll] for (int i = 0; i < 8; ++i)"
      "    acc = acc * 1.0009 + sin(uv.x * (i + 1)) * cos(uv.y * (i + 2));"
      "  return float4(saturate(tint.rgb * (0.5 + 0.5 * frac(acc))), 1.0);"
      "}";
  static const struct vertex vertices[4] = {
      {{-1.0f, -1.0f}}, {{-1.0f, 1.0f}}, {{1.0f, -1.0f}}, {{1.0f, 1.0f}}};

  ID3D10Device *device = NULL;
  ID3D10Texture2D *target = NULL, *staging = NULL;
  ID3D10RenderTargetView *view = NULL;
  ID3D10VertexShader *vs = NULL;
  ID3D10PixelShader *ps = NULL;
  ID3D10InputLayout *layout = NULL;
  ID3D10Buffer *vertex_buffer = NULL, *cbuffer = NULL;
  ID3DBlob *vs_blob = NULL, *ps_blob = NULL;
  HMODULE compiler_module = NULL;
  d3d_compile_fn compile = NULL;
  D3D10_TEXTURE2D_DESC texture_desc = {0};
  D3D10_MAPPED_TEXTURE2D mapped;
  HRESULT hr;

  hr = D3D10CreateDevice(NULL, D3D10_DRIVER_TYPE_HARDWARE, NULL, 0,
                         D3D10_SDK_VERSION, &device);
  if (FAILED(hr)) return fail_hr("create-device", hr);

  compiler_module = LoadLibraryA("d3dcompiler_47.dll");
  if (!compiler_module)
    return fail_hr("load-compiler", HRESULT_FROM_WIN32(GetLastError()));
  compile = (d3d_compile_fn)GetProcAddress(compiler_module, "D3DCompile");
  if (!compile)
    return fail_hr("find-compiler", HRESULT_FROM_WIN32(GetLastError()));

  texture_desc.Width = width;
  texture_desc.Height = height;
  texture_desc.MipLevels = 1;
  texture_desc.ArraySize = 1;
  texture_desc.Format = DXGI_FORMAT_R8G8B8A8_UNORM;
  texture_desc.SampleDesc.Count = 1;
  texture_desc.Usage = D3D10_USAGE_DEFAULT;
  texture_desc.BindFlags = D3D10_BIND_RENDER_TARGET;
  hr = ID3D10Device_CreateTexture2D(device, &texture_desc, NULL, &target);
  if (FAILED(hr)) return fail_hr("create-target", hr);
  hr = ID3D10Device_CreateRenderTargetView(device, (ID3D10Resource *)target,
                                           NULL, &view);
  if (FAILED(hr)) return fail_hr("create-rtv", hr);

  {
    D3D10_TEXTURE2D_DESC staging_desc = texture_desc;
    staging_desc.Usage = D3D10_USAGE_STAGING;
    staging_desc.BindFlags = 0;
    staging_desc.CPUAccessFlags = D3D10_CPU_ACCESS_READ;
    hr = ID3D10Device_CreateTexture2D(device, &staging_desc, NULL, &staging);
  }
  if (FAILED(hr)) return fail_hr("create-staging", hr);

  hr = compile_shader(compile, vs_source, "main", "vs_4_0", &vs_blob);
  if (FAILED(hr)) return fail_hr("compile-vs", hr);
  hr = compile_shader(compile, ps_source, "main", "ps_4_0", &ps_blob);
  if (FAILED(hr)) return fail_hr("compile-ps", hr);
  hr = ID3D10Device_CreateVertexShader(
      device, ID3D10Blob_GetBufferPointer(vs_blob),
      ID3D10Blob_GetBufferSize(vs_blob), &vs);
  if (FAILED(hr)) return fail_hr("create-vs", hr);
  hr = ID3D10Device_CreatePixelShader(
      device, ID3D10Blob_GetBufferPointer(ps_blob),
      ID3D10Blob_GetBufferSize(ps_blob), &ps);
  if (FAILED(hr)) return fail_hr("create-ps", hr);

  {
    const D3D10_INPUT_ELEMENT_DESC element = {
        "POSITION", 0, DXGI_FORMAT_R32G32_FLOAT, 0, 0,
        D3D10_INPUT_PER_VERTEX_DATA, 0};
    hr = ID3D10Device_CreateInputLayout(
        device, &element, 1, ID3D10Blob_GetBufferPointer(vs_blob),
        ID3D10Blob_GetBufferSize(vs_blob), &layout);
    if (FAILED(hr)) return fail_hr("create-layout", hr);
  }

  {
    D3D10_BUFFER_DESC desc = {0};
    D3D10_SUBRESOURCE_DATA data = {0};
    desc.ByteWidth = sizeof(vertices);
    desc.Usage = D3D10_USAGE_IMMUTABLE;
    desc.BindFlags = D3D10_BIND_VERTEX_BUFFER;
    data.pSysMem = vertices;
    hr = ID3D10Device_CreateBuffer(device, &desc, &data, &vertex_buffer);
    if (FAILED(hr)) return fail_hr("create-vertex-buffer", hr);
  }

  {
    D3D10_BUFFER_DESC desc = {0};
    desc.ByteWidth = sizeof(struct params);
    desc.Usage = D3D10_USAGE_DEFAULT;
    desc.BindFlags = D3D10_BIND_CONSTANT_BUFFER;
    hr = ID3D10Device_CreateBuffer(device, &desc, NULL, &cbuffer);
    if (FAILED(hr)) return fail_hr("create-cbuffer", hr);
  }

  {
    const FLOAT black[4] = {0, 0, 0, 1};
    D3D10_VIEWPORT viewport = {0, 0, width, height, 0.0f, 1.0f};
    UINT stride = sizeof(struct vertex), offset = 0;
    ID3D10Device_OMSetRenderTargets(device, 1, &view, NULL);
    ID3D10Device_RSSetViewports(device, 1, &viewport);
    ID3D10Device_IASetPrimitiveTopology(
        device, D3D10_PRIMITIVE_TOPOLOGY_TRIANGLESTRIP);
    ID3D10Device_IASetInputLayout(device, layout);
    ID3D10Device_IASetVertexBuffers(device, 0, 1, &vertex_buffer, &stride,
                                    &offset);
    ID3D10Device_VSSetShader(device, vs);
    ID3D10Device_PSSetShader(device, ps);
    ID3D10Device_VSSetConstantBuffers(device, 0, 1, &cbuffer);
    ID3D10Device_PSSetConstantBuffers(device, 0, 1, &cbuffer);

    LARGE_INTEGER freq, t0, t1;
    QueryPerformanceFrequency(&freq);
    int frame;
    struct params p = {{0}};
    ID3D10Query *event_query = NULL;
    {
      D3D10_QUERY_DESC query_desc = {D3D10_QUERY_EVENT, 0};
      hr = ID3D10Device_CreateQuery(device, &query_desc, &event_query);
      if (FAILED(hr)) return fail_hr("create-event-query", hr);
    }

/* Map alone does not reliably block on GPU completion on this stack; an
 * explicit EVENT query poll is the proven fence (same as the draw smoke). */
#define BV_GPU_FENCE(step)                                                    \
  do {                                                                        \
    BOOL gpu_done = FALSE;                                                    \
    DWORD start_ms = GetTickCount();                                          \
    HRESULT query_hr = S_FALSE;                                               \
    ID3D10Asynchronous_End((ID3D10Asynchronous *)event_query);                \
    ID3D10Device_Flush(device);                                               \
    while ((GetTickCount() - start_ms) < 30000) {                             \
      query_hr = ID3D10Asynchronous_GetData(                                  \
          (ID3D10Asynchronous *)event_query, &gpu_done, sizeof(gpu_done), 0); \
      if (query_hr == S_OK && gpu_done) break;                                \
      if (FAILED(query_hr)) break;                                            \
      Sleep(1);                                                               \
    }                                                                         \
    if (query_hr != S_OK || !gpu_done)                                        \
      return fail_hr(step, query_hr);                                         \
  } while (0)

    for (frame = 0; frame < warmup + frames; ++frame) {
      if (frame == warmup) {
        /* Drain warmup work so the timed window measures steady state. */
        BV_GPU_FENCE("fence-warmup");
        QueryPerformanceCounter(&t0);
      }
      ID3D10Device_ClearRenderTargetView(device, view, black);
      for (int d = 0; d < draws; ++d) {
        p.xform[0] = 0.15f * (float)(d % 7) - 0.45f;
        p.xform[1] = 0.15f * (float)(d % 5) - 0.30f;
        p.xform[2] = 0.5f;
        p.xform[3] = 0.5f;
        p.tint[0] = 0.25f + 0.75f * (float)(d % 3) / 2.0f;
        p.tint[1] = 0.25f + 0.75f * (float)(d % 4) / 3.0f;
        p.tint[2] = 0.25f + 0.75f * (float)(d % 5) / 4.0f;
        ID3D10Device_UpdateSubresource(device, (ID3D10Resource *)cbuffer, 0,
                                       NULL, &p, 0, 0);
        ID3D10Device_DrawInstanced(device, 4, (UINT)instances, 0, 0);
      }
      ID3D10Device_Flush(device);
    }

    /* Fence every timed frame, then copy out and fence the copy. */
    BV_GPU_FENCE("fence-frames");
    QueryPerformanceCounter(&t1);
    ID3D10Device_CopyResource(device, (ID3D10Resource *)staging,
                              (ID3D10Resource *)target);
    BV_GPU_FENCE("fence-copy");
    hr = ID3D10Texture2D_Map(staging, 0, D3D10_MAP_READ, 0, &mapped);
    if (FAILED(hr)) return fail_hr("map-staging", hr);

    const uint8_t *center = (const uint8_t *)mapped.pData +
                            (height / 2) * mapped.RowPitch + (width / 2) * 4;
    uint8_t center_px[4] = {center[0], center[1], center[2], center[3]};
    ID3D10Texture2D_Unmap(staging, 0);

    double elapsed = (double)(t1.QuadPart - t0.QuadPart) / (double)freq.QuadPart;
    double fps = (double)frames / elapsed;
    double dps = fps * (double)draws;
    hr = ID3D10Device_GetDeviceRemovedReason(device);
    printf("BV-D3D10-BENCH frames=%d draws=%d instances=%d elapsed_ms=%.1f "
           "fps=%.2f draws_per_s=%.0f center=%02x%02x%02x%02x "
           "removed_reason=0x%08lx\n",
           frames, draws, instances, elapsed * 1000.0, fps, dps, center_px[0],
           center_px[1], center_px[2], center_px[3], (unsigned long)hr);

    if (center_px[0] == 0 && center_px[1] == 0 && center_px[2] == 0) {
      puts("BV-D3D10-BENCH-FAIL step=verify (center black)");
      return 1;
    }
    ID3D10Query_Release(event_query);
  }

  ID3D10Buffer_Release(cbuffer);
  ID3D10Buffer_Release(vertex_buffer);
  ID3D10InputLayout_Release(layout);
  ID3D10PixelShader_Release(ps);
  ID3D10VertexShader_Release(vs);
  ID3D10Blob_Release(ps_blob);
  ID3D10Blob_Release(vs_blob);
  ID3D10RenderTargetView_Release(view);
  ID3D10Texture2D_Release(staging);
  ID3D10Texture2D_Release(target);
  ID3D10Device_Release(device);
  FreeLibrary(compiler_module);

  puts("BV-D3D10-BENCH-PASS");
  return 0;
}
