#!/usr/bin/env python3
# Fixed diagnostic test inventory; no app launch or runtime discovery.
from pathlib import Path
import sys

prefix = "BridgeVMAppUIHostTests.AppUIHostContractTests/"
methods = {
    "testAcceptsOnlyExactRequestWithoutWritingToOutput",
    "testRejectsNonemptyOrNonprivateOutputWithoutRemovingEvidence",
    "testRejectsSymlinkedParentAndOutput",
    "testCancellationMarkerRefusesWithoutChangingOutputAdmission",
    "testFixtureConstructionHasNoDomainWorkOrLibraryWrites",
    "testIncompleteObservationCannotBecomeSuccessfulCompletion",
    "testUnpackagedTestProcessRefusesBeforeAppOrFixtureCreation",
}
expected = {prefix + method for method in methods}
fixture_methods = {
    "testActualSessionCancellationAcknowledgesBeforeRetryAndPreservesFixture",
    "testDiagnosticAbortSettlesHeldValidationWithoutLaunchingOrDeletingInputs",
}
expected |= {"BridgeVMAppUIHostTests.AppUIInstallCancelFixtureTests/" + method for method in fixture_methods}
overview_methods = ("testFixtureCreatesExactlyOneInertModelPerSeededVM",)
expected |= {"BridgeVMAppUIHostTests.AppUIHostOverviewContractTests/" + method for method in overview_methods}
actual = [line.strip() for line in Path(sys.argv[1]).read_text().splitlines()
          if line.strip().startswith(("BridgeVMAppUIHostTests.", "BridgeVMControlTests.AppUIInstallCancelFixtureTests/"))]
if len(actual) != 10 or set(actual) != expected:
    raise SystemExit(f"diagnostic host discovery differs: expected {sorted(expected)!r}; got {actual!r}")
print("diagnostic host discovery: exactly seven host contracts, one overview contract and two install-cancel Apple XCTest methods")
