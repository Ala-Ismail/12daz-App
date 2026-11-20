//! Streaming tokens and balanced archive blocks, with one retained object at a time.

use std::collections::HashMap;
use std::sync::atomic::AtomicBool;

use crate::archive::Limits;
use crate::error::{ConversionError, Result, checkpoint};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Token {
    pub value: String,
    pub line: usize,
    pub quoted: bool,
}

impl Token {
    fn is(&self, value: &str) -> bool {
        !self.quoted && self.value == value
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Item {
    Token(Token),
    Block(Block),
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Block {
    pub items: Vec<Item>,
}

#[derive(Debug, PartialEq, Eq)]
pub struct SourceEntity {
    pub ordinal: usize,
    pub kind: String,
    pub model: String,
    pub line: usize,
    pub block: Block,
    pub defaults: HashMap<String, String>,
}

pub struct Lexer<'a, I> {
    chunks: I,
    chunk: String,
    offset: usize,
    pending: Option<char>,
    line: usize,
    token_limit: usize,
    cancel: Option<&'a AtomicBool>,
    done: bool,
}

pub fn tokenize<I>(chunks: I, limits: Limits, cancel: Option<&AtomicBool>) -> Lexer<'_, I::IntoIter>
where
    I: IntoIterator<Item = Result<String>>,
{
    Lexer {
        chunks: chunks.into_iter(),
        chunk: String::new(),
        offset: 0,
        pending: None,
        line: 1,
        token_limit: limits.token_length,
        cancel,
        done: false,
    }
}

fn whitespace(character: char) -> bool {
    character.is_whitespace() || ('\u{1c}'..='\u{1f}').contains(&character)
}

impl<I: Iterator<Item = Result<String>>> Lexer<'_, I> {
    fn peek_char(&mut self) -> Result<Option<char>> {
        if self.pending.is_some() {
            return Ok(self.pending);
        }
        while self.offset == self.chunk.len() {
            checkpoint(self.cancel)?;
            let Some(chunk) = self.chunks.next() else {
                return Ok(None);
            };
            self.chunk = chunk?;
            self.offset = 0;
        }
        self.pending = self.chunk[self.offset..].chars().next();
        self.offset += self.pending.expect("nonempty chunk suffix").len_utf8();
        Ok(self.pending)
    }

    fn pop_char(&mut self) -> Result<Option<char>> {
        self.peek_char()?;
        let character = self.pending.take();
        if character == Some('\n') {
            self.line += 1;
        }
        Ok(character)
    }

    fn check_length(&self, length: usize, line: usize) -> Result<()> {
        if length > self.token_limit {
            return Err(ConversionError::message(format!(
                "Token length limit exceeded at line {line}."
            )));
        }
        Ok(())
    }

    fn quoted(&mut self, line: usize) -> Result<Token> {
        let mut value = String::new();
        let mut length = 1;
        let unterminated = || {
            ConversionError::message(format!(
                "Unterminated string or invalid token at line {line}."
            ))
        };
        loop {
            let character = self.pop_char()?.ok_or_else(unterminated)?;
            length += 1;
            self.check_length(length, line)?;
            match character {
                '"' => {
                    return Ok(Token {
                        value,
                        line,
                        quoted: true,
                    });
                }
                '\\' => {
                    let escaped = self.pop_char()?.ok_or_else(unterminated)?;
                    length += 1;
                    self.check_length(length, line)?;
                    if !matches!(escaped, '"' | '\\') {
                        value.push('\\');
                    }
                    value.push(escaped);
                }
                _ => value.push(character),
            }
        }
    }

    fn skip_comment(&mut self) -> Result<()> {
        while self
            .peek_char()?
            .is_some_and(|character| !matches!(character, '\n' | '\r'))
        {
            self.pop_char()?;
        }
        Ok(())
    }

    fn next_token(&mut self) -> Result<Option<Token>> {
        checkpoint(self.cancel)?;
        loop {
            let line = self.line;
            let Some(first) = self.pop_char()? else {
                return Ok(None);
            };
            if whitespace(first) {
                continue;
            }
            if first == '/' && self.peek_char()? == Some('/') {
                self.skip_comment()?;
                continue;
            }
            if first == '"' {
                return self.quoted(line).map(Some);
            }
            let mut value = String::from(first);
            let mut length = 1;
            self.check_length(length, line)?;
            if !matches!(first, '{' | '}') {
                while let Some(character) = self.peek_char()? {
                    if whitespace(character) || matches!(character, '{' | '}' | '"') {
                        break;
                    }
                    self.pop_char()?;
                    if character == '/' && self.peek_char()? == Some('/') {
                        self.skip_comment()?;
                        break;
                    }
                    value.push(character);
                    length += 1;
                    self.check_length(length, line)?;
                }
            }
            return Ok(Some(Token {
                value,
                line,
                quoted: false,
            }));
        }
    }
}

impl<I: Iterator<Item = Result<String>>> Iterator for Lexer<'_, I> {
    type Item = Result<Token>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.done {
            return None;
        }
        match self.next_token() {
            Ok(Some(token)) => Some(Ok(token)),
            Ok(None) => {
                self.done = true;
                None
            }
            Err(error) => {
                self.done = true;
                Some(Err(error))
            }
        }
    }
}

