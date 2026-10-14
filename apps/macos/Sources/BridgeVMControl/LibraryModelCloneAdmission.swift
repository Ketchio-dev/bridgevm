import Foundation

typealias LibraryCloneOperation = @Sendable (String, VMConfig, URL) -> VMConfig?

extension LibraryModel {
    var cloningSlugs: Set<String> { Set(cloningConfigurations.keys) }

    func controlWorkAdmission(for model: ControlModel) -> LibraryWorkAdmission {
        let admission = boundWorkAdmission(slug: model.config.slug) { [weak model] owner, current in
            model.map { owner.ownsControlModel($0, for: current) } ?? false
        }
        return { [weak self, weak model] report in
            if let refusal = admission(report) { return refusal }
            guard let self, let model, model.config.engineKind != .hvfEngine else { return nil }
            // Use the accepted source even if a rescan changes or removes its registration.
            guard self.cloningConfigurations.values.contains(where: {
                GenericRuntimeAssetReferences.overlaps($0, model.config)
            }) else { return nil }
            let refusal = "이 VM의 파일을 사용하는 복제 작업이 진행 중입니다. 복제가 끝난 뒤 다시 시도하세요."
            if report { self.operationError = refusal }
            return refusal
        }
    }
}
