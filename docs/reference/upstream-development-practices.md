<a name="qemuutm-개발-절차-참고-기록"></a>

# QEMU and UTM development practices

Document status: **Reference**

Last reviewed: 2026-09-06

<a name="조사-범위"></a>

## Scope of the review

At the operator’s request, we reviewed the projects’ official testing and
contribution guides. We compared how they divide work, verify changes and
manage experiment files. The observations cover what the guides document. We did
not survey every internal practice or use implementation code or device
behavior as development inputs. BridgeVM kept its independent implementation and
provenance policies and its existing licenses. Current contributors should
follow the repository’s [AGENTS.md](../../AGENTS.md).

At the time, the product-name check rejected any mention of UTM outside
operator records, including this requested process comparison. We added an
exception for this exact file path. The check still covers runtime code and
other documents. A separate check detects unsupported claims that code came
from another product. Four synthetic policy tests verified the exceptions for
this document and operator records, and rejection of runtime code, other
documents and similarly named files.

<a name="공식-문서에서-확인한-사항"></a>

## Findings from the official documentation

| Topic | What the project documented | Recommendation for BridgeVM |
| --- | --- | --- |
| QEMU test layers | Unit tests, QTest and block I/O tests cover different layers. | Run the fast tests for the layer that changed first. Verify claims about guest behavior in a separate live guest run. |
| QEMU temporary files | Functional tests clean up scratch files by default. An environment variable can retain them for debugging. | Before removing output disks, confirm the run has ended, extract the evidence and check whether anything still depends on the files. |
| QEMU logs and inputs | Execution logs are stored separately. Downloaded inputs are cached using their URL and SHA-256. | Give reusable inputs, run outputs and original evidence distinct storage locations and retention periods. |
| QEMU test cost | Quick and thorough runs are distinct. Large or long-running jobs can be selected explicitly. | Separate routine development from expensive experiments while preserving the release sample sizes and criteria. |
| QEMU CI | Most checks run in GitLab, with checks before merge. The project also documents custom runners. | Keep deterministic checks on GitHub-hosted runners and physical Mac work in its separate queue. |
| UTM change scope | Its contribution guide calls for discussion before large changes, small PRs, reasons for each change and a record of related issues. | Keep the experiment hypothesis, validation results and conclusion together as one unit of work. |
| UTM verification of AI contributions | A person must verify an AI-authored change before opening a PR and identify the device and OS used. | Record who or what ran the checks and in which environment. Verify user-visible behavior by exercising it directly. |
| UTM temporary diagnostics | The guide asks contributors to avoid unrelated edits and logs and to remove diagnostics used only during bring-up before committing. | Preserve the diagnostic results needed as evidence, then remove temporary instrumentation. |
| UTM dependencies | The guide describes managing patches with upstream in mind and documenting where dependencies come from. | Keep records of dependency versions, licenses and patch origins. Preserve modified checkouts when clearing caches. |

The QEMU testing guide describes the test layers. Its functional testing guide
covers scratch files, logs and assets, and its CI guide describes the runner
and merge process. The testing documentation identified itself as version
11.1.50 on the review date. The Avocado terminology belongs to the older
`devel/ci.html` page. [Testing in QEMU](https://www.qemu.org/docs/master/devel/testing/main.html),
[Functional testing with Python](https://www.qemu.org/docs/master/devel/testing/functional.html),
[Continuous Integration](https://www.qemu.org/docs/master/devel/testing/ci.html)

The UTM findings come from its official contribution guide. The review did
not cover every CI workflow, artifact expiration period or internal disk
deletion policy. The evidence leaves open whether UTM deletes experiment
disks after a fixed period or keeps only documentation.
[CONTRIBUTING.md](https://github.com/utmapp/UTM/blob/main/CONTRIBUTING.md)

<a name="그대로-채택하지-않는-부분"></a>

## Limits on applying these practices

QEMU’s default scratch cleanup concerns temporary test output. BridgeVM’s
canonical Windows images and users’ VMs must be protected from cleanup.
Publicly downloadable test assets and private Windows media have different
redistribution and recovery conditions. BridgeVM’s rules prohibit attaching a
private Mac runner to the public repository, even though QEMU documents
custom runners.

Other projects may make slow or unstable tests optional. BridgeVM’s release
criteria still require every sample to run and every failure to count in the
result. Shipping evidence comes from the applicable release tier and hosted CI
for the exact commit being shipped.

UTM’s human verification requirement applies to contributions to UTM.
Introducing a human approval requirement or an attribution format in BridgeVM
would require a separate, explicit policy change.

<a name="이번에-적용한-것과-이후-제안"></a>

## Work completed and recommendations

For the September cleanup, we preserved the inputs and original evidence,
kept representative cases from two completed campaigns, then removed output
clones. The repository record includes the hash list, per-file journal,
checks before and after deletion, measured disk space and the limits of
recovery. In response to a further request, we built a cleanup tool with
separate planning and execution modes for the two verified harness formats
and added deterministic tests. One-off local scripts performed the two
earlier cleanup runs. We verified the new tool on actual experiment files by
running its plan mode, which left the files in place.

We recommended recording the hypothesis and criteria before an experiment,
running the relevant deterministic checks, and using a live guest when
needed. Then assess the complete results, extract the evidence and review
what to retain. This was BridgeVM’s proposal following the review. The
research did not establish that either upstream project follows this sequence
or uses the same retention policy.

The results of the cleanup are in the
[September 6, 2026 record](../history/windows-hvf/experiment-storage-cleanup-20260906.md).
