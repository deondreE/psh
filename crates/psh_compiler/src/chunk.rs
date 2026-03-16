use crate::{Opcode, Value};

#[derive(Debug, Clone)]
pub struct Chunk {
    /// Compiled instructions.
    pub code:   Vec<Opcode>,
    /// Constant pool — literals referenced by index from opcodes.
    pub consts: Vec<Value>,
    /// Source lines parallel to `code` (for error reporting).
    pub lines:  Vec<u32>,
    /// Human-readable name (function/task name, or "<script>").
    pub name:   String,
}

impl Chunk {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            code: Vec::new(),
            consts: Vec::new(),
            lines: Vec::new(),
            name: name.into(),
        }
    }

    /// Append an instruction, recording its source line.
    pub fn emit(&mut self, op: Opcode, line: u32) -> usize {
        let idx = self.code.len();
        self.code.push(op);
        self.lines.push(line);
        idx
    }

    /// Append a constant to the pool, returning its index.
    /// Deduplicates strings to keep the pool compact.
    pub fn add_const(&mut self, val: Value) -> usize {
        if let Value::Str(ref s) = val {
           if let Some(idx) = self.consts.iter().position(|c| {
                if let Value::Str(cs) = c { cs == s } else { false }
                }) {
                return idx;
            }
        }
        let idx = self.consts.len();
        self.consts.push(val);
        idx
    }

    /// Emit a placeholder jump (target = 0). Returns the instruction index
    /// so it can be patched later with `patch_jump`.
    pub fn emit_jump(&mut self, op: fn(usize) -> Opcode, line: u32) -> usize {
        self.emit(op(0), line)
    }

    /// Patch a previously emitted jump so its target is the *current* end of
    /// the instruction stream.
    pub fn patch_jump(&mut self, jump_idx: usize) {
        let target = self.code.len();
        match &mut self.code[jump_idx] {
            Opcode::Jump(t)           => *t = target,
            Opcode::JumpIfFalse(t)    => *t = target,
            Opcode::JumpIfTrue(t)     => *t = target,
            Opcode::JumpIfFalseAnd(t) => *t = target,
            Opcode::JumpIfTrueOr(t)   => *t = target,
            Opcode::PushTry(t)        => *t = target,
            other => panic!("patch_jump called on non-jump: {other:?}"),
        }
    }

    /// Current instruction count (= next instruction index).
    pub fn len(&self) -> usize {
        self.code.len()
    }

    pub fn is_empty(&self) -> bool {
        self.code.is_empty()
    }

    /// Print a human-readable disassembly to stdout.
    pub fn disassemble(&self) {
        println!("=== {} ===", self.name);
        println!("constants ({}):", self.consts.len());
        for (i, c) in self.consts.iter().enumerate() {
            println!("  [{i:3}] {c}");
        }
        println!("code ({} instructions):", self.code.len());
        for (i, op) in self.code.iter().enumerate() {
            let line = self.lines.get(i).copied().unwrap_or(0);
            println!("  {i:4}  line {line:3}  {op}");
        }
        println!();
    }
}
