//! Internal Cypher lowering contracts; external integration tests cannot call
//! these crate-private helpers. Keep test source outside production coverage.
use super::*;
use crate::grammar::generated::cypher::{cypherlexer::CypherLexer, cypherparser::CypherParser};
use crate::language::cypher::ast::{BinaryOp, Expr, Literal, StringPredicateOp, UnaryOp};
use crate::language::cypher::parser::CypherParseError;
use antlr4rust::parser::Parser;
use antlr4rust::{InputStream, common_token_stream::CommonTokenStream};

macro_rules! lowering {
    ($name:ident,$rule:ident,$lower:path,$input:expr,$expected:expr) => {
        #[test]
        fn $name() {
            let errors = super::super::SyntaxErrors::default();
            let mut lexer = CypherLexer::new(InputStream::new($input));
            lexer.remove_error_listeners();
            lexer.add_error_listener(Box::new(errors.listener()));
            let mut parser = CypherParser::new(CommonTokenStream::new(lexer));
            parser.remove_error_listeners();
            parser.add_error_listener(Box::new(errors.listener()));
            let context = parser.$rule().unwrap();
            errors.into_result().expect("valid grammar fragment");
            assert_eq!($lower(context.as_ref()).unwrap(), $expected);
        }
    };
}
fn integer(n: &str) -> Expr {
    Expr::Literal(Literal::Integer(n.into()))
}
fn binary(op: BinaryOp, left: Expr, right: Expr) -> Expr {
    Expr::Binary {
        op,
        lhs: Box::new(left),
        rhs: Box::new(right),
    }
}
lowering!(
    addition,
    oC_AddOrSubtractExpression,
    arithmetic::lower_add_or_subtract_expression,
    "1+2",
    binary(BinaryOp::Add, integer("1"), integer("2"))
);
lowering!(
    multiplication,
    oC_MultiplyDivideModuloExpression,
    arithmetic::lower_multiply_divide_modulo_expression,
    "2*3",
    binary(BinaryOp::Mul, integer("2"), integer("3"))
);
lowering!(
    power,
    oC_PowerOfExpression,
    arithmetic::lower_power_of_expression,
    "2^3",
    Expr::Function {
        name: "pow".into(),
        args: vec![integer("2"), integer("3")],
        distinct: false
    }
);
lowering!(
    negative,
    oC_UnaryAddOrSubtractExpression,
    arithmetic::lower_unary_add_or_subtract_expression,
    "-x",
    Expr::Unary {
        op: UnaryOp::Neg,
        expr: Box::new(Expr::Variable("x".into()))
    }
);
lowering!(
    case_expression,
    oC_CaseExpression,
    cases::lower_case_expression,
    "CASE 1 WHEN 1 THEN 2 ELSE 3 END",
    Expr::Case {
        case: Some(Box::new(integer("1"))),
        arms: vec![(integer("1"), integer("2"))],
        otherwise: Some(Box::new(integer("3")))
    }
);
lowering!(
    case_arm,
    oC_CaseAlternative,
    cases::lower_case_alternative,
    "WHEN 1 THEN 2",
    (integer("1"), integer("2"))
);
lowering!(
    function,
    oC_FunctionInvocation,
    functions::lower_function_invocation,
    "abs(2)",
    Expr::Function {
        name: "abs".into(),
        distinct: false,
        args: vec![integer("2")]
    }
);
lowering!(
    function_name,
    oC_FunctionName,
    functions::lower_function_name,
    "math.abs",
    String::from("math.abs")
);
lowering!(
    node_labels,
    oC_NodeLabels,
    labels::lower_node_labels,
    ":Person:Employee",
    vec![String::from("Person"), String::from("Employee")]
);
lowering!(
    node_label,
    oC_NodeLabel,
    labels::lower_node_label,
    ":Person",
    String::from("Person")
);
lowering!(
    relationship_types,
    oC_RelationshipTypes,
    labels::lower_relationship_types,
    ":R|S",
    vec![String::from("R"), String::from("S")]
);
lowering!(
    literal,
    oC_Literal,
    literals::lower_literal,
    "42",
    Literal::Integer("42".into())
);
lowering!(
    integer_literal,
    oC_IntegerLiteral,
    literals::lower_integer_literal,
    "42",
    String::from("42")
);
lowering!(
    map_literal,
    oC_MapLiteral,
    literals::lower_map_literal,
    "{a:1}",
    Expr::Map(vec![("a".into(), integer("1"))])
);
lowering!(
    literal_parameter,
    oC_Parameter,
    literals::lower_parameter,
    "$value",
    Expr::Parameter("value".into())
);
lowering!(
    parameter,
    oC_Parameter,
    parameters::lower_parameter,
    "$value",
    Expr::Parameter("value".into())
);
lowering!(
    schema_name,
    oC_SchemaName,
    schema::lower_schema_name,
    "`two words`",
    String::from("two words")
);
lowering!(
    namespace,
    oC_Namespace,
    schema::lower_namespace,
    "one.two.",
    vec![String::from("one"), String::from("two")]
);
lowering!(
    reserved_word,
    oC_ReservedWord,
    schema::lower_reserved_word,
    "MATCH",
    String::from("MATCH")
);

