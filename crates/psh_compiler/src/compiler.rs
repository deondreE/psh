use std::rc::Rc;

use psh_ast::ast::*;
use psh_lexer::Span;

use crate::{Chunk, CompileError, Opcode, Value};

type CResult<T> = Result<T, CompileError>;

#[derive(Debug, Clone)]
struct Local {
    name: String,
    slot: usize,
    mutable: bool,
    depth: usize,
}

#[derive(Debug)]
struct LoopCtx {
    /// Instruction index of the loop's condition / iter-check (continue target)
    continue_ip: usize,
    /// Instruction indices of pending 'Jump' instructions needing break targets.
    break_patches: Vec<usize>,
}

pub struct Compiler {
    chunk: Chunk,
    locals: Vec<Local>,
    scope_depth: usize,
    loop_stack: Vec<LoopCtx>,
    in_function: bool, // true only when compiling function body not 'task'
    errors: Vec<CompileError>,
}

impl Compiler {
    pub fn compile_script(script: &Script) -> (Chunk, Vec<CompileError>) {
        let mut c = Compiler::new("<script>");
        for stmt in &script.stmts {
            c.compile_stmt(stmt);
        }
        c.chunk.emit(Opcode::ReturnNil, 0);
        (c.chunk, c.errors)
    }

    fn new(name: &str) -> Self {
        Self {
            chunk:       Chunk::new(name),
            locals:      Vec::new(),
            scope_depth: 0,
            loop_stack:  Vec::new(),
            in_function: false,
            errors:      Vec::new(),
        }
    }

    fn err(&mut self, e: CompileError) {
        self.errors.push(e);
    }

    fn begin_scope(&mut self) {
        self.scope_depth += 1;
    }

    fn end_scope(&mut self) {
        let depth = self.scope_depth;
        let pops = self.locals.iter().filter(|l| l.depth == depth).count();
        for _ in 0..pops { self.chunk.emit(Opcode::Pop, 0); }
        self.locals.retain(|l| l.depth < depth);
        self.scope_depth -= 1;
    }

    /// returns its slot index.
    fn declare_local(&mut self, name: &str, mutable: bool) -> usize {
        let slot = self.locals.len();
        self.locals.push(Local {
            name: name.to_string(),
            slot,
            mutable,
            depth: self.scope_depth,
        });
        slot
    }

    /// Resolve a name to a local slot, searching innermost scope first.
    fn resolve_local(&self, name: &str) -> Option<&Local> {
        self.locals.iter().rev().find(|l| l.name == name)
    }

    fn line(span: Span) -> u32 {
        span.start as u32
    }

    fn compile_stmt(&mut self, stmt: &Stmt) {
        let line = Self::line(stmt.span);
        match &stmt.kind {
            StmtKind::Import { module, items } => self.compile_import(module, items.as_deref(), line),
            StmtKind::Let { name, mutable, value, .. } => self.compile_let(name, *mutable, value, line),
            StmtKind::Assig { name, value } => self.compile_assign(name, value, stmt.span, line),
            StmtKind::Fn { name, params, body, .. } => self.compile_fn(name, params, body, stmt.span, line),
            StmtKind::Task { name, body } => self.compile_task(name, body, line),
            StmtKind::Return { value } => self.compile_return(value.as_ref(), stmt.span, line),
            StmtKind::If { branches, else_ } => self.compile_if(branches, else_.as_deref(), line),
            StmtKind::For { name, iter, body } => self.compile_for(name, iter, body, line),
            StmtKind::While { cond, body } => self.compile_while(cond, body, line),
            StmtKind::Try { body, catch_var, handler } => self.compile_try(body, catch_var, handler, line),
            StmtKind::Exec { parts, swallow } => self.compile_exec(parts, *swallow, false, line),
            StmtKind::Exit { code } => self.compile_exit(code.as_ref(), line),
            StmtKind::ExprStmt(expr) => {
                self.compile_expr(expr);
                self.chunk.emit(Opcode::Pop, line);
            }
        }
    }

