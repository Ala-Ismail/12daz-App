"""Bounded streaming input. Archive members are never extracted to disk."""
from contextlib import contextmanager
from dataclasses import dataclass
import codecs
import hashlib
import re
import zipfile

from .errors import ConversionError, checkpoint


@dataclass(frozen=True)
class Limits:
    expanded_bytes: int = 1024 * 1024 * 1024
    members: int = 100
    token_length: int = 1024 * 1024
    depth: int = 64
    entities: int = 1_000_000
    entity_tokens: int = 2_000_000


def input_hash(path, cancel=None):
    digest = hashlib.sha256()
    with open(path, "rb") as stream:
        while chunk := stream.read(1024 * 1024):
            checkpoint(cancel)
            digest.update(chunk)
    return digest.hexdigest()


class DecodedInput:
    def __init__(self, stream, limits, encoding, cancel, progress):
        self.stream, self.limits = stream, limits
        self.cancel, self.progress = cancel, progress
        self.bytes_read = 0
        self.prefix = self._read(8192)
        if self.prefix.startswith((b"\xff\xfe", b"\xfe\xff")):
            self.encoding = "utf-16"
        elif self.prefix.startswith(codecs.BOM_UTF8):
            self.encoding = "utf-8-sig"
        else:
            self.encoding = encoding or "utf-8"
        try:
            codecs.lookup(self.encoding)
        except LookupError as exc:
            raise ConversionError(f"Unknown encoding: {self.encoding}") from exc
        self.metadata = {}

    def _read(self, size=65536):
        checkpoint(self.cancel)
        data = self.stream.read(size)
        self.bytes_read += len(data)
        if self.bytes_read > self.limits.expanded_bytes:
            raise ConversionError("Expanded input exceeds configured byte limit.")
        if self.progress:
            self.progress("Reading archive", self.bytes_read)
        return data

    def chunks(self):
        decoder = codecs.getincrementaldecoder(self.encoding)(errors="strict")
        try:
            first = decoder.decode(self.prefix)
            version = re.search(r'//\s*archive_version\s+"([^"]+)"', first, re.I)
            self.metadata = {"encoding": self.encoding, "archive_version": version[1] if version else None}
            yield first
            while chunk := self._read():
                yield decoder.decode(chunk)
            yield decoder.decode(b"", final=True)
        except UnicodeError as exc:
            raise ConversionError(f"Cannot decode input as {self.encoding}: {exc}") from exc


@contextmanager
def open_input(path, *, limits=Limits(), encoding=None, cancel=None, progress=None):
    try:
        with open(path, "rb") as raw:
            signature = raw.read(4)
            raw.seek(0)
            if signature.startswith(b"PK"):
                with zipfile.ZipFile(raw) as archive:
                    entries = archive.infolist()
                    if len(entries) > limits.members:
                        raise ConversionError("Archive member limit exceeded.")
                    if any(e.flag_bits & 1 for e in entries):
                        raise ConversionError("Encrypted archives are not supported.")
                    candidates = [e for e in entries if not e.is_dir() and e.filename.lower().endswith(".12da")]
                    if len(candidates) != 1:
                        raise ConversionError("Archive must contain exactly one .12da payload.")
                    if candidates[0].file_size > limits.expanded_bytes:
                        raise ConversionError("Expanded input exceeds configured byte limit.")
                    with archive.open(candidates[0]) as member:
                        decoded = DecodedInput(member, limits, encoding, cancel, progress)
                        decoded.extra_members = len(entries) - 1
                        yield decoded
            else:
                if signature.startswith((b"\x1f\x8b", b"7z", b"%PDF", b"Rar!")):
                    raise ConversionError("Unsupported container; expected ZIP or plain 12da text.")
                decoded = DecodedInput(raw, limits, encoding, cancel, progress)
                decoded.extra_members = 0
                yield decoded
    except ConversionError:
        raise
    except (OSError, zipfile.BadZipFile, RuntimeError, EOFError, NotImplementedError) as exc:
        raise ConversionError(f"Cannot read input: {exc}") from exc