struct TokenStream<I: Iterator<Item = Result<Token>>> {
    tokens: std::iter::Peekable<I>,
}

impl<I: Iterator<Item = Result<Token>>> TokenStream<I> {
    fn peek(&mut self) -> bool {
        self.tokens.peek().is_some()
    }

    fn pop(&mut self) -> Result<Token> {
        self.tokens
            .next()
            .ok_or_else(|| ConversionError::message("Unexpected end of archive."))?
    }
}

fn read_block(
    stream: &mut TokenStream<impl Iterator<Item = Result<Token>>>,
    limits: Limits,
    depth: usize,
    discard: bool,
    budget: &mut usize,
) -> Result<Block> {
    if depth > limits.depth {
        return Err(ConversionError::message("Nesting depth limit exceeded."));
    }
    let mut block = Block::default();
    loop {
        let token = stream.pop()?;
        *budget += 1;
        if *budget > limits.entity_tokens {
            return Err(ConversionError::message("Per-object token limit exceeded."));
        }
        if token.is("}") {
            return Ok(block);
        }
        if token.is("{") {
            let discard_child = discard
                || matches!(block.items.last(), Some(Item::Token(previous))
                if matches!(previous.value.to_ascii_lowercase().as_str(), "attributes" | "drawables" | "computator"));
            let child = read_block(stream, limits, depth + 1, discard_child, budget)?;
            if !discard {
                block.items.push(Item::Block(child));
            }
        } else if !discard {
            block.items.push(Item::Token(token));
        }
    }
}

pub type Fields<'a> = HashMap<String, &'a Item>;

pub fn fields(block: &Block) -> Result<Fields<'_>> {
    if !block.items.len().is_multiple_of(2) {
        return Err(ConversionError::message(
            "Invalid command/value structure in object.",
        ));
    }
    let mut result = HashMap::new();
    for pair in block.items.chunks_exact(2) {
        let Item::Token(key) = &pair[0] else {
            return Err(ConversionError::message("Missing command before block."));
        };
        let name = key.value.to_lowercase();
        if result.insert(name.clone(), &pair[1]).is_some() {
            return Err(ConversionError::message(format!(
                "Repeated command '{name}' at line {}.",
                key.line
            )));
        }
    }
    Ok(result)
}

pub fn scalar<'a>(
    mapping: &Fields<'a>,
    name: &str,
    default: Option<&'a str>,
) -> Result<Option<&'a str>> {
    match mapping.get(name) {
        None => Ok(default),
        Some(Item::Token(value)) => Ok(Some(&value.value)),
        Some(Item::Block(_)) => Err(ConversionError::message(format!(
            "Expected scalar for {name}."
        ))),
    }
}

pub fn block_values(value: Option<&Item>) -> Result<Vec<&str>> {
    let invalid = || ConversionError::message("Expected flat data block.");
    let Some(Item::Block(block)) = value else {
        return Err(invalid());
    };
    block
        .items
        .iter()
        .map(|item| match item {
            Item::Token(token) => Ok(token.value.as_str()),
            Item::Block(_) => Err(invalid()),
        })
        .collect()
}

