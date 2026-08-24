"""Lock-free IOSurface access for the B4 visible-reaction watcher."""
import ctypes, hashlib

lib = ctypes.CDLL("/System/Library/Frameworks/IOSurface.framework/IOSurface")
lib.IOSurfaceLookup.argtypes = [ctypes.c_uint32]; lib.IOSurfaceLookup.restype = ctypes.c_void_p
lib.IOSurfaceLock.argtypes = [ctypes.c_void_p, ctypes.c_uint32, ctypes.POINTER(ctypes.c_uint32)]
lib.IOSurfaceUnlock.argtypes = lib.IOSurfaceLock.argtypes
for name, restype in (("IOSurfaceGetBaseAddress", ctypes.c_void_p), ("IOSurfaceGetBytesPerRow", ctypes.c_size_t),
                      ("IOSurfaceGetWidth", ctypes.c_size_t), ("IOSurfaceGetHeight", ctypes.c_size_t),
                      ("IOSurfaceGetSeed", ctypes.c_uint32)):
    getattr(lib, name).argtypes = [ctypes.c_void_p]; getattr(lib, name).restype = restype


def surface(path):
    ident, width, height = map(int, path.read_text().split())
    ref = lib.IOSurfaceLookup(ident)
    if not ref or (width, height) != (1600, 900): raise RuntimeError("invalid active IOSurface")
    return ref, ident


def seed(ref):
    return lib.IOSurfaceGetSeed(ref)


def frame(ref, full=False):
    locked = ctypes.c_uint32()
    if lib.IOSurfaceLock(ref, 1, ctypes.byref(locked)): raise RuntimeError("IOSurface lock failed")
    try:
        width, height, stride = lib.IOSurfaceGetWidth(ref), lib.IOSurfaceGetHeight(ref), lib.IOSurfaceGetBytesPerRow(ref)
        base = lib.IOSurfaceGetBaseAddress(ref)
        if (width, height) != (1600, 900) or not base: raise RuntimeError("bad IOSurface geometry")
        x0, y0, side = width // 2 - 64, height // 2 - 64, 128
        region = b"".join(ctypes.string_at(base + y * stride + x0 * 4, side * 4) for y in range(y0, y0 + side))
        return region, (ctypes.string_at(base, height * stride) if full else b""), locked.value
    finally: lib.IOSurfaceUnlock(ref, 1, ctypes.byref(locked))


def white_pixels(region):
    return sum(region[i:i + 4] == b"\xff\xff\xff\xff" for i in range(0, len(region), 4))


def digest(region):
    return hashlib.sha256(region).hexdigest()
