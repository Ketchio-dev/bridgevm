# Development owned-pair preparation and queue admission — 2026-10-03

Evidence rank: deterministic owned-fixture tests and source review. Integrated
whole-project, exact-source hosted CI and physical preparation remain pending
at this checkpoint. This change has produced zero ready installed pairs.

The `d10-t22-owned-pair-preparation` tier prepares development inputs for a
future T22 run. Its receipts always retain `pass=false`, `claim_eligible=false`
and `criterion_pass=false`. Preparation completion is not a release gate.
Product truth remains in the [capability registry](../../../capabilities/windows-hvf.json).

## Input and ownership boundaries

Preparation requires authenticated native input and retained origin seals.
Canonical media stays immutable. The owned disk clone and separate vars clone
use same-volume `cp -c`; cross-volume sources are refused and must first be
staged into an internal cache. The fixed packaged boot uses no vTPM.
Boot writes these owned clones before encryption can be observed; this is not
pre-write clearance or proof of independence from a future TPM configuration.

The copied guest query uses the existing service, share, PowerShell file and
CIM process protocol. Admission requires successful provider return values,
one OS volume and all fixed NTFS volumes fully decrypted with encryption method
and protection status zero. Protection being off alone is insufficient.
No decrypt operation or recovery-key/protector extraction is introduced.

Successful preparation requires retained query facts, natural shutdown,
owned launch identities, terminal and process-group absence observations,
output hashes and an authenticated prepared manifest. Missing or uncertain
proof preserves the running job and cleanup fence rather than releasing it.
Each process action belongs to a retained launch; guessed PIDs are not signaled.
Private outputs remain outside source and CI artifacts.

## Preserved admission failures and repairs

Supported queue operations now admit the exact clean source before repository
Python imports. The investigation retained these failed owned experiments:

| Failed observation | Successor behavior |
| --- | --- |
| Dirty verifier code could run before the clean-source gate | Fixed system Git admission precedes repository Python |
| Ignored valid bytecode bypassed a clean Git status | Bounded flat import-root inventory also refuses caches |
| Case-alias cache names bypassed inventory on case-insensitive APFS | Cache matching folds case |
| A supported archive read created caches and blocked later admission | Supported interpreter entry points suppress bytecode writes |
| Existing archive caches ran before receipt validation | D10 archive routing reaches source admission before local imports |
| A cached stdlib-name collision ran before bootstrap admission | Bootstrap and router start in isolated Python mode |

The stdlib collision fixture only raises a known owned error. The failed
baseline and successor replay are retained; an arbitrary nonzero exit cannot
satisfy the regression's explicit cache-refusal and empty-output assertions.
Unrelated historical receipt readers retain their prior reader semantics.

Admission checks are dated point observations, not continuous same-owner
immutability. Trusted system Git/Perl and selected interpreter/system-site trust
remain prerequisites. Legitimate repository caches are also refused. Reading
a D10 archive requires a matching clean cache-free source checkout; no generic
receipt fallback supplies missing D10 proof.

## Deterministic evidence and remaining work

The frozen v6 A19 developer run passed 26 suites and 184 tests, with zero
failed suite terminals. Its raw SHA-256 is
`261e51d68c2372a313c46646d095a4cae20182a5e22ac3b7a6dd30da81387de9`.
The final v7 source differs only by removal of a helper's EOF blank line and
lowering its registered ceiling from 50 to 49. The failed staged whitespace
check remains recorded. All 18 tests in its three affected helper suites,
the final staged diff and budget checks passed after that repair.

The queue integration found 31 protected paths, including release collection,
cleanup and the guest query, byte-identical to its parent. The fixed boot
function is unchanged by extraction; the 123-line query retains CRLF endings.
Existing ceilings were not raised and no budget rows were removed.

Earlier preparation checks included 27 host contracts and 72 mocked
PowerShell 7 checks. These do not prove Windows provider or VM behavior.
The added GitHub-hosted workflow runs the mocked query contracts under Windows
PowerShell 5.1 and PowerShell 7; its actual results remain pending here.
Physical guest/provider behavior, natural shutdown and retained-pair reuse
still require live measurements. No criterion or product state is promoted.
