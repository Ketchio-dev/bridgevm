# Host report integrity, helper output and A19 fill failures — deterministic

Classification: deterministic development evidence (automated tests, evidence
rank 3). Current criterion wording and product state come from
`capabilities/windows-hvf.json`. No criterion changes; A9, A11 and A19 remain
OPEN.

## Guest text inside the host report

The probe printed guest-controlled PE CodeView paths, both in the image record
and in frame-chain summaries, and the last serial symbol lines raw into the
host's final report, ahead of its stop record. Three new tests failed on the
unmodified source; one showed a guest path that ended the report line and
printed an exact SYSTEM_OFF record. Guest-derived text there is now escaped:
printable ASCII passes and every other byte becomes `\xNN`, so each record
stays on one line. The separate symbols file still receives the raw lines.

## Shutdown and audio readers outside T17

Shell and Python readers in the installed-boot runner, the NVMe tiers, the
snapshot lifecycle, the guest-input gate and the B7 and A5 audio verifiers
still accepted a SYSTEM_OFF line or CoreAudio counters anywhere in the run
log. A new contract gave them forged logs: the stop line after a diagnostic
stop in the serial tail, in agent output, after a carriage return, or in a log
without a host report. The unmodified readers accepted most of them. They now
use one binding, available as a Python module, a CLI and a bash equivalent,
which accepts SYSTEM_OFF only from the framed final host report and reads audio
counters only from the host tail after it. The three implementations agree on
all 895 retained report-bearing logs.

Retained logs from probes older than `9c4e2858` use a legacy serial-count
format that the binding does not accept, so re-running the A5, B7, NVMe or
snapshot live verifiers requires a probe built at or after that commit. The
retained vTPM evidence check still uses its original reader because it
re-verifies legacy evidence.

## Helper output

The swtpm team check read codesign's standard error to the end while its
standard output went to a pipe that nothing read; a stand-in codesign that
wrote 200 KB to standard output blocked it until a 15-second watchdog fired.
Standard output now goes to `/dev/null`. The media import helper already
drained its output but kept all of it; a stand-in writing 4 MiB and exiting 0
passed as an import. It now uses the shared runner with a fail-closed 1 MiB
cap. The real helper prints at most about 250 bytes.

## A19 fill failures and interruption stop points

On the unmodified source, a snapshot create whose copy failed after staging
was claimed left the full staged disk behind, and a restore that failed during
a streaming copy left its partial copy until the next restore. Both now clear
the staging they own under the existing debris rules, which remove only files
BridgeVM writes there and refuse foreign entries. A child-process test that
exits after the first streaming chunk passed unchanged: the old pair stayed
selected and a retry succeeded.

The T22 interrupted-restore contract now declares two further typed stop
points, a restore over an existing selected generation and a snapshot create
killed while its staged disk is hashed. Each is counted only when its recorded
content hashes agree. These are synthetic contracts; the live T22 runner still
exercises only the first-restore stop point.

## Limit

None of these changes has run on physical hardware beyond what earlier live
receipts record. No live sample, interruption case or lifecycle count is
added.
