use crate::{LexerError, Span, Token};
use crate::token::Spanned;

#[derive(Debug, Clone)]
enum Mode {
    Normal,
    Exec,
    ExecInterp { depth: u32 },
    FStr,
    FStrInterp { depth: u32 },
}

pub struct Lexer<'src> {
    src: &'src str,
    bytes: &'src [u8],
    pos: usize,
    modes: Vec<Mode>,
    errors: Vec<LexerError>,
}

impl<'src> Lexer<'src> {
    pub fn new(src: &'src str) -> Self {
        Self {
            src,
            bytes: src.as_bytes(),
            pos: 0,
            modes: vec![Mode::Normal],
            errors: Vec::new(),
        }
    }

    /// Lex the entire source into a flat token stream.
    /// Returns `(tokens, errors)`. The stream always end with 'EoF'
    pub fn tokenize(mut self)  -> (Vec<Spanned>, Vec<LexerError>) {
        let mut out = Vec::new();
        loop {
            let s = self.next();
            let done = s.token == Token::Eof;
            out.push(s);
            if done { break; }
        }
        (out, self.errors)
    }

    #[inline] fn mode(&self) -> &Mode { self.modes.last().unwrap() }
    #[inline] fn peek(&self) -> Option<u8> { self.bytes.get(self.pos).copied() }
    #[inline] fn peek2(&self) -> Option<u8> { self.bytes.get(self.pos + 1).copied() }

    #[inline]
    fn advance(&mut self) -> Option<u8> {
        let b = self.bytes.get(self.pos).copied();
        if b.is_some() { self.pos += 1; }
        b
    }

    fn eat_while(&mut self, pred: impl Fn(u8) -> bool) {
        while self.peek().map_or(false, |b| pred(b)) { self.advance(); }
    }

