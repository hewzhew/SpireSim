"""Canonical bounded tensor/scalar trees for optimizer and RNG ownership."""

from __future__ import annotations

import math
import operator
import struct
from collections.abc import Mapping

import numpy as np
import torch


class TorchOwnerStateError(RuntimeError):
    """An optimizer or generator state tree is unsafe or malformed."""


_MAGIC = b"STS-TORCH-OWNER\x00"
_FORMAT_VERSION = 1
_MAX_NODES = 1_000_000
_MAX_DEPTH = 64
_MAX_CONTAINER_ITEMS = 1_000_000
_MAX_STRING_BYTES = 1 << 20
_MAX_DIMENSIONS = 32

_NONE = 0
_FALSE = 1
_TRUE = 2
_INTEGER = 3
_FLOAT = 4
_STRING = 5
_BYTES = 6
_LIST = 7
_TUPLE = 8
_MAPPING = 9
_TENSOR = 10
_UNSIGNED_INTEGER = 11

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
    identifier: (dtype, numpy_dtype)
    for dtype, (identifier, numpy_dtype) in _DTYPES.items()
}


class _Writer:
    def __init__(self, max_bytes: int) -> None:
        self.max_bytes = _positive_integer(max_bytes, "max_bytes")
        self.data = bytearray()

    def append(self, value: bytes) -> None:
        if len(self.data) + len(value) > self.max_bytes:
            raise TorchOwnerStateError("owner state exceeds its byte limit")
        self.data.extend(value)


class _Reader:
    def __init__(self, payload: bytes, max_bytes: int) -> None:
        limit = _positive_integer(max_bytes, "max_bytes")
        if not isinstance(payload, bytes):
            raise TorchOwnerStateError("owner state payload must be bytes")
        if len(payload) > limit:
            raise TorchOwnerStateError("owner state exceeds its byte limit")
        self.view = memoryview(payload)
        self.position = 0
        self.nodes = 0

    def read(self, length: int) -> memoryview:
        end = self.position + length
        if length < 0 or end > len(self.view):
            raise TorchOwnerStateError("owner state ended before a complete field")
        value = self.view[self.position : end]
        self.position = end
        return value

    def node(self, depth: int) -> None:
        if depth > _MAX_DEPTH:
            raise TorchOwnerStateError("owner state exceeds its nesting limit")
        self.nodes += 1
        if self.nodes > _MAX_NODES:
            raise TorchOwnerStateError("owner state has too many nodes")


def encode_owner_state(value: object, *, max_bytes: int) -> bytes:
    """Encode one canonical tree of tensors and non-executable scalar values."""

    writer = _Writer(max_bytes)
    writer.append(_MAGIC)
    writer.append(struct.pack(">I", _FORMAT_VERSION))
    nodes = [0]
    _encode_value(value, writer, depth=0, nodes=nodes)
    return bytes(writer.data)


def decode_owner_state(payload: bytes, *, max_bytes: int) -> object:
    """Decode and canonicalize one bounded owner-state tree."""

    reader = _Reader(payload, max_bytes)
    if bytes(reader.read(len(_MAGIC))) != _MAGIC:
        raise TorchOwnerStateError("owner state magic is invalid")
    version = struct.unpack(">I", reader.read(4))[0]
    if version != _FORMAT_VERSION:
        raise TorchOwnerStateError("owner state format version is unsupported")
    value = _decode_value(reader, depth=0)
    if reader.position != len(reader.view):
        raise TorchOwnerStateError("owner state contains trailing bytes")
    if encode_owner_state(value, max_bytes=max_bytes) != payload:
        raise TorchOwnerStateError("owner state encoding is not canonical")
    return value


