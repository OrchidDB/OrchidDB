//! Solution ops for typed SPARQL lowering.

use super::*;

impl Lowerer<'_, '_> {
    pub(super) fn filter(&mut self, sol: Sol, condition: &IrExpr) -> RelResult<Sol> {
        let mut env = Env {
            plan: sol.plan.clone(),
            vars: sol.vars.clone(),
            temps: Vec::new(),
        };
        let predicate = self.ebv(&mut env, condition)?;
        let plan = self.filter_plan(env.plan, predicate.is_true())?;
        let plan = if env.temps.is_empty() {
            plan
        } else {
            let columns = self.nonempty(sol.columns());
            self.project(plan, columns)?
        };
        Ok(Sol { plan, ..sol })
    }

    /// BIND: an error leaves the variable unbound.
    pub(super) fn extend(&mut self, sol: Sol, items: &[ProjectionItem]) -> RelResult<Sol> {
        let mut sol = sol;
        for item in items {
            let mut env = Env {
                plan: sol.plan.clone(),
                vars: sol.vars.clone(),
                temps: Vec::new(),
            };
            let term = self.materialize(&mut env, &item.expr)?;
            let certain = self.never_errors(&sol, &item.expr);
            let mut vars = sol.vars.clone();
            vars.remove(&item.alias);
            let mut next = Sol {
                plan: env.plan.clone(),
                vars,
                keys: sol.keys.clone(),
                ord: sol.ord.clone(),
            };
            let mut columns = next.columns();
            columns.extend(guarded(term).aliased(&item.alias));
            next.plan = self.project(env.plan, columns)?;
            next.vars.insert(item.alias.clone(), certain);
            sol = next;
        }
        Ok(sol)
    }

    pub(super) fn never_errors(&self, sol: &Sol, expr: &IrExpr) -> bool {
        match expr {
            IrExpr::Binding(name) => sol.vars.get(name).copied().unwrap_or(false),
            IrExpr::Lit(Lit::Null) => false,
            IrExpr::Lit(_) => true,
            IrExpr::Call { name, args }
                if name == crate::language::sparql::SPARQL_IRI_CALL
                    || name == crate::language::sparql::SPARQL_LITERAL_CALL
                    || name == crate::language::sparql::SPARQL_LANG_LITERAL_CALL =>
            {
                args.iter()
                    .all(|arg| matches!(arg, IrExpr::Lit(Lit::String(_))))
            }
            _ => false,
        }
    }

    /// Sub-select projection.
    pub(super) fn select(&mut self, sol: Sol, items: &[ProjectionItem]) -> RelResult<Sol> {
        let mut env = Env {
            plan: sol.plan.clone(),
            vars: sol.vars.clone(),
            temps: Vec::new(),
        };
        let mut columns = Vec::new();
        let mut vars = BTreeMap::new();
        for item in items {
            let (term, certain) = match &item.expr {
                IrExpr::Binding(name) => {
                    (env.term(name), sol.vars.get(name).copied().unwrap_or(false))
                }
                IrExpr::Lit(Lit::Null) => (Term::error(), false),
                other => (guarded(self.materialize(&mut env, other)?), false),
            };
            columns.extend(term.aliased(&item.alias));
            vars.insert(item.alias.clone(), certain);
        }
        if let Some(ord) = &sol.ord {
            columns.push(col_exact(ord));
        }
        if columns.is_empty() {
            columns.push(lit(1_i64).alias(self.fresh("row")));
        }
        let plan = self.project(env.plan, columns)?;
        Ok(Sol {
            plan,
            vars,
            keys: BTreeSet::new(),
            ord: sol.ord,
        })
    }

