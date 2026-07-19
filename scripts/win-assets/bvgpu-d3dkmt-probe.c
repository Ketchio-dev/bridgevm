#define WIN32_LEAN_AND_MEAN
#include <stdio.h>
#include <stdint.h>
#include <windows.h>

#define BV_NT_SUCCESS(status) ((LONG)(status) >= 0)
#define BV_VIOGPU_IAM UINT64_C(0x56696f475055)
#define BV_MAX_ADAPTERS 64

typedef UINT BV_D3DKMT_HANDLE;

typedef struct {
    HDC hDc;
    BV_D3DKMT_HANDLE hAdapter;
    LUID AdapterLuid;
    UINT VidPnSourceId;
} BV_D3DKMT_OPENADAPTERFROMHDC;

typedef struct {
    BV_D3DKMT_HANDLE hAdapter;
    LUID AdapterLuid;
    ULONG NumOfSources;
    BOOL bPrecisePresentRegionsPreferred;
} BV_D3DKMT_ADAPTERINFO;

typedef struct {
    ULONG NumAdapters;
    BV_D3DKMT_ADAPTERINFO *pAdapters;
} BV_D3DKMT_ENUMADAPTERS2;

typedef struct {
    BV_D3DKMT_HANDLE hAdapter;
    UINT Type;
    void *pPrivateDriverData;
    UINT PrivateDriverDataSize;
} BV_D3DKMT_QUERYADAPTERINFO;

typedef union {
    BV_D3DKMT_HANDLE hAdapter;
    void *pAdapter;
} BV_D3DKMT_CREATEDEVICE_ADAPTER;

typedef struct {
    BV_D3DKMT_CREATEDEVICE_ADAPTER Adapter;
    UINT Flags;
    BV_D3DKMT_HANDLE hDevice;
    void *pCommandBuffer;
    UINT CommandBufferSize;
    void *pAllocationList;
    UINT AllocationListSize;
    void *pPatchLocationList;
    UINT PatchLocationListSize;
} BV_D3DKMT_CREATEDEVICE;

typedef struct {
    BV_D3DKMT_HANDLE hDevice;
} BV_D3DKMT_DESTROYDEVICE;

typedef struct {
    BV_D3DKMT_HANDLE hAdapter;
} BV_D3DKMT_CLOSEADAPTER;

typedef struct {
    uint64_t IamVioGPU;
    uint32_t Flags;
    uint64_t SupportedCapsetIDs;
} BV_VIOGPU_ADAPTERINFO;

typedef LONG(WINAPI *BV_OPEN_FROM_HDC)(BV_D3DKMT_OPENADAPTERFROMHDC *);
typedef LONG(WINAPI *BV_ENUM_ADAPTERS2)(BV_D3DKMT_ENUMADAPTERS2 *);
typedef LONG(WINAPI *BV_QUERY_ADAPTER_INFO)(BV_D3DKMT_QUERYADAPTERINFO *);
typedef LONG(WINAPI *BV_CREATE_DEVICE)(BV_D3DKMT_CREATEDEVICE *);
typedef LONG(WINAPI *BV_DESTROY_DEVICE)(BV_D3DKMT_DESTROYDEVICE *);
typedef LONG(WINAPI *BV_CLOSE_ADAPTER)(BV_D3DKMT_CLOSEADAPTER *);

static FILE *g_log;

static void log_bytes(const char *source, const char *label, const uint8_t *data, UINT size) {
    UINT shown = size < 64 ? size : 64;
    fprintf(g_log, "[d3dkmt-probe] %s %s first_%u_bytes=", source, label, shown);
    for (UINT i = 0; i < shown; i++) {
        fprintf(g_log, "%02x", data[i]);
    }
    fputc('\n', g_log);
}

