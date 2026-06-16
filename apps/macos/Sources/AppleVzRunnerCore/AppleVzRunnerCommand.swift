import Foundation

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
          try AppleVzVirtualMachineLauncher.launchLinuxKernelVirtualMachine(
            spec: spec,
            options: launchOptions
          )
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

  fileprivate static let usage = "usage: AppleVzRunner [--handoff-json PATH] [--validate-only] [--print-config-plan] [--validate-vz-config] [--allow-real-vz-start] [--stop-after-seconds N] [--force-stop-grace-seconds N]"

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
      case "--help", "-h":
        throw AppleVzRunnerCommandError.help
      default:
        throw AppleVzRunnerCommandError.unknownArgument(argument)
      }
    }

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
        )
      )
    )
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
  case unknownArgument(String)
  case missingLaunchSpecPath
  case virtualizationFrameworkUnavailable
  case realStartRequiresOptIn

  public var errorDescription: String? {
    switch self {
    case .help:
      return AppleVzRunnerCommand.usage
    case .missingValue(let argument):
      return "\(argument) requires a value"
    case .invalidPositiveInteger(let argument, let value):
      return "\(argument) requires a positive integer, got \(value)"
    case .unknownArgument(let argument):
      return "unknown argument \(argument)"
    case .missingLaunchSpecPath:
      return "--validate-vz-config requires handoff launch_spec_path"
    case .virtualizationFrameworkUnavailable:
      return "--validate-vz-config requires Virtualization.framework"
    case .realStartRequiresOptIn:
      return "real Apple VZ start requires --allow-real-vz-start"
    }
  }
}
