//! Joins for typed SPARQL lowering.

use super::*;

impl Lowerer<'_, '_> {
    /// SPARQL compatibility join. Shared variables match when either side is
    /// unbound or both hold the same RDF term; the merged solution takes the
    /// bound side.
    pub(super) fn join(&mut self, left: Sol, right: Sol, optional: bool) -> RelResult<Sol> {
        let prefix = self.fresh("r:");
        let right_names = right.column_names();
        let renamed = self.project(
            right.plan.clone(),
            right_names
                .iter()
                .map(|name| col_exact(name).alias(format!("{prefix}{name}")))
                .collect(),
        )?;
        let right_term = |var: &str| {
            let [kind, dt, lang] = binding_identity_columns(var);
            Term {
                value: col_exact(format!("{prefix}{var}")),
                kind: col_exact(format!("{prefix}{kind}")),
                dt: col_exact(format!("{prefix}{dt}")),
                lang: col_exact(format!("{prefix}{lang}")),
            }
        };
        let mut conditions = Vec::new();
        for (var, left_certain) in &left.vars {
            let Some(right_certain) = right.vars.get(var) else {
                continue;
            };
            let (l, r) = (Term::columns(var), right_term(var));
            if *left_certain && *right_certain {
                conditions.push(l.same_term(&r));
            } else {
                conditions.push(or_all(vec![
                    l.kind.clone().is_null(),
                    r.kind.clone().is_null(),
                    l.same_term(&r),
                ]));
            }
        }
        for key in left.keys.intersection(&right.keys) {
            conditions.push(col_exact(key).eq(col_exact(format!("{prefix}{key}"))));
        }
        let join_type = if optional {
            JoinType::Left
        } else {
            JoinType::Inner
        };
        let plan = if conditions.is_empty() && !optional {
            LogicalPlanBuilder::from(left.plan.clone())
                .cross_join(renamed)?
                .build()?
        } else {
            if conditions.is_empty() {
                conditions.push(lit(true));
            }
            LogicalPlanBuilder::from(left.plan.clone())
                .join_on(renamed, join_type, vec![fold(and_all(conditions))?])?
                .build()?
        };
        let mut projections = Vec::new();
        let mut vars = BTreeMap::new();
        let all_vars: BTreeSet<_> = left.vars.keys().chain(right.vars.keys()).cloned().collect();
        for var in all_vars {
            let in_left = left.vars.get(&var).copied();
            let in_right = right.vars.get(&var).copied();
            let (term, certain) = match (in_left, in_right) {
                (Some(certain), None) => (Term::columns(&var), certain),
                (None, Some(certain)) => (right_term(&var), certain && !optional),
                (Some(true), Some(_)) => (Term::columns(&var), true),
                (Some(false), Some(right_certain)) => {
                    let l = Term::columns(&var);
                    let r = right_term(&var);
                    let left_unbound = l.kind.clone().is_null();
                    let pick = |a: Expr, b: Expr| case(vec![(left_unbound.clone(), b)], Some(a));
                    (
                        Term {
                            value: pick(l.value, r.value),
                            kind: pick(l.kind, r.kind),
                            dt: pick(l.dt, r.dt),
                            lang: pick(l.lang, r.lang),
                        },
                        right_certain && !optional,
                    )
                }
                (None, None) => unreachable!(),
            };
            projections.extend(term.aliased(&var));
            vars.insert(var, certain);
        }
        let mut keys = left.keys.clone();
        for key in &left.keys {
            projections.push(col_exact(key));
        }
        for key in right.keys.difference(&left.keys) {
            projections.push(col_exact(format!("{prefix}{key}")).alias(key));
            keys.insert(key.clone());
        }
        if projections.is_empty() {
            projections.push(lit(1_i64).alias(self.fresh("row")));
        }
        let plan = self.project(plan, projections)?;
        Ok(Sol {
            plan,
            vars,
            keys,
            ord: None,
        })
    }

    /// Add a deterministic per-row key. Identical rows may exchange keys,
    /// which is harmless because they are indistinguishable solutions.
    pub(super) fn with_row_key(&mut self, sol: Sol) -> RelResult<(Sol, String)> {
        let key = self.fresh("rid");
        let order: Vec<SortExpr> = sol
            .column_names()
            .into_iter()
            .map(|name| col_exact(name).sort(true, true))
            .collect();
        let window = df_window::row_number().order_by(order).build()?.alias(&key);
        let plan = LogicalPlanBuilder::from(sol.plan.clone())
            .window(vec![window])?
            .build()?;
        let mut columns = sol.columns();
        columns.push(col_exact(&key));
        let plan = self.window_projection(plan, columns)?;
        let plan = self.cte(plan)?;
        let mut keys = sol.keys.clone();
        keys.insert(key.clone());
        Ok((
            Sol {
                plan,
                vars: sol.vars,
                keys,
                ord: sol.ord,
            },
            key,
        ))
    }

