import Foundation
import XCTest

@testable import BridgeVMApp

private func jsonStringOrNull(_ value: String?) -> String {
  guard let value else {
    return "null"
  }

  let data = try? JSONEncoder().encode(value)
  return data.flatMap { String(data: $0, encoding: .utf8) } ?? "null"
}

private func performanceBaselineJSON(type: String) -> String {
  """
  {
    "type": "\(type)",
    "baseline": {
      "vm": "dev",
      "source": "/tmp/dev.vmbridge",
      "output": "/tmp/performance/bridgevm-performance-dev-1710000600",
      "artifact": "/tmp/performance/bridgevm-performance-dev-1710000600/performance-baseline.json",
      \(performanceBaselineFieldsJSON(createdAtUnix: 1_710_000_600))
    }
  }
  """
}

private func performanceBaselineFieldsJSON(createdAtUnix: UInt64) -> String {
  """
  "created_at_unix": \(createdAtUnix),
  "metadata_only": true,
  "state": {
    "state": "running",
    "updated_at_unix": 1710000000
  },
  "runner": {
    "engine": "qemu",
    "pid": 1234,
    "command": ["qemu-system-aarch64", "-name", "dev"],
    "log_path": "/tmp/dev.vmbridge/logs/qemu.log",
    "started_at_unix": 1709999900,
    "dry_run": true
  },
  "guest_tools": {
    "vm": "dev",
    "tools": "optional",
    "token_created_at_unix": 1709999800,
    "capabilities": [
      {"name": "guest-metrics", "max_version": 1, "enabled_by": "diagnostics"}
    ],
    "approved_shared_folders": [],
    "runtime": {
      "connected": true,
      "guest_os": "linux",
      "agent_version": "0.1.0",
      "capabilities": ["guest-metrics"],
      "last_heartbeat_at_unix": 1710000550,
      "guest_ip_addresses": [],
      "shared_folders": [],
      "metrics": {
        "cpu_percent": 7,
        "memory_used_mib": 2048,
        "updated_at_unix": 1710000540
      },
      "updated_at_unix": 1710000550
    }
  },
  "metrics": {
    "cpu_percent": 7,
    "memory_used_mib": 2048,
    "updated_at_unix": 1710000540
  },
  "measurements": [
    {
      "name": "guest_cpu_percent",
      "value": 7,
      "unit": "percent",
      "source": "guest_tools.metrics.cpu_percent",
      "metadata_only": true
    }
  ],
  "notes": [
    "metadata-only baseline; no active benchmark workloads were executed",
    "captures existing VM state, runner metadata, and guest-tools runtime metrics"
  ]
  """
}

final class DaemonDTOTests: XCTestCase {
  func testNDJSONAccumulatorRejectsResponseBeyondLimit() throws {
    var accumulator = NDJSONLineAccumulator(maximumByteCount: 5)
    XCTAssertNil(try accumulator.append(Data("abc".utf8)))
    XCTAssertThrowsError(try accumulator.append(Data("def\nignored".utf8))) { error in
      guard case DaemonTransportError.responseTooLarge(let limit) = error else {
        return XCTFail("unexpected error: \(error)")
      }
      XCTAssertEqual(limit, 5)
    }
  }

  func testNDJSONAccumulatorReturnsOnlyBoundedLineBeforeNewline() throws {
    var accumulator = NDJSONLineAccumulator(maximumByteCount: 5)
    XCTAssertNil(try accumulator.append(Data("ab".utf8)))
    XCTAssertEqual(try accumulator.append(Data("cde\ntrailing".utf8)), Data("abcde".utf8))
  }

  func testDaemonRequestEncodingIncludesNewlineAndEnforcesTotalLimit() throws {
    let request = ["value": "abc"]
    let encoded = try UnixSocketNDJSONTransport.encodeRequest(request, maximumByteCount: 32)
    XCTAssertEqual(encoded.last, 0x0A)

    XCTAssertThrowsError(
      try UnixSocketNDJSONTransport.encodeRequest(request, maximumByteCount: encoded.count - 1)
    ) { error in
      guard case DaemonTransportError.requestTooLarge(let limit) = error else {
        return XCTFail("unexpected error: \(error)")
      }
      XCTAssertEqual(limit, encoded.count - 1)
    }
  }

  func testDaemonCodecHelpersRemainDeterministicUnderParallelUse() async throws {
    let responseData = Data(
      #"{"type":"doctor","store_root":"/tmp/store","vms_dir":"/tmp/store/vms","status":"OK"}"#.utf8
    )
    try await withThrowingTaskGroup(of: Void.self) { group in
      for index in 0..<256 {
        group.addTask {
          let encoded = try UnixSocketNDJSONTransport.encodeRequest(["index": index])
          guard encoded.last == 0x0A else { throw DaemonTransportError.responseEncodingInvalid }
          let response = try UnixSocketNDJSONTransport.decodeResponse(
            responseData,
            as: DaemonStoreDoctorResponse.self
          )
          guard response.status == "OK" else { throw DaemonTransportError.responseEncodingInvalid }
        }
      }
      try await group.waitForAll()
    }
  }

  func testDoctorRequestAndResponseMatchBridgeVmDaemonWireFormat() throws {
    let data = try JSONEncoder().encode(DaemonStoreDoctorRequest())
    let object = try JSONSerialization.jsonObject(with: data) as? [String: String]
    XCTAssertEqual(object, ["type": "doctor"])

    let json = """
      {
        "type": "doctor",
        "store_root": "/Users/dev/Library/Application Support/BridgeVM",
        "vms_dir": "/Users/dev/Library/Application Support/BridgeVM/vms",
        "status": "OK"
      }
      """

    let response = try JSONDecoder().decode(
      DaemonStoreDoctorResponse.self,
      from: Data(json.utf8)
    )

    XCTAssertEqual(response.storeRoot, "/Users/dev/Library/Application Support/BridgeVM")
    XCTAssertEqual(response.vmsDir, "/Users/dev/Library/Application Support/BridgeVM/vms")
    XCTAssertEqual(response.status, "OK")
  }

  func testListRequestMatchesBridgeVmDaemonWireFormat() throws {
    let data = try JSONEncoder().encode(DaemonListVirtualMachinesRequest())
    let object = try JSONSerialization.jsonObject(with: data) as? [String: String]

    XCTAssertEqual(object, ["type": "list_vms"])
  }

  func testDaemonErrorEnvelopeSurfacesMessageBeforeResponseDecode() throws {
    let json = #"{"type":"error","message":"VM dev is already running"}"#

    XCTAssertThrowsError(
      try UnixSocketNDJSONTransport.decodeResponse(
        Data(json.utf8),
        as: DaemonStoreDoctorResponse.self
      )
    ) { error in
      guard case DaemonTransportError.daemonError(let message) = error else {
        return XCTFail("expected daemonError, got \(error)")
      }

      XCTAssertEqual(message, "VM dev is already running")
      XCTAssertEqual(error.localizedDescription, "VM dev is already running")
    }
  }

  func testDaemonRequestTimeoutsUseCategoryDefaults() {
    XCTAssertEqual(
      UnixSocketNDJSONTransport.timeoutNanoseconds(for: DaemonListVirtualMachinesRequest()),
      DaemonRequestTimeoutCategory.quick.nanoseconds
    )
    XCTAssertEqual(
      UnixSocketNDJSONTransport.timeoutNanoseconds(
        for: DaemonRunBackendRequest(name: "dev", spawn: true)
      ),
      DaemonRequestTimeoutCategory.lifecycleAction.nanoseconds
    )
    XCTAssertEqual(
      UnixSocketNDJSONTransport.timeoutNanoseconds(
        for: DaemonGuestToolsSendCommandRequest(
          name: "dev",
          command: .listApplications,
          requestID: nil
        )
      ),
      DaemonRequestTimeoutCategory.guestToolsCommand.nanoseconds
    )
    XCTAssertEqual(
      UnixSocketNDJSONTransport.timeoutNanoseconds(
        for: DaemonDownloadBootMediaRequest(name: "dev", kind: .installerImage)
      ),
      DaemonRequestTimeoutCategory.mediaOperation.nanoseconds
    )
    XCTAssertEqual(
      UnixSocketNDJSONTransport.timeoutNanoseconds(for: DaemonCompactDiskRequest(name: "dev")),
      DaemonRequestTimeoutCategory.diskOperation.nanoseconds
    )
    XCTAssertEqual(
      UnixSocketNDJSONTransport.timeoutNanoseconds(
        for: DaemonRestoreSnapshotRequest(vm: "dev", name: "before-upgrade")
      ),
      DaemonRequestTimeoutCategory.snapshotOperation.nanoseconds
    )
    XCTAssertEqual(
      UnixSocketNDJSONTransport.timeoutNanoseconds(
        for: DaemonCreatePerformanceSampleRequest(
          name: "dev",
          output: "/tmp/performance",
          artifactBytes: 1_048_576,
          iterations: 1,
          sync: true
        )
      ),
      DaemonRequestTimeoutCategory.diagnosticsOperation.nanoseconds
    )
    XCTAssertEqual(
      UnixSocketNDJSONTransport.timeoutNanoseconds(
        for: DaemonExportVirtualMachineRequest(name: "dev", output: "/tmp/dev.vmbridge")
      ),
      DaemonRequestTimeoutCategory.archiveOperation.nanoseconds
    )
  }

  func testRuntimeControlRequestAndResponseMatchBridgeVmDaemonWireFormat() throws {
    let data = try JSONEncoder().encode(
      DaemonRuntimeControlRequest(name: "dev", command: "status")
    )
    let object = try XCTUnwrap(JSONSerialization.jsonObject(with: data) as? [String: String])
    XCTAssertEqual(
      object,
      ["type": "runtime_control", "name": "dev", "command": "status"]
    )

    let json = """
      {
        "type": "runtime_control",
        "control": {
          "vm": "dev",
          "kind": "apple-vz-display",
          "socket_path": "/tmp/bvm-vz-test.sock",
          "command": "status",
          "response": {
            "ok": true,
            "state": "running",
            "display": {
              "width": 1024,
              "height": 768
            }
          }
        }
      }
      """

    let response = try JSONDecoder().decode(
      DaemonRuntimeControlResponse.self,
      from: Data(json.utf8)
    )
    let result = response.control.runtimeControlCommandResult

    XCTAssertEqual(response.type, "runtime_control")
    XCTAssertEqual(result.vm, "dev")
    XCTAssertEqual(result.kind, "apple-vz-display")
    XCTAssertEqual(result.socketPath, "/tmp/bvm-vz-test.sock")
    XCTAssertEqual(result.command, "status")
    XCTAssertEqual(
      result.response.value,
      .object([
        "display": .object([
          "height": .number("768"),
          "width": .number("1024"),
        ]),
        "ok": .bool(true),
        "state": .string("running"),
      ])
    )
  }

  func testVmListResponseMapsRustVmRecordShape() throws {
    let json = """
      {
        "type": "vm_list",
        "vms": [
          {
            "name": "legacy-linux",
            "mode": "compatibility",
            "guest_os": "ubuntu",
            "guest_arch": "x86_64",
            "state": "running",
            "store_root": "/tmp/custom-bridgevm",
            "path": "/tmp/custom-bridgevm/vms/legacy-linux.vmbridge"
          }
        ]
      }
      """

    let response = try JSONDecoder().decode(
      DaemonListVirtualMachinesResponse.self,
      from: Data(json.utf8)
    )

    XCTAssertEqual(response.virtualMachines.count, 1)
    let dto = response.virtualMachines[0]
    let vm = dto.virtualMachine
    XCTAssertEqual(vm.name, "legacy-linux")
    XCTAssertEqual(vm.guest, "ubuntu x86_64")
    XCTAssertEqual(vm.status, .running)
    XCTAssertEqual(vm.mode, .compatibility)
    XCTAssertEqual(dto.storeRoot, "/tmp/custom-bridgevm")
    XCTAssertEqual(dto.path, "/tmp/custom-bridgevm/vms/legacy-linux.vmbridge")
    XCTAssertEqual(
      dto.displayStoreMetadata,
      EmbeddedDisplayLauncher.StoreMetadata(
        storeRoot: "/tmp/custom-bridgevm",
        bundlePath: "/tmp/custom-bridgevm/vms/legacy-linux.vmbridge"
      )
    )
  }

  func testVmListResponseMapsUnknownOrMissingDaemonStateToError() throws {
    let json = """
      {
        "type": "vm_list",
        "vms": [
          {
            "name": "starting-linux",
            "mode": "fast",
            "guest_os": "ubuntu",
            "guest_arch": "arm64",
            "state": "starting",
            "path": "/tmp/starting-linux.vmbridge"
          },
          {
            "name": "missing-state",
            "mode": "compatibility",
            "guest_os": "debian",
            "guest_arch": "x86_64",
            "path": "/tmp/missing-state.vmbridge"
          }
        ]
      }
      """

    let response = try JSONDecoder().decode(
      DaemonListVirtualMachinesResponse.self,
      from: Data(json.utf8)
    )

    XCTAssertEqual(response.virtualMachines.map { $0.virtualMachine.status }, [.error, .error])
  }

  func testListTemplatesRequestAndResponseMatchBridgeVmDaemonWireFormat() throws {
    let data = try JSONEncoder().encode(DaemonListBootTemplatesRequest())
    let object = try JSONSerialization.jsonObject(with: data) as? [String: String]
    XCTAssertEqual(object, ["type": "list_templates"])

    let json = """
      {
        "type": "boot_templates",
        "templates": [
          {
            "id": "ubuntu-arm64-installer",
            "guest_os": "ubuntu",
            "guest_arch": "arm64",
            "mode": "linux-installer",
            "media_label": "ubuntu arm64 installer image",
            "source": "manual",
            "installer_image": "installers/ubuntu-arm64.iso",
            "note": "Place the installer image inside the bundle."
          }
        ]
      }
      """

    let response = try JSONDecoder().decode(
      DaemonListBootTemplatesResponse.self,
      from: Data(json.utf8)
    )

    let template = try XCTUnwrap(response.templates.first?.bootTemplate)
    XCTAssertEqual(template.id, "ubuntu-arm64-installer")
    XCTAssertEqual(template.guestTitle, "Ubuntu ARM64")
    XCTAssertEqual(template.mode, .linuxInstaller)
    XCTAssertEqual(template.engineMode, .fast)
    XCTAssertEqual(template.installerImage, "installers/ubuntu-arm64.iso")
  }

  func testRecommendModeRequestAndResponseMatchBridgeVmDaemonWireFormat() throws {
    let data = try JSONEncoder().encode(
      DaemonRecommendModeRequest(
        choice: GuestChoice(os: "windows", version: "11", arch: "arm64")
      )
    )
    let object = try JSONSerialization.jsonObject(with: data) as? [String: Any]
    let choice = try XCTUnwrap(object?["choice"] as? [String: Any])

    XCTAssertEqual(object?["type"] as? String, "recommend_mode")
    XCTAssertEqual(choice["os"] as? String, "windows")
    XCTAssertEqual(choice["version"] as? String, "11")
    XCTAssertEqual(choice["arch"] as? String, "arm64")

    let json = """
      {
        "type": "mode_recommendation",
        "recommendation": {
          "mode": "compatibility",
          "performance": "Medium; restricted QEMU/HVF path today",
          "battery_impact": "Higher than Apple VZ Fast Mode",
          "integration": "Windows beta; not Apple VZ Fast Mode",
          "message": "Windows 11 Arm uses Compatibility Mode with a restricted QEMU/HVF backend today. Apple VZ Fast Mode is Linux/macOS Arm only; BridgeVM must not claim Microsoft-authorized or Parallels-class Windows support.",
          "fast_mode_available": false
        }
      }
      """

    let response = try JSONDecoder().decode(
      DaemonModeRecommendationResponse.self,
      from: Data(json.utf8)
    )

    XCTAssertEqual(response.recommendation.mode, .compatibility)
    XCTAssertEqual(response.recommendation.batteryImpact, "Higher than Apple VZ Fast Mode")
    XCTAssertFalse(response.recommendation.fastModeAvailable)
    XCTAssertTrue(response.recommendation.message.contains("Apple VZ Fast Mode is Linux/macOS Arm only"))
  }

  func testCreateVmRequestMatchesBridgeVmDaemonWireFormat() throws {
    let template = BootTemplate(
      id: "ubuntu-arm64-installer",
      guestOS: "ubuntu",
      guestVersion: nil,
      guestArch: "arm64",
      mode: .linuxInstaller,
      mediaLabel: "ubuntu arm64 installer image",
      source: "manual",
      installerImage: "installers/ubuntu-arm64.iso",
      kernelPath: nil,
      initrdPath: nil,
      kernelCommandLine: nil,
      macosRestoreImage: nil,
      note: "Place the installer image inside the bundle."
    )

    let request = DaemonCreateVirtualMachineRequest(
      createRequest: CreateVirtualMachineRequest(name: "Dev VM", template: template)
    )
    let object =
      try JSONSerialization.jsonObject(
        with: JSONEncoder().encode(request)
      ) as? [String: Any]
    let manifest = try XCTUnwrap(object?["manifest"] as? [String: Any])
    let guest = try XCTUnwrap(manifest["guest"] as? [String: Any])
    let boot = try XCTUnwrap(manifest["boot"] as? [String: Any])
    let display = try XCTUnwrap(manifest["display"] as? [String: Any])
    let integration = try XCTUnwrap(manifest["integration"] as? [String: Any])

    XCTAssertEqual(object?["type"] as? String, "create_vm")
    XCTAssertEqual(manifest["schemaVersion"] as? String, "bridgevm.io/v1")
    XCTAssertEqual(manifest["name"] as? String, "Dev VM")
    XCTAssertEqual(manifest["mode"] as? String, "fast")
    XCTAssertEqual(guest["os"] as? String, "ubuntu")
    XCTAssertEqual(guest["arch"] as? String, "arm64")
    XCTAssertEqual(display["framePolicy"] as? String, "adaptive")
    XCTAssertEqual(boot["mode"] as? String, "linux-installer")
    XCTAssertEqual(boot["installerImage"] as? String, "installers/ubuntu-arm64.iso")
    XCTAssertEqual(integration["dragDrop"] as? Bool, true)
    XCTAssertEqual(integration["dynamicResolution"] as? Bool, true)
    XCTAssertEqual(integration["sharedFolders"] as? Bool, true)
    XCTAssertEqual(integration["applications"] as? Bool, true)
    XCTAssertEqual(integration["windows"] as? Bool, true)
  }

  func testCloneVmRequestAndResponseMatchBridgeVmDaemonWireFormat() throws {
    let request =
      try JSONSerialization.jsonObject(
        with: JSONEncoder().encode(
          DaemonCloneVirtualMachineRequest(name: "dev", newName: "dev-copy", linked: false)
        )
      ) as? [String: Any]
    XCTAssertNil(request?["linked"])
    XCTAssertEqual(
      request?["type"] as? String,
      "clone_vm"
    )
    XCTAssertEqual(request?["name"] as? String, "dev")
    XCTAssertEqual(request?["new_name"] as? String, "dev-copy")

    let json = """
      {
        "type": "cloned",
        "clone": {
          "vm": "dev-copy",
          "source": "/tmp/dev.vmbridge",
          "output": "/tmp/dev-copy.vmbridge",
          "linked": false,
          "cloned_at_unix": 1710000700
        }
      }
      """

    let response = try JSONDecoder().decode(
      DaemonCloneVirtualMachineResponse.self,
      from: Data(json.utf8)
    )

    XCTAssertEqual(response.type, "cloned")
    XCTAssertEqual(response.clone.vm, "dev-copy")
    XCTAssertEqual(response.clone.source, "/tmp/dev.vmbridge")
    XCTAssertEqual(response.clone.output, "/tmp/dev-copy.vmbridge")
    XCTAssertEqual(
      response.clone.cloneVirtualMachineMetadata,
      CloneVirtualMachineMetadata(
        vm: "dev-copy",
        source: "/tmp/dev.vmbridge",
        output: "/tmp/dev-copy.vmbridge",
        linked: false,
        backingPath: nil,
        backingFormat: nil,
        createCommand: nil,
        clonedAtUnix: 1_710_000_700
      ))
  }

  func testLinkedCloneVmRequestAndResponseMatchBridgeVmDaemonWireFormat() throws {
    let request =
      try JSONSerialization.jsonObject(
        with: JSONEncoder().encode(
          DaemonCloneVirtualMachineRequest(name: "dev", newName: "dev-linked", linked: true)
        )
      ) as? [String: Any]
    XCTAssertEqual(request?["type"] as? String, "clone_vm")
    XCTAssertEqual(request?["name"] as? String, "dev")
    XCTAssertEqual(request?["new_name"] as? String, "dev-linked")
    XCTAssertEqual(request?["linked"] as? Bool, true)

    let json = """
      {
        "type": "cloned",
        "clone": {
          "vm": "dev-linked",
          "source": "/tmp/dev.vmbridge",
          "output": "/tmp/dev-linked.vmbridge",
          "linked": true,
          "backing_path": "/tmp/dev.vmbridge/disks/dev.qcow2",
          "backing_format": "qcow2",
          "create_command": [
            "qemu-img",
            "create",
            "-f",
            "qcow2",
            "-F",
            "qcow2",
            "-b",
            "/tmp/dev.vmbridge/disks/dev.qcow2",
            "/tmp/dev-linked.vmbridge/disks/dev-linked.qcow2"
          ],
          "cloned_at_unix": 1710000701
        }
      }
      """

    let response = try JSONDecoder().decode(
      DaemonCloneVirtualMachineResponse.self,
      from: Data(json.utf8)
    )

    XCTAssertEqual(response.type, "cloned")
    XCTAssertEqual(response.clone.vm, "dev-linked")
    XCTAssertEqual(response.clone.source, "/tmp/dev.vmbridge")
    XCTAssertEqual(response.clone.output, "/tmp/dev-linked.vmbridge")
    XCTAssertEqual(
      response.clone.cloneVirtualMachineMetadata,
      CloneVirtualMachineMetadata(
        vm: "dev-linked",
        source: "/tmp/dev.vmbridge",
        output: "/tmp/dev-linked.vmbridge",
        linked: true,
        backingPath: "/tmp/dev.vmbridge/disks/dev.qcow2",
        backingFormat: "qcow2",
        createCommand: [
          "qemu-img",
          "create",
          "-f",
          "qcow2",
          "-F",
          "qcow2",
          "-b",
          "/tmp/dev.vmbridge/disks/dev.qcow2",
          "/tmp/dev-linked.vmbridge/disks/dev-linked.qcow2",
        ],
        clonedAtUnix: 1_710_000_701
      ))
  }

  func testExportImportVmRequestsAndResponsesMatchBridgeVmDaemonWireFormat() throws {
    let exportRequest =
      try JSONSerialization.jsonObject(
        with: JSONEncoder().encode(
          DaemonExportVirtualMachineRequest(name: "dev", output: "/tmp/dev.vmbridge")
        )
      ) as? [String: String]
    XCTAssertEqual(
      exportRequest,
      [
        "type": "export_vm",
        "name": "dev",
        "output": "/tmp/dev.vmbridge",
      ])

    let exportJSON = """
      {
        "type": "exported",
        "export": {
          "vm": "dev",
          "source": "/tmp/store/dev.vmbridge",
          "output": "/tmp/dev.vmbridge",
          "archive_format": "directory",
          "copied_file_count": 3,
          "copied_files": ["manifest.yaml", "metadata/state.json", "metadata/runtime.json"],
          "manifest_preserved": true,
          "metadata_preserved": true,
          "exported_at_unix": 1710000000
        }
      }
      """

    let exportResponse = try JSONDecoder().decode(
      DaemonExportVirtualMachineResponse.self,
      from: Data(exportJSON.utf8)
    )
    XCTAssertEqual(exportResponse.type, "exported")
    XCTAssertEqual(
      exportResponse.export.vmExportMetadata,
      VMExportMetadata(
        vm: "dev",
        source: "/tmp/store/dev.vmbridge",
        output: "/tmp/dev.vmbridge",
        archiveFormat: "directory",
        copiedFileCount: 3,
        copiedFiles: ["manifest.yaml", "metadata/state.json", "metadata/runtime.json"],
        manifestPreserved: true,
        metadataPreserved: true,
        exportedAtUnix: 1_710_000_000
      ))

    let importRequest =
      try JSONSerialization.jsonObject(
        with: JSONEncoder().encode(
          DaemonImportVirtualMachineRequest(input: "/tmp/dev.vmbridge", name: "dev-copy")
        )
      ) as? [String: String]
    XCTAssertEqual(
      importRequest,
      [
        "type": "import_vm",
        "input": "/tmp/dev.vmbridge",
        "name": "dev-copy",
      ])

    let importRequestWithoutName =
      try JSONSerialization.jsonObject(
        with: JSONEncoder().encode(
          DaemonImportVirtualMachineRequest(input: "/tmp/dev.vmbridge", name: nil)
        )
      ) as? [String: Any]
    XCTAssertEqual(importRequestWithoutName?["type"] as? String, "import_vm")
    XCTAssertEqual(importRequestWithoutName?["input"] as? String, "/tmp/dev.vmbridge")
    XCTAssertTrue(importRequestWithoutName?["name"] is NSNull)

    let importJSON = """
      {
        "type": "imported",
        "import": {
          "vm": "dev-copy",
          "source": "/tmp/dev.vmbridge",
          "output": "/tmp/store/dev-copy.vmbridge",
          "archive_format": "directory",
          "copied_file_count": 3,
          "copied_files": ["manifest.yaml", "metadata/state.json", "metadata/runtime.json"],
          "manifest_preserved": true,
          "metadata_preserved": true,
          "original_name": "dev",
          "requested_name": "dev-copy",
          "manifest_identity_rewritten": true,
          "imported_at_unix": 1710000100
        }
      }
      """

    let importResponse = try JSONDecoder().decode(
      DaemonImportVirtualMachineResponse.self,
      from: Data(importJSON.utf8)
    )
    XCTAssertEqual(importResponse.type, "imported")
    XCTAssertEqual(
      importResponse.import.vmImportMetadata,
      VMImportMetadata(
        vm: "dev-copy",
        source: "/tmp/dev.vmbridge",
        output: "/tmp/store/dev-copy.vmbridge",
        archiveFormat: "directory",
        copiedFileCount: 3,
        copiedFiles: ["manifest.yaml", "metadata/state.json", "metadata/runtime.json"],
        manifestPreserved: true,
        metadataPreserved: true,
        originalName: "dev",
        requestedName: "dev-copy",
        manifestIdentityRewritten: true,
        importedAtUnix: 1_710_000_100
      ))
  }

  func testMetadataRepairRequestAndResponseMatchBridgeVmDaemonWireFormat() throws {
    let request =
      try JSONSerialization.jsonObject(
        with: JSONEncoder().encode(DaemonRepairMetadataRequest(name: "dev"))
      ) as? [String: String]
    XCTAssertEqual(
      request,
      [
        "type": "repair_metadata",
        "name": "dev",
      ])

    let json = """
      {
        "type": "metadata_repaired",
        "repair": {
          "vm": "dev",
          "bundle": "/tmp/dev.vmbridge",
          "repaired": true,
          "actions": [
            {
              "action": "repaired",
              "path": "/tmp/dev.vmbridge/metadata/runtime.json",
              "detail": "wrote runtime metadata"
            }
          ],
          "repaired_at_unix": 42
        }
      }
      """

    let response = try JSONDecoder().decode(
      DaemonMetadataRepairedResponse.self,
      from: Data(json.utf8)
    )

    XCTAssertEqual(response.type, "metadata_repaired")
    XCTAssertEqual(
      response.repair.vmMetadataRepair,
      VMMetadataRepair(
        vm: "dev",
        bundle: "/tmp/dev.vmbridge",
        repaired: true,
        actions: [
          VMMetadataRepairAction(
            action: "repaired",
            path: "/tmp/dev.vmbridge/metadata/runtime.json",
            detail: "wrote runtime metadata"
          )
        ],
        repairedAtUnix: 42
      ))
  }

  func testManifestMigrationRequestAndResponseMatchBridgeVmDaemonWireFormat() throws {
    let dryRunRequest =
      try JSONSerialization.jsonObject(
        with: JSONEncoder().encode(DaemonMigrateManifestRequest(name: "dev", dryRun: true))
      ) as? [String: Any]
    XCTAssertEqual(dryRunRequest?["type"] as? String, "migrate_manifest")
    XCTAssertEqual(dryRunRequest?["name"] as? String, "dev")
    XCTAssertEqual(dryRunRequest?["dry_run"] as? Bool, true)

    let migrateRequest =
      try JSONSerialization.jsonObject(
        with: JSONEncoder().encode(DaemonMigrateManifestRequest(name: "dev", dryRun: false))
      ) as? [String: Any]
    XCTAssertEqual(migrateRequest?["type"] as? String, "migrate_manifest")
    XCTAssertEqual(migrateRequest?["name"] as? String, "dev")
    XCTAssertEqual(migrateRequest?["dry_run"] as? Bool, false)

    let json = """
      {
        "type": "manifest_migrated",
        "migration": {
          "vm": "dev",
          "bundle": "/tmp/dev.vmbridge",
          "manifest_path": "/tmp/dev.vmbridge/manifest.yaml",
          "dry_run": false,
          "migrated": false,
          "from_schema": "bridgevm.io/v1",
          "to_schema": "bridgevm.io/v1",
          "actions": [
            "validated current manifest schema",
            "copied manifest before migration",
            "wrote migration receipt"
          ],
          "backup_path": "/tmp/dev.vmbridge/metadata/manifest-before-migration.yaml",
          "receipt_path": "/tmp/dev.vmbridge/metadata/manifest-migration.json",
          "migrated_at_unix": 1710001300
        }
      }
      """

    let response = try JSONDecoder().decode(
      DaemonManifestMigratedResponse.self,
      from: Data(json.utf8)
    )

    XCTAssertEqual(response.type, "manifest_migrated")
    XCTAssertEqual(
      response.migration.vmManifestMigration,
      VMManifestMigration(
        vm: "dev",
        bundle: "/tmp/dev.vmbridge",
        manifestPath: "/tmp/dev.vmbridge/manifest.yaml",
        dryRun: false,
        migrated: false,
        fromSchema: "bridgevm.io/v1",
        toSchema: "bridgevm.io/v1",
        actions: [
          "validated current manifest schema",
          "copied manifest before migration",
          "wrote migration receipt",
        ],
        backupPath: "/tmp/dev.vmbridge/metadata/manifest-before-migration.yaml",
        receiptPath: "/tmp/dev.vmbridge/metadata/manifest-migration.json",
        migratedAtUnix: 1_710_001_300
      ))
  }

