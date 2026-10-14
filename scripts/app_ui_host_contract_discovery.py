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
actual = [line.strip() for line in Path(sys.argv[1]).read_text().splitlines()
          if line.strip().startswith("BridgeVMAppUIHostTests.")]
if len(actual) != 7 or set(actual) != expected:
    raise SystemExit(f"diagnostic host discovery differs: expected {sorted(expected)!r}; got {actual!r}")
print("diagnostic host discovery: exactly seven qualified Apple XCTest methods")

fixture_prefix = "BridgeVMControlTests.AppUIInstallCancelFixtureTests/"
fixture_methods = {
    "testActualSessionCancellationAcknowledgesBeforeRetryAndPreservesFixture",
    "testDiagnosticAbortSettlesHeldValidationWithoutLaunchingOrDeletingInputs",
}
fixture_expected = {fixture_prefix + method for method in fixture_methods}
fixture_actual = [line.strip() for line in Path(sys.argv[1]).read_text().splitlines()
                  if line.strip().startswith(fixture_prefix)]
if len(fixture_actual) != 2 or set(fixture_actual) != fixture_expected:
    raise SystemExit(f"install-cancel discovery differs: expected {sorted(fixture_expected)!r}; got {fixture_actual!r}")
print("install-cancel discovery: exactly two qualified Apple XCTest methods")
