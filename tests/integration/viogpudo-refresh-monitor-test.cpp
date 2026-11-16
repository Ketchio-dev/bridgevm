// The actual AddSingleMonitorMode body is included by the source-kit test.
// Only the WDDM allocation/ownership callbacks are replaced here.
#include <cassert>
#include <vector>

using UINT = unsigned int;
using ULONG = unsigned long;
using NTSTATUS = int;
using LONG_PTR = long long;
constexpr NTSTATUS STATUS_SUCCESS = 0;
constexpr NTSTATUS STATUS_NOT_SUPPORTED = -3;
constexpr NTSTATUS STATUS_GRAPHICS_MODE_ALREADY_IN_MODESET = -1;
constexpr NTSTATUS addFailure = -2;
#define NT_SUCCESS(status) ((status) >= 0)
#define PAGED_CODE() ((void)0)
#define _In_
#define CONST const
#define DbgPrint(...) ((void)0)
#define NT_ASSERT(value) assert(value)
#define UNREFERENCED_PARAMETER(value) ((void)(value))
constexpr int D3DKMDT_MCO_DRIVER = 1, D3DKMDT_MP_PREFERRED = 1;
constexpr int D3DKMDT_MP_NOTPREFERRED = 0, D3DKMDT_CB_SRGB = 1;
constexpr UINT NOM_WIDTH_SIZE = 1024, NOM_HEIGHT_SIZE = 768;
static const ULONG g_BridgeRefreshRates[] = {120,30,60,75,90,144,165,180,240,360,480,540,600,720};
constexpr UINT g_BridgeRefreshRateCount = 14;

struct VIDEO_MODE_INFORMATION { UINT VisScreenWidth, VisScreenHeight; ULONG Frequency; };
using PVIDEO_MODE_INFORMATION = VIDEO_MODE_INFORMATION*;
struct Signal { UINT width, height; ULONG hz; };
struct D3DKMDT_MONITOR_SOURCE_MODE {
    Signal VideoSignalInfo{};
    int Origin{}, Preference{}, ColorBasis{};
    struct { int FirstChannel{}, SecondChannel{}, ThirdChannel{}, FourthChannel{}; } ColorCoeffDynamicRanges;
};
struct Modes {
    std::vector<Signal> registered;
    int allocated = 0, releases = 0;
    bool failCustom = false;
};
NTSTATUS create(Modes* set, D3DKMDT_MONITOR_SOURCE_MODE** mode) {
    ++set->allocated;
    *mode = new D3DKMDT_MONITOR_SOURCE_MODE;
    return STATUS_SUCCESS;
}
NTSTATUS add(Modes* set, D3DKMDT_MONITOR_SOURCE_MODE* mode) {
    const auto s = mode->VideoSignalInfo;
    if (set->failCustom && s.width == 3600) return addFailure;
    for (const auto& old : set->registered)
        if (old.width == s.width && old.height == s.height && old.hz == s.hz)
            return STATUS_GRAPHICS_MODE_ALREADY_IN_MODESET;
    set->registered.push_back(s);
    --set->allocated;
    delete mode; // pfnAddMode takes ownership only on success.
    return STATUS_SUCCESS;
}
NTSTATUS release(Modes* set, D3DKMDT_MONITOR_SOURCE_MODE* mode) {
    --set->allocated;
    ++set->releases;
    delete mode;
    return STATUS_SUCCESS;
}
struct MonitorInterface {
    decltype(&create) pfnCreateNewModeInfo = create;
    decltype(&add) pfnAddMode = add;
    decltype(&release) pfnReleaseModeInfo = release;
};
struct DXGKARG_RECOMMENDMONITORMODES {
    Modes* hMonitorSourceModeSet;
    const MonitorInterface* pMonitorSourceModeSetInterface;
    UINT VideoPresentTargetId{};
};
using D3DDDI_VIDEO_PRESENT_TARGET_ID = UINT;
constexpr int DXGK_MONITOR_INTERFACE_VERSION_V1 = 1;
struct MonitorAdapter {
    Modes modes;
    MonitorInterface callbacks;
    int acquired = 0, released = 0;
    NTSTATUS acquireStatus = STATUS_SUCCESS;
};
NTSTATUS acquireMonitor(void* handle, UINT target, Modes** modes, const MonitorInterface** callbacks) {
    assert(target == 0);
    auto* adapter = static_cast<MonitorAdapter*>(handle);
    if (!NT_SUCCESS(adapter->acquireStatus)) return adapter->acquireStatus;
    ++adapter->acquired;
    *modes = &adapter->modes;
    *callbacks = &adapter->callbacks;
    return STATUS_SUCCESS;
}
NTSTATUS releaseMonitor(void* handle, Modes* modes) {
    auto* adapter = static_cast<MonitorAdapter*>(handle);
    assert(modes == &adapter->modes);
    ++adapter->released;
    return STATUS_SUCCESS;
}
struct DXGK_MONITOR_INTERFACE {
    int Version = DXGK_MONITOR_INTERFACE_VERSION_V1;
    decltype(&acquireMonitor) pfnAcquireMonitorSourceModeSet = acquireMonitor;
    decltype(&releaseMonitor) pfnReleaseMonitorSourceModeSet = releaseMonitor;
};
NTSTATUS queryMonitor(void* handle, int version, const DXGK_MONITOR_INTERFACE** monitor) {
    assert(handle != nullptr && version == DXGK_MONITOR_INTERFACE_VERSION_V1);
    static DXGK_MONITOR_INTERFACE api;
    *monitor = &api;
    return STATUS_SUCCESS;
}
struct Hardware {
    std::vector<VIDEO_MODE_INFORMATION> modes{{1920,1200,120}, {3600,2260,120}};
    UINT GetCurrentModeIndex() { return 0; }
    UINT GetModeCount() { return static_cast<UINT>(modes.size()); }
    PVIDEO_MODE_INFORMATION GetModeInfo(UINT i) { return &modes.at(i); }
};
struct VioGpuDod {
    Hardware* m_pHWDevice;
    struct { void* DeviceHandle{}; decltype(&queryMonitor) DxgkCbQueryMonitorInterface{}; } m_DxgkInterface;
    static void BuildVideoSignalInfo(Signal* signal, PVIDEO_MODE_INFORMATION mode) {
        *signal = {mode->VisScreenWidth, mode->VisScreenHeight, mode->Frequency};
    }
    NTSTATUS AddSingleMonitorMode(const DXGKARG_RECOMMENDMONITORMODES*);
    NTSTATUS UpdateConnectedMonitorModes(D3DDDI_VIDEO_PRESENT_TARGET_ID);
};
#include "monitor-body.inc"

