// sources: apps/macos/Sources/BridgeVMControl/HvfEngine/HelperCodeSignature.swift
import Foundation
import Testing

@Suite("HelperCodeSignature")
struct HelperCodeSignatureTests {

    /// A path that exists and is executable, so the test exercises the
    /// signature rules rather than the missing-file rule.
    private func executableFile() -> String {
        "/bin/sh"
    }

    @Test("a helper signed by the app's team is accepted")
    func matchingTeamAccepted() {
        let rejection = HelperCodeSignature.validate(
            helperPath: executableFile(),
            appTeam: "ABCDE12345",
            teamReader: { _ in "ABCDE12345" }
        )
        #expect(rejection == nil)
    }

    @Test("a helper signed by a different team is refused")
    func differentTeamRefused() {
        let rejection = HelperCodeSignature.validate(
            helperPath: executableFile(),
            appTeam: "ABCDE12345",
            teamReader: { _ in "ZZZZZ99999" }
        )
        #expect(rejection == .teamMismatch(
            helper: executableFile(),
            expected: "ABCDE12345",
            found: "ZZZZZ99999"
        ))
    }

    @Test("an unsigned helper is refused when the app has a team")
    func unsignedRefused() {
        let rejection = HelperCodeSignature.validate(
            helperPath: executableFile(),
            appTeam: "ABCDE12345",
            teamReader: { _ in nil }
        )
        #expect(rejection == .unsigned(executableFile()))
    }

    @Test("a missing helper is refused before any signature is read")
    func missingHelperRefused() {
        var readerCalled = false
        let rejection = HelperCodeSignature.validate(
            helperPath: "/nonexistent/helper-\(UUID().uuidString)",
            appTeam: "ABCDE12345",
            teamReader: { _ in readerCalled = true; return "ABCDE12345" }
        )
        #expect(readerCalled == false)
        if case .notAFile = rejection {} else {
            Issue.record("expected notAFile, got \(String(describing: rejection))")
        }
    }

    @Test("an ad-hoc signed app does not require a team, so local builds run")
    func adHocAppSkipsTeamCheck() {
        #expect(HelperCodeSignature.validate(
            helperPath: executableFile(),
            appTeam: nil,
            teamReader: { _ in nil }
        ) == nil)
        #expect(HelperCodeSignature.validate(
            helperPath: executableFile(),
            appTeam: "",
            teamReader: { _ in nil }
        ) == nil)
    }

    @Test("an empty helper team reads as unsigned rather than as a match")
    func emptyHelperTeamIsUnsigned() {
        let rejection = HelperCodeSignature.validate(
            helperPath: executableFile(),
            appTeam: "ABCDE12345",
            teamReader: { _ in "" }
        )
        #expect(rejection == .unsigned(executableFile()))
    }

    @Test("codesign's 'not set' is absence, not a team named 'not set'")
    func notSetIsAbsence() {
        let output = """
        Executable=/tmp/helper
        Identifier=helper
        TeamIdentifier=not set
        """
        #expect(HelperCodeSignature.teamIdentifier(inCodesignOutput: output) == nil)
    }

    @Test("a real team identifier is parsed out of codesign's description")
    func parsesTeamIdentifier() {
        let output = """
        Executable=/tmp/helper
        Identifier=com.example.helper
        TeamIdentifier=ABCDE12345
        Sealed Resources=none
        """
        #expect(HelperCodeSignature.teamIdentifier(inCodesignOutput: output) == "ABCDE12345")
    }

    @Test("output without a TeamIdentifier line yields nil")
    func noTeamLine() {
        #expect(HelperCodeSignature.teamIdentifier(
            inCodesignOutput: "Executable=/tmp/helper\nIdentifier=helper"
        ) == nil)
    }

    @Test("the rejection message names the helper and both teams")
    func messageNamesWhatIsWrong() {
        let message = HelperCodeSignature.Rejection.teamMismatch(
            helper: "/Applications/X.app/Contents/Helpers/swtpm",
            expected: "ABCDE12345",
            found: "ZZZZZ99999"
        ).message
        #expect(message.contains("swtpm"))
        #expect(message.contains("ABCDE12345"))
        #expect(message.contains("ZZZZZ99999"))
    }
}