static void log_private_info(
    const char *source,
    BV_D3DKMT_HANDLE adapter,
    BV_QUERY_ADAPTER_INFO query_adapter_info) {
    BV_VIOGPU_ADAPTERINFO info;
    ZeroMemory(&info, sizeof(info));
    BV_D3DKMT_QUERYADAPTERINFO query;
    ZeroMemory(&query, sizeof(query));
    query.hAdapter = adapter;
    query.Type = 0; /* KMTQAITYPE_UMDRIVERPRIVATE */
    query.pPrivateDriverData = &info;
    query.PrivateDriverDataSize = sizeof(info);
    LONG status = query_adapter_info(&query);
    fprintf(
        g_log,
        "[d3dkmt-probe] %s query_private status=0x%08lx size=%u iam=0x%016llx "
        "flags=0x%08x supports3d=%u capset_fix=%u blob=%u host_visible=%u "
        "uuid=%u context_init=%u capset_ids=0x%016llx\n",
        source,
        (unsigned long)status,
        query.PrivateDriverDataSize,
        (unsigned long long)info.IamVioGPU,
        info.Flags,
        (info.Flags >> 0) & 1,
        (info.Flags >> 1) & 1,
        (info.Flags >> 2) & 1,
        (info.Flags >> 3) & 1,
        (info.Flags >> 4) & 1,
        (info.Flags >> 5) & 1,
        (unsigned long long)info.SupportedCapsetIDs);
    if (!BV_NT_SUCCESS(status)) {
        uint64_t large_private_data[512];
        ZeroMemory(large_private_data, sizeof(large_private_data));
        query.pPrivateDriverData = large_private_data;
        query.PrivateDriverDataSize = sizeof(large_private_data);
        LONG large_status = query_adapter_info(&query);
        fprintf(
            g_log,
            "[d3dkmt-probe] %s query_private_large status=0x%08lx size=%u\n",
            source,
            (unsigned long)large_status,
            query.PrivateDriverDataSize);
        log_bytes(
            source,
            "query_private_large",
            (const uint8_t *)large_private_data,
            query.PrivateDriverDataSize);
        if (BV_NT_SUCCESS(large_status)) {
            const BV_VIOGPU_ADAPTERINFO *large_info =
                (const BV_VIOGPU_ADAPTERINFO *)large_private_data;
            fprintf(
                g_log,
                "[d3dkmt-probe] %s query_private_large_decoded iam=0x%016llx "
                "flags=0x%08x capset_ids=0x%016llx\n",
                source,
                (unsigned long long)large_info->IamVioGPU,
                large_info->Flags,
                (unsigned long long)large_info->SupportedCapsetIDs);
        }
    }
}

static void log_adapter_type(
    const char *source,
    BV_D3DKMT_HANDLE adapter,
    UINT type,
    BV_QUERY_ADAPTER_INFO query_adapter_info) {
    uint32_t value = 0;
    BV_D3DKMT_QUERYADAPTERINFO query;
    ZeroMemory(&query, sizeof(query));
    query.hAdapter = adapter;
    query.Type = type;
    query.pPrivateDriverData = &value;
    query.PrivateDriverDataSize = sizeof(value);
    LONG status = query_adapter_info(&query);
    fprintf(
        g_log,
        "[d3dkmt-probe] %s query_type_%u status=0x%08lx size=%u value=0x%08x "
        "render=%u display=%u software=%u post=%u paravirtualized=%u compute_only=%u\n",
        source,
        type,
        (unsigned long)status,
        query.PrivateDriverDataSize,
        value,
        (value >> 0) & 1,
        (value >> 1) & 1,
        (value >> 2) & 1,
        (value >> 3) & 1,
        (value >> 7) & 1,
        (value >> 11) & 1);
}

static void log_create_device(
    const char *source,
    BV_D3DKMT_HANDLE adapter,
    BV_CREATE_DEVICE create_device,
    BV_DESTROY_DEVICE destroy_device) {
    BV_D3DKMT_CREATEDEVICE create;
    ZeroMemory(&create, sizeof(create));
    create.Adapter.hAdapter = adapter;
    LONG status = create_device(&create);
    fprintf(
        g_log,
        "[d3dkmt-probe] %s create_device status=0x%08lx device=0x%08x "
        "command_buffer=%p command_bytes=%u allocations=%u patches=%u\n",
        source,
        (unsigned long)status,
        create.hDevice,
        create.pCommandBuffer,
        create.CommandBufferSize,
        create.AllocationListSize,
        create.PatchLocationListSize);
    if (BV_NT_SUCCESS(status) && create.hDevice != 0) {
        BV_D3DKMT_DESTROYDEVICE destroy;
        destroy.hDevice = create.hDevice;
        LONG destroy_status = destroy_device(&destroy);
        fprintf(
            g_log,
            "[d3dkmt-probe] %s destroy_device status=0x%08lx\n",
            source,
            (unsigned long)destroy_status);
    }
}

