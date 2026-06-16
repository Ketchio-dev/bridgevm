# OS Templates

BridgeVM's first template layer is metadata-only. It describes the boot/install
media a user should place in a VM bundle. Listing or using templates does not
download files; downloads happen only through the explicit `media download`
execution step after a `media download-plan` has been recorded.

Current Fast Mode installer hints are produced by `bridgevm-core`. `bridgevm
create <name> --template <id>` can copy the template's guest OS, arch, and boot
media metadata into `manifest.yaml` when those fields are not provided
explicitly. Explicit OS/arch creates still copy matching boot metadata when no
explicit boot flags are provided.

The same template metadata can be listed with `bridgevm templates` or through
the daemon socket. Listing or using templates reports hints only and does not
download installer or restore media.

| Hint id | Guest | Boot mode | Expected bundle path | Source |
| --- | --- | --- | --- | --- |
| `ubuntu-arm64-installer` | Ubuntu Arm64 | `linux-installer` | `installers/ubuntu-arm64.iso` | manual |
| `fedora-arm64-installer` | Fedora Arm64 | `linux-installer` | `installers/fedora-arm64.iso` | manual |
| `debian-arm64-installer` | Debian Arm64 | `linux-installer` | `installers/debian-arm64.iso` | manual |
| `macos-restore` | macOS Arm | `macos-restore` | `installers/macos-restore.ipsw` | manual |

Example flow:

```bash
bridgevm recommend --os ubuntu --arch arm64
bridgevm templates
bridgevm create ubuntu-template-dev --template ubuntu-arm64-installer
bridgevm boot-media ubuntu-template-dev
bridgevm media import ubuntu-template-dev --source ~/Downloads/ubuntu-arm64.iso
bridgevm boot-media ubuntu-template-dev
bridgevm media status ubuntu-template-dev
bridgevm media download-plan ubuntu-template-dev --url https://example.invalid/ubuntu.iso --sha256 <digest>
bridgevm media download ubuntu-template-dev
bridgevm media verify ubuntu-template-dev --sha256 <digest>
bridgevm create ubuntu-dev --os ubuntu --arch arm64
bridgevm prepare-run ubuntu-dev
lightvm-runner ubuntu-dev --print-plan
```

`bridgevm boot-media ubuntu-template-dev` resolves the template-provided boot
media against the `.vmbridge` bundle and reports the expected installer,
kernel/initrd, or macOS restore path with its `exists` state. It is a focused
check for template media wiring and does not download installer or restore
media.

`bridgevm media import ubuntu-template-dev --source ~/Downloads/ubuntu-arm64.iso`
copies a local installer the user already has into the expected bundle path
from the template, such as `installers/ubuntu-arm64.iso`. Running
`bridgevm boot-media ubuntu-template-dev` again should then show the same media
entry with `exists: true`. This is still a manual import step, not an OS
download flow. The import also records metadata under
`.vmbridge/metadata/boot-media/<kind>.json`.

`bridgevm media status ubuntu-template-dev` is the import-friendly status view
for the same Fast Mode boot media. It shows each installer, kernel, initrd, or
macOS restore entry with its resolved path, `exists` state, file size, and
latest `media import`, `media verify`, `media download-plan`, and
`media download` records when present. It is also an inspection command and does
not download media.

`bridgevm media download-plan ubuntu-template-dev --url <url> [--sha256 <hex>]`
records the remote media URL the user intends to use before any download is
performed. It prints and writes the resolved bundle destination, optional
expected SHA-256, current file existence and size, and latest import/verify
state under `.vmbridge/metadata/boot-media/<kind>-download.json`. For boot
modes with more than one media path, pass `--kind` with `installer-image`,
`kernel`, `initrd`, or `macos-restore-image` to choose which destination to
plan.

`bridgevm media download ubuntu-template-dev` executes the recorded download
plan. It reads `.vmbridge/metadata/boot-media/<kind>-download.json`, downloads
the planned HTTP(S) URL with curl into a temporary file, checks the planned
SHA-256 before placement when one is present, moves a successful download to
the resolved destination, and records the result under
`.vmbridge/metadata/boot-media/<kind>-download-result.json`. For boot modes
with more than one media path, pass `--kind` with `installer-image`, `kernel`,
`initrd`, or `macos-restore-image` to choose which plan to run.

`bridgevm media verify ubuntu-template-dev --sha256 <hex>` verifies the
resolved Fast Mode boot media file after import by computing its SHA-256 digest
and comparing it with the expected digest provided by the user. The result is
recorded under `.vmbridge/metadata/boot-media/<kind>-verify.json`. It does not
download media. For boot modes with more than one media path, pass `--kind`
with `installer-image`, `kernel`, `initrd`, or `macos-restore-image` to choose
which file to verify.

The printed `AppleVzLaunchSpec` resolves the same expected media path against
the `.vmbridge` bundle and reports `exists: false` until the user provides the
installer image.
