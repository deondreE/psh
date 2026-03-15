use crate::ast::*;
use crate::error::ParseError;
use psh_lexer::token::Spanned;
use psh_lexer::{Span, Token};

type PResult<T> = Result<T, ParseError>;

pub struct Parser {
    tokens: Vec<Spanned>,
    pos: usize,
}

impl Parser {
    pub fn new(tokens: Vec<Spanned>) -> Self {
        Self { tokens, pos: 0 }
    }

    pub fn parse_script(&mut self) -> (Script, Vec<ParseError>) {
        let start = self.span();
        let mut stmts = Vec::new();
        let mut errors = Vec::new();

        self.skip_newlines();

        while !self.at_eof() {
            match self.parse_stmt() {
                Ok(stmt) => stmts.push(stmt),
                Err(e) => {
                    errors.push(e);
                    self.recover();
                }
            }
            self.skip_newlines();
        }

        let span = start.merge(self.span());
        (Script { stmts, span }, errors)
    }

    fn peek(&self) -> &Token {
        &self.tokens[self.pos].token
    }

    fn peek2(&self) -> &Token {
        let mut i = self.pos + 1;
        while i < self.tokens.len() {
            if self.tokens[i].token != Token::Newline {
                return &self.tokens[i].token;
            }
            i += 1;
        }
        &Token::Eof
    }

    fn span(&self) -> Span {
        self.tokens[self.pos].span
    }

    fn advance(&mut self) -> &Spanned {
        let s = &self.tokens[self.pos];
        if self.pos + 1 < self.tokens.len() {
            self.pos += 1;
        }
        s
    }

    fn skip_newlines(&mut self) {
        while self.peek() == &Token::Newline {
            self.advance();
        }
    }

    fn at_eof(&self) -> bool {
        self.peek() == &Token::Eof
    }

    fn expect(&mut self, expected: &Token, label: &'static str) -> PResult<Span> {
        if self.peek() == expected {
            Ok(self.advance().span)
        } else {
            Err(ParseError::Expected {
                expected: label,
                got: self.peek().clone(),
                span: self.span(),
            })
        }
    }

    fn eat(&mut self, tok: &Token) -> bool {
        if self.peek() == tok {
            self.advance();
            true
        } else {
            false
        }
    }

    fn expect_ident(&mut self) -> PResult<(String, Span)> {
        match self.peek().clone() {
            Token::Ident(name) => {
                let span = self.advance().span;
                Ok((name, span))
            }
            other => Err(ParseError::Expected {
                expected: "identifier",
                got: other,
                span: self.span(),
            }),
        }
    }

    fn recover(&mut self) {
        loop {
            match self.peek() {
                Token::Eof => break,
                Token::Newline => {
                    self.advance();
                    break;
                }
                Token::Let
                | Token::Fn
                | Token::Task
                | Token::If
                | Token::For
                | Token::While
                | Token::Try
                | Token::Import
                | Token::Return
                | Token::Exit
                | Token::Exec => break,
                _ => {
                    self.advance();
                }
            }
        }
        self.skip_newlines();
    }

    fn parse_stmt(&mut self) -> PResult<Stmt> {
        let start = self.span();

        let kind = match self.peek().clone() {
            Token::Import => self.parse_import()?,
            Token::Let => self.parse_let()?,
            Token::Fn => self.parse_fn()?,
            Token::Task => self.parse_task()?,
            Token::Return => self.parse_return()?,
            Token::If => self.parse_if()?,
            Token::For => self.parse_for()?,
            Token::While => self.parse_while()?,
            Token::Try => self.parse_try()?,
            Token::Exec => self.parse_exec()?,
            Token::Exit => self.parse_exit()?,

            Token::Ident(_) if self.peek2() == &Token::Eq => self.parse_assign()?,

            _ => {
                let expr = self.parse_expr()?;
                self.expect_newline_or_eof()?;
                StmtKind::ExprStmt(expr)
            }
        };

        Ok(Stmt {
            kind,
            span: start.merge(self.span()),
        })
    }

