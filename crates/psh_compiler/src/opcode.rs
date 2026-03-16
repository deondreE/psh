
/// Instruction pointer offset for jump targets.
pub type JumpOffset = usize;

/// Index into the chunk's constant pool.
pub type ConstIdx = usize;

/// Slot index in the current call frame's local variable array.
pub type LocalIdx = usize;

#[derive(Debug, Clone, PartialEq)]
pub enum Opcode {
    // ── Constants ─────────────────────────────────────────────────────────────
    /// Push a value from the constant pool.   ( -- val )
    LoadConst(ConstIdx),

    /// Push nil.                               ( -- nil )
    LoadNil,

    /// Push true.                              ( -- true )
    LoadTrue,

    /// Push false.                             ( -- false )
    LoadFalse,

    // ── Locals ────────────────────────────────────────────────────────────────
    /// Push the value of a local variable.    ( -- val )
    LoadLocal(LocalIdx),

    /// Pop and store into a local slot.        ( val -- )
    StoreLocal(LocalIdx),

    /// Declare a new local slot (initialised from top of stack).  ( val -- )
    DefineLocal(LocalIdx),

    // ── Globals (tasks, fns, modules) ─────────────────────────────────────────
    /// Push a named global value (fn/task/module).   ( -- val )
    LoadGlobal(ConstIdx),   // ConstIdx points to a Str name in the pool

    /// Bind top of stack to a global name.            ( val -- )
    StoreGlobal(ConstIdx),

    // ── Stack ops ─────────────────────────────────────────────────────────────
    /// Discard the top of the stack.           ( val -- )
    Pop,

    /// Duplicate the top of the stack.        ( val -- val val )
    Dup,

    // ── Arithmetic ────────────────────────────────────────────────────────────
    /// ( a b -- a+b )
    Add,
    /// ( a b -- a-b )
    Sub,
    /// ( a b -- a*b )
    Mul,
    /// ( a b -- a/b )
    Div,
    /// ( a b -- a%b )
    Mod,
    /// ( a -- -a )
    Neg,

    // ── Comparison ────────────────────────────────────────────────────────────
    /// ( a b -- a==b )
    CmpEq,
    /// ( a b -- a!=b )
    CmpNotEq,
    /// ( a b -- a<b )
    CmpLt,
    /// ( a b -- a<=b )
    CmpLtEq,
    /// ( a b -- a>b )
    CmpGt,
    /// ( a b -- a>=b )
    CmpGtEq,

    // ── Logic ─────────────────────────────────────────────────────────────────
    /// ( a -- !a )
    Not,

    /// Short-circuit AND:
    /// If top is falsy, jump to `offset` (leaving false on stack).
    /// Otherwise pop and continue.             ( a -- )
    JumpIfFalseAnd(JumpOffset),

    /// Short-circuit OR / fallback:
    /// If top is truthy, jump to `offset` (leaving value on stack).
    /// Otherwise pop and continue.             ( a -- )
    JumpIfTrueOr(JumpOffset),

    // ── Control flow ──────────────────────────────────────────────────────────
    /// Unconditional jump.
    Jump(JumpOffset),

    /// Pop top; jump if falsy.                 ( cond -- )
    JumpIfFalse(JumpOffset),

    /// Pop top; jump if truthy.                ( cond -- )
    JumpIfTrue(JumpOffset),

    // ── Iterators ─────────────────────────────────────────────────────────────
    /// Push an iterator over the value on top. ( iterable -- iterator )
    MakeIter,

    /// Advance iterator: push next value and true, or push nil and false.
    /// Used to drive for-loops.                ( iter -- iter val ok )
    IterNext,

    // ── Field / index access ──────────────────────────────────────────────────
    /// ( obj -- obj.field )   field name is ConstIdx into pool
    GetField(ConstIdx),

    /// ( obj val -- )         field name is ConstIdx into pool
    SetField(ConstIdx),

    /// ( obj idx -- obj[idx] )
    GetIndex,

    /// ( obj idx val -- )
    SetIndex,

    // ── Collections ───────────────────────────────────────────────────────────
    /// Pop `n` values and build a List.        ( v0..vN -- list )
    MakeList(usize),

    /// Pop `n` key-value pairs and build a Map. Keys are Str on the stack.
    /// Stack layout: k0 v0 k1 v1 … kN vN (kN vN on top)
    /// ( k0 v0 .. kN vN -- map )
    MakeMap(usize),

    // ── String building ───────────────────────────────────────────────────────
    /// Pop `n` values, coerce each to string, concatenate.
    /// Used to build f-strings.               ( s0..sN -- str )
    BuildStr(usize),

    // ── Calls ─────────────────────────────────────────────────────────────────
    /// Call the callee with `argc` arguments.
    /// Stack before:  callee  arg0  arg1 … argN   (argN on top)
    /// Stack after:   return_value
    Call(usize),

    /// Call a method on an object.
    /// Stack before:  object  arg0 … argN
    /// Stack after:   return_value
    /// The method name is a ConstIdx into the pool.
    CallMethod { method: ConstIdx, argc: usize },

    /// Return from the current call frame.    ( val -- )  [to caller]
    Return,

    /// Return Nil from the current frame (for void functions/tasks).
    ReturnNil,

    // ── Exec ──────────────────────────────────────────────────────────────────
    /// Execute a shell command.
    /// Stack before: word0  word1 … wordN   (wordN on top, all Str)
    /// Stack after:  Str(stdout)  if captured; else Nil
    /// `argc`  = number of words
    /// `swallow` = if true, ignore non-zero exit code
    /// `capture`  = if true, capture stdout into a Str value
    Exec { argc: usize, swallow: bool, capture: bool },

