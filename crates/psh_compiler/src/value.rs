use std::collections::HashMap;
use std::fmt;
use std::rc::Rc;

use crate::Chunk;

#[derive(Debug, Clone)]
pub enum Value {
    Nil,
    Bool(bool),
    Int(i64),
    Float(f64),
    Str(Rc<String>),
    List(Rc<Vec<Value>>),
    Map(Rc<HashMap<String, Value>>),

    /// A compiled function: its parameter names + the chunck to execute.
    Function {
        name: Rc<String>,
        params: Rc<Vec<String>>,
        chunk: Rc<Chunk>,
    },

    /// A task is like a function but invokable from the CLI.
    Task {
        name: Rc<String>,
        chunk: Rc<Chunk>,
    },

    ///  A system module: a flat map of field name -> value.
    /// Populated at runtime by 'psh_modules'.
    Module(Rc<HashMap<String, Value>>),
}

impl Value {
    pub fn str(s: impl Into<String>) -> Value {
        Value::Str(Rc::new(s.into()))
    }

    pub fn list(v: Vec<Value>) -> Value {
        Value::List(Rc::new(v))
    }

    pub fn map(m: HashMap<String, Value>) -> Value {
        Value::Map(Rc::new(m))
    }

    /// PSH truthiness: nil, false, 0, "" are falsy; everything else truthy.
    pub fn is_truthy(&self) -> bool {
        match self {
            Value::Nil => false,
            Value::Bool(b) => *b,
            Value::Int(n) => *n != 0,
            Value::Float(f) => *f != 0.0,
            Value::Str(s) => !s.is_empty(),
            Value::List(l) => !l.is_empty(),
            Value::Map(m) => !m.is_empty(),
            _ => true
        }
    }

    /// Typename for error messages
    pub fn type_name(&self) -> &'static str {
        match self {
            Value::Nil        => "nil",
            Value::Bool(_)    => "bool",
            Value::Int(_)     => "int",
            Value::Float(_)   => "float",
            Value::Str(_)     => "string",
            Value::List(_)    => "list",
            Value::Map(_)     => "map",
            Value::Function { .. } => "function",
            Value::Task { .. }     => "task",
            Value::Module(_)  => "module",
        }
    }

    /// Coerce to string for display / f-string integration
    pub fn display(&self) -> String {
        match self {
            Value::Nil       => String::new(),
            Value::Bool(b)   => b.to_string(),
            Value::Int(n)    => n.to_string(),
            Value::Float(f)  => f.to_string(),
            Value::Str(s)    => s.as_ref().clone(),
            Value::List(l)   => {
                let items: Vec<_> = l.iter().map(Value::display).collect();
                format!("[{}]", items.join(", "))
            }
            Value::Map(m) => {
                let mut pairs: Vec<_> = m.iter()
                    .map(|(k, v)| format!("{k}: {}", v.display()))
                    .collect();
                pairs.sort();
                format!("{{{}}}", pairs.join(", "))
            }
            Value::Function { name, .. } => format!("<fn {name}>"),
            Value::Task     { name, .. } => format!("<task {name}>"),
            Value::Module(_)             => "<module>".into(),
        }
    }
}

impl PartialEq for Value {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Value::Nil,        Value::Nil)        => true,
            (Value::Bool(a),    Value::Bool(b))    => a == b,
            (Value::Int(a),     Value::Int(b))     => a == b,
            (Value::Float(a),   Value::Float(b))   => a == b,
            (Value::Int(a),     Value::Float(b))   => (*a as f64) == *b,
            (Value::Float(a),   Value::Int(b))     => *a == (*b as f64),
            (Value::Str(a),     Value::Str(b))     => a == b,
            (Value::List(a),    Value::List(b))    => a == b,
            _ => false,
        }
    }
}

impl PartialOrd for Value {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        match (self, other) {
            (Value::Int(a),   Value::Int(b))   => a.partial_cmp(b),
            (Value::Float(a), Value::Float(b)) => a.partial_cmp(b),
            (Value::Int(a),   Value::Float(b)) => (*a as f64).partial_cmp(b),
            (Value::Float(a), Value::Int(b))   => a.partial_cmp(&(*b as f64)),
            (Value::Str(a),   Value::Str(b))   => a.partial_cmp(b),
            _ => None,
        }
    }
}

impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.display())
    }
}
