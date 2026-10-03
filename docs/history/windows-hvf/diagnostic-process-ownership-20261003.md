# Diagnostic process ownership — 2026-10-03

Evidence rank: deterministic tests, disposable local children and modeled
failure boundaries. No Windows boot, private media or live queue job was used.
The focused conclusion is `b991aeb421c439871e631839b37a4f2b62e5d6a3`,
integrated as `6cb0adde70a4fe24b93a6422162bd04cab130f4e`.
Those commits have identical Git trees. Product truth remains in
[`capabilities/windows-hvf.json`](../../../capabilities/windows-hvf.json).

## Failed experiments retained

The leader baseline imports exact frozen `955c248d` source bytes and records
their equality with `954883ec`. It found two missing refusals: an actual
already-reaped leader, and a modeled residual-group transition. A reaped
leader ends the PID reservation; it does not establish process-group absence.
This baseline does not demonstrate a PID-reuse exploit.

The D5 constructor experiment models a constructor exception after a real
disposable child starts. The child remained alive while the runner's receipt
claimed cleanup. Its harness prints a frozen `955c248d` hash but imports
checkout bytes. That printed identity does not independently authenticate
the executed checkout source. The failure is retained with this source-binding
limit; it is not described as a sealed frozen-source execution.

Intermediate regression logs have their own source scope. Failing subcases
below are not substituted for test-method totals or live observations.

| Log basename | Recorded result | SHA-256 |
| --- | --- | --- |
| `leader-ownership-baseline.log` | Two missing refusals | `49701b53275a3c6433a8118c1894ae211f23682c4c0ddada182b5826fbd1b6b1` |
| `ownership-regression-before.log` | 18 tests: 6 failures, 1 error | `ad3594c3ba3a17b316cc7ca4565a49c759502e7d5be6d29fd434ac3c64820c79` |
| `d5-constructor-uncertainty-baseline.log` | One modeled constructor scenario; disposable child remained alive | `4bb4290db186a284e975131be29e12cd9663ab8ba06d85a9246ff24d05621b50` |
| `launch-regression-before.log` | 23 tests; 4 failing subcases | `1547452f2a2c77556d24e5955e225e684369456fce2b62be90f924d9eb8ceda3` |
| `signal-boundary-review-before.log` | 24 tests; 6 failing subcases | `766dd9ff3c9f3306ce38eeaef18bc51f34eb4744ed33c29043d5bb68b43b1168` |
| `b9-prior-work-collision-before.log` | 26 tests; 1 failure | `e6d6ba7f74289d246f77c19396ec0fa84181dd6a06f8ee76410553266f9c3071` |
| `affected-winpe-first.log` | 39 tests; 1 failure | `2e52130227f7e132692df8caaef64c163c54fbfd6e6baf8f83324106b8332115` |

The WinPE regression expected a cleanup signal after leader reap. That old
expectation was wrong: the repaired contract refuses a residual group after
reap and tests a real clean exit separately. The failed run remains retained.

## Ownership and cleanup corrections

The shared stop helper keeps an owned leader unreaped through the final
nonzero group signal. It checks recorded terminal status again at each signal
boundary and sends no nonzero signal after reap. Bounded TERM/CONT/KILL and
leader wait are followed by a required absent-group observation. Darwin EPERM
for a zombie-only group is handled as uncertainty until reap and a subsequent
absence check; permission denial or a surviving group cannot prove cleanup.

D5 records that spawn was attempted before constructor entry. Losing the
child handle after that attempt leaves cleanup incomplete. It skips integrity
hashing and permission changes on media a surviving child could still write.
B9 uses the supported host diagnostic-stop request before bounded group
cleanup, preserves uncertain work, and does not delete an existing work
directory that the current invocation never owned. Missing ownership is a
refusal boundary, not permission to signal a guessed PID or process group.

## Focused proof and remaining boundary

`affected-all-final.log` contains 20 unittest suites, 160 tests and 20 OK
summaries, plus passing B6 smoke/self-tests. Its SHA-256 is
`24d70fb26ae7ca5d5123c53b883a6d98c36c8c0c6acf88abe00723b16e7f62f2`.
Six static checks exited zero; their retained log SHA-256 is
`937b967e32e4e3e25a0af8f893087a683baafe87437de53392f20e7b5fa1bafc`.
The focused conclusion record hashes to
`85ec7cfe6887d0c99c7ddda8c09f57283e91ee70f9df7df90c10519d09b6783b`.

Its six relevant source-file hashes match both conclusion commits:

| Source basename | SHA-256 |
| --- | --- |
| `check-project.sh` | `1b337f0af4a1e38b5131bd45cb5a3abc12d2f1d36e22f0e5176f908d580a0d85` |
| `guest_input_owned_group.py` | `693c69b543131d6d8bcf1cb588efad5fb5d972bd9d6c68c925a59469d0385992` |
| `guest_input_live_cleanup.py` | `e7047db93f81901fd2dfdbaf3e0a04602224d6feeff71ab116f089c99f949128` |
| `run-guest-input-live.py` | `fd76d4cf26fc48354713e82eaabd41f7e16e8bcc5c0acaec91b295cb080a8cff` |
| `run-b9-real-workload-pilot.py` | `f36118d0d9c4cfcf14011675e63381fe42b9eeae05c90e7ffeaa2ff07fa5a4ac` |
| `b9_diagnostic_stop.py` | `e7a91def27c5554f9e61d1f48f817a5de1cae1d5ce4eb9d598d7cbc79893ee19` |

This is a relevant-file binding to subsequently committed Python source,
not a complete frozen compiled-source inventory. Modeled concurrent reaping
tests establish control flow; disposable children establish the tested local
process behavior. Neither supplies an adversarial PID-reuse or live guest proof.

Integrated exact-source full-project and pushed-SHA hosted checks remain
pending for this tranche and must be recorded in the change PR. Earlier checks
for an ancestor commit do not cover these source changes. No criterion state,
threshold, promotion flag or product capability wording changes here.
