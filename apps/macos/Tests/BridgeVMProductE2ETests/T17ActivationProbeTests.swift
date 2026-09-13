import Testing
@testable import BridgeVMProductE2E

struct T17ActivationProbeTests {
    @Test func failedActivationRetainsNativeAndAXResultsWithMonotonicElapsed() {
        var now = 0.0
        var attempts = 0
        let result = T17ActivationProbe.measure(
            timeout: 0.4, clock: { now }, isActive: { false },
            activate: { attempts += 1; return false }, setFront: { -25204 },
            readFront: { (-25202, nil) }, pause: { now += $0 }, foreground: { 77 })
        #expect(!result.succeeded)
        #expect(result.attempts == 2 && attempts == 2)
        #expect(result.elapsedMilliseconds == 400)
        #expect(result.nativeActivationAccepted == false)
        #expect(result.axSetCode == -25204 && result.axReadCode == -25202)
        #expect(result.nativeActive == false && result.axFront == nil)
        #expect(result.observedFrontPID == 77)
    }

    @Test func alreadyActiveDoesNotRequestActivationOrReadAX() {
        var touched = false
        let result = T17ActivationProbe.measure(
            timeout: 5, clock: { 123 }, isActive: { true },
            activate: { touched = true; return false }, setFront: { touched = true; return 0 },
            readFront: { touched = true; return (0, true) },
            pause: { _ in touched = true }, foreground: { 42 })
        #expect(result.succeeded && !touched)
        #expect(result.attempts == 0 && result.elapsedMilliseconds == 0)
        #expect(result.nativeActive == true)
        #expect(result.axSetCode == nil && result.axReadCode == nil)
    }

    @Test func AXSuccessPreservesUnacceptedNativeRequestAsDistinctObservation() {
        var now = 0.0
        let result = T17ActivationProbe.measure(
            timeout: 5, clock: { now }, isActive: { false }, activate: { false },
            setFront: { 0 }, readFront: { (0, true) },
            pause: { now += $0 }, foreground: { 42 })
        #expect(result.succeeded && result.attempts == 1)
        #expect(result.nativeActivationAccepted == false && result.nativeActive == false)
        #expect(result.axFront == true && result.axReadCode == 0)
        #expect(result.elapsedMilliseconds == 200)
    }

    @Test func deadlineFallbackStillAcceptsNativeActiveState() {
        var now = 0.0
        let result = T17ActivationProbe.measure(
            timeout: 0.2, clock: { now }, isActive: { now >= 0.2 }, activate: { nil },
            setFront: { -25202 }, readFront: { (-25202, nil) },
            pause: { now += $0 }, foreground: { 42 })
        #expect(result.succeeded && result.attempts == 1)
        #expect(result.nativeActivationAccepted == nil && result.nativeActive == true)
        #expect(result.axFront == nil && result.axReadCode == -25202)
    }
}
