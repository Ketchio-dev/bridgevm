import CryptoKit
import Foundation

/// "Mac으로 인쇄" (vm.json `printToMac`; nil = on, as Parallels shares the Mac's printers by default; false =
/// off). On, the app sends `PRINTWATCH` at each `BVAGENT SERVICE start`; the agent makes the Windows printer
/// `Mac (BridgeVM)` (bvagent.ps1, print), the probe brings each job over into `<evidence>/print` and logs
/// `BVPRINT job bytes=<n> sha256=<hex> path=<path>` (agent_console/print_jobs.rs), and the app opens the macOS
/// print panel for it, one job at a time, then deletes the file. Turned off, the app sends `PRINTWATCH off` once
/// per app session, which removes the printer.
enum HvfGuestPrint {
    static let watchCommand = "PRINTWATCH"
    static let offCommand = "PRINTWATCH off"
    static let jobPrefix = "BVPRINT job "
    static let refusedPrefix = "BVPRINT refused "
    /// The agent's PRINTWATCH output when it could not make the printer.
    static let printerFailed = "BVPRINT printer=failed"
    /// agent_console/print_jobs.rs MAX_PRINT_JOB_BYTES.
    static let maxJobBytes = 256 << 20

    struct Job: Equatable {
        let url: URL
        let bytes: Int
        /// The line's digest; nil for a job an earlier probe left (`leftovers`).
        let sha256: String?
    }

    /// `<start>-<n>.pdf`, as the probe names a job (its start in Unix ms, its counter), in that order.
    static func jobOrder(_ name: String) -> (UInt64, UInt64)? {
        guard name.hasSuffix(".pdf") else { return nil }
        let parts = name.dropLast(4).split(separator: "-", omittingEmptySubsequences: false)
        guard parts.count == 2, parts.allSatisfy({ (1...19).contains($0.count) && $0.allSatisfy { $0.isASCII && $0.isNumber } }),
              let start = UInt64(parts[0]), let n = UInt64(parts[1]) else { return nil }
        return (start, n)
    }

    /// The job a probe line names, only as a file named `<start>-<n>.pdf` directly in `directory` (the probe's
    /// print folder): run.log also holds the guest's output, so no line chooses another Mac file.
    static func job(from line: String, directory: URL) -> Job? {
        let line = line.hasSuffix("\r") ? String(line.dropLast()) : line
        guard line.hasPrefix(jobPrefix + "bytes="), let pathRange = line.range(of: " path=") else { return nil }
        let fields = line[line.index(line.startIndex, offsetBy: jobPrefix.count)..<pathRange.lowerBound].split(separator: " ")
        guard fields.count == 2, fields[0].hasPrefix("bytes="), fields[1].hasPrefix("sha256="),
              let bytes = Int(fields[0].dropFirst(6)), bytes > 0, bytes <= maxJobBytes else { return nil }
        let sha = String(fields[1].dropFirst(7))
        guard sha.count == 64, sha.allSatisfy({ $0.isHexDigit && !$0.isUppercase }) else { return nil }
        let path = String(line[pathRange.upperBound...])
        let url = URL(fileURLWithPath: path)
        let name = url.lastPathComponent
        guard path.hasPrefix("/"), jobOrder(name) != nil,
              url.deletingLastPathComponent().resolvingSymlinksInPath().path == directory.resolvingSymlinksInPath().path
        else { return nil }
        return Job(url: url, bytes: bytes, sha256: sha)
    }

    /// The jobs in the print folder, oldest first: at an agent start, those an earlier probe published that no
    /// print panel showed (the app quit or the VM stopped first). Only the probe writes there (0700).
    static func leftovers(in directory: URL) -> [Job] {
        let names = (try? FileManager.default.contentsOfDirectory(atPath: directory.path)) ?? []
        return names.compactMap { name -> (UInt64, UInt64, Job)? in
            let url = directory.appendingPathComponent(name)
            guard let order = jobOrder(name), let attributes = try? FileManager.default.attributesOfItem(atPath: url.path),
                  attributes[.type] as? FileAttributeType == .typeRegular,
                  let bytes = (attributes[.size] as? NSNumber)?.intValue, bytes > 0, bytes <= maxJobBytes else { return nil }
            return (order.0, order.1, Job(url: url, bytes: bytes, sha256: nil))
        }.sorted { ($0.0, $0.1) < ($1.0, $1.1) }.map(\.2)
    }