    // ── Modules ───────────────────────────────────────────────────────────────
    /// Probe and push a system module by name.  ( -- module )
    /// The module name is a ConstIdx.
    ImportModule(ConstIdx),

    /// Import specific fields from a module into locals.
    /// `module` = ConstIdx of the module name string
    /// `fields` = list of (field_name ConstIdx, local_slot LocalIdx)
    ImportFields { module: ConstIdx, fields: Vec<(ConstIdx, LocalIdx)> },

    // ── Exit ──────────────────────────────────────────────────────────────────
    /// Exit the process.                       ( code -- )  code is Int
    Exit,

    // ── Error handling ────────────────────────────────────────────────────────
    /// Push an error handler frame. On error, jump to `handler` offset
    /// and push the error value onto the stack.
    PushTry(JumpOffset),

    /// Pop the error handler frame (end of try body without error).
    PopTry,

    /// Re-raise the current error (used inside catch when re-throwing).
    Throw,

    // ── Debug ─────────────────────────────────────────────────────────────────
    /// No-op. Used as a placeholder during compilation (e.g. before patching
    /// jump targets).
    Nop,
}

impl std::fmt::Display for Opcode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Opcode::LoadConst(i)      => write!(f, "LOAD_CONST    {i}"),
            Opcode::LoadNil           => write!(f, "LOAD_NIL"),
            Opcode::LoadTrue          => write!(f, "LOAD_TRUE"),
            Opcode::LoadFalse         => write!(f, "LOAD_FALSE"),
            Opcode::LoadLocal(i)      => write!(f, "LOAD_LOCAL    {i}"),
            Opcode::StoreLocal(i)     => write!(f, "STORE_LOCAL   {i}"),
            Opcode::DefineLocal(i)    => write!(f, "DEFINE_LOCAL  {i}"),
            Opcode::LoadGlobal(i)     => write!(f, "LOAD_GLOBAL   {i}"),
            Opcode::StoreGlobal(i)    => write!(f, "STORE_GLOBAL  {i}"),
            Opcode::Pop               => write!(f, "POP"),
            Opcode::Dup               => write!(f, "DUP"),
            Opcode::Add               => write!(f, "ADD"),
            Opcode::Sub               => write!(f, "SUB"),
            Opcode::Mul               => write!(f, "MUL"),
            Opcode::Div               => write!(f, "DIV"),
            Opcode::Mod               => write!(f, "MOD"),
            Opcode::Neg               => write!(f, "NEG"),
            Opcode::CmpEq             => write!(f, "CMP_EQ"),
            Opcode::CmpNotEq          => write!(f, "CMP_NEQ"),
            Opcode::CmpLt             => write!(f, "CMP_LT"),
            Opcode::CmpLtEq           => write!(f, "CMP_LTEQ"),
            Opcode::CmpGt             => write!(f, "CMP_GT"),
            Opcode::CmpGtEq           => write!(f, "CMP_GTEQ"),
            Opcode::Not               => write!(f, "NOT"),
            Opcode::JumpIfFalseAnd(o) => write!(f, "JUMP_AND      {o}"),
            Opcode::JumpIfTrueOr(o)   => write!(f, "JUMP_OR       {o}"),
            Opcode::Jump(o)           => write!(f, "JUMP          {o}"),
            Opcode::JumpIfFalse(o)    => write!(f, "JUMP_FALSE    {o}"),
            Opcode::JumpIfTrue(o)     => write!(f, "JUMP_TRUE     {o}"),
            Opcode::MakeIter          => write!(f, "MAKE_ITER"),
            Opcode::IterNext          => write!(f, "ITER_NEXT"),
            Opcode::GetField(i)       => write!(f, "GET_FIELD     {i}"),
            Opcode::SetField(i)       => write!(f, "SET_FIELD     {i}"),
            Opcode::GetIndex          => write!(f, "GET_INDEX"),
            Opcode::SetIndex          => write!(f, "SET_INDEX"),
            Opcode::MakeList(n)       => write!(f, "MAKE_LIST     {n}"),
            Opcode::MakeMap(n)        => write!(f, "MAKE_MAP      {n}"),
            Opcode::BuildStr(n)       => write!(f, "BUILD_STR     {n}"),
            Opcode::Call(n)           => write!(f, "CALL          {n}"),
            Opcode::CallMethod { method, argc } => write!(f, "CALL_METHOD   method={method} argc={argc}"),
            Opcode::Return            => write!(f, "RETURN"),
            Opcode::ReturnNil         => write!(f, "RETURN_NIL"),
            Opcode::Exec { argc, swallow, capture } =>
                write!(f, "EXEC          argc={argc} swallow={swallow} capture={capture}"),
            Opcode::ImportModule(i)   => write!(f, "IMPORT_MODULE {i}"),
            Opcode::ImportFields { module, fields } =>
                write!(f, "IMPORT_FIELDS module={module} n={}", fields.len()),
            Opcode::Exit              => write!(f, "EXIT"),
            Opcode::PushTry(o)        => write!(f, "PUSH_TRY      {o}"),
            Opcode::PopTry            => write!(f, "POP_TRY"),
            Opcode::Throw             => write!(f, "THROW"),
            Opcode::Nop               => write!(f, "NOP"),
        }
    }
}