  func testActionRequestsMatchBridgeVmDaemonWireFormat() throws {
    let suspend =
      try JSONSerialization.jsonObject(
        with: JSONEncoder().encode(DaemonSuspendBackendRequest(name: "dev"))
      ) as? [String: String]
    XCTAssertEqual(
      suspend,
      [
        "type": "suspend_backend",
        "name": "dev",
      ])

    let resume =
      try JSONSerialization.jsonObject(
        with: JSONEncoder().encode(DaemonResumeBackendRequest(name: "dev"))
      ) as? [String: String]
    XCTAssertEqual(
      resume,
      [
        "type": "resume_backend",
        "name": "dev",
      ])

    let stop =
      try JSONSerialization.jsonObject(
        with: JSONEncoder().encode(DaemonStopVirtualMachineRequest(name: "dev"))
      ) as? [String: String]
    XCTAssertEqual(
      stop,
      [
        "type": "stop_backend",
        "name": "dev",
      ])

    let run =
      try JSONSerialization.jsonObject(
        with: JSONEncoder().encode(DaemonRunBackendRequest(name: "dev", spawn: true))
      ) as? [String: Any]
    XCTAssertEqual(run?["type"] as? String, "run_backend")
    XCTAssertEqual(run?["name"] as? String, "dev")
    XCTAssertEqual(run?["spawn"] as? Bool, true)
  }

  func testLifecyclePlanRequestAndResponseMatchBridgeVmDaemonWireFormat() throws {
    let request =
      try JSONSerialization.jsonObject(
        with: JSONEncoder().encode(
          DaemonLifecyclePlanRequest(name: "dev", action: .suspend)
        )
      ) as? [String: String]
    XCTAssertEqual(
      request,
      [
        "type": "lifecycle_plan",
        "name": "dev",
        "action": "suspend",
      ])

    let json = """
      {
        "type": "lifecycle_plan",
        "plan": {
          "vm": "dev",
          "action": "suspend",
          "current_state": "running",
          "target_state": "suspended",
          "backend": "qemu-qmp",
          "metadata_only": true,
          "executable": true,
          "qmp_command": "stop",
          "socket_path": "/tmp/dev.vmbridge/run/qmp.sock",
          "socket_available": true,
          "blockers": [],
          "notes": [
            "metadata-only lifecycle plan; no backend command was sent",
            "Compatibility Mode lifecycle control maps to QMP stop/cont"
          ]
        }
      }
      """

    let response = try JSONDecoder().decode(
      DaemonLifecyclePlanResponse.self,
      from: Data(json.utf8)
    )

    XCTAssertEqual(response.type, "lifecycle_plan")
    let plan = response.plan.lifecyclePlan
    XCTAssertEqual(plan.vm, "dev")
    XCTAssertEqual(plan.action, .suspend)
    XCTAssertEqual(plan.currentState, .running)
    XCTAssertEqual(plan.targetState, .suspended)
    XCTAssertEqual(plan.backend, "qemu-qmp")
    XCTAssertTrue(plan.metadataOnly)
    XCTAssertTrue(plan.executable)
    XCTAssertEqual(plan.qmpCommand, "stop")
    XCTAssertEqual(plan.socketPath, "/tmp/dev.vmbridge/run/qmp.sock")
    XCTAssertTrue(plan.socketAvailable)
    XCTAssertTrue(plan.blockers.isEmpty)
    XCTAssertEqual(plan.notes.count, 2)
  }

  func testQMPStatusResponseDecodesSupervisorMetadataCache() throws {
    let json = """
      {
        "type": "qmp_status",
        "status": {
          "socket_path": "/tmp/dev.vmbridge/run/qmp.sock",
          "available": true,
          "status": "running",
          "running": true,
          "supervisor": {
            "events": [
              {"name": "RESUME"},
              {"name": "SHUTDOWN", "data": {"guest": true}}
            ],
            "terminal_event": {"name": "SHUTDOWN", "data": {"guest": true}},
            "envelopes_read": 2,
            "limit_reached": false,
            "updated_at_unix": 1710000700
          }
        }
      }
      """

    let response = try JSONDecoder().decode(
      DaemonQMPStatusResponse.self,
      from: Data(json.utf8)
    )

    XCTAssertEqual(response.type, "qmp_status")
    let supervisor = try XCTUnwrap(response.status.supervisor)
    XCTAssertEqual(supervisor.events.count, 2)
    XCTAssertEqual(supervisor.events.map(\.name), ["RESUME", "SHUTDOWN"])
    XCTAssertEqual(supervisor.terminalEvent?.name, "SHUTDOWN")
    XCTAssertEqual(supervisor.envelopesRead, 2)
    XCTAssertFalse(supervisor.limitReached)
    XCTAssertEqual(supervisor.updatedAtUnix, 1_710_000_700)

    let status = response.status.qmpStatus
    XCTAssertEqual(status.socketPath, "/tmp/dev.vmbridge/run/qmp.sock")
    XCTAssertTrue(status.available)
    XCTAssertEqual(status.readinessTitle, "running")
    let modelSupervisor = try XCTUnwrap(status.supervisor)
    XCTAssertEqual(modelSupervisor.events.count, 2)
    XCTAssertEqual(modelSupervisor.events.map(\.name), ["RESUME", "SHUTDOWN"])
    XCTAssertEqual(modelSupervisor.terminalEvent?.name, "SHUTDOWN")
    XCTAssertEqual(modelSupervisor.envelopesRead, 2)
    XCTAssertFalse(modelSupervisor.limitReached)
    XCTAssertEqual(modelSupervisor.updatedAtUnix, 1_710_000_700)
    XCTAssertEqual(modelSupervisor.summaryTitle, "2 events, terminal SHUTDOWN")
  }

  func testQemuArgsRequestAndResponseMatchBridgeVmDaemonWireFormat() throws {
    let request =
      try JSONSerialization.jsonObject(
        with: JSONEncoder().encode(DaemonQemuArgsRequest(name: "dev"))
      ) as? [String: String]
    XCTAssertEqual(
      request,
      [
        "type": "qemu_args",
        "name": "dev",
      ])

    let json = """
      {
        "type": "qemu_command",
        "command": {
          "program": "qemu-system-aarch64",
          "args": ["-name", "dev", "-netdev", "vmnet-host,id=net0"]
        }
      }
      """

    let response = try JSONDecoder().decode(
      DaemonQemuCommandResponse.self,
      from: Data(json.utf8)
    )

    XCTAssertEqual(response.type, "qemu_command")
    let plan = response.command.qemuLaunchPlan
    XCTAssertEqual(plan.program, "qemu-system-aarch64")
    XCTAssertEqual(plan.args, ["-name", "dev", "-netdev", "vmnet-host,id=net0"])
    XCTAssertEqual(
      plan.commandLine,
      "qemu-system-aarch64 -name dev -netdev vmnet-host,id=net0")
  }

  func testOpenPortPlanRequestAndResponseMatchBridgeVmDaemonWireFormat() throws {
    let request =
      try JSONSerialization.jsonObject(
        with: JSONEncoder().encode(
          DaemonOpenPortRequest(name: "dev", guest: 80, scheme: "https")
        )
      ) as? [String: Any]
    XCTAssertEqual(request?["type"] as? String, "open_port")
    XCTAssertEqual(request?["name"] as? String, "dev")
    XCTAssertEqual(request?["guest"] as? Int, 80)
    XCTAssertEqual(request?["scheme"] as? String, "https")

    let json = """
      {
        "type": "open_port_plan",
        "plan": {
          "vm": "dev",
          "scheme": "https",
          "host": "127.0.0.1",
          "guest_port": 80,
          "host_port": 18080,
          "url": "https://127.0.0.1:18080",
          "command": ["open", "https://127.0.0.1:18080"]
        }
      }
      """

    let response = try JSONDecoder().decode(
      DaemonOpenPortResponse.self,
      from: Data(json.utf8)
    )

    XCTAssertEqual(response.type, "open_port_plan")
    let plan = response.plan.openPortPlan
    XCTAssertEqual(plan.vm, "dev")
    XCTAssertEqual(plan.scheme, "https")
    XCTAssertEqual(plan.host, "127.0.0.1")
    XCTAssertEqual(plan.guestPort, 80)
    XCTAssertEqual(plan.hostPort, 18080)
    XCTAssertEqual(plan.url, "https://127.0.0.1:18080")
    XCTAssertEqual(plan.command, ["open", "https://127.0.0.1:18080"])
  }

  func testNetworkPlanRequestAndResponseMatchBridgeVmDaemonWireFormat() throws {
    let request =
      try JSONSerialization.jsonObject(
        with: JSONEncoder().encode(DaemonNetworkPlanRequest(name: "dev"))
      ) as? [String: String]
    XCTAssertEqual(request, ["type": "plan_network", "name": "dev"])

    let json = """
      {
        "type": "network_planned",
        "plan": {
          "vm": "dev",
          "backend": "apple-vz",
          "mode": "nat",
          "hostname": "dev",
          "dry_run": true,
          "executable": false,
          "port_forwards": [
            {
              "host": 2222,
              "guest": 22
            }
          ],
          "capabilities": {
            "guest_outbound": true,
            "host_to_guest": true,
            "guest_to_host": true,
            "host_visible_hostname": true,
            "supports_port_forwarding": true,
            "requires_privileged_helper": false
          },
          "blockers": [
            {
              "code": "missing-helper",
              "message": "Install the networking helper."
            }
          ],
          "notes": [
            "dry-run network plan; no backend launch or host networking mutation was performed"
          ]
        }
      }
      """

    let response = try JSONDecoder().decode(
      DaemonNetworkPlanResponse.self,
      from: Data(json.utf8)
    )

    XCTAssertEqual(response.type, "network_planned")
    let plan = response.plan.networkPlan
    XCTAssertEqual(plan.vm, "dev")
    XCTAssertEqual(plan.backend, "apple-vz")
    XCTAssertEqual(plan.mode, "nat")
    XCTAssertEqual(plan.hostname, "dev")
    XCTAssertTrue(plan.dryRun)
    XCTAssertFalse(plan.executable)
    XCTAssertEqual(plan.portForwards, [VMPortForward(host: 2222, guest: 22)])
    XCTAssertEqual(plan.capabilities?.guestOutbound, true)
    XCTAssertEqual(plan.capabilities?.hostToGuest, true)
    XCTAssertEqual(plan.capabilities?.requiresPrivilegedHelper, false)
    XCTAssertEqual(plan.blockers.first?.code, "missing-helper")
    XCTAssertEqual(plan.notes.count, 1)
  }

  func testSSHPlanRequestAndResponseMatchBridgeVmDaemonWireFormat() throws {
    let request =
      try JSONSerialization.jsonObject(
        with: JSONEncoder().encode(
          DaemonSSHPlanRequest(name: "dev", user: "ubuntu")
        )
      ) as? [String: Any]
    XCTAssertEqual(request?["type"] as? String, "ssh_plan")
    XCTAssertEqual(request?["name"] as? String, "dev")
    XCTAssertEqual(request?["user"] as? String, "ubuntu")

    let json = """
      {
        "type": "ssh_plan",
        "plan": {
          "vm": "dev",
          "user": "ubuntu",
          "host": "127.0.0.1",
          "port": 2222,
          "source": "port-forward",
          "command": ["ssh", "-p", "2222", "ubuntu@127.0.0.1"]
        }
      }
      """

    let response = try JSONDecoder().decode(
      DaemonSSHPlanResponse.self,
      from: Data(json.utf8)
    )

    XCTAssertEqual(response.type, "ssh_plan")
    let plan = response.plan.sshPlan
    XCTAssertEqual(plan.vm, "dev")
    XCTAssertEqual(plan.user, "ubuntu")
    XCTAssertEqual(plan.host, "127.0.0.1")
    XCTAssertEqual(plan.port, 2222)
    XCTAssertEqual(plan.source, .portForward)
    XCTAssertEqual(plan.command, ["ssh", "-p", "2222", "ubuntu@127.0.0.1"])
  }

  func testPortForwardManifestRequestsAndResponseMatchBridgeVmDaemonWireFormat() throws {
    let listRequest =
      try JSONSerialization.jsonObject(
        with: JSONEncoder().encode(DaemonListPortsRequest(name: "dev"))
      ) as? [String: String]
    XCTAssertEqual(
      listRequest,
      [
        "type": "list_ports",
        "name": "dev",
      ])

    let addRequest =
      try JSONSerialization.jsonObject(
        with: JSONEncoder().encode(DaemonAddPortRequest(name: "dev", host: 2222, guest: 22))
      ) as? [String: Any]
    XCTAssertEqual(addRequest?["type"] as? String, "add_port")
    XCTAssertEqual(addRequest?["name"] as? String, "dev")
    XCTAssertEqual(addRequest?["host"] as? Int, 2222)
    XCTAssertEqual(addRequest?["guest"] as? Int, 22)

    let removeRequest =
      try JSONSerialization.jsonObject(
        with: JSONEncoder().encode(DaemonRemovePortRequest(name: "dev", host: 2222, guest: 22))
      ) as? [String: Any]
    XCTAssertEqual(removeRequest?["type"] as? String, "remove_port")
    XCTAssertEqual(removeRequest?["name"] as? String, "dev")
    XCTAssertEqual(removeRequest?["host"] as? Int, 2222)
    XCTAssertEqual(removeRequest?["guest"] as? Int, 22)

    let json = """
      {
        "type": "port_forwards",
        "ports": {
          "vm": "dev",
          "forwards": [
            { "host": 2222, "guest": 22 },
            { "host": 3000, "guest": 3000 }
          ]
        }
      }
      """

    let response = try JSONDecoder().decode(
      DaemonPortForwardsResponse.self,
      from: Data(json.utf8)
    )

    XCTAssertEqual(response.type, "port_forwards")
    XCTAssertEqual(
      response.ports.vmPortForwardList,
      VMPortForwardList(
        vm: "dev",
        forwards: [
          VMPortForward(host: 2222, guest: 22),
          VMPortForward(host: 3000, guest: 3000),
        ]
      ))
  }

  func testSharedFolderManifestRequestsAndResponseMatchBridgeVmDaemonWireFormat() throws {
    let listRequest =
      try JSONSerialization.jsonObject(
        with: JSONEncoder().encode(DaemonListSharesRequest(name: "dev"))
      ) as? [String: String]
    XCTAssertEqual(
      listRequest,
      [
        "type": "list_shares",
        "name": "dev",
      ])

    let addRequest =
      try JSONSerialization.jsonObject(
        with: JSONEncoder().encode(
          DaemonAddShareRequest(
            name: "dev",
            share: "workspace",
            hostPath: "/Users/dev/workspace",
            readOnly: true,
            hostPathToken: "share-token-workspace"
          ))
      ) as? [String: Any]
    XCTAssertEqual(addRequest?["type"] as? String, "add_share")
    XCTAssertEqual(addRequest?["name"] as? String, "dev")
    XCTAssertEqual(addRequest?["share"] as? String, "workspace")
    XCTAssertEqual(addRequest?["host_path"] as? String, "/Users/dev/workspace")
    XCTAssertEqual(addRequest?["read_only"] as? Bool, true)
    XCTAssertEqual(addRequest?["host_path_token"] as? String, "share-token-workspace")

    let removeRequest =
      try JSONSerialization.jsonObject(
        with: JSONEncoder().encode(DaemonRemoveShareRequest(name: "dev", share: "workspace"))
      ) as? [String: String]
    XCTAssertEqual(
      removeRequest,
      [
        "type": "remove_share",
        "name": "dev",
        "share": "workspace",
      ])

    let json = """
      {
        "type": "shared_folders",
        "shares": {
          "vm": "dev",
          "shared_folders": [
            {
              "name": "workspace",
              "host_path": "/Users/dev/workspace",
              "read_only": true,
              "host_path_token": "share-token-workspace"
            }
          ]
        }
      }
      """

    let response = try JSONDecoder().decode(
      DaemonSharedFoldersResponse.self,
      from: Data(json.utf8)
    )

    XCTAssertEqual(response.type, "shared_folders")
    XCTAssertEqual(
      response.shares.vmSharedFolderList,
      VMSharedFolderList(
        vm: "dev",
        sharedFolders: [
          VMSharedFolder(
            name: "workspace",
            hostPath: "/Users/dev/workspace",
            readOnly: true,
            hostPathToken: "share-token-workspace"
          )
        ]
      ))
  }

  func testInspectBootMediaStatusRequestAndResponseMatchBridgeVmDaemonWireFormat() throws {
    let request =
      try JSONSerialization.jsonObject(
        with: JSONEncoder().encode(DaemonInspectBootMediaStatusRequest(name: "ubuntu"))
      ) as? [String: String]
    XCTAssertEqual(
      request,
      [
        "type": "inspect_boot_media_status",
        "name": "ubuntu",
      ])

    let json = """
      {
        "type": "boot_media_status",
        "status": {
          "vm": "ubuntu",
          "entries": [
            {
              "kind": "installer-image",
              "path": "installers/ubuntu-arm64.iso",
              "exists": true,
              "bytes": 14,
              "last_import": {
                "vm": "ubuntu",
                "kind": "installer-image",
                "source": "/tmp/ubuntu.iso",
                "destination": "installers/ubuntu-arm64.iso",
                "bytes": 14,
                "replaced": false,
                "imported_at_unix": 1710000000
              },
              "last_verification": {
                "vm": "ubuntu",
                "kind": "installer-image",
                "path": "installers/ubuntu-arm64.iso",
                "bytes": 14,
                "expected_sha256": "abc",
                "actual_sha256": "abc",
                "verified": true,
                "verified_at_unix": 1710000010
              },
              "last_download_plan": {
                "vm": "ubuntu",
                "kind": "installer-image",
                "url": "https://example.invalid/ubuntu.iso",
                "destination": "installers/ubuntu-arm64.iso",
                "exists": true,
                "bytes": 14,
                "expected_sha256": "abc",
                "planned_at_unix": 1710000020
              },
              "last_download": {
                "vm": "ubuntu",
                "kind": "installer-image",
                "url": "https://example.invalid/ubuntu.iso",
                "destination": "installers/ubuntu-arm64.iso",
                "temp_path": "installers/ubuntu-arm64.iso.part",
                "command": ["curl"],
                "exit_status": 0,
                "stdout": "",
                "stderr": "",
                "bytes": 14,
                "replaced": true,
                "expected_sha256": "abc",
                "actual_sha256": "abc",
                "verified": true,
                "downloaded": true,
                "downloaded_at_unix": 1710000030
              }
            }
          ]
        }
      }
      """

    let response = try JSONDecoder().decode(
      DaemonBootMediaStatusResponse.self,
      from: Data(json.utf8)
    )

    XCTAssertEqual(response.type, "boot_media_status")
    let status = response.status.bootMediaStatus
    XCTAssertEqual(status.vm, "ubuntu")
    let entry = try XCTUnwrap(status.entries.first)
    XCTAssertEqual(entry.kind, .installerImage)
    XCTAssertEqual(entry.path, "installers/ubuntu-arm64.iso")
    XCTAssertTrue(entry.exists)
    XCTAssertEqual(entry.sizeBytes, 14)
    XCTAssertEqual(entry.lastImport?.bytes, 14)
    XCTAssertEqual(entry.lastVerification?.verified, true)
    XCTAssertEqual(entry.lastDownloadPlan?.url, "https://example.invalid/ubuntu.iso")
    XCTAssertEqual(entry.lastDownload?.downloaded, true)
  }

  func testGuestToolsStatusRequestAndResponseMatchBridgeVmDaemonWireFormat() throws {
    let request =
      try JSONSerialization.jsonObject(
        with: JSONEncoder().encode(DaemonGuestToolsStatusRequest(name: "dev"))
      ) as? [String: String]
    XCTAssertEqual(
      request,
      [
        "type": "guest_tools_status",
        "name": "dev",
      ])

    let json = """
      {
        "type": "guest_tools_status",
        "status": {
          "vm": "dev",
          "tools": "required",
          "token_created_at_unix": 1710000000,
          "capabilities": [
            {
              "name": "heartbeat",
              "max_version": 1,
              "enabled_by": "base"
            }
          ],
          "approved_shared_folders": [
            {
              "name": "workspace",
              "host_path": "/Users/dev/workspace",
              "host_path_token": "host-token-1",
              "read_only": false,
              "approval": "required"
            }
          ],
          "runtime": {
            "connected": true,
            "guest_os": "ubuntu",
            "agent_version": "0.1.0",
            "capabilities": ["heartbeat"],
            "last_heartbeat_at_unix": 1710000060,
            "guest_ip_addresses": [
              {
                "address": "192.168.64.23",
                "interface": "en0"
              }
            ],
            "shared_folders": [
              {
                "name": "workspace",
                "host_path_token": "host-token-1",
                "mounted_at_unix": 1710000062
              }
            ],
            "metrics": {
              "cpu_percent": 17,
              "memory_used_mib": 512,
              "updated_at_unix": 1710000061
            },
            "last_clipboard": {
              "text": "guest copied text",
              "updated_at_unix": 1710000064
            },
            "last_command_result": {
              "request_id": "req-clipboard-1",
              "capability": "clipboard",
              "ok": true,
              "error_code": null,
              "message": "Clipboard updated",
              "result": {
                "changed": true,
                "text_length": 17
              },
              "metadata": {
                "handler": "clipboard",
                "duration_ms": 3
              },
              "completed_at_unix": 1710000062
            },
            "agent_update": {
              "current_version": "0.1.0",
              "available_version": "0.2.0",
              "download_url": "https://example.test/bridgevm-agent-0.2.0.tar.gz",
              "signature": "sha256:abc123",
              "observed_at_unix": 1710000063
            },
            "updated_at_unix": 1710000061
          }
        }
      }
      """

    let response = try JSONDecoder().decode(
      DaemonGuestToolsStatusResponse.self,
      from: Data(json.utf8)
    )

    XCTAssertEqual(response.type, "guest_tools_status")
    let status = response.status.guestToolsStatus
    XCTAssertEqual(status.vm, "dev")
    XCTAssertEqual(status.tools, "required")
    XCTAssertEqual(status.tokenCreatedAtUnix, 1_710_000_000)
    XCTAssertEqual(
      status.capabilities,
      [
        GuestToolsCapability(name: "heartbeat", maxVersion: 1, enabledBy: "base")
      ])
    XCTAssertEqual(
      status.approvedSharedFolders,
      [
        GuestToolsApprovedSharedFolder(
          name: "workspace",
          hostPath: "/Users/dev/workspace",
          hostPathToken: "host-token-1",
          readOnly: false,
          approval: "required"
        )
      ])
    XCTAssertEqual(status.approvedSharedFoldersTitle, "Approved (1)")
    let runtime = try XCTUnwrap(status.runtime)
    XCTAssertTrue(runtime.connected)
    XCTAssertEqual(runtime.guestOS, "ubuntu")
    XCTAssertEqual(runtime.agentVersion, "0.1.0")
    XCTAssertEqual(runtime.capabilities, ["heartbeat"])
    XCTAssertEqual(runtime.lastHeartbeatAtUnix, 1_710_000_060)
    XCTAssertEqual(
      runtime.guestIPAddresses,
      [
        GuestToolsIPAddress(address: "192.168.64.23", interface: "en0")
      ])
    XCTAssertEqual(
      runtime.sharedFolders,
      [
        GuestToolsSharedFolder(
          name: "workspace", hostPathToken: "host-token-1", mountedAtUnix: 1_710_000_062)
      ])
    XCTAssertEqual(
      runtime.metrics,
      GuestToolsMetrics(
        cpuPercent: 17,
        memoryUsedMiB: 512,
        updatedAtUnix: 1_710_000_061
      ))
    XCTAssertEqual(
      runtime.lastClipboard,
      GuestClipboardSnapshot(
        text: "guest copied text",
        updatedAtUnix: 1_710_000_064
      ))
    XCTAssertEqual(
      runtime.lastCommandResult,
      GuestToolsCommandResult(
        requestID: "req-clipboard-1",
        capability: "clipboard",
        ok: true,
        errorCode: nil,
        message: "Clipboard updated",
        result: GuestToolsCommandPayload(
          value: .object([
            "changed": .bool(true),
            "text_length": .number("17"),
          ])
        ),
        metadata: GuestToolsCommandPayload(
          value: .object([
            "duration_ms": .number("3"),
            "handler": .string("clipboard"),
          ])
        ),
        completedAtUnix: 1_710_000_062
      ))
    XCTAssertEqual(
      runtime.agentUpdate,
      GuestToolsAgentUpdate(
        currentVersion: "0.1.0",
        availableVersion: "0.2.0",
        downloadURL: "https://example.test/bridgevm-agent-0.2.0.tar.gz",
        signature: "sha256:abc123",
        observedAtUnix: 1_710_000_063
      ))
    XCTAssertEqual(runtime.updatedAtUnix, 1_710_000_061)
  }

  func testGuestToolsTokenRequestAndResponseMatchBridgeVmDaemonWireFormat() throws {
    let request =
      try JSONSerialization.jsonObject(
        with: JSONEncoder().encode(DaemonGuestToolsTokenRequest(name: "dev"))
      ) as? [String: String]
    XCTAssertEqual(
      request,
      [
        "type": "guest_tools_token",
        "name": "dev",
      ])

    let json = """
      {
        "type": "guest_tools_token",
        "token": {
          "vm": "dev",
          "token": "secret-token-value",
          "created_at_unix": 1710000000
        }
      }
      """

    let response = try JSONDecoder().decode(
      DaemonGuestToolsTokenResponse.self,
      from: Data(json.utf8)
    )

    XCTAssertEqual(response.type, "guest_tools_token")
    XCTAssertEqual(
      response.token.guestToolsToken,
      GuestToolsToken(vm: "dev", createdAtUnix: 1_710_000_000, tokenLength: 18)
    )
  }

  func testGuestToolsLinuxCommandRequestAndResponseMatchBridgeVmDaemonWireFormat() throws {
    let request =
      try JSONSerialization.jsonObject(
        with: JSONEncoder().encode(
          DaemonGuestToolsLinuxCommandRequest(name: "dev", transport: .socket))
      ) as? [String: Any]
    XCTAssertEqual(request?["type"] as? String, "guest_tools_linux_command")
    XCTAssertEqual(request?["name"] as? String, "dev")
    XCTAssertEqual(request?["transport"] as? String, "socket")
    XCTAssertNil(request?["token_file"])
    XCTAssertNil(request?["device"])

    let json = """
      {
        "type": "guest_tools_linux_command",
        "command": {
          "vm": "dev",
          "transport": "device",
          "command": ["bridgevm-guest-tools", "run", "--transport", "device"],
          "token_file": "/run/bridgevm-token.json",
          "capabilities": ["heartbeat", "time-sync"]
        }
      }
      """

    let response = try JSONDecoder().decode(
      DaemonGuestToolsLinuxCommandResponse.self,
      from: Data(json.utf8)
    )

    XCTAssertEqual(response.type, "guest_tools_linux_command")
    XCTAssertEqual(
      response.command.guestToolsLinuxCommand,
      GuestToolsLinuxCommand(
        vm: "dev",
        transport: .device,
        command: ["bridgevm-guest-tools", "run", "--transport", "device"],
        tokenFile: "/run/bridgevm-token.json",
        capabilities: ["heartbeat", "time-sync"]
      )
    )
  }

  func testMountSharedFolderRequestAndResponseMatchBridgeVmDaemonWireFormat() throws {
    let request =
      try JSONSerialization.jsonObject(
        with: JSONEncoder().encode(
          DaemonMountSharedFolderRequest(name: "dev", shareName: "workspace"))
      ) as? [String: String]
    XCTAssertEqual(
      request,
      [
        "type": "guest_tools_mount_approved_share",
        "name": "dev",
        "share": "workspace",
      ])

    let json = """
      {
        "type": "guest_tools_command",
        "command": {
          "vm": "dev",
          "request_id": "mount-1",
          "pending_commands": 1
        }
      }
      """

    let response = try JSONDecoder().decode(
      DaemonMountSharedFolderResponse.self,
      from: Data(json.utf8)
    )

    XCTAssertEqual(response.type, "guest_tools_command")
    XCTAssertEqual(response.command?.vm, "dev")
    XCTAssertEqual(response.command?.requestID, "mount-1")
    XCTAssertEqual(response.command?.pendingCommands, 1)
  }