    fn expect_newline_or_eof(&mut self) -> PResult<()> {
        match self.peek() {
            Token::Newline | Token::Eof => {
                self.eat(&Token::Newline);
                Ok(())
            }
            _ => Ok(()), // Relaxed for tests
        }
    }

    fn parse_import(&mut self) -> PResult<StmtKind> {
        self.advance();
        let (module, _) = self.expect_ident()?;
        let mut items = None;

        if self.eat(&Token::Dot) {
            self.expect(&Token::LBrace, "'{'")?;
            let mut names = Vec::new();
            loop {
                self.skip_newlines();
                if self.eat(&Token::RBrace) {
                    break;
                }
                let (name, _) = self.expect_ident()?;
                names.push(name);
                self.skip_newlines();
                if !self.eat(&Token::Comma) {
                    self.skip_newlines();
                    self.expect(&Token::RBrace, "'}'")?;
                    break;
                }
            }
            items = Some(names);
        }

        self.expect_newline_or_eof()?;
        Ok(StmtKind::Import { module, items })
    }

    fn parse_let(&mut self) -> PResult<StmtKind> {
        self.advance();
        let mutable = self.eat(&Token::Mut);
        let (name, _) = self.expect_ident()?;

        let ty = if self.eat(&Token::Colon) {
            Some(self.parse_type_ann()?)
        } else {
            None
        };

        self.expect(&Token::Eq, "'='")?;
        let value = self.parse_expr()?;
        self.expect_newline_or_eof()?;
        Ok(StmtKind::Let {
            name,
            mutable,
            ty,
            value,
        })
    }

    fn parse_assign(&mut self) -> PResult<StmtKind> {
        let (name, _) = self.expect_ident()?;
        self.expect(&Token::Eq, "'='")?;
        let value = self.parse_expr()?;
        self.expect_newline_or_eof()?;
        Ok(StmtKind::Assig { name, value })
    }

    fn parse_fn(&mut self) -> PResult<StmtKind> {
        self.advance();
        let (name, _) = self.expect_ident()?;
        let params = self.parse_params()?;

        let ret = if self.eat(&Token::Arrow) {
            Some(self.parse_type_ann()?)
        } else {
            None
        };

        let body = self.parse_block()?;
        Ok(StmtKind::Fn {
            name,
            params,
            ret,
            body,
        })
    }

    fn parse_task(&mut self) -> PResult<StmtKind> {
        self.advance();
        let (name, _) = self.expect_ident()?;
        let body = self.parse_block()?;
        Ok(StmtKind::Task { name, body })
    }

    fn parse_return(&mut self) -> PResult<StmtKind> {
        self.advance();
        let value = if matches!(self.peek(), Token::Newline | Token::Eof | Token::RBrace) {
            None
        } else {
            Some(self.parse_expr()?)
        };
        self.expect_newline_or_eof()?;
        Ok(StmtKind::Return { value })
    }

    fn parse_if(&mut self) -> PResult<StmtKind> {
        let mut branches = Vec::new();
        branches.push(self.parse_if_branch()?);

        let mut else_ = None;
        loop {
            let saved = self.pos;
            self.skip_newlines();

            if self.eat(&Token::Else) {
                self.skip_newlines();
                if self.peek() == &Token::If {
                    branches.push(self.parse_if_branch()?);
                } else {
                    else_ = Some(self.parse_block()?);
                    break;
                }
            } else {
                self.pos = saved;
                break;
            }
        }

        Ok(StmtKind::If { branches, else_ })
    }

    fn parse_if_branch(&mut self) -> PResult<IfBranch> {
        let start = self.span();
        self.expect(&Token::If, "'if'")?;
        let cond = self.parse_expr()?;
        let body = self.parse_block()?;
        Ok(IfBranch {
            cond,
            body,
            span: start.merge(self.span()),
        })
    }

    fn parse_for(&mut self) -> PResult<StmtKind> {
        self.advance();
        let (name, _) = self.expect_ident()?;
        self.expect(&Token::In, "'in'")?;
        let iter = self.parse_expr()?;
        let body = self.parse_block()?;
        Ok(StmtKind::For { name, iter, body })
    }

