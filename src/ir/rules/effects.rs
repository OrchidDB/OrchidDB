//! Conservative adapters for existing, successfully bound scalar proofs.
//! These are not Graph IR type inference: an unbound property or a sampled
//! value domain cannot establish these facts. Unknown is distinct from false.
use crate::ir::expr::{BinaryOp, IrExpr, Lit};
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct EffectFacts {
    pub may_throw: Option<bool>,
    pub volatile: Option<bool>,
    pub external_effects: Option<bool>,
    pub repeatable_within_scope: Option<bool>,
}
impl EffectFacts {
    pub const BOUND_TOTAL_REPEATABLE: Self = Self {
        may_throw: Some(false),
        volatile: Some(false),
        external_effects: Some(false),
        repeatable_within_scope: Some(true),
    };
    pub fn safe_to_duplicate(self) -> bool {
        self.may_throw == Some(false)
            && self.volatile == Some(false)
            && self.external_effects == Some(false)
            && self.repeatable_within_scope == Some(true)
    }
    pub fn safe_to_discard(self) -> bool {
        self.may_throw == Some(false)
            && self.volatile == Some(false)
            && self.external_effects == Some(false)
    }
}
/// Existing scalar stability proof. Use only after successful SQL binding and
/// domain validation; in particular this does not prove missing-property policy.
pub fn effects_for_bound_scalar(expr: &IrExpr) -> EffectFacts {
    let safe = match expr {
        IrExpr::Lit(_) | IrExpr::Binding(_) | IrExpr::Property { .. } => true,
        IrExpr::List(items) => items
            .iter()
            .all(|item| effects_for_bound_scalar(item).safe_to_duplicate()),
        _ => false,
    };
    if safe {
        EffectFacts::BOUND_TOTAL_REPEATABLE
    } else {
        EffectFacts::default()
    }
}
/// Wider membership proof, retaining its existing exact whitelist. The caller
/// provides the established scoped-transform proof; arbitrary lambdas remain
/// unknown. This does not infer equivalence from function volatility alone.
pub fn effects_for_bound_membership(
    expr: &IrExpr,
    scoped_transform: &impl Fn(&IrExpr) -> bool,
) -> EffectFacts {
    let safe_child =
        |expr| effects_for_bound_membership(expr, scoped_transform).safe_to_duplicate();
    let safe = match expr {
        IrExpr::Lit(_) | IrExpr::Binding(_) | IrExpr::Property { .. } => true,
        IrExpr::List(items) => items.iter().all(safe_child),
        IrExpr::ListTransform { .. } => scoped_transform(expr),
        IrExpr::Not(inner) | IrExpr::IsNull(inner) | IrExpr::IsNotNull(inner) => safe_child(inner),
        IrExpr::Binary { op, lhs, rhs }
            if matches!(
                op,
                BinaryOp::Eq
                    | BinaryOp::Neq
                    | BinaryOp::Lt
                    | BinaryOp::Lte
                    | BinaryOp::Gt
                    | BinaryOp::Gte
                    | BinaryOp::And
                    | BinaryOp::Or
            ) =>
        {
            safe_child(lhs) && safe_child(rhs)
        }
        IrExpr::Call { name, args }
            if matches!(
                name.as_str(),
                "cypher_eq"
                    | "cypher_neq"
                    | "cypher_lt"
                    | "cypher_lte"
                    | "cypher_gt"
                    | "cypher_gte"
                    | "xor"
            ) =>
        {
            args.iter().all(safe_child)
        }
        IrExpr::Call { name, args } if matches!(name.as_str(), "keys" | "labels") => {
            matches!(args.as_slice(), [IrExpr::Binding(_)])
        }
        IrExpr::Call { name, args } if name == "cypher_subscript" => {
            matches!(args.as_slice(), [target, IrExpr::Lit(Lit::Int(_))] if safe_child(target))
        }
        _ => false,
    };
    if safe {
        EffectFacts::BOUND_TOTAL_REPEATABLE
    } else {
        EffectFacts::default()
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn unknown_throwing_and_volatile_work_is_never_safe() {
        for name in ["rand", "clock", "callback", "to_integer", "divide"] {
            let call = IrExpr::Call {
                name: name.into(),
                args: vec![IrExpr::lit_int(1)],
            };
            assert!(!effects_for_bound_scalar(&call).safe_to_duplicate());
            assert!(!effects_for_bound_membership(&call, &|_| false).safe_to_discard());
        }
        let equality = IrExpr::eq(IrExpr::binding("x"), IrExpr::lit_int(1));
        assert!(!effects_for_bound_scalar(&equality).safe_to_duplicate());
        assert!(effects_for_bound_membership(&equality, &|_| false).safe_to_duplicate());
        assert!(!EffectFacts::default().safe_to_discard());
    }
}
