use super::ast::Command;
use super::lexer::{TokenKind, lex};
use crate::error::ShellError;

pub fn parse(input: &str) -> Result<Option<Command>, ShellError> {
    let tokens = lex(input)?;
    let mut words = Vec::new();
    for token in tokens {
        match token.kind {
            TokenKind::Word(word) => words.push(word),
            TokenKind::Operator(operator) => {
                return Err(ShellError::syntax(
                    format!("operator {operator:?} is not supported yet"),
                    token.span,
                ));
            }
        }
    }
    Ok(if words.is_empty() {
        None
    } else {
        Some(Command { words })
    })
}
