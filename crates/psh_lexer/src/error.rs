use crate::Span;
use std::fmt;

#[derive(Debug, Clone, PartialEq)]
pub enum LexerError {
    UnexpectedChar { ch: char, span: Span },
    UnterminatedString { span: Span },
    UnterminatedBlock { span: Span },
    InvalidEscape { ch: char, span: Span },
    InvalidNumber { span: Span },
}

impl fmt::Display for LexerError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LexerError::UnexpectedChar { ch, span } =>
                write!(f, "unexpected character '{}' at {}", ch, span),
            LexerError::UnterminatedString { span } =>
                write!(f, "unterminated string starting at {}", span),
            LexerError::UnterminatedBlock { span } =>
                write!(f, "unterminated block comment starting at {}", span),
            LexerError::InvalidEscape { ch, span } =>
                write!(f, "invalid escape sequence '\\{}' at {}", ch, span),
            LexerError::InvalidNumber { span } =>
                write!(f, "invalid number literal at {}", span),
        }
    }
}

impl std::error::Error for LexerError {}