    /// `LeftJoin(L, R, expr)`: the filter sees the merged solution. Left rows
    /// with no compatible, filter-passing right row are kept unextended.
    ///
    /// Evaluated in one pass over `L ⟕ R`: every candidate is marked by
    /// whether it passes, and per left row (identified by a row key) either
    /// the passing candidates or one unextended row survive. The left input
    /// is referenced once, which also avoids a DuckDB optimizer failure on
    /// reused CTEs.
    pub(super) fn left_join_filtered(&mut self, left: Sol, right: Sol, condition: &IrExpr) -> RelResult<Sol> {
        let left = self.ensure_seeded(left)?;
        let shared_uncertain: Vec<String> = left
            .vars
            .iter()
            .filter(|(var, certain)| !**certain && right.vars.contains_key(*var))
            .map(|(var, _)| var.clone())
            .collect();
        // Remember whether each possibly-unbound shared variable was bound
        // on the left, to restore it on unextended rows.
        let mut left = left;
        let mut left_bound = BTreeMap::new();
        if !shared_uncertain.is_empty() {
            let mut columns = left.columns();
            for var in &shared_uncertain {
                let name = self.fresh("left_kind");
                columns.push(Term::columns(var).kind.alias(&name));
                left.keys.insert(name.clone());
                left_bound.insert(var.clone(), name);
            }
            left.plan = self.project(left.plan.clone(), columns)?;
        }
        let (left, key) = self.with_row_key(left)?;
        let marker = self.fresh("right_row");
        let mut right = right;
        let mut columns = right.columns();
        columns.push(lit(true).alias(&marker));
        right.plan = self.project(right.plan.clone(), columns)?;
        right.keys.insert(marker.clone());
        let joined = self.join(left.clone(), right.clone(), true)?;

        let mut env = Env {
            plan: joined.plan.clone(),
            vars: joined.vars.clone(),
            temps: Vec::new(),
        };
        let passes = self.ebv(&mut env, condition)?;
        let pass = self.fresh("pass");
        let mut columns = joined.columns();
        columns.push(
            col_exact(&marker)
                .is_not_null()
                .and(passes.is_true())
                .alias(&pass),
        );
        let plan = self.project(env.plan, columns)?;
        let rank = self.fresh("rank");
        let window = df_window::row_number()
            .partition_by(vec![col_exact(&key)])
            .order_by(vec![col_exact(&pass).sort(false, false)])
            .build()?
            .alias(&rank);
        let plan = LogicalPlanBuilder::from(plan)
            .window(vec![window])?
            .build()?;
        let mut columns = joined.columns();
        columns.push(col_exact(&pass));
        columns.push(col_exact(&rank));
        let plan = self.window_projection(plan, columns)?;
        let plan = self.filter_plan(plan, col_exact(&pass).or(col_exact(&rank).eq(lit(1_u64))))?;

        let passed = col_exact(&pass);
        let mut columns = Vec::new();
        let mut vars = BTreeMap::new();
        for (var, certain) in &joined.vars {
            let term = Term::columns(var);
            let (term, certain) = match (left.vars.get(var), right.vars.contains_key(var)) {
                (Some(_), false) | (Some(true), true) => (term, *certain),
                (None, _) => (term.only_if(passed.clone()), false),
                (Some(false), true) => {
                    let bound = passed.clone().or(col_exact(&left_bound[var]).is_not_null());
                    (term.only_if(bound), false)
                }
            };
            columns.extend(term.aliased(var));
            vars.insert(var.clone(), certain);
        }
        let mut keys = left.keys.clone();
        keys.remove(&key);
        for name in left_bound.values() {
            keys.remove(name);
        }
        columns.extend(keys.iter().map(col_exact));
        if columns.is_empty() {
            columns.push(lit(1_i64).alias(self.fresh("row")));
        }
        let plan = self.project(plan, columns)?;
        Ok(Sol {
            plan,
            vars,
            keys,
            ord: None,
        })
    }