def _encode_value(
    value: object,
    writer: _Writer,
    *,
    depth: int,
    nodes: list[int],
) -> None:
    _encode_node(depth, nodes)
    if value is None:
        writer.append(bytes([_NONE]))
    elif value is False:
        writer.append(bytes([_FALSE]))
    elif value is True:
        writer.append(bytes([_TRUE]))
    elif type(value) is int:
        if -(1 << 63) <= value < (1 << 63):
            writer.append(bytes([_INTEGER]) + struct.pack(">q", value))
        elif 0 <= value < (1 << 64):
            writer.append(bytes([_UNSIGNED_INTEGER]) + struct.pack(">Q", value))
        else:
            raise TorchOwnerStateError("owner state integer is outside 64-bit range")
    elif type(value) is float:
        if not math.isfinite(value):
            raise TorchOwnerStateError("owner state float must be finite")
        writer.append(bytes([_FLOAT]) + struct.pack(">d", value))
    elif isinstance(value, str):
        encoded = value.encode("utf-8")
        if len(encoded) > _MAX_STRING_BYTES:
            raise TorchOwnerStateError("owner state string is too large")
        writer.append(bytes([_STRING]) + struct.pack(">I", len(encoded)) + encoded)
    elif isinstance(value, bytes):
        writer.append(bytes([_BYTES]) + struct.pack(">Q", len(value)) + value)
    elif isinstance(value, list):
        _encode_sequence(_LIST, value, writer, depth=depth, nodes=nodes)
    elif isinstance(value, tuple):
        _encode_sequence(_TUPLE, value, writer, depth=depth, nodes=nodes)
    elif isinstance(value, Mapping):
        _encode_mapping(value, writer, depth=depth, nodes=nodes)
    elif isinstance(value, torch.Tensor):
        _encode_tensor(value, writer)
    else:
        raise TorchOwnerStateError(
            f"owner state contains unsupported {type(value).__name__} value"
        )


def _encode_sequence(
    tag: int,
    values: list[object] | tuple[object, ...],
    writer: _Writer,
    *,
    depth: int,
    nodes: list[int],
) -> None:
    if len(values) > _MAX_CONTAINER_ITEMS:
        raise TorchOwnerStateError("owner state container has too many items")
    writer.append(bytes([tag]) + struct.pack(">I", len(values)))
    for value in values:
        _encode_value(value, writer, depth=depth + 1, nodes=nodes)


def _encode_mapping(
    values: Mapping[object, object],
    writer: _Writer,
    *,
    depth: int,
    nodes: list[int],
) -> None:
    if len(values) > _MAX_CONTAINER_ITEMS:
        raise TorchOwnerStateError("owner state mapping has too many items")
    keyed = []
    for key, value in values.items():
        if type(key) not in (int, str):
            raise TorchOwnerStateError("owner state mapping keys must be int or str")
        keyed.append((_canonical_key(key), key, value))
    keyed.sort(key=lambda row: row[0])
    writer.append(bytes([_MAPPING]) + struct.pack(">I", len(keyed)))
    for _, key, value in keyed:
        _encode_value(key, writer, depth=depth + 1, nodes=nodes)
        _encode_value(value, writer, depth=depth + 1, nodes=nodes)


def _canonical_key(key: int | str) -> bytes:
    if type(key) is int:
        if -(1 << 63) <= key < (1 << 63):
            return bytes([_INTEGER]) + struct.pack(">q", key)
        if 0 <= key < (1 << 64):
            return bytes([_UNSIGNED_INTEGER]) + struct.pack(">Q", key)
        raise TorchOwnerStateError("owner state integer key is outside 64-bit range")
    encoded = key.encode("utf-8")
    if len(encoded) > _MAX_STRING_BYTES:
        raise TorchOwnerStateError("owner state string key is too large")
    return bytes([_STRING]) + struct.pack(">I", len(encoded)) + encoded


def _encode_tensor(value: torch.Tensor, writer: _Writer) -> None:
    if value.layout is not torch.strided or value.is_quantized:
        raise TorchOwnerStateError("owner state supports only dense tensors")
    try:
        dtype_id, numpy_dtype = _DTYPES[value.dtype]
    except KeyError as error:
        raise TorchOwnerStateError(
            f"unsupported owner-state tensor dtype {value.dtype}"
        ) from error
    shape = tuple(value.shape)
    if len(shape) > _MAX_DIMENSIONS:
        raise TorchOwnerStateError("owner-state tensor has too many dimensions")
    array = value.detach().cpu().contiguous().numpy().astype(numpy_dtype, copy=False)
    data = array.tobytes(order="C")
    writer.append(bytes([_TENSOR, dtype_id]) + struct.pack(">I", len(shape)))
    for dimension in shape:
        writer.append(struct.pack(">Q", dimension))
    writer.append(struct.pack(">Q", len(data)))
    writer.append(data)