    pub(super) fn distinct(&mut self, sol: Sol) -> RelResult<Sol> {
        let Some(ord) = sol.ord.clone() else {
            let columns = self.nonempty(sol.columns());
            let plan = self.project(sol.plan.clone(), columns)?;
            let plan = LogicalPlanBuilder::from(plan).distinct()?.build()?;
            return Ok(Sol { plan, ..sol });
        };
        let group: Vec<Expr> = sol
            .column_names()
            .into_iter()
            .filter(|name| name != &ord)
            .map(col_exact)
            .collect();
        let input = self.cte(sol.plan.clone())?;
        let plan = LogicalPlanBuilder::from(input)
            .aggregate(group, vec![df_min(col_exact(&ord)).alias(&ord)])?
            .build()?;
        Ok(Sol { plan, ..sol })
    }

    pub(super) fn slice(&mut self, sol: Sol, slice: &Slice) -> RelResult<Sol> {
        if slice.tail.is_some() {
            return unsupported("tail slices are not SPARQL modifiers");
        }
        let mut builder = LogicalPlanBuilder::from(sol.plan.clone());
        if let Some(ord) = &sol.ord {
            builder = builder.sort(vec![col_exact(ord).sort(true, false)])?;
        }
        let plan = builder
            .limit(
                slice.offset as usize,
                slice.fetch.map(|fetch| fetch as usize),
            )?
            .build()?;
        // A later operator must not lose the LIMIT's row set to reordering.
        let plan = self.cte(plan)?;
        Ok(Sol { plan, ..sol })
    }

    pub(super) fn sort(&mut self, sol: Sol, keys: &[SortKey]) -> RelResult<Sol> {
        let mut env = Env {
            plan: sol.plan.clone(),
            vars: sol.vars.clone(),
            temps: Vec::new(),
        };
        let mut order = Vec::new();
        for key in keys {
            let term = self.materialize(&mut env, &key.expr)?;
            let asc = key.dir == SortDir::Asc;
            order.extend(
                order_keys(&term)
                    .into_iter()
                    .map(|expr| expr.sort(asc, asc)),
            );
        }
        let ord = self.fresh("ord");
        let window = df_window::row_number()
            .window_frame(datafusion::logical_expr::WindowFrame::new(None))
            .order_by(order).build()?.alias(&ord);
        let plan = LogicalPlanBuilder::from(env.plan)
            .window(vec![window])?
            .build()?;
        let mut columns = sol.columns();
        if let Some(previous) = &sol.ord {
            columns
                .retain(|expr| !matches!(expr, Expr::Column(column) if &column.name == previous));
        }
        columns.push(col_exact(&ord));
        let plan = self.window_projection(plan, columns)?;
        Ok(Sol {
            plan,
            vars: sol.vars,
            keys: sol.keys,
            ord: Some(ord),
        })
    }

    pub(super) fn union(&mut self, left: Sol, right: Sol) -> RelResult<Sol> {
        let all_vars: BTreeSet<_> = left.vars.keys().chain(right.vars.keys()).cloned().collect();
        let keys: BTreeSet<_> = left.keys.intersection(&right.keys).cloned().collect();
        let align = |sol: &Sol| {
            let mut columns = Vec::new();
            for var in &all_vars {
                columns.extend(sol.term(var).unwrap_or_else(Term::error).aliased(var));
            }
            columns.extend(keys.iter().map(col_exact));
            columns
        };
        let mut left_columns = align(&left);
        let mut right_columns = align(&right);
        if left_columns.is_empty() {
            left_columns.push(lit(1_i64).alias("__sq_union_row"));
            right_columns.push(lit(1_i64).alias("__sq_union_row"));
        }
        let left_plan = self.project(left.plan.clone(), left_columns)?;
        let right_plan = self.project(right.plan.clone(), right_columns)?;
        let plan = LogicalPlanBuilder::from(left_plan)
            .union(right_plan)?
            .build()?;
        let plan = self.cte(plan)?;
        let vars = all_vars
            .into_iter()
            .map(|var| {
                let certain = left.vars.get(&var).copied().unwrap_or(false)
                    && right.vars.get(&var).copied().unwrap_or(false);
                (var, certain)
            })
            .collect();
        Ok(Sol {
            plan,
            vars,
            keys,
            ord: None,
        })
    }