    /// The file is there, a regular file (not a link) of the line's size and SHA-256 (when it had a line) that
    /// begins as a PDF does.
    static func verified(_ job: Job) -> Bool {
        guard let attributes = try? FileManager.default.attributesOfItem(atPath: job.url.path),
              attributes[.type] as? FileAttributeType == .typeRegular,
              (attributes[.size] as? NSNumber)?.intValue == job.bytes,
              let data = try? Data(contentsOf: job.url, options: .mappedIfSafe),
              data.count == job.bytes, data.starts(with: Data("%PDF-".utf8)) else { return false }
        guard let sha256 = job.sha256 else { return true }
        return SHA256.hash(data: data).map { String(format: "%02x", $0) }.joined() == sha256
    }

    /// Whether the job file is still there at all: a line read again (a reattached session reads run.log from
    /// its start) names a job printed and deleted long ago.
    static func exists(_ job: Job) -> Bool {
        (try? FileManager.default.attributesOfItem(atPath: job.url.path)) != nil
    }
}

/// Shows the print panel for a job and calls `done` once it is over (printed or cancelled); false when it cannot
/// show one. Tests substitute their own.
@MainActor
protocol HvfPrintPresenter: AnyObject {
    func present(_ url: URL, done: @escaping @MainActor () -> Void) -> Bool
}

#if canImport(AppKit) && canImport(PDFKit)
import AppKit
import PDFKit

/// The macOS print panel for a Windows print job, as a sheet on the app's window so the VM keeps running, with
/// the app brought forward.
@MainActor
final class HvfPDFPrintPresenter: NSObject, HvfPrintPresenter {
    private var done: (@MainActor () -> Void)?

    func present(_ url: URL, done: @escaping @MainActor () -> Void) -> Bool {
        guard let document = PDFDocument(url: url), document.pageCount > 0,
              let operation = document.printOperation(for: NSPrintInfo.shared, scalingMode: .pageScaleDownToFit, autoRotate: true)
        else { return false }
        operation.jobTitle = "Windows"
        operation.showsPrintPanel = true
        operation.showsProgressPanel = true
        ControlAppActivation.activate()
        guard let window = NSApp.keyWindow ?? NSApp.mainWindow ?? NSApp.windows.first(where: { $0.isVisible && $0.canBecomeKey }) else {
            // No window to hold a sheet: the panel on its own, modal while it is open.
            operation.run()
            done()
            return true
        }
        self.done = done
        operation.runModal(for: window, delegate: self, didRun: #selector(printOperationDidRun(_:success:contextInfo:)), contextInfo: nil)
        return true
    }

    @objc private func printOperationDidRun(_ operation: NSPrintOperation, success: Bool, contextInfo: UnsafeMutableRawPointer?) {
        let done = self.done
        self.done = nil
        done?()
    }
}
#endif

/// The last thing about Windows printing the user should know; cleared by the next job printed.
@MainActor
final class HvfGuestPrintNotice: ObservableObject {
    enum Kind: Equatable { case printerFailed, tooLarge, notReceived, notOpened }

    @Published private(set) var shown: Kind?

    func show(_ kind: Kind?) {
        if shown != kind { shown = kind }
    }

    var text: String? {
        switch shown {
        case nil: return nil
        case .printerFailed: return "Windows에 Mac 프린터를 만들지 못했으니 Windows 기능에서 'Microsoft Print to PDF'를 켠 뒤 VM을 다시 시작하세요."
        case .tooLarge: return "Windows 인쇄 작업이 256MB보다 커서 Mac으로 가져오지 않았습니다."
        case .notReceived: return "Windows 인쇄 작업을 Mac으로 가져오지 못했으니 다시 인쇄해 보세요."
        case .notOpened: return "Windows 인쇄 작업을 Mac에서 열지 못했으니 다시 인쇄해 보세요."
        }
    }
}

extension HvfEngineSession {
    /// vm.json `printToMac` for a library VM: nil (never set) and true are on, false is off.
    var printToMacSetting: Bool? { config.libraryContext?.config.printToMac }
    var printToMacWanted: Bool { config.libraryContext != nil && printToMacSetting != false }

