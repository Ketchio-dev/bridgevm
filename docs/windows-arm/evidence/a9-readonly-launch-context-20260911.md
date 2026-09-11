# A9 readonly launch-context comparison: 2026-09-11

This is historical diagnostic evidence, not a product A9 pass.
No TCC grants, resets, database edits, registered-helper replacement, app launch,
or VM launch were performed for this experiment.

## Method

The standalone --accessibility-diagnostic mode calls AXIsProcessTrusted without
requesting a prompt and emits observation_only=true and criterion_pass=false.
One debug executable was copied outside Desktop and run at that identical path
both directly from a shell and through a temporary user LaunchAgent.
The copied and original executable SHA256 was
`eb0b2351ab34076aa85ac20fb4de808c0b58112807cc367d71dcabd2572048e7`.

Both observations reported on-disk code CDHash
`3a7a18b8bb0bc68ef34ba83460f8ae9e80ce1391`, the same code identifier and canonical
bundle-path digest. This metadata is not signature validation or proof of the
TCC database's responsible-process attribution. The bare binary had no bundle ID.

| Venue | PID | Parent PID | Accessibility trusted | JSON SHA256 |
| --- | --- | --- | --- | --- |
| Direct shell | 53151 | 53145 | true | 6b5128eca466b53195aa8329955b9b5e182d6e6c8779a3b2569ba52e309125a0 |
| User LaunchAgent | 53163 | 1 | false | bc3853fabb24a0d16d635d067b65e0725bed650713de600e7ec12329050f6498 |

The LaunchAgent exited with code zero before it was unloaded. A false trust
observation is a successfully completed diagnostic, not an E2E test success.

## Failed and incomplete preliminary attempts

The first launch attempt was prematurely unloaded while still running and had
no JSON result. It is incomplete, not evidence of a false trust result.
The second attempt used the executable under Desktop. A one-second process
sample of PID 51316 showed dyld's mapFileReadOnly/open/__open before main.
It was unloaded without a trust result. The cause of this file-open wait was
not established; it must not be labeled an Accessibility denial.
All three temporary LaunchAgents were unloaded. Three stalled observation-shell
sessions were separately terminated and returned exit 143; none was a live VM.

## Boundary

The r3 observations isolate launch venue for this debug executable at one path.
They do not identify the stored TCC row or establish trust for the separately
registered product helper, whose executable and CDHash were left unchanged.
Direct-shell trust must not be substituted for product-launch-chain evidence.
A9 remains OPEN. Raw JSON and the preliminary failed attempts remain private
under the operator's codex-ten-hour-20260910 work directory.
