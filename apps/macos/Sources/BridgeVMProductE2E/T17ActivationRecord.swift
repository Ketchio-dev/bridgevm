/// Fixed typed metadata only: no titles, paths, text values or arbitrary errors.
struct T17ActivationRecord {
    var succeeded = false
    var attempts = 0
    var elapsedMilliseconds: UInt64 = 0
    var nativeActivationAccepted: Bool?
    var axSetCode: Int32?
    var axReadCode: Int32?
    var nativeActive: Bool?
    var axFront: Bool?
    var observedFrontPID: Int32?
}
