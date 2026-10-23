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
fixture_methods = {"testActualSessionCancellationAcknowledgesBeforeRetryAndPreservesFixture",
                   "testDiagnosticAbortSettlesHeldValidationWithoutLaunchingOrDeletingInputs"}
expected |= {"BridgeVMAppUIHostTests.AppUIInstallCancelFixtureTests/" + method for method in fixture_methods}
expected |= {"BridgeVMAppUIHostTests.AppUIHostOverviewContractTests/testFixtureCreatesExactlyOneInertModelPerSeededVM",
             "BridgeVMAppUIHostTests.AppUIHostOverviewStateContractTests/testWaitFindsEachCurrentRowOnceInTheImportFormOverview",
             "BridgeVMAppUIHostTests.AppUIHostOverviewRequestsContractTests/testPressOverviewWaitsUntilTheModelHoldsBothSeededRows",
             "BridgeVMAppUIHostTests.AppUIHostOverviewRequestsContractTests/testSeededMeansBothFixtureRowsEachExactlyOnce"}
actual = [line.strip() for line in Path(sys.argv[1]).read_text().splitlines()
          if line.strip().startswith(("BridgeVMAppUIHostTests.", "BridgeVMControlTests.AppUIInstallCancelFixtureTests/"))]
if len(actual) != 13 or set(actual) != expected:
    raise SystemExit(f"diagnostic host discovery differs: expected {sorted(expected)!r}; got {actual!r}")
print("diagnostic host discovery: exactly seven host contracts, four overview contracts and two install-cancel Apple XCTest methods")
