// The actual BuildVideoSignalInfo, AddSingleMonitorMode, AddSingleTargetMode
// and SetRefreshTiming bodies are included by the source-kit test. Only the
// WDDM allocation/ownership callbacks and kernel primitives are replaced here.
#include <cassert>
#include <cstddef>
#include <vector>

using UINT = unsigned int;
using ULONG = unsigned long;
using LONG = long;
using LONG64 = long long;
using UINT64 = unsigned long long;
using USHORT = unsigned short;
using BOOLEAN = unsigned char;
using NTSTATUS = int;
using LONG_PTR = long long;
#define VOID void
#define FALSE 0
constexpr NTSTATUS STATUS_SUCCESS = 0;
constexpr NTSTATUS STATUS_GRAPHICS_MODE_ALREADY_IN_MODESET = -1;
constexpr NTSTATUS addFailure = -2;
constexpr NTSTATUS STATUS_GRAPHICS_INVALID_VIDPN_TARGETMODESET = -3;
constexpr NTSTATUS wildcardRejected = -4;
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
constexpr UINT D3DKMDT_FREQUENCY_NOTSPECIFIED = ~1u;
constexpr UINT NOM_WIDTH_SIZE = 1024, NOM_HEIGHT_SIZE = 768;
// Whatever the VidPN manager leaves in a record it has just created.
constexpr UINT CREATED = 0x5a5a5a5au;
static const ULONG g_BridgeRefreshRates[] = {120,30,60,75,90,144,165,180,240,360,480,540,600,720};
constexpr UINT g_BridgeRefreshRateCount = 14;

