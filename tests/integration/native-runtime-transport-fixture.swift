import Darwin
import Foundation

@main
enum NativeRuntimeTransportContractMain {
    static func main() {
        do {
            let args = Array(CommandLine.arguments.dropFirst())
            guard args.count >= 2 else { throw NativeRuntimeError.invalidMessage }
            let root = URL(fileURLWithPath: args[1], isDirectory: true)
            switch args[0] {
            case "pure":
                try NativeRuntimeTransportPureContracts.run(root: root)
                print("pure filesystem/framing/peer/canonical contracts PASS")
            case "churn":
                try NativeRuntimeTransportOwnershipContracts.run(root: root)
                print("64 owner close/descriptor reuse cycles PASS")
            case "owner", "slow":
                guard args.count == 3 else { throw NativeRuntimeError.invalidMessage }
                try NativeRuntimeTransportFixture.runOwner(root: root,
                    control: URL(fileURLWithPath: args[2], isDirectory: true), mode: args[0])
            case "query":
                let library = try NativeRuntimeLibraryHandle.open(rootURL: root, create: false)
                let request = NativeRuntimeTransportFixture.request(library.identity)
                let response = try NativeRuntimeClient.query(rootURL: root, request: request)
                FileHandle.standardOutput.write(try NativeRuntimeCodec.encode(response))
            default: throw NativeRuntimeError.invalidMessage
            }
        } catch {
            FileHandle.standardError.write(Data("\(error.localizedDescription)\n".utf8))
            exit(1)
        }
    }
}
