use super::Clause;

#[derive(Debug, Clone, PartialEq)]
pub struct Query {
    pub clauses: Vec<Clause>,
    pub unions: Vec<UnionBranch>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct UnionBranch {
    pub all: bool,
    pub query: Box<Query>,
}

impl Query {
    pub fn new(clauses: Vec<Clause>) -> Self {
        Self {
            clauses,
            unions: Vec::new(),
        }
    }

    pub fn with_unions(mut self, unions: Vec<UnionBranch>) -> Self {
        self.unions = unions;
        self
    }
}
