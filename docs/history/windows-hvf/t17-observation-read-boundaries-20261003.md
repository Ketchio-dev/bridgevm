# T17 report and manifest read boundaries — 2026-10-03

Classification: deterministic owned fixtures and static review. No actual
LaunchServices, Accessibility, TCC, product UI or guest outcome is measured.
Product state and criterion policy remain in `capabilities/windows-hvf.json`.

## Failed existing validator boundaries

Frozen source `4ea7ca108465a015a9f9d6a49a2bf36f96ae165f` accepted conflicting
JSON accessibility flags by retaining the last key. An owned stat/read model
substituted a 41,681-byte file after lstat checked a 721-byte inode, bypassing
its 32 KiB bound. These prove parser/file-read defects in controlled fixtures;
they do not demonstrate a foreign-UID or live LaunchServices attack.

Other baseline assertions accepted nonfinite JSON constants and integer
boolean flags. Changing the manifest during a mocked observation selected a
different valid app for the final check. The frozen helper/report therefore
could disagree with the later selector in this controlled model.
Baseline ten methods produced nine failed subcases and two AttributeErrors
for integer/bool PID fields. These errors were not controlled refusals.
Raw log SHA-256:
`9db306430a1130120ae4586d4d5679fb3753ea85ba0da3484a7ab174f392b490`.
The initial test source hash was not captured. The final twelve methods add
before-launch selection refusal and descriptor-open cleanup cases; they are
not relabeled as ten baseline methods or as live report-origin evidence.

## Resulting observation behavior

Report reads open nofollow/nonblock and validate owned regular-file metadata
on the descriptor. A limit-plus-one read enforces 32 KiB and detects growth;
metadata stability checks reject substitution/mutation. Strict UTF-8 JSON
rejects duplicate keys and nonfinite values; booleans and identity strings
must have their declared types. PID fields require ASCII digit strings.
The preflight captures one manifest into an owned read-only snapshot and
checks original bytes before and after observing that selected helper.

Existing exact app/helper hashes, fixed LaunchServices app path and observation
scopes remain. PID/CDHash shape is not independent OS-process, signature or
TCC attestation. No additional CDHash matching API is introduced. This repair
does not establish or fix a later queue-copy race after preflight returns.

## Focused source proof

Retained development source SHA
`29a65a445f38ffeeef800edc231c6bfd40eec0db` is integrated as `a2d8e2df`.
Twelve reader methods, five existing admission methods and two existing report
origin methods passed, plus one self-test. Raw final log SHA-256:
`1bef6ffe28dfddc72bfaadab269cbe403f152902b536e890e22f2ee29b9a983b`.
The valid borrowed app/manifest fixture exercises the real parser, hashing and
preflight; only the LaunchServices invocation is mocked. Independent review
replayed nineteen methods and the self-test with no required change.
All five standalone changed-file hashes match the committed source. Four
implementation/gate/test files also match the integration exactly; its budget
union preserves preceding work. New modules register actual 79/199 lines,
parent ceiling decreases to 187, and no existing ceiling increases.
Python/shell/structural statics passed. Full integrated/project/hosted checks
and a fresh matching artifact precede any genuine hardware pilot. The existing
4ea build/input preparation remains retained; it is not relabeled as this source.
