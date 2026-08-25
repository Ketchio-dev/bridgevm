# Windows guest driver notes

This directory contains the guest-side assets used by BridgeVM's Windows 11 Arm
installation and driver-injection flows.

The WinPE injector (`bvinject.cmd`) installs the driver packages staged under its
`\drivers` tree into the offline Windows image. Product packaging must keep each
package complete: INF, SYS, CAT, companion binaries, certificates where required,
and any user-mode components referenced by the INF belong to one package
identity.

## General rules

- Never replace only a `.sys` underneath a catalog from a different build.
- Treat INF/SYS/CAT and referenced user-mode DLLs as one versioned package.
- Verify the bound device and package identity after installation.
- Do not report a driver as working because DISM or `pnputil` returned success;
  the device must start successfully in the live guest.
- Keep third-party license notices with any redistributed package.
- Windows media is user-supplied; BridgeVM does not redistribute Windows.

## Networking

The Windows HVF engine exposes virtio networking when the VM launch enables it.
The guest package must contain the complete ARM64 NetKVM payload expected by its
INF, including companion executables when the selected package references them.

Staging only `netkvm.inf`, `netkvm.sys`, and `netkvm.cat` is not sufficient for
packages whose INF also references `netkvmp.exe` or `netkvmco.exe`; DISM/PnP can
otherwise fail with a missing-file error.

## Experimental 3D display driver

BridgeVM supports an experimental virtio-gpu 3D driver lifecycle. Two signing
shapes are possible:

1. a package already trusted by Windows under its normal production signing
   policy; or
2. a development/test-signed package used by testers of the experimental
   graphics path.

For the second case, the injector can stage a development-only activation.
The shipping guided installer seeds Microsoft Secure Boot and therefore refuses
a test-signed or unverifiable package **before it starts any install mutation**;
the fallback is to disable 3D injection. In developer-prepared SB-off guests,
`bvinject.cmd` plants the first-boot handoff and `bvgpu-firstboot.cmd` runs a
read-only signing/Secure Boot/BCD/PnP preflight before its first certificate,
BCD or DriverStore mutation.

The four-stage activation is intentional:

1. prove the package/signing/Secure Boot/BCD/PnP state is compatible; only then
   clean superseded certificates, enable BCD test-signing, trust the current
   package certificate, and reboot;
2. remove superseded DriverStore generations and reboot;
3. prove the store is clean, install exactly one package, and reboot;
4. verify the live binding and package identity.

The sequence separates enabling test mode from the actual bind because trying to
install/start the test-signed display driver in the same boot can leave Windows
with a persistent failed-start state.

### Important Secure Boot note

Windows may refuse a request to enable test-signing while Secure Boot policy is
active. Treat that as an explicit compatibility error; do not silently weaken
Secure Boot or claim the driver is active. Preview builds that use a test-signed
GPU package must make their security/signing requirements clear to the user.

A production-signed guest driver avoids this development-mode requirement, but it
is a distribution/usability milestone rather than a requirement for the custom
HVF VMM itself.

## User-mode graphics components

A kernel display driver can start while user-mode graphics registration is still
incomplete. A render-capable package must include and register the exact user-mode
components it needs.

After installation, verify both:

- the PnP/display device state; and
- the user-mode renderer/ICD identity actually loaded by the workload.

A successful device bind without a real graphics submission is not a 3D pass.

## Recovering a damaged Windows boot configuration

If BCD edits leave the guest unable to enter Windows, repair the boot files from
WinPE with the appropriate `bcdboot` command for the target EFI System Partition,
then let BridgeVM's staged first-boot flow reapply only the settings it owns.

Avoid repeatedly applying unrelated BCD mutations while diagnosing a driver
problem. Preserve the failed logs and package identity so the failure remains
reproducible.

## Logs

The activation scripts write guest-side logs under `C:\BridgeVM`. When a driver
install fails, keep at least:

- the first-boot activation log;
- `pnputil`/PnP device state;
- package/DriverStore identity;
- the BridgeVM host run evidence;
- the exact guest image/vars hashes used by the run.

Those artifacts are more useful than a screenshot of Device Manager alone.
