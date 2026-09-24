//! Comparison, range, and text predicate lowering.

use crate::grammar::generated::gremlin::gremlinvisitor::GremlinVisitor;

use super::{CompareOp, GremlinError, LoweringVisitor, Predicate, Rc, TextKind};
use crate::grammar::generated::gremlin::gremlinparser::*;
#[allow(non_snake_case)]
impl LoweringVisitor {
    pub(super) fn lower_range_predicate<'input>(
        &mut self,
        args: Vec<Rc<GenericArgumentContextAll<'input>>>,
        inclusive_lo: bool,
        inclusive_hi: bool,
        name: &str,
    ) {
        if args.len() != 2 {
            self.fail(GremlinError::Parse(format!(
                "{name}() expected two arguments"
            )));
            return;
        }
        let mut iter = args.into_iter();
        let lo_ctx = iter.next().unwrap();
        let hi_ctx = iter.next().unwrap();
        self.visit_genericArgument(&lo_ctx);
        let Some(lo) = self.pop_value() else { return };
        self.visit_genericArgument(&hi_ctx);
        let Some(hi) = self.pop_value() else { return };
        self.predicate_stack.push(Predicate::Range {
            lo,
            hi,
            inclusive_lo,
            inclusive_hi,
        });
    }

    pub(super) fn lower_outside_predicate<'input>(
        &mut self,
        args: Vec<Rc<GenericArgumentContextAll<'input>>>,
    ) {
        if args.len() != 2 {
            self.fail(GremlinError::Parse(
                "outside() expected two arguments".to_string(),
            ));
            return;
        }
        let mut iter = args.into_iter();
        let lo_ctx = iter.next().unwrap();
        let hi_ctx = iter.next().unwrap();
        self.visit_genericArgument(&lo_ctx);
        let Some(lo) = self.pop_value() else { return };
        self.visit_genericArgument(&hi_ctx);
        let Some(hi) = self.pop_value() else { return };
        self.predicate_stack.push(Predicate::Outside { lo, hi });
    }

    pub(super) fn lower_text_predicate<'input>(
        &mut self,
        arg: Option<Rc<StringArgumentContextAll<'input>>>,
        kind: TextKind,
        negated: bool,
    ) {
        let Some(arg_ctx) = arg else {
            self.fail(GremlinError::Parse(
                "text predicate missing argument".to_string(),
            ));
            return;
        };
        let pattern = match self.string_argument_text(&arg_ctx) {
            Some(s) => s,
            None => {
                // visit pushed an error; bail.
                return;
            }
        };
        let predicate = Predicate::TextLike { pattern, kind };
        let predicate = if negated {
            Predicate::Not(Box::new(predicate))
        } else {
            predicate
        };
        self.predicate_stack.push(predicate);
    }

    pub(super) fn lower_regex_predicate<'input>(
        &mut self,
        arg: Option<Rc<StringArgumentContextAll<'input>>>,
        negated: bool,
    ) {
        let Some(arg_ctx) = arg else {
            self.fail(GremlinError::Parse(
                "regex predicate missing argument".to_string(),
            ));
            return;
        };
        let pattern = match self.string_argument_text(&arg_ctx) {
            Some(s) => s,
            None => return,
        };
        let predicate = Predicate::Regex(pattern);
        let predicate = if negated {
            Predicate::Not(Box::new(predicate))
        } else {
            predicate
        };
        self.predicate_stack.push(predicate);
    }

    pub(super) fn push_compare_predicate<'input>(
        &mut self,
        op: CompareOp,
        arg: Option<Rc<GenericArgumentContextAll<'input>>>,
        name: &str,
    ) {
        let Some(arg_ctx) = arg else {
            self.fail(GremlinError::Parse(format!("{name}() missing argument")));
            return;
        };
        self.visit_genericArgument(&arg_ctx);
        let Some(value) = self.pop_value() else {
            return;
        };
        self.predicate_stack.push(Predicate::Compare { op, value });
    }
}
