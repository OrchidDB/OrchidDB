use super::*;
use datafusion::logical_expr::{LogicalPlanBuilder, lit};

fn row_kernel(plan: &LogicalPlan) -> RowKernel {
    let LogicalPlan::Extension(extension) = plan else {panic!("expected compiled boundary")};
    extension.node.as_any().downcast_ref::<RowKernel>().unwrap().clone()
}

#[test]
fn cypher_transport_rewrites_preserve_identity_schema_and_dependency_count() {
    let input = LogicalPlanBuilder::empty(true).build().unwrap();
    let original = row_kernel(&kernel("DecodeTraversers",vec![input.clone()],|mut inputs,_|Ok(inputs.remove(0))));
    let replacement = LogicalPlanBuilder::empty(false).build().unwrap();
    let rewritten = original.with_exprs_and_inputs(vec![],vec![replacement.clone()]).unwrap();
    assert_eq!(UserDefinedLogicalNodeCore::inputs(&rewritten),vec![&replacement]);
    assert_eq!(rewritten.schema(),original.schema());
    assert_eq!(rewritten.name(),"DecodeTraversers");
    assert!(rewritten.expressions().is_empty());
    assert_ne!(rewritten,original);
    assert_eq!(rewritten.partial_cmp(&original),replacement.partial_cmp(&input));
    assert_ne!(rewritten.partial_cmp(&original),Some(std::cmp::Ordering::Equal));
    assert!(original.with_exprs_and_inputs(vec![lit(true)],vec![input]).is_err());
    assert!(original.with_exprs_and_inputs(vec![],vec![]).is_err());
    assert_eq!(original.prevent_predicate_push_down_columns().len(),original.schema.fields().len());
    assert!(format!("{original:?}").contains("DecodeTraversers"));
    use std::hash::{Hash,Hasher};
    let hash = |value: &RowKernel| {let mut h=std::collections::hash_map::DefaultHasher::new();value.hash(&mut h);h.finish()};
    assert_eq!(hash(&original),hash(&original.clone()));
}

#[test]
fn cypher_transport_physical_rewrites_reject_extra_dependencies_and_partitions() {
    let boundary = row_kernel(&kernel("DecodeTraversers",vec![],|_,_|Ok(vec![])));
    let planner = KernelPlanner {state:Arc::new(Mutex::new(KernelState::default()))};
    let physical = planner.plan(&boundary,vec![]).unwrap();
    assert_eq!(physical.name(),"DecodeTraversers");
    assert_eq!(physical.description(),"DecodeTraversers");
    assert!(physical.children().is_empty());
    assert!(is_kernel_exec(physical.as_ref()));
    assert_eq!(physical.schema().as_ref(),boundary.schema.as_arrow());
    let rewritten = physical.clone().with_new_children(vec![]).unwrap();
    assert_eq!(rewritten.schema(),physical.schema());
    assert!(physical.clone().with_new_children(vec![rewritten]).is_err());
    let error = match physical.execute(1,Arc::new(TaskContext::default())) {Ok(_)=>panic!("partition must be rejected"),Err(e)=>e};
    assert!(error.to_string().contains("one partition"));
    let relational = row_kernel(&kernel("Filter",vec![],|_,_|Ok(vec![])));
    assert!(planner.plan(&relational,vec![]).unwrap_err().to_string().contains("SQL-required lowering rejected"));
}

#[test]
fn cypher_transport_boundary_blocks_unary_fusion() {
    let input = LogicalPlanBuilder::empty(true).build().unwrap();
    let source = kernel("DecodeTraversers",vec![input],|mut rows,_|Ok(rows.remove(0)));
    let parent = kernel("Project",vec![source],|mut rows,_|Ok(rows.remove(0)));
    let unchanged = fuse_unary_kernels(parent.clone()).unwrap();
    assert_eq!(unchanged,parent,"result transport boundary must remain explicit");
    let plain = LogicalPlanBuilder::empty(false).build().unwrap();
    assert_eq!(fuse_unary_kernels(plain.clone()).unwrap(),plain);
}

#[test]
fn cypher_mutation_snapshot_proof_selects_only_filtered_pure_prefixes() {
    use crate::ir::expr::{IrExpr,Lit};
    let filter = |input: Node| Node::GraphFilter {condition:IrExpr::Lit(Lit::Bool(true)),input:Box::new(input)};
    let mutation = |input: Node| Node::GraphDelete {targets:vec![IrExpr::binding("n")],detach:false,input:Box::new(input)};
    for plan in [Node::GraphOneRow,filter(Node::GraphOneRow),mutation(Node::GraphOneRow)] {
        assert!(initial_snapshot_reads(&plan).is_empty());
        assert!(!has_mutating_branches(&plan));
    }
    let plan = mutation(filter(Node::GraphOneRow));
    let Node::GraphDelete {input,..} = &plan else {unreachable!()};
    let Node::GraphFilter {input:seed,..} = input.as_ref() else {unreachable!()};
    assert_eq!(initial_snapshot_reads(&plan),std::collections::BTreeSet::from([
        input.as_ref() as *const Node as usize,seed.as_ref() as *const Node as usize,
    ]));
    // A read above a write still snapshots the pure prefix before the first write.
    let wrapped = filter(plan);
    assert_eq!(initial_snapshot_reads(&wrapped).len(),2);
    assert!(!has_mutating_branches(&wrapped));
    let branched = Node::GraphUnion {all:true,align:crate::ir::plan::UnionAlign::ByVariableName,
        left:Box::new(mutation(filter(Node::GraphOneRow))),right:Box::new(Node::GraphOneRow)};
    assert!(has_mutating_branches(&branched));
    assert!(initial_snapshot_reads(&branched).is_empty(),"branch ordering provides no unary-prefix proof");
    let outer = mutation(filter(branched));
    assert!(initial_snapshot_reads(&outer).is_empty(),"mutation-bearing input cannot be a pure snapshot");
}

#[test]
fn cypher_kernel_ordering_agrees_with_equality_after_input_rewrites() {
    use std::cmp::Ordering;
    let first=LogicalPlanBuilder::empty(true).build().unwrap();
    let second=LogicalPlanBuilder::empty(false).build().unwrap();
    let original=row_kernel(&kernel("DecodeTraversers",vec![first.clone()],|mut rows,_|Ok(rows.remove(0))));
    let changed=original.with_exprs_and_inputs(vec![],vec![second]).unwrap();
    let unchanged=original.with_exprs_and_inputs(vec![],vec![first.clone()]).unwrap();
    let separate=row_kernel(&kernel("DecodeTraversers",vec![first],|mut rows,_|Ok(rows.remove(0))));
    let values=[original.clone(),original,changed,unchanged,separate];
    for a in &values {
        for b in &values {
            assert_eq!(a.partial_cmp(b)==Some(Ordering::Equal),a==b);
            assert_eq!(a.partial_cmp(b),b.partial_cmp(a).map(Ordering::reverse));
        }
    }
}

#[test]
fn cypher_kernel_ordering_preserves_incomparable_extension_inputs() {
    let first=kernel("DecodeTraversers",vec![],|_,_|Ok(vec![]));
    let second=crate::operations::RequestResult {schema:first.schema().clone()}.into_plan();
    assert_eq!(first.partial_cmp(&second),None);
    let original=row_kernel(&kernel("DecodeTraversers",vec![first],|mut rows,_|Ok(rows.remove(0))));
    let changed=original.with_exprs_and_inputs(vec![],vec![second]).unwrap();
    assert_ne!(original,changed);
    assert_eq!(original.partial_cmp(&changed),None);
    assert_eq!(changed.partial_cmp(&original),None);
}