    /// `MINUS`: remove a left solution when some right solution is
    /// compatible with it and shares at least one bound variable.
    pub(super) fn minus(&mut self, left: Sol, right: Sol) -> RelResult<Sol> {
        let shared: Vec<_> = left
            .vars
            .keys()
            .filter(|var| right.vars.contains_key(*var))
            .cloned()
            .collect();
        if shared.is_empty() {
            return Ok(left);
        }
        let prefix = self.fresh("m:");
        let mut renamed_columns = Vec::new();
        for var in &shared {
            for name in var_columns(var) {
                renamed_columns.push(col_exact(&name).alias(format!("{prefix}{name}")));
            }
        }
        let renamed = self.project(right.plan, renamed_columns)?;
        let right_term = |var: &str| {
            let [kind, dt, lang] = binding_identity_columns(var);
            Term {
                value: col_exact(format!("{prefix}{var}")),
                kind: col_exact(format!("{prefix}{kind}")),
                dt: col_exact(format!("{prefix}{dt}")),
                lang: col_exact(format!("{prefix}{lang}")),
            }
        };
        let mut overlaps = Vec::new();
        let mut compatible = Vec::new();
        for var in &shared {
            let (l, r) = (Term::columns(var), right_term(var));
            overlaps.push(l.bound().and(r.bound()));
            compatible.push(or_all(vec![
                l.kind.clone().is_null(),
                r.kind.clone().is_null(),
                l.same_term(&r),
            ]));
        }
        let mut condition = vec![or_all(overlaps)];
        condition.extend(compatible);
        let plan = LogicalPlanBuilder::from(left.plan.clone())
            .join_on(renamed, JoinType::LeftAnti, vec![fold(and_all(condition))?])?
            .build()?;
        Ok(Sol { plan, ..left })
    }

    pub(super) fn values(&mut self, bindings: &[String], rows: &[Vec<Value>]) -> RelResult<Sol> {
        if rows.iter().any(|row| row.len() != bindings.len()) {
            return unsupported("VALUES row width does not match its variables");
        }
        let mut vars = BTreeMap::new();
        for name in bindings {
            if name.starts_with("__rdf:term:") {
                continue;
            }
            let identity = binding_identity_columns(name);
            let kind_index = bindings
                .iter()
                .position(|binding| binding == &identity[0])
                .ok_or_else(|| {
                    RelError::Unsupported(format!(
                        "SPARQL VALUES variable `{name}` lacks RDF term identity columns"
                    ))
                })?;
            let certain = rows
                .iter()
                .all(|row| !matches!(row[kind_index], Value::Null));
            vars.insert(name.clone(), certain);
        }
        if bindings.is_empty() {
            return match rows.len() {
                0 => self.lower(&Node::GraphEmpty),
                1 => self.one_row(),
                _ => unsupported("VALUES with no variables and several rows"),
            };
        }
        let schema = Arc::new(Schema::new(
            bindings
                .iter()
                .map(|name| Field::new(name, DataType::Utf8, true))
                .collect::<Vec<_>>(),
        ));
        let arrays = (0..bindings.len())
            .map(|column| {
                let values = rows
                    .iter()
                    .map(|row| match &row[column] {
                        Value::Null => Ok(None),
                        Value::String(value) => Ok(Some(value.clone())),
                        other => Err(RelError::Unsupported(format!(
                            "SPARQL VALUES term component must be text, got {other:?}"
                        ))),
                    })
                    .collect::<RelResult<Vec<_>>>()?;
                Ok(Arc::new(StringArray::from(values)) as ArrayRef)
            })
            .collect::<RelResult<Vec<_>>>()?;
        let batch = RecordBatch::try_new(schema, arrays)?;
        let lowered = self.ctx.scan_batches("sparql_values", vec![batch])?;
        let mut columns = Vec::new();
        for var in vars.keys() {
            columns.extend(var_columns(var).into_iter().map(col_exact));
        }
        let plan = self.project(lowered.plan, columns)?;
        Ok(Sol {
            plan,
            vars,
            keys: BTreeSet::new(),
            ord: None,
        })
    }

    // -- aggregates -----------------------------------------------------------

}
