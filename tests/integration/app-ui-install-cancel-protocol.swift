import Darwin
import Foundation

@main
enum AppUIInstallCancelProtocolContracts {
    static func main() {
        do {
            try AppUIInstallCancelCanonicalContracts.run()
            try AppUIInstallCancelAdmissionContracts.run()
            try AppUIInstallCancelCompletionContracts.run()
            try AppUIInstallCancelMailboxContracts.run()
            print("PASS: \(AppUIDriverProtocolTestSupport.checks) install cancellation protocol checks; no GUI or permission query")
        } catch {
            let last = AppUIDriverProtocolTestSupport.lastCheck
            FileHandle.standardError.write(Data("FAIL near '\(last)': \(error)\n".utf8))
            exit(1)
        }
    }
}
