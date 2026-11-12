use std::iter::Peekable;
use std::ops::Range;
use std::str::CharIndices;

use super::ast::{Quote, Word, WordPart};
use crate::error::ShellError;

#[derive(Debug, PartialEq, Eq)]
pub enum TokenKind {
    Word(Word),
    Operator(String),
}

#[derive(Debug, PartialEq, Eq)]
pub struct Token {
    pub kind: TokenKind,
    pub span: Range<usize>,
}

fn is_operator(c: char) -> bool {
    matches!(c, '|' | '&' | ';' | '<' | '>' | '\n' | '\r')
}

fn quoted(
    chars: &mut Peekable<CharIndices<'_>>,
    quote: char,
    start: usize,
    input_len: usize,
    incomplete: bool,
) -> Result<WordPart, ShellError> {
    let mut text = String::new();
    for (_, c) in chars.by_ref() {
        if c == quote {
            return Ok(WordPart {
                text,
                quote: if quote == '\'' {
                    Quote::Single
                } else {
                    Quote::Double
                },
            });
        }
        // Backslashes are literal, including before a closing quote. This keeps
        // drive and UNC paths intact; adjacent opposite-quoted parts quote quotes.
        text.push(c);
    }
    if incomplete {
        return Ok(WordPart {
            text,
            quote: if quote == '\'' {
                Quote::Single
            } else {
                Quote::Double
            },
        });
    }
    Err(ShellError::syntax("unterminated quote", start..input_len))
}

pub fn lex(input: &str) -> Result<Vec<Token>, ShellError> {
    lex_with_mode(input, false)
}

/// Completion accepts unfinished quotes while execution always uses strict lexing.
pub fn lex_incomplete(input: &str) -> Vec<Token> {
    lex_with_mode(input, true).unwrap_or_default()
}

fn lex_with_mode(input: &str, incomplete: bool) -> Result<Vec<Token>, ShellError> {
    let mut chars = input.char_indices().peekable();
    let mut tokens = Vec::new();
    while let Some(&(start, c)) = chars.peek() {
        if c.is_whitespace() && c != '\n' && c != '\r' {
            chars.next();
            continue;
        }
        if is_operator(c) {
            chars.next();
            let mut operator = c.to_string();
            if matches!(c, '|' | '&' | '>') && chars.peek().is_some_and(|&(_, n)| n == c) {
                chars.next();
                operator.push(c);
            }
            let end = chars.peek().map_or(input.len(), |&(i, _)| i);
            tokens.push(Token {
                kind: TokenKind::Operator(operator),
                span: start..end,
            });
            continue;
        }
        let mut parts = Vec::new();
        while let Some(&(index, c)) = chars.peek() {
            if c.is_whitespace() || is_operator(c) {
                break;
            }
            if matches!(c, '\'' | '"') {
                chars.next();
                parts.push(quoted(&mut chars, c, index, input.len(), incomplete)?);
            } else {
                let mut text = String::new();
                while let Some(&(_, c)) = chars.peek() {
                    if c.is_whitespace() || is_operator(c) || matches!(c, '\'' | '"') {
                        break;
                    }
                    chars.next();
                    text.push(c);
                }
                parts.push(WordPart {
                    text,
                    quote: Quote::Unquoted,
                });
            }
        }
        let end = chars.peek().map_or(input.len(), |&(i, _)| i);
        let span = start..end;
        tokens.push(Token {
            kind: TokenKind::Word(Word {
                parts,
                span: span.clone(),
            }),
            span,
        });
    }
    Ok(tokens)
}
