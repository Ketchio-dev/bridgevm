#if canImport(AppKit)
extension FBLayerView {
    func readUInt32(
        from pointer: UnsafeMutableRawPointer,
        offset: Int
    ) -> UInt32 {
        UInt32(littleEndian: pointer.load(fromByteOffset: offset, as: UInt32.self))
    }

    func readUInt64(
        from pointer: UnsafeMutableRawPointer,
        offset: Int
    ) -> UInt64 {
        UInt64(littleEndian: pointer.load(fromByteOffset: offset, as: UInt64.self))
    }
}
#endif
