// The actual BuildVideoSignalInfo, AddSingleMonitorMode and AddSingleTargetMode
// bodies are included by the source-kit test. Only the WDDM allocation and
// ownership callbacks are replaced here.
#include <cassert>
#include <cstddef>
#include <vector>

using UINT = unsigned int;
using ULONG = unsigned long;
using UINT64 = unsigned long long;
using USHORT = unsigned short;
using NTSTATUS = int;
using LONG_PTR = long long;
#define VOID void
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
constexpr int D3DKMDT_MCO_DRIVER = 1, D3DKMDT_MP_PREFERRED = 1;
constexpr int D3DKMDT_MP_NOTPREFERRED = 2, D3DKMDT_CB_SRGB = 1;
constexpr int D3DKMDT_VSS_OTHER = 255, D3DDDI_VSSLO_PROGRESSIVE = 1;
static const ULONG g_BridgeRefreshRates[] = {120,30,60,75,90,144,165,180,240,360,480,540,600,720};
constexpr UINT g_BridgeRefreshRateCount = 14;

struct VIDEO_MODE_INFORMATION { UINT VisScreenWidth, VisScreenHeight; ULONG Frequency; };
using PVIDEO_MODE_INFORMATION = VIDEO_MODE_INFORMATION*;
struct Region { UINT cx{}, cy{}; };
struct D3DDDI_RATIONAL { UINT Numerator{}, Denominator{}; };
struct D3DKMDT_VIDEO_SIGNAL_INFO {
    int VideoStandard = 0;
    Region TotalSize, ActiveSize;
    D3DDDI_RATIONAL VSyncFreq, HSyncFreq;
    UINT64 PixelRate = 0;
    int ScanLineOrdering = 0;
};
bool same(const D3DKMDT_VIDEO_SIGNAL_INFO& a, const D3DKMDT_VIDEO_SIGNAL_INFO& b) {
    return a.ActiveSize.cx == b.ActiveSize.cx && a.ActiveSize.cy == b.ActiveSize.cy &&
           a.TotalSize.cx == b.TotalSize.cx && a.TotalSize.cy == b.TotalSize.cy &&
           a.VSyncFreq.Numerator == b.VSyncFreq.Numerator && a.VSyncFreq.Denominator == b.VSyncFreq.Denominator;
}
struct D3DKMDT_MONITOR_SOURCE_MODE {
    D3DKMDT_VIDEO_SIGNAL_INFO VideoSignalInfo{};
    int Origin{}, Preference{}, ColorBasis{};
    struct { int FirstChannel{}, SecondChannel{}, ThirdChannel{}, FourthChannel{}; } ColorCoeffDynamicRanges;
};
struct Modes {
    std::vector<D3DKMDT_MONITOR_SOURCE_MODE> registered;
    int allocated = 0, releases = 0;
    bool failCustom = false;
};
NTSTATUS create(Modes* set, D3DKMDT_MONITOR_SOURCE_MODE** mode) {
    ++set->allocated;
    *mode = new D3DKMDT_MONITOR_SOURCE_MODE;
    return STATUS_SUCCESS;
}
NTSTATUS add(Modes* set, D3DKMDT_MONITOR_SOURCE_MODE* mode) {
    if (set->failCustom && mode->VideoSignalInfo.ActiveSize.cx == 3600) return addFailure;
    for (const auto& old : set->registered)
        if (same(old.VideoSignalInfo, mode->VideoSignalInfo))
            return STATUS_GRAPHICS_MODE_ALREADY_IN_MODESET;
    set->registered.push_back(*mode);
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
    struct { struct { Region VisibleRegionSize; } Graphics; } Format;
};
struct DXGKARG_RECOMMENDMONITORMODES {
    Modes* hMonitorSourceModeSet;
    MonitorInterface* pMonitorSourceModeSetInterface;
};
struct Hardware {
    // As upstream: mode 0 is 640x480 and the host's custom size is the last slot.
    std::vector<VIDEO_MODE_INFORMATION> modes{{640,480,120}, {1920,1200,120}, {3600,2260,120}};
    USHORT current = 1;
    USHORT GetCurrentModeIndex() { return current; }
    USHORT GetCustomModeIndex() { return static_cast<USHORT>(modes.size() - 1); }
    UINT GetModeCount() { return static_cast<UINT>(modes.size()); }
    PVIDEO_MODE_INFORMATION GetModeInfo(UINT i) { return &modes.at(i); }
};
struct VioGpuDod {
    Hardware* m_pHWDevice;
    VOID BuildVideoSignalInfo(D3DKMDT_VIDEO_SIGNAL_INFO*, PVIDEO_MODE_INFORMATION);
    NTSTATUS AddSingleMonitorMode(const DXGKARG_RECOMMENDMONITORMODES*);
    NTSTATUS AddSingleTargetMode(const DXGK_VIDPNTARGETMODESET_INTERFACE*,
        D3DKMDT_HVIDPNTARGETMODESET, const D3DKMDT_VIDPN_SOURCE_MODE*, D3DDDI_VIDEO_PRESENT_SOURCE_ID);
};
#include "monitor-body.inc"

