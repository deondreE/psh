use crate::{Span};
use std::fmt;

#[derive(Debug, Clone, PartialEq)]
pub enum Token {
    Int(i64),
    Float(f64),
    Str(String),
    FStrStart,
    FStrText(String),
    FStrOpen,
    FStrClose,
    FStrEnd,
    Ident(String),
    And,
    Catch,
    Else,
    Exec,
    Exit,
    False,
    Fn,
    For,
    If,
    Import,
    In,
    Let,
    Mut,
    Not,
    Or,
    Return,
    Task,
    True,
    Try,
    While,

    EqEq, // ==
    BangEq, // !=
    Lt, // <
    LtEq, // <=
    Gt, // >
    GtEq, // >=
    Eq, // =
    Plus, // +
    Minus, // -
    Star, // *
    Percent, // %
    Arrow, // ->
    DotDot, // ..
    Slash, // /
    Dot, // .
    Question, // ?

    Comma,
    Colon,
    Semi,
    LBrace,
    RBrace,
    LParen,
    RParen,
    LBracket,
    RBracket,

    ExecWord(String),
    ExecOpen,
    ExecClose,
    ExecEnd,

    Newline,
    Eof,
}

impl fmt::Display for Token {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Token::Int(n)        => write!(f, "{n}"),
            Token::Float(n)      => write!(f, "{n}"),
            Token::Str(s)        => write!(f, "\"{s}\""),
            Token::FStrStart     => write!(f, "f\""),
            Token::FStrText(t)   => write!(f, "{t}"),
            Token::FStrOpen      => write!(f, "{{"),
            Token::FStrClose     => write!(f, "}}"),
            Token::FStrEnd       => write!(f, "\""),
            Token::Ident(s)      => write!(f, "{s}"),
            Token::And           => write!(f, "and"),
            Token::Catch         => write!(f, "catch"),
            Token::Else          => write!(f, "else"),
            Token::Exec          => write!(f, "exec"),
            Token::Exit          => write!(f, "exit"),
            Token::False         => write!(f, "false"),
            Token::Fn            => write!(f, "fn"),
            Token::For           => write!(f, "for"),
            Token::If            => write!(f, "if"),
            Token::Import        => write!(f, "import"),
            Token::In            => write!(f, "in"),
            Token::Let           => write!(f, "let"),
            Token::Mut           => write!(f, "mut"),
            Token::Not           => write!(f, "not"),
            Token::Or            => write!(f, "or"),
            Token::Return        => write!(f, "return"),
            Token::Task          => write!(f, "task"),
            Token::True          => write!(f, "true"),
            Token::Try           => write!(f, "try"),
            Token::While         => write!(f, "while"),
            Token::EqEq          => write!(f, "=="),
            Token::BangEq        => write!(f, "!="),
            Token::Lt            => write!(f, "<"),
            Token::LtEq          => write!(f, "<="),
            Token::Gt            => write!(f, ">"),
            Token::GtEq          => write!(f, ">="),
            Token::Eq            => write!(f, "="),
            Token::Plus          => write!(f, "+"),
            Token::Minus         => write!(f, "-"),
            Token::Star          => write!(f, "*"),
            Token::Slash         => write!(f, "/"),
            Token::Percent       => write!(f, "%"),
            Token::Arrow         => write!(f, "->"),
            Token::DotDot        => write!(f, ".."),
            Token::Dot           => write!(f, "."),
            Token::Question      => write!(f, "?"),
            Token::Comma         => write!(f, ","),
            Token::Colon         => write!(f, ":"),
            Token::Semi          => write!(f, ";"),
            Token::LBrace        => write!(f, "{{"),
            Token::RBrace        => write!(f, "}}"),
            Token::LParen        => write!(f, "("),
            Token::RParen        => write!(f, ")"),
            Token::LBracket      => write!(f, "["),
            Token::RBracket      => write!(f, "]"),
            Token::ExecWord(w)   => write!(f, "{w}"),
            Token::ExecOpen      => write!(f, "{{"),
            Token::ExecClose     => write!(f, "}}"),
            Token::ExecEnd       => write!(f, "<exec-end>"),
            Token::Newline       => write!(f, "<newline>"),
            Token::Eof           => write!(f, "<eof>"),
        }
    }
}

impl Token {
    /// Map bare word to its keyword token, or wrap as ident.
    pub fn from_word(w: &str) -> Token {
            match w {
                "and"    => Token::And,
                "catch"  => Token::Catch,
                "else"   => Token::Else,
                "exec"   => Token::Exec,
                "exit"   => Token::Exit,
                "false"  => Token::False,
                "fn"     => Token::Fn,
                "for"    => Token::For,
                "if"     => Token::If,
                "import" => Token::Import,
                "in"     => Token::In,
                "let"    => Token::Let,
                "mut"    => Token::Mut,
                "not"    => Token::Not,
                "or"     => Token::Or,
                "return" => Token::Return,
                "task"   => Token::Task,
                "true"   => Token::True,
                "try"    => Token::Try,
                "while"  => Token::While,
                other    => Token::Ident(other.to_string()),
            }
        }
}

/// A token together with its source location
#[derive(Debug, Clone, PartialEq)]
pub struct Spanned {
    pub token: Token,
    pub span: Span,
}

impl Spanned {
    #[inline]
    pub fn new(token: Token, span: Span) -> Self {
        Self { token, span }
    }
}