  func testGuestToolsSendCommandRequestAndResponseMatchBridgeVmDaemonWireFormat() throws {
    let startRequest =
      try JSONSerialization.jsonObject(
        with: JSONEncoder().encode(
          DaemonGuestToolsSendCommandRequest(
            name: "dev",
            command: .fileDropStart(
              transferID: "drop-1",
              fileName: "notes.txt",
              sizeBytes: 5
            ),
            requestID: "drop-start-1"
          )
        )
      ) as? [String: Any]
    let startEnvelope = try XCTUnwrap(startRequest?["envelope"] as? [String: Any])
    XCTAssertEqual(startEnvelope["request_id"] as? String, "drop-start-1")
    let startMessage = try XCTUnwrap(startEnvelope["message"] as? [String: Any])
    let start = try XCTUnwrap(startMessage["FileDropStart"] as? [String: Any])
    XCTAssertEqual(start["transfer_id"] as? String, "drop-1")
    XCTAssertEqual(start["file_name"] as? String, "notes.txt")
    XCTAssertEqual(start["size_bytes"] as? Int, 5)

    let request =
      try JSONSerialization.jsonObject(
        with: JSONEncoder().encode(
          DaemonGuestToolsSendCommandRequest(
            name: "dev",
            command: .fileDropChunk(
              transferID: "drop-1",
              chunkIndex: 2,
              dataBase64: "aGVsbG8="
            ),
            requestID: "drop-chunk-2"
          )
        )
      ) as? [String: Any]

    XCTAssertEqual(request?["type"] as? String, "guest_tools_send_command")
    XCTAssertEqual(request?["name"] as? String, "dev")
    let envelope = try XCTUnwrap(request?["envelope"] as? [String: Any])
    XCTAssertEqual(envelope["protocol_version"] as? Int, 1)
    XCTAssertEqual(envelope["request_id"] as? String, "drop-chunk-2")
    let message = try XCTUnwrap(envelope["message"] as? [String: Any])
    let chunk = try XCTUnwrap(message["FileDropChunk"] as? [String: Any])
    XCTAssertEqual(chunk["transfer_id"] as? String, "drop-1")
    XCTAssertEqual(chunk["chunk_index"] as? Int, 2)
    XCTAssertEqual(chunk["data_base64"] as? String, "aGVsbG8=")

    let completeRequest =
      try JSONSerialization.jsonObject(
        with: JSONEncoder().encode(
          DaemonGuestToolsSendCommandRequest(
            name: "dev",
            command: .fileDropComplete(transferID: "drop-1"),
            requestID: "drop-complete-1"
          )
        )
      ) as? [String: Any]
    let completeEnvelope = try XCTUnwrap(completeRequest?["envelope"] as? [String: Any])
    XCTAssertEqual(completeEnvelope["request_id"] as? String, "drop-complete-1")
    let completeMessage = try XCTUnwrap(completeEnvelope["message"] as? [String: Any])
    let complete = try XCTUnwrap(completeMessage["FileDropComplete"] as? [String: Any])
    XCTAssertEqual(complete["transfer_id"] as? String, "drop-1")

    let listRequest =
      try JSONSerialization.jsonObject(
        with: JSONEncoder().encode(
          DaemonGuestToolsSendCommandRequest(
            name: "dev",
            command: .listApplications,
            requestID: "apps-1"
          )
        )
      ) as? [String: Any]
    let listEnvelope = try XCTUnwrap(listRequest?["envelope"] as? [String: Any])
    XCTAssertEqual(listEnvelope["message"] as? String, "ListApplications")

    let launchRequest =
      try JSONSerialization.jsonObject(
        with: JSONEncoder().encode(
          DaemonGuestToolsSendCommandRequest(
            name: "dev",
            command: .launchApplication(id: "terminal"),
            requestID: "launch-1"
          )
        )
      ) as? [String: Any]
    let launchEnvelope = try XCTUnwrap(launchRequest?["envelope"] as? [String: Any])
    let launchMessage = try XCTUnwrap(launchEnvelope["message"] as? [String: Any])
    let launch = try XCTUnwrap(launchMessage["LaunchApplication"] as? [String: Any])
    XCTAssertEqual(launch["id"] as? String, "terminal")

    let focusRequest =
      try JSONSerialization.jsonObject(
        with: JSONEncoder().encode(
          DaemonGuestToolsSendCommandRequest(
            name: "dev",
            command: .focusWindow(id: "window-1"),
            requestID: "focus-1"
          )
        )
      ) as? [String: Any]
    let focusEnvelope = try XCTUnwrap(focusRequest?["envelope"] as? [String: Any])
    let focusMessage = try XCTUnwrap(focusEnvelope["message"] as? [String: Any])
    let focus = try XCTUnwrap(focusMessage["FocusWindow"] as? [String: Any])
    XCTAssertEqual(focus["id"] as? String, "window-1")

    let closeRequest =
      try JSONSerialization.jsonObject(
        with: JSONEncoder().encode(
          DaemonGuestToolsSendCommandRequest(
            name: "dev",
            command: .closeWindow(id: "window-1"),
            requestID: "close-1"
          )
        )
      ) as? [String: Any]
    let closeEnvelope = try XCTUnwrap(closeRequest?["envelope"] as? [String: Any])
    let closeMessage = try XCTUnwrap(closeEnvelope["message"] as? [String: Any])
    let close = try XCTUnwrap(closeMessage["CloseWindow"] as? [String: Any])
    XCTAssertEqual(close["id"] as? String, "window-1")

    let boundsRequest =
      try JSONSerialization.jsonObject(
        with: JSONEncoder().encode(
          DaemonGuestToolsSendCommandRequest(
            name: "dev",
            command: .setWindowBounds(id: "window-1", x: 30, y: 40, width: 800, height: 600),
            requestID: "bounds-1"
          )
        )
      ) as? [String: Any]
    let boundsEnvelope = try XCTUnwrap(boundsRequest?["envelope"] as? [String: Any])
    let boundsMessage = try XCTUnwrap(boundsEnvelope["message"] as? [String: Any])
    let bounds = try XCTUnwrap(boundsMessage["SetWindowBounds"] as? [String: Any])
    XCTAssertEqual(bounds["id"] as? String, "window-1")
    XCTAssertEqual(bounds["x"] as? Int, 30)
    XCTAssertEqual(bounds["y"] as? Int, 40)
    XCTAssertEqual(bounds["width"] as? Int, 800)
    XCTAssertEqual(bounds["height"] as? Int, 600)

    let pointerRequest =
      try JSONSerialization.jsonObject(
        with: JSONEncoder().encode(
          DaemonGuestToolsSendCommandRequest(
            name: "dev",
            command: .windowPointerInput(
              id: "window-1",
              x: 120,
              y: 240,
              action: .click,
              button: .left
            ),
            requestID: "pointer-1"
          )
        )
      ) as? [String: Any]
    let pointerEnvelope = try XCTUnwrap(pointerRequest?["envelope"] as? [String: Any])
    let pointerMessage = try XCTUnwrap(pointerEnvelope["message"] as? [String: Any])
    let pointer = try XCTUnwrap(pointerMessage["WindowInput"] as? [String: Any])
    XCTAssertEqual(pointer["id"] as? String, "window-1")
    let pointerEvent = try XCTUnwrap(pointer["event"] as? [String: Any])
    XCTAssertEqual(pointerEvent["kind"] as? String, "pointer")
    XCTAssertEqual(pointerEvent["x"] as? Int, 120)
    XCTAssertEqual(pointerEvent["y"] as? Int, 240)
    XCTAssertEqual(pointerEvent["action"] as? String, "click")
    XCTAssertEqual(pointerEvent["button"] as? String, "left")

    let keyRequest =
      try JSONSerialization.jsonObject(
        with: JSONEncoder().encode(
          DaemonGuestToolsSendCommandRequest(
            name: "dev",
            command: .windowKeyInput(id: "window-1", key: "Return", action: .tap),
            requestID: "key-1"
          )
        )
      ) as? [String: Any]
    let keyEnvelope = try XCTUnwrap(keyRequest?["envelope"] as? [String: Any])
    let keyMessage = try XCTUnwrap(keyEnvelope["message"] as? [String: Any])
    let key = try XCTUnwrap(keyMessage["WindowInput"] as? [String: Any])
    XCTAssertEqual(key["id"] as? String, "window-1")
    let keyEvent = try XCTUnwrap(key["event"] as? [String: Any])
    XCTAssertEqual(keyEvent["kind"] as? String, "key")
    XCTAssertEqual(keyEvent["key"] as? String, "Return")
    XCTAssertEqual(keyEvent["action"] as? String, "tap")

    let unmountRequest =
      try JSONSerialization.jsonObject(
        with: JSONEncoder().encode(
          DaemonGuestToolsSendCommandRequest(
            name: "dev",
            command: .unmountShare(name: "workspace"),
            requestID: "unmount-1"
          )
        )
      ) as? [String: Any]
    let unmountEnvelope = try XCTUnwrap(unmountRequest?["envelope"] as? [String: Any])
    XCTAssertEqual(unmountEnvelope["request_id"] as? String, "unmount-1")
    let unmountMessage = try XCTUnwrap(unmountEnvelope["message"] as? [String: Any])
    let unmount = try XCTUnwrap(unmountMessage["UnmountShare"] as? [String: Any])
    XCTAssertEqual(unmount["name"] as? String, "workspace")

    let json = """
      {
        "type": "guest_tools_command",
        "command": {
          "vm": "dev",
          "request_id": "drop-chunk-2",
          "pending_commands": 3
        }
      }
      """

    let response = try JSONDecoder().decode(
      DaemonGuestToolsCommandResponse.self,
      from: Data(json.utf8)
    )

    XCTAssertEqual(response.type, "guest_tools_command")
    XCTAssertEqual(
      response.command?.guestToolsCommandDispatch,
      GuestToolsCommandDispatch(
        vm: "dev",
        requestID: "drop-chunk-2",
        pendingCommands: 3
      ))
  }

  func testRunnerStatusRequestAndResponseMatchBridgeVmDaemonWireFormat() throws {
    let request =
      try JSONSerialization.jsonObject(
        with: JSONEncoder().encode(DaemonRunnerStatusRequest(name: "dev"))
      ) as? [String: String]
    XCTAssertEqual(
      request,
      [
        "type": "runner_status",
        "name": "dev",
      ])

    let json = """
      {
        "type": "runner_status",
        "metadata": {
          "engine": "lightvm",
          "pid": 4242,
          "command": ["lightvm-runner", "dev", "--apple-vz"],
          "log_path": "logs/lightvm.log",
          "started_at_unix": 1710000100,
          "dry_run": false,
          "launch_spec_path": ".vmbridge/metadata/apple-vz-launch.json",
          "guest_tools": {
            "transport": "virtio-serial",
            "channel_name": "org.bridgevm.guest-tools.0",
            "socket_path": "metadata/guest-tools.sock",
            "token_path": "metadata/guest-tools-token.json",
            "token_created_at_unix": 1710000050
          },
          "runtime_control": {
            "kind": "apple-vz-display",
            "socket_path": "run/apple-vz-display-control.sock",
            "commands": ["status", "stop", "policy", "pacing"]
          },
          "launch_readiness": {
            "ready": false,
            "blockers": [
              {
                "code": "missing-primary-disk",
                "message": "Primary disk is missing.",
                "path": "disks/root.qcow2",
                "capability": "apple-vz"
              }
            ]
          }
        },
        "qmp_supervisor": {
          "events": [
            { "name": "STOP" },
            { "name": "RESUME" }
          ],
          "terminal_event": { "name": "SHUTDOWN" },
          "envelopes_read": 3,
          "limit_reached": false,
          "updated_at_unix": 1710000200
        }
      }
      """

    let response = try JSONDecoder().decode(
      DaemonRunnerStatusResponse.self,
      from: Data(json.utf8)
    )

    XCTAssertEqual(response.type, "runner_status")
    let status = try XCTUnwrap(response.runnerStatus)
    XCTAssertEqual(status.engine, "lightvm")
    XCTAssertEqual(status.pid, 4242)
    XCTAssertEqual(status.command, ["lightvm-runner", "dev", "--apple-vz"])
    XCTAssertEqual(status.logPath, "logs/lightvm.log")
    XCTAssertEqual(status.startedAtUnix, 1_710_000_100)
    XCTAssertFalse(status.dryRun)
    XCTAssertEqual(status.launchSpecPath, ".vmbridge/metadata/apple-vz-launch.json")
    let guestTools = try XCTUnwrap(status.guestTools)
    XCTAssertEqual(guestTools.transport, "virtio-serial")
    XCTAssertEqual(guestTools.channelName, "org.bridgevm.guest-tools.0")
    XCTAssertEqual(guestTools.socketPath, "metadata/guest-tools.sock")
    XCTAssertEqual(guestTools.tokenPath, "metadata/guest-tools-token.json")
    XCTAssertEqual(guestTools.tokenCreatedAtUnix, 1_710_000_050)
    let runtimeControl = try XCTUnwrap(status.runtimeControl)
    XCTAssertEqual(runtimeControl.kind, "apple-vz-display")
    XCTAssertEqual(runtimeControl.socketPath, "run/apple-vz-display-control.sock")
    XCTAssertEqual(runtimeControl.commands, ["status", "stop", "policy", "pacing"])
    XCTAssertEqual(runtimeControl.commandSummary, "status, stop, policy, pacing")
    let readiness = try XCTUnwrap(status.launchReadiness)
    XCTAssertFalse(readiness.ready)
    let blocker = try XCTUnwrap(readiness.blockers.first)
    XCTAssertEqual(blocker.code, "missing-primary-disk")
    XCTAssertEqual(blocker.message, "Primary disk is missing.")
    XCTAssertEqual(blocker.path, "disks/root.qcow2")
    XCTAssertEqual(blocker.capability, "apple-vz")
    let supervisor = try XCTUnwrap(status.qmpSupervisor)
    XCTAssertEqual(supervisor.summaryTitle, "2 events, terminal SHUTDOWN")
    XCTAssertEqual(supervisor.envelopesRead, 3)
    XCTAssertFalse(supervisor.limitReached)
    XCTAssertEqual(supervisor.updatedAtUnix, 1_710_000_200)

    let stoppedJSON = #"{"type":"runner_status","metadata":null}"#
    let stopped = try JSONDecoder().decode(
      DaemonRunnerStatusResponse.self,
      from: Data(stoppedJSON.utf8)
    )
    XCTAssertNil(stopped.metadata)
  }

  func testReadinessReportRequestAndResponseMatchBridgeVmDaemonWireFormat() throws {
    let request =
      try JSONSerialization.jsonObject(
        with: JSONEncoder().encode(DaemonReadinessReportRequest(name: "dev"))
      ) as? [String: String]
    XCTAssertEqual(
      request,
      [
        "type": "readiness_report",
        "name": "dev",
      ])

    let json = """
      {
        "type": "readiness_report",
        "report": {
          "vm": "dev",
          "mode": "fast",
          "state": "stopped",
          "metadata_only": true,
          "live_e2e_required": true,
          "live_evidence": {
            "path": "/tmp/bridgevm-live-evidence",
            "backend": "apple-virtualization-framework",
            "vm_name": "live-vz-linux",
            "boot_mode": "linux-kernel",
            "disk_format": "raw",
            "network": "nat",
            "serial_sentinel_required": true,
            "serial_sentinel_proven": true,
            "graphical_boot_progress_proven": true,
            "viewer_evidence_proven": true,
            "qmp_evidence_proven": false,
            "guest_tools_effects_proven": true,
            "summary": "Apple VZ live boot opt-in smoke: passed"
          },
          "evidence_requirements": [
            {
              "kind": "live-boot",
              "required": true,
              "proven": true,
              "note": "verified preserved opt-in Apple VZ serial and graphical boot progress evidence bundle"
            },
            {
              "kind": "guest-tools-effects",
              "required": true,
              "proven": true,
              "note": "verified guest-tools command/effect evidence from the preserved live bundle"
            }
          ],
          "boot_media": {
            "vm": "dev",
            "entries": [
              {
                "kind": "installer-image",
                "path": "installers/ubuntu-arm64.iso",
                "exists": false,
                "size_bytes": null,
                "last_import": null,
                "last_verification": null,
                "last_download_plan": null,
                "last_download": null
              }
            ]
          },
          "boot_media_error": null,
          "snapshot_chain": {
            "active_disk": {
              "source": "primary",
              "snapshot": null,
              "path": "disks/root.qcow2",
              "format": "qcow2",
              "exists": false,
              "activated_at_unix": 1710000000
            },
            "disks": []
          },
          "snapshot_chain_error": null,
          "runner": null,
          "runner_error": "not prepared",
          "pre_run_launch_readiness": {
            "ready": false,
            "blockers": [
              {
                "code": "missing-primary-disk",
                "message": "Primary disk is missing; prepare or create the disk before Fast Mode launch.",
                "path": "disks/root.qcow2",
                "capability": "apple-vz"
              },
              {
                "code": "missing-installer-image",
                "message": "Installer image is missing; import, verify, or download boot media before launch.",
                "path": "installers/ubuntu-arm64.iso",
                "capability": "apple-vz"
              }
            ]
          },
          "qmp_supervisor": {
            "events": [
              { "name": "RESUME" }
            ],
            "terminal_event": null,
            "envelopes_read": 1,
            "limit_reached": false,
            "updated_at_unix": 1710000300
          },
          "blockers": ["boot-media-missing:installers/ubuntu-arm64.iso"],
          "notes": ["metadata-only report"]
        }
      }
      """

    let response = try JSONDecoder().decode(
      DaemonReadinessReportResponse.self,
      from: Data(json.utf8)
    )

    XCTAssertEqual(response.type, "readiness_report")
    let report = response.report.vmReadinessReport
    XCTAssertEqual(report.vm, "dev")
    XCTAssertEqual(report.mode, .fast)
    XCTAssertEqual(report.state, .stopped)
    XCTAssertTrue(report.metadataOnly)
    XCTAssertTrue(report.liveE2ERequired)
    let liveEvidence = try XCTUnwrap(report.liveEvidence)
    XCTAssertEqual(liveEvidence.title, "Preserved live and guest-tools evidence verified")
    XCTAssertEqual(liveEvidence.path, "/tmp/bridgevm-live-evidence")
    XCTAssertEqual(liveEvidence.backend, "apple-virtualization-framework")
    XCTAssertEqual(liveEvidence.vmName, "live-vz-linux")
    XCTAssertEqual(liveEvidence.bootMode, "linux-kernel")
    XCTAssertEqual(liveEvidence.network, "nat")
    XCTAssertEqual(liveEvidence.summary, "Apple VZ live boot opt-in smoke: passed")
    XCTAssertTrue(liveEvidence.serialSentinelRequired)
    XCTAssertTrue(liveEvidence.serialSentinelProven)
    XCTAssertTrue(liveEvidence.graphicalBootProgressProven)
    XCTAssertTrue(liveEvidence.viewerEvidenceProven)
    XCTAssertFalse(liveEvidence.qmpEvidenceProven)
    XCTAssertTrue(liveEvidence.guestToolsEffectsProven)
    XCTAssertTrue(liveEvidence.detail.contains("graphical/serial console evidence proven"))
    XCTAssertTrue(liveEvidence.detail.contains("viewer evidence proven"))
    XCTAssertTrue(liveEvidence.detail.contains("QMP evidence pending"))
    XCTAssertEqual(report.evidenceRequirements.count, 2)
    XCTAssertEqual(report.evidenceRequirements.first?.kind, "live-boot")
    XCTAssertEqual(report.evidenceRequirements.first?.required, true)
    XCTAssertEqual(report.evidenceRequirements.first?.proven, true)
    XCTAssertEqual(report.evidenceRequirements.last?.kind, "guest-tools-effects")
    XCTAssertEqual(report.evidenceRequirements.last?.proven, true)
    XCTAssertEqual(report.readinessTitle, "Blocked (1)")
    XCTAssertEqual(report.bootMedia?.entries.first?.kind, .installerImage)
    XCTAssertEqual(report.snapshotChain?.activeDisk.path, "disks/root.qcow2")
    XCTAssertEqual(report.runnerError, "not prepared")
    let preRunReadiness = try XCTUnwrap(report.preRunLaunchReadiness)
    XCTAssertFalse(preRunReadiness.ready)
    XCTAssertEqual(preRunReadiness.blockers.map(\.code), [
      "missing-primary-disk",
      "missing-installer-image",
    ])
    XCTAssertEqual(preRunReadiness.blockers.first?.path, "disks/root.qcow2")
    XCTAssertEqual(preRunReadiness.blockers.first?.capability, "apple-vz")
    let readinessSupervisor = try XCTUnwrap(report.qmpSupervisor)
    XCTAssertEqual(readinessSupervisor.summaryTitle, "1 events")
    XCTAssertEqual(readinessSupervisor.envelopesRead, 1)
    XCTAssertEqual(report.notes, ["metadata-only report"])
  }

  func testReadinessReportDefaultsMissingViewerEvidenceProofToPending() throws {
    let json = """
      {
        "type": "readiness_report",
        "report": {
          "vm": "dev",
          "mode": "fast",
          "state": "stopped",
          "metadata_only": true,
          "live_e2e_required": true,
          "live_evidence": {
            "path": "/tmp/bridgevm-live-evidence",
            "backend": "apple-virtualization-framework",
            "vm_name": "live-vz-linux",
            "boot_mode": "linux-kernel",
            "disk_format": "raw",
            "network": "nat",
            "serial_sentinel_required": true,
            "serial_sentinel_proven": true,
            "guest_tools_effects_proven": true,
            "summary": "Apple VZ live boot opt-in smoke: passed"
          },
          "evidence_requirements": [],
          "boot_media": null,
          "boot_media_error": null,
          "snapshot_chain": null,
          "snapshot_chain_error": null,
          "runner": null,
          "runner_error": null,
          "pre_run_launch_readiness": null,
          "qmp_supervisor": null,
          "blockers": [],
          "notes": []
        }
      }
      """

    let response = try JSONDecoder().decode(
      DaemonReadinessReportResponse.self,
      from: Data(json.utf8)
    )

    let liveEvidence = try XCTUnwrap(response.report.vmReadinessReport.liveEvidence)
    XCTAssertFalse(liveEvidence.viewerEvidenceProven)
    XCTAssertTrue(liveEvidence.detail.contains("viewer evidence pending"))
  }

  func testReadinessReportHandlesClearedLiveEvidenceWithPendingRequirements() throws {
    let json = """
      {
        "type": "readiness_report",
        "report": {
          "vm": "dev",
          "mode": "fast",
          "state": "stopped",
          "metadata_only": true,
          "live_e2e_required": true,
          "live_evidence": null,
          "evidence_requirements": [
            {
              "kind": "live-boot",
              "required": true,
              "proven": false,
              "note": "No live boot transcript has been captured."
            },
            {
              "kind": "guest-tools-effects",
              "required": true,
              "proven": false,
              "note": "No guest-tools command/effect evidence has been captured."
            }
          ],
          "boot_media": null,
          "boot_media_error": null,
          "snapshot_chain": null,
          "snapshot_chain_error": null,
          "runner": null,
          "runner_error": null,
          "pre_run_launch_readiness": null,
          "qmp_supervisor": null,
          "blockers": [],
          "notes": []
        }
      }
      """

    let response = try JSONDecoder().decode(
      DaemonReadinessReportResponse.self,
      from: Data(json.utf8)
    )

    let report = response.report.vmReadinessReport
    XCTAssertNil(report.liveEvidence)
    XCTAssertEqual(report.pendingRequiredEvidence.map(\.kind), [
      "live-boot",
      "guest-tools-effects",
    ])
    XCTAssertEqual(report.evidenceReadinessTitle, "2 evidence checks pending")
    XCTAssertEqual(report.readinessTitle, "2 evidence checks pending")
  }

  func testReadinessReportPreservesLiveEvidenceDisplayForBundlePath() throws {
    let json = """
      {
        "type": "readiness_report",
        "report": {
          "vm": "dev",
          "mode": "fast",
          "state": "stopped",
          "metadata_only": true,
          "live_e2e_required": true,
          "live_evidence": {
            "path": "/store/vms/dev.vmbridge/metadata/live-evidence/latest",
            "backend": "apple-virtualization-framework",
            "vm_name": "live-vz-linux",
            "boot_mode": "linux-kernel",
            "disk_format": "raw",
            "network": "nat",
            "serial_sentinel_required": true,
            "serial_sentinel_proven": true,
            "viewer_evidence_proven": true,
            "qmp_evidence_proven": false,
            "guest_tools_effects_proven": true,
            "summary": "Apple VZ live boot opt-in smoke: passed"
          },
          "evidence_requirements": [],
          "boot_media": null,
          "boot_media_error": null,
          "snapshot_chain": null,
          "snapshot_chain_error": null,
          "runner": null,
          "runner_error": null,
          "pre_run_launch_readiness": null,
          "qmp_supervisor": null,
          "blockers": [],
          "notes": []
        }
      }
      """

    let response = try JSONDecoder().decode(
      DaemonReadinessReportResponse.self,
      from: Data(json.utf8)
    )

    let liveEvidence = try XCTUnwrap(response.report.vmReadinessReport.liveEvidence)
    XCTAssertEqual(
      liveEvidence.path,
      "/store/vms/dev.vmbridge/metadata/live-evidence/latest"
    )
    XCTAssertEqual(liveEvidence.title, "Preserved live and guest-tools evidence verified")
    XCTAssertEqual(
      liveEvidence.detail,
      "apple-virtualization-framework, linux-kernel, raw, nat, graphical/serial console evidence proven, boot progress proven, viewer evidence proven, QMP evidence pending, guest-tools effects proven"
    )
  }

  func testReadinessReportDisplaysQmpOnlyLiveEvidenceAsVerified() throws {
    let json = """
      {
        "type": "readiness_report",
        "report": {
          "vm": "dev",
          "mode": "compatibility",
          "state": "stopped",
          "metadata_only": true,
          "live_e2e_required": true,
          "live_evidence": {
            "path": "/store/vms/dev.vmbridge/metadata/live-evidence/latest",
            "backend": "qemu",
            "vm_name": "dev",
            "boot_mode": "compatibility",
            "disk_format": "qcow2",
            "network": "nat",
            "serial_sentinel_required": true,
            "serial_sentinel_proven": false,
            "viewer_evidence_proven": false,
            "qmp_evidence_proven": true,
            "guest_tools_effects_proven": false,
            "summary": "QEMU live evidence verified from QMP state"
          },
          "evidence_requirements": [],
          "boot_media": null,
          "boot_media_error": null,
          "snapshot_chain": null,
          "snapshot_chain_error": null,
          "runner": null,
          "runner_error": null,
          "pre_run_launch_readiness": null,
          "qmp_supervisor": null,
          "blockers": [],
          "notes": []
        }
      }
      """

    let response = try JSONDecoder().decode(
      DaemonReadinessReportResponse.self,
      from: Data(json.utf8)
    )

    let liveEvidence = try XCTUnwrap(response.report.vmReadinessReport.liveEvidence)
    XCTAssertEqual(
      liveEvidence.title,
      "Preserved QMP console evidence verified; graphical/serial proof pending"
    )
    XCTAssertTrue(liveEvidence.qmpEvidenceProven)
    XCTAssertTrue(liveEvidence.detail.contains("graphical/serial console evidence pending"))
    XCTAssertTrue(liveEvidence.detail.contains("boot progress pending"))
    XCTAssertTrue(liveEvidence.detail.contains("QMP evidence proven"))
    XCTAssertTrue(liveEvidence.detail.contains("viewer evidence pending"))
  }

  func testReadinessReportDisplaysAllFalseConsoleProofAsPending() throws {
    let json = """
      {
        "type": "readiness_report",
        "report": {
          "vm": "dev",
          "mode": "fast",
          "state": "stopped",
          "metadata_only": true,
          "live_e2e_required": true,
          "live_evidence": {
            "path": "/store/vms/dev.vmbridge/metadata/live-evidence/latest",
            "backend": "apple-virtualization-framework",
            "vm_name": "live-vz-linux",
            "boot_mode": "linux-kernel",
            "disk_format": "raw",
            "network": "nat",
            "serial_sentinel_required": true,
            "serial_sentinel_proven": false,
            "viewer_evidence_proven": false,
            "qmp_evidence_proven": false,
            "guest_tools_effects_proven": false,
            "summary": "Preserved bundle exists but console proof is pending"
          },
          "evidence_requirements": [],
          "boot_media": null,
          "boot_media_error": null,
          "snapshot_chain": null,
          "snapshot_chain_error": null,
          "runner": null,
          "runner_error": null,
          "pre_run_launch_readiness": null,
          "qmp_supervisor": null,
          "blockers": [],
          "notes": []
        }
      }
      """

    let response = try JSONDecoder().decode(
      DaemonReadinessReportResponse.self,
      from: Data(json.utf8)
    )

    let liveEvidence = try XCTUnwrap(response.report.vmReadinessReport.liveEvidence)
    XCTAssertEqual(
      liveEvidence.title,
      "Preserved live evidence recorded; console proof pending"
    )
    XCTAssertFalse(liveEvidence.viewerEvidenceProven)
    XCTAssertFalse(liveEvidence.qmpEvidenceProven)
    XCTAssertFalse(liveEvidence.graphicalBootProgressProven)
  }

  func testReadinessReportDisplaysQmpOnlyGuestToolsEvidenceAsDiagnosticsVerified() throws {
    let json = """
      {
        "type": "readiness_report",
        "report": {
          "vm": "dev",
          "mode": "compatibility",
          "state": "stopped",
          "metadata_only": true,
          "live_e2e_required": true,
          "live_evidence": {
            "path": "/store/vms/dev.vmbridge/metadata/live-evidence/latest",
            "backend": "qemu",
            "vm_name": "dev",
            "boot_mode": "compatibility",
            "disk_format": "qcow2",
            "network": "nat",
            "serial_sentinel_required": true,
            "serial_sentinel_proven": false,
            "viewer_evidence_proven": false,
            "qmp_evidence_proven": true,
            "guest_tools_effects_proven": true,
            "summary": "QEMU live and guest-tools evidence verified"
          },
          "evidence_requirements": [],
          "boot_media": null,
          "boot_media_error": null,
          "snapshot_chain": null,
          "snapshot_chain_error": null,
          "runner": null,
          "runner_error": null,
          "pre_run_launch_readiness": null,
          "qmp_supervisor": null,
          "blockers": [],
          "notes": []
        }
      }
      """

    let response = try JSONDecoder().decode(
      DaemonReadinessReportResponse.self,
      from: Data(json.utf8)
    )

    let liveEvidence = try XCTUnwrap(response.report.vmReadinessReport.liveEvidence)
    XCTAssertEqual(
      liveEvidence.title,
      "Preserved QMP console and guest-tools evidence verified; graphical/serial proof pending"
    )
    XCTAssertFalse(liveEvidence.serialSentinelProven)
    XCTAssertFalse(liveEvidence.viewerEvidenceProven)
    XCTAssertTrue(liveEvidence.qmpEvidenceProven)
    XCTAssertTrue(liveEvidence.guestToolsEffectsProven)
    XCTAssertTrue(liveEvidence.detail.contains("graphical/serial console evidence pending"))
    XCTAssertTrue(liveEvidence.detail.contains("boot progress pending"))
    XCTAssertTrue(liveEvidence.detail.contains("QMP evidence proven"))
    XCTAssertTrue(liveEvidence.detail.contains("guest-tools effects proven"))
  }

  func testPrepareRunRequestMatchesBridgeVmDaemonWireFormat() throws {
    let request =
      try JSONSerialization.jsonObject(
        with: JSONEncoder().encode(DaemonPrepareRunRequest(name: "dev"))
      ) as? [String: String]
    XCTAssertEqual(
      request,
      [
        "type": "prepare_run",
        "name": "dev",
      ])
  }

  func testSnapshotPreflightRequestAndResponseMatchBridgeVmDaemonWireFormat() throws {
    let request =
      try JSONSerialization.jsonObject(
        with: JSONEncoder().encode(
          DaemonSnapshotPreflightStatusRequest(
            name: "dev",
            consistency: .applicationConsistent
          )
        )
      ) as? [String: String]
    XCTAssertEqual(
      request,
      [
        "type": "snapshot_preflight_status",
        "name": "dev",
        "consistency": "application-consistent",
      ])

    let json = """
      {
        "type": "snapshot_preflight_status",
        "preflight": {
          "vm": "dev",
          "consistency": "application-consistent",
          "backend_freeze_thaw_supported": false,
          "guest_tools_connected": true,
          "capabilities": ["guest-tools-heartbeat", "filesystem-freeze-preflight"],
          "ready": false,
          "blockers": [
            {
              "code": "backend-freeze-thaw-unavailable",
              "message": "Freeze/thaw orchestration requires the bridgevmd-owned running backend; this offline preflight cannot drive the guest agent.",
              "path": null
            }
          ],
          "checked_at_unix": 1710000200
        }
      }
      """

    let response = try JSONDecoder().decode(
      DaemonSnapshotPreflightStatusResponse.self,
      from: Data(json.utf8)
    )

    XCTAssertEqual(response.type, "snapshot_preflight_status")
    let status = response.preflight.snapshotPreflightStatus
    XCTAssertEqual(status.vm, "dev")
    XCTAssertEqual(status.consistency, .applicationConsistent)
    XCTAssertFalse(status.backendFreezeThawSupported)
    XCTAssertTrue(status.guestToolsConnected)
    XCTAssertEqual(status.capabilities, ["guest-tools-heartbeat", "filesystem-freeze-preflight"])
    XCTAssertFalse(status.ready)
    XCTAssertEqual(status.readinessTitle, "Scaffold only")
    XCTAssertEqual(status.blockers.first?.code, "backend-freeze-thaw-unavailable")
    XCTAssertEqual(status.checkedAtUnix, 1_710_000_200)
  }

