use super::Step;

#[derive(Debug, Clone, PartialEq)]
pub struct Traversal {
    pub steps: Vec<Step>,
}

impl Traversal {
    pub fn new(steps: Vec<Step>) -> Self {
        Self { steps }
    }
}
