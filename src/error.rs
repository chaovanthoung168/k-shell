use std::fmt;
use std::ops::Range;

#[derive(Debug)]
pub struct ShellError {
    pub message: String,
    pub span: Option<Range<usize>>,
    pub status: i32,
}

impl ShellError {
    pub fn new(message: impl Into<String>, status: i32) -> Self {
        Self {
            message: message.into(),
            span: None,
            status,
        }
    }

    pub fn syntax(message: impl Into<String>, span: Range<usize>) -> Self {
        Self {
            message: message.into(),
            span: Some(span),
            status: 2,
        }
    }
}

impl fmt::Display for ShellError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.message)?;
        if let Some(span) = &self.span {
            write!(f, " (bytes {}..{})", span.start, span.end)?;
        }
        Ok(())
    }
}

impl std::error::Error for ShellError {}
