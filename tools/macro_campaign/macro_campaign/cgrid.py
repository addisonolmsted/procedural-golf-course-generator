"""CGRID1 read/write (mirrors crates/course-world/src/gridio.rs).

Layout: magic b"CGRID1\\0", version u8, dtype u8 (0=f32, 1=u32, 2=u8),
origin_x f64, origin_y f64, cell_size f64, nx u32, ny u32, payload
row-major LE."""

import struct

import numpy as np

MAGIC = b"CGRID1\x00"
VERSION = 1
DTYPE_F32 = 0
DTYPE_U32 = 1
DTYPE_U8 = 2

_DTYPES = {DTYPE_F32: ("<f4", 4), DTYPE_U32: ("<u4", 4), DTYPE_U8: ("u1", 1)}


def _write(path, data: np.ndarray, dtype_code: int, ox: float, oy: float, cell: float):
    np_dtype, _ = _DTYPES[dtype_code]
    ny, nx = data.shape
    with open(path, "wb") as f:
        f.write(MAGIC)
        f.write(struct.pack("<BB", VERSION, dtype_code))
        f.write(struct.pack("<ddd", ox, oy, cell))
        f.write(struct.pack("<II", nx, ny))
        f.write(np.ascontiguousarray(data, dtype=np_dtype).tobytes())


def _read(path, dtype_code: int):
    np_dtype, size = _DTYPES[dtype_code]
    with open(path, "rb") as f:
        magic = f.read(7)
        ver, dtype = struct.unpack("<BB", f.read(2))
        if magic != MAGIC or ver != VERSION or dtype != dtype_code:
            raise ValueError(f"bad CGRID1 header in {path} (dtype {dtype}, want {dtype_code})")
        ox, oy, cell = struct.unpack("<ddd", f.read(24))
        nx, ny = struct.unpack("<II", f.read(8))
        data = np.frombuffer(f.read(size * nx * ny), dtype=np_dtype).reshape(ny, nx)
    return data.copy(), (ox, oy, cell)


def write_f32(path, data: np.ndarray, origin_x: float, origin_y: float, cell: float):
    """data[y, x] row-major, row 0 = southernmost (y up in world)."""
    _write(path, data, DTYPE_F32, origin_x, origin_y, cell)


def read_f32(path):
    """Return (data[y, x] float32, (origin_x, origin_y, cell))."""
    return _read(path, DTYPE_F32)


def write_u8(path, data: np.ndarray, origin_x: float, origin_y: float, cell: float):
    """u8 raster (class bit-flags), same orientation as write_f32."""
    _write(path, data, DTYPE_U8, origin_x, origin_y, cell)


def read_u8(path):
    """Return (data[y, x] uint8, (origin_x, origin_y, cell))."""
    return _read(path, DTYPE_U8)