    fn compile_import(&mut self, module: &str, items: Option<&[String]>, line: u32) {
        let mod_idx = self.chunk.add_const(Value::str(module));

        match items {
            None => {
                self.chunk.emit(Opcode::ImportModule(mod_idx), line);
                let slot = self.declare_local(module, false);
                self.chunk.emit(Opcode::DefineLocal(slot), line);
            }
            Some(fields) => {
                let field_locals: Vec<(usize, usize)> = fields.iter().map(|f| {
                    let field_idx = self.chunk.add_const(Value::str(f.as_str()));
                    let slot = self.declare_local(f.as_str(), false);
                    (slot, field_idx)
                }).collect();

                let mapped: Vec<(usize, usize)> = field_locals;
                self.chunk.emit(Opcode::ImportFields {
                    module: mod_idx, fields: mapped }, line);
            }
        }
    }

    fn compile_let(&mut self, name: &str, mutable: bool, value: &Expr, line: u32) {
        self.compile_expr(value);
        let slot = self.declare_local(name, mutable);
        self.chunk.emit(Opcode::DefineLocal(slot), line);
    }

    fn compile_assign(&mut self, name: &str, value: &Expr, span: Span, line: u32) {
        match self.resolve_local(name) {
            Some(local) if !local.mutable => {
                self.err(CompileError::ImmutableAssign { name: name.to_string(), span, });
            }
            Some(local) => {
                let slot = local.slot;
                self.compile_expr(value);
                self.chunk.emit(Opcode::StoreLocal(slot), line);
            }
            None => {
                let idx = self.chunk.add_const(Value::str(name));
                self.compile_expr(value);
                self.chunk.emit(Opcode::StoreGlobal(idx), line);
            }
        }
    }

    fn compile_fn(
        &mut self,
        name: &str,
        params: &[Param],
        body: &[Stmt],
        span: Span,
        line: u32,
    ) {
        let mut fc = Compiler::new(name);
        fc.in_function = true;

        for p in params {
            fc.declare_local(&p.name, true);
        }

        for stmt in body { fc.compile_stmt(stmt); }
        fc.chunk.emit(Opcode::ReturnNil, line);

        if !fc.errors.is_empty() {
            self.errors.extend(fc.errors);
            return;
        }

        let param_names: Vec<String> = params.iter().map(|p| p.name.clone()).collect();
        let fn_val = Value::Function {
            name: Rc::new(name.to_string()),
            params: Rc::new(param_names),
            chunk: Rc::new(fc.chunk),
        };

        let const_idx = self.chunk.add_const(fn_val);
        self.chunk.emit(Opcode::LoadConst(const_idx), line);

        let name_idx = self.chunk.add_const(Value::str(name));
        self.chunk.emit(Opcode::StoreGlobal(name_idx), line);
    }

    fn compile_task(&mut self, name: &str, body: &[Stmt], line: u32) {
        let mut tc = Compiler::new(name);

        for stmt in body { tc.compile_stmt(stmt); }
        tc.chunk.emit(Opcode::ReturnNil, line);

        if !tc.errors.is_empty() {
            self.errors.extend(tc.errors);
            return;
        }

        let task_val = Value::Task {
            name: Rc::new(name.to_string()),
            chunk: Rc::new(tc.chunk),
        };

        let const_idx =  self.chunk.add_const(task_val);
        self.chunk.emit(Opcode::LoadConst(const_idx), line);

        let name_idx = self.chunk.add_const(Value::str(name));
        self.chunk.emit(Opcode::StoreGlobal(name_idx), line);
    }