def _decode_value(reader: _Reader, *, depth: int) -> object:
    reader.node(depth)
    tag = reader.read(1)[0]
    if tag == _NONE:
        return None
    if tag == _FALSE:
        return False
    if tag == _TRUE:
        return True
    if tag == _INTEGER:
        return struct.unpack(">q", reader.read(8))[0]
    if tag == _UNSIGNED_INTEGER:
        return struct.unpack(">Q", reader.read(8))[0]
    if tag == _FLOAT:
        value = struct.unpack(">d", reader.read(8))[0]
        if not math.isfinite(value):
            raise TorchOwnerStateError("owner state float must be finite")
        return value
    if tag == _STRING:
        length = struct.unpack(">I", reader.read(4))[0]
        if length > _MAX_STRING_BYTES:
            raise TorchOwnerStateError("owner state string is too large")
        try:
            return bytes(reader.read(length)).decode("utf-8")
        except UnicodeDecodeError as error:
            raise TorchOwnerStateError("owner state string is not UTF-8") from error
    if tag == _BYTES:
        length = struct.unpack(">Q", reader.read(8))[0]
        return bytes(reader.read(length))
    if tag in (_LIST, _TUPLE):
        count = _container_count(reader)
        values = tuple(_decode_value(reader, depth=depth + 1) for _ in range(count))
        return list(values) if tag == _LIST else values
    if tag == _MAPPING:
        count = _container_count(reader)
        mapping: dict[int | str, object] = {}
        for _ in range(count):
            key = _decode_value(reader, depth=depth + 1)
            if type(key) not in (int, str):
                raise TorchOwnerStateError("owner state mapping key is not int or str")
            if key in mapping:
                raise TorchOwnerStateError("owner state mapping repeats a key")
            mapping[key] = _decode_value(reader, depth=depth + 1)
        return mapping
    if tag == _TENSOR:
        return _decode_tensor(reader)
    raise TorchOwnerStateError("owner state value tag is unknown")


def _container_count(reader: _Reader) -> int:
    count = struct.unpack(">I", reader.read(4))[0]
    if count > _MAX_CONTAINER_ITEMS:
        raise TorchOwnerStateError("owner state container has too many items")
    return count


def _decode_tensor(reader: _Reader) -> torch.Tensor:
    dtype_id = reader.read(1)[0]
    dimensions = struct.unpack(">I", reader.read(4))[0]
    if dimensions > _MAX_DIMENSIONS:
        raise TorchOwnerStateError("owner-state tensor has too many dimensions")
    try:
        torch_dtype, numpy_dtype = _DTYPES_BY_ID[dtype_id]
    except KeyError as error:
        raise TorchOwnerStateError("owner-state tensor dtype is unknown") from error
    shape = tuple(
        struct.unpack(">Q", reader.read(8))[0] for _ in range(dimensions)
    )
    data_length = struct.unpack(">Q", reader.read(8))[0]
    expected_length = math.prod(shape) * numpy_dtype.itemsize
    if data_length != expected_length:
        raise TorchOwnerStateError("owner-state tensor byte length is inconsistent")
    raw = reader.read(data_length)
    native_dtype = numpy_dtype.newbyteorder("=")
    array = np.frombuffer(raw, dtype=numpy_dtype).astype(native_dtype, copy=True)
    return torch.from_numpy(array.reshape(shape)).to(torch_dtype)


def _encode_node(depth: int, nodes: list[int]) -> None:
    if depth > _MAX_DEPTH:
        raise TorchOwnerStateError("owner state exceeds its nesting limit")
    nodes[0] += 1
    if nodes[0] > _MAX_NODES:
        raise TorchOwnerStateError("owner state has too many nodes")


def _positive_integer(value: object, name: str) -> int:
    if isinstance(value, bool):
        raise TorchOwnerStateError(f"{name} must be an integer")
    try:
        normalized = operator.index(value)
    except TypeError as error:
        raise TorchOwnerStateError(f"{name} must be an integer") from error
    if normalized <= 0:
        raise TorchOwnerStateError(f"{name} must be positive")
    return normalized
