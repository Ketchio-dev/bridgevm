# A9 WinPE installation checkpoints

## Scope

This record preserves physical T17 jobs `t17-cdb6940c-source-lock-pilot-r13`
and `t17-87b2db1b-winpe-comparator-pilot-r14`. Neither job promotes A9, A11,
the product state, or any graphics claim. Both used the supported 3D-off path.

## R13: missing WinPE comparator

R13 ran exact main `cdb6940ce0f8a87565127baa0ed0dfb20046ad79`, whose 45 hosted push
workflows passed, including CI `35488319044`. The Apple Development app used
Team ID `CP9SP67S98`, CDHash `fd8bb984ad69171f840a45eb380a9c16eec81c43`,
and input manifest SHA-256 `738ad9b27efe81345a512b68811d3034ccef2c80cdc9d1f7251a7b29ace8be08`.
Strict receipts verified at SHA-256 `5790e48f2d51d5c63bdd70c7916fefdc99dc693d5c987d349286d3f26118ef52`.

The failed receipt records `installer-failed`, one artifact-preflight pass, one
exact VM-creation pass, zero later passes, and verified cleanup. The framebuffer
showed DISM at 100% and all three driver packages installed, followed by `fc`
not found and a receipt-copy mismatch before `bcdboot`. Read-only target
inspection found neither the Microsoft boot file nor BridgeVM success marker.
The host recorded 233 completed writes and 13 flushes with no pending
completion; this proves target I/O only, not installation.

Source `0caef866610399ef13836c3dee7b9821004697ef` replaced `fc` with a
fail-closed app-owned ARM64 Windows comparator injected into boot.wim image 2.
Its complete project check passed at log SHA-256
`1069d530161254243a4200cdd3286a7ad2fe18ee8a31d6669ab67fc0ab7c7995`.

## R14: transient completion UI

R14 ran exact main `87b2db1b0257b1690d2fd133e3eb101c356cf970`, whose 45 hosted push
workflows passed. The signed artifact had Team ID `CP9SP67S98`, CDHash
`875c62de3be26c554aab292288abb8025e70e55e`, and input manifest SHA-256
`26350c6217e447bfdd49d2f6079c4f142d84febef6c5e9c432f3dfe43d95ada8`.
Strict public/private receipts both verified at SHA-256
`d8d81aa4d4fa3483f811ad30ba5726007840f22bfc3be004333596b68fbfd18e`;
the authenticated lane result SHA-256 is
`05058d63d4c8274b688e7b48aeeb9f604c8aef4ffc5b8df28e1b8ccd681630c7`.

The direct install marker recorded payload roles storage, serial and network
plus `bcdboot=complete`; the app published `installPending=false`, a 64 GiB
target, UEFI vars and `hvf-install-done.json`, then displayed the stopped
installed VM runtime view. The helper nevertheless remained in
`installWindows` until its 1,800-second bound because the transient installer
`완료` node had already left the AX graph. The strict failed receipt therefore
records `installer-failed`, one artifact-preflight pass, one VM-creation pass,
zero official later passes, failure detail `product install did not reach a
terminal UI stage`, and verified cleanup. It does not claim installed boot or
any guest journey stage.

Source `f500ba6d372020c485e7273f33f635277bd4679e` accepts the exact
`bridgevm.windows.runtime.view` transition as a terminal UI observation; the
unchanged next step still requires `installPending=false`, disk, vars and
`hvf-install-done.json` before proving installation. Source head
`c5b41a98f363af7e2dc9d595477f0cac84fc6b14` also makes the release-override
gate inspect Xcode 27 Swift-build objects. Focused tests and the complete local
project check passed; log SHA-256 is
`211b56690c3a3a9573ca636ec7cfa60c5d572a841fb6a9f5093add89a3756530`.
A new exact-source signed physical pilot remains required. A9 and A11 remain OPEN.