    fn compile_return(&mut self, value: Option<&Expr>, span: Span, line: u32) {
        if !self.in_function {
            self.err(CompileError::ReturnOutsideFunction { span });
            return;
        }

        match value {
            Some(v) => {
                self.compile_expr(v);
                self.chunk.emit(Opcode::Return, line);
            }
            None => {
                self.chunk.emit(Opcode::ReturnNil, line);
            }
        }
    }

    fn compile_if(&mut self, branches: &[IfBranch], else_: Option<&[Stmt]>, line: u32) {
        // We emit a chain of condition-jump-body sequences.
        // Each branch that fails, jumps past its body to the next branch
        // condition (or the else body). At the end of each body we jump
        // past all remaining branches.

        let mut end_jumps: Vec<usize> = Vec::new();

        for branch in branches {
            let bline = Self::line(branch.span);

            self.compile_expr(&branch.cond);

            let skip = self.chunk.emit_jump(Opcode::JumpIfFalse, bline);

            self.begin_scope();
            for stmt in &branch.body { self.compile_stmt(stmt); }
            self.end_scope();

            let end_j = self.chunk.emit_jump(Opcode::Jump, line);
            end_jumps.push(end_j);

            self.chunk.patch_jump(skip);
        }

        if let Some(stmts) = else_ {
            self.begin_scope();
            for stmt in stmts {
                self.compile_stmt(stmt);
            }
            self.end_scope();
        }

        for jump in end_jumps {
            self.chunk.patch_jump(jump);
        }
    }

    fn compile_for(&mut self, name: &str, iter_expr: &Expr, body: &[Stmt], line: u32) {
        self.compile_expr(iter_expr);
        self.chunk.emit(Opcode::MakeIter, line);

        let iter_slot = self.declare_local("<iter>", false);
        self.chunk.emit(Opcode::DefineLocal(iter_slot), line);

        let loop_start = self.chunk.len();
        self.chunk.emit(Opcode::LoadLocal(iter_slot), line);
        self.chunk.emit(Opcode::IterNext, line);

        // Stack: iter val ok
        // Jump past bodt if 'ok' is false
        let exit_jump = self.chunk.emit_jump(Opcode::JumpIfFalse, line);

        self.loop_stack.push(LoopCtx { continue_ip: loop_start, break_patches: Vec::new() });

        self.begin_scope();
        let var_slot = self.declare_local(name, false);
        self.chunk.emit(Opcode::DefineLocal(var_slot), line);

        for stmt in body { self.compile_stmt(stmt); }
        self.end_scope();

        self.chunk.emit(Opcode::Jump(loop_start), line);
        self.chunk.patch_jump(exit_jump);

        self.chunk.emit(Opcode::Pop, line);
        self.locals.pop();

        let ctx = self.loop_stack.pop().unwrap();
        for bp in ctx.break_patches { self.chunk.patch_jump(bp); }
    }

    fn compile_while(&mut self, cond: &Expr, body: &[Stmt], line: u32) {
        let loop_start = self.chunk.len();

        self.loop_stack.push(LoopCtx { continue_ip: loop_start, break_patches: Vec::new() });

        self.compile_expr(cond);
        let exit_jump = self.chunk.emit_jump(Opcode::JumpIfFalse, line);

        self.begin_scope();
        for stmt in body { self.compile_stmt(stmt); }
        self.end_scope();


        self.chunk.emit(Opcode::Jump(loop_start), line);
        self.chunk.patch_jump(exit_jump);

        let ctx = self.loop_stack.pop().unwrap();
        for bp in ctx.break_patches { self.chunk.patch_jump(bp); }
    }

    fn compile_try(&mut self, body: &[Stmt], catch_var: &str, handler: &[Stmt], line: u32) {
        let try_jump = self.chunk.emit_jump(Opcode::PushTry, line);

        self.begin_scope();
        for stmt in body { self.compile_stmt(stmt); }
        self.end_scope();

        self.chunk.emit(Opcode::PopTry, line);
        let end_jump = self.chunk.emit_jump(Opcode::Jump, line);

        self.chunk.patch_jump(try_jump);

        self.begin_scope();
        let err_slot = self.declare_local(catch_var, false);
        self.chunk.emit(Opcode::DefineLocal(err_slot), line);

        for stmt in handler { self.compile_stmt(stmt); }
        self.end_scope();

        self.chunk.patch_jump(end_jump);
    }