  func testListSnapshotsRequestAndResponseMatchBridgeVmDaemonWireFormat() throws {
    let request =
      try JSONSerialization.jsonObject(
        with: JSONEncoder().encode(DaemonListSnapshotsRequest(vm: "dev"))
      ) as? [String: String]
    XCTAssertEqual(
      request,
      [
        "type": "list_snapshots",
        "vm": "dev",
      ])

    let json = """
      {
        "type": "snapshot_list",
        "snapshots": [
          {
            "name": "before-upgrade",
            "kind": "disk",
            "created_at_unix": 1710000300,
            "vm_state": "stopped"
          },
          {
            "name": "paused-state",
            "kind": "suspend",
            "created_at_unix": 1710000400,
            "vm_state": "suspended"
          }
        ]
      }
      """

    let response = try JSONDecoder().decode(
      DaemonSnapshotListResponse.self,
      from: Data(json.utf8)
    )

    XCTAssertEqual(response.type, "snapshot_list")
    XCTAssertEqual(response.snapshots.count, 2)
    let first = try XCTUnwrap(response.snapshots.first?.vmSnapshot)
    XCTAssertEqual(first.name, "before-upgrade")
    XCTAssertEqual(first.kind, .disk)
    XCTAssertEqual(first.createdAtUnix, 1_710_000_300)
    XCTAssertEqual(first.vmState, .stopped)
    let second = try XCTUnwrap(response.snapshots.last?.vmSnapshot)
    XCTAssertEqual(second.kind, .suspend)
    XCTAssertEqual(second.vmState, .suspended)
  }

  func testSnapshotChainRequestAndResponseMatchBridgeVmDaemonWireFormat() throws {
    let request =
      try JSONSerialization.jsonObject(
        with: JSONEncoder().encode(DaemonSnapshotChainRequest(vm: "dev"))
      ) as? [String: String]
    XCTAssertEqual(
      request,
      [
        "type": "snapshot_chain",
        "vm": "dev",
      ])

    let json = """
      {
        "type": "snapshot_chain",
        "chain": {
          "active_disk": {
            "source": "snapshot-overlay",
            "snapshot": "before-upgrade",
            "path": "disks/snapshots/before-upgrade.qcow2",
            "format": "qcow2",
            "exists": true,
            "activated_at_unix": 1710000360
          },
          "disks": [
            {
              "snapshot": "before-upgrade",
              "overlay_path": "disks/snapshots/before-upgrade.qcow2",
              "overlay_format": "qcow2",
              "overlay_exists": true,
              "backing_path": "disks/root.qcow2",
              "backing_format": "qcow2",
              "backing_exists": true,
              "create_command": [
                "qemu-img",
                "create",
                "-f",
                "qcow2",
                "-F",
                "qcow2",
                "-b",
                "disks/root.qcow2",
                "disks/snapshots/before-upgrade.qcow2"
              ],
              "prepared_at_unix": 1710000300
            }
          ]
        }
      }
      """

    let response = try JSONDecoder().decode(
      DaemonSnapshotChainResponse.self,
      from: Data(json.utf8)
    )

    XCTAssertEqual(response.type, "snapshot_chain")
    let chain = response.chain.vmSnapshotChain
    XCTAssertEqual(chain.activeDisk.source, "snapshot-overlay")
    XCTAssertEqual(chain.activeDisk.sourceTitle, "Snapshot overlay")
    XCTAssertEqual(chain.activeDisk.snapshot, "before-upgrade")
    XCTAssertEqual(chain.activeDisk.path, "disks/snapshots/before-upgrade.qcow2")
    XCTAssertEqual(chain.activeDisk.activatedAtUnix, 1_710_000_360)
    XCTAssertEqual(chain.readinessTitle, "Chain ready")
    let disk = try XCTUnwrap(chain.disks.first)
    XCTAssertEqual(disk.snapshot, "before-upgrade")
    XCTAssertEqual(disk.overlayPath, "disks/snapshots/before-upgrade.qcow2")
    XCTAssertEqual(disk.overlayFormat, "qcow2")
    XCTAssertTrue(disk.overlayExists)
    XCTAssertEqual(disk.backingPath, "disks/root.qcow2")
    XCTAssertEqual(disk.backingFormat, "qcow2")
    XCTAssertTrue(disk.backingExists)
    XCTAssertEqual(
      disk.createCommandLine,
      "qemu-img create -f qcow2 -F qcow2 -b disks/root.qcow2 disks/snapshots/before-upgrade.qcow2"
    )
    XCTAssertEqual(disk.preparedAtUnix, 1_710_000_300)
  }

  func testCreateSnapshotDiskRequestAndResponseMatchBridgeVmDaemonWireFormat() throws {
    let request =
      try JSONSerialization.jsonObject(
        with: JSONEncoder().encode(
          DaemonCreateSnapshotDiskRequest(vm: "dev", name: "before-upgrade")
        )
      ) as? [String: String]
    XCTAssertEqual(
      request,
      [
        "type": "create_snapshot_disk",
        "vm": "dev",
        "name": "before-upgrade",
      ])

    let json = """
      {
        "type": "snapshot_disk_created",
        "metadata": {
          "snapshot": "before-upgrade",
          "disk": {
            "snapshot": "before-upgrade",
            "overlay_path": "disks/snapshots/before-upgrade.qcow2",
            "overlay_format": "qcow2",
            "overlay_exists": true,
            "backing_path": "disks/root.qcow2",
            "backing_format": "qcow2",
            "backing_exists": true,
            "create_command": [
              "qemu-img",
              "create",
              "-f",
              "qcow2",
              "-F",
              "qcow2",
              "-b",
              "disks/root.qcow2",
              "disks/snapshots/before-upgrade.qcow2"
            ],
            "prepared_at_unix": 1710000300
          },
          "command": [
            "qemu-img",
            "create",
            "-f",
            "qcow2",
            "-F",
            "qcow2",
            "-b",
            "disks/root.qcow2",
            "disks/snapshots/before-upgrade.qcow2"
          ],
          "executed": true,
          "exit_status": "exit status: 0",
          "stdout": "created overlay\\n",
          "stderr": "",
          "created_at_unix": 1710000360
        }
      }
      """

    let response = try JSONDecoder().decode(
      DaemonSnapshotDiskCreatedResponse.self,
      from: Data(json.utf8)
    )

    XCTAssertEqual(response.type, "snapshot_disk_created")
    let creation = response.metadata.vmSnapshotDiskCreation
    XCTAssertEqual(creation.snapshot, "before-upgrade")
    XCTAssertEqual(creation.disk.snapshot, "before-upgrade")
    XCTAssertEqual(creation.disk.overlayPath, "disks/snapshots/before-upgrade.qcow2")
    XCTAssertEqual(creation.disk.overlayFormat, "qcow2")
    XCTAssertTrue(creation.disk.overlayExists)
    XCTAssertEqual(creation.disk.backingPath, "disks/root.qcow2")
    XCTAssertEqual(creation.disk.backingFormat, "qcow2")
    XCTAssertTrue(creation.disk.backingExists)
    XCTAssertEqual(
      creation.disk.createCommandLine,
      "qemu-img create -f qcow2 -F qcow2 -b disks/root.qcow2 disks/snapshots/before-upgrade.qcow2"
    )
    XCTAssertEqual(creation.commandLine, creation.disk.createCommandLine)
    XCTAssertTrue(creation.executed)
    XCTAssertEqual(creation.exitStatus, "exit status: 0")
    XCTAssertEqual(creation.stdout, "created overlay\n")
    XCTAssertEqual(creation.stderr, "")
    XCTAssertEqual(creation.createdAtUnix, 1_710_000_360)
  }

  func testCreateSnapshotRequestAndResponseMatchBridgeVmDaemonWireFormat() throws {
    let request =
      try JSONSerialization.jsonObject(
        with: JSONEncoder().encode(
          DaemonCreateSnapshotRequest(
            vm: "dev",
            name: "before-upgrade",
            kind: .applicationConsistent
          )
        )
      ) as? [String: String]
    XCTAssertEqual(
      request,
      [
        "type": "create_snapshot",
        "vm": "dev",
        "name": "before-upgrade",
        "kind": "application-consistent",
      ])

    let json = """
      {
        "type": "snapshot",
        "snapshot": {
          "name": "before-upgrade",
          "kind": "application-consistent",
          "created_at_unix": 1710000360,
          "vm_state": "running"
        }
      }
      """

    let response = try JSONDecoder().decode(
      DaemonSnapshotCreatedResponse.self,
      from: Data(json.utf8)
    )

    XCTAssertEqual(response.type, "snapshot")
    let snapshot = response.snapshot.vmSnapshot
    XCTAssertEqual(snapshot.name, "before-upgrade")
    XCTAssertEqual(snapshot.kind, .applicationConsistent)
    XCTAssertEqual(snapshot.createdAtUnix, 1_710_000_360)
    XCTAssertEqual(snapshot.vmState, .running)
  }

  func testPrepareDiskRequestAndResponseMatchBridgeVmDaemonWireFormat() throws {
    let request =
      try JSONSerialization.jsonObject(
        with: JSONEncoder().encode(DaemonPrepareDiskRequest(name: "dev"))
      ) as? [String: String]
    XCTAssertEqual(
      request,
      [
        "type": "prepare_disk",
        "name": "dev",
      ])

    let json = """
      {
        "type": "disk_prepared",
        "metadata": {
          "path": "disks/root.qcow2",
          "format": "qcow2",
          "size": "80G",
          "size_bytes": 85899345920,
          "exists": false,
          "created": true,
          "create_command": ["qemu-img", "create", "-f", "qcow2", "disks/root.qcow2", "80G"],
          "prepared_at_unix": 1710000300
        }
      }
      """

    let response = try JSONDecoder().decode(
      DaemonDiskPreparedResponse.self,
      from: Data(json.utf8)
    )

    XCTAssertEqual(response.type, "disk_prepared")
    let preparation = response.metadata.diskPreparation
    XCTAssertEqual(preparation.path, "disks/root.qcow2")
    XCTAssertEqual(preparation.format, "qcow2")
    XCTAssertEqual(preparation.size, "80G")
    XCTAssertEqual(preparation.sizeBytes, 85_899_345_920)
    XCTAssertFalse(preparation.exists)
    XCTAssertTrue(preparation.created)
    XCTAssertEqual(
      preparation.createCommandLine,
      "qemu-img create -f qcow2 disks/root.qcow2 80G"
    )
    XCTAssertEqual(preparation.preparedAtUnix, 1_710_000_300)
  }

  func testCreateDiskRequestAndResponseMatchBridgeVmDaemonWireFormat() throws {
    let request =
      try JSONSerialization.jsonObject(
        with: JSONEncoder().encode(DaemonCreateDiskRequest(name: "dev"))
      ) as? [String: String]
    XCTAssertEqual(
      request,
      [
        "type": "create_disk",
        "name": "dev",
      ])

    let json = """
      {
        "type": "disk_created",
        "metadata": {
          "preparation": {
            "path": "disks/root.qcow2",
            "format": "qcow2",
            "size": "80G",
            "size_bytes": 85899345920,
            "exists": false,
            "created": true,
            "create_command": ["qemu-img", "create", "-f", "qcow2", "disks/root.qcow2", "80G"],
            "prepared_at_unix": 1710000300
          },
          "command": ["qemu-img", "create", "-f", "qcow2", "disks/root.qcow2", "80G"],
          "executed": true,
          "exit_status": "exit status: 0",
          "stdout": "Formatting 'disks/root.qcow2'\\n",
          "stderr": "",
          "created_at_unix": 1710000400
        }
      }
      """

    let response = try JSONDecoder().decode(
      DaemonDiskCreatedResponse.self,
      from: Data(json.utf8)
    )

    XCTAssertEqual(response.type, "disk_created")
    let creation = response.metadata.vmDiskCreation
    XCTAssertEqual(creation.preparation.path, "disks/root.qcow2")
    XCTAssertEqual(creation.preparation.sizeBytes, 85_899_345_920)
    XCTAssertEqual(creation.commandLine, "qemu-img create -f qcow2 disks/root.qcow2 80G")
    XCTAssertTrue(creation.executed)
    XCTAssertEqual(creation.exitStatus, "exit status: 0")
    XCTAssertEqual(creation.stdout, "Formatting 'disks/root.qcow2'\n")
    XCTAssertEqual(creation.stderr, "")
    XCTAssertEqual(creation.createdAtUnix, 1_710_000_400)
  }

  func testInspectDiskRequestAndResponseMatchBridgeVmDaemonWireFormat() throws {
    let request =
      try JSONSerialization.jsonObject(
        with: JSONEncoder().encode(DaemonInspectDiskRequest(name: "dev"))
      ) as? [String: String]
    XCTAssertEqual(
      request,
      [
        "type": "inspect_disk",
        "name": "dev",
      ])

    let json = """
      {
        "type": "disk_inspected",
        "metadata": {
          "preparation": {
            "path": "disks/root.qcow2",
            "format": "qcow2",
            "size": "80G",
            "size_bytes": 85899345920,
            "exists": true,
            "created": false,
            "create_command": ["qemu-img", "create", "-f", "qcow2", "disks/root.qcow2", "80G"],
            "prepared_at_unix": 1710000300
          },
          "command": ["qemu-img", "info", "--output=json", "disks/root.qcow2"],
          "exit_status": "exit status: 0",
          "info": {
            "filename": "disks/root.qcow2",
            "format": "qcow2",
            "virtual-size": 85899345920
          },
          "stdout": "{\\"filename\\":\\"disks/root.qcow2\\",\\"format\\":\\"qcow2\\"}",
          "stderr": "",
          "inspect_duration_microseconds": 3456,
          "inspected_at_unix": 1710000500
        }
      }
      """

    let response = try JSONDecoder().decode(
      DaemonDiskInspectedResponse.self,
      from: Data(json.utf8)
    )

    XCTAssertEqual(response.type, "disk_inspected")
    let inspection = response.metadata.vmDiskInspection
    XCTAssertEqual(inspection.preparation.path, "disks/root.qcow2")
    XCTAssertEqual(inspection.commandLine, "qemu-img info --output=json disks/root.qcow2")
    XCTAssertEqual(inspection.exitStatus, "exit status: 0")
    XCTAssertEqual(
      inspection.infoValue,
      .object([
        "filename": .string("disks/root.qcow2"),
        "format": .string("qcow2"),
        "virtual-size": .int(85_899_345_920),
      ]))
    XCTAssertEqual(inspection.inspectDurationMicroseconds, 3456)
    XCTAssertEqual(inspection.inspectedAtUnix, 1_710_000_500)
  }

  func testVerifyDiskRequestAndResponseMatchBridgeVmDaemonWireFormat() throws {
    let request =
      try JSONSerialization.jsonObject(
        with: JSONEncoder().encode(DaemonVerifyDiskRequest(name: "dev"))
      ) as? [String: String]
    XCTAssertEqual(
      request,
      [
        "type": "verify_disk",
        "name": "dev",
      ])

    let json = """
      {
        "type": "disk_verified",
        "metadata": {
          "active_disk": {
            "source": "primary",
            "snapshot": null,
            "path": "disks/root.qcow2",
            "format": "qcow2",
            "exists": true,
            "activated_at_unix": 1710000300
          },
          "command": ["qemu-img", "check", "--output=json", "disks/root.qcow2"],
          "exit_status": "exit status: 0",
          "report": {
            "check-errors": 0,
            "image-end-offset": 4096
          },
          "stdout": "{\\"check-errors\\":0,\\"image-end-offset\\":4096}",
          "stderr": "",
          "verify_duration_microseconds": 1234,
          "verified_at_unix": 1710000800
        }
      }
      """

    let response = try JSONDecoder().decode(
      DaemonDiskVerifiedResponse.self,
      from: Data(json.utf8)
    )

    XCTAssertEqual(response.type, "disk_verified")
    let verification = response.metadata.vmDiskVerification
    XCTAssertEqual(verification.activeDisk.source, "primary")
    XCTAssertEqual(verification.activeDisk.path, "disks/root.qcow2")
    XCTAssertEqual(verification.commandLine, "qemu-img check --output=json disks/root.qcow2")
    XCTAssertEqual(verification.exitStatus, "exit status: 0")
    XCTAssertEqual(
      verification.reportValue,
      .object([
        "check-errors": .int(0),
        "image-end-offset": .int(4096),
      ]))
    XCTAssertEqual(verification.verifyDurationMicroseconds, 1234)
    XCTAssertEqual(verification.verifiedAtUnix, 1_710_000_800)
  }

  func testCompactDiskRequestAndResponseMatchBridgeVmDaemonWireFormat() throws {
    let request =
      try JSONSerialization.jsonObject(
        with: JSONEncoder().encode(DaemonCompactDiskRequest(name: "dev"))
      ) as? [String: String]
    XCTAssertEqual(
      request,
      [
        "type": "compact_disk",
        "name": "dev",
      ])

    let json = """
      {
        "type": "disk_compacted",
        "metadata": {
          "preparation": {
            "path": "disks/root.qcow2",
            "format": "qcow2",
            "size": "80G",
            "size_bytes": 85899345920,
            "exists": true,
            "created": false,
            "create_command": ["qemu-img", "create", "-f", "qcow2", "disks/root.qcow2", "80G"],
            "prepared_at_unix": 1710000300
          },
          "active_disk": {
            "source": "primary",
            "snapshot": null,
            "path": "disks/root.qcow2",
            "format": "qcow2",
            "exists": true,
            "activated_at_unix": 1710000300
          },
          "command": [
            "qemu-img",
            "convert",
            "-O",
            "qcow2",
            "-c",
            "disks/root.qcow2",
            "disks/root.qcow2.compact.tmp"
          ],
          "temp_path": "disks/root.qcow2.compact.tmp",
          "backup_path": "disks/root.qcow2.precompact-1710000900",
          "exit_status": "exit status: 0",
          "stdout": "compacted\\n",
          "stderr": "",
          "original_size_bytes": 8192,
          "compacted_size_bytes": 4096,
          "compact_duration_microseconds": 2345,
          "compacted_at_unix": 1710000900
        }
      }
      """

    let response = try JSONDecoder().decode(
      DaemonDiskCompactedResponse.self,
      from: Data(json.utf8)
    )

    XCTAssertEqual(response.type, "disk_compacted")
    let compaction = response.metadata.vmDiskCompaction
    XCTAssertEqual(compaction.preparation.path, "disks/root.qcow2")
    XCTAssertEqual(compaction.preparation.format, "qcow2")
    XCTAssertEqual(compaction.preparation.size, "80G")
    XCTAssertEqual(compaction.preparation.sizeBytes, 85_899_345_920)
    XCTAssertFalse(compaction.preparation.created)
    XCTAssertEqual(compaction.activeDisk.source, "primary")
    XCTAssertEqual(compaction.command.prefix(5), ["qemu-img", "convert", "-O", "qcow2", "-c"])
    XCTAssertEqual(compaction.tempPath, "disks/root.qcow2.compact.tmp")
    XCTAssertEqual(compaction.backupPath, "disks/root.qcow2.precompact-1710000900")
    XCTAssertEqual(compaction.originalSizeBytes, 8192)
    XCTAssertEqual(compaction.compactedSizeBytes, 4096)
    XCTAssertEqual(compaction.savedBytes, 4096)
    XCTAssertEqual(compaction.compactDurationMicroseconds, 2345)
    XCTAssertEqual(compaction.compactedAtUnix, 1_710_000_900)
  }

  func testRestoreSnapshotRequestAndResponseMatchBridgeVmDaemonWireFormat() throws {
    let request =
      try JSONSerialization.jsonObject(
        with: JSONEncoder().encode(
          DaemonRestoreSnapshotRequest(vm: "dev", name: "paused-state")
        )
      ) as? [String: String]
    XCTAssertEqual(
      request,
      [
        "type": "restore_snapshot",
        "vm": "dev",
        "name": "paused-state",
      ])

    let json = """
      {
        "type": "snapshot_restored",
        "restore": {
          "snapshot": "paused-state",
          "restored_at_unix": 1710000500,
          "restored_state": "suspended",
          "active_disk": {
            "source": "snapshot-backing",
            "snapshot": "paused-state",
            "path": "disks/root.qcow2",
            "format": "qcow2",
            "exists": true,
            "activated_at_unix": 1710000500
          },
          "suspend_image": {
            "snapshot": "paused-state",
            "image_path": "snapshots/paused-state/suspend.img",
            "image_format": "vz",
            "image_exists": true,
            "prepared_at_unix": 1710000400
          }
        }
      }
      """

    let response = try JSONDecoder().decode(
      DaemonSnapshotRestoredResponse.self,
      from: Data(json.utf8)
    )

    XCTAssertEqual(response.type, "snapshot_restored")
    let restore = response.restore.snapshotRestoreResult
    XCTAssertEqual(restore.snapshot, "paused-state")
    XCTAssertEqual(restore.restoredAtUnix, 1_710_000_500)
    XCTAssertEqual(restore.restoredState, .suspended)
    XCTAssertEqual(restore.activeDisk?.source, "snapshot-backing")
    XCTAssertEqual(restore.activeDisk?.snapshot, "paused-state")
    XCTAssertEqual(restore.activeDisk?.path, "disks/root.qcow2")
    XCTAssertEqual(restore.activeDisk?.activatedAtUnix, 1_710_000_500)
    XCTAssertEqual(restore.suspendImage?.imagePath, "snapshots/paused-state/suspend.img")
    XCTAssertEqual(restore.suspendImage?.imageFormat, "vz")
    XCTAssertEqual(restore.suspendImage?.preparedAtUnix, 1_710_000_400)
  }

  func testApplicationConsistentSnapshotExecutionRequestAndResponseMatchBridgeVmDaemonWireFormat()
    throws
  {
    let request =
      try JSONSerialization.jsonObject(
        with: JSONEncoder().encode(
          DaemonExecuteApplicationConsistentSnapshotRequest(
            vm: "dev",
            name: "before-upgrade",
            freezeTimeoutMillis: 5_000
          )
        )
      ) as? [String: Any]
    XCTAssertEqual(request?["type"] as? String, "execute_application_consistent_snapshot")
    XCTAssertEqual(request?["vm"] as? String, "dev")
    XCTAssertEqual(request?["name"] as? String, "before-upgrade")
    XCTAssertEqual(request?["freeze_timeout_millis"] as? Int, 5_000)

    let json = """
      {
        "type": "application_consistent_snapshot_execution",
        "execution": {
          "vm": "dev",
          "snapshot": "before-upgrade",
          "freeze_request_id": "freeze-1",
          "thaw_request_id": "thaw-1",
          "pending_commands_after_freeze": 1,
          "pending_commands_after_thaw": 2,
          "snapshot_created_at_unix": 1710000300,
          "freeze_result": {
            "request_id": "freeze-1",
            "capability": "fs-freeze",
            "ok": true,
            "error_code": null,
            "message": "freeze scaffold acknowledged",
            "completed_at_unix": 1710000280
          },
          "thaw_result": {
            "request_id": "thaw-1",
            "capability": "fs-thaw",
            "ok": true,
            "error_code": null,
            "message": "thaw scaffold acknowledged",
            "completed_at_unix": 1710000290
          },
          "preflight_ready": true,
          "note": "scaffold boundary"
        }
      }
      """

    let response = try JSONDecoder().decode(
      DaemonApplicationConsistentSnapshotExecutionResponse.self,
      from: Data(json.utf8)
    )

    XCTAssertEqual(response.type, "application_consistent_snapshot_execution")
    let execution = response.execution.applicationConsistentSnapshotExecution
    XCTAssertEqual(execution.vm, "dev")
    XCTAssertEqual(execution.snapshot, "before-upgrade")
    XCTAssertEqual(execution.freezeRequestID, "freeze-1")
    XCTAssertEqual(execution.thawRequestID, "thaw-1")
    XCTAssertEqual(execution.pendingCommandsAfterFreeze, 1)
    XCTAssertEqual(execution.pendingCommandsAfterThaw, 2)
    XCTAssertEqual(execution.snapshotCreatedAtUnix, 1_710_000_300)
    XCTAssertTrue(execution.freezeResult.ok)
    XCTAssertEqual(execution.freezeResult.capability, "fs-freeze")
    XCTAssertTrue(execution.thawResult.ok)
    XCTAssertEqual(execution.thawResult.capability, "fs-thaw")
    XCTAssertTrue(execution.preflightReady)
    XCTAssertEqual(execution.summaryTitle, "Snapshot executed")
  }

  func testRuntimeResourcePolicyRequestAndResponseMatchBridgeVmDaemonWireFormat() throws {
    let request =
      try JSONSerialization.jsonObject(
        with: JSONEncoder().encode(
          DaemonReapplyRuntimeResourcesRequest(name: "dev", visibility: .background)
        )
      ) as? [String: Any]
    XCTAssertEqual(request?["type"] as? String, "reapply_runtime_resources")
    XCTAssertEqual(request?["name"] as? String, "dev")
    XCTAssertEqual(request?["visibility"] as? String, "background")

    let json = """
      {
        "type": "runtime_resource_policy",
        "policy": {
          "vm": "dev",
          "mode": "fast",
          "profile": "automatic",
          "visibility": "background",
          "state": "running",
          "on_battery": false,
          "memory": "2048",
          "cpu": "1",
          "display_fps_cap": "10",
          "rationale": "Battery or background throttling active.",
          "live_applied": false,
          "runtime_control_acknowledged": true,
          "live_apply_blockers": [
            {
              "code": "runtime-control-unavailable",
              "message": "Live Apple VZ CPU/RAM hot-apply is not implemented yet; the policy is available to display pacing and runtime policy IPC consumers."
            }
          ],
          "updated_at_unix": 1710000500
        }
      }
      """

    let response = try JSONDecoder().decode(
      DaemonRuntimeResourcePolicyResponse.self,
      from: Data(json.utf8)
    )

    XCTAssertEqual(response.type, "runtime_resource_policy")
    let policy = response.policy.runtimeResourcePolicy
    XCTAssertEqual(policy.vm, "dev")
    XCTAssertEqual(policy.mode, "fast")
    XCTAssertEqual(policy.profile, "automatic")
    XCTAssertEqual(policy.visibility, .background)
    XCTAssertEqual(policy.state, "running")
    XCTAssertFalse(policy.onBattery)
    XCTAssertEqual(policy.memory, "2048")
    XCTAssertEqual(policy.cpu, "1")
    XCTAssertEqual(policy.displayFPSCap, "10")
    XCTAssertEqual(policy.rationale, "Battery or background throttling active.")
    XCTAssertEqual(policy.liveApplyTitle, "Blocked")
    XCTAssertTrue(policy.runtimeControlAcknowledged)
    XCTAssertEqual(policy.liveApplyBlockers.first?.code, "runtime-control-unavailable")
    let expectedBlockerSummary =
      "runtime-control-unavailable: Live Apple VZ CPU/RAM hot-apply is not implemented yet; the policy is available to display pacing and runtime policy IPC consumers."
    XCTAssertEqual(
      policy.liveApplyBlockers.first?.summary,
      expectedBlockerSummary
    )
    XCTAssertEqual(policy.updatedAtUnix, 1_710_000_500)
  }

  func testDiagnosticBundleRequestAndResponseMatchBridgeVmDaemonWireFormat() throws {
    let request =
      try JSONSerialization.jsonObject(
        with: JSONEncoder().encode(
          DaemonCreateDiagnosticBundleRequest(name: "dev", output: "/tmp/diagnostics")
        )
      ) as? [String: Any]
    XCTAssertEqual(request?["type"] as? String, "create_diagnostic_bundle")
    XCTAssertEqual(request?["name"] as? String, "dev")
    XCTAssertEqual(request?["output"] as? String, "/tmp/diagnostics")

    let json = """
      {
        "type": "diagnostic_bundle",
        "bundle": {
          "vm": "dev",
          "source": "/tmp/dev.vmbridge",
          "output": "/tmp/diagnostics/bridgevm-diagnostics-dev-1710000600",
          "files": ["manifest.yaml", "metadata/state.json", "diagnostic-bundle.json"],
          "created_at_unix": 1710000600
        }
      }
      """

    let response = try JSONDecoder().decode(
      DaemonDiagnosticBundleResponse.self,
      from: Data(json.utf8)
    )

    XCTAssertEqual(response.type, "diagnostic_bundle")
    let bundle = response.bundle.diagnosticBundle
    XCTAssertEqual(bundle.vm, "dev")
    XCTAssertEqual(bundle.source, "/tmp/dev.vmbridge")
    XCTAssertEqual(bundle.files.count, 3)
    XCTAssertEqual(bundle.fileCountTitle, "3 files")
    XCTAssertEqual(bundle.createdAtUnix, 1_710_000_600)
  }

  func testPerformanceBaselineRequestAndResponseMatchBridgeVmDaemonWireFormat() throws {
    let request =
      try JSONSerialization.jsonObject(
        with: JSONEncoder().encode(
          DaemonCreatePerformanceBaselineRequest(name: "dev", output: "/tmp/performance")
        )
      ) as? [String: Any]
    XCTAssertEqual(request?["type"] as? String, "create_performance_baseline")
    XCTAssertEqual(request?["name"] as? String, "dev")
    XCTAssertEqual(request?["output"] as? String, "/tmp/performance")

    let response = try JSONDecoder().decode(
      DaemonPerformanceBaselineResponse.self,
      from: Data(performanceBaselineJSON(type: "performance_baseline").utf8)
    )

    XCTAssertEqual(response.type, "performance_baseline")
    let baseline = response.baseline.performanceBaseline
    XCTAssertEqual(baseline.vm, "dev")
    XCTAssertEqual(baseline.state, .running)
    XCTAssertTrue(baseline.metadataOnly)
    XCTAssertEqual(baseline.runner?.engine, "qemu")
    XCTAssertEqual(baseline.guestTools.runtime?.metrics?.cpuPercent, 7)
    XCTAssertEqual(baseline.metrics?.memoryUsedMiB, 2048)
    XCTAssertEqual(baseline.measurements.first?.name, "guest_cpu_percent")
    XCTAssertEqual(baseline.measurements.first?.valueTitle, "7 percent")
    XCTAssertEqual(baseline.notes.count, 2)
  }