#[test]
fn literal_lowering_rejects_a_map_as_a_scalar_literal() {
    let lexer = CypherLexer::new(InputStream::new("{a:1}"));
    let mut parser = CypherParser::new(CommonTokenStream::new(lexer));
    let context = parser.oC_Literal().unwrap();
    let error = literals::lower_literal(context.as_ref()).unwrap_err();
    assert!(error.to_string().contains("non-literal expression"));
}
#[test]
fn operator_names_are_case_insensitive_and_reject_unknown_spellings() {
    for (text, expected) in [
        (" or ", BinaryOp::Or),
        ("and", BinaryOp::And),
        ("=", BinaryOp::Eq),
        ("<>", BinaryOp::Neq),
        ("!=", BinaryOp::Neq),
        ("<", BinaryOp::Lt),
        ("<=", BinaryOp::Lte),
        (">", BinaryOp::Gt),
        (">=", BinaryOp::Gte),
        ("+", BinaryOp::Add),
        ("-", BinaryOp::Sub),
        ("*", BinaryOp::Mul),
        ("/", BinaryOp::Div),
    ] {
        assert_eq!(operators::binary_operator(text).unwrap(), expected);
    }
    for (text, expected) in [(" not ", UnaryOp::Not), ("-", UnaryOp::Neg)] {
        assert_eq!(operators::unary_operator(text).unwrap(), expected);
    }
    for (text, expected) in [
        ("starts with", StringPredicateOp::StartsWith),
        ("startswith", StringPredicateOp::StartsWith),
        ("ends with", StringPredicateOp::EndsWith),
        ("endswith", StringPredicateOp::EndsWith),
        ("contains", StringPredicateOp::Contains),
        ("=~", StringPredicateOp::Regex),
    ] {
        assert_eq!(
            operators::string_predicate_operator(text).unwrap(),
            expected
        );
    }
    for text in ["", "mystery"] {
        assert!(operators::binary_operator(text).is_err());
        assert!(operators::unary_operator(text).is_err());
        assert!(operators::string_predicate_operator(text).is_err());
    }
}
#[test]
fn lowering_context_returns_the_first_error_or_the_supplied_value() {
    assert_eq!(context::ParseLowering::new().finish(42).unwrap(), 42);
    let mut state = context::ParseLowering::new();
    state.fail(CypherParseError::Parse("first".into()));
    state.fail(CypherParseError::Parse("second".into()));
    assert_eq!(state.finish(42).unwrap_err().to_string(), "parse: first");
    assert_eq!(
        context::unsupported::<()>("not supported")
            .unwrap_err()
            .to_string(),
        "unsupported cypher construct: not supported"
    );
    assert_eq!(
        context::missing::<()>("missing").unwrap_err().to_string(),
        "parse: missing"
    );
}
#[test]
fn frame_stack_is_lifo_and_empty_pop_is_safe() {
    let mut stack = frames::FrameStack::default();
    assert!(stack.is_empty());
    assert_eq!(stack.pop(), None);
    stack.push(frames::LoweringFrame::Name("first".into()));
    stack.push(frames::LoweringFrame::Expr(integer("2")));
    assert!(!stack.is_empty());
    assert_eq!(stack.pop(), Some(frames::LoweringFrame::Expr(integer("2"))));
    assert_eq!(
        stack.pop(),
        Some(frames::LoweringFrame::Name("first".into()))
    );
    assert!(stack.is_empty());
}
#[test]
fn diagnostics_keep_span_and_error_category() {
    for (span, suffix) in [
        (None, ""),
        (Some(source::SourceSpan { line: 3, column: 7 }), " at 3:7"),
    ] {
        assert_eq!(
            diagnostics::format_with_span(span, "message".into()),
            format!("message{suffix}")
        );
        assert_eq!(
            diagnostics::unsupported_at::<()>(span, "message")
                .unwrap_err()
                .to_string(),
            format!("unsupported cypher construct: message{suffix}")
        );
        assert_eq!(
            diagnostics::parse_error_at::<()>(span, "message")
                .unwrap_err()
                .to_string(),
            format!("parse: message{suffix}")
        );
    }
}
#[test]
fn text_helpers_preserve_source_and_normalize_only_delimiters() {
    assert_eq!(text::normalized_keyword("  mAtCh "), "MATCH");
    for (input, expected) in [
        (" `name` ", "name"),
        ("plain", "plain"),
        ("`open", "`open"),
        ("close`", "close`"),
    ] {
        assert_eq!(text::strip_backticks(input), expected);
    }
    let lexer = CypherLexer::new(InputStream::new("RETURN 1"));
    let mut parser = CypherParser::new(CommonTokenStream::new(lexer));
    let context = parser.oC_Cypher().unwrap();
    assert_eq!(text::raw_text(context.as_ref()), "RETURN 1<EOF>");
    assert_eq!(
        source::start_span(context.as_ref()),
        Some(source::SourceSpan { line: 1, column: 0 })
    );
    assert_eq!(
        super::lower_cypher(context.as_ref()).unwrap(),
        crate::language::cypher::parser::parse_query("RETURN 1").unwrap()
    );
}

