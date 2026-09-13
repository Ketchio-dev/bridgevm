import ApplicationServices
import Testing
@testable import BridgeVMProductE2E

struct T17FileChooserKeyDiagnosticTests {
    @Test func activationFailureKeepsCurrentMetadataAndNeverPosts() throws {
        var record = T17ActivationRecord()
        record.attempts = 3; record.elapsedMilliseconds = 600
        record.nativeActivationAccepted = false; record.nativeActive = false
        record.axSetCode = -25204; record.axReadCode = -25202; record.observedFrontPID = 77
        var touched = false
        let detail = try failure {
            _ = try T17FileChooserKeyDiagnostic.run(
                pid: 42, code: 5, flags: [.maskCommand, .maskShift], activate: { record },
                foreground: { touched = true; return 42 }, postPair: { touched = true })
        }
        #expect(!touched)
        for field in ["phase=activation", "key=5", "flags=1179648", "attempts=3", "elapsed_ms=600",
                      "native_activation_accepted=false", "ax_set=-25204", "ax_read=-25202",
                      "native_active=false", "ax_front=unknown", "activation_front=77", "pair_posted=false"] {
            #expect(detail.contains(field))
        }
    }

    @Test func postPairFocusFailureKeepsPostedStateWithoutReplay() throws {
        var record = T17ActivationRecord(); record.succeeded = true
        var probes = 0
        var events: [String] = []
        let detail = try failure {
            _ = try T17FileChooserKeyDiagnostic.run(
                pid: 42, code: 36, flags: [], activate: { record }, foreground: {
                    probes += 1; return probes == 1 ? 42 : 99
                }, postPair: { events.append("down"); events.append("up") })
        }
        #expect(events == ["down", "up"] && probes == 2)
        #expect(detail.contains("phase=after_pair"))
        #expect(detail.contains("front=99") && detail.contains("pair_posted=true"))
    }

    @Test func prePairFocusFailureDoesNotSend() throws {
        var record = T17ActivationRecord(); record.succeeded = true
        var posted = false
        let detail = try failure {
            _ = try T17FileChooserKeyDiagnostic.run(
                pid: 42, code: 36, flags: [], activate: { record },
                foreground: { nil }, postPair: { posted = true })
        }
        #expect(!posted)
        #expect(detail.contains("phase=before_pair") && detail.contains("pair_posted=false"))
    }

    @Test func invalidTargetAndMissingEventPairRetainTheirOwnPhaseWithoutActivation() throws {
        var touched = false
        for missingPair in [false, true] {
            let pair: (() -> Void)? = missingPair ? nil : { touched = true }
            let detail = try failure {
                _ = try T17FileChooserKeyDiagnostic.run(
                    pid: 1, code: 5, flags: [], activate: { touched = true; return T17ActivationRecord() },
                    foreground: { touched = true; return 1 }, postPair: pair)
            }
            #expect(detail.contains(missingPair ? "phase=event_creation" : "phase=target_guard"))
            #expect(detail.contains("attempts=0") && detail.contains("pair_posted=false"))
        }
        #expect(!touched)
    }

    @Test func typedMetadataRemainsBoundedASCIIWithoutAnArbitraryStringSurface() throws {
        var record = T17ActivationRecord()
        record.attempts = Int.max; record.elapsedMilliseconds = UInt64.max
        record.axSetCode = Int32.min; record.axReadCode = Int32.max
        record.observedFrontPID = Int32.max
        let detail = try failure {
            _ = try T17FileChooserKeyDiagnostic.run(
                pid: Int32.max, code: UInt16.max, flags: CGEventFlags(rawValue: UInt64.max),
                activate: { record }, foreground: { nil }, postPair: {})
        }
        let allowed = Set("abcdefghijklmnopqrstuvwxyz0123456789_=-; ")
        #expect(detail.count <= 900 && detail.utf8.count <= 900)
        #expect(detail.allSatisfy { allowed.contains($0) })
    }

    @Test func successfulDeliveryStillPostsExactlyOnePair() throws {
        var record = T17ActivationRecord(); record.succeeded = true
        var pairs = 0
        let detail = try T17FileChooserKeyDiagnostic.run(
            pid: 42, code: 36, flags: [], activate: { record },
            foreground: { 42 }, postPair: { pairs += 1 })
        #expect(pairs == 1)
        #expect(detail.contains("phase=complete") && detail.contains("pair_posted=true"))
    }

    private func failure(_ body: () throws -> Void) throws -> String {
        do { try body(); Issue.record("expected a chooser blocker"); return "" }
        catch let blocker as T17Blocker {
            #expect(blocker.code == "input-selection-failed")
            return blocker.detail
        }
    }
}
