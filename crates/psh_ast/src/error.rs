use psh_lexer::{Span, Token};
use std::fmt;

#[derive(Debug, Clone, PartialEq)]
pub enum ParseError {
    /// Got a token we didn't expect at all
    Unexpected { got: Token, span: Span, msg: &'static str },
    /// Expected a specific token, got something else
    Expected { expected: &'static str, got: Token, span: Span },
    /// Reached EOF before the construct was complete
    UnexpectedEof { span: Span, msg: &'static str },
    /// A well-formed but semantically invalid construct
    Invalid { span: Span, msg: &'static str },
}

impl ParseError {
    pub fn span(&self) -> Span {
        match self {
            ParseError::Unexpected     { span, .. } => *span,
            ParseError::Expected       { span, .. } => *span,
            ParseError::UnexpectedEof  { span, .. } => *span,
            ParseError::Invalid        { span, .. } => *span,
        }
    }
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ParseError::Unexpected { got, msg, .. } =>
                write!(f, "{msg}, got `{got}`"),
            ParseError::Expected { expected, got, .. } =>
                write!(f, "expected {expected}, got `{got}`"),
            ParseError::UnexpectedEof { msg, .. } =>
                write!(f, "unexpected end of file: {msg}"),
            ParseError::Invalid { msg, .. } =>
                write!(f, "{msg}"),
        }
    }
}

impl std::error::Error for ParseError {}