lowering!(
    partial_comparison,
    oC_PartialComparisonExpression,
    predicates::lower_partial_comparison,
    ">= 2",
    (BinaryOp::Gte, integer("2"))
);
lowering!(
    starts_with,
    oC_StringPredicateExpression,
    predicates::lower_string_predicate_expression,
    " STARTS WITH 'a'",
    (
        StringPredicateOp::StartsWith,
        Expr::Literal(Literal::String("a".into()))
    )
);
lowering!(
    ends_with,
    oC_StringPredicateExpression,
    predicates::lower_string_predicate_expression,
    " ENDS WITH 'a'",
    (
        StringPredicateOp::EndsWith,
        Expr::Literal(Literal::String("a".into()))
    )
);
lowering!(
    contains,
    oC_StringPredicateExpression,
    predicates::lower_string_predicate_expression,
    " CONTAINS 'a'",
    (
        StringPredicateOp::Contains,
        Expr::Literal(Literal::String("a".into()))
    )
);

lowering!(
    list_predicate,
    oC_ListPredicateExpression,
    predicates::lower_list_predicate_expression,
    " IN [1,2]",
    Expr::List(vec![integer("1"), integer("2")])
);
lowering!(
    null_predicate,
    oC_NullPredicateExpression,
    predicates::lower_null_predicate_expression,
    " IS NULL",
    Expr::Literal(Literal::Bool(false))
);
lowering!(
    not_null_predicate,
    oC_NullPredicateExpression,
    predicates::lower_null_predicate_expression,
    " IS NOT NULL",
    Expr::Literal(Literal::Bool(true))
);
lowering!(
    property_expression,
    oC_PropertyExpression,
    properties::lower_property_expression,
    "n.x",
    Expr::Property {
        target: Box::new(Expr::Variable("n".into())),
        key: "x".into()
    }
);
lowering!(
    non_arithmetic,
    oC_NonArithmeticOperatorExpression,
    properties::lower_non_arithmetic_operator_expression,
    "n.x",
    Expr::Property {
        target: Box::new(Expr::Variable("n".into())),
        key: "x".into()
    }
);
lowering!(
    property_lookup,
    oC_PropertyLookup,
    properties::lower_property_lookup,
    ".x",
    String::from("x")
);
lowering!(
    property_key,
    oC_PropertyKeyName,
    properties::lower_property_key_name,
    "`two words`",
    String::from("two words")
);
lowering!(
    properties_parameter,
    oC_Properties,
    properties::lower_properties,
    "$props",
    Expr::Parameter("props".into())
);
lowering!(
    properties_map,
    oC_Properties,
    properties::lower_properties,
    "{x:1}",
    Expr::Map(vec![("x".into(), integer("1"))])
);
lowering!(
    left_arrow,
    oC_LeftArrowHead,
    ranges::lower_left_arrow_head,
    "<",
    ()
);
lowering!(
    right_arrow,
    oC_RightArrowHead,
    ranges::lower_right_arrow_head,
    ">",
    ()
);
lowering!(dash, oC_Dash, ranges::lower_dash, "-", ());
lowering!(
    variable,
    oC_Variable,
    names::lower_variable,
    "`two words`",
    String::from("two words")
);
lowering!(
    symbolic_name,
    oC_SymbolicName,
    names::lower_symbolic_name,
    "`two``words`",
    String::from("two`words")
);