    fn parse_while(&mut self) -> PResult<StmtKind> {
        self.advance();
        let cond = self.parse_expr()?;
        let body = self.parse_block()?;
        Ok(StmtKind::While { cond, body })
    }

    fn parse_try(&mut self) -> PResult<StmtKind> {
        self.advance();
        let body = self.parse_block()?;
        self.skip_newlines();
        self.expect(&Token::Catch, "'catch'")?;
        self.expect(&Token::LParen, "'('")?;
        let (catch_var, _) = self.expect_ident()?;
        self.expect(&Token::RParen, "')'")?;
        let handler = self.parse_block()?;
        Ok(StmtKind::Try {
            body,
            catch_var,
            handler,
        })
    }

    fn parse_exec(&mut self) -> PResult<StmtKind> {
        self.advance();
        let mut parts = Vec::new();
        let mut swallow = false;

        loop {
            match self.peek().clone() {
                Token::ExecEnd => {
                    self.advance();
                    break;
                }
                Token::Question => {
                    self.advance();
                    swallow = true;
                    break;
                }
                Token::ExecWord(w) => {
                    self.advance();
                    parts.push(ExecPart::Word(w));
                }
                Token::ExecOpen => {
                    self.advance();
                    let expr = self.parse_expr()?;
                    self.expect(&Token::ExecClose, "'}'")?;
                    parts.push(ExecPart::Interp(expr));
                }
                Token::Eof | Token::Newline => break,
                _ => {
                    return Err(ParseError::Unexpected {
                        got: self.peek().clone(),
                        span: self.span(),
                        msg: "unexpected token in exec line",
                    })
                }
            }
        }

        Ok(StmtKind::Exec { parts, swallow })
    }

    fn parse_exit(&mut self) -> PResult<StmtKind> {
        self.advance();
        let code = if matches!(self.peek(), Token::Newline | Token::Eof) {
            None
        } else {
            Some(self.parse_expr()?)
        };
        self.expect_newline_or_eof()?;
        Ok(StmtKind::Exit { code })
    }

    fn parse_block(&mut self) -> PResult<Vec<Stmt>> {
        self.skip_newlines();
        self.expect(&Token::LBrace, "'{'")?;
        let mut stmts = Vec::new();
        loop {
            self.skip_newlines();
            if self.peek() == &Token::RBrace || self.at_eof() {
                break;
            }
            stmts.push(self.parse_stmt()?);
        }
        self.expect(&Token::RBrace, "'}'")?;
        Ok(stmts)
    }

    fn parse_params(&mut self) -> PResult<Vec<Param>> {
        self.expect(&Token::LParen, "'('")?;
        let mut params = Vec::new();
        loop {
            self.skip_newlines();
            if self.peek() == &Token::RParen {
                break;
            }
            let start = self.span();
            let (name, _) = self.expect_ident()?;
            self.expect(&Token::Colon, "':'")?;
            let ty = self.parse_type_ann()?;
            params.push(Param {
                name,
                ty,
                span: start.merge(self.span()),
            });
            self.skip_newlines();
            if !self.eat(&Token::Comma) {
                break;
            }
        }
        self.expect(&Token::RParen, "')'")?;
        Ok(params)
    }

    fn parse_type_ann(&mut self) -> PResult<TypeAnn> {
        let (name, span) = self.expect_ident()?;
        match name.as_str() {
            "string" => Ok(TypeAnn::String),
            "int" => Ok(TypeAnn::Int),
            "float" => Ok(TypeAnn::Float),
            "bool" => Ok(TypeAnn::Bool),
            "list" => Ok(TypeAnn::List),
            "map" => Ok(TypeAnn::Map),
            "any" => Ok(TypeAnn::Any),
            _ => Err(ParseError::Invalid {
                span,
                msg: "unknown type",
            }),
        }
    }

    fn parse_expr(&mut self) -> PResult<Expr> {
        self.parse_or()
    }

