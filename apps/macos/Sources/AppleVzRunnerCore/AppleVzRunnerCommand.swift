import Foundation

enum AppleVzDisplayLimits {
  static let maximumPixelCount = 32 * 1024 * 1024

  static func supports(width: Int, height: Int) -> Bool {
    width > 0 && height > 0 && width <= maximumPixelCount / height
  }
}

public enum AppleVzRunnerCommand {
  public struct Dependencies {
    public var readStandardInput: () throws -> Data
    public var readFile: (String) throws -> Data
    public var validateVzConfiguration: (AppleVzLaunchSpec) throws -> Void
    public var launchVirtualMachine: (AppleVzLaunchSpec, AppleVzLaunchOptions) throws -> Void
    public var writeOutput: (String) -> Void
    public var writeError: (String) -> Void

    public init(
      readStandardInput: @escaping () throws -> Data,
      readFile: @escaping (String) throws -> Data,
      validateVzConfiguration: @escaping (AppleVzLaunchSpec) throws -> Void,
      launchVirtualMachine: @escaping (AppleVzLaunchSpec, AppleVzLaunchOptions) throws -> Void,
      writeOutput: @escaping (String) -> Void,
      writeError: @escaping (String) -> Void
    ) {
      self.readStandardInput = readStandardInput
      self.readFile = readFile
      self.validateVzConfiguration = validateVzConfiguration
      self.launchVirtualMachine = launchVirtualMachine
      self.writeOutput = writeOutput
      self.writeError = writeError
    }

