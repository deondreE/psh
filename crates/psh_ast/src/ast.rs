use psh_lexer::Span;

/// A Complete parsed script: a sequence of statements.
#[derive(Debug, Clone)]
pub struct Script {
    pub stmts: Vec<Stmt>,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct Stmt {
    pub kind: StmtKind,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub enum StmtKind {
    Import {
        module: String,
        items: Option<Vec<String>>,
    },
    Let {
        name: String,
        mutable: bool,
        ty: Option<TypeAnn>,
        value: Expr,
    },
    Assig {
        name: String,
        value: Expr,
    },
    Fn {
        name: String,
        params: Vec<Param>,
        ret: Option<TypeAnn>,
        body: Vec<Stmt>,
    },
    Task {
        name: String,
        body: Vec<Stmt>,
    },
    Return {
        value: Option<Expr>,
    },
    If {
        branches: Vec<IfBranch>,
        else_: Option<Vec<Stmt>>,
    },
    For {
        name: String,
        iter: Expr,
        body: Vec<Stmt>,
    },
    While {
        cond: Expr,
        body: Vec<Stmt>,
    },
    Try {
        body: Vec<Stmt>,
        catch_var: String,
        handler: Vec<Stmt>,
    },
    Exec {
        parts: Vec<ExecPart>,
        swallow: bool,
    },
    Exit {
        code: Option<Expr>,
    },
    ExprStmt(Expr),
}

#[derive(Debug, Clone)]
pub struct IfBranch {
    pub cond: Expr,
    pub body: Vec<Stmt>,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub enum ExecPart {
     /// A raw literal word (possibly containing flags, paths, URLs)
     Word(String),
     /// An interpolated `{expr}` within the exec line
     Interp(Expr),
}

#[derive(Debug, Clone)]
pub struct Expr {
    pub kind: ExprKind,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub enum ExprKind {
    Int(i64),
    Float(f64),
    Bool(bool),
    Str(String),
    /// `f"text {expr} text …"`
    FStr(Vec<FStrPart>),
    /// `[a, b, c]`
    List(Vec<Expr>),

    /// `{ key: value, … }`
    Map(Vec<(String, Expr)>),
    /// A bare name: `x`, `os`, `tools`
    Ident(String),
    /// `expr.field`
    Field {
        object: Box<Expr>,
        field:  String,
    },
    /// `expr[index]`
    Index {
        object: Box<Expr>,
        index:  Box<Expr>,
    },
    /// Binary: `a + b`, `a == b`, `a and b`, …
    BinOp {
        op:    BinOp,
        left:  Box<Expr>,
        right: Box<Expr>,
    },
    /// Unary: `not x`, `-x`
    UnaryOp {
        op:      UnaryOp,
        operand: Box<Expr>,
    },

    /// `name(args)` — function or task call
    Call {
        callee: Box<Expr>,
        args:   Vec<Expr>,
    },

    /// `expr.method(args)` — module method call: `env.get("VAR")`
    MethodCall {
        object: Box<Expr>,
        method: String,
        args:   Vec<Expr>,
    },

    /// `expr or default` — fallback / short-circuit
    Or {
        left:  Box<Expr>,
        right: Box<Expr>,
    },

    /// `a..b` range literal
    Range {
        start: Box<Expr>,
        end:   Box<Expr>,
    },
}

#[derive(Debug, Clone)]
pub enum FStrPart {
    /// A literal text run: `"hello "`
    Text(String),
    /// An interpolated expression: `{name}`
    Interp(Expr),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinOp {
    // Arithmetic
    Add, Sub, Mul, Div, Mod,
    // Comparison
    Eq, NotEq, Lt, LtEq, Gt, GtEq,
    // Logical
    And,
}

impl std::fmt::Display for BinOp {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BinOp::Add   => write!(f, "+"),
            BinOp::Sub   => write!(f, "-"),
            BinOp::Mul   => write!(f, "*"),
            BinOp::Div   => write!(f, "/"),
            BinOp::Mod   => write!(f, "%"),
            BinOp::Eq    => write!(f, "=="),
            BinOp::NotEq => write!(f, "!="),
            BinOp::Lt    => write!(f, "<"),
            BinOp::LtEq  => write!(f, "<="),
            BinOp::Gt    => write!(f, ">"),
            BinOp::GtEq  => write!(f, ">="),
            BinOp::And   => write!(f, "and"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnaryOp {
    Neg,  // -x
    Not,  // not x
}

#[derive(Debug, Clone)]
pub struct Param {
    pub name: String,
    pub ty: TypeAnn,
    pub span: Span,
}

/// PSH is dynamically typed at runtime, but allows optional annotations
/// for documentation and future type-checking.
#[derive(Debug, Clone, PartialEq)]
pub enum TypeAnn {
    String,
    Int,
    Float,
    Bool,
    List,
    Map,
    Any,
}

impl std::fmt::Display for TypeAnn {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TypeAnn::String => write!(f, "string"),
            TypeAnn::Int    => write!(f, "int"),
            TypeAnn::Float  => write!(f, "float"),
            TypeAnn::Bool   => write!(f, "bool"),
            TypeAnn::List   => write!(f, "list"),
            TypeAnn::Map    => write!(f, "map"),
            TypeAnn::Any    => write!(f, "any"),
        }
    }
}
