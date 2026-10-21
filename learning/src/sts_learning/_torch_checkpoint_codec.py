"""Versioned canonical tensor-state codec used by the checkpoint store."""

from __future__ import annotations

import math
import struct
from collections.abc import Mapping

import numpy as np
import torch


class TorchCheckpointError(RuntimeError):
    """A checkpoint is malformed, unbounded, corrupt, or incompatible."""


_MAGIC = b"STS-TORCH-STATE\x00"
_FORMAT_VERSION = 1
_MAX_TENSORS = 1_000_000
_MAX_DIMENSIONS = 32
_MAX_NAME_BYTES = 1 << 20
_DTYPES: dict[torch.dtype, tuple[int, np.dtype[object]]] = {
    torch.float16: (1, np.dtype("<f2")),
    torch.float32: (2, np.dtype("<f4")),
    torch.float64: (3, np.dtype("<f8")),
    torch.int8: (4, np.dtype("i1")),
    torch.uint8: (5, np.dtype("u1")),
    torch.int16: (6, np.dtype("<i2")),
    torch.int32: (7, np.dtype("<i4")),
    torch.int64: (8, np.dtype("<i8")),
    torch.bool: (9, np.dtype("?")),
}
_DTYPES_BY_ID = {
    identifier: (dtype, numpy)
    for dtype, (identifier, numpy) in _DTYPES.items()
}


def encode_state_dict(
    state: Mapping[str, object],
    *,
    max_bytes: int,
) -> bytes:
    if not isinstance(state, Mapping):
        raise TorchCheckpointError("model state_dict must be a mapping")
    raw_names = tuple(state)
    if not all(isinstance(name, str) for name in raw_names):
        raise TorchCheckpointError("model state keys must be strings")
    names = sorted(raw_names)
    if len(names) > _MAX_TENSORS:
        raise TorchCheckpointError("model checkpoint has too many tensors")
    output = bytearray(_MAGIC)
    output.extend(struct.pack(">II", _FORMAT_VERSION, len(names)))
    for name in names:
        tensor = state[name]
        if not isinstance(tensor, torch.Tensor):
            raise TorchCheckpointError("model state values must be tensors")
        if tensor.layout is not torch.strided or tensor.is_quantized:
            raise TorchCheckpointError("checkpoint supports only dense tensors")
        try:
            dtype_id, numpy_dtype = _DTYPES[tensor.dtype]
        except KeyError as error:
            raise TorchCheckpointError(
                f"unsupported checkpoint tensor dtype {tensor.dtype}"
            ) from error
        name_bytes = name.encode("utf-8")
        shape = tuple(tensor.shape)
        if len(name_bytes) > _MAX_NAME_BYTES:
            raise TorchCheckpointError("checkpoint state key is too large")
        if len(shape) > _MAX_DIMENSIONS:
            raise TorchCheckpointError("checkpoint tensor has too many dimensions")
        array = tensor.detach().cpu().contiguous().numpy()
        array = array.astype(numpy_dtype, copy=False)
        data_bytes = array.nbytes
        header_bytes = 4 + len(name_bytes) + 1 + 4 + 8 * len(shape) + 8
        if len(output) + header_bytes + data_bytes > max_bytes:
            raise TorchCheckpointError("model checkpoint exceeds its byte limit")
        output.extend(struct.pack(">I", len(name_bytes)))
        output.extend(name_bytes)
        output.extend(struct.pack(">BI", dtype_id, len(shape)))
        for dimension in shape:
            output.extend(struct.pack(">Q", dimension))
        output.extend(struct.pack(">Q", data_bytes))
        output.extend(array.tobytes(order="C"))
    return bytes(output)


def decode_state_dict(payload: bytes) -> dict[str, torch.Tensor]:
    if not isinstance(payload, bytes) or not payload.startswith(_MAGIC):
        raise TorchCheckpointError("checkpoint magic is invalid")
    view = memoryview(payload)
    position = len(_MAGIC)
    version, tensor_count = struct.unpack(">II", _read(view, position, 8))
    position += 8
    if version != _FORMAT_VERSION:
        raise TorchCheckpointError("checkpoint format version is unsupported")
    if tensor_count > _MAX_TENSORS:
        raise TorchCheckpointError("checkpoint declares too many tensors")
    state: dict[str, torch.Tensor] = {}
    for _ in range(tensor_count):
        name_length = struct.unpack(">I", _read(view, position, 4))[0]
        position += 4
        if name_length > _MAX_NAME_BYTES:
            raise TorchCheckpointError("checkpoint state key is too large")
        name_bytes = bytes(_read(view, position, name_length))
        position += name_length
        try:
            name = name_bytes.decode("utf-8")
        except UnicodeDecodeError as error:
            raise TorchCheckpointError("checkpoint state key is not UTF-8") from error
        if name in state:
            raise TorchCheckpointError("checkpoint repeats a state key")
        dtype_id, dimensions = struct.unpack(">BI", _read(view, position, 5))
        position += 5
        if dimensions > _MAX_DIMENSIONS:
            raise TorchCheckpointError("checkpoint tensor has too many dimensions")
        try:
            torch_dtype, numpy_dtype = _DTYPES_BY_ID[dtype_id]
        except KeyError as error:
            raise TorchCheckpointError("checkpoint tensor dtype is unknown") from error
        shape = []
        for _ in range(dimensions):
            shape.append(struct.unpack(">Q", _read(view, position, 8))[0])
            position += 8
        data_length = struct.unpack(">Q", _read(view, position, 8))[0]
        position += 8
        expected_length = math.prod(shape) * numpy_dtype.itemsize
        if data_length != expected_length:
            raise TorchCheckpointError("checkpoint tensor byte length is inconsistent")
        raw = _read(view, position, data_length)
        position += data_length
        native_dtype = numpy_dtype.newbyteorder("=")
        array = np.frombuffer(raw, dtype=numpy_dtype).astype(native_dtype, copy=True)
        state[name] = torch.from_numpy(array.reshape(tuple(shape))).to(torch_dtype)
    if position != len(view):
        raise TorchCheckpointError("checkpoint contains trailing bytes")
    return state


def validate_compatible_state(
    expected: Mapping[str, object],
    restored: Mapping[str, torch.Tensor],
) -> None:
    if set(expected) != set(restored):
        raise TorchCheckpointError("checkpoint state keys do not match model definition")
    for name, expected_value in expected.items():
        if not isinstance(expected_value, torch.Tensor):
            raise TorchCheckpointError("model state contains a non-tensor value")
        restored_value = restored[name]
        if expected_value.dtype != restored_value.dtype:
            raise TorchCheckpointError(f"checkpoint dtype does not match for {name}")
        if tuple(expected_value.shape) != tuple(restored_value.shape):
            raise TorchCheckpointError(f"checkpoint shape does not match for {name}")


def _read(view: memoryview, position: int, length: int) -> memoryview:
    end = position + length
    if position < 0 or length < 0 or end > len(view):
        raise TorchCheckpointError("checkpoint ended before a complete field")
    return view[position:end]
