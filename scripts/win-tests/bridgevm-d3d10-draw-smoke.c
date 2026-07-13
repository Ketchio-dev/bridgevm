#define COBJMACROS
#define WIN32_LEAN_AND_MEAN
#include <windows.h>
#include <d3d10.h>
#include <d3dcompiler.h>
#include <dxgi.h>
#include <stdint.h>
#include <stdio.h>
#include <string.h>

struct vertex {
  float position[2];
};

typedef HRESULT(WINAPI *d3d_compile_fn)(
    LPCVOID, SIZE_T, LPCSTR, const D3D_SHADER_MACRO *, ID3DInclude *, LPCSTR,
    LPCSTR, UINT, UINT, ID3DBlob **, ID3DBlob **);

static int fail_hr(const char *step, HRESULT hr) {
  printf("BV-D3D10-DRAW-FAIL step=%s hr=0x%08lx\n", step,
         (unsigned long)hr);
  return 1;
}

static HRESULT compile_shader(d3d_compile_fn compile, const char *source,
                              const char *entry, const char *profile,
                              ID3DBlob **shader) {
  ID3DBlob *errors = NULL;
  HRESULT hr = compile(source, strlen(source), "bridgevm-smoke", NULL, NULL,
                       entry, profile, 0, 0, shader, &errors);
  if (errors) {
    printf("BV-D3D10-DRAW-COMPILER %.*s\n",
           (int)ID3D10Blob_GetBufferSize(errors),
           (const char *)ID3D10Blob_GetBufferPointer(errors));
    ID3D10Blob_Release(errors);
  }
  return hr;
}

int main(void) {
  static const char vs_source[] =
      "float4 main(float2 position : POSITION) : SV_POSITION {"
      "  return float4(position, 0.0, 1.0);"
      "}";
  static const char ps_source[] =
      "float4 main() : SV_TARGET { return float4(1.0, 0.0, 1.0, 1.0); }";
  static const struct vertex vertices[3] = {
      {{-1.0f, -1.0f}}, {{-1.0f, 3.0f}}, {{3.0f, -1.0f}}};
  ID3D10Device *device = NULL;
  ID3D10Texture2D *target = NULL, *staging = NULL;
  ID3D10RenderTargetView *view = NULL;
  ID3D10VertexShader *vs = NULL;
  ID3D10PixelShader *ps = NULL;
  ID3D10InputLayout *layout = NULL;
  ID3D10Buffer *vertex_buffer = NULL;
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
  if (!compiler_module) return fail_hr("load-compiler", HRESULT_FROM_WIN32(GetLastError()));
  compile = (d3d_compile_fn)GetProcAddress(compiler_module, "D3DCompile");
  if (!compile) return fail_hr("find-compiler", HRESULT_FROM_WIN32(GetLastError()));

  texture_desc.Width = 64;
  texture_desc.Height = 64;
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
  }
  if (FAILED(hr)) return fail_hr("create-layout", hr);

  {
    D3D10_BUFFER_DESC desc = {0};
    D3D10_SUBRESOURCE_DATA data = {0};
    desc.ByteWidth = sizeof(vertices);
    desc.Usage = D3D10_USAGE_IMMUTABLE;
    desc.BindFlags = D3D10_BIND_VERTEX_BUFFER;
    data.pSysMem = vertices;
    hr = ID3D10Device_CreateBuffer(device, &desc, &data, &vertex_buffer);
  }
  if (FAILED(hr)) return fail_hr("create-vertex-buffer", hr);

  {
    const FLOAT black[4] = {0, 0, 0, 1};
    D3D10_VIEWPORT viewport = {0, 0, 64, 64, 0.0f, 1.0f};
    UINT stride = sizeof(struct vertex), offset = 0;
    ID3D10Device_OMSetRenderTargets(device, 1, &view, NULL);
    ID3D10Device_RSSetViewports(device, 1, &viewport);
    ID3D10Device_IASetInputLayout(device, layout);
    ID3D10Device_IASetPrimitiveTopology(device,
                                        D3D10_PRIMITIVE_TOPOLOGY_TRIANGLELIST);
    ID3D10Device_IASetVertexBuffers(device, 0, 1, &vertex_buffer, &stride,
                                    &offset);
    ID3D10Device_VSSetShader(device, vs);
    ID3D10Device_PSSetShader(device, ps);
    ID3D10Device_ClearRenderTargetView(device, view, black);
    ID3D10Device_Draw(device, 3, 0);
  }

  ID3D10Device_CopyResource(device, (ID3D10Resource *)staging,
                            (ID3D10Resource *)target);
  ID3D10Device_Flush(device);
  hr = ID3D10Texture2D_Map(staging, 0, D3D10_MAP_READ, 0, &mapped);
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
      if (pixel[0] >= 254 && pixel[1] <= 1 &&
          pixel[2] >= 254 && pixel[3] >= 254)
        ++magenta;
      else
        ++bad;
    }
  }
  ID3D10Texture2D_Unmap(staging, 0);
  hr = ID3D10Device_GetDeviceRemovedReason(device);
  printf("BV-D3D10-DRAW-RESULT center=%02x%02x%02x%02x "
         "magenta_pixels=%u bad_pixels=%u removed_reason=0x%08lx\n",
         center[0], center[1], center[2], center[3], magenta, bad,
         (unsigned long)hr);

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

  if (magenta < 4000 || bad > 96) {
    puts("BV-D3D10-DRAW-FAIL step=verify");
    return 1;
  }
  puts("BV-D3D10-DRAW-PASS");
  return 0;
}
