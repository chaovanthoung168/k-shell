use std::ops::Range;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Quote {
    Unquoted,
    Single,
    Double,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WordPart {
    pub text: String,
    pub quote: Quote,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Word {
    pub parts: Vec<WordPart>,
    pub span: Range<usize>,
}

impl Word {
    // Expansion will consume quote-aware parts here in a later milestone.
    pub fn literal(&self) -> String {
        self.parts.iter().map(|part| part.text.as_str()).collect()
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct Command {
    pub words: Vec<Word>,
}