macro_rules! malformed {
    ($name:ident,$rule:ident,$input:expr,$lower:path,$message:expr) => {
        #[test]
        fn $name() {
            use antlr4rust::{parser_rule_context::ParserRuleContext, tree::Tree};
            let errors = super::super::SyntaxErrors::default();
            let mut lexer = CypherLexer::new(InputStream::new($input));
            lexer.remove_error_listeners();
            lexer.add_error_listener(Box::new(errors.listener()));
            let mut parser = CypherParser::new(CommonTokenStream::new(lexer));
            parser.remove_error_listeners();
            parser.add_error_listener(Box::new(errors.listener()));
            let context = parser.$rule().unwrap();
            errors.into_result().expect("valid grammar fragment");
            while context.get_child_count() > 0 {
                context.remove_last_child();
            }
            let error = $lower(context.as_ref()).unwrap_err();
            assert!(error.to_string().contains($message), "{error}");
        }
    };
}
malformed!(
    missing_comparison_rhs,
    oC_PartialComparisonExpression,
    ">= 2",
    predicates::lower_partial_comparison,
    "missing right-hand expression"
);
malformed!(
    missing_string_rhs,
    oC_StringPredicateExpression,
    " CONTAINS 'x'",
    predicates::lower_string_predicate_expression,
    "missing right-hand expression"
);
malformed!(
    missing_list_rhs,
    oC_ListPredicateExpression,
    " IN [1]",
    predicates::lower_list_predicate_expression,
    "missing collection expression"
);
malformed!(
    missing_pattern,
    oC_PatternPredicate,
    "(a)-->(b)",
    predicates::lower_pattern_predicate,
    "missing relationships pattern"
);
malformed!(
    missing_properties,
    oC_Properties,
    "{a:1}",
    properties::lower_properties,
    "missing map literal or parameter"
);
malformed!(
    list_operator_without_target,
    oC_ListOperatorExpression,
    "[0]",
    properties::lower_list_operator_expression,
    "require their target expression"
);
#[test]
fn source_span_is_absent_for_a_synthetic_context() {
    use antlr4rust::parser_rule_context::ParserRuleContext;
    let lexer = CypherLexer::new(InputStream::new("RETURN 1"));
    let mut parser = CypherParser::new(CommonTokenStream::new(lexer));
    let context = parser.oC_Cypher().unwrap();
    context.set_start(None);
    assert_eq!(source::start_span(context.as_ref()), None);
}
#[test]
fn empty_visitor_reports_that_no_query_was_lowered() {
    assert!(
        visitor::AstLoweringVisitor::new()
            .finish()
            .unwrap_err()
            .to_string()
            .contains("no Cypher query lowered")
    );
}
#[test]
fn relationship_direction_handles_each_arrow_configuration() {
    use crate::ir::plan::Direction;
    for (left, right, expected) in [
        (false, false, Direction::Both),
        (true, false, Direction::In),
        (false, true, Direction::Out),
        (true, true, Direction::Both),
    ] {
        assert_eq!(ranges::relationship_direction(left, right), expected);
    }
}
// Recovery contexts must produce diagnostics instead of panicking or dropping clauses.
malformed!(
    empty_reading_clause,
    oC_ReadingClause,
    "MATCH (n)",
    clauses::lower_reading_clause,
    "missing MATCH, UNWIND, or CALL"
);
malformed!(
    match_without_pattern,
    oC_Match,
    "MATCH (n)",
    clauses::lower_match,
    "MATCH missing pattern"
);
malformed!(
    unwind_without_expression,
    oC_Unwind,
    "UNWIND [1] AS x",
    clauses::lower_unwind,
    "UNWIND missing expression"
);
malformed!(
    list_comprehension_without_filter,
    oC_ListComprehension,
    "[x IN [1] | x]",
    collections::lower_list_comprehension,
    "missing filter expression"
);
malformed!(
    pattern_comprehension_without_pattern,
    oC_PatternComprehension,
    "[(a)-->(b) | b]",
    collections::lower_pattern_comprehension,
    "missing relationships pattern"
);
malformed!(
    quantifier_without_filter,
    oC_Quantifier,
    "all(x IN [1] WHERE true)",
    collections::lower_quantifier,
    "missing filter expression"
);
malformed!(
    filter_without_iterator,
    oC_FilterExpression,
    "x IN [1] WHERE true",
    collections::lower_filter_expression,
    "missing id-in-collection"
);
malformed!(
    iterator_without_variable,
    oC_IdInColl,
    "x IN [1]",
    collections::lower_id_in_coll,
    "missing variable"
);
malformed!(
    query_without_body,
    oC_Query,
    "RETURN 1",
    queries::lower_query,
    "missing regular query or standalone CALL"
);
malformed!(
    regular_query_without_single_query,
    oC_RegularQuery,
    "RETURN 1",
    queries::lower_regular_query,
    "missing single query"
);
malformed!(
    single_query_without_body,
    oC_SingleQuery,
    "RETURN 1",
    queries::lower_single_query,
    "no query body"
);
malformed!(
    cypher_without_statement,
    oC_Cypher,
    "RETURN 1",
    statements::lower_cypher,
    "did not contain a statement"
);
malformed!(
    statement_without_query,
    oC_Statement,
    "RETURN 1",
    statements::lower_statement,
    "did not contain a query"
);
malformed!(
    call_without_invocation,
    oC_InQueryCall,
    "CALL db.labels()",
    procedures::lower_in_query_call,
    "missing procedure invocation"
);
malformed!(
    standalone_call_without_invocation,
    oC_StandaloneCall,
    "CALL db.labels()",
    procedures::lower_standalone_call,
    "missing procedure invocation"
);
malformed!(
    explicit_invocation_without_name,
    oC_ExplicitProcedureInvocation,
    "db.labels()",
    procedures::lower_explicit_procedure_invocation,
    "missing name"
);
malformed!(
    implicit_invocation_without_name,
    oC_ImplicitProcedureInvocation,
    "db.labels",
    procedures::lower_implicit_procedure_invocation,
    "missing name"
);
malformed!(
    yield_without_variable,
    oC_YieldItem,
    "label AS x",
    procedures::lower_yield_item,
    "missing output variable"
);
malformed!(
    procedure_field_without_name,
    oC_ProcedureResultField,
    "label",
    procedures::lower_procedure_result_field,
    "missing name"
);
malformed!(
    procedure_name_without_name,
    oC_ProcedureName,
    "db.labels",
    procedures::lower_procedure_name,
    "missing symbolic name"
);
malformed!(
    updating_without_clause,
    oC_UpdatingClause,
    "CREATE (n)",
    updating::lower_updating_clause,
    "missing CREATE, MERGE, DELETE, SET, or REMOVE"
);
malformed!(
    create_without_pattern,
    oC_Create,
    "CREATE (n)",
    updating::lower_create,
    "CREATE missing pattern"
);
malformed!(
    merge_without_pattern,
    oC_Merge,
    "MERGE (n)",
    updating::lower_merge,
    "MERGE missing pattern"
);
malformed!(
    set_without_target,
    oC_SetItem,
    "n.x=1",
    updating::lower_set_item,
    "missing variable or property expression"
);
malformed!(
    delete_without_expression,
    oC_Delete,
    "DELETE n",
    updating::lower_delete,
    "DELETE missing expression"
);
malformed!(
    remove_without_target,
    oC_RemoveItem,
    "n.x",
    updating::lower_remove_item,
    "REMOVE missing variable"
);
malformed!(
    projection_without_items,
    oC_ProjectionBody,
    " 1 AS x",
    projections::lower_projection_body,
    "missing projection items"
);
malformed!(
    projection_item_without_expression,
    oC_ProjectionItem,
    "1 AS x",
    projections::lower_projection_item,
    "missing expression"
);
malformed!(
    sort_without_expression,
    oC_SortItem,
    "1 ASC",
    projections::lower_sort_item,
    "missing expression"
);
malformed!(
    skip_without_expression,
    oC_Skip,
    "SKIP 1",
    projections::lower_skip,
    "missing expression"
);
malformed!(
    limit_without_expression,
    oC_Limit,
    "LIMIT 1",
    projections::lower_limit,
    "missing expression"
);
#[test]
fn required_projection_body_rejects_absence() {
    assert!(
        projections::lower_projection_body_required(None)
            .unwrap_err()
            .to_string()
            .contains("projection body missing")
    );
}
malformed!(
    relationships_without_start,
    oC_RelationshipsPattern,
    "(a)-->(b)",
    patterns::lower_relationships_pattern,
    "missing start node"
);
malformed!(
    pattern_part_without_anonymous,
    oC_PatternPart,
    "p=(a)-->(b)",
    patterns::lower_pattern_part,
    "missing anonymous pattern"
);
malformed!(
    pattern_element_without_start,
    oC_PatternElement,
    "(a)-->(b)",
    patterns::lower_pattern_element,
    "missing start node"
);
malformed!(
    pattern_chain_without_relationship,
    oC_PatternElementChain,
    "-->(b)",
    patterns::lower_pattern_chain,
    "missing relationship"
);
malformed!(
    expression_without_or,
    oC_Expression,
    "1",
    expressions::lower_expression,
    "missing OR expression"
);
malformed!(
    or_without_operand,
    oC_OrExpression,
    "true OR false",
    expressions::lower_or_expression,
    "missing child expression"
);
malformed!(
    and_without_operand,
    oC_AndExpression,
    "true AND false",
    expressions::lower_and_expression,
    "missing child expression"
);
malformed!(
    xor_without_operand,
    oC_XorExpression,
    "true XOR false",
    expressions::lower_xor_expression,
    "missing child expression"
);
malformed!(
    not_without_comparison,
    oC_NotExpression,
    "NOT true",
    expressions::lower_not_expression,
    "missing comparison"
);
malformed!(
    comparison_without_left,
    oC_ComparisonExpression,
    "1<2",
    expressions::lower_comparison_expression,
    "missing left expression"
);
malformed!(
    predicate_without_base,
    oC_StringListNullPredicateExpression,
    "1 IN [1]",
    expressions::lower_string_list_null_predicate_expression,
    "missing base expression"
);
malformed!(
    addition_without_operand,
    oC_AddOrSubtractExpression,
    "1+2",
    expressions::lower_add_or_subtract_expression,
    "missing child expression"
);
malformed!(
    multiplication_without_operand,
    oC_MultiplyDivideModuloExpression,
    "1*2",
    expressions::lower_multiply_divide_modulo_expression,
    "missing child expression"
);
malformed!(
    power_without_operand,
    oC_PowerOfExpression,
    "1^2",
    expressions::lower_power_expression,
    "missing child expression"
);
malformed!(
    unary_without_operand,
    oC_UnaryAddOrSubtractExpression,
    "-x",
    expressions::lower_unary_add_or_subtract_expression,
    "missing operand"
);
malformed!(
    property_without_atom,
    oC_PropertyExpression,
    "n.x",
    expressions::lower_property_expression,
    "missing atom"
);
malformed!(
    non_arithmetic_without_atom,
    oC_NonArithmeticOperatorExpression,
    "n.x",
    expressions::lower_non_arithmetic_operator_expression,
    "missing atom"
);
malformed!(
    atom_without_kind,
    oC_Atom,
    "1",
    expressions::lower_atom,
    "is not lowerable to the current graph IR"
);
malformed!(
    function_without_name,
    oC_FunctionInvocation,
    "abs(1)",
    expressions::lower_function_invocation,
    "missing name"
);
malformed!(
    cast_without_value,
    oC_CastExpression,
    "CAST(1 AS INT64)",
    expressions::lower_cast_expression,
    "missing value"
);
malformed!(
    function_name_without_symbol,
    oC_FunctionName,
    "math.abs",
    expressions::lower_function_name,
    "missing symbolic name"
);
malformed!(
    case_without_when,
    oC_CaseAlternative,
    "WHEN true THEN 1",
    expressions::lower_case_alternative,
    "missing WHEN expression"
);
malformed!(
    literal_without_kind,
    oC_Literal,
    "1",
    expressions::lower_literal_expr,
    "unrecognized Cypher literal"
);
malformed!(
    parameter_without_name,
    oC_Parameter,
    "$x",
    expressions::lower_parameter,
    "missing name"
);
malformed!(
    property_lookup_without_key,
    oC_PropertyLookup,
    ".x",
    expressions::lower_property_lookup,
    "missing property key"
);

lowering!(
    explicit_list_literal_entrypoint_preserves_nested_values,
    oC_ListLiteral,
    collections::lower_list_literal,
    "[null, 0x10, [true, 'quoted']]",
    Expr::List(vec![
        Expr::Literal(Literal::Null), Expr::Literal(Literal::Integer("16".into())),
        Expr::List(vec![Expr::Literal(Literal::Bool(true)),Expr::Literal(Literal::String("quoted".into()))]),
    ])
);
lowering!(empty_list_literal_entrypoint,oC_ListLiteral,collections::lower_list_literal,"[]",Expr::List(vec![]));

#[test]
fn radix_integer_overflow_reports_the_original_literal() {
    for text in ["0x100000000000000000000000000000000", "0o4000000000000000000000000000000000000000000"] {
        let lexer=CypherLexer::new(InputStream::new(text));
        let mut parser=CypherParser::new(CommonTokenStream::new(lexer));
        let tree=parser.oC_IntegerLiteral().unwrap();
        let error=expressions::lower_integer_literal(tree.as_ref()).unwrap_err().to_string();
        assert!(error.contains("invalid integer literal") && error.contains(text),"{error}");
    }
}
