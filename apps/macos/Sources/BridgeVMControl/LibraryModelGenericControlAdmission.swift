import Foundation

extension LibraryModel {
    private var activeGenericControls: [(VMConfig, ControlModel)] {
        retainedControlRecords.compactMap {
            guard case let .generic(config, model) = $0.descriptor, model.hasActiveRuntimeWork else { return nil }
            return (config, model)
        }
    }

    func genericRetainedWorkRefusal(for config: VMConfig) -> String? {
        guard activeGenericControls.contains(where: {
            $0.0.slug == config.slug || GenericRuntimeAssetReferences.overlaps($0.0, config)
        }) else { return nil }
        return "보관한 VM 제어가 이 항목의 파일을 사용 중입니다. 기존 실행과 작업이 완전히 끝난 뒤 다시 시도하세요."
    }

    func genericRetainedFileRefusal(for config: VMConfig, action: LibraryFileAction) -> String? {
        if action == .clone {
            if let refusal = genericRetainedWorkRefusal(for: config) { return refusal }
            if activeCachedGenericConfigurations.contains(where: { GenericRuntimeAssetReferences.overlaps($0, config) }) {
                return "VM 제어가 이 항목의 파일을 사용 중입니다. 기존 실행과 작업이 완전히 끝난 뒤 다시 시도하세요."
            }
        }
        let affected = action == .deletion ? rootURL.appendingPathComponent(config.slug)
            : URL(fileURLWithPath: config.bundlePath)
        guard activeGenericControls.contains(where: {
            $0.0.slug == config.slug || GenericRuntimeAssetReferences.mayAffect($0.0, path: affected)
        }) else { return nil }
        return "보관한 VM 제어가 이 항목의 파일을 사용 중입니다. 기존 실행과 작업이 완전히 끝난 뒤 다시 시도하세요."
    }

    func legacyStopAdmission(for model: ControlModel) -> LibraryWorkAdmission? {
        guard model.config.engineKind != .hvfEngine else { return nil }
        return { [weak self, weak model] report in
            guard let self, let model else { return "VM 제어를 사용할 수 없습니다." }
            let refusal: String?
            if self.vms.first(where: { $0.slug == model.config.slug }) != model.config
                || !self.ownsControlModel(model, for: model.config) {
                refusal = "VM 제어 대상이 변경되어 정지하지 않았습니다."
            } else { refusal = self.genericRetainedWorkRefusal(for: model.config) }
            if report, let refusal { self.operationError = refusal }
            return refusal
        }
    }
}

private enum GenericRuntimeAssetReferences {
    static func mayAffect(_ config: VMConfig, path: URL) -> Bool {
        HvfPendingDeletionReferences.references(config: config, removedEntry: path)
            || (!config.bundlePath.isEmpty && LibraryDeletionPathReference.mayReference(path.path,
                removedEntry: URL(fileURLWithPath: config.bundlePath)))
    }

    static func overlaps(_ lhs: VMConfig, _ rhs: VMConfig) -> Bool {
        [(lhs, rhs), (rhs, lhs)].contains { source, target in
            let paths = [target.bundlePath, target.diskPath, target.launchSpecPath, target.handoffPath]
            return paths.compactMap { $0 }.filter { !$0.isEmpty }.contains {
                HvfPendingDeletionReferences.references(config: source, removedEntry: URL(fileURLWithPath: $0))
            }
        }
    }
}