int main() {
    Hardware hardware;
    VioGpuDod driver{&hardware, {}};
    MonitorInterface callbacks;
    // A monitor set already holds the current mode when the new host size arrives.
    Modes set;
    set.registered.push_back({1920,1200,120});
    DXGKARG_RECOMMENDMONITORMODES request{&set, &callbacks};
    assert(driver.AddSingleMonitorMode(&request) == STATUS_SUCCESS);
    assert(set.registered.size() == 28); // Both geometries, every refresh preset.
    assert(set.allocated == 0 && set.releases > 0);
    const int released = set.releases;
    assert(driver.AddSingleMonitorMode(&request) == STATUS_SUCCESS);
    assert(set.registered.size() == 28 && set.allocated == 0 && set.releases > released);
    // A real callback error must survive the release of the unowned mode.
    Modes failing;
    failing.failCustom = true;
    request.hMonitorSourceModeSet = &failing;
    assert(driver.AddSingleMonitorMode(&request) == addFailure);
    assert(failing.allocated == 0);
    // Updating an acquired, retained monitor set adds new timings without HPD.
    MonitorAdapter adapter;
    adapter.modes.registered.push_back({1920,1200,120});
    driver.m_DxgkInterface = {&adapter, queryMonitor};
    assert(driver.UpdateConnectedMonitorModes(0) == STATUS_SUCCESS);
    assert(adapter.modes.registered.size() == 28);
    assert(adapter.acquired == 1 && adapter.released == 1);
    adapter.modes.failCustom = true;
    assert(driver.UpdateConnectedMonitorModes(0) == addFailure);
    assert(adapter.acquired == 2 && adapter.released == 2 && adapter.modes.allocated == 0);
    adapter.acquireStatus = STATUS_NOT_SUPPORTED;
    assert(driver.UpdateConnectedMonitorModes(0) == STATUS_NOT_SUPPORTED);
    assert(adapter.acquired == 2 && adapter.released == 2);
}