    pub(super) fn drop_key(&mut self, mut sol: Sol, key: &str) -> RelResult<Sol> {
        sol.keys.remove(key);
        let columns = self.nonempty(sol.columns());
        let plan = self.project(sol.plan.clone(), columns)?;
        Ok(Sol { plan, ..sol })
    }

    /// Inside EXISTS, join the outer solution so filters see its bindings.
    pub(super) fn ensure_seeded(&mut self, sol: Sol) -> RelResult<Sol> {
        let Some(seed) = self.seeds.last().cloned() else {
            return Ok(sol);
        };
        if seed.keys.is_subset(&sol.keys) {
            return Ok(sol);
        }
        self.join(sol, seed, false)
    }

    pub(super) fn exists(&mut self, left: Sol, right: &Node, negated: bool) -> RelResult<Sol> {
        let left = self.ensure_seeded(left)?;
        // When no expression inside the pattern reads an outer binding that
        // the pattern itself does not bind, substitution equals a
        // compatibility semi-join and needs no correlation.
        let inner = self.lower(right)?;
        let mut referenced = BTreeSet::new();
        expression_variables(right, &mut referenced);
        let correlated = referenced.iter().any(|var| {
            left.vars.contains_key(var) && !inner.vars.get(var).copied().unwrap_or(false)
        }) || rdf_paths::needs_substitution(right, &left.vars);
        if !correlated {
            return self.compatible_semi_join(left, inner, negated);
        }
        let (seeded_left, key) = self.with_row_key(left.clone())?;
        self.seeds.push(seeded_left.clone());
        let inner = self
            .lower(right)
            .and_then(|inner| self.ensure_seeded(inner));
        self.seeds.pop();
        let inner = inner?;
        let matched = self.project(
            inner.plan,
            vec![col_exact(&key).alias(format!("{key}_exists"))],
        )?;
        let plan = LogicalPlanBuilder::from(seeded_left.plan.clone())
            .join_on(
                matched,
                if negated {
                    JoinType::LeftAnti
                } else {
                    JoinType::LeftSemi
                },
                vec![col_exact(&key).eq(col_exact(format!("{key}_exists")))],
            )?
            .build()?;
        let sol = Sol {
            plan,
            ..seeded_left
        };
        self.drop_key(sol, &key)
    }