    public static var live: Dependencies {
      Dependencies(
        readStandardInput: {
          FileHandle.standardInput.readDataToEndOfFile()
        },
        readFile: { path in
          try Data(contentsOf: URL(fileURLWithPath: path))
        },
        validateVzConfiguration: { spec in
          #if canImport(Virtualization)
          try AppleVzConfigurationBuilder.validateLinuxKernelConfiguration(spec: spec)
          #else
          throw AppleVzRunnerCommandError.virtualizationFrameworkUnavailable
          #endif
        },
        launchVirtualMachine: { spec, launchOptions in
          #if canImport(Virtualization)
          if let restorePath = launchOptions.restoreStatePath {
            guard #available(macOS 14.0, *) else {
              throw AppleVzRunnerCommandError.saveRestoreRequiresMacOS14
            }
            try AppleVzVirtualMachineLauncher.restoreLinuxKernelVirtualMachine(
              spec: spec,
              fromStatePath: restorePath,
              options: launchOptions
            )
          } else if let savePath = launchOptions.saveStatePath {
            guard #available(macOS 14.0, *) else {
              throw AppleVzRunnerCommandError.saveRestoreRequiresMacOS14
            }
            try AppleVzVirtualMachineLauncher.suspendLinuxKernelVirtualMachine(
              spec: spec,
              afterSeconds: launchOptions.stopAfterSeconds ?? 30,
              toStatePath: savePath
            )
          } else if launchOptions.displayWindow {
            guard #available(macOS 14.0, *) else {
              throw AppleVzRunnerCommandError.displayRequiresMacOS14
            }
            try AppleVzVirtualMachineLauncher.launchLinuxKernelVirtualMachineWithDisplay(
              spec: spec,
              options: launchOptions
            )
          } else {
            try AppleVzVirtualMachineLauncher.launchLinuxKernelVirtualMachine(
              spec: spec,
              options: launchOptions
            )
          }
          #else
          throw AppleVzRunnerCommandError.virtualizationFrameworkUnavailable
          #endif
        },
        writeOutput: { line in
          print(line)
        },
        writeError: { line in
          fputs("\(line)\n", stderr)
        }
      )
    }
  }

  public static func run(
    arguments: [String],
    dependencies: Dependencies = .live
  ) -> Int32 {
    do {
      if isHelpRequested(arguments) {
        dependencies.writeOutput(usage)
        return 0
      }

      let options = try parse(arguments: arguments)
      let data = try readHandoffData(path: options.handoffJSON, dependencies: dependencies)
      let handoff = try AppleVzHandoffValidator.decode(data)
      let validation = try AppleVzHandoffValidator.validate(handoff)
      writeValidationSummary(validation, dependencies: dependencies)

      if options.printConfigPlan {
        let plan = try AppleVzHandoffValidator.configurationPlan(for: handoff)
        writeConfigurationPlan(plan, dependencies: dependencies)
      }

      if options.validateVzConfig {
        let spec = try readLaunchSpec(from: handoff, dependencies: dependencies)
        try dependencies.validateVzConfiguration(spec)
        dependencies.writeOutput("VZ configuration validation: ready")
      }

      if options.validateOnly || (!options.allowRealVzStart && options.isInspectionOnly) {
        return 0
      }

      guard options.allowRealVzStart else {
        throw AppleVzRunnerCommandError.realStartRequiresOptIn
      }

      let spec = try readLaunchSpec(from: handoff, dependencies: dependencies)
      writeLaunchSpecDiagnostics(spec, dependencies: dependencies)
      try dependencies.launchVirtualMachine(spec, options.launchOptions)
      return 0
    } catch {
      dependencies.writeError(formatError(error))
      return 1
    }
  }

  fileprivate static let usage = "usage: AppleVzRunner [--handoff-json PATH] [--validate-only] [--print-config-plan] [--validate-vz-config] [--allow-real-vz-start] [--stop-after-seconds N] [--force-stop-grace-seconds N] [--save-state PATH] [--restore-state PATH] [--display] [--graphics] [--display-width PX] [--display-height PX] [--runtime-control-socket PATH] [--proxy-framebuffer-rgba-file PATH] [--proxy-framebuffer-capture-interval-ms N] [--share [ro:]TAG=HOST_PATH ...] [--share-dir PATH] [--share-tag TAG] [--share-read-only]"

  private static func isHelpRequested(_ arguments: [String]) -> Bool {
    arguments.contains { argument in
      argument == "--help" || argument == "-h"
    }
  }

  private static func formatError(_ error: Error) -> String {
    if error is AppleVzRunnerCommandError || error is AppleVzRunnerError {
      return error.localizedDescription
    }

    let nsError = error as NSError
    var details = [
      nsError.localizedDescription,
      "domain=\(nsError.domain)",
      "code=\(nsError.code)",
    ]
    if let reason = nsError.userInfo[NSLocalizedFailureReasonErrorKey] as? String {
      details.append("reason=\(reason)")
    }
    if let suggestion = nsError.userInfo[NSLocalizedRecoverySuggestionErrorKey] as? String {
      details.append("recovery=\(suggestion)")
    }
    if let underlying = nsError.userInfo[NSUnderlyingErrorKey] as? NSError {
      details.append("underlying=\(formatNSError(underlying))")
    }
    return details.joined(separator: "; ")
  }

  private static func formatNSError(_ error: NSError) -> String {
    var details = [
      error.localizedDescription,
      "domain=\(error.domain)",
      "code=\(error.code)",
    ]
    if let reason = error.userInfo[NSLocalizedFailureReasonErrorKey] as? String {
      details.append("reason=\(reason)")
    }
    if let suggestion = error.userInfo[NSLocalizedRecoverySuggestionErrorKey] as? String {
      details.append("recovery=\(suggestion)")
    }
    return details.joined(separator: ", ")
  }

  private struct Options {
    var handoffJSON: String?
    var validateOnly: Bool
    var printConfigPlan: Bool
    var validateVzConfig: Bool
    var allowRealVzStart: Bool
    var launchOptions: AppleVzLaunchOptions

    var isInspectionOnly: Bool {
      printConfigPlan || validateVzConfig
    }
  }

  private static func parse(arguments: [String]) throws -> Options {
    var handoffJSON: String?
    var validateOnly = false
    var printConfigPlan = false
    var validateVzConfig = false
    var allowRealVzStart = false
    var stopAfterSeconds: UInt64?
    var forceStopGraceSeconds: UInt64?
    var saveStatePath: String?
    var restoreStatePath: String?
    var displayWindow = false
    var displayWidthInPixels = 1280
    var displayHeightInPixels = 800
    var runtimeControlSocketPath: String?
    var proxyFramebufferRGBAPath: String?
    var proxyFramebufferCaptureIntervalMillis: UInt64 = 500
    var graphicsHeadless = false
    // Legacy single-share flags (kept as an alias for one share so the existing
    // run-vz-display-demo.sh, which passes --share-dir, keeps working).
    var sharedDirectoryPath: String?
    var sharedDirectoryTag: String?
    var sharedDirectoryReadOnly = false
    // Repeatable `--share <tag>=<host_path>` (optionally `ro:`-prefixed) flag,
    // one per shared folder, parsed into an ordered array of specs.
    var sharedDirectorySpecs: [AppleVzSharedDirectorySpec] = []
    var index = 0

    while index < arguments.count {
      let argument = arguments[index]
      switch argument {
      case "--handoff-json":
        let valueIndex = index + 1
        guard valueIndex < arguments.count else {
          throw AppleVzRunnerCommandError.missingValue(argument)
        }
        handoffJSON = arguments[valueIndex]
        index += 2
      case "--validate-only":
        validateOnly = true
        index += 1
      case "--print-config-plan":
        printConfigPlan = true
        index += 1
      case "--validate-vz-config":
        validateVzConfig = true
        index += 1
      case "--allow-real-vz-start":
        allowRealVzStart = true
        index += 1
      case "--stop-after-seconds":
        let valueIndex = index + 1
        guard valueIndex < arguments.count else {
          throw AppleVzRunnerCommandError.missingValue(argument)
        }
        guard let parsed = UInt64(arguments[valueIndex]), parsed > 0 else {
          throw AppleVzRunnerCommandError.invalidPositiveInteger(
            argument,
            arguments[valueIndex]
          )
        }
        stopAfterSeconds = parsed
        index += 2
      case "--force-stop-grace-seconds":
        let valueIndex = index + 1
        guard valueIndex < arguments.count else {
          throw AppleVzRunnerCommandError.missingValue(argument)
        }
        guard let parsed = UInt64(arguments[valueIndex]), parsed > 0 else {
          throw AppleVzRunnerCommandError.invalidPositiveInteger(
            argument,
            arguments[valueIndex]
          )
        }
        forceStopGraceSeconds = parsed
        index += 2
      case "--save-state":
        let valueIndex = index + 1
        guard valueIndex < arguments.count else {
          throw AppleVzRunnerCommandError.missingValue(argument)
        }
        saveStatePath = arguments[valueIndex]
        index += 2
      case "--restore-state":
        let valueIndex = index + 1
        guard valueIndex < arguments.count else {
          throw AppleVzRunnerCommandError.missingValue(argument)
        }
        restoreStatePath = arguments[valueIndex]
        index += 2
      case "--display":
        displayWindow = true
        index += 1
      case "--display-width":
        let valueIndex = index + 1
        guard valueIndex < arguments.count else {
          throw AppleVzRunnerCommandError.missingValue(argument)
        }
        displayWidthInPixels = try parseDisplayDimension(argument, arguments[valueIndex])
        index += 2
      case "--display-height":
        let valueIndex = index + 1
        guard valueIndex < arguments.count else {
          throw AppleVzRunnerCommandError.missingValue(argument)
        }
        displayHeightInPixels = try parseDisplayDimension(argument, arguments[valueIndex])
        index += 2
      case "--runtime-control-socket":
        let valueIndex = index + 1
        guard valueIndex < arguments.count else {
          throw AppleVzRunnerCommandError.missingValue(argument)
        }
        runtimeControlSocketPath = arguments[valueIndex]
        index += 2
      case "--proxy-framebuffer-rgba-file":
        let valueIndex = index + 1
        guard valueIndex < arguments.count else {
          throw AppleVzRunnerCommandError.missingValue(argument)
        }
        proxyFramebufferRGBAPath = arguments[valueIndex]
        index += 2
      case "--proxy-framebuffer-capture-interval-ms":
        let valueIndex = index + 1
        guard valueIndex < arguments.count else {
          throw AppleVzRunnerCommandError.missingValue(argument)
        }
        proxyFramebufferCaptureIntervalMillis = try parsePositiveUInt64(
          argument,
          arguments[valueIndex]
        )
        index += 2
      case "--graphics":
        graphicsHeadless = true
        index += 1
      case "--share":
        let valueIndex = index + 1
        guard valueIndex < arguments.count else {
          throw AppleVzRunnerCommandError.missingValue(argument)
        }
        sharedDirectorySpecs.append(try parseShareFlagValue(arguments[valueIndex]))
        index += 2
      case "--share-dir":
        let valueIndex = index + 1
        guard valueIndex < arguments.count else {
          throw AppleVzRunnerCommandError.missingValue(argument)
        }
        sharedDirectoryPath = arguments[valueIndex]
        index += 2
      case "--share-tag":
        let valueIndex = index + 1
        guard valueIndex < arguments.count else {
          throw AppleVzRunnerCommandError.missingValue(argument)
        }
        sharedDirectoryTag = arguments[valueIndex]
        index += 2
      case "--share-read-only":
        sharedDirectoryReadOnly = true
        index += 1
      case "--help", "-h":
        throw AppleVzRunnerCommandError.help
      default:
        throw AppleVzRunnerCommandError.unknownArgument(argument)
      }
    }

    // The launch "mode" flags are mutually exclusive: each selects a different
    // VM configuration + run loop (windowed display vs headless graphics check
    // vs suspend vs resume), and silently letting one override another (e.g.
    // `--display --restore-state` quietly restoring headless) is a footgun.
    let activeModeFlags = [
      ("--display", displayWindow),
      ("--graphics", graphicsHeadless),
      ("--save-state", saveStatePath != nil),
      ("--restore-state", restoreStatePath != nil),
    ]
    .filter { $0.1 }
    .map { $0.0 }
    if activeModeFlags.count > 1 {
      throw AppleVzRunnerCommandError.conflictingFlags(activeModeFlags.joined(separator: ", "))
    }
    if proxyFramebufferRGBAPath != nil && !displayWindow {
      throw AppleVzRunnerCommandError.proxyFramebufferExportRequiresDisplay
    }
    guard AppleVzDisplayLimits.supports(
      width: displayWidthInPixels,
      height: displayHeightInPixels
    ) else {
      throw AppleVzRunnerCommandError.displaySizeTooLarge(
        width: displayWidthInPixels,
        height: displayHeightInPixels,
        maximumPixelCount: AppleVzDisplayLimits.maximumPixelCount
      )
    }

    // Fold the legacy single-share flags (--share-dir/--share-tag/
    // --share-read-only) into one share, prepended so it keeps acting like the
    // old single attachment when used on its own. New repeatable --share flags
    // follow it.
    var shares: [AppleVzSharedDirectorySpec] = []
    if let path = sharedDirectoryPath {
      shares.append(
        AppleVzSharedDirectorySpec(
          path: path,
          tag: sharedDirectoryTag ?? "share",
          readOnly: sharedDirectoryReadOnly
        )
      )
    }
    shares.append(contentsOf: sharedDirectorySpecs)

    return Options(
      handoffJSON: handoffJSON,
      validateOnly: validateOnly,
      printConfigPlan: printConfigPlan,
      validateVzConfig: validateVzConfig,
      allowRealVzStart: allowRealVzStart,
      launchOptions: AppleVzLaunchOptions(
        stopAfterSeconds: stopAfterSeconds,
        forceStopGraceSeconds: forceStopGraceSeconds ?? defaultForceStopGraceSeconds(
          stopAfterSeconds: stopAfterSeconds
        ),
        saveStatePath: saveStatePath,
        restoreStatePath: restoreStatePath,
        displayWindow: displayWindow,
        displayWidthInPixels: displayWidthInPixels,
        displayHeightInPixels: displayHeightInPixels,
        graphicsHeadlessVerification: graphicsHeadless,
        sharedDirectorySpecs: shares,
        runtimeControlSocketPath: runtimeControlSocketPath,
        proxyFramebufferRGBAPath: proxyFramebufferRGBAPath,
        proxyFramebufferCaptureIntervalMillis: proxyFramebufferCaptureIntervalMillis
      )
    )
  }

  private static func parsePositiveUInt64(_ argument: String, _ value: String) throws -> UInt64 {
    guard let parsed = UInt64(value), parsed > 0 else {
      throw AppleVzRunnerCommandError.invalidPositiveInteger(argument, value)
    }
    return parsed
  }

  private static func parseDisplayDimension(_ argument: String, _ value: String) throws -> Int {
    guard let parsed = Int(value), parsed > 0 else {
      throw AppleVzRunnerCommandError.invalidPositiveInteger(argument, value)
    }
    return parsed
  }

  /// Parse one `--share` value of the form `[ro:]<tag>=<host_path>`.
  ///
  /// The optional `ro:` prefix marks the share read-only. The tag is everything
  /// up to the FIRST `=`; the host path is the remainder (so paths may contain
  /// `=`, spaces, or commas). A missing `=` or an empty tag is rejected.
  private static func parseShareFlagValue(_ value: String) throws -> AppleVzSharedDirectorySpec {
    var remainder = Substring(value)
    var readOnly = false
    if remainder.hasPrefix("ro:") {
      readOnly = true
      remainder = remainder.dropFirst(3)
    }
    guard let separator = remainder.firstIndex(of: "=") else {
      throw AppleVzRunnerCommandError.invalidShareValue(value)
    }
    let tag = String(remainder[remainder.startIndex..<separator])
    let path = String(remainder[remainder.index(after: separator)...])
    guard !tag.isEmpty, !path.isEmpty else {
      throw AppleVzRunnerCommandError.invalidShareValue(value)
    }
    return AppleVzSharedDirectorySpec(path: path, tag: tag, readOnly: readOnly)
  }

  private static func defaultForceStopGraceSeconds(stopAfterSeconds: UInt64?) -> UInt64? {
    stopAfterSeconds == nil ? nil : 10
  }

  private static func readHandoffData(
    path: String?,
    dependencies: Dependencies
  ) throws -> Data {
    guard let path else {
      return try dependencies.readStandardInput()
    }
    return try dependencies.readFile(path)
  }

  private static func readLaunchSpec(
    from handoff: AppleVzLaunchHandoff,
    dependencies: Dependencies
  ) throws -> AppleVzLaunchSpec {
    guard let launchSpecPath = handoff.launchSpecPath else {
      throw AppleVzRunnerCommandError.missingLaunchSpecPath
    }

    let data = try dependencies.readFile(launchSpecPath)
    return try JSONDecoder().decode(AppleVzLaunchSpec.self, from: data)
  }

  private static func writeValidationSummary(
    _ validation: AppleVzRunnerValidation,
    dependencies: Dependencies
  ) {
    dependencies.writeOutput("AppleVzRunner handoff ready")
    dependencies.writeOutput("VM: \(validation.vmName)")
    dependencies.writeOutput("Backend: \(validation.backend)")
    dependencies.writeOutput("Boot mode: \(validation.bootMode)")
    dependencies.writeOutput("Disk: \(validation.diskPath)")
    dependencies.writeOutput("Memory MiB: \(validation.memoryMiB)")
    dependencies.writeOutput("CPU count: \(validation.cpuCount)")
    dependencies.writeOutput(
      "Virtualization.framework linked: \(validation.virtualizationFrameworkLinked)"
    )
  }

  private static func writeConfigurationPlan(
    _ plan: AppleVzConfigurationPlan,
    dependencies: Dependencies
  ) {
    dependencies.writeOutput("Configuration plan:")
    dependencies.writeOutput("Boot loader: \(plan.bootLoader)")
    dependencies.writeOutput("Platform: \(plan.platform)")
    dependencies.writeOutput("Disk attachment: \(plan.diskAttachment)")
    dependencies.writeOutput("Network attachment: \(plan.networkAttachment)")
    dependencies.writeOutput("Memory bytes: \(plan.memoryBytes)")
    dependencies.writeOutput("Serial log: \(plan.serialLogPath)")
  }

  private static func writeLaunchSpecDiagnostics(
    _ spec: AppleVzLaunchSpec,
    dependencies: Dependencies
  ) {
    dependencies.writeOutput("Launch spec diagnostics:")
    if let kernel = spec.boot.kernel {
      dependencies.writeOutput("Kernel: \(describePath(kernel.path, declaredExists: kernel.exists))")
    } else {
      dependencies.writeOutput("Kernel: missing from launch spec")
    }
    if let initrd = spec.boot.initrd {
      dependencies.writeOutput("Initrd: \(describePath(initrd.path, declaredExists: initrd.exists))")
    } else {
      dependencies.writeOutput("Initrd: none")
    }
    dependencies.writeOutput(
      "Disk: \(describePath(spec.disk.path, declaredExists: FileManager.default.fileExists(atPath: spec.disk.path)))"
    )
    if let commandLine = spec.boot.kernelCommandLine {
      dependencies.writeOutput("Kernel command line: \(commandLine)")
    }
  }

  private static func describePath(_ path: String, declaredExists: Bool) -> String {
    let url = URL(fileURLWithPath: path)
    let attributes = try? FileManager.default.attributesOfItem(atPath: path)
    let size = (attributes?[.size] as? NSNumber)?.uint64Value
    let actualExists = attributes != nil
    let sizeText = size.map(String.init) ?? "missing"
    return "\(path) (declared_exists=\(declaredExists), actual_exists=\(actualExists), size_bytes=\(sizeText), signature=\(fileSignature(url)))"
  }

  private static func fileSignature(_ url: URL) -> String {
    guard let handle = try? FileHandle(forReadingFrom: url) else {
      return "missing"
    }
    defer { try? handle.close() }
    guard let data = try? handle.read(upToCount: 4), !data.isEmpty else {
      return "empty"
    }
    let bytes = [UInt8](data)
    if bytes.starts(with: [0x4d, 0x5a]) {
      return "pe32/mz"
    }
    if bytes.starts(with: [0x7f, 0x45, 0x4c, 0x46]) {
      return "elf"
    }
    if bytes.starts(with: [0x1f, 0x8b]) {
      return "gzip"
    }
    return bytes.map { String(format: "%02x", $0) }.joined()
  }
}

