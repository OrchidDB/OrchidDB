use super::{Expr, PatternPart};

#[derive(Debug, Clone, PartialEq)]
pub enum Clause {
    Match(MatchClause),
    Unwind(UnwindClause),
    Call(ProcedureCallClause),
    Create(CreateClause),
    Set(SetClause),
    Delete(DeleteClause),
    With(WithClause),
    Return(ReturnClause),
}

#[derive(Debug, Clone, PartialEq)]
pub struct MatchClause {
    pub optional: bool,
    pub patterns: Vec<PatternPart>,
    pub predicate: Option<Expr>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct UnwindClause {
    pub expr: Expr,
    pub alias: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ProcedureCallClause {
    pub name: String,
    pub args: Vec<Expr>,
    pub yields: Vec<ProcedureYieldItem>,
    pub yield_all: bool,
    pub predicate: Option<Expr>,
    pub standalone: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcedureYieldItem {
    pub field: String,
    pub alias: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CreateClause {
    pub patterns: Vec<PatternPart>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SetClause {
    pub items: Vec<SetItem>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum SetItem {
    Property {
        target: Expr,
        key: String,
        value: Expr,
    },
    Replace {
        variable: String,
        value: Expr,
    },
    Merge {
        variable: String,
        value: Expr,
    },
    Labels {
        variable: String,
        labels: Vec<String>,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub struct DeleteClause {
    pub detach: bool,
    pub expressions: Vec<Expr>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct WithClause {
    pub projection: ProjectionBody,
    pub predicate: Option<Expr>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ReturnClause {
    pub projection: ProjectionBody,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ProjectionBody {
    pub distinct: bool,
    pub include_existing: bool,
    pub items: Vec<ProjectionItem>,
    pub order_by: Vec<SortItem>,
    pub skip: Option<Expr>,
    pub limit: Option<Expr>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ProjectionItem {
    pub expr: Expr,
    pub alias: Option<String>,
    pub explicit_alias: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SortItem {
    pub expr: Expr,
    pub direction: SortDirection,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SortDirection {
    Asc,
    Desc,
}
