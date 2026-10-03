extension HvfWindowsInstallPlan {
    static let installResourcePaths = [
        "scripts/build-hvf-windows-scripted-source.sh",
        "scripts/stage-hvf-windows-guest-payload.sh",
        "scripts/hvf-disk-image-utils.sh",
        "scripts/run-hvf-windows-scripted-install.sh",
        "scripts/run-hvf-windows-scripted-install-policy.sh",
        "scripts/verify-hvf-windows-install-target.sh",
        "target/release/examples/hvf_gic_boot_probe",
        "scripts/win-assets/winpeshl.ini",
        "scripts/win-assets/bvinstall.cmd",
        "scripts/win-assets/bvdiskpart.txt",
        "helpers/bv-file-compare.exe",
        "scripts/win-assets/unattend.xml",
        "helpers/bridgevm-catalog-verify",
    ] + HvfWindowsAgentAssets.requiredPaths
}