int main(void) {
    g_log = fopen("C:\\BridgeVM\\bvgpu-d3dkmt-probe.log", "wb");
    if (g_log == NULL) {
        return 2;
    }
    setvbuf(g_log, NULL, _IONBF, 0);
    fprintf(
        g_log,
        "[d3dkmt-probe] struct_sizes open_hdc=%u adapter=%u enum2=%u query=%u "
        "create=%u private=%u\n",
        (unsigned int)sizeof(BV_D3DKMT_OPENADAPTERFROMHDC),
        (unsigned int)sizeof(BV_D3DKMT_ADAPTERINFO),
        (unsigned int)sizeof(BV_D3DKMT_ENUMADAPTERS2),
        (unsigned int)sizeof(BV_D3DKMT_QUERYADAPTERINFO),
        (unsigned int)sizeof(BV_D3DKMT_CREATEDEVICE),
        (unsigned int)sizeof(BV_VIOGPU_ADAPTERINFO));

    HMODULE gdi32 = LoadLibraryW(L"gdi32.dll");
    fprintf(g_log, "[d3dkmt-probe] load_gdi32 module=%p error=%lu\n", gdi32, GetLastError());
    if (gdi32 == NULL) {
        fclose(g_log);
        return 3;
    }

    BV_OPEN_FROM_HDC open_from_hdc =
        (BV_OPEN_FROM_HDC)GetProcAddress(gdi32, "D3DKMTOpenAdapterFromHdc");
    BV_ENUM_ADAPTERS2 enum_adapters2 =
        (BV_ENUM_ADAPTERS2)GetProcAddress(gdi32, "D3DKMTEnumAdapters2");
    BV_QUERY_ADAPTER_INFO query_adapter_info =
        (BV_QUERY_ADAPTER_INFO)GetProcAddress(gdi32, "D3DKMTQueryAdapterInfo");
    BV_CREATE_DEVICE create_device =
        (BV_CREATE_DEVICE)GetProcAddress(gdi32, "D3DKMTCreateDevice");
    BV_DESTROY_DEVICE destroy_device =
        (BV_DESTROY_DEVICE)GetProcAddress(gdi32, "D3DKMTDestroyDevice");
    BV_CLOSE_ADAPTER close_adapter =
        (BV_CLOSE_ADAPTER)GetProcAddress(gdi32, "D3DKMTCloseAdapter");
    fprintf(
        g_log,
        "[d3dkmt-probe] entrypoints open_hdc=%p enum2=%p query=%p create=%p "
        "destroy=%p close=%p\n",
        open_from_hdc,
        enum_adapters2,
        query_adapter_info,
        create_device,
        destroy_device,
        close_adapter);
    if (open_from_hdc == NULL || enum_adapters2 == NULL ||
        query_adapter_info == NULL || create_device == NULL ||
        destroy_device == NULL || close_adapter == NULL) {
        FreeLibrary(gdi32);
        fclose(g_log);
        return 4;
    }

    HDC primary_hdc = GetDC(NULL);
    fprintf(
        g_log,
        "[d3dkmt-probe] get_primary_hdc hdc=%p error=%lu\n",
        primary_hdc,
        GetLastError());
    if (primary_hdc != NULL) {
        BV_D3DKMT_OPENADAPTERFROMHDC open;
        ZeroMemory(&open, sizeof(open));
        open.hDc = primary_hdc;
        LONG status = open_from_hdc(&open);
        ReleaseDC(NULL, primary_hdc);
        fprintf(
            g_log,
            "[d3dkmt-probe] open_primary_hdc status=0x%08lx adapter=0x%08x "
            "luid=%08lx:%08lx source=%u\n",
            (unsigned long)status,
            open.hAdapter,
            (unsigned long)open.AdapterLuid.HighPart,
            (unsigned long)open.AdapterLuid.LowPart,
            open.VidPnSourceId);
        if (BV_NT_SUCCESS(status)) {
            log_private_info("primary_hdc", open.hAdapter, query_adapter_info);
            log_adapter_type("primary_hdc", open.hAdapter, 15, query_adapter_info);
            log_adapter_type("primary_hdc", open.hAdapter, 57, query_adapter_info);
            log_create_device("primary_hdc", open.hAdapter, create_device, destroy_device);
            BV_D3DKMT_CLOSEADAPTER close;
            close.hAdapter = open.hAdapter;
            fprintf(
                g_log,
                "[d3dkmt-probe] primary_hdc close status=0x%08lx\n",
                (unsigned long)close_adapter(&close));
        }
    }

    for (DWORD index = 0; index < 16; index++) {
        DISPLAY_DEVICEW display;
        ZeroMemory(&display, sizeof(display));
        display.cb = sizeof(display);
        if (!EnumDisplayDevicesW(NULL, index, &display, 0)) {
            fprintf(
                g_log,
                "[d3dkmt-probe] enum_display_end index=%lu error=%lu\n",
                (unsigned long)index,
                GetLastError());
            break;
        }
        fprintf(
            g_log,
            "[d3dkmt-probe] display index=%lu name=%ls string=%ls flags=0x%08lx id=%ls\n",
            (unsigned long)index,
            display.DeviceName,
            display.DeviceString,
            (unsigned long)display.StateFlags,
            display.DeviceID);
    }

    BV_D3DKMT_ADAPTERINFO adapters[BV_MAX_ADAPTERS];
    ZeroMemory(adapters, sizeof(adapters));
    BV_D3DKMT_ENUMADAPTERS2 enumeration;
    enumeration.NumAdapters = BV_MAX_ADAPTERS;
    enumeration.pAdapters = adapters;
    LONG enum_status = enum_adapters2(&enumeration);
    fprintf(
        g_log,
        "[d3dkmt-probe] enum_adapters2 status=0x%08lx count=%lu\n",
        (unsigned long)enum_status,
        (unsigned long)enumeration.NumAdapters);
    if (BV_NT_SUCCESS(enum_status)) {
        ULONG count = enumeration.NumAdapters;
        if (count > BV_MAX_ADAPTERS) {
            count = BV_MAX_ADAPTERS;
        }
        for (ULONG index = 0; index < count; index++) {
            char source[48];
            snprintf(source, sizeof(source), "enum_adapter_%lu", (unsigned long)index);
            fprintf(
                g_log,
                "[d3dkmt-probe] %s handle=0x%08x luid=%08lx:%08lx sources=%lu "
                "precise=%ld\n",
                source,
                adapters[index].hAdapter,
                (unsigned long)adapters[index].AdapterLuid.HighPart,
                (unsigned long)adapters[index].AdapterLuid.LowPart,
                (unsigned long)adapters[index].NumOfSources,
                (long)adapters[index].bPrecisePresentRegionsPreferred);
            log_private_info(source, adapters[index].hAdapter, query_adapter_info);
            log_adapter_type(source, adapters[index].hAdapter, 15, query_adapter_info);
            log_adapter_type(source, adapters[index].hAdapter, 57, query_adapter_info);
            log_create_device(source, adapters[index].hAdapter, create_device, destroy_device);
            BV_D3DKMT_CLOSEADAPTER close;
            close.hAdapter = adapters[index].hAdapter;
            fprintf(
                g_log,
                "[d3dkmt-probe] %s close status=0x%08lx\n",
                source,
                (unsigned long)close_adapter(&close));
        }
    }

    FreeLibrary(gdi32);
    fclose(g_log);
    return 0;
}