// Exactly one mode in the set is preferred, and it is width x height at 120Hz.
static bool onlyPreferredIs(const Modes& set, UINT width, UINT height) {
    int preferred = 0;
    for (const auto& mode : set.registered) {
        if (mode.Preference != D3DKMDT_MP_PREFERRED) {
            if (mode.Preference != D3DKMDT_MP_NOTPREFERRED) return false;
            continue;
        }
        const auto& signal = mode.VideoSignalInfo;
        if (signal.ActiveSize.cx != width || signal.ActiveSize.cy != height || signal.VSyncFreq.Numerator != 120)
            return false;
        ++preferred;
    }
    return preferred == 1;
}

int main() {
    Hardware hardware;
    VioGpuDod driver{&hardware};
    MonitorInterface callbacks;
    // A monitor set already holds the current mode when the new host size arrives.
    Modes set;
    D3DKMDT_MONITOR_SOURCE_MODE current;
    driver.BuildVideoSignalInfo(&current.VideoSignalInfo, hardware.GetModeInfo(hardware.GetCurrentModeIndex()));
    current.Preference = D3DKMDT_MP_NOTPREFERRED;
    set.registered.push_back(current);
    DXGKARG_RECOMMENDMONITORMODES request{&set, &callbacks};
    assert(driver.AddSingleMonitorMode(&request) == STATUS_SUCCESS);
    assert(set.registered.size() == 42); // Every geometry, every refresh preset.
    assert(set.allocated == 0 && set.releases > 0);
    assert(onlyPreferredIs(set, 3600, 2260)); // The custom size at 120Hz, nothing else.
    const int released = set.releases;
    assert(driver.AddSingleMonitorMode(&request) == STATUS_SUCCESS);
    assert(set.registered.size() == 42 && set.allocated == 0 && set.releases > released);
    // A fresh set whose current mode is not the custom size, and one where it is.
    Modes fresh;
    request.hMonitorSourceModeSet = &fresh;
    assert(driver.AddSingleMonitorMode(&request) == STATUS_SUCCESS);
    assert(fresh.registered.size() == 42 && onlyPreferredIs(fresh, 3600, 2260));
    hardware.current = 2;
    Modes currentIsCustom;
    request.hMonitorSourceModeSet = &currentIsCustom;
    assert(driver.AddSingleMonitorMode(&request) == STATUS_SUCCESS);
    assert(currentIsCustom.registered.size() == 42 && onlyPreferredIs(currentIsCustom, 3600, 2260));
    hardware.current = 1;
    // A real callback error must survive the release of the unowned mode.
    Modes failing;
    failing.failCustom = true;
    request.hMonitorSourceModeSet = &failing;
    assert(driver.AddSingleMonitorMode(&request) == addFailure);
    assert(failing.allocated == 0);

    // A pinned source constrains identity scanout targets to its geometry, and
    // exactly its 120Hz timing is preferred, whether or not it is the custom size.
    D3DKMDT_VIDPN_SOURCE_MODE source;
    const UINT listed[][2] = {{3600,2260}, {1920,1200}};
    for (const auto& size : listed) {
        source.Format.Graphics.VisibleRegionSize = {size[0], size[1]};
        Modes targets;
        assert(driver.AddSingleTargetMode(&callbacks, &targets, &source, 0) == STATUS_SUCCESS);
        assert(targets.registered.size() == 14 && targets.allocated == 0);
        for (std::size_t i = 0; i < 14; ++i) {
            const auto& signal = targets.registered[i].VideoSignalInfo;
            assert(signal.ActiveSize.cx == size[0] && signal.ActiveSize.cy == size[1]);
            assert(signal.VSyncFreq.Numerator == g_BridgeRefreshRates[i] && signal.VSyncFreq.Denominator == 1);
        }
        assert(onlyPreferredIs(targets, size[0], size[1]));
    }
    // Without a pinned source every geometry is offered; the custom size's 120Hz is preferred.
    Modes unconstrained;
    assert(driver.AddSingleTargetMode(&callbacks, &unconstrained, nullptr, 0) == STATUS_SUCCESS);
    assert(unconstrained.registered.size() == 42 && onlyPreferredIs(unconstrained, 3600, 2260));
    // A size missing from the mode list still gets no targets, as in r7.
    source.Format.Graphics.VisibleRegionSize = {2560,1656};
    Modes unlisted;
    assert(driver.AddSingleTargetMode(&callbacks, &unlisted, &source, 0) == STATUS_SUCCESS);
    assert(unlisted.registered.empty() && unlisted.allocated == 0);
    source.Format.Graphics.VisibleRegionSize = {3600,2260};
    Modes targetFailure;
    targetFailure.failCustom = true;
    assert(driver.AddSingleTargetMode(&callbacks, &targetFailure, &source, 0) == addFailure);
    assert(targetFailure.allocated == 0);
}
