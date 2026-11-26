#include "bridgevm_refresh.h"
#include <cassert>

int main()
{
    // 720Hz is below 2ms per frame. The retrace starts with 20 blank lines
    // (25.253us for a 1080-line image), then visible line zero follows.
    constexpr unsigned long long clock = 1'000'000'000;
    assert(BridgeVmRefreshRasterLine(0, clock, 720, 1080) == 1080);
    assert(BridgeVmRefreshRasterLine(25'252, clock, 720, 1080) == 1099);
    assert(BridgeVmRefreshRasterLine(25'253, clock, 720, 1080) == 0);
    assert(BridgeVmRefreshRasterLine(1'388'888, clock, 720, 1080) == 1079);
    assert(BridgeVmRefreshRasterLine(1'388'889, clock, 720, 1080) == 1080);
    // A late scanline query must retain its phase without overflowing after
    // hours of uptime. 60Hz has the same blank-first contract.
    assert(BridgeVmRefreshRasterLine(86'400 * clock, clock, 720, 1080) == 1080);
    assert(BridgeVmRefreshRasterLine(0, clock, 60, 1080) == 1080);
    assert(BridgeVmRefreshRasterLine(303'031, clock, 60, 1080) == 0);
}
