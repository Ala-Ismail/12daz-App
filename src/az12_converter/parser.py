"""Streaming tokenizer and balanced archive blocks; retain one object at a time."""
from dataclasses import dataclass, field
import re

from .archive import Limits
from .errors import ConversionError, checkpoint


@dataclass
class Token:
    value: str
    line: int
    quoted: bool = False


@dataclass
class Block:
    items: list = field(default_factory=list)


@dataclass
class SourceEntity:
    ordinal: int
    kind: str
    model: str
    line: int
    block: Block
    defaults: dict


# A trailing atom or comment is held until the next chunk, as it may continue.
LEXEME = re.compile(r'(?P<space>\s+)|(?P<comment>//[^\n\r]*)|(?P<quoted>"(?:\\.|[^"\\])*")|(?P<brace>[{}])|(?P<atom>(?:(?!//)[^\s{}"])+)', re.S)


def tokenize(chunks, limits=Limits(), cancel=None):
    buffer, line, eof = "", 1, False
    iterator = iter(chunks)
    while not eof or buffer:
        checkpoint(cancel)
        if not eof:
            try:
                buffer += next(iterator)
            except StopIteration:
                eof = True
        position = 0
        for match in LEXEME.finditer(buffer):
            if match.start() != position:
                break  # Incomplete quoted string; ask for another chunk.
            category, text = match.lastgroup, match.group()
            if match.end() == len(buffer) and not eof and category in {"atom", "comment"}:
                break
            if category not in {"space", "comment"}:
                if len(text) > limits.token_length:
                    raise ConversionError(f"Token length limit exceeded at line {line}.")
                if category == "quoted":
                    value = re.sub(r'\\(["\\])', r'\1', text[1:-1])
                    yield Token(value, line, True)
                else:
                    yield Token(text, line)
            line += text.count("\n")
            position = match.end()
        buffer = buffer[position:]
        if buffer and eof:
            raise ConversionError(f"Unterminated string or invalid token at line {line}.")
        if len(buffer) > limits.token_length:
            raise ConversionError(f"Token length limit exceeded at line {line}.")


class TokenStream:
    def __init__(self, tokens):
        self.tokens, self.pending = iter(tokens), None

    def peek(self):
        if self.pending is None:
            self.pending = next(self.tokens, None)
        return self.pending

    def pop(self):
        token = self.peek()
        self.pending = None
        if token is None:
            raise ConversionError("Unexpected end of archive.")
        return token


# These contain source metadata or independently rendered labels, not base plan geometry.
DISCARD_BLOCKS = {"attributes", "drawables", "horizontal_parts", "vertical_parts", "vertical_data", "computator", "geometry_modifiers"}


def read_block(stream, limits, depth=1, discard=False, budget=None):
    if depth > limits.depth:
        raise ConversionError("Nesting depth limit exceeded.")
    if budget is None:
        budget = [0]
    items = []
    while True:
        token = stream.pop()
        budget[0] += 1
        if budget[0] > limits.entity_tokens:
            raise ConversionError("Per-object token limit exceeded.")
        if not token.quoted and token.value == "}":
            return Block(items)
        if not token.quoted and token.value == "{":
            previous = items[-1].value.lower() if items and isinstance(items[-1], Token) else ""
            block = read_block(stream, limits, depth + 1, discard or previous in DISCARD_BLOCKS, budget)
            if not discard:
                items.append(block)
        elif not discard:
            items.append(token)


def fields(block):
    """Read command/value pairs without applying this to numeric data blocks."""
    result = {}
    items = block.items
    if len(items) % 2:
        raise ConversionError("Invalid command/value structure in object.")
    for i in range(0, len(items), 2):
        key, value = items[i:i + 2]
        if not isinstance(key, Token):
            raise ConversionError("Missing command before block.")
        key = key.value.lower()
        if key in result:
            raise ConversionError(f"Repeated command {key!r} at line {items[i].line}.")
        result[key] = value
    return result


def scalar(mapping, name, default=None):
    value = mapping.get(name)
    if value is None:
        return default
    if not isinstance(value, Token):
        raise ConversionError(f"Expected scalar for {name}.")
    return value.value


def block_values(value):
    if not isinstance(value, Block) or any(not isinstance(t, Token) for t in value.items):
        raise ConversionError("Expected flat data block.")
    return [t.value for t in value.items]


def iter_entities(chunks, limits=Limits(), cancel=None, on_command=None):
    stream = TokenStream(tokenize(chunks, limits, cancel))
    defaults = {"model": "data", "colour": "red", "style": "1", "breakline": "point", "null": "-999"}
    count = 0
    while stream.peek() is not None:
        checkpoint(cancel)
        command = stream.pop()
        key = command.value.lower()
        value = stream.pop()
        if not command.quoted and key in {"{", "}"}:
            raise ConversionError(f"Unexpected brace at line {command.line}.")
        if key == "string":
            kind = value.value.lower()
            opening = stream.pop()
            if opening.quoted or opening.value != "{":
                raise ConversionError(f"Expected object block at line {opening.line}.")
            body = read_block(stream, limits)
            # Unsupported civil objects may legally contain repeated blocks (e.g. pits).
            # Read only their optional model override; do not impose primitive fields.
            if kind in {"super", "super_alignment", "2d", "3d", "polyline", "arc", "circle", "text"}:
                props = fields(body)
            else:
                props = {}
                for i, item in enumerate(body.items[:-1]):
                    if isinstance(item, Token) and item.value.lower() == "model" and isinstance(body.items[i + 1], Token):
                        props["model"] = body.items[i + 1]
            count += 1
            if count > limits.entities:
                raise ConversionError("Entity limit exceeded.")
            model = scalar(props, "model", defaults["model"]).strip()
            if not model:
                raise ConversionError("Model name cannot be empty.")
            yield SourceEntity(count, kind, model, command.line, body, defaults.copy())
        elif not value.quoted and value.value == "{":
            body = read_block(stream, limits, discard=key not in {"model"})
            if key == "model":
                defaults["model"] = scalar(fields(body), "name", "").strip()
                if not defaults["model"]:
                    raise ConversionError("Model name cannot be empty.")
            else:
                count += 1
                if count > limits.entities:
                    raise ConversionError("Entity limit exceeded.")
                yield SourceEntity(count, key, defaults["model"], command.line, Block(), defaults.copy())
        elif key in defaults:
            defaults[key] = value.value
        else:
            if value.value in {"{", "}"} and not value.quoted:
                raise ConversionError(f"Invalid command at line {command.line}.")
            if on_command:
                on_command(key, value.value, command.line)
