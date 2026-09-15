"""AppKit metadata for the reconstructed native UI diagnostic."""


def bundle_metadata(bundle_identifier):
    return {
        "CFBundleExecutable": "BridgeVMControl",
        "CFBundlePackageType": "APPL",
        "CFBundleIdentifier": bundle_identifier,
        "CFBundleName": "BridgeVM UI Diagnostic",
        "CFBundleVersion": "1",
        "CFBundleShortVersionString": "1.0",
        "LSMinimumSystemVersion": "14.0",
        "NSHighResolutionCapable": True,
        "NSPrincipalClass": "NSApplication",
    }
