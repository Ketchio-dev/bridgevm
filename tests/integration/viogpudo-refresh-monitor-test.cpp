// The actual AddSingleMonitorMode and AddSingleTargetMode bodies are included
// by the source-kit test. Only the WDDM allocation/ownership callbacks and the
// stock BuildVideoSignalInfo are replaced here.
#include <cassert>
#include <cstddef>
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
constexpr int D3DKMDT_MCO_DRIVER = 1, D3DKMDT_MP_PREFERRED = 1;
constexpr int D3DKMDT_MP_NOTPREFERRED = 2, D3DKMDT_CB_SRGB = 1;
constexpr UINT NOM_WIDTH_SIZE = 1024, NOM_HEIGHT_SIZE = 768;
constexpr UINT NOTSPECIFIED = ~1u; // D3DKMDT_FREQUENCY_NOTSPECIFIED
// Whatever the VidPN manager leaves in a record it has just created.
constexpr UINT CREATED = 0x5a5a5a5au;
static const ULONG g_BridgeRefreshRates[] = {120,30,60,75,90,144,165,180,240,360,480,540,600,720};
constexpr UINT g_BridgeRefreshRateCount = 14;

struct VIDEO_MODE_INFORMATION { UINT VisScreenWidth, VisScreenHeight; };
using PVIDEO_MODE_INFORMATION = VIDEO_MODE_INFORMATION*;
struct Region { UINT cx = CREATED, cy = CREATED; };
struct Rational { UINT Numerator = CREATED, Denominator = CREATED; };
struct Signal { Region TotalSize, ActiveSize; Rational VSyncFreq; };
bool same(const Signal& a, const Signal& b) {
    return a.TotalSize.cx == b.TotalSize.cx && a.TotalSize.cy == b.TotalSize.cy &&
           a.ActiveSize.cx == b.ActiveSize.cx && a.ActiveSize.cy == b.ActiveSize.cy &&
           a.VSyncFreq.Numerator == b.VSyncFreq.Numerator && a.VSyncFreq.Denominator == b.VSyncFreq.Denominator;
}
struct D3DKMDT_MONITOR_SOURCE_MODE {
    Signal VideoSignalInfo{};
    int Origin{}, Preference{}, ColorBasis{};
    struct { int FirstChannel{}, SecondChannel{}, ThirdChannel{}, FourthChannel{}; } ColorCoeffDynamicRanges;
};
struct Modes {
    std::vector<D3DKMDT_MONITOR_SOURCE_MODE> registered;
    int allocated = 0, releases = 0;
    UINT failHz = 0;
};
NTSTATUS create(Modes* set, D3DKMDT_MONITOR_SOURCE_MODE** mode) {
    ++set->allocated;
    *mode = new D3DKMDT_MONITOR_SOURCE_MODE;
    return STATUS_SUCCESS;
}
NTSTATUS add(Modes* set, D3DKMDT_MONITOR_SOURCE_MODE* mode) {
    if (set->failHz && mode->VideoSignalInfo.VSyncFreq.Numerator == set->failHz) return addFailure;
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
    // Upstream mode 0 is MIN_WIDTH_SIZE x MIN_HEIGHT_SIZE; 1024x768 is current.
    std::vector<VIDEO_MODE_INFORMATION> modes{{640,480}, {1024,768}, {3600,2260}};
    UINT GetCurrentModeIndex() { return 1; }
    UINT GetModeCount() { return static_cast<UINT>(modes.size()); }
    PVIDEO_MODE_INFORMATION GetModeInfo(UINT i) { return &modes.at(i); }
};
struct VioGpuDod {
    Hardware* m_pHWDevice;
    // Stock: TotalSize from the mode, every timing NOTSPECIFIED, ActiveSize untouched.
    static void BuildVideoSignalInfo(Signal* signal, PVIDEO_MODE_INFORMATION mode) {
        signal->TotalSize = {mode->VisScreenWidth, mode->VisScreenHeight};
        signal->VSyncFreq = {NOTSPECIFIED, NOTSPECIFIED};
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

    // Targets never depend on the pinned source: a host size absent from the
    // monitor snapshot (2560x1656) gets the stock target (mode 0, ActiveSize as
    // created) once per virtual rate, with 120Hz, the BVR1 default, preferred.
    D3DKMDT_VIDPN_SOURCE_MODE unlisted, listed;
    unlisted.Format.Graphics.VisibleRegionSize = {2560,1656};
    listed.Format.Graphics.VisibleRegionSize = {3600,2260};
    Modes targets;
    assert(driver.AddSingleTargetMode(&callbacks, &targets, &unlisted, 0) == STATUS_SUCCESS);
    assert(targets.registered.size() == g_BridgeRefreshRateCount && targets.allocated == 0);
    int preferred = 0;
    for (UINT i = 0; i < g_BridgeRefreshRateCount; ++i) {
        const auto& target = targets.registered[i];
        const auto& signal = target.VideoSignalInfo;
        assert(signal.VSyncFreq.Numerator == g_BridgeRefreshRates[i] && signal.VSyncFreq.Denominator == 1);
        assert(signal.TotalSize.cx == 640 && signal.TotalSize.cy == 480);
        assert(signal.ActiveSize.cx == CREATED && signal.ActiveSize.cy == CREATED);
        if (target.Preference == D3DKMDT_MP_PREFERRED) {
            ++preferred;
            assert(g_BridgeRefreshRates[i] == 120);
        } else {
            assert(target.Preference == D3DKMDT_MP_NOTPREFERRED);
        }
    }
    assert(preferred == 1);
    const D3DKMDT_VIDPN_SOURCE_MODE* otherSources[] = {&listed, nullptr};
    for (const auto* source : otherSources) {
        Modes other;
        assert(driver.AddSingleTargetMode(&callbacks, &other, source, 0) == STATUS_SUCCESS);
        assert(other.registered.size() == targets.registered.size());
        for (std::size_t i = 0; i < other.registered.size(); ++i)
            assert(same(other.registered[i].VideoSignalInfo, targets.registered[i].VideoSignalInfo) &&
                   other.registered[i].Preference == targets.registered[i].Preference);
    }
    // A real target add failure must survive the release of the unowned mode.
    Modes targetFailure;
    targetFailure.failHz = 720;
    assert(driver.AddSingleTargetMode(&callbacks, &targetFailure, &unlisted, 0) == addFailure);
    assert(targetFailure.allocated == 0);

    // The monitor set already holds the current mode when the driver is asked.
    Modes set;
    D3DKMDT_MONITOR_SOURCE_MODE current;
    VioGpuDod::BuildVideoSignalInfo(&current.VideoSignalInfo, hardware.GetModeInfo(hardware.GetCurrentModeIndex()));
    set.registered.push_back(current);
    DXGKARG_RECOMMENDMONITORMODES request{&set, &callbacks};
    assert(driver.AddSingleMonitorMode(&request) == STATUS_SUCCESS);
    // Stock rate-less modes per size, plus exactly one copy of every target.
    assert(set.registered.size() == hardware.modes.size() + g_BridgeRefreshRateCount);
    assert(set.allocated == 0 && set.releases > 0);
    UINT rated = 0;
    for (const auto& mode : set.registered) rated += mode.VideoSignalInfo.VSyncFreq.Numerator != NOTSPECIFIED;
    assert(rated == g_BridgeRefreshRateCount);
    for (const auto& target : targets.registered) {
        int matches = 0;
        for (const auto& mode : set.registered) {
            if (same(mode.VideoSignalInfo, target.VideoSignalInfo)) {
                ++matches;
                assert(mode.Preference == target.Preference);
            }
        }
        assert(matches == 1);
    }
    const int released = set.releases;
    assert(driver.AddSingleMonitorMode(&request) == STATUS_SUCCESS);
    assert(set.registered.size() == hardware.modes.size() + g_BridgeRefreshRateCount);
    assert(set.allocated == 0 && set.releases > released);
    // A real callback error must survive the release of the unowned mode record.
    Modes failing;
    failing.failHz = 720;
    request.hMonitorSourceModeSet = &failing;
    assert(driver.AddSingleMonitorMode(&request) == addFailure);
    assert(failing.allocated == 0);
}
