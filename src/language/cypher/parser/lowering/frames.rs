use crate::language::cypher::ast::{
    Clause, Expr, Literal, PatternElement, PatternElementChain, PatternPart, ProjectionBody,
    ProjectionItem, Query, SortItem,
};

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum LoweringFrame {
    Query(Query),
    Clause(Clause),
    PatternPart(PatternPart),
    PatternElement(PatternElement),
    PatternChain(PatternElementChain),
    ProjectionBody(ProjectionBody),
    ProjectionItem(ProjectionItem),
    SortItem(SortItem),
    Expr(Expr),
    Literal(Literal),
    Name(String),
}

#[derive(Debug, Default)]
pub(crate) struct FrameStack {
    frames: Vec<LoweringFrame>,
}

impl FrameStack {
    pub(crate) fn push(&mut self, frame: LoweringFrame) {
        self.frames.push(frame);
    }

    pub(crate) fn pop(&mut self) -> Option<LoweringFrame> {
        self.frames.pop()
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.frames.is_empty()
    }
}