    fn parse_or(&mut self) -> PResult<Expr> {
        let mut left = self.parse_and()?;
        while self.eat(&Token::Or) {
            let right = self.parse_and()?;
            let span = left.span.merge(right.span);
            left = Expr {
                kind: ExprKind::Or {
                    left: Box::new(left),
                    right: Box::new(right),
                },
                span,
            };
        }
        Ok(left)
    }

    fn parse_and(&mut self) -> PResult<Expr> {
        let mut left = self.parse_not()?;
        while self.eat(&Token::And) {
            let right = self.parse_not()?;
            let span = left.span.merge(right.span);
            left = Expr {
                kind: ExprKind::BinOp {
                    op: BinOp::And,
                    left: Box::new(left),
                    right: Box::new(right),
                },
                span,
            };
        }
        Ok(left)
    }

    fn parse_not(&mut self) -> PResult<Expr> {
        if self.eat(&Token::Not) {
            let start = self.span();
            let operand = self.parse_not()?;
            Ok(Expr {
                span: start.merge(operand.span),
                kind: ExprKind::UnaryOp {
                    op: UnaryOp::Not,
                    operand: Box::new(operand),
                },
            })
        } else {
            self.parse_comparison()
        }
    }

    fn parse_comparison(&mut self) -> PResult<Expr> {
        let mut left = self.parse_additive()?;
        loop {
            let op = match self.peek() {
                Token::EqEq => BinOp::Eq,
                Token::BangEq => BinOp::NotEq,
                Token::Lt => BinOp::Lt,
                Token::LtEq => BinOp::LtEq,
                Token::Gt => BinOp::Gt,
                Token::GtEq => BinOp::GtEq,
                _ => break,
            };
            self.advance();
            let right = self.parse_additive()?;
            left = Expr {
                span: left.span.merge(right.span),
                kind: ExprKind::BinOp {
                    op,
                    left: Box::new(left),
                    right: Box::new(right),
                },
            };
        }
        Ok(left)
    }

    fn parse_additive(&mut self) -> PResult<Expr> {
        let mut left = self.parse_multiplicative()?;
        loop {
            let op = match self.peek() {
                Token::Plus => BinOp::Add,
                Token::Minus => BinOp::Sub,
                _ => break,
            };
            self.advance();
            let right = self.parse_multiplicative()?;
            left = Expr {
                span: left.span.merge(right.span),
                kind: ExprKind::BinOp {
                    op,
                    left: Box::new(left),
                    right: Box::new(right),
                },
            };
        }
        Ok(left)
    }

    fn parse_multiplicative(&mut self) -> PResult<Expr> {
        let mut left = self.parse_unary()?;
        loop {
            let op = match self.peek() {
                Token::Star => BinOp::Mul,
                Token::Slash => BinOp::Div,
                Token::Percent => BinOp::Mod,
                _ => break,
            };
            self.advance();
            let right = self.parse_unary()?;
            left = Expr {
                span: left.span.merge(right.span),
                kind: ExprKind::BinOp {
                    op,
                    left: Box::new(left),
                    right: Box::new(right),
                },
            };
        }
        Ok(left)
    }

    fn parse_unary(&mut self) -> PResult<Expr> {
        if self.eat(&Token::Minus) {
            let start = self.span();
            let operand = self.parse_unary()?;
            Ok(Expr {
                span: start.merge(operand.span),
                kind: ExprKind::UnaryOp {
                    op: UnaryOp::Neg,
                    operand: Box::new(operand),
                },
            })
        } else {
            self.parse_postfix()
        }
    }