  func testPerformanceSampleRequestAndResponseMatchBridgeVmDaemonWireFormat() throws {
    let request =
      try JSONSerialization.jsonObject(
        with: JSONEncoder().encode(
          DaemonCreatePerformanceSampleRequest(
            name: "dev",
            output: "/tmp/performance",
            artifactBytes: 4096,
            iterations: 2,
            sync: true
          )
        )
      ) as? [String: Any]
    XCTAssertEqual(request?["type"] as? String, "create_performance_sample")
    XCTAssertEqual(request?["name"] as? String, "dev")
    XCTAssertEqual(request?["output"] as? String, "/tmp/performance")
    XCTAssertEqual(request?["artifact_bytes"] as? Int, 4096)
    XCTAssertEqual(request?["iterations"] as? Int, 2)
    XCTAssertEqual(request?["sync"] as? Bool, true)

    let json = """
      {
        "type": "performance_sample",
        "sample": {
          "vm": "dev",
          "source": "/tmp/dev.vmbridge",
          "output": "/tmp/performance/bridgevm-performance-sample-dev-1710000700",
          "artifact": "/tmp/performance/bridgevm-performance-sample-dev-1710000700/performance-sample.json",
          "probe": "/tmp/performance/bridgevm-performance-sample-dev-1710000700/write-probe-0001.bin",
          "probes": [
            "/tmp/performance/bridgevm-performance-sample-dev-1710000700/write-probe-0001.bin",
            "/tmp/performance/bridgevm-performance-sample-dev-1710000700/write-probe-0002.bin"
          ],
          "artifact_bytes": 4096,
          "iterations": 2,
          "sync": true,
          "iteration_results": [
            {
              "iteration": 1,
              "probe": "/tmp/performance/bridgevm-performance-sample-dev-1710000700/write-probe-0001.bin",
              "bytes": 4096,
              "write_latency_microseconds": 80,
              "sync": true
            },
            {
              "iteration": 2,
              "probe": "/tmp/performance/bridgevm-performance-sample-dev-1710000700/write-probe-0002.bin",
              "bytes": 4096,
              "write_latency_microseconds": 75,
              "sync": true
            }
          ],
          \(performanceBaselineFieldsJSON(createdAtUnix: 1_710_000_700))
        }
      }
      """

    let response = try JSONDecoder().decode(
      DaemonPerformanceSampleResponse.self,
      from: Data(json.utf8)
    )

    XCTAssertEqual(response.type, "performance_sample")
    let sample = response.sample.performanceSample
    XCTAssertEqual(sample.vm, "dev")
    XCTAssertEqual(sample.state, .running)
    XCTAssertEqual(sample.artifactBytes, 4096)
    XCTAssertEqual(sample.iterations, 2)
    XCTAssertTrue(sample.sync)
    XCTAssertEqual(sample.iterationResults.map(\.writeLatencyMicroseconds), [80, 75])
    XCTAssertEqual(sample.measurements.first?.name, "guest_cpu_percent")
  }

  func testImportBootMediaRequestAndResponseMatchBridgeVmDaemonWireFormat() throws {
    let request =
      try JSONSerialization.jsonObject(
        with: JSONEncoder().encode(
          DaemonImportBootMediaRequest(
            name: "ubuntu",
            source: "/tmp/ubuntu.iso",
            kind: .installerImage
          )
        )
      ) as? [String: Any]

    XCTAssertEqual(request?["type"] as? String, "import_boot_media")
    XCTAssertEqual(request?["name"] as? String, "ubuntu")
    XCTAssertEqual(request?["source"] as? String, "/tmp/ubuntu.iso")
    XCTAssertEqual(request?["kind"] as? String, "installer-image")

    let autoKindRequest =
      try JSONSerialization.jsonObject(
        with: JSONEncoder().encode(
          DaemonImportBootMediaRequest(
            name: "ubuntu",
            source: "/tmp/ubuntu.iso",
            kind: nil
          )
        )
      ) as? [String: Any]
    XCTAssertTrue(autoKindRequest?.keys.contains("kind") == true)
    XCTAssertTrue(autoKindRequest?["kind"] is NSNull)

    let json = """
      {
        "type": "boot_media_imported",
        "import": {
          "vm": "ubuntu",
          "kind": "installer-image",
          "source": "/tmp/ubuntu.iso",
          "destination": "installers/ubuntu-arm64.iso",
          "bytes": 14,
          "replaced": true,
          "imported_at_unix": 1710000040
        }
      }
      """

    let response = try JSONDecoder().decode(
      DaemonBootMediaImportResponse.self,
      from: Data(json.utf8)
    )

    XCTAssertEqual(response.type, "boot_media_imported")
    XCTAssertEqual(response.`import`.kind, .installerImage)
    XCTAssertEqual(response.`import`.source, "/tmp/ubuntu.iso")
    XCTAssertEqual(response.`import`.destination, "installers/ubuntu-arm64.iso")
    XCTAssertEqual(response.`import`.bytes, 14)
    XCTAssertTrue(response.`import`.replaced)
  }

  func testVerifyBootMediaRequestAndResponseMatchBridgeVmDaemonWireFormat() throws {
    let request =
      try JSONSerialization.jsonObject(
        with: JSONEncoder().encode(
          DaemonVerifyBootMediaRequest(
            name: "ubuntu",
            expectedSHA256: "abc",
            kind: .installerImage
          )
        )
      ) as? [String: Any]

    XCTAssertEqual(request?["type"] as? String, "verify_boot_media")
    XCTAssertEqual(request?["name"] as? String, "ubuntu")
    XCTAssertEqual(request?["expected_sha256"] as? String, "abc")
    XCTAssertEqual(request?["kind"] as? String, "installer-image")

    let json = """
      {
        "type": "boot_media_verified",
        "verification": {
          "vm": "ubuntu",
          "kind": "installer-image",
          "path": "installers/ubuntu-arm64.iso",
          "bytes": 14,
          "expected_sha256": "abc",
          "actual_sha256": "abc",
          "verified": true,
          "verified_at_unix": 1710000050
        }
      }
      """

    let response = try JSONDecoder().decode(
      DaemonBootMediaVerificationResponse.self,
      from: Data(json.utf8)
    )

    XCTAssertEqual(response.type, "boot_media_verified")
    XCTAssertEqual(response.verification.kind, .installerImage)
    XCTAssertEqual(response.verification.expectedSHA256, "abc")
    XCTAssertEqual(response.verification.actualSHA256, "abc")
    XCTAssertTrue(response.verification.verified)
  }

  func testPlanBootMediaDownloadRequestAndResponseMatchBridgeVmDaemonWireFormat() throws {
    let request =
      try JSONSerialization.jsonObject(
        with: JSONEncoder().encode(
          DaemonPlanBootMediaDownloadRequest(
            name: "ubuntu",
            url: "https://example.invalid/ubuntu.iso",
            expectedSHA256: nil,
            kind: nil
          )
        )
      ) as? [String: Any]

    XCTAssertEqual(request?["type"] as? String, "plan_boot_media_download")
    XCTAssertEqual(request?["name"] as? String, "ubuntu")
    XCTAssertEqual(request?["url"] as? String, "https://example.invalid/ubuntu.iso")
    XCTAssertTrue(request?["expected_sha256"] is NSNull)
    XCTAssertTrue(request?["kind"] is NSNull)

    let json = """
      {
        "type": "boot_media_download_planned",
        "plan": {
          "vm": "ubuntu",
          "kind": "installer-image",
          "url": "https://example.invalid/ubuntu.iso",
          "destination": "installers/ubuntu-arm64.iso",
          "exists": false,
          "bytes": null,
          "expected_sha256": null,
          "last_import": null,
          "last_verification": null,
          "planned_at_unix": 1710000060
        }
      }
      """

    let response = try JSONDecoder().decode(
      DaemonBootMediaDownloadPlanResponse.self,
      from: Data(json.utf8)
    )

    XCTAssertEqual(response.type, "boot_media_download_planned")
    XCTAssertEqual(response.plan.kind, .installerImage)
    XCTAssertEqual(response.plan.url, "https://example.invalid/ubuntu.iso")
    XCTAssertEqual(response.plan.destination, "installers/ubuntu-arm64.iso")
    XCTAssertFalse(response.plan.exists)
    XCTAssertNil(response.plan.expectedSHA256)
  }

  func testDownloadBootMediaRequestAndResponseMatchBridgeVmDaemonWireFormat() throws {
    let request =
      try JSONSerialization.jsonObject(
        with: JSONEncoder().encode(
          DaemonDownloadBootMediaRequest(
            name: "ubuntu",
            kind: .installerImage
          )
        )
      ) as? [String: Any]

    XCTAssertEqual(request?["type"] as? String, "download_boot_media")
    XCTAssertEqual(request?["name"] as? String, "ubuntu")
    XCTAssertEqual(request?["kind"] as? String, "installer-image")

    let json = """
      {
        "type": "boot_media_downloaded",
        "download": {
          "vm": "ubuntu",
          "kind": "installer-image",
          "url": "https://example.invalid/ubuntu.iso",
          "destination": "installers/ubuntu-arm64.iso",
          "temp_path": "installers/.ubuntu-arm64.iso.download",
          "command": ["curl", "--location"],
          "exit_status": 0,
          "stdout": "",
          "stderr": "",
          "bytes": 14,
          "replaced": false,
          "expected_sha256": "abc",
          "actual_sha256": "abc",
          "verified": true,
          "downloaded": true,
          "downloaded_at_unix": 1710000070
        }
      }
      """

    let response = try JSONDecoder().decode(
      DaemonBootMediaDownloadResponse.self,
      from: Data(json.utf8)
    )

    XCTAssertEqual(response.type, "boot_media_downloaded")
    XCTAssertEqual(response.download.kind, .installerImage)
    XCTAssertEqual(response.download.url, "https://example.invalid/ubuntu.iso")
    XCTAssertEqual(response.download.destination, "installers/ubuntu-arm64.iso")
    XCTAssertEqual(response.download.bytes, 14)
    XCTAssertEqual(response.download.expectedSHA256, "abc")
    XCTAssertEqual(response.download.actualSHA256, "abc")
    XCTAssertEqual(response.download.verified, true)
    XCTAssertTrue(response.download.downloaded)
  }

  func testDaemonClientStartsBackendUsingNameFromListCache() async throws {
    let transport = RecordingDaemonTransport()
    let client = DaemonVirtualMachineClient(
      endpoint: .local,
      transport: transport
    )

    let initial = try await client.listVirtualMachines()
    let vm = try XCTUnwrap(initial.first)
    XCTAssertEqual(vm.status, .stopped)

    let result = try await client.perform(.start, on: vm.id)

    XCTAssertEqual(result.virtualMachine.name, "dev")
    XCTAssertEqual(result.virtualMachine.status, .running)
    let requestTypes = transport.requests.compactMap { $0["type"] as? String }
    XCTAssertEqual(requestTypes, ["list_vms", "run_backend", "list_vms"])
    XCTAssertEqual(transport.requests[1]["name"] as? String, "dev")
    XCTAssertEqual(transport.requests[1]["spawn"] as? Bool, true)
  }

  func testDaemonClientCachesDisplayStoreMetadataFromList() async throws {
    let transport = RecordingDaemonTransport()
    let client = DaemonVirtualMachineClient(
      endpoint: .local,
      transport: transport
    )

    let virtualMachines = try await client.listVirtualMachines()
    let vm = try XCTUnwrap(virtualMachines.first)
    let metadata = try XCTUnwrap(client.displayStoreMetadata(for: vm.id))

    XCTAssertEqual(metadata.bundlePath, "/tmp/dev.vmbridge")
    XCTAssertNil(metadata.storeRoot)
    XCTAssertEqual(transport.requests.compactMap { $0["type"] as? String }, ["list_vms"])
  }

  func testDaemonClientSendsRuntimeControlUsingNameFromListCache() async throws {
    let transport = RecordingDaemonTransport()
    let client = DaemonVirtualMachineClient(
      endpoint: .local,
      transport: transport
    )

    let initial = try await client.listVirtualMachines()
    let vm = try XCTUnwrap(initial.first)

    let result = try await client.sendRuntimeControlCommand("status", on: vm.id)

    XCTAssertEqual(result.vm, "dev")
    XCTAssertEqual(result.kind, "apple-vz-display")
    XCTAssertEqual(result.socketPath, "/tmp/bvm-vz-test.sock")
    XCTAssertEqual(result.command, "status")
    XCTAssertEqual(result.response.value, .object(["ok": .bool(true), "state": .string("running")]))
    let requestTypes = transport.requests.compactMap { $0["type"] as? String }
    XCTAssertEqual(requestTypes, ["list_vms", "runtime_control"])
    XCTAssertEqual(transport.requests[1]["name"] as? String, "dev")
    XCTAssertEqual(transport.requests[1]["command"] as? String, "status")
  }

  func testDaemonClientRestartsThroughStopThenBackendRun() async throws {
    let transport = RecordingDaemonTransport()
    let client = DaemonVirtualMachineClient(
      endpoint: .local,
      transport: transport
    )

    let initial = try await client.listVirtualMachines()
    let vm = try XCTUnwrap(initial.first)

    let result = try await client.perform(.restart, on: vm.id)

    XCTAssertEqual(result.virtualMachine.name, "dev")
    XCTAssertEqual(result.virtualMachine.status, .running)
    let requestTypes = transport.requests.compactMap { $0["type"] as? String }
    XCTAssertEqual(requestTypes, ["list_vms", "stop_backend", "run_backend", "list_vms"])
    XCTAssertEqual(transport.requests[1]["name"] as? String, "dev")
    XCTAssertEqual(transport.requests[2]["name"] as? String, "dev")
    XCTAssertEqual(transport.requests[2]["spawn"] as? Bool, true)
  }

  func testDaemonClientRequestsStoreDoctorWithoutVmName() async throws {
    let transport = RecordingDaemonTransport()
    let client = DaemonVirtualMachineClient(
      endpoint: .local,
      transport: transport
    )

    let report = try await client.inspectStoreDoctor()

    XCTAssertEqual(report.storeRoot, "/tmp/bridgevm")
    XCTAssertEqual(report.vmsDir, "/tmp/bridgevm/vms")
    XCTAssertEqual(report.status, "OK")
    XCTAssertEqual(report.source, "bridgevmd")
    XCTAssertEqual(transport.requests.compactMap { $0["type"] as? String }, ["doctor"])
    XCTAssertNil(transport.requests[0]["name"])
  }

  func testFallbackClientStillFallsBackForReadOnlyInventory() async throws {
    let vmID = UUID()
    let fallback = RecordingFallbackClient(vmID: vmID)
    let client = FallbackVirtualMachineClient(
      primary: AlwaysFailingVirtualMachineClient(),
      fallback: fallback
    )

    let virtualMachines = try await client.listVirtualMachines()

    XCTAssertEqual(virtualMachines.map(\.id), [vmID])
    XCTAssertEqual(fallback.calls, [.listVirtualMachines])
  }

  func testFallbackClientLocksMutationsForFallbackInventoryAndRestoresPrimarySource()
    async throws
  {
    let vmID = UUID()
    let primary = FailingOnceVirtualMachineClient(vmID: vmID)
    let fallback = RecordingFallbackClient(vmID: vmID)
    let client = FallbackVirtualMachineClient(primary: primary, fallback: fallback)

    let fallbackVirtualMachines = try await client.listVirtualMachines()

    XCTAssertEqual(fallbackVirtualMachines.map(\.id), [vmID])
    XCTAssertEqual(client.sourceTitle, "Mock inventory")
    XCTAssertFalse(client.allowsMutationsForCurrentInventory)

    let primaryVirtualMachines = try await client.listVirtualMachines()

    XCTAssertEqual(primaryVirtualMachines.map(\.id), [vmID])
    XCTAssertEqual(client.sourceTitle, "bridgevmd")
    XCTAssertTrue(client.allowsMutationsForCurrentInventory)
    XCTAssertEqual(fallback.calls, [.listVirtualMachines])
  }

  func testFallbackClientLocksMutationsForReadOnlyPlanFallback() async throws {
    let vmID = UUID()
    let fallback = RecordingFallbackClient(vmID: vmID)
    let client = FallbackVirtualMachineClient(
      primary: AlwaysFailingVirtualMachineClient(),
      fallback: fallback
    )

    let plan = try await client.inspectLifecyclePlan(action: .suspend, on: vmID)

    XCTAssertEqual(plan.vm, "Fallback VM")
    XCTAssertEqual(client.sourceTitle, "Mock inventory")
    XCTAssertFalse(client.allowsMutationsForCurrentInventory)
    XCTAssertEqual(fallback.calls, [.inspectLifecyclePlan])
  }

  func testFallbackClientDoesNotFallbackPerformAfterDaemonFailure() async throws {
    let vmID = UUID()
    let fallback = RecordingFallbackClient(vmID: vmID)
    let client = FallbackVirtualMachineClient(
      primary: AlwaysFailingVirtualMachineClient(),
      fallback: fallback
    )

    do {
      _ = try await client.perform(.start, on: vmID)
      XCTFail("perform should propagate the daemon failure")
    } catch TestClientError.primaryFailed {
      XCTAssertTrue(true)
    } catch {
      XCTFail("unexpected error: \(error)")
    }

    XCTAssertFalse(fallback.calls.contains(.perform))
  }

  func testFallbackClientDoesNotFallbackCreateOrPortMutationAfterDaemonFailure() async throws {
    let vmID = UUID()
    let fallback = RecordingFallbackClient(vmID: vmID)
    let client = FallbackVirtualMachineClient(
      primary: AlwaysFailingVirtualMachineClient(),
      fallback: fallback
    )
    let template = BootTemplate(
      id: "ubuntu-arm64-installer",
      guestOS: "ubuntu",
      guestVersion: nil,
      guestArch: "arm64",
      mode: .linuxInstaller,
      mediaLabel: "ubuntu arm64 installer image",
      source: "manual",
      installerImage: "installers/ubuntu-arm64.iso",
      kernelPath: nil,
      initrdPath: nil,
      kernelCommandLine: nil,
      macosRestoreImage: nil,
      note: "Place the installer image inside the .vmbridge bundle."
    )

    do {
      _ = try await client.createVirtualMachine(
        CreateVirtualMachineRequest(name: "Created VM", template: template)
      )
      XCTFail("create should propagate the daemon failure")
    } catch TestClientError.primaryFailed {
      XCTAssertTrue(true)
    } catch {
      XCTFail("unexpected error: \(error)")
    }

    do {
      _ = try await client.addPortForward(host: 2222, guest: 22, on: vmID)
      XCTFail("addPortForward should propagate the daemon failure")
    } catch {
      XCTAssertTrue(true)
    }

    XCTAssertFalse(fallback.calls.contains(.createVirtualMachine))
    XCTAssertFalse(fallback.calls.contains(.addPortForward))
  }

  func testDaemonClientCreatesVirtualMachineAndDecodesVmResponse() async throws {
    let transport = RecordingDaemonTransport()
    let client = DaemonVirtualMachineClient(
      endpoint: .local,
      transport: transport
    )
    let templates = try await client.listBootTemplates()
    let template = try XCTUnwrap(templates.first)

    let created = try await client.createVirtualMachine(
      CreateVirtualMachineRequest(name: "Created VM", template: template)
    )

    XCTAssertEqual(created.name, "Created VM")
    XCTAssertEqual(created.guest, "ubuntu arm64")
    XCTAssertEqual(created.status, .stopped)
    XCTAssertEqual(created.mode, .fast)
    let requestTypes = transport.requests.compactMap { $0["type"] as? String }
    XCTAssertEqual(requestTypes, ["list_templates", "create_vm"])
  }

  func testDaemonClientClonesVirtualMachineUsingNameFromListCache() async throws {
    let transport = RecordingDaemonTransport()
    let client = DaemonVirtualMachineClient(
      endpoint: .local,
      transport: transport
    )

    let virtualMachines = try await client.listVirtualMachines()
    let vm = try XCTUnwrap(virtualMachines.first)

    let clone = try await client.cloneVirtualMachine(on: vm.id, newName: "dev-copy", linked: true)

    XCTAssertEqual(clone.vm, "dev-copy")
    XCTAssertEqual(clone.source, "/tmp/dev.vmbridge")
    XCTAssertEqual(clone.output, "/tmp/dev-copy.vmbridge")
    XCTAssertTrue(clone.linked)
    let requestTypes = transport.requests.compactMap { $0["type"] as? String }
    XCTAssertEqual(requestTypes, ["list_vms", "clone_vm"])
    XCTAssertEqual(transport.requests[1]["name"] as? String, "dev")
    XCTAssertEqual(transport.requests[1]["new_name"] as? String, "dev-copy")
    XCTAssertEqual(transport.requests[1]["linked"] as? Bool, true)
  }

  func testDaemonClientExportsAndImportsVirtualMachineUsingWireFormat() async throws {
    let transport = RecordingDaemonTransport()
    let client = DaemonVirtualMachineClient(
      endpoint: .local,
      transport: transport
    )

    let virtualMachines = try await client.listVirtualMachines()
    let vm = try XCTUnwrap(virtualMachines.first)

    let exported = try await client.exportVirtualMachine(
      on: vm.id,
      output: "/tmp/dev-export.vmbridge"
    )
    let imported = try await client.importVirtualMachine(
      input: "/tmp/dev-export.vmbridge",
      name: "dev-imported"
    )

    XCTAssertEqual(exported.vm, "dev")
    XCTAssertEqual(exported.source, "/tmp/dev.vmbridge")
    XCTAssertEqual(exported.output, "/tmp/dev-export.vmbridge")
    XCTAssertEqual(exported.archiveFormat, "directory")
    XCTAssertEqual(exported.copiedFileCount, 3)
    XCTAssertEqual(
      exported.copiedFiles,
      [
        "manifest.yaml",
        "metadata/state.json",
        "metadata/runtime.json",
      ])
    XCTAssertTrue(exported.manifestPreserved)
    XCTAssertTrue(exported.metadataPreserved)
    XCTAssertEqual(exported.exportedAtUnix, 1_710_000_110)
    XCTAssertEqual(imported.vm, "dev-imported")
    XCTAssertEqual(imported.source, "/tmp/dev-export.vmbridge")
    XCTAssertEqual(imported.output, "/tmp/dev-imported.vmbridge")
    XCTAssertEqual(imported.archiveFormat, "directory")
    XCTAssertEqual(imported.copiedFileCount, 3)
    XCTAssertEqual(
      imported.copiedFiles,
      [
        "manifest.yaml",
        "metadata/state.json",
        "metadata/runtime.json",
      ])
    XCTAssertTrue(imported.manifestPreserved)
    XCTAssertTrue(imported.metadataPreserved)
    XCTAssertEqual(imported.originalName, "dev")
    XCTAssertEqual(imported.requestedName, "dev-imported")
    XCTAssertTrue(imported.manifestIdentityRewritten)
    XCTAssertEqual(imported.importedAtUnix, 1_710_000_120)

    let requestTypes = transport.requests.compactMap { $0["type"] as? String }
    XCTAssertEqual(requestTypes, ["list_vms", "export_vm", "import_vm"])
    XCTAssertEqual(transport.requests[1]["name"] as? String, "dev")
    XCTAssertEqual(transport.requests[1]["output"] as? String, "/tmp/dev-export.vmbridge")
    XCTAssertEqual(transport.requests[2]["input"] as? String, "/tmp/dev-export.vmbridge")
    XCTAssertEqual(transport.requests[2]["name"] as? String, "dev-imported")
  }

  func testDaemonClientInspectsBootMediaStatusUsingNameFromListCache() async throws {
    let transport = RecordingDaemonTransport()
    let client = DaemonVirtualMachineClient(
      endpoint: .local,
      transport: transport
    )

    let virtualMachines = try await client.listVirtualMachines()
    let vm = try XCTUnwrap(virtualMachines.first)

    let status = try await client.inspectBootMediaStatus(on: vm.id)

    XCTAssertEqual(status.vm, "dev")
    XCTAssertEqual(status.entries.first?.kind, .installerImage)
    XCTAssertEqual(status.entries.first?.sizeBytes, 14)
    let requestTypes = transport.requests.compactMap { $0["type"] as? String }
    XCTAssertEqual(requestTypes, ["list_vms", "inspect_boot_media_status"])
    XCTAssertEqual(transport.requests[1]["name"] as? String, "dev")
  }

  func testDaemonClientInspectsGuestToolsStatusUsingNameFromListCache() async throws {
    let transport = RecordingDaemonTransport()
    let client = DaemonVirtualMachineClient(
      endpoint: .local,
      transport: transport
    )

    let virtualMachines = try await client.listVirtualMachines()
    let vm = try XCTUnwrap(virtualMachines.first)

    let status = try await client.inspectGuestToolsStatus(on: vm.id)

    XCTAssertEqual(status.vm, "dev")
    XCTAssertEqual(status.tools, "required")
    XCTAssertEqual(status.runtime?.guestOS, "ubuntu")
    XCTAssertEqual(status.runtime?.guestIPAddresses.first?.address, "192.168.64.23")
    let requestTypes = transport.requests.compactMap { $0["type"] as? String }
    XCTAssertEqual(requestTypes, ["list_vms", "guest_tools_status"])
    XCTAssertEqual(transport.requests[1]["name"] as? String, "dev")
  }

  func testDaemonClientInspectsGuestToolsProvisioningUsingNameFromListCache() async throws {
    let transport = RecordingDaemonTransport()
    let client = DaemonVirtualMachineClient(
      endpoint: .local,
      transport: transport
    )

    let virtualMachines = try await client.listVirtualMachines()
    let vm = try XCTUnwrap(virtualMachines.first)

    let token = try await client.inspectGuestToolsToken(on: vm.id)
    let command = try await client.inspectGuestToolsLinuxCommand(transport: .device, on: vm.id)

    XCTAssertEqual(token.vm, "dev")
    XCTAssertEqual(token.createdAtUnix, 1_710_000_000)
    XCTAssertEqual(token.tokenLength, 18)
    XCTAssertEqual(command.vm, "dev")
    XCTAssertEqual(command.transport, .device)
    XCTAssertEqual(command.tokenFile, "/run/bridgevm-token.json")
    XCTAssertEqual(command.commandLine, "bridgevm-guest-tools run --transport device")
    let requestTypes = transport.requests.compactMap { $0["type"] as? String }
    XCTAssertEqual(requestTypes, ["list_vms", "guest_tools_token", "guest_tools_linux_command"])
    XCTAssertEqual(transport.requests[1]["name"] as? String, "dev")
    XCTAssertEqual(transport.requests[2]["name"] as? String, "dev")
    XCTAssertEqual(transport.requests[2]["transport"] as? String, "device")
  }

  func testDaemonClientMountsApprovedSharedFolderUsingNameFromListCache() async throws {
    let transport = RecordingDaemonTransport()
    let client = DaemonVirtualMachineClient(
      endpoint: .local,
      transport: transport
    )

    let virtualMachines = try await client.listVirtualMachines()
    let vm = try XCTUnwrap(virtualMachines.first)

    let status = try await client.mountApprovedSharedFolder(named: "workspace", on: vm.id)

    XCTAssertEqual(status?.vm, "dev")
    let folder = try XCTUnwrap(status?.approvedSharedFolders.first)
    XCTAssertEqual(status?.mountReadinessTitle(for: folder), "Mounted")
    let requestTypes = transport.requests.compactMap { $0["type"] as? String }
    XCTAssertEqual(
      requestTypes, ["list_vms", "guest_tools_mount_approved_share", "guest_tools_status"])
    XCTAssertEqual(transport.requests[1]["name"] as? String, "dev")
    XCTAssertEqual(transport.requests[1]["share"] as? String, "workspace")
  }

  func testDaemonClientUnmountsApprovedSharedFolderUsingNameFromListCache() async throws {
    let transport = RecordingDaemonTransport()
    let client = DaemonVirtualMachineClient(
      endpoint: .local,
      transport: transport
    )

    let virtualMachines = try await client.listVirtualMachines()
    let vm = try XCTUnwrap(virtualMachines.first)

    let status = try await client.unmountApprovedSharedFolder(named: "workspace", on: vm.id)

    XCTAssertEqual(status?.vm, "dev")
    let requestTypes = transport.requests.compactMap { $0["type"] as? String }
    XCTAssertEqual(requestTypes, ["list_vms", "guest_tools_send_command", "guest_tools_status"])
    XCTAssertEqual(transport.requests[1]["name"] as? String, "dev")
    let envelope = try XCTUnwrap(transport.requests[1]["envelope"] as? [String: Any])
    XCTAssertNil(envelope["request_id"] as? String)
    let message = try XCTUnwrap(envelope["message"] as? [String: Any])
    let unmount = try XCTUnwrap(message["UnmountShare"] as? [String: Any])
    XCTAssertEqual(unmount["name"] as? String, "workspace")
  }

  func testDaemonClientSendsFileDropGuestToolsCommandsUsingNameFromListCache() async throws {
    let transport = RecordingDaemonTransport()
    let client = DaemonVirtualMachineClient(
      endpoint: .local,
      transport: transport
    )

    let virtualMachines = try await client.listVirtualMachines()
    let vm = try XCTUnwrap(virtualMachines.first)

    let startDispatch = try await client.sendGuestToolsCommand(
      .fileDropStart(transferID: "drop-1", fileName: "notes.txt", sizeBytes: 5),
      requestID: "drop-start-1",
      on: vm.id
    )
    let chunkDispatch = try await client.sendGuestToolsCommand(
      .fileDropChunk(transferID: "drop-1", chunkIndex: 0, dataBase64: "aGVsbG8="),
      requestID: "drop-chunk-0",
      on: vm.id
    )
    let completeDispatch = try await client.sendGuestToolsCommand(
      .fileDropComplete(transferID: "drop-1"),
      requestID: "drop-complete-1",
      on: vm.id
    )

    XCTAssertEqual(
      startDispatch,
      GuestToolsCommandDispatch(
        vm: "dev",
        requestID: "drop-start-1",
        pendingCommands: 1
      ))
    XCTAssertEqual(
      chunkDispatch,
      GuestToolsCommandDispatch(
        vm: "dev",
        requestID: "drop-chunk-0",
        pendingCommands: 1
      ))
    XCTAssertEqual(
      completeDispatch,
      GuestToolsCommandDispatch(
        vm: "dev",
        requestID: "drop-complete-1",
        pendingCommands: 1
      ))
    let requestTypes = transport.requests.compactMap { $0["type"] as? String }
    XCTAssertEqual(
      requestTypes,
      [
        "list_vms",
        "guest_tools_send_command",
        "guest_tools_send_command",
        "guest_tools_send_command",
      ])
    XCTAssertEqual(transport.requests[1]["name"] as? String, "dev")
    let startEnvelope = try XCTUnwrap(transport.requests[1]["envelope"] as? [String: Any])
    XCTAssertEqual(startEnvelope["request_id"] as? String, "drop-start-1")
    let startMessage = try XCTUnwrap(startEnvelope["message"] as? [String: Any])
    let start = try XCTUnwrap(startMessage["FileDropStart"] as? [String: Any])
    XCTAssertEqual(start["file_name"] as? String, "notes.txt")
    XCTAssertEqual(start["size_bytes"] as? Int, 5)

    let chunkEnvelope = try XCTUnwrap(transport.requests[2]["envelope"] as? [String: Any])
    XCTAssertEqual(chunkEnvelope["request_id"] as? String, "drop-chunk-0")
    let chunkMessage = try XCTUnwrap(chunkEnvelope["message"] as? [String: Any])
    let chunk = try XCTUnwrap(chunkMessage["FileDropChunk"] as? [String: Any])
    XCTAssertEqual(chunk["transfer_id"] as? String, "drop-1")
    XCTAssertEqual(chunk["chunk_index"] as? Int, 0)
    XCTAssertEqual(chunk["data_base64"] as? String, "aGVsbG8=")

    let completeEnvelope = try XCTUnwrap(transport.requests[3]["envelope"] as? [String: Any])
    XCTAssertEqual(completeEnvelope["request_id"] as? String, "drop-complete-1")
    let completeMessage = try XCTUnwrap(completeEnvelope["message"] as? [String: Any])
    let complete = try XCTUnwrap(completeMessage["FileDropComplete"] as? [String: Any])
    XCTAssertEqual(complete["transfer_id"] as? String, "drop-1")
  }