    #[inline] fn slice(&self, start: usize) -> &str { &self.src[start..self.pos] }
    #[inline] fn span(&self, start: usize) -> Span { Span::new(start, self.pos) }
    #[inline] fn tok(&self, token: Token, start: usize) -> Spanned {
        Spanned::new(token, self.span(start))
    }

    fn push_mode(&mut self, m: Mode) { self.modes.push(m); }
    fn pop_mode(&mut self) { if self.modes.len() > 1 { self.modes.pop(); } }

    fn skip_inline_ws (&mut self) {
        self.eat_while(|b| b == b' ' || b == b'\t' || b == b'\r');
    }

    fn skip_line_comment(&mut self) {
        self.eat_while(|b| b != b'\n');
    }

    fn skip_block_comment(&mut self, start: usize) {
        loop {
            match self.advance() {
                None => { self.errors.push(LexerError::UnterminatedBlock { span: self.span(start) }); break; }
                Some(b'*') if self.peek() == Some(b'/') => { self.advance(); break; }
                _ => {}
            }
        }
    }

    fn scan_escape(&mut self, str_start: usize, out: &mut String) {
        let esc = self.pos - 1;
        match self.advance() {
            Some(b'n') => out.push('\n'),
            Some(b't')  => out.push('\t'),
            Some(b'r')  => out.push('\r'),
            Some(b'\\') => out.push('\\'),
            Some(b'"')  => out.push('"'),
            Some(b'{')  => out.push('{'),
            Some(ch) => self.errors.push(LexerError::InvalidEscape { ch: ch as char, span: Span::new(esc, self.pos) }),
            None     => self.errors.push(LexerError::UnterminatedString { span: self.span(str_start) }),
        }
    }

    fn scan_number(&mut self, start: usize) -> Spanned {
        self.eat_while(|b| b.is_ascii_digit() || b == b'_');

        if self.peek() == Some(b'.') && self.peek2().map_or(false, |b| b.is_ascii_digit()) {
            self.advance();
            self.eat_while(|b| b.is_ascii_digit() || b == b'_');
            let raw = self.slice(start).replace('_', "");
            return match raw.parse::<f64>() {
                Ok(v)  => self.tok(Token::Float(v), start),
                Err(_) => { self.errors.push(LexerError::InvalidNumber { span: self.span(start) }); self.tok(Token::Float(0.0), start) }
            };
        }

        let raw = self.slice(start).replace('_', "");
        match raw.parse::<i64>() {
            Ok(v)  => self.tok(Token::Int(v), start),
            Err(_) => { self.errors.push(LexerError::InvalidNumber { span: self.span(start) }); self.tok(Token::Int(0), start) }
        }
        }

    /// '"' already consumed: detects and dispatches triple quote.
    fn scan_string(&mut self, start: usize) -> Spanned {
        if self.peek() == Some(b'"') && self.peek2() == Some(b'"') {
            self.advance(); self.advance();
            return self.scan_triple_string(start);
        }
        self.scan_single_string(start)
    }

    fn scan_single_string(&mut self, start: usize) -> Spanned {
        let mut s = String::new();
        loop {
            match self.advance() {
                None | Some(b'\n') => { self.errors.push(LexerError::UnterminatedString { span: self.span(start) }); break;}
                Some(b'"') => break,
                Some(b'\\') => self.scan_escape(start, &mut s),
                Some(b) => s.push(b as char),
            }
        }
        self.tok(Token::Str(s), start)
    }

    fn scan_triple_string(&mut self, start: usize) -> Spanned {
        let mut s = String::new();
        loop {
            if self.peek() == Some(b'"') && self.peek2() == Some(b'"') && self.bytes.get(self.pos + 2) == Some(&b'"') {
              self.pos += 3; break;
            }
            match self.advance() {
                None => { self.errors.push(LexerError::UnterminatedString { span: self.span(start) }); break;}
                Some(b'\\') => self.scan_escape(start, &mut s),
                Some(b) => s.push(b as char),
            }
        }
        self.tok(Token::Str(s), start)
    }

    // Returns Some(text) if the run is non-empty
    fn scan_fstr_text(&mut self) -> Option<(String, usize, usize)> {
        let start = self.pos;
        let mut s = String::new();
        loop {
            match self.peek() {
                None | Some(b'"') | Some(b'{') => break,
                Some(b'\\') => { self.advance(); self.scan_escape(start, &mut s); }
                Some(b)     => { self.advance(); s.push(b as char); }
            }
        }
        if s.is_empty() { None } else { Some((s, start, self.pos)) }
    }

    fn scan_exec_word(&mut self, start: usize) -> Spanned {
        loop {
            match self.peek() {
                None
                | Some(b'\n') | Some(b'\r')
                | Some(b' ')  | Some(b'\t')
                | Some(b'{')  | Some(b'?') => break,
                Some(b'/') if self.peek2() == Some(b'/') && self.pos == start => break,
                _ => { self.advance(); }
            }
        }
        self.tok(Token::ExecWord(self.slice(start).to_owned()), start)
    }

    fn next(&mut self) -> Spanned {
        match self.mode().clone() {
            Mode::Normal                 => self.lex_normal(),
            Mode::Exec                   => self.lex_exec(),
            Mode::ExecInterp { depth }   => self.lex_exec_interp(depth),
            Mode::FStr                   => self.lex_fstr(),
            Mode::FStrInterp { depth }   => self.lex_fstr_interp(depth),
        }
    }

    fn lex_normal(&mut self) -> Spanned {
           self.skip_inline_ws();
           let start = self.pos;

           let b = match self.peek() {
               None    => return self.tok(Token::Eof, start),
               Some(b) => b,
           };

           // Comments
           if b == b'/' {
               match self.peek2() {
                   Some(b'/') => {
                       self.advance(); self.advance();
                       self.skip_line_comment();
                       return self.lex_normal();
                   }
                   Some(b'*') => {
                       self.advance(); self.advance();
                       self.skip_block_comment(start);
                       return self.lex_normal();
                   }
                   _ => {}
               }
           }

           // Newlines
           if b == b'\n' {
               self.advance();
               return self.tok(Token::Newline, start);
           }
           if b == b'\r' {
               self.advance();
               if self.peek() == Some(b'\n') { self.advance(); }
               return self.tok(Token::Newline, start);
           }

           // f-string: f followed immediately by `"`
           if b == b'f' && self.peek2() == Some(b'"') {
               self.advance(); self.advance(); // consume f"
               self.push_mode(Mode::FStr);
               return self.tok(Token::FStrStart, start);
           }

           // Plain string
           if b == b'"' {
               self.advance();
               return self.scan_string(start);
           }

           // Number
           if b.is_ascii_digit() {
               self.advance();
               return self.scan_number(start);
           }

           // Identifier / keyword
           if b.is_ascii_alphabetic() || b == b'_' {
               self.advance();
               self.eat_while(|b| b.is_ascii_alphanumeric() || b == b'_');
               let tok = Token::from_word(self.slice(start));
               if tok == Token::Exec { self.push_mode(Mode::Exec); }
               return self.tok(tok, start);
           }

           // Two-char operators (must check before single-char fallthrough)
           if let Some(two) = self.two_char_op(b) {
               self.advance(); self.advance();
               return self.tok(two, start);
           }

           // Single-char tokens
           self.advance();
           let tok = match b {
               b'=' => Token::Eq,       b'<' => Token::Lt,       b'>' => Token::Gt,
               b'+' => Token::Plus,     b'-' => Token::Minus,    b'*' => Token::Star,
               b'/' => Token::Slash,    b'%' => Token::Percent,  b'?' => Token::Question,
               b'.' => Token::Dot,      b',' => Token::Comma,    b':' => Token::Colon,
               b';' => Token::Semi,     b'{' => Token::LBrace,   b'}' => Token::RBrace,
               b'(' => Token::LParen,   b')' => Token::RParen,   b'[' => Token::LBracket,
               b']' => Token::RBracket,
               ch => {
                   self.errors.push(LexerError::UnexpectedChar { ch: ch as char, span: self.span(start) });
                   return self.lex_normal(); // skip bad char and continue
               }
           };
           self.tok(tok, start)
       }

    fn two_char_op(&self, first: u8) -> Option<Token> {
        let second = self.peek2()?;
        match (first, second) {
            (b'=', b'=') => Some(Token::EqEq),
            (b'!', b'=') => Some(Token::BangEq),
            (b'<', b'=') => Some(Token::LtEq),
            (b'>', b'=') => Some(Token::GtEq),
            (b'-', b'>') => Some(Token::Arrow),
            (b'.', b'.') => Some(Token::DotDot),
            _            => None,
        }
    }

    fn lex_exec(&mut self) -> Spanned {
        // Skip only spaces/tabs — NOT newlines (newline terminates the exec line)
        self.skip_inline_ws();
        let start = self.pos;

        match self.peek() {
            // End of source or newline terminates the exec line
            None => {
                self.pop_mode();
                self.tok(Token::ExecEnd, start)
            }
            Some(b'\n') => {
                self.advance();
                self.pop_mode();
                self.tok(Token::ExecEnd, start)
            }
            Some(b'\r') => {
                self.advance();
                if self.peek() == Some(b'\n') { self.advance(); }
                self.pop_mode();
                self.tok(Token::ExecEnd, start)
            }

            // `?` swallow-error suffix: emit Question, consume trailing newline, end exec
            Some(b'?') => {
                self.advance();
                self.skip_inline_ws();
                match self.peek() {
                    Some(b'\n') => { self.advance(); }
                    Some(b'\r') => { self.advance(); if self.peek() == Some(b'\n') { self.advance(); } }
                    _ => {}
                }
                self.pop_mode();
                self.tok(Token::Question, start)
            }

            // `{expr}` interpolation
            Some(b'{') => {
                self.advance();
                self.push_mode(Mode::ExecInterp { depth: 1 });
                self.tok(Token::ExecOpen, start)
            }

            // `//` comment ends the exec line (do not emit Newline)
            Some(b'/') if self.peek2() == Some(b'/') => {
                self.skip_line_comment();
                match self.peek() {
                    Some(b'\n') => { self.advance(); },
                    Some(b'\r') => {
                        self.advance();
                        if self.peek() == Some(b'\n') { self.advance(); }
                    }
                    _ => {}
                }
                self.pop_mode();
                self.tok(Token::ExecEnd, start)
            }

            // Raw command word
            _ => self.scan_exec_word(start),
        }
    }

    fn lex_exec_interp(&mut self, depth: u32) -> Spanned {
        self.skip_inline_ws();
        let start = self.pos;

        match self.peek() {
            None => {
                self.pop_mode();
                self.tok(Token::Eof, start)
            }
            Some(b'}') => {
                self.advance();
                if depth <= 1 {
                    self.pop_mode(); // back to Exec
                    self.tok(Token::ExecClose, start)
                } else {
                    *self.modes.last_mut().unwrap() = Mode::ExecInterp { depth: depth - 1 };
                    self.tok(Token::RBrace, start)
                }
            }
            Some(b'{') => {
                self.advance();
                *self.modes.last_mut().unwrap() = Mode::ExecInterp { depth: depth + 1 };
                self.tok(Token::LBrace, start)
            }
            // Any other token: lex as a normal expression token
            _ => self.lex_normal_one(),
        }
       }

       fn lex_fstr(&mut self) -> Spanned {
               let start = self.pos;

               match self.peek() {
                   None => {
                       self.errors.push(LexerError::UnterminatedString { span: self.span(start) });
                       self.pop_mode();
                       self.tok(Token::Eof, start)
                   }
                   Some(b'"') => {
                       self.advance();
                       self.pop_mode();
                       self.tok(Token::FStrEnd, start)
                   }
                   Some(b'{') => {
                       self.advance();
                       self.push_mode(Mode::FStrInterp { depth: 1 });
                       self.tok(Token::FStrOpen, start)
                   }
                   _ => {
                       if let Some((text, s, e)) = self.scan_fstr_text() {
                           Spanned::new(Token::FStrText(text), Span::new(s, e))
                       } else {
                           self.advance();
                           self.lex_fstr()
                       }
                   }
               }
           }

    fn lex_fstr_interp(&mut self, depth: u32) -> Spanned {
        self.skip_inline_ws();
        let start = self.pos;

        match self.peek() {
            None => {
                self.pop_mode();
                self.tok(Token::Eof, start)
            }
            Some(b'}') => {
                self.advance();
                if depth <= 1 {
                    self.pop_mode(); // back to FStr
                    self.tok(Token::FStrClose, start)
                } else {
                    *self.modes.last_mut().unwrap() = Mode::FStrInterp { depth: depth - 1 };
                    self.tok(Token::RBrace, start)
                }
            }
            Some(b'{') => {
                self.advance();
                *self.modes.last_mut().unwrap() = Mode::FStrInterp { depth: depth + 1 };
                self.tok(Token::LBrace, start)
            }
            // Any other token: lex as a normal expression token
            _ => self.lex_normal_one(),
        }
    }


    /// Lex exactly one token as if in Normal mode, without disturbing the mode
    /// stack.  Used by interp contexts to parse expression tokens.
    fn lex_normal_one(&mut self) -> Spanned {
        let depth_before = self.modes.len();
        let tok = self.lex_normal();
        // Pop any modes lex_normal pushed (e.g. Exec or FStr mode from a
        // keyword/f-string that shouldn't be active inside an interpolation).
        while self.modes.len() > depth_before {
            self.modes.pop();
        }
        tok
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tokens(src: &str) -> Vec<Token> {
        let (spanned, errors) = Lexer::new(src).tokenize();
        assert!(errors.is_empty(), "unexpected lex errors: {errors:?}");
        spanned.into_iter().map(|s| s.token).collect()
    }

    fn with_errors(src: &str) -> (Vec<Token>, Vec<LexerError>) {
        let (spanned, errors) = Lexer::new(src).tokenize();
        (spanned.into_iter().map(|s| s.token).collect(), errors)
    }

    // ── Keywords ─────────────────────────────────────────────────────────────

    #[test]
    fn keywords() {
        // Test each keyword individually because `exec` changes the lex mode
        // for everything that follows it.
        let cases = [
            ("and",    Token::And),
            ("catch",  Token::Catch),
            ("else",   Token::Else),
            ("exit",   Token::Exit),
            ("false",  Token::False),
            ("fn",     Token::Fn),
            ("for",    Token::For),
            ("if",     Token::If),
            ("import", Token::Import),
            ("in",     Token::In),
            ("let",    Token::Let),
            ("mut",    Token::Mut),
            ("not",    Token::Not),
            ("or",     Token::Or),
            ("return", Token::Return),
            ("task",   Token::Task),
            ("true",   Token::True),
            ("try",    Token::Try),
            ("while",  Token::While),
        ];
        for (src, expected) in &cases {
            let toks = tokens(src);
            assert_eq!(toks[0], *expected, "failed for keyword: {src}");
        }
        // exec is special: it transitions to Exec mode
        let toks_exec = tokens("exec\n");
        assert_eq!(toks_exec[0], Token::Exec);
    }

    // ── Operators ─────────────────────────────────────────────────────────────

    #[test]
    fn two_char_operators() {
        let toks = tokens("== != <= >= -> ..");
        assert_eq!(toks[0], Token::EqEq);
        assert_eq!(toks[1], Token::BangEq);
        assert_eq!(toks[2], Token::LtEq);
        assert_eq!(toks[3], Token::GtEq);
        assert_eq!(toks[4], Token::Arrow);
        assert_eq!(toks[5], Token::DotDot);
    }

    #[test]
    fn single_char_operators() {
        let toks = tokens("= < > + - * / % ? . , : ;");
        let expected = [
            Token::Eq, Token::Lt, Token::Gt, Token::Plus, Token::Minus,
            Token::Star, Token::Slash, Token::Percent, Token::Question,
            Token::Dot, Token::Comma, Token::Colon, Token::Semi,
        ];
        for (i, exp) in expected.iter().enumerate() { assert_eq!(&toks[i], exp); }
    }

    #[test]
    fn delimiters() {
        let toks = tokens("{ } ( ) [ ]");
        assert_eq!(toks[0], Token::LBrace);
        assert_eq!(toks[1], Token::RBrace);
        assert_eq!(toks[2], Token::LParen);
        assert_eq!(toks[3], Token::RParen);
        assert_eq!(toks[4], Token::LBracket);
        assert_eq!(toks[5], Token::RBracket);
    }

    // ── Literals ──────────────────────────────────────────────────────────────

    #[test]
    fn integer_literals() {
        let toks = tokens("0 42 1_000_000");
        assert_eq!(toks[0], Token::Int(0));
        assert_eq!(toks[1], Token::Int(42));
        assert_eq!(toks[2], Token::Int(1_000_000));
    }

    #[test]
    fn float_literals() {
        let toks = tokens("3.14 0.5 100.0");
        assert_eq!(toks[0], Token::Float(3.14));
        assert_eq!(toks[1], Token::Float(0.5));
        assert_eq!(toks[2], Token::Float(100.0));
    }

    #[test]
    fn plain_string() {
        let toks = tokens(r#""hello world""#);
        assert_eq!(toks[0], Token::Str("hello world".into()));
    }

    #[test]
    fn string_escapes() {
        let toks = tokens(r#""line1\nline2\ttab""#);
        assert_eq!(toks[0], Token::Str("line1\nline2\ttab".into()));
    }

    #[test]
    fn triple_quoted_string() {
        let src = "\"\"\"hello\nworld\"\"\"";
        let toks = tokens(src);
        assert_eq!(toks[0], Token::Str("hello\nworld".into()));
    }

    // ── F-strings ─────────────────────────────────────────────────────────────

    #[test]
    fn fstring_no_interp() {
        let toks = tokens(r#"f"hello""#);
        assert_eq!(toks[0], Token::FStrStart);
        assert_eq!(toks[1], Token::FStrText("hello".into()));
        assert_eq!(toks[2], Token::FStrEnd);
        assert_eq!(toks[3], Token::Eof);
    }

    #[test]
    fn fstring_single_interp() {
        let toks = tokens(r#"f"hello {name}""#);
        assert_eq!(toks[0], Token::FStrStart);
        assert_eq!(toks[1], Token::FStrText("hello ".into()));
        assert_eq!(toks[2], Token::FStrOpen);
        assert_eq!(toks[3], Token::Ident("name".into()));
        assert_eq!(toks[4], Token::FStrClose);
        assert_eq!(toks[5], Token::FStrEnd);
    }

    #[test]
    fn fstring_multiple_interps() {
        let toks = tokens(r#"f"{a} and {b}""#);
        assert_eq!(toks[0], Token::FStrStart);
        assert_eq!(toks[1], Token::FStrOpen);
        assert_eq!(toks[2], Token::Ident("a".into()));
        assert_eq!(toks[3], Token::FStrClose);
        assert_eq!(toks[4], Token::FStrText(" and ".into()));
        assert_eq!(toks[5], Token::FStrOpen);
        assert_eq!(toks[6], Token::Ident("b".into()));
        assert_eq!(toks[7], Token::FStrClose);
        assert_eq!(toks[8], Token::FStrEnd);
    }

    #[test]
    fn fstring_dot_access_in_interp() {
        let toks = tokens(r#"f"{os.platform}""#);
        assert_eq!(toks[0], Token::FStrStart);
        assert_eq!(toks[1], Token::FStrOpen);
        assert_eq!(toks[2], Token::Ident("os".into()));
        assert_eq!(toks[3], Token::Dot);
        assert_eq!(toks[4], Token::Ident("platform".into()));
        assert_eq!(toks[5], Token::FStrClose);
        assert_eq!(toks[6], Token::FStrEnd);
    }

    // ── Exec lines ────────────────────────────────────────────────────────────

    #[test]
    fn exec_simple() {
        let toks = tokens("exec git clone https://github.com/org/repo\n");
        assert_eq!(toks[0], Token::Exec);
        assert_eq!(toks[1], Token::ExecWord("git".into()));
        assert_eq!(toks[2], Token::ExecWord("clone".into()));
        assert_eq!(toks[3], Token::ExecWord("https://github.com/org/repo".into()));
        assert_eq!(toks[4], Token::ExecEnd);
    }

    #[test]
    fn exec_with_interp() {
        let toks = tokens("exec cargo build --{profile}\n");
        assert_eq!(toks[0], Token::Exec);
        assert_eq!(toks[1], Token::ExecWord("cargo".into()));
        assert_eq!(toks[2], Token::ExecWord("build".into()));
        assert_eq!(toks[3], Token::ExecWord("--".into()));
        assert_eq!(toks[4], Token::ExecOpen);
        assert_eq!(toks[5], Token::Ident("profile".into()));
        assert_eq!(toks[6], Token::ExecClose);
        assert_eq!(toks[7], Token::ExecEnd);
    }

    #[test]
    fn exec_question_swallow() {
        let toks = tokens("exec rm -rf dist ?\n");
        assert_eq!(toks[0], Token::Exec);
        assert_eq!(toks[1], Token::ExecWord("rm".into()));
        assert_eq!(toks[2], Token::ExecWord("-rf".into()));
        assert_eq!(toks[3], Token::ExecWord("dist".into()));
        assert_eq!(toks[4], Token::Question);
        assert_eq!(toks[5], Token::Eof);
    }

    #[test]
    fn exec_comment_ends_line() {
        let toks = tokens("exec echo hello // ignored\nlet x = 1\n");
        let end = toks.iter().position(|t| t == &Token::ExecEnd).unwrap();
        assert_eq!(toks[end + 1], Token::Let);
    }

    // ── Comments ──────────────────────────────────────────────────────────────

    #[test]
    fn line_comment_skipped() {
        let toks = tokens("let x = 1 // comment\nlet y = 2\n");
        let non: Vec<_> = toks.iter()
            .filter(|t| !matches!(t, Token::Newline | Token::Eof))
            .collect();
        assert_eq!(non.len(), 8); // let x = 1  let y = 2
    }

    #[test]
    fn block_comment_skipped() {
        let toks = tokens("let /* gone */ x = 1\n");
        let non: Vec<_> = toks.iter()
            .filter(|t| !matches!(t, Token::Newline | Token::Eof))
            .collect();
        assert_eq!(non[0], &Token::Let);
        assert_eq!(non[1], &Token::Ident("x".into()));
    }

    // ── Span tracking ─────────────────────────────────────────────────────────

    #[test]
    fn span_byte_offsets() {
        let src = "let x = 42";
        let (spanned, _) = Lexer::new(src).tokenize();
        assert_eq!(spanned[0].span, Span::new(0, 3));  // let
        assert_eq!(spanned[1].span, Span::new(4, 5));  // x
        assert_eq!(spanned[2].span, Span::new(6, 7));  // =
        assert_eq!(spanned[3].span, Span::new(8, 10)); // 42
    }

    #[test]
    fn span_line_column() {
        let src = "let x = 1\nlet y = 2";
        let (spanned, _) = Lexer::new(src).tokenize();
        let second_let = spanned.iter().filter(|s| s.token == Token::Let).nth(1).unwrap();
        let (line, col) = second_let.span.location(src);
        assert_eq!(line, 2);
        assert_eq!(col, 1);
    }

    // ── Identifiers ───────────────────────────────────────────────────────────

    #[test]
    fn dot_access_lexes_as_three_tokens() {
        let toks = tokens("os.platform");
        assert_eq!(toks[0], Token::Ident("os".into()));
        assert_eq!(toks[1], Token::Dot);
        assert_eq!(toks[2], Token::Ident("platform".into()));
    }

    #[test]
    fn let_mut_sequence() {
        let toks = tokens("let mut count = 0\n");
        assert_eq!(toks[0], Token::Let);
        assert_eq!(toks[1], Token::Mut);
        assert_eq!(toks[2], Token::Ident("count".into()));
        assert_eq!(toks[3], Token::Eq);
        assert_eq!(toks[4], Token::Int(0));
    }

    // ── Error recovery ────────────────────────────────────────────────────────

    #[test]
    fn unterminated_string() {
        let (_, errs) = with_errors(r#""oops"#);
        assert!(errs.iter().any(|e| matches!(e, LexerError::UnterminatedString { .. })));
    }

    #[test]
    fn invalid_escape() {
        let (_, errs) = with_errors(r#""bad \q escape""#);
        assert!(errs.iter().any(|e| matches!(e, LexerError::InvalidEscape { ch: 'q', .. })));
    }

    #[test]
    fn unterminated_block_comment() {
        let (_, errs) = with_errors("/* no end");
        assert!(errs.iter().any(|e| matches!(e, LexerError::UnterminatedBlock { .. })));
    }

    #[test]
    fn eof_is_always_last_token() {
        for src in ["", "let x = 1\n", "// just a comment\n"] {
            let toks = tokens(src);
            assert_eq!(toks.last(), Some(&Token::Eof), "missing Eof for: {src:?}");
        }
    }
}
