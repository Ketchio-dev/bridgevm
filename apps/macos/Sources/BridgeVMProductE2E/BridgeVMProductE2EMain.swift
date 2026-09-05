import Darwin
import Foundation

@main
enum BridgeVMProductE2EMain {
    static func main() {
        do {
            let cli = try T17CLI.parse(Array(CommandLine.arguments.dropFirst()))
            let request = try T17Request.load(cli.request)
            let outcome = T17ProductRunner(request: request).run()
            let result = outcome.evidence.result(
                request: request, failureCode: outcome.failureCode,
                cleanupVerified: outcome.cleanupVerified,
                installerSourcePath: outcome.installerSourcePath,
                uiFrontendAutomated: outcome.uiFrontendAutomated
            )
            try T17ResultWriter.write(result, to: cli.result)
            exit(EXIT_SUCCESS)
        } catch let blocker as T17Blocker {
            FileHandle.standardError.write(Data("BridgeVMProductE2E BLOCKER[\(blocker.code)]: \(blocker.detail)\n".utf8))
        } catch {
            FileHandle.standardError.write(Data("BridgeVMProductE2E BLOCKER[internal-error]: request or result processing failed\n".utf8))
        }
        exit(EXIT_FAILURE)
    }
}