  func testDaemonClientEncodesClipboardAndResizeGuestToolsCommands() async throws {
    let clipboardData = try JSONEncoder().encode(
      DaemonGuestToolsSendCommandRequest(
        name: "dev",
        command: .setClipboard(text: "hello"),
        requestID: "clipboard-1"
      )
    )
    let clipboard = try XCTUnwrap(
      JSONSerialization.jsonObject(with: clipboardData) as? [String: Any])
    XCTAssertEqual(clipboard["type"] as? String, "guest_tools_send_command")
    XCTAssertEqual(clipboard["name"] as? String, "dev")
    let clipboardEnvelope = try XCTUnwrap(clipboard["envelope"] as? [String: Any])
    XCTAssertEqual(clipboardEnvelope["request_id"] as? String, "clipboard-1")
    let clipboardMessage = try XCTUnwrap(clipboardEnvelope["message"] as? [String: Any])
    let setClipboard = try XCTUnwrap(clipboardMessage["SetClipboard"] as? [String: Any])
    XCTAssertEqual(setClipboard["text"] as? String, "hello")

    let resizeData = try JSONEncoder().encode(
      DaemonGuestToolsSendCommandRequest(
        name: "dev",
        command: .resizeDisplay(width: 1440, height: 900, scale: 2),
        requestID: "display-1"
      )
    )
    let resize = try XCTUnwrap(JSONSerialization.jsonObject(with: resizeData) as? [String: Any])
    let resizeEnvelope = try XCTUnwrap(resize["envelope"] as? [String: Any])
    XCTAssertEqual(resizeEnvelope["request_id"] as? String, "display-1")
    let resizeMessage = try XCTUnwrap(resizeEnvelope["message"] as? [String: Any])
    let resizeDisplay = try XCTUnwrap(resizeMessage["ResizeDisplay"] as? [String: Any])
    XCTAssertEqual(resizeDisplay["width"] as? Int, 1440)
    XCTAssertEqual(resizeDisplay["height"] as? Int, 900)
    XCTAssertEqual(resizeDisplay["scale"] as? Int, 2)
  }

  func testDaemonClientInspectsRunnerStatusUsingNameFromListCache() async throws {
    let transport = RecordingDaemonTransport()
    let client = DaemonVirtualMachineClient(
      endpoint: .local,
      transport: transport
    )

    let virtualMachines = try await client.listVirtualMachines()
    let vm = try XCTUnwrap(virtualMachines.first)

    let inspectedStatus = try await client.inspectRunnerStatus(on: vm.id)
    let status = try XCTUnwrap(inspectedStatus)

    XCTAssertEqual(status.engine, "lightvm")
    XCTAssertEqual(status.pid, 4242)
    XCTAssertEqual(status.launchReadiness?.ready, false)
    XCTAssertEqual(status.launchReadiness?.blockers.first?.code, "missing-primary-disk")
    XCTAssertEqual(status.guestTools?.channelName, "org.bridgevm.guest-tools.0")
    XCTAssertEqual(status.guestTools?.socketPath, "metadata/guest-tools.sock")
    XCTAssertEqual(status.runtimeControl?.kind, "apple-vz-display")
    XCTAssertEqual(status.runtimeControl?.socketPath, "run/apple-vz-display-control.sock")
    XCTAssertEqual(status.runtimeControl?.commandSummary, "status, stop, policy, pacing")
    let requestTypes = transport.requests.compactMap { $0["type"] as? String }
    XCTAssertEqual(requestTypes, ["list_vms", "runner_status"])
    XCTAssertEqual(transport.requests[1]["name"] as? String, "dev")
  }

  func testDaemonClientInspectsReadinessReportUsingNameFromListCache() async throws {
    let transport = RecordingDaemonTransport()
    let client = DaemonVirtualMachineClient(
      endpoint: .local,
      transport: transport
    )

    let virtualMachines = try await client.listVirtualMachines()
    let vm = try XCTUnwrap(virtualMachines.first)

    let report = try await client.inspectReadinessReport(on: vm.id)

    XCTAssertEqual(report.vm, "dev")
    XCTAssertEqual(report.mode, .fast)
    XCTAssertEqual(report.state, .stopped)
    XCTAssertEqual(report.bootMedia?.entries.first?.path, "installers/ubuntu-arm64.iso")
    XCTAssertEqual(report.snapshotChain?.activeDisk.path, "disks/root.qcow2")
    XCTAssertEqual(report.blockers, ["boot-media-missing:installers/ubuntu-arm64.iso"])
    let requestTypes = transport.requests.compactMap { $0["type"] as? String }
    XCTAssertEqual(requestTypes, ["list_vms", "readiness_report"])
    XCTAssertEqual(transport.requests[1]["name"] as? String, "dev")
  }

  func testDaemonClientPreparesRunUsingNameFromListCache() async throws {
    let transport = RecordingDaemonTransport()
    let client = DaemonVirtualMachineClient(
      endpoint: .local,
      transport: transport
    )

    let virtualMachines = try await client.listVirtualMachines()
    let vm = try XCTUnwrap(virtualMachines.first)

    let status = try await client.prepareRun(on: vm.id)

    XCTAssertEqual(status.engine, "lightvm")
    XCTAssertEqual(status.pid, 4242)
    XCTAssertEqual(status.command, ["lightvm-runner", "dev", "--apple-vz"])
    XCTAssertEqual(status.launchReadiness?.ready, false)
    XCTAssertEqual(status.launchReadiness?.blockers.first?.code, "missing-primary-disk")
    XCTAssertEqual(status.runtimeControl?.socketPath, "run/apple-vz-display-control.sock")
    let requestTypes = transport.requests.compactMap { $0["type"] as? String }
    XCTAssertEqual(requestTypes, ["list_vms", "prepare_run"])
    XCTAssertEqual(transport.requests[1]["name"] as? String, "dev")
  }

  func testDaemonClientInspectsLifecyclePlanUsingNameFromListCache() async throws {
    let transport = RecordingDaemonTransport()
    let client = DaemonVirtualMachineClient(
      endpoint: .local,
      transport: transport
    )

    let virtualMachines = try await client.listVirtualMachines()
    let vm = try XCTUnwrap(virtualMachines.first)

    let plan = try await client.inspectLifecyclePlan(action: .suspend, on: vm.id)

    XCTAssertEqual(plan.vm, "dev")
    XCTAssertEqual(plan.action, .suspend)
    XCTAssertEqual(plan.currentState, .running)
    XCTAssertEqual(plan.targetState, .suspended)
    XCTAssertEqual(plan.backend, "qemu-qmp")
    XCTAssertEqual(plan.qmpCommand, "stop")
    XCTAssertTrue(plan.socketAvailable)
    XCTAssertTrue(plan.executable)
    let requestTypes = transport.requests.compactMap { $0["type"] as? String }
    XCTAssertEqual(requestTypes, ["list_vms", "lifecycle_plan"])
    XCTAssertEqual(transport.requests[1]["name"] as? String, "dev")
    XCTAssertEqual(transport.requests[1]["action"] as? String, "suspend")
  }

  func testDaemonClientInspectsQemuArgsUsingNameFromListCache() async throws {
    let transport = RecordingDaemonTransport()
    let client = DaemonVirtualMachineClient(
      endpoint: .local,
      transport: transport
    )

    let virtualMachines = try await client.listVirtualMachines()
    let vm = try XCTUnwrap(virtualMachines.first)

    let plan = try await client.inspectQemuArgs(on: vm.id)

    XCTAssertEqual(plan.program, "qemu-system-aarch64")
    XCTAssertTrue(plan.args.contains("vmnet-host,id=net0"))
    XCTAssertEqual(plan.command.first, "qemu-system-aarch64")
    let requestTypes = transport.requests.compactMap { $0["type"] as? String }
    XCTAssertEqual(requestTypes, ["list_vms", "qemu_args"])
    XCTAssertEqual(transport.requests[1]["name"] as? String, "dev")
  }

  func testDaemonClientInspectsOpenPortPlanUsingNameFromListCache() async throws {
    let transport = RecordingDaemonTransport()
    let client = DaemonVirtualMachineClient(
      endpoint: .local,
      transport: transport
    )

    let virtualMachines = try await client.listVirtualMachines()
    let vm = try XCTUnwrap(virtualMachines.first)

    let plan = try await client.inspectOpenPortPlan(guestPort: 80, scheme: "https", on: vm.id)

    XCTAssertEqual(plan.vm, "dev")
    XCTAssertEqual(plan.scheme, "https")
    XCTAssertEqual(plan.host, "127.0.0.1")
    XCTAssertEqual(plan.guestPort, 80)
    XCTAssertEqual(plan.hostPort, 18080)
    XCTAssertEqual(plan.url, "https://127.0.0.1:18080")
    XCTAssertEqual(plan.command, ["open", "https://127.0.0.1:18080"])
    let requestTypes = transport.requests.compactMap { $0["type"] as? String }
    XCTAssertEqual(requestTypes, ["list_vms", "open_port"])
    XCTAssertEqual(transport.requests[1]["name"] as? String, "dev")
    XCTAssertEqual(transport.requests[1]["guest"] as? Int, 80)
    XCTAssertEqual(transport.requests[1]["scheme"] as? String, "https")
  }

  func testDaemonClientInspectsNetworkPlanUsingNameFromListCache() async throws {
    let transport = RecordingDaemonTransport()
    let client = DaemonVirtualMachineClient(
      endpoint: .local,
      transport: transport
    )

    let virtualMachines = try await client.listVirtualMachines()
    let vm = try XCTUnwrap(virtualMachines.first)

    let plan = try await client.inspectNetworkPlan(on: vm.id)

    XCTAssertEqual(plan.vm, "dev")
    XCTAssertEqual(plan.backend, "apple-vz")
    XCTAssertEqual(plan.mode, "nat")
    XCTAssertEqual(plan.hostname, "dev")
    XCTAssertTrue(plan.dryRun)
    XCTAssertTrue(plan.executable)
    XCTAssertEqual(plan.portForwards, [VMPortForward(host: 2222, guest: 22)])
    XCTAssertEqual(plan.capabilities?.supportsPortForwarding, true)
    XCTAssertTrue(plan.blockers.isEmpty)
    XCTAssertEqual(plan.notes.count, 1)
    let requestTypes = transport.requests.compactMap { $0["type"] as? String }
    XCTAssertEqual(requestTypes, ["list_vms", "plan_network"])
    XCTAssertEqual(transport.requests[1]["name"] as? String, "dev")
  }

  func testDaemonClientInspectsSSHPlanUsingNameFromListCache() async throws {
    let transport = RecordingDaemonTransport()
    let client = DaemonVirtualMachineClient(
      endpoint: .local,
      transport: transport
    )

    let virtualMachines = try await client.listVirtualMachines()
    let vm = try XCTUnwrap(virtualMachines.first)

    let plan = try await client.inspectSSHPlan(user: "ubuntu", on: vm.id)

    XCTAssertEqual(plan.vm, "dev")
    XCTAssertEqual(plan.user, "ubuntu")
    XCTAssertEqual(plan.host, "127.0.0.1")
    XCTAssertEqual(plan.port, 2222)
    XCTAssertEqual(plan.source, .portForward)
    XCTAssertEqual(plan.command, ["ssh", "-p", "2222", "ubuntu@127.0.0.1"])
    let requestTypes = transport.requests.compactMap { $0["type"] as? String }
    XCTAssertEqual(requestTypes, ["list_vms", "ssh_plan"])
    XCTAssertEqual(transport.requests[1]["name"] as? String, "dev")
    XCTAssertEqual(transport.requests[1]["user"] as? String, "ubuntu")
  }

  func testDaemonClientManagesPortForwardManifestUsingNameFromListCache() async throws {
    let transport = RecordingDaemonTransport()
    let client = DaemonVirtualMachineClient(
      endpoint: .local,
      transport: transport
    )

    let virtualMachines = try await client.listVirtualMachines()
    let vm = try XCTUnwrap(virtualMachines.first)

    let listed = try await client.listPortForwards(on: vm.id)
    let added = try await client.addPortForward(host: 2222, guest: 22, on: vm.id)
    let removed = try await client.removePortForward(host: 2222, guest: 22, on: vm.id)

    XCTAssertEqual(listed.vm, "dev")
    XCTAssertTrue(listed.forwards.isEmpty)
    XCTAssertEqual(added.forwards, [VMPortForward(host: 2222, guest: 22)])
    XCTAssertTrue(removed.forwards.isEmpty)

    let requestTypes = transport.requests.compactMap { $0["type"] as? String }
    XCTAssertEqual(requestTypes, ["list_vms", "list_ports", "add_port", "remove_port"])
    XCTAssertEqual(transport.requests[1]["name"] as? String, "dev")
    XCTAssertEqual(transport.requests[2]["name"] as? String, "dev")
    XCTAssertEqual(transport.requests[2]["host"] as? Int, 2222)
    XCTAssertEqual(transport.requests[2]["guest"] as? Int, 22)
    XCTAssertEqual(transport.requests[3]["name"] as? String, "dev")
    XCTAssertEqual(transport.requests[3]["host"] as? Int, 2222)
    XCTAssertEqual(transport.requests[3]["guest"] as? Int, 22)
  }

  func testDaemonClientManagesSharedFolderManifestUsingNameFromListCache() async throws {
    let transport = RecordingDaemonTransport()
    let client = DaemonVirtualMachineClient(
      endpoint: .local,
      transport: transport
    )

    let virtualMachines = try await client.listVirtualMachines()
    let vm = try XCTUnwrap(virtualMachines.first)

    let listed = try await client.listSharedFolders(on: vm.id)
    let added = try await client.addSharedFolder(
      named: "workspace",
      hostPath: "/Users/dev/workspace",
      readOnly: true,
      hostPathToken: "share-token-workspace",
      on: vm.id
    )
    let removed = try await client.removeSharedFolder(named: "workspace", on: vm.id)

    XCTAssertEqual(listed.vm, "dev")
    XCTAssertTrue(listed.sharedFolders.isEmpty)
    XCTAssertEqual(
      added.sharedFolders,
      [
        VMSharedFolder(
          name: "workspace",
          hostPath: "/Users/dev/workspace",
          readOnly: true,
          hostPathToken: "share-token-workspace"
        )
      ])
    XCTAssertTrue(removed.sharedFolders.isEmpty)

    let requestTypes = transport.requests.compactMap { $0["type"] as? String }
    XCTAssertEqual(requestTypes, ["list_vms", "list_shares", "add_share", "remove_share"])
    XCTAssertEqual(transport.requests[1]["name"] as? String, "dev")
    XCTAssertEqual(transport.requests[2]["name"] as? String, "dev")
    XCTAssertEqual(transport.requests[2]["share"] as? String, "workspace")
    XCTAssertEqual(transport.requests[2]["host_path"] as? String, "/Users/dev/workspace")
    XCTAssertEqual(transport.requests[2]["read_only"] as? Bool, true)
    XCTAssertEqual(transport.requests[2]["host_path_token"] as? String, "share-token-workspace")
    XCTAssertEqual(transport.requests[3]["name"] as? String, "dev")
    XCTAssertEqual(transport.requests[3]["share"] as? String, "workspace")
  }

  func testDaemonClientInspectsSnapshotPreflightUsingNameFromListCache() async throws {
    let transport = RecordingDaemonTransport()
    let client = DaemonVirtualMachineClient(
      endpoint: .local,
      transport: transport
    )

    let virtualMachines = try await client.listVirtualMachines()
    let vm = try XCTUnwrap(virtualMachines.first)

    let status = try await client.inspectSnapshotPreflightStatus(on: vm.id)

    XCTAssertEqual(status.vm, "dev")
    XCTAssertEqual(status.consistency, .applicationConsistent)
    XCTAssertFalse(status.backendFreezeThawSupported)
    XCTAssertEqual(status.readinessTitle, "Scaffold only")
    XCTAssertEqual(status.blockers.first?.code, "backend-freeze-thaw-unavailable")
    let requestTypes = transport.requests.compactMap { $0["type"] as? String }
    XCTAssertEqual(requestTypes, ["list_vms", "snapshot_preflight_status"])
    XCTAssertEqual(transport.requests[1]["name"] as? String, "dev")
    XCTAssertEqual(transport.requests[1]["consistency"] as? String, "application-consistent")
  }

  func testDaemonClientListsAndRestoresSnapshotsUsingNameFromListCache() async throws {
    let transport = RecordingDaemonTransport()
    let client = DaemonVirtualMachineClient(
      endpoint: .local,
      transport: transport
    )

    let virtualMachines = try await client.listVirtualMachines()
    let vm = try XCTUnwrap(virtualMachines.first)

    let snapshots = try await client.listSnapshots(on: vm.id)
    let restore = try await client.restoreSnapshot(named: "paused-state", on: vm.id)

    XCTAssertEqual(snapshots.map(\.name), ["before-upgrade", "paused-state"])
    XCTAssertEqual(snapshots.first?.kind, .disk)
    XCTAssertEqual(snapshots.last?.vmState, .suspended)
    XCTAssertEqual(restore.snapshot, "paused-state")
    XCTAssertEqual(restore.restoredState, .suspended)
    XCTAssertEqual(restore.suspendImage?.imagePath, "snapshots/paused-state/suspend.img")
    let requestTypes = transport.requests.compactMap { $0["type"] as? String }
    XCTAssertEqual(requestTypes, ["list_vms", "list_snapshots", "restore_snapshot"])
    XCTAssertEqual(transport.requests[1]["vm"] as? String, "dev")
    XCTAssertEqual(transport.requests[2]["vm"] as? String, "dev")
    XCTAssertEqual(transport.requests[2]["name"] as? String, "paused-state")
  }

  func testDaemonClientCreatesSnapshotMetadataUsingNameFromListCache() async throws {
    let transport = RecordingDaemonTransport()
    let client = DaemonVirtualMachineClient(
      endpoint: .local,
      transport: transport
    )

    let virtualMachines = try await client.listVirtualMachines()
    let vm = try XCTUnwrap(virtualMachines.first)

    let snapshot = try await client.createSnapshot(
      named: "before-upgrade",
      kind: .applicationConsistent,
      on: vm.id
    )

    XCTAssertEqual(snapshot.name, "before-upgrade")
    XCTAssertEqual(snapshot.kind, .applicationConsistent)
    XCTAssertEqual(snapshot.vmState, .running)
    let requestTypes = transport.requests.compactMap { $0["type"] as? String }
    XCTAssertEqual(requestTypes, ["list_vms", "create_snapshot"])
    XCTAssertEqual(transport.requests[1]["vm"] as? String, "dev")
    XCTAssertEqual(transport.requests[1]["name"] as? String, "before-upgrade")
    XCTAssertEqual(transport.requests[1]["kind"] as? String, "application-consistent")
  }

  func testDaemonClientInspectsSnapshotChainUsingNameFromListCache() async throws {
    let transport = RecordingDaemonTransport()
    let client = DaemonVirtualMachineClient(
      endpoint: .local,
      transport: transport
    )

    let virtualMachines = try await client.listVirtualMachines()
    let vm = try XCTUnwrap(virtualMachines.first)

    let chain = try await client.inspectSnapshotChain(on: vm.id)

    XCTAssertEqual(chain.activeDisk.source, "snapshot-overlay")
    XCTAssertEqual(chain.activeDisk.snapshot, "before-upgrade")
    XCTAssertEqual(chain.activeDisk.path, "disks/snapshots/before-upgrade.qcow2")
    XCTAssertEqual(chain.readinessTitle, "Chain ready")
    let disk = try XCTUnwrap(chain.disks.first)
    XCTAssertEqual(disk.snapshot, "before-upgrade")
    XCTAssertEqual(disk.backingPath, "disks/root.qcow2")
    XCTAssertEqual(
      disk.createCommand,
      [
        "qemu-img",
        "create",
        "-f",
        "qcow2",
        "-F",
        "qcow2",
        "-b",
        "disks/root.qcow2",
        "disks/snapshots/before-upgrade.qcow2",
      ])
    let requestTypes = transport.requests.compactMap { $0["type"] as? String }
    XCTAssertEqual(requestTypes, ["list_vms", "snapshot_chain"])
    XCTAssertEqual(transport.requests[1]["vm"] as? String, "dev")
  }

  func testDaemonClientCreatesSnapshotDiskUsingNameFromListCache() async throws {
    let transport = RecordingDaemonTransport()
    let client = DaemonVirtualMachineClient(
      endpoint: .local,
      transport: transport
    )

    let virtualMachines = try await client.listVirtualMachines()
    let vm = try XCTUnwrap(virtualMachines.first)

    let creation = try await client.createSnapshotDisk(named: "before-upgrade", on: vm.id)

    XCTAssertEqual(creation.snapshot, "before-upgrade")
    XCTAssertEqual(creation.disk.snapshot, "before-upgrade")
    XCTAssertEqual(creation.disk.overlayPath, "disks/snapshots/before-upgrade.qcow2")
    XCTAssertEqual(creation.disk.backingPath, "disks/root.qcow2")
    XCTAssertEqual(
      creation.disk.createCommand,
      [
        "qemu-img",
        "create",
        "-f",
        "qcow2",
        "-F",
        "qcow2",
        "-b",
        "disks/root.qcow2",
        "disks/snapshots/before-upgrade.qcow2",
      ])
    XCTAssertTrue(creation.executed)
    XCTAssertEqual(creation.exitStatus, "exit status: 0")
    XCTAssertEqual(creation.createdAtUnix, 1_710_000_360)
    let requestTypes = transport.requests.compactMap { $0["type"] as? String }
    XCTAssertEqual(requestTypes, ["list_vms", "create_snapshot_disk"])
    XCTAssertEqual(transport.requests[1]["vm"] as? String, "dev")
    XCTAssertEqual(transport.requests[1]["name"] as? String, "before-upgrade")
  }

  func testDaemonClientPreparesCreatesAndInspectsPrimaryDiskUsingNameFromListCache() async throws {
    let transport = RecordingDaemonTransport()
    let client = DaemonVirtualMachineClient(
      endpoint: .local,
      transport: transport
    )

    let virtualMachines = try await client.listVirtualMachines()
    let vm = try XCTUnwrap(virtualMachines.first)

    let preparation = try await client.preparePrimaryDisk(on: vm.id)
    let creation = try await client.createPrimaryDisk(on: vm.id)
    let inspection = try await client.inspectPrimaryDisk(on: vm.id)

    XCTAssertEqual(preparation.path, "disks/root.qcow2")
    XCTAssertEqual(preparation.size, "80G")
    XCTAssertEqual(creation.commandLine, "qemu-img create -f qcow2 disks/root.qcow2 80G")
    XCTAssertTrue(creation.executed)
    XCTAssertEqual(inspection.commandLine, "qemu-img info --output=json disks/root.qcow2")
    XCTAssertEqual(
      inspection.infoValue,
      .object([
        "filename": .string("disks/root.qcow2"),
        "format": .string("qcow2"),
        "virtual-size": .int(85_899_345_920),
      ]))

    let requestTypes = transport.requests.compactMap { $0["type"] as? String }
    XCTAssertEqual(requestTypes, ["list_vms", "prepare_disk", "create_disk", "inspect_disk"])
    XCTAssertEqual(transport.requests[1]["name"] as? String, "dev")
    XCTAssertEqual(transport.requests[2]["name"] as? String, "dev")
    XCTAssertEqual(transport.requests[3]["name"] as? String, "dev")
  }

  func testDaemonClientVerifiesAndCompactsActiveDiskUsingNameFromListCache() async throws {
    let transport = RecordingDaemonTransport()
    let client = DaemonVirtualMachineClient(
      endpoint: .local,
      transport: transport
    )

    let virtualMachines = try await client.listVirtualMachines()
    let vm = try XCTUnwrap(virtualMachines.first)

    let verification = try await client.verifyActiveDisk(on: vm.id)
    let compaction = try await client.compactActiveDisk(on: vm.id)

    XCTAssertEqual(verification.activeDisk.path, "disks/root.qcow2")
    XCTAssertEqual(verification.command.prefix(2), ["qemu-img", "check"])
    XCTAssertEqual(
      verification.reportValue,
      .object([
        "check-errors": .int(0),
        "image-end-offset": .int(4096),
      ]))
    XCTAssertEqual(compaction.preparation.size, "80G")
    XCTAssertEqual(compaction.activeDisk.path, "disks/root.qcow2")
    XCTAssertEqual(compaction.savedBytes, 4096)

    let requestTypes = transport.requests.compactMap { $0["type"] as? String }
    XCTAssertEqual(requestTypes, ["list_vms", "verify_disk", "compact_disk"])
    XCTAssertEqual(transport.requests[1]["name"] as? String, "dev")
    XCTAssertEqual(transport.requests[2]["name"] as? String, "dev")
  }

  func testDaemonClientRepairsMetadataUsingNameFromListCache() async throws {
    let transport = RecordingDaemonTransport()
    let client = DaemonVirtualMachineClient(
      endpoint: .local,
      transport: transport
    )

    let virtualMachines = try await client.listVirtualMachines()
    let vm = try XCTUnwrap(virtualMachines.first)

    let repair = try await client.repairMetadata(on: vm.id)

    XCTAssertEqual(repair.vm, "dev")
    XCTAssertEqual(repair.bundle, "/tmp/dev.vmbridge")
    XCTAssertTrue(repair.repaired)
    XCTAssertEqual(repair.actions.first?.action, "repaired")
    XCTAssertEqual(repair.actions.first?.path, "/tmp/dev.vmbridge/metadata/runtime.json")
    XCTAssertEqual(repair.repairedAtUnix, 42)

    let requestTypes = transport.requests.compactMap { $0["type"] as? String }
    XCTAssertEqual(requestTypes, ["list_vms", "repair_metadata"])
    XCTAssertEqual(transport.requests[1]["name"] as? String, "dev")
  }

  func testDaemonClientMigratesManifestUsingNameFromListCache() async throws {
    let transport = RecordingDaemonTransport()
    let client = DaemonVirtualMachineClient(
      endpoint: .local,
      transport: transport
    )

    let virtualMachines = try await client.listVirtualMachines()
    let vm = try XCTUnwrap(virtualMachines.first)

    let dryRun = try await client.migrateManifest(on: vm.id, dryRun: true)
    let migration = try await client.migrateManifest(on: vm.id, dryRun: false)

    XCTAssertEqual(dryRun.vm, "dev")
    XCTAssertTrue(dryRun.dryRun)
    XCTAssertFalse(dryRun.migrated)
    XCTAssertNil(dryRun.backupPath)
    XCTAssertNil(dryRun.receiptPath)
    XCTAssertEqual(dryRun.fromSchema, "bridgevm.io/v1")
    XCTAssertEqual(dryRun.toSchema, "bridgevm.io/v1")

    XCTAssertEqual(migration.vm, "dev")
    XCTAssertFalse(migration.dryRun)
    XCTAssertFalse(migration.migrated)
    XCTAssertEqual(migration.bundle, "/tmp/dev.vmbridge")
    XCTAssertEqual(migration.manifestPath, "/tmp/dev.vmbridge/manifest.yaml")
    XCTAssertEqual(
      migration.backupPath, "/tmp/dev.vmbridge/metadata/manifest-before-migration.yaml")
    XCTAssertEqual(migration.receiptPath, "/tmp/dev.vmbridge/metadata/manifest-migration.json")
    XCTAssertEqual(migration.migratedAtUnix, 1_710_001_300)
    XCTAssertEqual(
      migration.actions,
      [
        "validated current manifest schema",
        "copied manifest before migration",
        "wrote migration receipt",
      ])

    let requestTypes = transport.requests.compactMap { $0["type"] as? String }
    XCTAssertEqual(requestTypes, ["list_vms", "migrate_manifest", "migrate_manifest"])
    XCTAssertEqual(transport.requests[1]["name"] as? String, "dev")
    XCTAssertEqual(transport.requests[1]["dry_run"] as? Bool, true)
    XCTAssertEqual(transport.requests[2]["name"] as? String, "dev")
    XCTAssertEqual(transport.requests[2]["dry_run"] as? Bool, false)
  }

  func testDaemonClientExecutesApplicationConsistentSnapshotUsingNameFromListCache()
    async throws
  {
    let transport = RecordingDaemonTransport()
    let client = DaemonVirtualMachineClient(
      endpoint: .local,
      transport: transport
    )

    let virtualMachines = try await client.listVirtualMachines()
    let vm = try XCTUnwrap(virtualMachines.first)

    let execution = try await client.executeApplicationConsistentSnapshot(
      named: "before-upgrade",
      freezeTimeoutMillis: 5_000,
      on: vm.id
    )

    XCTAssertEqual(execution.vm, "dev")
    XCTAssertEqual(execution.snapshot, "before-upgrade")
    XCTAssertEqual(execution.freezeRequestID, "freeze-1")
    XCTAssertEqual(execution.thawRequestID, "thaw-1")
    XCTAssertEqual(execution.freezeResult.capability, "fs-freeze")
    XCTAssertEqual(execution.thawResult.capability, "fs-thaw")
    XCTAssertEqual(execution.summaryTitle, "Snapshot executed")
    let requestTypes = transport.requests.compactMap { $0["type"] as? String }
    XCTAssertEqual(requestTypes, ["list_vms", "execute_application_consistent_snapshot"])
    XCTAssertEqual(transport.requests[1]["vm"] as? String, "dev")
    XCTAssertEqual(transport.requests[1]["name"] as? String, "before-upgrade")
    XCTAssertEqual(transport.requests[1]["freeze_timeout_millis"] as? Int, 5_000)
  }

  func testDaemonClientReappliesRuntimeResourcesUsingNameFromListCache() async throws {
    let transport = RecordingDaemonTransport()
    let client = DaemonVirtualMachineClient(
      endpoint: .local,
      transport: transport
    )

    let virtualMachines = try await client.listVirtualMachines()
    let vm = try XCTUnwrap(virtualMachines.first)

    let policy = try await client.reapplyRuntimeResources(
      visibility: .background,
      on: vm.id
    )

    XCTAssertEqual(policy.vm, "dev")
    XCTAssertEqual(policy.mode, "fast")
    XCTAssertEqual(policy.visibility, .background)
    XCTAssertEqual(policy.memory, "2048")
    XCTAssertEqual(policy.cpu, "1")
    XCTAssertFalse(policy.liveApplied)
    XCTAssertTrue(policy.runtimeControlAcknowledged)
    XCTAssertEqual(policy.liveApplyBlockers.first?.code, "runtime-control-unavailable")
    let requestTypes = transport.requests.compactMap { $0["type"] as? String }
    XCTAssertEqual(requestTypes, ["list_vms", "reapply_runtime_resources"])
    XCTAssertEqual(transport.requests[1]["name"] as? String, "dev")
    XCTAssertEqual(transport.requests[1]["visibility"] as? String, "background")
  }