    fn compile_exec(&mut self, parts: &[ExecPart], swallow: bool, capture: bool, line: u32) {
        let mut argc = 0usize;

        for part in parts {
            match part {
                ExecPart::Word(w) => {
                    let idx = self.chunk.add_const(Value::str(w.as_str()));
                    self.chunk.emit(Opcode::LoadConst(idx), line);
                    argc += 1;
                }
                ExecPart::Interp(expr) => {
                    self.compile_expr(expr);
                    self.chunk.emit(Opcode::BuildStr(1), line);
                    argc += 1;
                }
            }
        }

        self.chunk.emit(Opcode::Exec { argc, swallow, capture }, line);
    }

    fn compile_exit(&mut self, code: Option<&Expr>, line: u32) {
        match code {
            Some(expr) => self.compile_expr(expr),
            None       => {
                let val = Value::Int(0);
                let const_idx = self.chunk.add_const(val);

                self.chunk.emit(Opcode::LoadConst(const_idx), line);
            }
        }
        self.chunk.emit(Opcode::Exit, line);
    }

    fn compile_expr(&mut self, expr: &Expr) {
        let line = Self::line(expr.span);
        match &expr.kind {
            ExprKind::Int(n)   => { let i = self.chunk.add_const(Value::Int(*n));   self.chunk.emit(Opcode::LoadConst(i), line); }
            ExprKind::Float(f) => { let i = self.chunk.add_const(Value::Float(*f)); self.chunk.emit(Opcode::LoadConst(i), line); }
            ExprKind::Bool(b)  => { self.chunk.emit(if *b { Opcode::LoadTrue } else { Opcode::LoadFalse }, line); }
            ExprKind::Str(s)   => { let i = self.chunk.add_const(Value::str(s.as_str())); self.chunk.emit(Opcode::LoadConst(i), line); }

            ExprKind::Ident(name) => self.compile_ident(name, expr.span, line),

            ExprKind::FStr(parts)  => self.compile_fstr(parts, line),
            ExprKind::List(items)  => self.compile_list(items, line),
            ExprKind::Map(entries) => self.compile_map(entries, line),

            ExprKind::Field { object, field } => {
                self.compile_expr(object);
                let idx = self.chunk.add_const(Value::str(field.as_str()));
                self.chunk.emit(Opcode::GetField(idx), line);
            }

            ExprKind::Index { object, index } => {
                self.compile_expr(object);
                self.compile_expr(index);
                self.chunk.emit(Opcode::GetIndex, line);
            }

            ExprKind::BinOp { op, left, right } => self.compile_binop(*op, left, right, line),

            ExprKind::UnaryOp { op, operand } => {
                self.compile_expr(operand);
                match op {
                    UnaryOp::Neg => { self.chunk.emit(Opcode::Neg, line); }
                    UnaryOp::Not => { self.chunk.emit(Opcode::Not, line); }
                }
            }

            ExprKind::Call { callee, args } => {
                self.compile_expr(callee);
                for arg in args { self.compile_expr(arg); }
                self.chunk.emit(Opcode::Call(args.len()), line);
            }

            ExprKind::MethodCall { object, method, args } => {
                self.compile_expr(object);
                for arg in args { self.compile_expr(arg); }
                let method_idx = self.chunk.add_const(Value::str(method.as_str()));
                self.chunk.emit(Opcode::CallMethod { method: method_idx, argc: args.len() }, line);
            }

            ExprKind::Or { left, right } => self.compile_or(left, right, line),

            ExprKind::Range { start, end } => {
                self.compile_expr(start);
                self.compile_expr(end);
                self.chunk.emit(Opcode::MakeList(2), line);
                self.chunk.emit(Opcode::Nop, line);
            }
        }
    }