pub struct EntityStream<'a, I: Iterator<Item = Result<String>>> {
    stream: TokenStream<Lexer<'a, I>>,
    defaults: HashMap<String, String>,
    limits: Limits,
    cancel: Option<&'a AtomicBool>,
    count: usize,
}

pub fn iter_entities<I>(
    chunks: I,
    limits: Limits,
    cancel: Option<&AtomicBool>,
) -> EntityStream<'_, I::IntoIter>
where
    I: IntoIterator<Item = Result<String>>,
{
    let defaults = [
        ("model", "data"),
        ("colour", "red"),
        ("style", "1"),
        ("breakline", "point"),
        ("null", "-999"),
    ];
    EntityStream {
        stream: TokenStream {
            tokens: tokenize(chunks, limits, cancel).peekable(),
        },
        defaults: defaults
            .into_iter()
            .map(|(key, value)| (key.into(), value.into()))
            .collect(),
        limits,
        cancel,
        count: 0,
    }
}

impl<I: Iterator<Item = Result<String>>> EntityStream<'_, I> {
    fn entity(
        &mut self,
        kind: String,
        model: String,
        line: usize,
        block: Block,
    ) -> Result<SourceEntity> {
        self.count += 1;
        if self.count > self.limits.entities {
            return Err(ConversionError::message("Entity limit exceeded."));
        }
        Ok(SourceEntity {
            ordinal: self.count,
            kind,
            model,
            line,
            block,
            defaults: self.defaults.clone(),
        })
    }

    pub fn next_entity(
        &mut self,
        on_command: &mut dyn FnMut(&str, &str, usize),
    ) -> Result<Option<SourceEntity>> {
        while self.stream.peek() {
            checkpoint(self.cancel)?;
            let command = self.stream.pop()?;
            let key = command.value.to_lowercase();
            let value = self.stream.pop()?;
            if command.is("{") || command.is("}") {
                return Err(ConversionError::message(format!(
                    "Unexpected brace at line {}.",
                    command.line
                )));
            }
            if key == "string" {
                let kind = value.value.to_lowercase();
                let opening = self.stream.pop()?;
                if !opening.is("{") {
                    return Err(ConversionError::message(format!(
                        "Expected object block at line {}.",
                        opening.line
                    )));
                }
                let body = read_block(&mut self.stream, self.limits, 1, false, &mut 0)?;
                let default_model = self.defaults["model"].as_str();
                let model = if matches!(
                    kind.as_str(),
                    "super"
                        | "super_alignment"
                        | "2d"
                        | "3d"
                        | "polyline"
                        | "arc"
                        | "circle"
                        | "text"
                ) {
                    scalar(&fields(&body)?, "model", Some(default_model))?.unwrap_or(default_model)
                } else {
                    body.items
                        .windows(2)
                        .filter_map(|pair| match pair {
                            [Item::Token(key), Item::Token(value)]
                                if key.value.eq_ignore_ascii_case("model") =>
                            {
                                Some(value.value.as_str())
                            }
                            _ => None,
                        })
                        .next_back()
                        .unwrap_or(default_model)
                };
                let model = model.trim().to_owned();
                if model.is_empty() {
                    return Err(ConversionError::message("Model name cannot be empty."));
                }
                return self.entity(kind, model, command.line, body).map(Some);
            }
            if value.is("{") {
                let body = read_block(&mut self.stream, self.limits, 1, key != "model", &mut 0)?;
                if key != "model" {
                    return self
                        .entity(
                            key,
                            self.defaults["model"].clone(),
                            command.line,
                            Block::default(),
                        )
                        .map(Some);
                }
                let model = scalar(&fields(&body)?, "name", None)?
                    .unwrap_or_default()
                    .trim();
                if model.is_empty() {
                    return Err(ConversionError::message("Model name cannot be empty."));
                }
                self.defaults.insert(key, model.to_owned());
            } else if let Some(default) = self.defaults.get_mut(&key) {
                *default = value.value;
            } else {
                if value.is("}") {
                    return Err(ConversionError::message(format!(
                        "Invalid command at line {}.",
                        command.line
                    )));
                }
                on_command(&key, &value.value, command.line);
            }
        }
        Ok(None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn collect_entities(text: &str, limits: Limits) -> Result<Vec<SourceEntity>> {
        let mut parser = iter_entities([Ok(text.into())], limits, None);
        let mut entities = Vec::new();
        while let Some(entity) = parser.next_entity(&mut |_, _, _| {})? {
            entities.push(entity);
        }
        Ok(entities)
    }

    #[test]
    fn rust_05_tokens_do_not_depend_on_chunk_boundaries() {
        let content = "model \"Café { \\\"\"// trailing comment\r\nstring 3d{ data { 1 2 3 } name \"a\\\\b\\n\" }";
        let expected: Vec<_> = [
            ("model", 1, false),
            ("Café { \"", 1, true),
            ("string", 2, false),
            ("3d", 2, false),
            ("{", 2, false),
            ("data", 2, false),
            ("{", 2, false),
            ("1", 2, false),
            ("2", 2, false),
            ("3", 2, false),
            ("}", 2, false),
            ("name", 2, false),
            ("a\\b\\n", 2, true),
            ("}", 2, false),
        ]
        .into_iter()
        .map(|(value, line, quoted)| Token {
            value: value.into(),
            line,
            quoted,
        })
        .collect();
        for split in content.char_indices().map(|(offset, _)| offset) {
            let chunks = [Ok(content[..split].into()), Ok(content[split..].into())];
            assert_eq!(
                tokenize(chunks, Limits::default(), None)
                    .collect::<Result<Vec<_>>>()
                    .unwrap(),
                expected
            );
        }
    }

    #[test]
    fn rust_05_preserves_model_defaults_and_unsupported_neighbours() {
        let text = "null null\nMoDeL { NAME \"Road\" attributes { text x y } }\ncolour green\n\
                    string 3D { data { 1 2 3 } }\nstring future { pit {} pit {} model Other }\n\
                    example { attributes { any { x } } }\nunknown \"setting\"\nstring circle { radius 10 }";
        let mut warnings = Vec::new();
        let mut parser = iter_entities(
            text.chars().map(|character| Ok(character.into())),
            Limits::default(),
            None,
        );
        let mut entities = Vec::new();
        while let Some(entity) = parser
            .next_entity(&mut |key, value, line| {
                warnings.push((key.to_owned(), value.to_owned(), line))
            })
            .unwrap()
        {
            entities.push(entity);
        }
        assert_eq!(warnings, [("unknown".into(), "setting".into(), 7)]);
        assert_eq!(
            entities
                .iter()
                .map(|e| (e.ordinal, e.kind.as_str(), e.model.as_str(), e.line))
                .collect::<Vec<_>>(),
            [
                (1, "3d", "Road", 4),
                (2, "future", "Other", 5),
                (3, "example", "Road", 6),
                (4, "circle", "Road", 8)
            ]
        );
        assert_eq!(entities[0].defaults["colour"], "green");
        assert_eq!(
            block_values(fields(&entities[0].block).unwrap().get("data").copied()).unwrap(),
            ["1", "2", "3"]
        );
        assert!(entities[2].block.items.is_empty());
    }

    #[test]
    fn rust_05_rejects_bad_syntax_and_crossing_each_limit() {
        let valid = "string 3d { data { 1 2 3 } }";
        for invalid in [
            "string 3d {",
            "model \"unfinished",
            "string 3d { data }",
            "string 3d { model A MODEL B }",
        ] {
            assert!(
                collect_entities(invalid, Limits::default()).is_err(),
                "{invalid}"
            );
        }
        let limits = Limits {
            depth: 2,
            token_length: 6,
            entities: 1,
            entity_tokens: 7,
            ..Limits::default()
        };
        assert_eq!(collect_entities(valid, limits).unwrap().len(), 1);
        for limited in [
            Limits { depth: 1, ..limits },
            Limits {
                token_length: 5,
                ..limits
            },
            Limits {
                entities: 0,
                ..limits
            },
            Limits {
                entity_tokens: 6,
                ..limits
            },
        ] {
            assert!(collect_entities(valid, limited).is_err());
        }
    }
}