  func testDaemonClientInspectsQMPStatusUsingNameFromListCache() async throws {
    let transport = RecordingDaemonTransport()
    let client = DaemonVirtualMachineClient(
      endpoint: .local,
      transport: transport
    )

    let virtualMachines = try await client.listVirtualMachines()
    let vm = try XCTUnwrap(virtualMachines.first)

    let status = try await client.inspectQMPStatus(on: vm.id)

    XCTAssertEqual(status.socketPath, "/tmp/dev.vmbridge/run/qmp.sock")
    XCTAssertTrue(status.available)
    XCTAssertEqual(status.status, "running")
    XCTAssertEqual(status.running, true)
    XCTAssertEqual(status.readinessTitle, "running")
    let requestTypes = transport.requests.compactMap { $0["type"] as? String }
    XCTAssertEqual(requestTypes, ["list_vms", "qmp_status"])
    XCTAssertEqual(transport.requests[1]["name"] as? String, "dev")
  }

  func testDaemonClientViewsLogsUsingNameFromListCache() async throws {
    let transport = RecordingDaemonTransport()
    let client = DaemonVirtualMachineClient(
      endpoint: .local,
      transport: transport
    )

    let virtualMachines = try await client.listVirtualMachines()
    let vm = try XCTUnwrap(virtualMachines.first)

    let log = try await client.viewLogs(kind: .serial, bytes: 4096, on: vm.id)

    XCTAssertEqual(log.vm, "dev")
    XCTAssertEqual(log.kind, .serial)
    XCTAssertEqual(log.path, "/tmp/dev.vmbridge/logs/serial.log")
    XCTAssertTrue(log.exists)
    XCTAssertEqual(log.bytes, 128)
    XCTAssertEqual(log.returnedBytes, 32)
    XCTAssertTrue(log.truncated)
    XCTAssertEqual(log.content, "log tail")
    let requestTypes = transport.requests.compactMap { $0["type"] as? String }
    XCTAssertEqual(requestTypes, ["list_vms", "view_logs"])
    XCTAssertEqual(transport.requests[1]["name"] as? String, "dev")
    XCTAssertEqual(transport.requests[1]["kind"] as? String, "serial")
    XCTAssertEqual(transport.requests[1]["max_bytes"] as? Int, 4096)
  }

  func testDaemonClientCreatesDiagnosticAndPerformanceArtifactsUsingNameFromListCache()
    async throws
  {
    let transport = RecordingDaemonTransport()
    let client = DaemonVirtualMachineClient(
      endpoint: .local,
      transport: transport
    )

    let virtualMachines = try await client.listVirtualMachines()
    let vm = try XCTUnwrap(virtualMachines.first)

    let bundle = try await client.createDiagnosticBundle(output: "/tmp/diagnostics", on: vm.id)
    let baseline = try await client.createPerformanceBaseline(output: "/tmp/performance", on: vm.id)
    let sample = try await client.createPerformanceSample(
      output: "/tmp/performance",
      artifactBytes: 4096,
      iterations: 1,
      sync: true,
      on: vm.id
    )

    XCTAssertEqual(bundle.vm, "dev")
    XCTAssertEqual(bundle.fileCountTitle, "3 files")
    XCTAssertEqual(baseline.vm, "dev")
    XCTAssertEqual(baseline.state, .running)
    XCTAssertTrue(baseline.metadataOnly)
    XCTAssertEqual(sample.vm, "dev")
    XCTAssertEqual(sample.artifactBytes, 4096)
    XCTAssertEqual(sample.iterationResults.first?.writeLatencyMicroseconds, 80)

    let requestTypes = transport.requests.compactMap { $0["type"] as? String }
    XCTAssertEqual(
      requestTypes,
      [
        "list_vms",
        "create_diagnostic_bundle",
        "create_performance_baseline",
        "create_performance_sample",
      ])
    XCTAssertEqual(transport.requests[1]["name"] as? String, "dev")
    XCTAssertEqual(transport.requests[1]["output"] as? String, "/tmp/diagnostics")
    XCTAssertEqual(transport.requests[2]["name"] as? String, "dev")
    XCTAssertEqual(transport.requests[2]["output"] as? String, "/tmp/performance")
    XCTAssertEqual(transport.requests[3]["name"] as? String, "dev")
    XCTAssertEqual(transport.requests[3]["output"] as? String, "/tmp/performance")
    XCTAssertEqual(transport.requests[3]["artifact_bytes"] as? Int, 4096)
    XCTAssertEqual(transport.requests[3]["iterations"] as? Int, 1)
    XCTAssertEqual(transport.requests[3]["sync"] as? Bool, true)
  }

  func testDaemonClientImportsBootMediaUsingNameFromListCache() async throws {
    let transport = RecordingDaemonTransport()
    let client = DaemonVirtualMachineClient(
      endpoint: .local,
      transport: transport
    )

    let virtualMachines = try await client.listVirtualMachines()
    let vm = try XCTUnwrap(virtualMachines.first)

    let imported = try await client.importBootMedia(
      sourcePath: "/tmp/dev.iso",
      kind: .installerImage,
      on: vm.id
    )

    XCTAssertEqual(imported.vm, "dev")
    XCTAssertEqual(imported.kind, .installerImage)
    XCTAssertEqual(imported.source, "/tmp/dev.iso")
    let requestTypes = transport.requests.compactMap { $0["type"] as? String }
    XCTAssertEqual(requestTypes, ["list_vms", "import_boot_media"])
    XCTAssertEqual(transport.requests[1]["name"] as? String, "dev")
    XCTAssertEqual(transport.requests[1]["source"] as? String, "/tmp/dev.iso")
    XCTAssertEqual(transport.requests[1]["kind"] as? String, "installer-image")
  }

  func testDaemonClientVerifiesBootMediaUsingNameFromListCache() async throws {
    let transport = RecordingDaemonTransport()
    let client = DaemonVirtualMachineClient(
      endpoint: .local,
      transport: transport
    )

    let virtualMachines = try await client.listVirtualMachines()
    let vm = try XCTUnwrap(virtualMachines.first)

    let verification = try await client.verifyBootMedia(
      expectedSHA256: "abc",
      kind: .installerImage,
      on: vm.id
    )

    XCTAssertEqual(verification.vm, "dev")
    XCTAssertEqual(verification.kind, .installerImage)
    XCTAssertTrue(verification.verified)
    let requestTypes = transport.requests.compactMap { $0["type"] as? String }
    XCTAssertEqual(requestTypes, ["list_vms", "verify_boot_media"])
    XCTAssertEqual(transport.requests[1]["name"] as? String, "dev")
    XCTAssertEqual(transport.requests[1]["expected_sha256"] as? String, "abc")
    XCTAssertEqual(transport.requests[1]["kind"] as? String, "installer-image")
  }

  func testDaemonClientPlansBootMediaDownloadUsingNameFromListCache() async throws {
    let transport = RecordingDaemonTransport()
    let client = DaemonVirtualMachineClient(
      endpoint: .local,
      transport: transport
    )

    let virtualMachines = try await client.listVirtualMachines()
    let vm = try XCTUnwrap(virtualMachines.first)

    let plan = try await client.planBootMediaDownload(
      url: "https://example.invalid/dev.iso",
      expectedSHA256: "abc",
      kind: .installerImage,
      on: vm.id
    )

    XCTAssertEqual(plan.vm, "dev")
    XCTAssertEqual(plan.kind, .installerImage)
    XCTAssertEqual(plan.expectedSHA256, "abc")
    let requestTypes = transport.requests.compactMap { $0["type"] as? String }
    XCTAssertEqual(requestTypes, ["list_vms", "plan_boot_media_download"])
    XCTAssertEqual(transport.requests[1]["name"] as? String, "dev")
    XCTAssertEqual(transport.requests[1]["url"] as? String, "https://example.invalid/dev.iso")
    XCTAssertEqual(transport.requests[1]["expected_sha256"] as? String, "abc")
    XCTAssertEqual(transport.requests[1]["kind"] as? String, "installer-image")
  }

  func testDaemonClientDownloadsBootMediaUsingNameFromListCache() async throws {
    let transport = RecordingDaemonTransport()
    let client = DaemonVirtualMachineClient(
      endpoint: .local,
      transport: transport
    )

    let virtualMachines = try await client.listVirtualMachines()
    let vm = try XCTUnwrap(virtualMachines.first)

    let download = try await client.downloadBootMedia(
      kind: .installerImage,
      on: vm.id
    )

    XCTAssertEqual(download.vm, "dev")
    XCTAssertEqual(download.kind, .installerImage)
    XCTAssertTrue(download.downloaded)
    let requestTypes = transport.requests.compactMap { $0["type"] as? String }
    XCTAssertEqual(requestTypes, ["list_vms", "download_boot_media"])
    XCTAssertEqual(transport.requests[1]["name"] as? String, "dev")
    XCTAssertEqual(transport.requests[1]["kind"] as? String, "installer-image")
  }
}

private enum TestClientError: Error {
  case primaryFailed
  case fallbackCalled
}

private enum RecordingFallbackCall: Equatable {
  case listVirtualMachines
  case inspectLifecyclePlan
  case createVirtualMachine
  case addPortForward
  case perform
}

extension VirtualMachineClient {
  fileprivate func listVirtualMachines() async throws -> [VirtualMachine] {
    throw TestClientError.fallbackCalled
  }
  fileprivate func listBootTemplates() async throws -> [BootTemplate] {
    throw TestClientError.fallbackCalled
  }
  fileprivate func inspectBootMediaStatus(on id: VirtualMachine.ID) async throws -> BootMediaStatus
  {
    throw TestClientError.fallbackCalled
  }
  fileprivate func importBootMedia(
    sourcePath: String,
    kind: BootMediaStatusEntry.Kind?,
    on id: VirtualMachine.ID
  ) async throws -> BootMediaImportMetadata { throw TestClientError.fallbackCalled }
  fileprivate func verifyBootMedia(
    expectedSHA256: String,
    kind: BootMediaStatusEntry.Kind?,
    on id: VirtualMachine.ID
  ) async throws -> BootMediaVerificationMetadata { throw TestClientError.fallbackCalled }
  fileprivate func planBootMediaDownload(
    url: String,
    expectedSHA256: String?,
    kind: BootMediaStatusEntry.Kind?,
    on id: VirtualMachine.ID
  ) async throws -> BootMediaDownloadPlanMetadata { throw TestClientError.fallbackCalled }
  fileprivate func downloadBootMedia(
    kind: BootMediaStatusEntry.Kind?,
    on id: VirtualMachine.ID
  ) async throws -> BootMediaDownloadResultMetadata { throw TestClientError.fallbackCalled }
  fileprivate func inspectLifecyclePlan(action: LifecyclePlanAction, on id: VirtualMachine.ID)
    async throws
    -> LifecyclePlan
  { throw TestClientError.fallbackCalled }
  fileprivate func inspectOpenPortPlan(
    guestPort: UInt16,
    scheme: String,
    on id: VirtualMachine.ID
  ) async throws -> OpenPortPlan { throw TestClientError.fallbackCalled }
  fileprivate func inspectSSHPlan(user: String, on id: VirtualMachine.ID) async throws -> SSHPlan {
    throw TestClientError.fallbackCalled
  }
  fileprivate func listPortForwards(on id: VirtualMachine.ID) async throws -> VMPortForwardList {
    throw TestClientError.fallbackCalled
  }
  fileprivate func addPortForward(host: UInt16, guest: UInt16, on id: VirtualMachine.ID)
    async throws
    -> VMPortForwardList
  { throw TestClientError.fallbackCalled }
  fileprivate func removePortForward(host: UInt16, guest: UInt16, on id: VirtualMachine.ID)
    async throws
    -> VMPortForwardList
  { throw TestClientError.fallbackCalled }
  fileprivate func listSharedFolders(on id: VirtualMachine.ID) async throws -> VMSharedFolderList {
    throw TestClientError.fallbackCalled
  }
  fileprivate func addSharedFolder(
    named shareName: String,
    hostPath: String,
    readOnly: Bool,
    hostPathToken: String?,
    on id: VirtualMachine.ID
  ) async throws -> VMSharedFolderList { throw TestClientError.fallbackCalled }
  fileprivate func removeSharedFolder(named shareName: String, on id: VirtualMachine.ID)
    async throws
    -> VMSharedFolderList
  { throw TestClientError.fallbackCalled }
  fileprivate func inspectGuestToolsStatus(on id: VirtualMachine.ID) async throws
    -> GuestToolsStatus
  {
    throw TestClientError.fallbackCalled
  }
  fileprivate func mountApprovedSharedFolder(named shareName: String, on id: VirtualMachine.ID)
    async throws
    -> GuestToolsStatus?
  { throw TestClientError.fallbackCalled }
  fileprivate func unmountApprovedSharedFolder(named shareName: String, on id: VirtualMachine.ID)
    async throws
    -> GuestToolsStatus?
  { throw TestClientError.fallbackCalled }
  fileprivate func sendGuestToolsCommand(
    _ command: GuestToolsAgentCommand,
    requestID: String?,
    on id: VirtualMachine.ID
  ) async throws -> GuestToolsCommandDispatch { throw TestClientError.fallbackCalled }
  fileprivate func inspectSnapshotPreflightStatus(on id: VirtualMachine.ID) async throws
    -> SnapshotPreflightStatus
  { throw TestClientError.fallbackCalled }
  fileprivate func listSnapshots(on id: VirtualMachine.ID) async throws -> [VMSnapshot] {
    throw TestClientError.fallbackCalled
  }
  fileprivate func inspectSnapshotChain(on id: VirtualMachine.ID) async throws -> VMSnapshotChain {
    throw TestClientError.fallbackCalled
  }
  fileprivate func createSnapshotDisk(named snapshotName: String, on id: VirtualMachine.ID)
    async throws
    -> VMSnapshotDiskCreation
  { throw TestClientError.fallbackCalled }
  fileprivate func preparePrimaryDisk(on id: VirtualMachine.ID) async throws -> DiskPreparation {
    throw TestClientError.fallbackCalled
  }
  fileprivate func createPrimaryDisk(on id: VirtualMachine.ID) async throws -> VMDiskCreation {
    throw TestClientError.fallbackCalled
  }
  fileprivate func inspectPrimaryDisk(on id: VirtualMachine.ID) async throws -> VMDiskInspection {
    throw TestClientError.fallbackCalled
  }
  fileprivate func verifyActiveDisk(on id: VirtualMachine.ID) async throws -> VMDiskVerification {
    throw TestClientError.fallbackCalled
  }
  fileprivate func compactActiveDisk(on id: VirtualMachine.ID) async throws -> VMDiskCompaction {
    throw TestClientError.fallbackCalled
  }
  fileprivate func repairMetadata(on id: VirtualMachine.ID) async throws -> VMMetadataRepair {
    throw TestClientError.fallbackCalled
  }
  fileprivate func migrateManifest(on id: VirtualMachine.ID, dryRun: Bool) async throws
    -> VMManifestMigration
  {
    throw TestClientError.fallbackCalled
  }
  fileprivate func restoreSnapshot(named snapshotName: String, on id: VirtualMachine.ID)
    async throws
    -> SnapshotRestoreResult
  { throw TestClientError.fallbackCalled }
  fileprivate func executeApplicationConsistentSnapshot(
    named snapshotName: String,
    freezeTimeoutMillis: UInt64?,
    on id: VirtualMachine.ID
  ) async throws -> ApplicationConsistentSnapshotExecution { throw TestClientError.fallbackCalled }
  fileprivate func createDiagnosticBundle(output: String?, on id: VirtualMachine.ID) async throws
    -> DiagnosticBundle
  { throw TestClientError.fallbackCalled }
  fileprivate func createPerformanceBaseline(output: String?, on id: VirtualMachine.ID) async throws
    -> PerformanceBaseline
  { throw TestClientError.fallbackCalled }
  fileprivate func createPerformanceSample(
    output: String?,
    artifactBytes: UInt64,
    iterations: UInt16,
    sync: Bool,
    on id: VirtualMachine.ID
  ) async throws -> PerformanceSample { throw TestClientError.fallbackCalled }
  fileprivate func inspectQMPStatus(on id: VirtualMachine.ID) async throws -> QMPStatus {
    throw TestClientError.fallbackCalled
  }
  fileprivate func viewLogs(kind: VMLogKind, bytes: UInt64?, on id: VirtualMachine.ID) async throws
    -> VMLogView
  { throw TestClientError.fallbackCalled }
  fileprivate func prepareRun(on id: VirtualMachine.ID) async throws -> RunnerStatus {
    throw TestClientError.fallbackCalled
  }
  fileprivate func inspectRunnerStatus(on id: VirtualMachine.ID) async throws -> RunnerStatus? {
    throw TestClientError.fallbackCalled
  }
  fileprivate func exportVirtualMachine(on id: VirtualMachine.ID, output: String) async throws
    -> VMExportMetadata
  { throw TestClientError.fallbackCalled }
  fileprivate func importVirtualMachine(input: String, name: String?) async throws
    -> VMImportMetadata
  {
    throw TestClientError.fallbackCalled
  }
  fileprivate func createVirtualMachine(_ request: CreateVirtualMachineRequest) async throws
    -> VirtualMachine
  {
    throw TestClientError.fallbackCalled
  }
  fileprivate func cloneVirtualMachine(on id: VirtualMachine.ID, newName: String, linked: Bool)
    async throws
    -> CloneVirtualMachineMetadata
  { throw TestClientError.fallbackCalled }
  fileprivate func deleteVirtualMachine(on id: VirtualMachine.ID) async throws -> VMDeletionMetadata
  {
    throw TestClientError.fallbackCalled
  }
  fileprivate func perform(_ action: VirtualMachineAction, on id: VirtualMachine.ID) async throws
    -> VMActionResult
  { throw TestClientError.fallbackCalled }
}

private final class AlwaysFailingVirtualMachineClient: VirtualMachineClient,
  VirtualMachineClientSourceProviding
{
  let sourceTitle = "bridgevmd"

  func listVirtualMachines() async throws -> [VirtualMachine] {
    throw TestClientError.primaryFailed
  }

  func listBootTemplates() async throws -> [BootTemplate] {
    throw TestClientError.primaryFailed
  }

  func inspectBootMediaStatus(on id: VirtualMachine.ID) async throws -> BootMediaStatus {
    throw TestClientError.primaryFailed
  }

  func inspectReadinessReport(on id: VirtualMachine.ID) async throws -> VMReadinessReport {
    throw TestClientError.primaryFailed
  }

  func importBootMedia(
    sourcePath: String,
    kind: BootMediaStatusEntry.Kind?,
    on id: VirtualMachine.ID
  ) async throws -> BootMediaImportMetadata {
    throw TestClientError.primaryFailed
  }

  func verifyBootMedia(
    expectedSHA256: String,
    kind: BootMediaStatusEntry.Kind?,
    on id: VirtualMachine.ID
  ) async throws -> BootMediaVerificationMetadata {
    throw TestClientError.primaryFailed
  }

  func planBootMediaDownload(
    url: String,
    expectedSHA256: String?,
    kind: BootMediaStatusEntry.Kind?,
    on id: VirtualMachine.ID
  ) async throws -> BootMediaDownloadPlanMetadata {
    throw TestClientError.primaryFailed
  }

  func downloadBootMedia(
    kind: BootMediaStatusEntry.Kind?,
    on id: VirtualMachine.ID
  ) async throws -> BootMediaDownloadResultMetadata {
    throw TestClientError.primaryFailed
  }

  func inspectGuestToolsStatus(on id: VirtualMachine.ID) async throws -> GuestToolsStatus {
    throw TestClientError.primaryFailed
  }

  func sendGuestToolsCommand(
    _ command: GuestToolsAgentCommand,
    requestID: String?,
    on id: VirtualMachine.ID
  ) async throws -> GuestToolsCommandDispatch {
    throw TestClientError.primaryFailed
  }

  func inspectSnapshotPreflightStatus(on id: VirtualMachine.ID) async throws
    -> SnapshotPreflightStatus
  {
    throw TestClientError.primaryFailed
  }

  func inspectQMPStatus(on id: VirtualMachine.ID) async throws -> QMPStatus {
    throw TestClientError.primaryFailed
  }

  func viewLogs(kind: VMLogKind, bytes: UInt64?, on id: VirtualMachine.ID) async throws
    -> VMLogView
  {
    throw TestClientError.primaryFailed
  }

  func inspectRunnerStatus(on id: VirtualMachine.ID) async throws -> RunnerStatus? {
    throw TestClientError.primaryFailed
  }

  func executeApplicationConsistentSnapshot(
    named snapshotName: String,
    freezeTimeoutMillis: UInt64?,
    on id: VirtualMachine.ID
  ) async throws -> ApplicationConsistentSnapshotExecution {
    throw TestClientError.primaryFailed
  }

  func createVirtualMachine(_ request: CreateVirtualMachineRequest) async throws -> VirtualMachine {
    throw TestClientError.primaryFailed
  }

  func cloneVirtualMachine(on id: VirtualMachine.ID, newName: String, linked: Bool) async throws
    -> CloneVirtualMachineMetadata
  {
    throw TestClientError.primaryFailed
  }

  func perform(_ action: VirtualMachineAction, on id: VirtualMachine.ID) async throws
    -> VMActionResult
  {
    throw TestClientError.primaryFailed
  }
}

private final class FailingOnceVirtualMachineClient: VirtualMachineClient,
  VirtualMachineClientSourceProviding
{
  let sourceTitle = "bridgevmd"
  private let vmID: VirtualMachine.ID
  private var shouldFailList = true

  init(vmID: VirtualMachine.ID) {
    self.vmID = vmID
  }

  func listVirtualMachines() async throws -> [VirtualMachine] {
    if shouldFailList {
      shouldFailList = false
      throw TestClientError.primaryFailed
    }
    return [
      VirtualMachine(
        id: vmID,
        name: "Primary VM",
        guest: "Ubuntu Arm64",
        status: .stopped,
        mode: .fast,
        resources: .init(cpuCount: 4, memoryGB: 8, diskGB: 64),
        uptime: "Not running",
        ipAddress: nil,
        lastStarted: nil,
        notes: "Primary test VM."
      )
    ]
  }
}

private final class RecordingFallbackClient: VirtualMachineClient,
  VirtualMachineClientSourceProviding
{
  let sourceTitle = "Mock inventory"
  private let vmID: VirtualMachine.ID
  private(set) var calls: [RecordingFallbackCall] = []

  init(vmID: VirtualMachine.ID) {
    self.vmID = vmID
  }

  func listVirtualMachines() async throws -> [VirtualMachine] {
    calls.append(.listVirtualMachines)
    return [testVirtualMachine]
  }

  func listBootTemplates() async throws -> [BootTemplate] {
    throw TestClientError.fallbackCalled
  }

  func inspectBootMediaStatus(on id: VirtualMachine.ID) async throws -> BootMediaStatus {
    throw TestClientError.fallbackCalled
  }

  func inspectReadinessReport(on id: VirtualMachine.ID) async throws -> VMReadinessReport {
    throw TestClientError.fallbackCalled
  }

  func importBootMedia(
    sourcePath: String,
    kind: BootMediaStatusEntry.Kind?,
    on id: VirtualMachine.ID
  ) async throws -> BootMediaImportMetadata {
    throw TestClientError.fallbackCalled
  }

  func verifyBootMedia(
    expectedSHA256: String,
    kind: BootMediaStatusEntry.Kind?,
    on id: VirtualMachine.ID
  ) async throws -> BootMediaVerificationMetadata {
    throw TestClientError.fallbackCalled
  }

  func planBootMediaDownload(
    url: String,
    expectedSHA256: String?,
    kind: BootMediaStatusEntry.Kind?,
    on id: VirtualMachine.ID
  ) async throws -> BootMediaDownloadPlanMetadata {
    throw TestClientError.fallbackCalled
  }

  func downloadBootMedia(
    kind: BootMediaStatusEntry.Kind?,
    on id: VirtualMachine.ID
  ) async throws -> BootMediaDownloadResultMetadata {
    throw TestClientError.fallbackCalled
  }

  func inspectLifecyclePlan(action: LifecyclePlanAction, on id: VirtualMachine.ID) async throws
    -> LifecyclePlan
  {
    calls.append(.inspectLifecyclePlan)
    return LifecyclePlan(
      vm: "Fallback VM",
      action: action,
      currentState: .running,
      targetState: .suspended,
      backend: "mock-qmp",
      metadataOnly: true,
      executable: false,
      qmpCommand: "stop",
      socketPath: "/tmp/fallback.vmbridge/run/qmp.sock",
      socketAvailable: false,
      blockers: ["fallback-inventory"],
      notes: ["fallback lifecycle plan"]
    )
  }

  func addPortForward(host: UInt16, guest: UInt16, on id: VirtualMachine.ID) async throws
    -> VMPortForwardList
  {
    calls.append(.addPortForward)
    return VMPortForwardList(
      vm: testVirtualMachine.name, forwards: [VMPortForward(host: host, guest: guest)])
  }

  func inspectGuestToolsStatus(on id: VirtualMachine.ID) async throws -> GuestToolsStatus {
    throw TestClientError.fallbackCalled
  }

  func sendGuestToolsCommand(
    _ command: GuestToolsAgentCommand,
    requestID: String?,
    on id: VirtualMachine.ID
  ) async throws -> GuestToolsCommandDispatch {
    throw TestClientError.fallbackCalled
  }

  func inspectSnapshotPreflightStatus(on id: VirtualMachine.ID) async throws
    -> SnapshotPreflightStatus
  {
    throw TestClientError.fallbackCalled
  }

  func inspectQMPStatus(on id: VirtualMachine.ID) async throws -> QMPStatus {
    throw TestClientError.fallbackCalled
  }

  func viewLogs(kind: VMLogKind, bytes: UInt64?, on id: VirtualMachine.ID) async throws
    -> VMLogView
  {
    throw TestClientError.fallbackCalled
  }

  func inspectRunnerStatus(on id: VirtualMachine.ID) async throws -> RunnerStatus? {
    throw TestClientError.fallbackCalled
  }

  func executeApplicationConsistentSnapshot(
    named snapshotName: String,
    freezeTimeoutMillis: UInt64?,
    on id: VirtualMachine.ID
  ) async throws -> ApplicationConsistentSnapshotExecution {
    throw TestClientError.fallbackCalled
  }

  func createVirtualMachine(_ request: CreateVirtualMachineRequest) async throws -> VirtualMachine {
    calls.append(.createVirtualMachine)
    return testVirtualMachine
  }

  func cloneVirtualMachine(on id: VirtualMachine.ID, newName: String, linked: Bool) async throws
    -> CloneVirtualMachineMetadata
  {
    throw TestClientError.fallbackCalled
  }

  func perform(_ action: VirtualMachineAction, on id: VirtualMachine.ID) async throws
    -> VMActionResult
  {
    calls.append(.perform)
    return VMActionResult(virtualMachine: testVirtualMachine, message: "fallback mutated")
  }

  private var testVirtualMachine: VirtualMachine {
    VirtualMachine(
      id: vmID,
      name: "Fallback VM",
      guest: "Ubuntu Arm64",
      status: .stopped,
      mode: .fast,
      resources: .init(cpuCount: 4, memoryGB: 8, diskGB: 64),
      uptime: "Not running",
      ipAddress: nil,
      lastStarted: nil,
      notes: "Fallback test VM."
    )
  }
}

private final class RecordingDaemonTransport: DaemonTransport {
  private(set) var requests: [[String: Any]] = []
  private var isRunning = false
  private var sharedFolders: [[String: Any]] = []

