pub struct Output {
    pub kind: OutputKind,
    original_output: String,
}

pub enum OutputKind {
    Intermediate,
    Optimal,
    Unsatisfiable,
    Unbounded,
    Unknown,
}

pub struct Solution {
    pub solution: String,
    pub objective: isize,
}

impl Output {
    const SOLUTION_TERMINATOR: &str = "----------";
    const DONE_TERMINATOR: &str = "==========";
    const UNSATISFIABLE_TERMINATOR: &str = "=====UNSATISFIABLE=====";
    const UNBOUNDED_TERMINATOR: &str = "=====UNBOUNDED=====";
    const UNKNOWN_TERMINATOR: &str = "=====UNKNOWN=====";

    fn new(kind: OutputKind, original_output: String) -> Self {
        Self {
            kind,
            original_output,
        }
    }

    pub fn parse(output: &str) -> Self {
        let s = output.trim();
        if s.ends_with(Self::DONE_TERMINATOR) {
            return Self::new(OutputKind::Optimal, s.to_owned());
        } else if s.ends_with(Self::UNSATISFIABLE_TERMINATOR) {
            return Self::new(OutputKind::Unsatisfiable, s.to_owned());
        } else if s.ends_with(Self::UNBOUNDED_TERMINATOR) {
            return Self::new(OutputKind::Unbounded, s.to_owned());
        } else if s.ends_with(Self::UNKNOWN_TERMINATOR) {
            return Self::new(OutputKind::Unknown, s.to_owned());
        } else {
            return Self::new(OutputKind::Intermediate, s.to_owned());
        }
    }

    pub fn solutions(&self) -> Vec<Solution> {
        // Lazy load solutions (no need to parse if we're not interested)
        todo!()
    }
}
