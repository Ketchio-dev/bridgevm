// The actual AddSingleMonitorMode body is included by the source-kit test.
// Only the WDDM allocation/ownership callbacks are replaced here.
#include <algorithm>
#include <cassert>
#include <vector>

using UINT = unsigned int;
using ULONG = unsigned long;
using NTSTATUS = int;
using LONG_PTR = long long;
constexpr NTSTATUS STATUS_SUCCESS = 0;
constexpr NTSTATUS STATUS_GRAPHICS_MODE_ALREADY_IN_MODESET = -1;
constexpr NTSTATUS addFailure = -2;
#define NT_SUCCESS(status) ((status) >= 0)
#define PAGED_CODE() ((void)0)
#define _In_
#define _In_opt_
#define CONST const
#define DbgPrint(...) ((void)0)
#define NT_ASSERT(value) assert(value)
#define UNREFERENCED_PARAMETER(value) ((void)(value))
#define RTL_NUMBER_OF(value) (sizeof(value) / sizeof(value[0]))
constexpr int D3DKMDT_MCO_DRIVER = 1, D3DKMDT_MP_PREFERRED = 1;
constexpr int D3DKMDT_MP_NOTPREFERRED = 0, D3DKMDT_CB_SRGB = 1;
constexpr UINT NOM_WIDTH_SIZE = 1024, NOM_HEIGHT_SIZE = 768;

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
using D3DKMDT_VIDPN_TARGET_MODE = D3DKMDT_MONITOR_SOURCE_MODE;
using DXGK_VIDPNTARGETMODESET_INTERFACE = MonitorInterface;
using D3DKMDT_HVIDPNTARGETMODESET = Modes*;
using D3DDDI_VIDEO_PRESENT_SOURCE_ID = UINT;
struct D3DKMDT_VIDPN_SOURCE_MODE {
    struct { struct { struct { UINT cx{}, cy{}; } VisibleRegionSize; } Graphics; } Format;
};
struct DXGKARG_RECOMMENDMONITORMODES {
    Modes* hMonitorSourceModeSet;
    MonitorInterface* pMonitorSourceModeSetInterface;
};
struct Hardware {
    std::vector<VIDEO_MODE_INFORMATION> modes{{1920,1200,120}, {3600,2260,120}};
    UINT GetCurrentModeIndex() { return 0; }
    UINT GetModeCount() { return static_cast<UINT>(modes.size()); }
    PVIDEO_MODE_INFORMATION GetModeInfo(UINT i) { return &modes.at(i); }
};
struct VioGpuDod {
    Hardware* m_pHWDevice;
    static void BuildVideoSignalInfo(Signal* signal, PVIDEO_MODE_INFORMATION mode) {
        *signal = {mode->VisScreenWidth, mode->VisScreenHeight, mode->Frequency};
    }
    NTSTATUS AddSingleMonitorMode(const DXGKARG_RECOMMENDMONITORMODES*);
    NTSTATUS AddSingleTargetMode(const DXGK_VIDPNTARGETMODESET_INTERFACE*,
        D3DKMDT_HVIDPNTARGETMODESET, const D3DKMDT_VIDPN_SOURCE_MODE*, D3DDDI_VIDEO_PRESENT_SOURCE_ID);
};
#include "monitor-body.inc"

int main() {
    Hardware hardware;
    VioGpuDod driver{&hardware};
    MonitorInterface callbacks;
    // A monitor set already holds the current mode when the new host size arrives.
    Modes set;
    set.registered.push_back({1920,1200,120});
    DXGKARG_RECOMMENDMONITORMODES request{&set, &callbacks};
    assert(driver.AddSingleMonitorMode(&request) == STATUS_SUCCESS);
    assert(set.registered.size() == 30); // Both geometries, every refresh preset.
    assert(std::any_of(set.registered.begin(), set.registered.end(), [](const Signal& mode) {
        return mode.width == 1920 && mode.height == 1200 && mode.hz == 64;
    }));
    assert(set.allocated == 0 && set.releases > 0);
    const int released = set.releases;
    assert(driver.AddSingleMonitorMode(&request) == STATUS_SUCCESS);
    assert(set.registered.size() == 30 && set.allocated == 0 && set.releases > released);
    // A real callback error must survive the release of the unowned mode.
    Modes failing;
    failing.failCustom = true;
    request.hMonitorSourceModeSet = &failing;
    assert(driver.AddSingleMonitorMode(&request) == addFailure);
    assert(failing.allocated == 0);
    // A pinned source constrains identity scanout targets to its visible geometry.
    D3DKMDT_VIDPN_SOURCE_MODE source;
    source.Format.Graphics.VisibleRegionSize = {3600,2260};
    Modes targets;
    assert(driver.AddSingleTargetMode(&callbacks, &targets, &source, 0) == STATUS_SUCCESS);
    assert(targets.registered.size() == 15);
    for (const auto& mode : targets.registered) assert(mode.width == 3600 && mode.height == 2260);
    for (const ULONG hz : {30UL, 60UL, 64UL, 120UL}) {
        assert(std::any_of(targets.registered.begin(), targets.registered.end(),
                           [hz](const Signal& mode) { return mode.hz == hz; }));
    }
    Modes unconstrained;
    assert(driver.AddSingleTargetMode(&callbacks, &unconstrained, nullptr, 0) == STATUS_SUCCESS);
    assert(unconstrained.registered.size() == 30);
    Modes targetFailure;
    targetFailure.failCustom = true;
    assert(driver.AddSingleTargetMode(&callbacks, &targetFailure, &source, 0) == addFailure);
    assert(targetFailure.allocated == 0);
}
