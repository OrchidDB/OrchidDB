//! Managed graph identity remains relational. Encoded property payloads cross
//! only the scalar language boundary, never a native scan or join.
use super::*;
impl LoweringContext<'_> {
    pub(super) fn lower_managed_scan(
        &mut self,
        table: &str,
        binding: &str,
        labels: &LabelExpr,
        edge: bool,
    ) -> RelResult<LoweredNode> {
        use datafusion::datasource::provider_as_source;
        use datafusion::functions_nested::expr_fn::array_has;
        let fields = vec![
            Field::new("kind", DataType::Int32, false),
            Field::new("name", DataType::Utf8, false),
            Field::new("id", DataType::Int64, false),
            Field::new("live", DataType::Boolean, false),
            Field::new("src_name", DataType::Utf8, true),
            Field::new("src_id", DataType::Int64, true),
            Field::new("dst_name", DataType::Utf8, true),
            Field::new("dst_id", DataType::Int64, true),
            Field::new("cypher_id", DataType::Int64, true),
            Field::new(
                "labels",
                DataType::List(Arc::new(Field::new("item", DataType::Utf8, true))),
                true,
            ),
            Field::new("properties", DataType::Utf8, true),
            Field::new("payload", DataType::Binary, false),
        ];
        let provider = mapping::schema_only_provider(Arc::new(Schema::new(fields)));
        let plan = LogicalPlanBuilder::scan(
            datafusion::common::TableReference::parse_str(table),
            provider_as_source(provider),
            None,
        )?
        .build()?;
        fn label_filter(labels: &LabelExpr, edge: bool) -> Expr {
            let contains = |name: &String| {
                if edge {
                    col_exact("name").eq(lit(name.clone()))
                } else {
                    array_has(col_exact("labels"), lit(name.clone()))
                }
            };
            match labels {
                LabelExpr::Any => lit(true),
                LabelExpr::AnyOf(names) => names
                    .iter()
                    .map(contains)
                    .reduce(Expr::or)
                    .unwrap_or(lit(false)),
                LabelExpr::AllOf(names) => names
                    .iter()
                    .map(contains)
                    .reduce(Expr::and)
                    .unwrap_or(lit(true)),
                LabelExpr::Not(inner) => Expr::Not(Box::new(label_filter(inner, edge))),
            }
        }
        let mut descriptor = vec![
            lit("kind"),
            lit(if edge { "edge" } else { "node" }),
            lit("id"),
            col_exact("cypher_id"),
            lit("identity"),
            col_exact("id"),
            lit("label"),
            col_exact("name"),
            lit("labels"),
            col_exact("labels"),
            lit("properties"),
            df_core::named_struct(vec![lit(native_values::FIELD), col_exact("properties")]),
            lit("native_record"),
            crate::ir::functions::portable::function("fn.encode")
                .expect("portable binary encoding")
                .call(vec![col_exact("payload"), lit("base64pad")]),
        ];
        if edge {
            for (key, column) in [
                ("src_label", "src_name"),
                ("src_id", "src_id"),
                ("dst_label", "dst_name"),
                ("dst_id", "dst_id"),
            ] {
                descriptor.extend([lit(key), col_exact(column)]);
            }
        }
        let element = df_core::named_struct(vec![
            lit("__orchiddb_element_v1"),
            df_core::named_struct(descriptor),
        ]);
        let value = if self.language == Language::Cypher { native_values::typed::call("pack",vec![element]) } else { native_values::function(
            native_values::VALUE,
            native_values::value_type(),
            vec![
                lit(serde_json::to_string(&IrExpr::binding(binding))
                    .map_err(|e| RelError::Unsupported(e.to_string()))?),
                df_core::named_struct(vec![lit(binding), element]),
            ],
        ) };
        let mut columns = vec![
            col_exact("id").alias(id_col(binding)),
            col_exact("name").alias(label_col(binding)),
            value.alias(binding),
        ];
        if edge {
            columns.extend([
                col_exact("src_id").alias(src_id_col(binding)),
                col_exact("src_name").alias(src_label_col(binding)),
                col_exact("dst_id").alias(dst_id_col(binding)),
                col_exact("dst_name").alias(dst_label_col(binding)),
            ]);
        } else {
            columns.push(col_exact("labels").alias(node_labels_col(binding)));
        }
        self.scan_counter += 1;
        Ok(LoweredNode::new(
            LogicalPlanBuilder::from(plan)
                .filter(
                    col_exact("kind")
                        .eq(lit(if edge { 2_i32 } else { 1_i32 }))
                        .and(col_exact("live"))
                        .and(label_filter(labels, edge)),
                )?
                .project(columns)?
                .alias(format!("__managed_scan_{}", self.scan_counter))?
                .build()?,
        ))
    }
}
