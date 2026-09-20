"""Compile only Foundation/socket protocol fixtures; no application entry point."""
import subprocess

def build_transport_fixture(root, output):
    names = ["Protocol", "SessionObservation", "SessionValidation", "GraphicsValidation", "Codec", "LibraryIdentity",
             "Endpoint", "Owner", "OwnerLease", "Transport", "TransportConnection",
             "TransportConnectionCompatibility", "Server", "Client", "ClientExchange",
             "ControlProtocol", "ControlCodec", "ControlValidation", "ControlClient",
             "RequestContext", "RequestRouter", "RequestRouting", "StartProtocol", "StartCodec", "StartValidation", "StartClient"]
    sources = [root / "apps/macos/Sources/BridgeVMControl/NativeRuntime" / f"NativeRuntime{name}.swift"
               for name in names]
    sources += [root / "apps/macos/Sources/BridgeVMControl/NativeRuntime" / name for name in ["NativeInstallControlProtocol.swift", "NativeInstallControlCodec.swift", "NativeInstallObservation.swift", "NativeInstallRequestRouting.swift"]]
    sources += [root / "tests/integration" / name for name in ["NativeRuntimeTransportFixture.swift",
        "NativeRuntimeTransportPureContracts.swift", "NativeRuntimeTransportOwnershipContracts.swift",
        "NativeRuntimeControlContracts.swift", "NativeRuntimeStartContracts.swift", "native-runtime-transport-fixture.swift"]]
    executable = output / "transport-fixture"
    with (output / "compile.log").open("xb") as log:
        built = subprocess.run(["xcrun", "swiftc", "-parse-as-library", "-swift-version", "5",
            "-module-cache-path", str(output / "module-cache"), *map(str, sources), "-o", str(executable)],
            stdout=log, stderr=subprocess.STDOUT, timeout=90)
    if built.returncode:
        print((output / "compile.log").read_text())
    return executable, built.returncode