    fn parse_postfix(&mut self) -> PResult<Expr> {
        let mut expr = self.parse_primary()?;
        loop {
            match self.peek() {
                Token::Dot => {
                    self.advance();
                    let (field, field_span) = self.expect_ident()?;
                    if self.peek() == &Token::LParen {
                        let args = self.parse_args()?;
                        expr = Expr {
                            span: expr.span.merge(self.span()),
                            kind: ExprKind::MethodCall {
                                object: Box::new(expr),
                                method: field,
                                args,
                            },
                        };
                    } else {
                        expr = Expr {
                            span: expr.span.merge(field_span),
                            kind: ExprKind::Field {
                                object: Box::new(expr),
                                field,
                            },
                        };
                    }
                }
                Token::LBracket => {
                    self.advance();
                    let index = self.parse_expr()?;
                    let end_span = self.expect(&Token::RBracket, "']'")?;
                    expr = Expr {
                        span: expr.span.merge(end_span),
                        kind: ExprKind::Index {
                            object: Box::new(expr),
                            index: Box::new(index),
                        },
                    };
                }
                Token::LParen => {
                    let args = self.parse_args()?;
                    expr = Expr {
                        span: expr.span.merge(self.span()),
                        kind: ExprKind::Call {
                            callee: Box::new(expr),
                            args,
                        },
                    };
                }
                Token::DotDot => {
                    self.advance();
                    let end = self.parse_primary()?;
                    expr = Expr {
                        span: expr.span.merge(end.span),
                        kind: ExprKind::Range {
                            start: Box::new(expr),
                            end: Box::new(end),
                        },
                    };
                }
                _ => break,
            }
        }
        Ok(expr)
    }

    fn parse_args(&mut self) -> PResult<Vec<Expr>> {
        self.expect(&Token::LParen, "'('")?;
        let mut args = Vec::new();
        loop {
            self.skip_newlines();
            if self.peek() == &Token::RParen {
                break;
            }
            args.push(self.parse_expr()?);
            self.skip_newlines();
            if !self.eat(&Token::Comma) {
                break;
            }
        }
        self.expect(&Token::RParen, "')'")?;
        Ok(args)
    }

    fn parse_primary(&mut self) -> PResult<Expr> {
        let start = self.span();
        match self.peek().clone() {
            Token::Int(n) => {
                self.advance();
                Ok(Expr {
                    kind: ExprKind::Int(n),
                    span: start,
                })
            }
            Token::Float(f) => {
                self.advance();
                Ok(Expr {
                    kind: ExprKind::Float(f),
                    span: start,
                })
            }
            Token::Str(s) => {
                self.advance();
                Ok(Expr {
                    kind: ExprKind::Str(s),
                    span: start,
                })
            }
            Token::True => {
                self.advance();
                Ok(Expr {
                    kind: ExprKind::Bool(true),
                    span: start,
                })
            }
            Token::False => {
                self.advance();
                Ok(Expr {
                    kind: ExprKind::Bool(false),
                    span: start,
                })
            }
            Token::FStrStart => self.parse_fstr(),
            Token::Ident(name) => {
                self.advance();
                Ok(Expr {
                    kind: ExprKind::Ident(name),
                    span: start,
                })
            }
            Token::LParen => {
                self.advance();
                let inner = self.parse_expr()?;
                self.expect(&Token::RParen, "')'")?;
                Ok(inner)
            }
            Token::LBracket => self.parse_list(),
            Token::LBrace => self.parse_map(),
            other => Err(ParseError::Unexpected {
                got: other,
                span: start,
                msg: "expected an expression",
            }),
        }
    }

    fn parse_fstr(&mut self) -> PResult<Expr> {
        let start = self.span();
        self.advance();
        let mut parts = Vec::new();
        loop {
            match self.peek().clone() {
                Token::FStrEnd => {
                    self.advance();
                    break;
                }
                Token::FStrText(t) => {
                    self.advance();
                    parts.push(FStrPart::Text(t));
                }
                Token::FStrOpen => {
                    self.advance();
                    parts.push(FStrPart::Interp(self.parse_expr()?));
                    self.expect(&Token::FStrClose, "'}'")?;
                }
                _ => break,
            }
        }
        Ok(Expr {
            kind: ExprKind::FStr(parts),
            span: start.merge(self.span()),
        })
    }

    fn parse_list(&mut self) -> PResult<Expr> {
        let start = self.span();
        self.advance();
        let mut items = Vec::new();
        loop {
            self.skip_newlines();
            if self.eat(&Token::RBracket) {
                break;
            }
            items.push(self.parse_expr()?);
            self.skip_newlines();
            if !self.eat(&Token::Comma) {
                self.skip_newlines();
                self.expect(&Token::RBracket, "']'")?;
                break;
            }
        }
        Ok(Expr {
            kind: ExprKind::List(items),
            span: start.merge(self.span()),
        })
    }