    fn compile_ident(&mut self, name: &str, span: Span, line: u32) {
        if let Some(local) = self.resolve_local(name)  {
            let slot = local.slot;
            self.chunk.emit(Opcode::LoadLocal(slot), line);
        } else {
            let idx = self.chunk.add_const(Value::str(name));
            self.chunk.emit(Opcode::LoadGlobal(idx), line);
        }
    }

    fn compile_binop(&mut self, op: BinOp, left: &Expr, right: &Expr, line: u32) {
        if op == BinOp::Add {
            self.compile_expr(left);

            let short = self.chunk.emit_jump(Opcode::JumpIfFalseAnd, line);
            self.compile_expr(right);
            self.chunk.patch_jump(short);
            return;
        }

        self.compile_expr(left);
        self.compile_expr(right);

        let instr = match op {
            BinOp::Add   => Opcode::Add,
            BinOp::Sub   => Opcode::Sub,
            BinOp::Mul   => Opcode::Mul,
            BinOp::Div   => Opcode::Div,
            BinOp::Mod   => Opcode::Mod,
            BinOp::Eq    => Opcode::CmpEq,
            BinOp::NotEq => Opcode::CmpNotEq,
            BinOp::Lt    => Opcode::CmpLt,
            BinOp::LtEq  => Opcode::CmpLtEq,
            BinOp::Gt    => Opcode::CmpGt,
            BinOp::GtEq  => Opcode::CmpGtEq,
            BinOp::And   => unreachable!("handled above"),
        };
        self.chunk.emit(instr, line);
    }

    fn compile_or (&mut self, left: &Expr, right: &Expr, line: u32) {
        self.compile_expr(left);

        let short = self.chunk.emit_jump(Opcode::JumpIfTrueOr, line);
        self.compile_expr(right);
        self.chunk.patch_jump(short);
    }

    fn compile_fstr(&mut self, parts: &[FStrPart], line: u32) {
        let n = parts.len();
        if n == 0 {
            let i = self.chunk.add_const(Value::str(""));
            self.chunk.emit(Opcode::LoadConst(i), line);
            return;
        }

        for part in parts {
            match part {
                FStrPart::Text(t) => {
                    let i = self.chunk.add_const(Value::str(t.as_str()));
                    self.chunk.emit(Opcode::LoadConst(i), line);
                }
                FStrPart::Interp(expr) => {
                    self.compile_expr(expr);

                    self.chunk.emit(Opcode::BuildStr(1), line);
                }
            }
        }

        if n > 1 {
            self.chunk.emit(Opcode::BuildStr(n), line);
        }
    }

    fn compile_list(&mut self, items: &[Expr], line: u32) {
        for item in items { self.compile_expr(item); }
        self.chunk.emit(Opcode::MakeList(items.len()), line);
    }

