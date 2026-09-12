import Testing
@testable import BridgeVMProductE2E

struct T17FileChooserKeyboardTests {
    @Test func invalidTargetsNeverActivateOrPost() {
        for pid: Int32 in [-1, 0, 1] {
            var touched = false
            #expect(throws: (any Error).self) {
                try T17FileChooserKeyboard.deliver(pid: pid, activate: { touched = true; return true },
                    foreground: { pid }, postPair: { touched = true })
            }
            #expect(!touched)
        }
    }

    @Test func activationFailureNeverPosts() {
        var touched = false
        #expect(throws: (any Error).self) {
            try T17FileChooserKeyboard.deliver(pid: 42, activate: { false },
                foreground: { touched = true; return 42 }, postPair: { touched = true })
        }
        #expect(!touched)
    }

    @Test func missingOrDifferentForegroundNeverPosts() {
        for front in [nil, Int32(43)] {
            var posted = false
            #expect(throws: (any Error).self) {
                try T17FileChooserKeyboard.deliver(pid: 42, activate: { true },
                    foreground: { front }, postPair: { posted = true })
            }
            #expect(!posted)
        }
    }

    @Test func matchedForegroundDeliversExactlyOnePairInOrder() throws {
        var calls: [String] = []
        try T17FileChooserKeyboard.deliver(pid: 42,
            activate: { calls.append("activate"); return true },
            foreground: { calls.append("foreground"); return 42 },
            postPair: { calls.append("pair") })
        #expect(calls == ["activate", "foreground", "pair", "foreground"])
    }

    @Test func focusLossAfterPairFailsWithoutReplay() {
        var pairs = 0
        var probes = 0
        #expect(throws: (any Error).self) {
            try T17FileChooserKeyboard.deliver(pid: 42, activate: { true }, foreground: {
                probes += 1
                return probes == 1 ? 42 : 43
            }, postPair: { pairs += 1 })
        }
        #expect(pairs == 1)
        #expect(probes == 2)
    }
}