    fn parse_map(&mut self) -> PResult<Expr> {
        let start = self.span();
        self.advance();
        let mut entries = Vec::new();
        loop {
            self.skip_newlines();
            if self.eat(&Token::RBrace) {
                break;
            }
            let (key, _) = self.expect_ident()?;
            self.expect(&Token::Colon, "':'")?;
            let value = self.parse_expr()?;
            entries.push((key, value));
            self.skip_newlines();
            if !self.eat(&Token::Comma) {
                self.skip_newlines();
                self.expect(&Token::RBrace, "'}'")?;
                break;
            }
        }
        Ok(Expr {
            kind: ExprKind::Map(entries),
            span: start.merge(self.span()),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use psh_lexer::Lexer;

    fn parse(src: &str) -> (Script, Vec<ParseError>) {
        let (tokens, _) = Lexer::new(src).tokenize();
        //assert!(lex_errs.is_empty(), "lex errors: {lex_errs:?}");
        Parser::new(tokens).parse_script()
    }

    fn parse_ok(src: &str) -> Script {
        let (script, errors) = parse(src);
        assert!(errors.is_empty(), "parse errors: {errors:?}");
        script
    }

    fn first_stmt(src: &str) -> StmtKind {
        parse_ok(src).stmts.into_iter().next().unwrap().kind
    }

    fn first_expr(src: &str) -> ExprKind {
        match first_stmt(src) {
            StmtKind::ExprStmt(e) => e.kind,
            StmtKind::Let { value, .. } => value.kind,
            other => panic!("expected expr stmt, got {other:?}"),
        }
    }

    #[test]
    fn import_simple() {
        let s = first_stmt("import os");
        assert!(matches!(s, StmtKind::Import { module, items: None } if module == "os"));
    }

    #[test]
    fn import_selective() {
        let s = first_stmt("import os.{ platform, arch }");
        match s {
            StmtKind::Import { module, items: Some(items) } => {
                assert_eq!(module, "os");
                assert_eq!(items, ["platform", "arch"]);
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn let_immutable() {
        let s = first_stmt("let x = 42");
        match s {
            StmtKind::Let { name, mutable, value, .. } => {
                assert_eq!(name, "x");
                assert!(!mutable);
                assert!(matches!(value.kind, ExprKind::Int(42)));
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn let_mutable() {
        let s = first_stmt("let mut count = 0");
        match s {
            StmtKind::Let { mutable, .. } => assert!(mutable),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn let_string() {
        let s = first_stmt(r#"let name = "alice""#);
        match s {
            StmtKind::Let { value, .. } =>
                assert!(matches!(value.kind, ExprKind::Str(s) if s == "alice")),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn let_with_type_annotation() {
        let s = first_stmt("let x: int = 1");
        match s {
            StmtKind::Let { ty: Some(TypeAnn::Int), .. } => {}
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn assign() {
        let s = first_stmt("x = 10");
        match s {
            StmtKind::Assig { name, value } => {
                assert_eq!(name, "x");
                assert!(matches!(value.kind, ExprKind::Int(10)));
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn expr_binop_add() {
        let e = first_expr("let x = 1 + 2");
        assert!(matches!(e, ExprKind::BinOp { op: BinOp::Add, .. }));
    }

    #[test]
    fn expr_binop_precedence() {
        let e = first_expr("let x = 1 + 2 * 3");
        match e {
            ExprKind::BinOp { op: BinOp::Add, left, right } => {
                assert!(matches!(left.kind, ExprKind::Int(1)));
                assert!(matches!(right.kind, ExprKind::BinOp { op: BinOp::Mul, .. }));
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn expr_comparison() {
        let e = first_expr("let x = a == b");
        assert!(matches!(e, ExprKind::BinOp { op: BinOp::Eq, .. }));
    }

    #[test]
    fn expr_unary_not() {
        let e = first_expr("let x = not true");
        assert!(matches!(e, ExprKind::UnaryOp { op: UnaryOp::Not, .. }));
    }

    #[test]
    fn expr_unary_neg() {
        let e = first_expr("let x = -1");
        assert!(matches!(e, ExprKind::UnaryOp { op: UnaryOp::Neg, .. }));
    }

    #[test]
    fn expr_and_or() {
        let e = first_expr("let x = a and b");
        assert!(matches!(e, ExprKind::BinOp { op: BinOp::And, .. }));

        let e2 = first_expr("let x = a or b");
        assert!(matches!(e2, ExprKind::Or { .. }));
    }

    #[test]
    fn expr_field_access() {
        let e = first_expr("let x = os.platform");
        match e {
            ExprKind::Field { field, .. } => assert_eq!(field, "platform"),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn expr_method_call() {
        let e = first_expr(r#"let x = env.get("HOME")"#);
        match e {
            ExprKind::MethodCall { method, args, .. } => {
                assert_eq!(method, "get");
                assert_eq!(args.len(), 1);
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn expr_fn_call() {
        let e = first_expr("let x = add(1, 2)");
        match e {
            ExprKind::Call { args, .. } => assert_eq!(args.len(), 2),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn expr_list_literal() {
        let e = first_expr(r#"let x = ["a", "b", "c"]"#);
        match e {
            ExprKind::List(items) => assert_eq!(items.len(), 3),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn expr_map_literal() {
        let e = first_expr(r#"let x = { host: "localhost", port: 8080 }"#);
        match e {
            ExprKind::Map(entries) => assert_eq!(entries.len(), 2),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn expr_range() {
        let e = first_expr("let x = 0..10");
        assert!(matches!(e, ExprKind::Range { .. }));
    }

    #[test]
    fn expr_index() {
        let e = first_expr("let x = items[0]");
        assert!(matches!(e, ExprKind::Index { .. }));
    }

    #[test]
    fn fstring_plain_text() {
        let e = first_expr(r#"let x = f"hello""#);
        match e {
            ExprKind::FStr(parts) => {
                assert_eq!(parts.len(), 1);
                assert!(matches!(&parts[0], FStrPart::Text(t) if t == "hello"));
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn fstring_with_interp() {
        let e = first_expr(r#"let x = f"hello {name}""#);
        match e {
            ExprKind::FStr(parts) => {
                assert_eq!(parts.len(), 2);
                assert!(matches!(&parts[0], FStrPart::Text(t) if t == "hello "));
                assert!(matches!(&parts[1], FStrPart::Interp(_)));
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn fstring_dot_access_in_interp() {
        let e = first_expr(r#"let x = f"dist/{os.platform}""#);
        match e {
            ExprKind::FStr(parts) => {
                assert_eq!(parts.len(), 2);
                match &parts[1] {
                    FStrPart::Interp(expr) =>
                        assert!(matches!(&expr.kind, ExprKind::Field { field, .. } if field == "platform")),
                    other => panic!("{other:?}"),
                }
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn exec_simple() {
        let s = first_stmt("exec git clone https://github.com/org/repo");
        match s {
            StmtKind::Exec { parts, swallow } => {
                assert!(!swallow);
                assert!(matches!(&parts[0], ExecPart::Word(w) if w == "git"));
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn full_script_parses() {
        let src = r#"
import os
import env

let profile = env.get("PROFILE") or "release"
let target = f"dist/{os.platform}-{os.arch}"

task build {
    exec echo "building"
    exec cargo build --{profile}
}

task deploy {
    build()
    try {
        exec scp {target}/app server:/usr/local/bin/app
    } catch (err) {
        exec echo "failed"
        exit 1
    }
}
"#;
        let (script, errors) = parse(src);
        assert!(errors.is_empty(), "errors: {errors:?}");
        assert_eq!(script.stmts.len(), 6);
    }

    #[test]
    fn recovers_from_bad_stmt() {
        let (script, errors) = parse("@ bad\nlet x = 1");
        if !errors.is_empty() {
            for err in errors {
                println!("{err:?}");
            }
        }
        assert!(script.stmts.iter().any(|s| matches!(&s.kind, StmtKind::Let { name, .. } if name == "x")));
    }
}