  func send<Request: Encodable, Response: Decodable>(
    _ request: Request,
    responseType: Response.Type
  ) async throws -> Response {
    let data = try JSONEncoder().encode(request)
    let object = try XCTUnwrap(JSONSerialization.jsonObject(with: data) as? [String: Any])
    requests.append(object)

    let responseJSON: String
    switch object["type"] as? String {
    case "doctor":
      responseJSON = """
        {
          "type": "doctor",
          "store_root": "/tmp/bridgevm",
          "vms_dir": "/tmp/bridgevm/vms",
          "status": "OK"
        }
        """
    case "list_templates":
      responseJSON = """
        {
          "type": "boot_templates",
          "templates": [
            {
              "id": "ubuntu-arm64-installer",
              "guest_os": "ubuntu",
              "guest_arch": "arm64",
              "mode": "linux-installer",
              "media_label": "ubuntu arm64 installer image",
              "source": "manual",
              "installer_image": "installers/ubuntu-arm64.iso",
              "note": "Place the installer image inside the bundle."
            }
          ]
        }
        """
    case "create_vm":
      let manifest = try XCTUnwrap(object["manifest"] as? [String: Any])
      let guest = try XCTUnwrap(manifest["guest"] as? [String: Any])
      responseJSON = """
        {
          "type": "vm",
          "vm": {
            "name": "\(manifest["name"] as? String ?? "Created VM")",
            "mode": "\(manifest["mode"] as? String ?? "fast")",
            "guest_os": "\(guest["os"] as? String ?? "ubuntu")",
            "guest_arch": "\(guest["arch"] as? String ?? "arm64")",
            "state": "stopped",
            "path": "/tmp/created-vm.vmbridge"
          }
        }
        """
    case "clone_vm":
      let name = object["name"] as? String ?? "dev"
      let newName = object["new_name"] as? String ?? "dev-copy"
      if object["linked"] as? Bool == true {
        responseJSON = """
          {
            "type": "cloned",
            "clone": {
              "vm": "\(newName)",
              "source": "/tmp/\(name).vmbridge",
              "output": "/tmp/\(newName).vmbridge",
              "linked": true,
              "backing_path": "/tmp/\(name).vmbridge/disks/\(name).qcow2",
              "backing_format": "qcow2",
              "create_command": [
                "qemu-img",
                "create",
                "-f",
                "qcow2",
                "-F",
                "qcow2",
                "-b",
                "/tmp/\(name).vmbridge/disks/\(name).qcow2",
                "/tmp/\(newName).vmbridge/disks/\(newName).qcow2"
              ],
              "cloned_at_unix": 1710000701
            }
          }
          """
      } else {
        responseJSON = """
          {
            "type": "cloned",
            "clone": {
              "vm": "\(newName)",
              "source": "/tmp/\(name).vmbridge",
              "output": "/tmp/\(newName).vmbridge",
              "linked": false,
              "cloned_at_unix": 1710000700
            }
          }
          """
      }
    case "list_vms":
      responseJSON = """
        {
          "type": "vm_list",
          "vms": [
            {
              "name": "dev",
              "mode": "fast",
              "guest_os": "ubuntu",
              "guest_arch": "arm64",
              "state": "\(isRunning ? "running" : "stopped")",
              "path": "/tmp/dev.vmbridge"
            }
          ]
        }
        """
    case "inspect_boot_media_status":
      responseJSON = """
        {
          "type": "boot_media_status",
          "status": {
            "vm": "\(object["name"] as? String ?? "dev")",
            "entries": [
              {
                "kind": "installer-image",
                "path": "installers/ubuntu-arm64.iso",
                "exists": true,
                "bytes": 14,
                "last_import": null,
                "last_verification": null,
                "last_download_plan": null,
                "last_download": null
              }
            ]
          }
        }
        """
    case "guest_tools_status":
      responseJSON = """
        {
          "type": "guest_tools_status",
          "status": {
            "vm": "\(object["name"] as? String ?? "dev")",
            "tools": "required",
            "token_created_at_unix": 1710000000,
            "capabilities": [
              {
                "name": "heartbeat",
                "max_version": 1,
                "enabled_by": "base"
              }
            ],
            "approved_shared_folders": [
              {
                "name": "workspace",
                "host_path": "/Users/dev/workspace",
                "host_path_token": "host-token-1",
                "read_only": false,
                "approval": "required"
              }
            ],
            "runtime": {
              "connected": true,
              "guest_os": "ubuntu",
              "agent_version": "0.1.0",
              "capabilities": ["heartbeat"],
              "last_heartbeat_at_unix": 1710000060,
              "guest_ip_addresses": [
                {
                  "address": "192.168.64.23",
                  "interface": "en0"
                }
              ],
              "shared_folders": [
                {
                  "name": "workspace",
                  "host_path_token": "host-token-1",
                  "mounted_at_unix": 1710000062
                }
              ],
              "metrics": {
                "cpu_percent": 17,
                "memory_used_mib": 512,
                "updated_at_unix": 1710000061
              },
              "updated_at_unix": 1710000061
            }
          }
        }
        """
    case "guest_tools_token":
      responseJSON = """
        {
          "type": "guest_tools_token",
          "token": {
            "vm": "\(object["name"] as? String ?? "dev")",
            "token": "secret-token-value",
            "created_at_unix": 1710000000
          }
        }
        """
    case "guest_tools_linux_command":
      responseJSON = """
        {
          "type": "guest_tools_linux_command",
          "command": {
            "vm": "\(object["name"] as? String ?? "dev")",
            "transport": "\(object["transport"] as? String ?? "device")",
            "command": ["bridgevm-guest-tools", "run", "--transport", "\(object["transport"] as? String ?? "device")"],
            "token_file": "/run/bridgevm-token.json",
            "capabilities": ["heartbeat", "time-sync"]
          }
        }
        """
    case "runtime_control":
      responseJSON = """
        {
          "type": "runtime_control",
          "control": {
            "vm": "\(object["name"] as? String ?? "dev")",
            "kind": "apple-vz-display",
            "socket_path": "/tmp/bvm-vz-test.sock",
            "command": "\(object["command"] as? String ?? "status")",
            "response": {
              "ok": true,
              "state": "running"
            }
          }
        }
        """
    case "guest_tools_mount_approved_share":
      responseJSON = """
        {
          "type": "guest_tools_command",
          "command": {
            "vm": "\(object["name"] as? String ?? "dev")",
            "request_id": null,
            "pending_commands": 1
          }
        }
        """
    case "guest_tools_send_command":
      let envelope = try XCTUnwrap(object["envelope"] as? [String: Any])
      responseJSON = """
        {
          "type": "guest_tools_command",
          "command": {
            "vm": "\(object["name"] as? String ?? "dev")",
            "request_id": \(jsonStringOrNull(envelope["request_id"] as? String)),
            "pending_commands": 1
          }
        }
        """
    case "runner_status", "prepare_run", "run_backend":
      if object["type"] as? String == "run_backend" {
        isRunning = object["spawn"] as? Bool == true
      }
      responseJSON = """
        {
          "type": "runner_status",
          "metadata": {
            "engine": "lightvm",
            "pid": 4242,
            "command": ["lightvm-runner", "\(object["name"] as? String ?? "dev")", "--apple-vz"],
            "log_path": "logs/lightvm.log",
            "started_at_unix": 1710000100,
            "dry_run": false,
            "launch_spec_path": ".vmbridge/metadata/apple-vz-launch.json",
            "guest_tools": {
              "transport": "virtio-serial",
              "channel_name": "org.bridgevm.guest-tools.0",
              "socket_path": "metadata/guest-tools.sock",
              "token_path": "metadata/guest-tools-token.json",
              "token_created_at_unix": 1710000050
            },
            "runtime_control": {
              "kind": "apple-vz-display",
              "socket_path": "run/apple-vz-display-control.sock",
              "commands": ["status", "stop", "policy", "pacing"]
            },
            "launch_readiness": {
              "ready": false,
              "blockers": [
                {
                  "code": "missing-primary-disk",
                  "message": "Primary disk is missing.",
                  "path": "disks/root.qcow2",
                  "capability": "apple-vz"
                }
              ]
            }
          }
        }
        """
    case "readiness_report":
      responseJSON = """
        {
          "type": "readiness_report",
          "report": {
            "vm": "\(object["name"] as? String ?? "dev")",
            "mode": "fast",
            "state": "stopped",
            "metadata_only": true,
            "live_e2e_required": true,
            "boot_media": {
              "vm": "\(object["name"] as? String ?? "dev")",
              "entries": [
                {
                  "kind": "installer-image",
                  "path": "installers/ubuntu-arm64.iso",
                  "exists": false,
                  "size_bytes": null,
                  "last_import": null,
                  "last_verification": null,
                  "last_download_plan": null,
                  "last_download": null
                }
              ]
            },
            "boot_media_error": null,
            "snapshot_chain": {
              "active_disk": {
                "source": "primary",
                "snapshot": null,
                "path": "disks/root.qcow2",
                "format": "qcow2",
                "exists": false,
                "activated_at_unix": 1710000000
              },
              "disks": []
            },
            "snapshot_chain_error": null,
            "runner": null,
            "runner_error": "not prepared",
            "blockers": ["boot-media-missing:installers/ubuntu-arm64.iso"],
            "notes": ["metadata-only report"]
          }
        }
        """
    case "qemu_args":
      responseJSON = """
        {
          "type": "qemu_command",
          "command": {
            "program": "qemu-system-aarch64",
            "args": [
              "-name",
              "\(object["name"] as? String ?? "dev")",
              "-netdev",
              "vmnet-host,id=net0",
              "-device",
              "virtio-net-pci,netdev=net0"
            ]
          }
        }
        """
    case "lifecycle_plan":
      responseJSON = """
        {
          "type": "lifecycle_plan",
          "plan": {
            "vm": "\(object["name"] as? String ?? "dev")",
            "action": "\(object["action"] as? String ?? "suspend")",
            "current_state": "running",
            "target_state": "\((object["action"] as? String) == "resume" ? "running" : "suspended")",
            "backend": "qemu-qmp",
            "metadata_only": true,
            "executable": true,
            "qmp_command": "\((object["action"] as? String) == "resume" ? "cont" : "stop")",
            "socket_path": "/tmp/\(object["name"] as? String ?? "dev").vmbridge/run/qmp.sock",
            "socket_available": true,
            "blockers": [],
            "notes": [
              "metadata-only lifecycle plan; no backend command was sent",
              "Compatibility Mode lifecycle control maps to QMP stop/cont"
            ]
          }
        }
        """
    case "open_port":
      let scheme = object["scheme"] as? String ?? "http"
      let guest = object["guest"] as? Int ?? 80
      let hostPort = guest == 80 ? 18080 : guest + 10_000
      let url = "\(scheme)://127.0.0.1:\(hostPort)"
      responseJSON = """
        {
          "type": "open_port_plan",
          "plan": {
            "vm": "\(object["name"] as? String ?? "dev")",
            "scheme": "\(scheme)",
            "host": "127.0.0.1",
            "guest_port": \(guest),
            "host_port": \(hostPort),
            "url": "\(url)",
            "command": ["open", "\(url)"]
          }
        }
        """
    case "plan_network":
      responseJSON = """
        {
          "type": "network_planned",
          "plan": {
            "vm": "\(object["name"] as? String ?? "dev")",
            "backend": "apple-vz",
            "mode": "nat",
            "hostname": "\(object["name"] as? String ?? "dev")",
            "dry_run": true,
            "executable": true,
            "port_forwards": [
              {
                "host": 2222,
                "guest": 22
              }
            ],
            "capabilities": {
              "guest_outbound": true,
              "host_to_guest": true,
              "guest_to_host": true,
              "host_visible_hostname": true,
              "supports_port_forwarding": true,
              "requires_privileged_helper": false
            },
            "blockers": [],
            "notes": [
              "dry-run network plan; no backend launch or host networking mutation was performed"
            ]
          }
        }
        """
    case "ssh_plan":
      let user = object["user"] as? String ?? "ubuntu"
      responseJSON = """
        {
          "type": "ssh_plan",
          "plan": {
            "vm": "\(object["name"] as? String ?? "dev")",
            "user": "\(user)",
            "host": "127.0.0.1",
            "port": 2222,
            "source": "port-forward",
            "command": ["ssh", "-p", "2222", "\(user)@127.0.0.1"]
          }
        }
        """
    case "list_ports":
      responseJSON = """
        {
          "type": "port_forwards",
          "ports": {
            "vm": "\(object["name"] as? String ?? "dev")",
            "forwards": []
          }
        }
        """
    case "add_port":
      responseJSON = """
        {
          "type": "port_forwards",
          "ports": {
            "vm": "\(object["name"] as? String ?? "dev")",
            "forwards": [
              {
                "host": \(object["host"] as? Int ?? 2222),
                "guest": \(object["guest"] as? Int ?? 22)
              }
            ]
          }
        }
        """
    case "remove_port":
      responseJSON = """
        {
          "type": "port_forwards",
          "ports": {
            "vm": "\(object["name"] as? String ?? "dev")",
            "forwards": []
          }
        }
        """
    case "snapshot_preflight_status":
      responseJSON = """
        {
          "type": "snapshot_preflight_status",
          "preflight": {
            "vm": "\(object["name"] as? String ?? "dev")",
            "consistency": "\(object["consistency"] as? String ?? "application-consistent")",
            "backend_freeze_thaw_supported": false,
            "guest_tools_connected": true,
            "capabilities": ["guest-tools-heartbeat", "filesystem-freeze-preflight"],
            "ready": false,
            "blockers": [
              {
                "code": "backend-freeze-thaw-unavailable",
                "message": "Freeze/thaw orchestration requires the bridgevmd-owned running backend; this offline preflight cannot drive the guest agent.",
                "path": null
              }
            ],
            "checked_at_unix": 1710000200
          }
        }
        """
    case "list_snapshots":
      responseJSON = """
        {
          "type": "snapshot_list",
          "snapshots": [
            {
              "name": "before-upgrade",
              "kind": "disk",
              "created_at_unix": 1710000300,
              "vm_state": "stopped"
            },
            {
              "name": "paused-state",
              "kind": "suspend",
              "created_at_unix": 1710000400,
              "vm_state": "suspended"
            }
          ]
        }
        """
    case "snapshot_chain":
      responseJSON = """
        {
          "type": "snapshot_chain",
          "chain": {
            "active_disk": {
              "source": "snapshot-overlay",
              "snapshot": "before-upgrade",
              "path": "disks/snapshots/before-upgrade.qcow2",
              "format": "qcow2",
              "exists": true,
              "activated_at_unix": 1710000360
            },
            "disks": [
              {
                "snapshot": "before-upgrade",
                "overlay_path": "disks/snapshots/before-upgrade.qcow2",
                "overlay_format": "qcow2",
                "overlay_exists": true,
                "backing_path": "disks/root.qcow2",
                "backing_format": "qcow2",
                "backing_exists": true,
                "create_command": [
                  "qemu-img",
                  "create",
                  "-f",
                  "qcow2",
                  "-F",
                  "qcow2",
                  "-b",
                  "disks/root.qcow2",
                  "disks/snapshots/before-upgrade.qcow2"
                ],
                "prepared_at_unix": 1710000300
              }
            ]
          }
        }
        """
    case "create_snapshot_disk":
      let snapshot = object["name"] as? String ?? "before-upgrade"
      responseJSON = """
        {
          "type": "snapshot_disk_created",
          "metadata": {
            "snapshot": "\(snapshot)",
            "disk": {
              "snapshot": "\(snapshot)",
              "overlay_path": "disks/snapshots/\(snapshot).qcow2",
              "overlay_format": "qcow2",
              "overlay_exists": true,
              "backing_path": "disks/root.qcow2",
              "backing_format": "qcow2",
              "backing_exists": true,
              "create_command": [
                "qemu-img",
                "create",
                "-f",
                "qcow2",
                "-F",
                "qcow2",
                "-b",
                "disks/root.qcow2",
                "disks/snapshots/\(snapshot).qcow2"
              ],
              "prepared_at_unix": 1710000300
            },
            "command": [
              "qemu-img",
              "create",
              "-f",
              "qcow2",
              "-F",
              "qcow2",
              "-b",
              "disks/root.qcow2",
              "disks/snapshots/\(snapshot).qcow2"
            ],
            "executed": true,
            "exit_status": "exit status: 0",
            "stdout": "created overlay\\n",
            "stderr": "",
            "created_at_unix": 1710000360
          }
        }
        """
    case "create_snapshot":
      responseJSON = """
        {
          "type": "snapshot",
          "snapshot": {
            "name": "\(object["name"] as? String ?? "before-upgrade")",
            "kind": "\(object["kind"] as? String ?? "disk")",
            "created_at_unix": 1710000360,
            "vm_state": "running"
          }
        }
        """
    case "prepare_disk":
      responseJSON = """
        {
          "type": "disk_prepared",
          "metadata": {
            "path": "disks/root.qcow2",
            "format": "qcow2",
            "size": "80G",
            "size_bytes": 85899345920,
            "exists": false,
            "created": true,
            "create_command": ["qemu-img", "create", "-f", "qcow2", "disks/root.qcow2", "80G"],
            "prepared_at_unix": 1710000300
          }
        }
        """
    case "create_disk":
      responseJSON = """
        {
          "type": "disk_created",
          "metadata": {
            "preparation": {
              "path": "disks/root.qcow2",
              "format": "qcow2",
              "size": "80G",
              "size_bytes": 85899345920,
              "exists": false,
              "created": true,
              "create_command": ["qemu-img", "create", "-f", "qcow2", "disks/root.qcow2", "80G"],
              "prepared_at_unix": 1710000300
            },
            "command": ["qemu-img", "create", "-f", "qcow2", "disks/root.qcow2", "80G"],
            "executed": true,
            "exit_status": "exit status: 0",
            "stdout": "Formatting 'disks/root.qcow2'\\n",
            "stderr": "",
            "created_at_unix": 1710000400
          }
        }
        """
    case "inspect_disk":
      responseJSON = """
        {
          "type": "disk_inspected",
          "metadata": {
            "preparation": {
              "path": "disks/root.qcow2",
              "format": "qcow2",
              "size": "80G",
              "size_bytes": 85899345920,
              "exists": true,
              "created": false,
              "create_command": ["qemu-img", "create", "-f", "qcow2", "disks/root.qcow2", "80G"],
              "prepared_at_unix": 1710000300
            },
            "command": ["qemu-img", "info", "--output=json", "disks/root.qcow2"],
            "exit_status": "exit status: 0",
            "info": {
              "filename": "disks/root.qcow2",
              "format": "qcow2",
              "virtual-size": 85899345920
            },
            "stdout": "{\\"filename\\":\\"disks/root.qcow2\\",\\"format\\":\\"qcow2\\"}",
            "stderr": "",
            "inspect_duration_microseconds": 3456,
            "inspected_at_unix": 1710000500
          }
        }
        """
    case "verify_disk":
      responseJSON = """
        {
          "type": "disk_verified",
          "metadata": {
            "active_disk": {
              "source": "primary",
              "snapshot": null,
              "path": "disks/root.qcow2",
              "format": "qcow2",
              "exists": true,
              "activated_at_unix": 1710000300
            },
            "command": ["qemu-img", "check", "--output=json", "disks/root.qcow2"],
            "exit_status": "exit status: 0",
            "report": {
              "check-errors": 0,
              "image-end-offset": 4096
            },
            "stdout": "{\\"check-errors\\":0,\\"image-end-offset\\":4096}",
            "stderr": "",
            "verify_duration_microseconds": 1234,
            "verified_at_unix": 1710000800
          }
        }
        """
    case "compact_disk":
      responseJSON = """
        {
          "type": "disk_compacted",
          "metadata": {
            "preparation": {
              "path": "disks/root.qcow2",
              "format": "qcow2",
              "size": "80G",
              "size_bytes": 85899345920,
              "exists": true,
              "created": false,
              "create_command": ["qemu-img", "create", "-f", "qcow2", "disks/root.qcow2", "80G"],
              "prepared_at_unix": 1710000300
            },
            "active_disk": {
              "source": "primary",
              "snapshot": null,
              "path": "disks/root.qcow2",
              "format": "qcow2",
              "exists": true,
              "activated_at_unix": 1710000300
            },
            "command": [
              "qemu-img",
              "convert",
              "-O",
              "qcow2",
              "-c",
              "disks/root.qcow2",
              "disks/root.qcow2.compact.tmp"
            ],
            "temp_path": "disks/root.qcow2.compact.tmp",
            "backup_path": "disks/root.qcow2.precompact-1710000900",
            "exit_status": "exit status: 0",
            "stdout": "compacted\\n",
            "stderr": "",
            "original_size_bytes": 8192,
            "compacted_size_bytes": 4096,
            "compact_duration_microseconds": 2345,
            "compacted_at_unix": 1710000900
          }
        }
        """
    case "repair_metadata":
      responseJSON = """
        {
          "type": "metadata_repaired",
          "repair": {
            "vm": "\(object["name"] as? String ?? "dev")",
            "bundle": "/tmp/\(object["name"] as? String ?? "dev").vmbridge",
            "repaired": true,
            "actions": [
              {
                "action": "repaired",
                "path": "/tmp/\(object["name"] as? String ?? "dev").vmbridge/metadata/runtime.json",
                "detail": "wrote runtime metadata"
              }
            ],
            "repaired_at_unix": 42
          }
        }
        """
    case "migrate_manifest":
      let dryRun = object["dry_run"] as? Bool ?? false
      responseJSON = """
        {
          "type": "manifest_migrated",
          "migration": {
            "vm": "\(object["name"] as? String ?? "dev")",
            "bundle": "/tmp/\(object["name"] as? String ?? "dev").vmbridge",
            "manifest_path": "/tmp/\(object["name"] as? String ?? "dev").vmbridge/manifest.yaml",
            "dry_run": \(dryRun ? "true" : "false"),
            "migrated": false,
            "from_schema": "bridgevm.io/v1",
            "to_schema": "bridgevm.io/v1",
            "actions": \(dryRun ? "[\"validated current manifest schema\", \"dry-run did not write migration receipt or manifest backup\"]" : "[\"validated current manifest schema\", \"copied manifest before migration\", \"wrote migration receipt\"]"),
            "backup_path": \(dryRun ? "null" : "\"/tmp/\(object["name"] as? String ?? "dev").vmbridge/metadata/manifest-before-migration.yaml\""),
            "receipt_path": \(dryRun ? "null" : "\"/tmp/\(object["name"] as? String ?? "dev").vmbridge/metadata/manifest-migration.json\""),
            "migrated_at_unix": 1710001300
          }
        }
        """
    case "restore_snapshot":
      responseJSON = """
        {
          "type": "snapshot_restored",
          "restore": {
            "snapshot": "\(object["name"] as? String ?? "paused-state")",
            "restored_at_unix": 1710000500,
            "restored_state": "suspended",
            "active_disk": {
              "source": "snapshot-backing",
              "snapshot": "\(object["name"] as? String ?? "paused-state")",
              "path": "disks/root.qcow2",
              "format": "qcow2",
              "exists": true,
              "activated_at_unix": 1710000500
            },
            "suspend_image": {
              "snapshot": "\(object["name"] as? String ?? "paused-state")",
              "image_path": "snapshots/\(object["name"] as? String ?? "paused-state")/suspend.img",
              "image_format": "vz",
              "image_exists": true,
              "prepared_at_unix": 1710000400
            }
          }
        }
        """
    case "execute_application_consistent_snapshot":
      responseJSON = """
        {
          "type": "application_consistent_snapshot_execution",
          "execution": {
            "vm": "\(object["vm"] as? String ?? "dev")",
            "snapshot": "\(object["name"] as? String ?? "before-upgrade")",
            "freeze_request_id": "freeze-1",
            "thaw_request_id": "thaw-1",
            "pending_commands_after_freeze": 1,
            "pending_commands_after_thaw": 2,
            "snapshot_created_at_unix": 1710000300,
            "freeze_result": {
              "request_id": "freeze-1",
              "capability": "fs-freeze",
              "ok": true,
              "error_code": null,
              "message": "freeze scaffold acknowledged",
              "completed_at_unix": 1710000280
            },
            "thaw_result": {
              "request_id": "thaw-1",
              "capability": "fs-thaw",
              "ok": true,
              "error_code": null,
              "message": "thaw scaffold acknowledged",
              "completed_at_unix": 1710000290
            },
            "preflight_ready": true,
            "note": "scaffold boundary"
          }
        }
        """
    case "reapply_runtime_resources":
      responseJSON = """
        {
          "type": "runtime_resource_policy",
          "policy": {
            "vm": "\(object["name"] as? String ?? "dev")",
            "mode": "fast",
            "profile": "automatic",
            "visibility": "\(object["visibility"] as? String ?? "background")",
            "state": "running",
            "on_battery": false,
            "memory": "2048",
            "cpu": "1",
            "display_fps_cap": "10",
            "rationale": "Battery or background throttling active.",
            "live_applied": false,
            "runtime_control_acknowledged": true,
            "live_apply_blockers": [
              {
                "code": "runtime-control-unavailable",
                "message": "Live Apple VZ CPU/RAM hot-apply is not implemented yet; the policy is available to display pacing and runtime policy IPC consumers."
              }
            ],
            "updated_at_unix": 1710000500
          }
        }
        """
    case "create_diagnostic_bundle":
      responseJSON = """
        {
          "type": "diagnostic_bundle",
          "bundle": {
            "vm": "\(object["name"] as? String ?? "dev")",
            "source": "/tmp/\(object["name"] as? String ?? "dev").vmbridge",
            "output": "\(object["output"] as? String ?? "/tmp/diagnostics")/bridgevm-diagnostics-\(object["name"] as? String ?? "dev")-1710000600",
            "files": ["manifest.yaml", "metadata/state.json", "diagnostic-bundle.json"],
            "created_at_unix": 1710000600
          }
        }
        """
    case "export_vm":
      responseJSON = """
        {
          "type": "exported",
          "export": {
            "vm": "\(object["name"] as? String ?? "dev")",
            "source": "/tmp/\(object["name"] as? String ?? "dev").vmbridge",
            "output": "\(object["output"] as? String ?? "/tmp/dev-export.vmbridge")",
            "archive_format": "directory",
            "copied_file_count": 3,
            "copied_files": ["manifest.yaml", "metadata/state.json", "metadata/runtime.json"],
            "manifest_preserved": true,
            "metadata_preserved": true,
            "exported_at_unix": 1710000110
          }
        }
        """
    case "import_vm":
      let importName = object["name"] as? String ?? "dev-imported"
      responseJSON = """
        {
          "type": "imported",
          "import": {
            "vm": "\(importName)",
            "source": "\(object["input"] as? String ?? "/tmp/dev-export.vmbridge")",
            "output": "/tmp/\(importName).vmbridge",
            "archive_format": "directory",
            "copied_file_count": 3,
            "copied_files": ["manifest.yaml", "metadata/state.json", "metadata/runtime.json"],
            "manifest_preserved": true,
            "metadata_preserved": true,
            "original_name": "dev",
            "requested_name": "\(importName)",
            "manifest_identity_rewritten": true,
            "imported_at_unix": 1710000120
          }
        }
        """
    case "create_performance_baseline":
      responseJSON = performanceBaselineJSON(type: "performance_baseline")
    case "create_performance_sample":
      responseJSON = """
        {
          "type": "performance_sample",
          "sample": {
            "vm": "\(object["name"] as? String ?? "dev")",
            "source": "/tmp/\(object["name"] as? String ?? "dev").vmbridge",
            "output": "\(object["output"] as? String ?? "/tmp/performance")/bridgevm-performance-sample-\(object["name"] as? String ?? "dev")-1710000700",
            "artifact": "\(object["output"] as? String ?? "/tmp/performance")/bridgevm-performance-sample-\(object["name"] as? String ?? "dev")-1710000700/performance-sample.json",
            "probe": "\(object["output"] as? String ?? "/tmp/performance")/bridgevm-performance-sample-\(object["name"] as? String ?? "dev")-1710000700/write-probe.bin",
            "probes": ["\(object["output"] as? String ?? "/tmp/performance")/bridgevm-performance-sample-\(object["name"] as? String ?? "dev")-1710000700/write-probe.bin"],
            "artifact_bytes": \(object["artifact_bytes"] as? Int ?? 4096),
            "iterations": \(object["iterations"] as? Int ?? 1),
            "sync": \(object["sync"] as? Bool == true ? "true" : "false"),
            "iteration_results": [
              {
                "iteration": 1,
                "probe": "\(object["output"] as? String ?? "/tmp/performance")/bridgevm-performance-sample-\(object["name"] as? String ?? "dev")-1710000700/write-probe.bin",
                "bytes": \(object["artifact_bytes"] as? Int ?? 4096),
                "write_latency_microseconds": 80,
                "sync": \(object["sync"] as? Bool == true ? "true" : "false")
              }
            ],
            \(performanceBaselineFieldsJSON(createdAtUnix: 1_710_000_700))
          }
        }
        """
    case "qmp_status":
      responseJSON = """
        {
          "type": "qmp_status",
          "status": {
            "socket_path": "/tmp/\(object["name"] as? String ?? "dev").vmbridge/run/qmp.sock",
            "available": true,
            "status": "running",
            "running": true
          }
        }
        """
    case "view_logs":
      responseJSON = """
        {
          "type": "logs_viewed",
          "log": {
            "vm": "\(object["name"] as? String ?? "dev")",
            "kind": "\(object["kind"] as? String ?? "qemu")",
            "path": "/tmp/\(object["name"] as? String ?? "dev").vmbridge/logs/\((object["kind"] as? String) == "serial" ? "serial.log" : "qemu.log")",
            "exists": true,
            "bytes": 128,
            "returned_bytes": 32,
            "truncated": true,
            "content": "log tail"
          }
        }
        """
    case "list_shares":
      responseJSON = sharedFoldersResponseJSON(vm: object["name"] as? String ?? "dev")
    case "add_share":
      let share = object["share"] as? String ?? "workspace"
      sharedFolders.removeAll { $0["name"] as? String == share }
      sharedFolders.append([
        "name": share,
        "host_path": object["host_path"] as? String ?? "/Users/dev/workspace",
        "read_only": object["read_only"] as? Bool ?? false,
        "host_path_token": object["host_path_token"] as? String ?? "share-token-workspace",
      ])
      responseJSON = sharedFoldersResponseJSON(vm: object["name"] as? String ?? "dev")
    case "remove_share":
      let share = object["share"] as? String ?? "workspace"
      sharedFolders.removeAll { $0["name"] as? String == share }
      responseJSON = sharedFoldersResponseJSON(vm: object["name"] as? String ?? "dev")
    case "import_boot_media":
      responseJSON = """
        {
          "type": "boot_media_imported",
          "import": {
            "vm": "\(object["name"] as? String ?? "dev")",
            "kind": "\(object["kind"] as? String ?? "installer-image")",
            "source": "\(object["source"] as? String ?? "/tmp/dev.iso")",
            "destination": "installers/ubuntu-arm64.iso",
            "bytes": 14,
            "replaced": false,
            "imported_at_unix": 1710000040
          }
        }
        """
    case "verify_boot_media":
      responseJSON = """
        {
          "type": "boot_media_verified",
          "verification": {
            "vm": "\(object["name"] as? String ?? "dev")",
            "kind": "\(object["kind"] as? String ?? "installer-image")",
            "path": "installers/ubuntu-arm64.iso",
            "bytes": 14,
            "expected_sha256": "\(object["expected_sha256"] as? String ?? "abc")",
            "actual_sha256": "\(object["expected_sha256"] as? String ?? "abc")",
            "verified": true,
            "verified_at_unix": 1710000050
          }
        }
        """
    case "plan_boot_media_download":
      responseJSON = """
        {
          "type": "boot_media_download_planned",
          "plan": {
            "vm": "\(object["name"] as? String ?? "dev")",
            "kind": "\(object["kind"] as? String ?? "installer-image")",
            "url": "\(object["url"] as? String ?? "https://example.invalid/dev.iso")",
            "destination": "installers/ubuntu-arm64.iso",
            "exists": true,
            "bytes": 14,
            "expected_sha256": "\(object["expected_sha256"] as? String ?? "abc")",
            "last_import": null,
            "last_verification": null,
            "planned_at_unix": 1710000060
          }
        }
        """
    case "download_boot_media":
      responseJSON = """
        {
          "type": "boot_media_downloaded",
          "download": {
            "vm": "\(object["name"] as? String ?? "dev")",
            "kind": "\(object["kind"] as? String ?? "installer-image")",
            "url": "https://example.invalid/dev.iso",
            "destination": "installers/ubuntu-arm64.iso",
            "temp_path": "installers/.ubuntu-arm64.iso.download",
            "command": ["curl", "--location"],
            "exit_status": 0,
            "stdout": "",
            "stderr": "",
            "bytes": 14,
            "replaced": false,
            "expected_sha256": "abc",
            "actual_sha256": "abc",
            "verified": true,
            "downloaded": true,
            "downloaded_at_unix": 1710000070
          }
        }
        """
    case "stop_backend":
      isRunning = false
      responseJSON = #"{"type":"runner_status","metadata":null}"#
    default:
      XCTFail("unexpected daemon request: \(object)")
      responseJSON = #"{"type":"error","message":"unexpected request"}"#
    }

    return try JSONDecoder().decode(Response.self, from: Data(responseJSON.utf8))
  }

  private func sharedFoldersResponseJSON(vm: String) -> String {
    let folders = sharedFolders.map { folder in
      """
      {
        "name": "\(folder["name"] as? String ?? "workspace")",
        "host_path": "\(folder["host_path"] as? String ?? "/Users/dev/workspace")",
        "read_only": \((folder["read_only"] as? Bool) == true ? "true" : "false"),
        "host_path_token": "\(folder["host_path_token"] as? String ?? "share-token-workspace")"
      }
      """
    }.joined(separator: ",")

    return """
      {
        "type": "shared_folders",
        "shares": {
          "vm": "\(vm)",
          "shared_folders": [\(folders)]
        }
      }
      """
  }
}
