"""Generic, stride-safe snapshot conversion for a host IOSurface."""
import ctypes, hashlib

lib = ctypes.CDLL("/System/Library/Frameworks/IOSurface.framework/IOSurface")
lib.IOSurfaceLookup.argtypes = [ctypes.c_uint32]; lib.IOSurfaceLookup.restype = ctypes.c_void_p
lib.IOSurfaceLock.argtypes = [ctypes.c_void_p, ctypes.c_uint32, ctypes.POINTER(ctypes.c_uint32)]
lib.IOSurfaceUnlock.argtypes = lib.IOSurfaceLock.argtypes
for name, restype in (("IOSurfaceGetBaseAddress", ctypes.c_void_p), ("IOSurfaceGetBytesPerRow", ctypes.c_size_t),
                      ("IOSurfaceGetBytesPerElement", ctypes.c_size_t), ("IOSurfaceGetWidth", ctypes.c_size_t),
                      ("IOSurfaceGetHeight", ctypes.c_size_t), ("IOSurfaceGetSeed", ctypes.c_uint32)):
    getattr(lib, name).argtypes = [ctypes.c_void_p]; getattr(lib, name).restype = restype

def lookup(path):
    values = path.read_text().split()
    if len(values) != 3: raise RuntimeError("invalid IOSurface descriptor")
    ident, width, height = map(int, values); ref = lib.IOSurfaceLookup(ident)
    if not ref or width <= 0 or height <= 0: raise RuntimeError("active IOSurface unavailable")
    return ref, ident, width, height

def seed(ref): return lib.IOSurfaceGetSeed(ref)

def validate(width, height, stride, element, base):
    if width <= 0 or height <= 0 or element != 4 or stride < width * element or not base:
        raise RuntimeError("unsupported IOSurface storage")

def snapshot(ref, expected_width, expected_height):
    locked = ctypes.c_uint32()
    if lib.IOSurfaceLock(ref, 1, ctypes.byref(locked)): raise RuntimeError("IOSurface lock failed")
    try:
        width, height = lib.IOSurfaceGetWidth(ref), lib.IOSurfaceGetHeight(ref)
        stride, element = lib.IOSurfaceGetBytesPerRow(ref), lib.IOSurfaceGetBytesPerElement(ref)
        base = lib.IOSurfaceGetBaseAddress(ref); validate(width, height, stride, element, base)
        if (width, height) != (expected_width, expected_height): raise RuntimeError("IOSurface geometry changed")
        frame = b"".join(ctypes.string_at(base + row * stride, width * 4) for row in range(height))
        return frame, locked.value
    finally: lib.IOSurfaceUnlock(ref, 1, ctypes.byref(locked))

def ppm_from_bgra(frame, width, height):
    if len(frame) != width * height * 4: raise RuntimeError("invalid tight BGRA frame")
    rgb = bytearray(width * height * 3)
    rgb[0::3], rgb[1::3], rgb[2::3] = frame[2::4], frame[1::4], frame[0::4]
    return f"P6\n{width} {height}\n255\n".encode() + rgb

def nonblack_pixels(frame):
    return sum(any(frame[i:i + 3]) for i in range(0, len(frame), 4))

def validate_presented(initial_seed, captured_seed, frame):
    if captured_seed == initial_seed: raise RuntimeError("active IOSurface seed did not advance")
    count = nonblack_pixels(frame)
    if count == 0: raise RuntimeError("active IOSurface frame is all black")
    return count

def sha256(data): return hashlib.sha256(data).hexdigest()