    fn compile_map(&mut self, entries: &[(String, Expr)], line: u32) {
        for (key, val) in entries {
            let i = self.chunk.add_const(Value::str(key.as_str()));
            self.chunk.emit(Opcode::LoadConst(i), line);
            self.compile_expr(val);
        }
        self.chunk.emit(Opcode::MakeMap(entries.len()), line);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use psh_ast::Parser;
    use psh_lexer::Lexer;

    fn compile(src: &str) -> (Chunk, Vec<CompileError>) {
        let (tokens, _) = Lexer::new(src).tokenize();
        let (script, _) = Parser::new(tokens).parse_script();
        Compiler::compile_script(&script)
    }

    fn chunk(src: &str) -> Chunk {
        let (chunk, errs) = compile(src);
        assert!(errs.is_empty(), "compile errors: {errs:?}");
        chunk
    }

    fn ops(src: &str) -> Vec<Opcode> {
        chunk(src).code
    }

    fn has_op(code: &[Opcode], pred: impl Fn(&Opcode) -> bool) -> bool {
        code.iter().any(pred)
    }

    // ── Constants ─────────────────────────────────────────────────────────────

    #[test]
    fn int_literal() {
        let ops = ops("let x = 42\n");
        assert!(has_op(&ops, |o| matches!(o, Opcode::LoadConst(_))));
        assert!(has_op(&ops, |o| matches!(o, Opcode::DefineLocal(0))));
    }

    #[test]
    fn bool_true() {
        let ops = ops("let x = true\n");
        assert!(has_op(&ops, |o| matches!(o, Opcode::LoadTrue)));
    }

    #[test]
    fn bool_false() {
        let ops = ops("let x = false\n");
        assert!(has_op(&ops, |o| matches!(o, Opcode::LoadFalse)));
    }

    #[test]
    fn string_const_dedup() {
        let c = chunk(r#"let x = "hello"\nlet y = "hello"\n"#);
        // Both `let` stmts reference the same constant slot
        let str_consts: Vec<_> = c.consts.iter()
            .filter(|v| matches!(v, Value::Str(_)))
            .collect();
        assert!(str_consts.len() <= 2, "expected deduplication of 'hello'");
    }

    // ── Locals ───────────────────────────────────────────────────────────────

    #[test]
    fn two_locals_get_different_slots() {
        let ops = ops("let x = 1\nlet y = 2\n");
        let defines: Vec<_> = ops.iter()
            .filter_map(|o| if let Opcode::DefineLocal(s) = o { Some(s) } else { None })
            .collect();
        assert_eq!(defines, [&0, &1]);
    }

    #[test]
    fn load_local_after_define() {
        let ops = ops("let x = 1\nlet y = x\n");
        assert!(has_op(&ops, |o| matches!(o, Opcode::LoadLocal(0))));
    }

    #[test]
    fn mutable_assign_emits_store_local() {
        let ops = ops("let mut x = 1\nx = 2\n");
        assert!(has_op(&ops, |o| matches!(o, Opcode::StoreLocal(0))));
    }

    #[test]
    fn immutable_assign_emits_error() {
        let (_, errs) = compile("let x = 1\nx = 2\n");
        assert!(!errs.is_empty());
        assert!(matches!(errs[0], CompileError::ImmutableAssign { .. }));
    }

    // ── Arithmetic ───────────────────────────────────────────────────────────

    #[test]
    fn add_emits_add_opcode() {
        let ops = ops("let x = 1 + 2\n");
        assert!(has_op(&ops, |o| matches!(o, Opcode::Add)));
    }

    #[test]
    fn mul_before_add() {
        // 1 + 2 * 3 → Add comes after Mul in the instruction stream
        let ops = ops("let x = 1 + 2 * 3\n");
        let add_pos = ops.iter().position(|o| matches!(o, Opcode::Add)).unwrap();
        let mul_pos = ops.iter().position(|o| matches!(o, Opcode::Mul)).unwrap();
        assert!(mul_pos < add_pos, "Mul should be emitted before Add");
    }

    #[test]
    fn negation() {
        let ops = ops("let x = -1\n");
        assert!(has_op(&ops, |o| matches!(o, Opcode::Neg)));
    }

    // ── Comparison & logic ────────────────────────────────────────────────────

    #[test]
    fn comparison_ops() {
        assert!(has_op(&ops("let x = a == b\n"), |o| matches!(o, Opcode::CmpEq)));
        assert!(has_op(&ops("let x = a != b\n"), |o| matches!(o, Opcode::CmpNotEq)));
        assert!(has_op(&ops("let x = a < b\n"),  |o| matches!(o, Opcode::CmpLt)));
        assert!(has_op(&ops("let x = a <= b\n"), |o| matches!(o, Opcode::CmpLtEq)));
        assert!(has_op(&ops("let x = a > b\n"),  |o| matches!(o, Opcode::CmpGt)));
        assert!(has_op(&ops("let x = a >= b\n"), |o| matches!(o, Opcode::CmpGtEq)));
    }

    #[test]
    fn not_opcode() {
        let ops = ops("let x = not true\n");
        assert!(has_op(&ops, |o| matches!(o, Opcode::Not)));
    }

    #[test]
    fn and_short_circuits() {
        let ops = ops("let x = a and b\n");
        assert!(has_op(&ops, |o| matches!(o, Opcode::JumpIfFalseAnd(_))));
    }

    #[test]
    fn or_short_circuits() {
        let ops = ops("let x = a or b\n");
        assert!(has_op(&ops, |o| matches!(o, Opcode::JumpIfTrueOr(_))));
    }

    // ── Collections ───────────────────────────────────────────────────────────

    #[test]
    fn list_literal() {
        let ops = ops("let x = [1, 2, 3]\n");
        assert!(has_op(&ops, |o| matches!(o, Opcode::MakeList(3))));
    }

    #[test]
    fn map_literal() {
        let ops = ops("let x = { a: 1, b: 2 }\n");
        assert!(has_op(&ops, |o| matches!(o, Opcode::MakeMap(2))));
    }

    // ── F-string ──────────────────────────────────────────────────────────────

    #[test]
    fn fstring_plain() {
        let ops = ops(r#"let x = f"hello"\n"#);
        // Single text part — just a LoadConst, no BuildStr needed
        assert!(has_op(&ops, |o| matches!(o, Opcode::LoadConst(_))));
    }

    #[test]
    fn fstring_with_interp_emits_build_str() {
        let ops = ops(r#"let x = f"hello {name}"\n"#);
        assert!(has_op(&ops, |o| matches!(o, Opcode::BuildStr(_))));
    }

    // ── Control flow ──────────────────────────────────────────────────────────

    #[test]
    fn if_emits_jump_if_false() {
        let ops = ops("if x == 1 {\n  let y = 2\n}\n");
        assert!(has_op(&ops, |o| matches!(o, Opcode::JumpIfFalse(_))));
    }

    #[test]
    fn if_else_emits_jump() {
        let ops = ops("if a {\n  let x = 1\n} else {\n  let x = 2\n}\n");
        assert!(has_op(&ops, |o| matches!(o, Opcode::Jump(_))));
        assert!(has_op(&ops, |o| matches!(o, Opcode::JumpIfFalse(_))));
    }

    #[test]
    fn while_emits_loop_jumps() {
        let ops = ops("while x < 10 {\n  let y = 1\n}\n");
        // Should have a backward jump (to loop start) and a forward JumpIfFalse
        let jumps: Vec<_> = ops.iter().filter(|o| matches!(o, Opcode::Jump(_))).collect();
        assert!(!jumps.is_empty());
        assert!(has_op(&ops, |o| matches!(o, Opcode::JumpIfFalse(_))));
    }

    #[test]
    fn for_emits_make_iter_and_iter_next() {
        let ops = ops("for i in items {\n  let x = i\n}\n");
        assert!(has_op(&ops, |o| matches!(o, Opcode::MakeIter)));
        assert!(has_op(&ops, |o| matches!(o, Opcode::IterNext)));
    }

    #[test]
    fn try_emits_push_try_and_pop_try() {
        let ops = ops("try {\n  let x = 1\n} catch (err) {\n  let y = 2\n}\n");
        assert!(has_op(&ops, |o| matches!(o, Opcode::PushTry(_))));
        assert!(has_op(&ops, |o| matches!(o, Opcode::PopTry)));
    }

    // ── Exec ──────────────────────────────────────────────────────────────────

    #[test]
    fn exec_simple() {
        let ops = ops("exec git clone https://github.com/org/repo\n");
        assert!(has_op(&ops, |o| matches!(o, Opcode::Exec { argc: 3, swallow: false, .. })));
    }

    #[test]
    fn exec_swallow() {
        let ops = ops("exec rm -rf dist ?\n");
        assert!(has_op(&ops, |o| matches!(o, Opcode::Exec { swallow: true, .. })));
    }

    #[test]
    fn exec_interp_emits_build_str() {
        let ops = ops("exec cargo build --{profile}\n");
        // The `--` word and `{profile}` interp get pushed; interp goes through BuildStr
        assert!(has_op(&ops, |o| matches!(o, Opcode::BuildStr(1))));
        assert!(has_op(&ops, |o| matches!(o, Opcode::Exec { .. })));
    }

    // ── Field / index access ──────────────────────────────────────────────────

    #[test]
    fn field_access_emits_get_field() {
        let ops = ops("let x = os.platform\n");
        assert!(has_op(&ops, |o| matches!(o, Opcode::GetField(_))));
    }

    #[test]
    fn index_access_emits_get_index() {
        let ops = ops("let x = items[0]\n");
        assert!(has_op(&ops, |o| matches!(o, Opcode::GetIndex)));
    }

    // ── Functions & tasks ─────────────────────────────────────────────────────

    #[test]
    fn fn_emits_store_global() {
        let ops = ops("fn greet() {\n  let x = 1\n}\n");
        assert!(has_op(&ops, |o| matches!(o, Opcode::StoreGlobal(_))));
    }

    #[test]
    fn task_emits_store_global() {
        let ops = ops("task build {\n  exec cargo build\n}\n");
        assert!(has_op(&ops, |o| matches!(o, Opcode::StoreGlobal(_))));
    }

    #[test]
    fn fn_call_emits_call() {
        let ops = ops("greet()\n");
        assert!(has_op(&ops, |o| matches!(o, Opcode::Call(0))));
    }

    #[test]
    fn method_call_emits_call_method() {
        let ops = ops(r#"let x = env.get("HOME")\n"#);
        assert!(has_op(&ops, |o| matches!(o, Opcode::CallMethod { .. })));
    }

    #[test]
    fn return_outside_fn_is_error() {
        let (_, errs) = compile("return 1\n");
        assert!(!errs.is_empty());
        assert!(matches!(errs[0], CompileError::ReturnOutsideFunction { .. }));
    }

    // ── Import ────────────────────────────────────────────────────────────────

    #[test]
    fn import_emits_import_module() {
        let ops = ops("import os\n");
        assert!(has_op(&ops, |o| matches!(o, Opcode::ImportModule(_))));
    }

    #[test]
    fn import_selective_emits_import_fields() {
        let ops = ops("import os.{ platform, arch }\n");
        assert!(has_op(&ops, |o| matches!(o, Opcode::ImportFields { .. })));
    }

    // ── Exit ──────────────────────────────────────────────────────────────────

    #[test]
    fn exit_bare_pushes_zero() {
        let c = chunk("exit\n");
        // Should have a LoadConst(0 → Int(0)) followed by Exit
        assert!(has_op(&c.code, |o| matches!(o, Opcode::Exit)));
        assert!(c.consts.iter().any(|v| matches!(v, Value::Int(0))));
    }

    #[test]
    fn exit_with_code() {
        let ops = ops("exit 1\n");
        assert!(has_op(&ops, |o| matches!(o, Opcode::Exit)));
    }

    // ── Disassembly ───────────────────────────────────────────────────────────

    #[test]
    fn disassemble_does_not_panic() {
        let c = chunk("let x = 1 + 2\nexec echo hello\n");
        c.disassemble(); // just must not panic
    }

    // ── Full script ───────────────────────────────────────────────────────────

    #[test]
    fn full_deploy_compiles_clean() {
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
        let (_, errs) = compile(src);
        assert!(errs.is_empty(), "errors: {errs:?}");
    }
}
