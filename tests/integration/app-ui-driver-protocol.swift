import Foundation
import Darwin

@main
enum AppUIDriverProtocolContracts {
    static func main() {
        do {
            try AppUIDriverCanonicalContracts.run()
            try AppUIDriverAdmissionContracts.run()
            try AppUIDriverMailboxContracts.run()
            print("PASS: \(AppUIDriverProtocolTestSupport.checks) app UI driver protocol checks; no GUI or permission query")
        } catch {
            let last = AppUIDriverProtocolTestSupport.lastCheck
            FileHandle.standardError.write(Data("FAIL near '\(last)': \(error)\n".utf8))
            exit(1)
        }
    }
}
