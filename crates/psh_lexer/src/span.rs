
/// A byte-offset range into the source string.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Span {
    pub start: usize,
    pub end: usize,
}

impl Span {
    #[inline] pub fn new(start: usize, end: usize) -> Self { Self { start, end } }

    #[inline] pub fn len(&self) -> usize {
        self.end - self.start
    }

    #[inline] pub fn is_empty(&self) -> bool {
       self.start == self.end
    }

    /// Extend this span to cover `other` as well.
    pub fn merge(self, other: Span) -> Span {
        Span {
            start: self.start.min(other.start),
            end: self.end.max(other.end),
        }
    }

    /// Return the (1-based line, 1-based col)
    pub fn location(&self, src: &str) -> (usize, usize) {
        let mut line = 1usize;
        let mut col = 1usize;
        for (i, ch) in src.char_indices() {
            if i == self.start { break; }
            if ch == '\n' { line += 1; col = 1; } else { col += 1; }
        }

        (line, col)
    }
}

impl std::fmt::Display for Span {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}..{}", self.start, self.end)
    }
}