struct VIDEO_MODE_INFORMATION { UINT VisScreenWidth, VisScreenHeight; ULONG Frequency; };
using PVIDEO_MODE_INFORMATION = VIDEO_MODE_INFORMATION*;
struct Region { UINT cx = CREATED, cy = CREATED; };
struct D3DDDI_RATIONAL { UINT Numerator = CREATED, Denominator = CREATED; };
struct D3DKMDT_VIDEO_SIGNAL_INFO {
    int VideoStandard = 0;
    Region TotalSize, ActiveSize;
    D3DDDI_RATIONAL VSyncFreq, HSyncFreq;
    UINT64 PixelRate = CREATED;
    int ScanLineOrdering = 0;
};
bool same(const D3DKMDT_VIDEO_SIGNAL_INFO& a, const D3DKMDT_VIDEO_SIGNAL_INFO& b) {
    return a.VideoStandard == b.VideoStandard && a.TotalSize.cx == b.TotalSize.cx &&
           a.TotalSize.cy == b.TotalSize.cy && a.ActiveSize.cx == b.ActiveSize.cx &&
           a.ActiveSize.cy == b.ActiveSize.cy && a.VSyncFreq.Numerator == b.VSyncFreq.Numerator &&
           a.VSyncFreq.Denominator == b.VSyncFreq.Denominator && a.HSyncFreq.Numerator == b.HSyncFreq.Numerator &&
           a.HSyncFreq.Denominator == b.HSyncFreq.Denominator && a.PixelRate == b.PixelRate &&
           a.ScanLineOrdering == b.ScanLineOrdering;
}
struct D3DKMDT_MONITOR_SOURCE_MODE {
    D3DKMDT_VIDEO_SIGNAL_INFO VideoSignalInfo{};
    int Origin{}, Preference{}, ColorBasis{};
    struct { int FirstChannel{}, SecondChannel{}, ThirdChannel{}, FourthChannel{}; } ColorCoeffDynamicRanges;
};
struct Modes {
    std::vector<D3DKMDT_MONITOR_SOURCE_MODE> registered;
    int allocated = 0, releases = 0;
    bool failCustom = false, rejectWildcard = false;
};
NTSTATUS create(Modes* set, D3DKMDT_MONITOR_SOURCE_MODE** mode) {
    ++set->allocated;
    *mode = new D3DKMDT_MONITOR_SOURCE_MODE;
    return STATUS_SUCCESS;
}
NTSTATUS add(Modes* set, D3DKMDT_MONITOR_SOURCE_MODE* mode) {
    if (set->failCustom && mode->VideoSignalInfo.ActiveSize.cx == 3600) return addFailure;
    if (set->rejectWildcard && mode->VideoSignalInfo.VSyncFreq.Numerator == D3DKMDT_FREQUENCY_NOTSPECIFIED)
        return wildcardRejected;
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
    struct { struct { struct { UINT cx{}, cy{}; } VisibleRegionSize; } Graphics; } Format;
};
struct DXGKARG_RECOMMENDMONITORMODES {
    Modes* hMonitorSourceModeSet;
    MonitorInterface* pMonitorSourceModeSetInterface;
};
struct Hardware {
    // As upstream: mode 0 is 640x480, 1024x768 is current, the host size is last.
    std::vector<VIDEO_MODE_INFORMATION> modes{{640,480,120}, {1024,768,120}, {3600,2260,120}};
    USHORT GetCurrentModeIndex() { return 1; }
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
struct LARGE_INTEGER { LONG64 QuadPart; };
LARGE_INTEGER KeQueryPerformanceCounter(void*) { return {0}; }
#define KeAcquireGuardedMutex(mutex) ((void)(mutex))
#define KeReleaseGuardedMutex(mutex) ((void)(mutex))
LONG InterlockedExchange(volatile LONG* target, LONG value) { const LONG old = *target; *target = value; return old; }
LONG64 InterlockedExchange64(volatile LONG64* target, LONG64 value) { const LONG64 old = *target; *target = value; return old; }
struct VioGpuAdapter {
    int m_RefreshMutex = 0;
    volatile LONG m_RefreshHz = 120, m_ActiveLines = 800;
    volatile LONG64 m_LastVsyncQpc = 1;
    int applied = 0;
    void ApplyVsync() { ++applied; }
    NTSTATUS SetRefreshTiming(const D3DDDI_RATIONAL&, UINT);
};
#include "monitor-body.inc"

// r7's rule, unchanged: only 1024x768 at 120Hz is ever preferred.
static int preferredCount(const Modes& set) {
    int count = 0;
    for (const auto& mode : set.registered) {
        if (mode.Preference != D3DKMDT_MP_PREFERRED) continue;
        const auto& signal = mode.VideoSignalInfo;
        assert(signal.ActiveSize.cx == 1024 && signal.ActiveSize.cy == 768 && signal.VSyncFreq.Numerator == 120);
        ++count;
    }
    return count;
}
static bool isStockWildcard(const D3DKMDT_VIDPN_TARGET_MODE& mode) {
    const auto& s = mode.VideoSignalInfo;
    return s.ActiveSize.cx == CREATED && s.ActiveSize.cy == CREATED && s.TotalSize.cx == 640 &&
           s.TotalSize.cy == 480 && s.VideoStandard == D3DKMDT_VSS_OTHER &&
           s.VSyncFreq.Numerator == D3DKMDT_FREQUENCY_NOTSPECIFIED &&
           s.VSyncFreq.Denominator == D3DKMDT_FREQUENCY_NOTSPECIFIED &&
           s.HSyncFreq.Numerator == D3DKMDT_FREQUENCY_NOTSPECIFIED &&
           s.HSyncFreq.Denominator == D3DKMDT_FREQUENCY_NOTSPECIFIED &&
           s.PixelRate == D3DKMDT_FREQUENCY_NOTSPECIFIED && s.ScanLineOrdering == D3DDDI_VSSLO_PROGRESSIVE &&
           mode.Preference == D3DKMDT_MP_NOTPREFERRED;
}

int main() {
    Hardware hardware;
    VioGpuDod driver{&hardware};
    MonitorInterface callbacks;
    // Monitor modes are r7's: every geometry at every rate, even when the set
    // already holds the current mode, and only 1024x768@120 preferred.
    Modes set;
    D3DKMDT_MONITOR_SOURCE_MODE current;
    driver.BuildVideoSignalInfo(&current.VideoSignalInfo, hardware.GetModeInfo(hardware.GetCurrentModeIndex()));
    set.registered.push_back(current);
    DXGKARG_RECOMMENDMONITORMODES request{&set, &callbacks};
    assert(driver.AddSingleMonitorMode(&request) == STATUS_SUCCESS);
    assert(set.registered.size() == 42 && set.allocated == 0 && set.releases > 0);
    const int released = set.releases;
    assert(driver.AddSingleMonitorMode(&request) == STATUS_SUCCESS);
    assert(set.registered.size() == 42 && set.allocated == 0 && set.releases > released);
    Modes fresh;
    request.hMonitorSourceModeSet = &fresh;
    assert(driver.AddSingleMonitorMode(&request) == STATUS_SUCCESS);
    assert(fresh.registered.size() == 42 && preferredCount(fresh) == 1);
    // A real callback error must survive the release of the unowned mode.
    Modes failing;
    failing.failCustom = true;
    request.hMonitorSourceModeSet = &failing;
    assert(driver.AddSingleMonitorMode(&request) == addFailure);
    assert(failing.allocated == 0);

    // A pinned listed size keeps r7's 14 exact timings, then one
    // non-preferred stock wildcard; preference stays r7's.
    D3DKMDT_VIDPN_SOURCE_MODE source;
    const UINT listed[][3] = {{3600,2260,0}, {1024,768,1}};
    for (const auto& size : listed) {
        source.Format.Graphics.VisibleRegionSize = {size[0], size[1]};
        Modes targets;
        assert(driver.AddSingleTargetMode(&callbacks, &targets, &source, 0) == STATUS_SUCCESS);
        assert(targets.registered.size() == 15 && targets.allocated == 0);
        for (std::size_t i = 0; i < 14; ++i) {
            const auto& signal = targets.registered[i].VideoSignalInfo;
            assert(signal.ActiveSize.cx == size[0] && signal.ActiveSize.cy == size[1]);
            assert(signal.VSyncFreq.Numerator == g_BridgeRefreshRates[i] && signal.VSyncFreq.Denominator == 1);
        }
        assert(isStockWildcard(targets.registered[14]));
        assert(preferredCount(targets) == static_cast<int>(size[2]));
    }
    // A host size the monitor never approved still gets the stock wildcard.
    source.Format.Graphics.VisibleRegionSize = {2560,1656};
    Modes unlisted;
    assert(driver.AddSingleTargetMode(&callbacks, &unlisted, &source, 0) == STATUS_SUCCESS);
    assert(unlisted.registered.size() == 1 && isStockWildcard(unlisted.registered[0]));
    Modes unconstrained;
    assert(driver.AddSingleTargetMode(&callbacks, &unconstrained, nullptr, 0) == STATUS_SUCCESS);
    assert(unconstrained.registered.size() == 43 && isStockWildcard(unconstrained.registered[42]));
    assert(preferredCount(unconstrained) == 1);
    // If the VidPN manager rejects the wildcard, the exact targets survive and
    // the set is still reported; a repeated wildcard is harmless too.
    source.Format.Graphics.VisibleRegionSize = {3600,2260};
    Modes rejecting;
    rejecting.rejectWildcard = true;
    assert(driver.AddSingleTargetMode(&callbacks, &rejecting, &source, 0) == STATUS_SUCCESS);
    assert(rejecting.registered.size() == 14 && rejecting.allocated == 0 && rejecting.releases == 1);
    Modes repeated;
    repeated.registered.push_back(unlisted.registered[0]);
    assert(driver.AddSingleTargetMode(&callbacks, &repeated, &source, 0) == STATUS_SUCCESS);
    assert(repeated.registered.size() == 15 && repeated.allocated == 0);
    // A real exact-target error still fails the set after releasing its record.
    Modes targetFailure;
    targetFailure.failCustom = true;
    assert(driver.AddSingleTargetMode(&callbacks, &targetFailure, &source, 0) == addFailure);
    assert(targetFailure.allocated == 0);

    // Committing an exact target sets its rate; committing the wildcard keeps
    // the current rate and takes the raster height from the pinned source.
    VioGpuAdapter adapter;
    D3DDDI_RATIONAL rate;
    rate.Numerator = 144;
    rate.Denominator = 1;
    assert(adapter.SetRefreshTiming(rate, 2260) == STATUS_SUCCESS);
    assert(adapter.m_RefreshHz == 144 && adapter.m_ActiveLines == 2260);
    const D3DDDI_RATIONAL wildcard = unlisted.registered[0].VideoSignalInfo.VSyncFreq;
    assert(adapter.SetRefreshTiming(wildcard, 1656) == STATUS_SUCCESS);
    assert(adapter.m_RefreshHz == 144 && adapter.m_ActiveLines == 1656 && adapter.applied == 2);
    rate.Numerator = 50;
    assert(adapter.SetRefreshTiming(rate, 1656) == STATUS_GRAPHICS_INVALID_VIDPN_TARGETMODESET);
    assert(adapter.SetRefreshTiming(wildcard, 0) == STATUS_GRAPHICS_INVALID_VIDPN_TARGETMODESET);
    assert(adapter.m_RefreshHz == 144 && adapter.m_ActiveLines == 1656 && adapter.applied == 2);
}