    /// Extend each solution with `mark` = 1 when the pattern has a solution
    /// under it (EXISTS) and 0 otherwise.
    pub(super) fn exists_mark(&mut self, left: Sol, pattern: &Node, mark: &str) -> RelResult<Sol> {
        let left = self.ensure_seeded(left)?;
        let inner = self.lower(pattern)?;
        let mut referenced = BTreeSet::new();
        expression_variables(pattern, &mut referenced);
        let correlated = referenced.iter().any(|var| {
            left.vars.contains_key(var) && !inner.vars.get(var).copied().unwrap_or(false)
        }) || rdf_paths::needs_substitution(pattern, &left.vars);
        let (keyed, key) = self.with_row_key(left.clone())?;
        let found = self.fresh("found");
        let marked = if correlated {
            // Evaluate the pattern with the outer solution substituted, then
            // attach the surviving row keys.
            self.seeds.push(keyed.clone());
            let seeded = self
                .lower(pattern)
                .and_then(|inner| self.ensure_seeded(inner));
            self.seeds.pop();
            let seeded = seeded?;
            let matched_key = self.fresh("matched");
            let matched = self.project(seeded.plan, vec![col_exact(&key).alias(&matched_key)])?;
            let matched = LogicalPlanBuilder::from(matched).distinct()?.build()?;
            let plan = LogicalPlanBuilder::from(keyed.plan.clone())
                .join_on(
                    matched,
                    JoinType::Left,
                    vec![col_exact(&key).eq(col_exact(&matched_key))],
                )?
                .build()?;
            let mut columns = keyed.columns();
            columns.push(col_exact(&matched_key).is_not_null().alias(&found));
            self.project(plan, columns)?
        } else {
            // One pass: keep one row per left solution, preferring a match.
            let mut right = inner;
            let marker = self.fresh("right_row");
            let mut columns = right.columns();
            columns.push(lit(true).alias(&marker));
            right.plan = self.project(right.plan.clone(), columns)?;
            right.keys.insert(marker.clone());
            // Remember the left binding state; the join may fill unbound
            // shared variables from the pattern.
            let mut keyed_copy = keyed.clone();
            let mut restore = Vec::new();
            let mut columns = keyed_copy.columns();
            for (var, certain) in &keyed.vars {
                if !certain && right.vars.contains_key(var) {
                    let name = self.fresh("left_kind");
                    columns.push(Term::columns(var).kind.alias(&name));
                    keyed_copy.keys.insert(name.clone());
                    restore.push((var.clone(), name));
                }
            }
            keyed_copy.plan = self.project(keyed_copy.plan.clone(), columns)?;
            let joined = self.join(keyed_copy.clone(), right, true)?;
            let rank = self.fresh("rank");
            let window = df_window::row_number()
                .partition_by(vec![col_exact(&key)])
                .order_by(vec![col_exact(&marker).is_not_null().sort(false, false)])
                .build()?
                .alias(&rank);
            let plan = LogicalPlanBuilder::from(joined.plan.clone())
                .window(vec![window])?
                .build()?;
            let mut columns = joined.columns();
            columns.push(col_exact(&rank));
            let plan = self.window_projection(plan, columns)?;
            let plan = self.filter_plan(plan, col_exact(&rank).eq(lit(1_u64)))?;
            let mut columns = Vec::new();
            for var in keyed.vars.keys() {
                let term = Term::columns(var);
                let term = match restore.iter().find(|(name, _)| name == var) {
                    Some((_, kind)) => term.only_if(col_exact(kind).is_not_null()),
                    None => term,
                };
                columns.extend(term.aliased(var));
            }
            columns.extend(keyed.keys.iter().map(col_exact));
            if let Some(ord) = &keyed.ord {
                columns.push(col_exact(ord));
            }
            columns.push(col_exact(&marker).is_not_null().alias(&found));
            self.project(plan, columns)?
        };
        let mut sol = Sol {
            plan: marked,
            ..keyed
        };
        let mut columns = sol.columns();
        let mark_term = Term::integer(case(
            vec![(col_exact(&found).is_true(), lit(1_i64))],
            Some(lit(0_i64)),
        ));
        columns.extend(mark_term.aliased(mark));
        sol.plan = self.project(sol.plan.clone(), columns)?;
        sol.vars.insert(mark.to_string(), true);
        self.drop_key(sol, &key)
    }

    /// Keep (or, negated, drop) left solutions compatible with some right
    /// solution. Unlike MINUS, no shared bound variable is required.
    pub(super) fn compatible_semi_join(&mut self, left: Sol, right: Sol, negated: bool) -> RelResult<Sol> {
        let prefix = self.fresh("e:");
        let shared: Vec<_> = left
            .vars
            .keys()
            .filter(|var| right.vars.contains_key(*var))
            .cloned()
            .collect();
        let mut columns = Vec::new();
        for var in &shared {
            for name in var_columns(var) {
                columns.push(col_exact(&name).alias(format!("{prefix}{name}")));
            }
        }
        let shared_keys: Vec<_> = left.keys.intersection(&right.keys).cloned().collect();
        for key in &shared_keys {
            columns.push(col_exact(key).alias(format!("{prefix}{key}")));
        }
        if columns.is_empty() {
            columns.push(lit(1_i64).alias(format!("{prefix}row")));
        }
        let renamed = self.project(right.plan, columns)?;
        let mut conditions = Vec::new();
        for var in &shared {
            let l = Term::columns(var);
            let [kind, dt, lang] = binding_identity_columns(var);
            let r = Term {
                value: col_exact(format!("{prefix}{var}")),
                kind: col_exact(format!("{prefix}{kind}")),
                dt: col_exact(format!("{prefix}{dt}")),
                lang: col_exact(format!("{prefix}{lang}")),
            };
            let certain = left.vars[var] && right.vars[var];
            conditions.push(if certain {
                l.same_term(&r)
            } else {
                or_all(vec![
                    l.kind.clone().is_null(),
                    r.kind.clone().is_null(),
                    l.same_term(&r),
                ])
            });
        }
        for key in &shared_keys {
            conditions.push(col_exact(key).eq(col_exact(format!("{prefix}{key}"))));
        }
        let join_type = if negated {
            JoinType::LeftAnti
        } else {
            JoinType::LeftSemi
        };
        let plan = LogicalPlanBuilder::from(left.plan.clone())
            .join_on(renamed, join_type, vec![fold(and_all(conditions))?])?
            .build()?;
        Ok(Sol { plan, ..left })
    }

}
