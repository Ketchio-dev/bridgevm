import Darwin
import Foundation
@main
enum BridgeVMProductE2EMain {
    static func main() {
        do {
            if try T17PermissionProbe.run(Array(CommandLine.arguments.dropFirst())) { return }
            let cli = try T17CLI.parse(Array(CommandLine.arguments.dropFirst()))
            switch cli.mode {
            case .isoInstall:
                let request = try T17Request.load(cli.request)
                let outcome = T17ProductRunner(request: request).run()
                try T17ResultWriter.write(outcome.evidence.result(
                    request: request, failureCode: outcome.failureCode, failureDetail: outcome.failureDetail,
                    cleanupVerified: outcome.cleanupVerified, installerSourcePath: outcome.installerSourcePath,
                    uiFrontendAutomated: outcome.uiFrontendAutomated), to: cli.result)
            case .installedDiskImport:
                let request = try A9ImportRequest.load(cli.request)
                let outcome = A9ImportProductRunner(request: request).run()
                try T17ResultWriter.write(outcome.evidence.result(
                    request: request, failureCode: outcome.failureCode, failureDetail: outcome.failureDetail,
                    cleanupVerified: outcome.cleanupVerified,
                    uiFrontendAutomated: outcome.uiFrontendAutomated), to: cli.result)
            }
            exit(EXIT_SUCCESS)
        } catch let blocker as T17Blocker {
            FileHandle.standardError.write(Data("BridgeVMProductE2E BLOCKER[\(blocker.code)]: \(blocker.detail)\n".utf8))
        } catch {
            FileHandle.standardError.write(Data("BridgeVMProductE2E BLOCKER[internal-error]: request or result processing failed\n".utf8))
        }
        exit(EXIT_FAILURE)
    }
}