    /// The probe's print folder (BRIDGEVM_VIRTIO_CONSOLE_PRINT_DIR).
    var printDirectory: URL { URL(fileURLWithPath: config.evidenceDir).appendingPathComponent("print", isDirectory: true) }

    /// At `BVAGENT SERVICE start`. On: the agent makes the printer (only what is missing) and watches it, every
    /// boot, and jobs an earlier probe left join the print panels. Turned off: the printer's removal once per app
    /// session.
    func applyPrintSetting() {
        if printToMacWanted {
            printOffSent = false
            _ = sendCtl(HvfGuestPrint.watchCommand)
            HvfGuestPrint.leftovers(in: printDirectory).forEach(queuePrintJob)
            presentNextPrintJob()
        } else if printToMacSetting == false, !printOffSent {
            printOffSent = sendCtl(HvfGuestPrint.offCommand)
        }
    }

    /// The agent's PRINTWATCH output: a printer it could not make is one notice per app session.
    func printWatchReported(_ output: String) {
        guard printToMacWanted, output.contains(HvfGuestPrint.printerFailed), !printSetupFailureShown else { return }
        printSetupFailureShown = true
        printNotice.show(.printerFailed)
    }

    /// A probe `BVPRINT ...` line: a job joins the queue of print panels, a refusal is a notice.
    func printLineReported(_ line: String) {
        guard printToMacWanted else { return }
        if line.hasPrefix(HvfGuestPrint.refusedPrefix) {
            return printNotice.show(line.contains(" reason=too-large ") ? .tooLarge : .notReceived)
        }
        guard let job = HvfGuestPrint.job(from: line, directory: printDirectory), HvfGuestPrint.exists(job) else { return }
        queuePrintJob(job)
        presentNextPrintJob()
    }

    /// Each job file once: its line read again, or found again at an agent start, adds nothing.
    private func queuePrintJob(_ job: HvfGuestPrint.Job) {
        guard printShowing != job.url, !printQueue.contains(where: { $0.url == job.url }) else { return }
        printQueue.append(job)
    }

    /// One print panel at a time; each job's file goes once its panel is over, or at once when it cannot be shown.
    func presentNextPrintJob() {
        guard printShowing == nil, !printQueue.isEmpty else { return }
        let job = printQueue.removeFirst()
        let generation = printGeneration
        let verify = printVerifier
        // Reserve this exact job before yielding, including duplicate lines during verification.
        printShowing = job.url
        printVerificationTask = Task { [weak self] in
            let verified = await Task.detached(priority: .utility) { verify(job) }.value
            guard let self, self.printGeneration == generation, self.printShowing == job.url else { return }
            self.printVerificationTask = nil
            guard verified else {
                self.printShowing = nil
                self.removePrintJob(job.url)
                self.printNotice.show(.notReceived)
                self.presentNextPrintJob()
                return
            }
            let shown = self.printPresenter.present(job.url) { [weak self] in
                guard self?.printGeneration == generation else { return }
                self?.printJobFinished(job.url)
            }
            if !shown, self.printShowing == job.url {
                self.printShowing = nil
                self.removePrintJob(job.url)
                self.printNotice.show(.notOpened)
                self.presentNextPrintJob()
            }
        }
    }

    func resetPendingPrintJobs() {
        printGeneration = UUID()
        printVerificationTask?.cancel()
        printVerificationTask = nil
        printShowing = nil
        printQueue = [] // Files stay for the next agent start, as before.
    }

    private func printJobFinished(_ url: URL) {
        guard printShowing == url else { return }
        printShowing = nil
        removePrintJob(url)
        if printNotice.shown != .printerFailed { printNotice.show(nil) }
        presentNextPrintJob()
    }

    private func removePrintJob(_ url: URL) {
        try? FileManager.default.removeItem(at: url)
    }

    /// What the toggle shows afterwards, vm.json's value, plus any refusal. Off is saved as false, so the next
    /// agent start removes the printer; on again goes back to the default.
    func applyPrintToggle(_ enabled: Bool,
        publish: RegistrationPublish = { VMRegistrationWriter.commit($0, to: $1) }) -> (isOn: Bool, message: String?) {
        let message = saveLibraryPreference("인쇄 설정", publish: publish) {
            $0.printToMac = enabled ? ($0.printToMac == nil ? nil : true) : false
        }
        return (printToMacWanted, message)
    }
}
