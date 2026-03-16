
use std::fmt;
use psh_lexer::Span;

#[derive(Debug, Clone)]
pub enum CompileError {
    /// Tried to assign to an immutable variable
    ImmutableAssign { name: String, span: Span },
    /// Referenced a variable that hasn't been declared
    UndeclaredVar { name: String, span: Span },
    /// `return` outside a function
    ReturnOutsideFunction { span: Span },
    /// Too many constants or locals (VM limits)
    Overflow { msg: &'static str, span: Span },
    /// Generic compile-time error
    Invalid { msg: String, span:  Span },
}

impl CompileError {
    pub fn span(&self) -> Span {
        match self {
            CompileError::ImmutableAssign   { span, .. } => *span,
            CompileError::UndeclaredVar     { span, .. } => *span,
            CompileError::ReturnOutsideFunction { span } => *span,
            CompileError::Overflow          { span, .. } => *span,
            CompileError::Invalid           { span, .. } => *span,
        }
    }
}

impl fmt::Display for CompileError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CompileError::ImmutableAssign { name, .. } =>
                write!(f, "cannot assign to immutable variable `{name}`"),
            CompileError::UndeclaredVar { name, .. } =>
                write!(f, "use of undeclared variable `{name}`"),
            CompileError::ReturnOutsideFunction { .. } =>
                write!(f, "`return` outside of a function"),
            CompileError::Overflow { msg, .. } =>
                write!(f, "compiler limit exceeded: {msg}"),
            CompileError::Invalid { msg, .. } =>
                write!(f, "{msg}"),
        }
    }
}

impl std::error::Error for CompileError {}