public enum AppleVzRunnerCommandError: Error, LocalizedError, Equatable {
  case help
  case missingValue(String)
  case invalidPositiveInteger(String, String)
  case displaySizeTooLarge(width: Int, height: Int, maximumPixelCount: Int)
  case invalidShareValue(String)
  case unknownArgument(String)
  case missingLaunchSpecPath
  case virtualizationFrameworkUnavailable
  case realStartRequiresOptIn
  case saveRestoreRequiresMacOS14
  case displayRequiresMacOS14
  case conflictingFlags(String)
  case proxyFramebufferExportRequiresDisplay

  public var errorDescription: String? {
    switch self {
    case .help:
      return AppleVzRunnerCommand.usage
    case .missingValue(let argument):
      return "\(argument) requires a value"
    case .invalidPositiveInteger(let argument, let value):
      return "\(argument) requires a positive integer, got \(value)"
    case .displaySizeTooLarge(let width, let height, let maximumPixelCount):
      return "display size \(width)x\(height) exceeds the \(maximumPixelCount)-pixel limit"
    case .invalidShareValue(let value):
      return "--share requires [ro:]<tag>=<host_path>, got \(value)"
    case .unknownArgument(let argument):
      return "unknown argument \(argument)"
    case .missingLaunchSpecPath:
      return "--validate-vz-config requires handoff launch_spec_path"
    case .virtualizationFrameworkUnavailable:
      return "--validate-vz-config requires Virtualization.framework"
    case .realStartRequiresOptIn:
      return "real Apple VZ start requires --allow-real-vz-start"
    case .saveRestoreRequiresMacOS14:
      return "Apple VZ suspend/resume (--save-state/--restore-state) requires macOS 14 or later"
    case .displayRequiresMacOS14:
      return "Apple VZ embedded display (--display) requires macOS 14 or later"
    case .conflictingFlags(let flags):
      return "conflicting mutually-exclusive launch flags: \(flags)"
    case .proxyFramebufferExportRequiresDisplay:
      return "--proxy-framebuffer-rgba-file requires --display"
    }
  }
}
