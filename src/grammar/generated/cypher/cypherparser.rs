// Generated from languages/cypher/Cypher.g4 by ANTLR 4.13.2
#![allow(dead_code)]
#![allow(non_snake_case)]
#![allow(non_upper_case_globals)]
#![allow(nonstandard_style)]
#![allow(unused_imports)]
#![allow(unused_mut)]
#![allow(unused_braces)]
use super::cypherlistener::*;
use super::cyphervisitor::*;
use antlr4rust::PredictionContextCache;
use antlr4rust::TokenSource;
use antlr4rust::atn::{ATN, INVALID_ALT};
use antlr4rust::atn_deserializer::ATNDeserializer;
use antlr4rust::dfa::DFA;
use antlr4rust::error_strategy::{DefaultErrorStrategy, ErrorStrategy};
use antlr4rust::errors::*;
use antlr4rust::int_stream::EOF;
use antlr4rust::parser::{BaseParser, Parser, ParserNodeType, ParserRecog};
use antlr4rust::parser_atn_simulator::ParserATNSimulator;
use antlr4rust::parser_rule_context::{BaseParserRuleContext, ParserRuleContext, cast, cast_mut};
use antlr4rust::recognizer::{Actions, Recognizer};
use antlr4rust::rule_context::{BaseRuleContext, CustomRuleContext, RuleContext};
use antlr4rust::token::{OwningToken, TOKEN_EOF, Token};
use antlr4rust::token_factory::{CommonTokenFactory, TokenAware, TokenFactory};
use antlr4rust::token_stream::TokenStream;
use antlr4rust::tree::*;
use antlr4rust::vocabulary::{Vocabulary, VocabularyImpl};

use antlr4rust::lazy_static;
use antlr4rust::{TidAble, TidExt};

use std::any::{Any, TypeId};
use std::borrow::{Borrow, BorrowMut};
use std::cell::RefCell;
use std::convert::TryFrom;
use std::marker::PhantomData;
use std::ops::{Deref, DerefMut};
use std::rc::Rc;
use std::sync::Arc;

pub const Cypher_T__0: i32 = 1;
pub const Cypher_T__1: i32 = 2;
pub const Cypher_T__2: i32 = 3;
pub const Cypher_T__3: i32 = 4;
pub const Cypher_T__4: i32 = 5;
pub const Cypher_T__5: i32 = 6;
pub const Cypher_T__6: i32 = 7;
pub const Cypher_T__7: i32 = 8;
pub const Cypher_T__8: i32 = 9;
pub const Cypher_T__9: i32 = 10;
pub const Cypher_T__10: i32 = 11;
pub const Cypher_T__11: i32 = 12;
pub const Cypher_T__12: i32 = 13;
pub const Cypher_T__13: i32 = 14;
pub const Cypher_T__14: i32 = 15;
pub const Cypher_T__15: i32 = 16;
pub const Cypher_T__16: i32 = 17;
pub const Cypher_T__17: i32 = 18;
pub const Cypher_T__18: i32 = 19;
pub const Cypher_T__19: i32 = 20;
pub const Cypher_T__20: i32 = 21;
pub const Cypher_T__21: i32 = 22;
pub const Cypher_T__22: i32 = 23;
pub const Cypher_T__23: i32 = 24;
pub const Cypher_T__24: i32 = 25;
pub const Cypher_T__25: i32 = 26;
pub const Cypher_T__26: i32 = 27;
pub const Cypher_T__27: i32 = 28;
pub const Cypher_T__28: i32 = 29;
pub const Cypher_T__29: i32 = 30;
pub const Cypher_T__30: i32 = 31;
pub const Cypher_T__31: i32 = 32;
pub const Cypher_T__32: i32 = 33;
pub const Cypher_T__33: i32 = 34;
pub const Cypher_T__34: i32 = 35;
pub const Cypher_T__35: i32 = 36;
pub const Cypher_T__36: i32 = 37;
pub const Cypher_T__37: i32 = 38;
pub const Cypher_T__38: i32 = 39;
pub const Cypher_T__39: i32 = 40;
pub const Cypher_T__40: i32 = 41;
pub const Cypher_T__41: i32 = 42;
pub const Cypher_T__42: i32 = 43;
pub const Cypher_T__43: i32 = 44;
pub const Cypher_T__44: i32 = 45;
pub const Cypher_UNION: i32 = 46;
pub const Cypher_ALL: i32 = 47;
pub const Cypher_OPTIONAL: i32 = 48;
pub const Cypher_MATCH: i32 = 49;
pub const Cypher_UNWIND: i32 = 50;
pub const Cypher_AS: i32 = 51;
pub const Cypher_MERGE: i32 = 52;
pub const Cypher_ON: i32 = 53;
pub const Cypher_CREATE: i32 = 54;
pub const Cypher_SET: i32 = 55;
pub const Cypher_DETACH: i32 = 56;
pub const Cypher_DELETE: i32 = 57;
pub const Cypher_REMOVE: i32 = 58;
pub const Cypher_CALL: i32 = 59;
pub const Cypher_YIELD: i32 = 60;
pub const Cypher_WITH: i32 = 61;
pub const Cypher_RETURN: i32 = 62;
pub const Cypher_DISTINCT: i32 = 63;
pub const Cypher_ORDER: i32 = 64;
pub const Cypher_BY: i32 = 65;
pub const Cypher_L_SKIP: i32 = 66;
pub const Cypher_LIMIT: i32 = 67;
pub const Cypher_ASCENDING: i32 = 68;
pub const Cypher_ASC: i32 = 69;
pub const Cypher_DESCENDING: i32 = 70;
pub const Cypher_DESC: i32 = 71;
pub const Cypher_WHERE: i32 = 72;
pub const Cypher_OR: i32 = 73;
pub const Cypher_XOR: i32 = 74;
pub const Cypher_AND: i32 = 75;
pub const Cypher_NOT: i32 = 76;
pub const Cypher_STARTS: i32 = 77;
pub const Cypher_ENDS: i32 = 78;
pub const Cypher_CONTAINS: i32 = 79;
pub const Cypher_IN: i32 = 80;
pub const Cypher_IS: i32 = 81;
pub const Cypher_NULL: i32 = 82;
pub const Cypher_COUNT: i32 = 83;
pub const Cypher_CASE: i32 = 84;
pub const Cypher_ELSE: i32 = 85;
pub const Cypher_END: i32 = 86;
pub const Cypher_WHEN: i32 = 87;
pub const Cypher_THEN: i32 = 88;
pub const Cypher_ANY: i32 = 89;
pub const Cypher_NONE: i32 = 90;
pub const Cypher_SINGLE: i32 = 91;
pub const Cypher_CAST: i32 = 92;
pub const Cypher_EXISTS: i32 = 93;
pub const Cypher_TRUE: i32 = 94;
pub const Cypher_FALSE: i32 = 95;
pub const Cypher_HexInteger: i32 = 96;
pub const Cypher_DecimalInteger: i32 = 97;
pub const Cypher_OctalInteger: i32 = 98;
pub const Cypher_HexLetter: i32 = 99;
pub const Cypher_HexDigit: i32 = 100;
pub const Cypher_Digit: i32 = 101;
pub const Cypher_NonZeroDigit: i32 = 102;
pub const Cypher_NonZeroOctDigit: i32 = 103;
pub const Cypher_OctDigit: i32 = 104;
pub const Cypher_ZeroDigit: i32 = 105;
pub const Cypher_ExponentDecimalReal: i32 = 106;
pub const Cypher_RegularDecimalReal: i32 = 107;
pub const Cypher_StringLiteral: i32 = 108;
pub const Cypher_EscapedChar: i32 = 109;
pub const Cypher_CONSTRAINT: i32 = 110;
pub const Cypher_DO: i32 = 111;
pub const Cypher_FOR: i32 = 112;
pub const Cypher_REQUIRE: i32 = 113;
pub const Cypher_UNIQUE: i32 = 114;
pub const Cypher_MANDATORY: i32 = 115;
pub const Cypher_SCALAR: i32 = 116;
pub const Cypher_OF: i32 = 117;
pub const Cypher_ADD: i32 = 118;
pub const Cypher_DROP: i32 = 119;
pub const Cypher_FILTER: i32 = 120;
pub const Cypher_EXTRACT: i32 = 121;
pub const Cypher_UnescapedSymbolicName: i32 = 122;
pub const Cypher_IdentifierStart: i32 = 123;
pub const Cypher_IdentifierPart: i32 = 124;
pub const Cypher_EscapedSymbolicName: i32 = 125;
pub const Cypher_SP: i32 = 126;
pub const Cypher_WHITESPACE: i32 = 127;
pub const Cypher_Comment: i32 = 128;
pub const Cypher_EOF: i32 = EOF;
pub const RULE_oC_Cypher: usize = 0;
pub const RULE_oC_Statement: usize = 1;
pub const RULE_oC_Query: usize = 2;
pub const RULE_oC_RegularQuery: usize = 3;
pub const RULE_oC_Union: usize = 4;
pub const RULE_oC_SingleQuery: usize = 5;
pub const RULE_oC_SinglePartQuery: usize = 6;
pub const RULE_oC_MultiPartQuery: usize = 7;
pub const RULE_oC_UpdatingClause: usize = 8;
pub const RULE_oC_ReadingClause: usize = 9;
pub const RULE_oC_Match: usize = 10;
pub const RULE_oC_Unwind: usize = 11;
pub const RULE_oC_Merge: usize = 12;
pub const RULE_oC_MergeAction: usize = 13;
pub const RULE_oC_Create: usize = 14;
pub const RULE_oC_Set: usize = 15;
pub const RULE_oC_SetItem: usize = 16;
pub const RULE_oC_Delete: usize = 17;
pub const RULE_oC_Remove: usize = 18;
pub const RULE_oC_RemoveItem: usize = 19;
pub const RULE_oC_InQueryCall: usize = 20;
pub const RULE_oC_StandaloneCall: usize = 21;
pub const RULE_oC_YieldItems: usize = 22;
pub const RULE_oC_YieldItem: usize = 23;
pub const RULE_oC_With: usize = 24;
pub const RULE_oC_Return: usize = 25;
pub const RULE_oC_ProjectionBody: usize = 26;
pub const RULE_oC_ProjectionItems: usize = 27;
pub const RULE_oC_ProjectionItem: usize = 28;
pub const RULE_oC_Order: usize = 29;
pub const RULE_oC_Skip: usize = 30;
pub const RULE_oC_Limit: usize = 31;
pub const RULE_oC_SortItem: usize = 32;
pub const RULE_oC_Where: usize = 33;
pub const RULE_oC_Pattern: usize = 34;
pub const RULE_oC_PatternPart: usize = 35;
pub const RULE_oC_AnonymousPatternPart: usize = 36;
pub const RULE_oC_PatternElement: usize = 37;
pub const RULE_oC_RelationshipsPattern: usize = 38;
pub const RULE_oC_NodePattern: usize = 39;
pub const RULE_oC_PatternElementChain: usize = 40;
pub const RULE_oC_RelationshipPattern: usize = 41;
pub const RULE_oC_RelationshipDetail: usize = 42;
pub const RULE_oC_RecursiveRelationshipFilter: usize = 43;
pub const RULE_oC_Properties: usize = 44;
pub const RULE_oC_RelationshipTypes: usize = 45;
pub const RULE_oC_NodeLabels: usize = 46;
pub const RULE_oC_NodeLabel: usize = 47;
pub const RULE_oC_RangeLiteral: usize = 48;
pub const RULE_oC_LabelName: usize = 49;
pub const RULE_oC_RelTypeName: usize = 50;
pub const RULE_oC_PropertyExpression: usize = 51;
pub const RULE_oC_Expression: usize = 52;
pub const RULE_oC_OrExpression: usize = 53;
pub const RULE_oC_XorExpression: usize = 54;
pub const RULE_oC_AndExpression: usize = 55;
pub const RULE_oC_NotExpression: usize = 56;
pub const RULE_oC_ComparisonExpression: usize = 57;
pub const RULE_oC_PartialComparisonExpression: usize = 58;
pub const RULE_oC_StringListNullPredicateExpression: usize = 59;
pub const RULE_oC_StringPredicateExpression: usize = 60;
pub const RULE_oC_ListPredicateExpression: usize = 61;
pub const RULE_oC_NullPredicateExpression: usize = 62;
pub const RULE_oC_AddOrSubtractExpression: usize = 63;
pub const RULE_oC_MultiplyDivideModuloExpression: usize = 64;
pub const RULE_oC_PowerOfExpression: usize = 65;
pub const RULE_oC_UnaryAddOrSubtractExpression: usize = 66;
pub const RULE_oC_NonArithmeticOperatorExpression: usize = 67;
pub const RULE_oC_ListOperatorExpression: usize = 68;
pub const RULE_oC_PropertyLookup: usize = 69;
pub const RULE_oC_Atom: usize = 70;
pub const RULE_oC_CaseExpression: usize = 71;
pub const RULE_oC_CaseAlternative: usize = 72;
pub const RULE_oC_ListComprehension: usize = 73;
pub const RULE_oC_PatternComprehension: usize = 74;
pub const RULE_oC_Quantifier: usize = 75;
pub const RULE_oC_FilterExpression: usize = 76;
pub const RULE_oC_PatternPredicate: usize = 77;
pub const RULE_oC_ParenthesizedExpression: usize = 78;
pub const RULE_oC_IdInColl: usize = 79;
pub const RULE_oC_FunctionInvocation: usize = 80;
pub const RULE_oC_CastExpression: usize = 81;
pub const RULE_oC_CastType: usize = 82;
pub const RULE_oC_CastTypeArgument: usize = 83;
pub const RULE_oC_CastTypeField: usize = 84;
pub const RULE_oC_CastTypeName: usize = 85;
pub const RULE_oC_FunctionName: usize = 86;
pub const RULE_oC_ExistentialSubquery: usize = 87;
pub const RULE_oC_ExplicitProcedureInvocation: usize = 88;
pub const RULE_oC_ImplicitProcedureInvocation: usize = 89;
pub const RULE_oC_ProcedureResultField: usize = 90;
pub const RULE_oC_ProcedureName: usize = 91;
pub const RULE_oC_Namespace: usize = 92;
pub const RULE_oC_Variable: usize = 93;
pub const RULE_oC_Literal: usize = 94;
pub const RULE_oC_BooleanLiteral: usize = 95;
pub const RULE_oC_NumberLiteral: usize = 96;
pub const RULE_oC_IntegerLiteral: usize = 97;
pub const RULE_oC_DoubleLiteral: usize = 98;
pub const RULE_oC_ListLiteral: usize = 99;
pub const RULE_oC_MapLiteral: usize = 100;
pub const RULE_oC_PropertyKeyName: usize = 101;
pub const RULE_oC_Parameter: usize = 102;
pub const RULE_oC_SchemaName: usize = 103;
pub const RULE_oC_ReservedWord: usize = 104;
pub const RULE_oC_SymbolicName: usize = 105;
pub const RULE_oC_LeftArrowHead: usize = 106;
pub const RULE_oC_RightArrowHead: usize = 107;
pub const RULE_oC_Dash: usize = 108;
pub const ruleNames: [&'static str; 109] = [
    "oC_Cypher",
    "oC_Statement",
    "oC_Query",
    "oC_RegularQuery",
    "oC_Union",
    "oC_SingleQuery",
    "oC_SinglePartQuery",
    "oC_MultiPartQuery",
    "oC_UpdatingClause",
    "oC_ReadingClause",
    "oC_Match",
    "oC_Unwind",
    "oC_Merge",
    "oC_MergeAction",
    "oC_Create",
    "oC_Set",
    "oC_SetItem",
    "oC_Delete",
    "oC_Remove",
    "oC_RemoveItem",
    "oC_InQueryCall",
    "oC_StandaloneCall",
    "oC_YieldItems",
    "oC_YieldItem",
    "oC_With",
    "oC_Return",
    "oC_ProjectionBody",
    "oC_ProjectionItems",
    "oC_ProjectionItem",
    "oC_Order",
    "oC_Skip",
    "oC_Limit",
    "oC_SortItem",
    "oC_Where",
    "oC_Pattern",
    "oC_PatternPart",
    "oC_AnonymousPatternPart",
    "oC_PatternElement",
    "oC_RelationshipsPattern",
    "oC_NodePattern",
    "oC_PatternElementChain",
    "oC_RelationshipPattern",
    "oC_RelationshipDetail",
    "oC_RecursiveRelationshipFilter",
    "oC_Properties",
    "oC_RelationshipTypes",
    "oC_NodeLabels",
    "oC_NodeLabel",
    "oC_RangeLiteral",
    "oC_LabelName",
    "oC_RelTypeName",
    "oC_PropertyExpression",
    "oC_Expression",
    "oC_OrExpression",
    "oC_XorExpression",
    "oC_AndExpression",
    "oC_NotExpression",
    "oC_ComparisonExpression",
    "oC_PartialComparisonExpression",
    "oC_StringListNullPredicateExpression",
    "oC_StringPredicateExpression",
    "oC_ListPredicateExpression",
    "oC_NullPredicateExpression",
    "oC_AddOrSubtractExpression",
    "oC_MultiplyDivideModuloExpression",
    "oC_PowerOfExpression",
    "oC_UnaryAddOrSubtractExpression",
    "oC_NonArithmeticOperatorExpression",
    "oC_ListOperatorExpression",
    "oC_PropertyLookup",
    "oC_Atom",
    "oC_CaseExpression",
    "oC_CaseAlternative",
    "oC_ListComprehension",
    "oC_PatternComprehension",
    "oC_Quantifier",
    "oC_FilterExpression",
    "oC_PatternPredicate",
    "oC_ParenthesizedExpression",
    "oC_IdInColl",
    "oC_FunctionInvocation",
    "oC_CastExpression",
    "oC_CastType",
    "oC_CastTypeArgument",
    "oC_CastTypeField",
    "oC_CastTypeName",
    "oC_FunctionName",
    "oC_ExistentialSubquery",
    "oC_ExplicitProcedureInvocation",
    "oC_ImplicitProcedureInvocation",
    "oC_ProcedureResultField",
    "oC_ProcedureName",
    "oC_Namespace",
    "oC_Variable",
    "oC_Literal",
    "oC_BooleanLiteral",
    "oC_NumberLiteral",
    "oC_IntegerLiteral",
    "oC_DoubleLiteral",
    "oC_ListLiteral",
    "oC_MapLiteral",
    "oC_PropertyKeyName",
    "oC_Parameter",
    "oC_SchemaName",
    "oC_ReservedWord",
    "oC_SymbolicName",
    "oC_LeftArrowHead",
    "oC_RightArrowHead",
    "oC_Dash",
];

pub const _LITERAL_NAMES: [Option<&'static str>; 106] = [
    None,
    Some("';'"),
    Some("','"),
    Some("'='"),
    Some("'+='"),
    Some("'*'"),
    Some("'('"),
    Some("')'"),
    Some("'['"),
    Some("']'"),
    Some("':'"),
    Some("'|'"),
    Some("'..'"),
    Some("'<>'"),
    Some("'<'"),
    Some("'>'"),
    Some("'<='"),
    Some("'>='"),
    Some("'+'"),
    Some("'-'"),
    Some("'/'"),
    Some("'%'"),
    Some("'^'"),
    Some("'.'"),
    Some("'{'"),
    Some("'}'"),
    Some("'$'"),
    Some("'\\u27E8'"),
    Some("'\\u3008'"),
    Some("'\\uFE64'"),
    Some("'\\uFF1C'"),
    Some("'\\u27E9'"),
    Some("'\\u3009'"),
    Some("'\\uFE65'"),
    Some("'\\uFF1E'"),
    Some("'\\u00AD'"),
    Some("'\\u2010'"),
    Some("'\\u2011'"),
    Some("'\\u2012'"),
    Some("'\\u2013'"),
    Some("'\\u2014'"),
    Some("'\\u2015'"),
    Some("'\\u2212'"),
    Some("'\\uFE58'"),
    Some("'\\uFE63'"),
    Some("'\\uFF0D'"),
    None,
    None,
    None,
    None,
    None,
    None,
    None,
    None,
    None,
    None,
    None,
    None,
    None,
    None,
    None,
    None,
    None,
    None,
    None,
    None,
    None,
    None,
    None,
    None,
    None,
    None,
    None,
    None,
    None,
    None,
    None,
    None,
    None,
    None,
    None,
    None,
    None,
    None,
    None,
    None,
    None,
    None,
    None,
    None,
    None,
    None,
    None,
    None,
    None,
    None,
    None,
    None,
    None,
    None,
    None,
    None,
    None,
    None,
    None,
    Some("'0'"),
];
pub const _SYMBOLIC_NAMES: [Option<&'static str>; 129] = [
    None,
    None,
    None,
    None,
    None,
    None,
    None,
    None,
    None,
    None,
    None,
    None,
    None,
    None,
    None,
    None,
    None,
    None,
    None,
    None,
    None,
    None,
    None,
    None,
    None,
    None,
    None,
    None,
    None,
    None,
    None,
    None,
    None,
    None,
    None,
    None,
    None,
    None,
    None,
    None,
    None,
    None,
    None,
    None,
    None,
    None,
    Some("UNION"),
    Some("ALL"),
    Some("OPTIONAL"),
    Some("MATCH"),
    Some("UNWIND"),
    Some("AS"),
    Some("MERGE"),
    Some("ON"),
    Some("CREATE"),
    Some("SET"),
    Some("DETACH"),
    Some("DELETE"),
    Some("REMOVE"),
    Some("CALL"),
    Some("YIELD"),
    Some("WITH"),
    Some("RETURN"),
    Some("DISTINCT"),
    Some("ORDER"),
    Some("BY"),
    Some("L_SKIP"),
    Some("LIMIT"),
    Some("ASCENDING"),
    Some("ASC"),
    Some("DESCENDING"),
    Some("DESC"),
    Some("WHERE"),
    Some("OR"),
    Some("XOR"),
    Some("AND"),
    Some("NOT"),
    Some("STARTS"),
    Some("ENDS"),
    Some("CONTAINS"),
    Some("IN"),
    Some("IS"),
    Some("NULL"),
    Some("COUNT"),
    Some("CASE"),
    Some("ELSE"),
    Some("END"),
    Some("WHEN"),
    Some("THEN"),
    Some("ANY"),
    Some("NONE"),
    Some("SINGLE"),
    Some("CAST"),
    Some("EXISTS"),
    Some("TRUE"),
    Some("FALSE"),
    Some("HexInteger"),
    Some("DecimalInteger"),
    Some("OctalInteger"),
    Some("HexLetter"),
    Some("HexDigit"),
    Some("Digit"),
    Some("NonZeroDigit"),
    Some("NonZeroOctDigit"),
    Some("OctDigit"),
    Some("ZeroDigit"),
    Some("ExponentDecimalReal"),
    Some("RegularDecimalReal"),
    Some("StringLiteral"),
    Some("EscapedChar"),
    Some("CONSTRAINT"),
    Some("DO"),
    Some("FOR"),
    Some("REQUIRE"),
    Some("UNIQUE"),
    Some("MANDATORY"),
    Some("SCALAR"),
    Some("OF"),
    Some("ADD"),
    Some("DROP"),
    Some("FILTER"),
    Some("EXTRACT"),
    Some("UnescapedSymbolicName"),
    Some("IdentifierStart"),
    Some("IdentifierPart"),
    Some("EscapedSymbolicName"),
    Some("SP"),
    Some("WHITESPACE"),
    Some("Comment"),
];
lazy_static! {
    static ref _shared_context_cache: Arc<PredictionContextCache> =
        Arc::new(PredictionContextCache::new());
    static ref VOCABULARY: Box<dyn Vocabulary> = Box::new(VocabularyImpl::new(
        _LITERAL_NAMES.iter(),
        _SYMBOLIC_NAMES.iter(),
        None
    ));
}

type BaseParserType<'input, I> = BaseParser<
    'input,
    CypherParserExt<'input>,
    I,
    CypherParserContextType,
    dyn CypherListener<'input> + 'input,
>;

type TokenType<'input> = <LocalTokenFactory<'input> as TokenFactory<'input>>::Tok;
pub type LocalTokenFactory<'input> = CommonTokenFactory;

pub type CypherTreeWalker<'input, 'a> =
    ParseTreeWalker<'input, 'a, CypherParserContextType, dyn CypherListener<'input> + 'a>;

/// Parser for Cypher grammar
pub struct CypherParser<'input, I>
where
    I: TokenStream<'input, TF = LocalTokenFactory<'input>> + TidAble<'input>,
{
    base: BaseParserType<'input, I>,
    interpreter: Arc<ParserATNSimulator>,
    _shared_context_cache: Box<PredictionContextCache>,
    pub err_handler: Box<dyn ErrorStrategy<'input, BaseParserType<'input, I>>>,
}

impl<'input, I> CypherParser<'input, I>
where
    I: TokenStream<'input, TF = LocalTokenFactory<'input>> + TidAble<'input>,
{
    pub fn set_error_strategy(
        &mut self,
        strategy: Box<dyn ErrorStrategy<'input, BaseParserType<'input, I>>>,
    ) {
        self.err_handler = strategy
    }

    pub fn with_strategy(
        input: I,
        strategy: Box<dyn ErrorStrategy<'input, BaseParserType<'input, I>>>,
    ) -> Self {
        antlr4rust::recognizer::check_version("0", "5");
        let interpreter = Arc::new(ParserATNSimulator::new(
            _ATN.clone(),
            _decision_to_DFA.clone(),
            _shared_context_cache.clone(),
        ));
        Self {
            base: BaseParser::new_base_parser(
                input,
                Arc::clone(&interpreter),
                CypherParserExt {
                    _pd: Default::default(),
                },
            ),
            interpreter,
            _shared_context_cache: Box::new(PredictionContextCache::new()),
            err_handler: strategy,
        }
    }
}

type DynStrategy<'input, I> = Box<dyn ErrorStrategy<'input, BaseParserType<'input, I>> + 'input>;

impl<'input, I> CypherParser<'input, I>
where
    I: TokenStream<'input, TF = LocalTokenFactory<'input>> + TidAble<'input>,
{
    pub fn with_dyn_strategy(input: I) -> Self {
        Self::with_strategy(input, Box::new(DefaultErrorStrategy::new()))
    }
}

impl<'input, I> CypherParser<'input, I>
where
    I: TokenStream<'input, TF = LocalTokenFactory<'input>> + TidAble<'input>,
{
    pub fn new(input: I) -> Self {
        Self::with_strategy(input, Box::new(DefaultErrorStrategy::new()))
    }
}

/// Trait for monomorphized trait object that corresponds to the nodes of parse tree generated for CypherParser
pub trait CypherParserContext<'input>:
    for<'x> Listenable<dyn CypherListener<'input> + 'x>
    + for<'x> Visitable<dyn CypherVisitor<'input> + 'x>
    + ParserRuleContext<'input, TF = LocalTokenFactory<'input>, Ctx = CypherParserContextType>
{
}

antlr4rust::coerce_from! { 'input : CypherParserContext<'input> }

impl<'input, 'x, T> VisitableDyn<T> for dyn CypherParserContext<'input> + 'input
where
    T: CypherVisitor<'input> + 'x,
{
    fn accept_dyn(&self, visitor: &mut T) {
        self.accept(visitor as &mut (dyn CypherVisitor<'input> + 'x))
    }
}

impl<'input> CypherParserContext<'input> for TerminalNode<'input, CypherParserContextType> {}
impl<'input> CypherParserContext<'input> for ErrorNode<'input, CypherParserContextType> {}

antlr4rust::tid! { impl<'input> TidAble<'input> for dyn CypherParserContext<'input> + 'input }

antlr4rust::tid! { impl<'input> TidAble<'input> for dyn CypherListener<'input> + 'input }

pub struct CypherParserContextType;
antlr4rust::tid! {CypherParserContextType}

impl<'input> ParserNodeType<'input> for CypherParserContextType {
    type TF = LocalTokenFactory<'input>;
    type Type = dyn CypherParserContext<'input> + 'input;
}

impl<'input, I> Deref for CypherParser<'input, I>
where
    I: TokenStream<'input, TF = LocalTokenFactory<'input>> + TidAble<'input>,
{
    type Target = BaseParserType<'input, I>;

    fn deref(&self) -> &Self::Target {
        &self.base
    }
}

impl<'input, I> DerefMut for CypherParser<'input, I>
where
    I: TokenStream<'input, TF = LocalTokenFactory<'input>> + TidAble<'input>,
{
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.base
    }
}

pub struct CypherParserExt<'input> {
    _pd: PhantomData<&'input str>,
}

impl<'input> CypherParserExt<'input> {}
antlr4rust::tid! { CypherParserExt<'a> }

impl<'input> TokenAware<'input> for CypherParserExt<'input> {
    type TF = LocalTokenFactory<'input>;
}

impl<'input, I: TokenStream<'input, TF = LocalTokenFactory<'input>> + TidAble<'input>>
    ParserRecog<'input, BaseParserType<'input, I>> for CypherParserExt<'input>
{
}

impl<'input, I: TokenStream<'input, TF = LocalTokenFactory<'input>> + TidAble<'input>>
    Actions<'input, BaseParserType<'input, I>> for CypherParserExt<'input>
{
    fn get_grammar_file_name(&self) -> &str {
        "Cypher.g4"
    }

    fn get_rule_names(&self) -> &[&str] {
        &ruleNames
    }

    fn get_vocabulary(&self) -> &dyn Vocabulary {
        &**VOCABULARY
    }
}
//------------------- oC_Cypher ----------------
pub type OC_CypherContextAll<'input> = OC_CypherContext<'input>;

pub type OC_CypherContext<'input> = BaseParserRuleContext<'input, OC_CypherContextExt<'input>>;

#[derive(Clone)]
pub struct OC_CypherContextExt<'input> {
    ph: PhantomData<&'input str>,
}

impl<'input> CypherParserContext<'input> for OC_CypherContext<'input> {}

impl<'input, 'a> Listenable<dyn CypherListener<'input> + 'a> for OC_CypherContext<'input> {
    fn enter(&self, listener: &mut (dyn CypherListener<'input> + 'a)) -> Result<(), ANTLRError> {
        listener.enter_every_rule(self)?;
        listener.enter_oC_Cypher(self);
        Ok(())
    }
    fn exit(&self, listener: &mut (dyn CypherListener<'input> + 'a)) -> Result<(), ANTLRError> {
        listener.exit_oC_Cypher(self);
        listener.exit_every_rule(self)?;
        Ok(())
    }
}

impl<'input, 'a> Visitable<dyn CypherVisitor<'input> + 'a> for OC_CypherContext<'input> {
    fn accept(&self, visitor: &mut (dyn CypherVisitor<'input> + 'a)) {
        visitor.visit_oC_Cypher(self);
    }
}

impl<'input> CustomRuleContext<'input> for OC_CypherContextExt<'input> {
    type TF = LocalTokenFactory<'input>;
    type Ctx = CypherParserContextType;
    fn get_rule_index(&self) -> usize {
        RULE_oC_Cypher
    }
    //fn type_rule_index() -> usize where Self: Sized { RULE_oC_Cypher }
}
antlr4rust::tid! {OC_CypherContextExt<'a>}

impl<'input> OC_CypherContextExt<'input> {
    fn new(
        parent: Option<Rc<dyn CypherParserContext<'input> + 'input>>,
        invoking_state: i32,
    ) -> Rc<OC_CypherContextAll<'input>> {
        Rc::new(BaseParserRuleContext::new_parser_ctx(
            parent,
            invoking_state,
            OC_CypherContextExt { ph: PhantomData },
        ))
    }
}

pub trait OC_CypherContextAttrs<'input>:
    CypherParserContext<'input> + BorrowMut<OC_CypherContextExt<'input>>
{
    fn oC_Statement(&self) -> Option<Rc<OC_StatementContextAll<'input>>>
    where
        Self: Sized,
    {
        self.child_of_type(0)
    }
    /// Retrieves first TerminalNode corresponding to token EOF
    /// Returns `None` if there is no child corresponding to token EOF
    fn EOF(&self) -> Option<Rc<TerminalNode<'input, CypherParserContextType>>>
    where
        Self: Sized,
    {
        self.get_token(Cypher_EOF, 0)
    }
    /// Retrieves all `TerminalNode`s corresponding to token SP in current rule
    fn SP_all(&self) -> Vec<Rc<TerminalNode<'input, CypherParserContextType>>>
    where
        Self: Sized,
    {
        self.children_of_type()
    }
    /// Retrieves 'i's TerminalNode corresponding to token SP, starting from 0.
    /// Returns `None` if number of children corresponding to token SP is less or equal than `i`.
    fn SP(&self, i: usize) -> Option<Rc<TerminalNode<'input, CypherParserContextType>>>
    where
        Self: Sized,
    {
        self.get_token(Cypher_SP, i)
    }
}

impl<'input> OC_CypherContextAttrs<'input> for OC_CypherContext<'input> {}

impl<'input, I> CypherParser<'input, I>
where
    I: TokenStream<'input, TF = LocalTokenFactory<'input>> + TidAble<'input>,
{
    pub fn oC_Cypher(&mut self) -> Result<Rc<OC_CypherContextAll<'input>>, ANTLRError> {
        let mut recog = self;
        let _parentctx = recog.ctx.take();
        let mut _localctx = OC_CypherContextExt::new(_parentctx.clone(), recog.base.get_state());
        recog.base.enter_rule(_localctx.clone(), 0, RULE_oC_Cypher);
        let mut _localctx: Rc<OC_CypherContextAll> = _localctx;
        let mut _la: i32 = -1;
        let result: Result<(), ANTLRError> = (|| {
            //recog.base.enter_outer_alt(_localctx.clone(), 1)?;
            recog.base.enter_outer_alt(None, 1)?;
            {
                recog.base.set_state(219);
                recog.err_handler.sync(&mut recog.base)?;
                _la = recog.base.input.la(1);
                if _la == Cypher_SP {
                    {
                        recog.base.set_state(218);
                        recog.base.match_token(Cypher_SP, &mut recog.err_handler)?;
                    }
                }

                /*InvokeRule oC_Statement*/
                recog.base.set_state(221);
                recog.oC_Statement()?;

                recog.base.set_state(226);
                recog.err_handler.sync(&mut recog.base)?;
                match recog.interpreter.adaptive_predict(2, &mut recog.base)? {
                    x if x == 1 => {
                        recog.base.set_state(223);
                        recog.err_handler.sync(&mut recog.base)?;
                        _la = recog.base.input.la(1);
                        if _la == Cypher_SP {
                            {
                                recog.base.set_state(222);
                                recog.base.match_token(Cypher_SP, &mut recog.err_handler)?;
                            }
                        }

                        recog.base.set_state(225);
                        recog
                            .base
                            .match_token(Cypher_T__0, &mut recog.err_handler)?;
                    }

                    _ => {}
                }
                recog.base.set_state(229);
                recog.err_handler.sync(&mut recog.base)?;
                _la = recog.base.input.la(1);
                if _la == Cypher_SP {
                    {
                        recog.base.set_state(228);
                        recog.base.match_token(Cypher_SP, &mut recog.err_handler)?;
                    }
                }

                recog.base.set_state(231);
                recog.base.match_token(Cypher_EOF, &mut recog.err_handler)?;
            }
            Ok(())
        })();
        match result {
            Ok(_) => {}
            Err(e @ ANTLRError::FallThrough(_)) => return Err(e),
            Err(ref re) => {
                //_localctx.exception = re;
                recog.err_handler.report_error(&mut recog.base, re);
                recog.err_handler.recover(&mut recog.base, re)?;
            }
        }
        recog.base.exit_rule()?;

        Ok(_localctx)
    }
}
//------------------- oC_Statement ----------------
pub type OC_StatementContextAll<'input> = OC_StatementContext<'input>;

pub type OC_StatementContext<'input> =
    BaseParserRuleContext<'input, OC_StatementContextExt<'input>>;

#[derive(Clone)]
pub struct OC_StatementContextExt<'input> {
    ph: PhantomData<&'input str>,
}

impl<'input> CypherParserContext<'input> for OC_StatementContext<'input> {}

impl<'input, 'a> Listenable<dyn CypherListener<'input> + 'a> for OC_StatementContext<'input> {
    fn enter(&self, listener: &mut (dyn CypherListener<'input> + 'a)) -> Result<(), ANTLRError> {
        listener.enter_every_rule(self)?;
        listener.enter_oC_Statement(self);
        Ok(())
    }
    fn exit(&self, listener: &mut (dyn CypherListener<'input> + 'a)) -> Result<(), ANTLRError> {
        listener.exit_oC_Statement(self);
        listener.exit_every_rule(self)?;
        Ok(())
    }
}

impl<'input, 'a> Visitable<dyn CypherVisitor<'input> + 'a> for OC_StatementContext<'input> {
    fn accept(&self, visitor: &mut (dyn CypherVisitor<'input> + 'a)) {
        visitor.visit_oC_Statement(self);
    }
}

impl<'input> CustomRuleContext<'input> for OC_StatementContextExt<'input> {
    type TF = LocalTokenFactory<'input>;
    type Ctx = CypherParserContextType;
    fn get_rule_index(&self) -> usize {
        RULE_oC_Statement
    }
    //fn type_rule_index() -> usize where Self: Sized { RULE_oC_Statement }
}
antlr4rust::tid! {OC_StatementContextExt<'a>}

impl<'input> OC_StatementContextExt<'input> {
    fn new(
        parent: Option<Rc<dyn CypherParserContext<'input> + 'input>>,
        invoking_state: i32,
    ) -> Rc<OC_StatementContextAll<'input>> {
        Rc::new(BaseParserRuleContext::new_parser_ctx(
            parent,
            invoking_state,
            OC_StatementContextExt { ph: PhantomData },
        ))
    }
}

pub trait OC_StatementContextAttrs<'input>:
    CypherParserContext<'input> + BorrowMut<OC_StatementContextExt<'input>>
{
    fn oC_Query(&self) -> Option<Rc<OC_QueryContextAll<'input>>>
    where
        Self: Sized,
    {
        self.child_of_type(0)
    }
}

impl<'input> OC_StatementContextAttrs<'input> for OC_StatementContext<'input> {}

impl<'input, I> CypherParser<'input, I>
where
    I: TokenStream<'input, TF = LocalTokenFactory<'input>> + TidAble<'input>,
{
    pub fn oC_Statement(&mut self) -> Result<Rc<OC_StatementContextAll<'input>>, ANTLRError> {
        let mut recog = self;
        let _parentctx = recog.ctx.take();
        let mut _localctx = OC_StatementContextExt::new(_parentctx.clone(), recog.base.get_state());
        recog
            .base
            .enter_rule(_localctx.clone(), 2, RULE_oC_Statement);
        let mut _localctx: Rc<OC_StatementContextAll> = _localctx;
        let result: Result<(), ANTLRError> = (|| {
            //recog.base.enter_outer_alt(_localctx.clone(), 1)?;
            recog.base.enter_outer_alt(None, 1)?;
            {
                /*InvokeRule oC_Query*/
                recog.base.set_state(233);
                recog.oC_Query()?;
            }
            Ok(())
        })();
        match result {
            Ok(_) => {}
            Err(e @ ANTLRError::FallThrough(_)) => return Err(e),
            Err(ref re) => {
                //_localctx.exception = re;
                recog.err_handler.report_error(&mut recog.base, re);
                recog.err_handler.recover(&mut recog.base, re)?;
            }
        }
        recog.base.exit_rule()?;

        Ok(_localctx)
    }
}
//------------------- oC_Query ----------------
pub type OC_QueryContextAll<'input> = OC_QueryContext<'input>;

pub type OC_QueryContext<'input> = BaseParserRuleContext<'input, OC_QueryContextExt<'input>>;

#[derive(Clone)]
pub struct OC_QueryContextExt<'input> {
    ph: PhantomData<&'input str>,
}

impl<'input> CypherParserContext<'input> for OC_QueryContext<'input> {}

impl<'input, 'a> Listenable<dyn CypherListener<'input> + 'a> for OC_QueryContext<'input> {
    fn enter(&self, listener: &mut (dyn CypherListener<'input> + 'a)) -> Result<(), ANTLRError> {
        listener.enter_every_rule(self)?;
        listener.enter_oC_Query(self);
        Ok(())
    }
    fn exit(&self, listener: &mut (dyn CypherListener<'input> + 'a)) -> Result<(), ANTLRError> {
        listener.exit_oC_Query(self);
        listener.exit_every_rule(self)?;
        Ok(())
    }
}

impl<'input, 'a> Visitable<dyn CypherVisitor<'input> + 'a> for OC_QueryContext<'input> {
    fn accept(&self, visitor: &mut (dyn CypherVisitor<'input> + 'a)) {
        visitor.visit_oC_Query(self);
    }
}

impl<'input> CustomRuleContext<'input> for OC_QueryContextExt<'input> {
    type TF = LocalTokenFactory<'input>;
    type Ctx = CypherParserContextType;
    fn get_rule_index(&self) -> usize {
        RULE_oC_Query
    }
    //fn type_rule_index() -> usize where Self: Sized { RULE_oC_Query }
}
antlr4rust::tid! {OC_QueryContextExt<'a>}

impl<'input> OC_QueryContextExt<'input> {
    fn new(
        parent: Option<Rc<dyn CypherParserContext<'input> + 'input>>,
        invoking_state: i32,
    ) -> Rc<OC_QueryContextAll<'input>> {
        Rc::new(BaseParserRuleContext::new_parser_ctx(
            parent,
            invoking_state,
            OC_QueryContextExt { ph: PhantomData },
        ))
    }
}

pub trait OC_QueryContextAttrs<'input>:
    CypherParserContext<'input> + BorrowMut<OC_QueryContextExt<'input>>
{
    fn oC_RegularQuery(&self) -> Option<Rc<OC_RegularQueryContextAll<'input>>>
    where
        Self: Sized,
    {
        self.child_of_type(0)
    }
    fn oC_StandaloneCall(&self) -> Option<Rc<OC_StandaloneCallContextAll<'input>>>
    where
        Self: Sized,
    {
        self.child_of_type(0)
    }
}

impl<'input> OC_QueryContextAttrs<'input> for OC_QueryContext<'input> {}

impl<'input, I> CypherParser<'input, I>
where
    I: TokenStream<'input, TF = LocalTokenFactory<'input>> + TidAble<'input>,
{
    pub fn oC_Query(&mut self) -> Result<Rc<OC_QueryContextAll<'input>>, ANTLRError> {
        let mut recog = self;
        let _parentctx = recog.ctx.take();
        let mut _localctx = OC_QueryContextExt::new(_parentctx.clone(), recog.base.get_state());
        recog.base.enter_rule(_localctx.clone(), 4, RULE_oC_Query);
        let mut _localctx: Rc<OC_QueryContextAll> = _localctx;
        let result: Result<(), ANTLRError> = (|| {
            recog.base.set_state(237);
            recog.err_handler.sync(&mut recog.base)?;
            match recog.interpreter.adaptive_predict(4, &mut recog.base)? {
                1 => {
                    //recog.base.enter_outer_alt(_localctx.clone(), 1)?;
                    recog.base.enter_outer_alt(None, 1)?;
                    {
                        /*InvokeRule oC_RegularQuery*/
                        recog.base.set_state(235);
                        recog.oC_RegularQuery()?;
                    }
                }
                2 => {
                    //recog.base.enter_outer_alt(_localctx.clone(), 2)?;
                    recog.base.enter_outer_alt(None, 2)?;
                    {
                        /*InvokeRule oC_StandaloneCall*/
                        recog.base.set_state(236);
                        recog.oC_StandaloneCall()?;
                    }
                }

                _ => {}
            }
            Ok(())
        })();
        match result {
            Ok(_) => {}
            Err(e @ ANTLRError::FallThrough(_)) => return Err(e),
            Err(ref re) => {
                //_localctx.exception = re;
                recog.err_handler.report_error(&mut recog.base, re);
                recog.err_handler.recover(&mut recog.base, re)?;
            }
        }
        recog.base.exit_rule()?;

        Ok(_localctx)
    }
}
//------------------- oC_RegularQuery ----------------
pub type OC_RegularQueryContextAll<'input> = OC_RegularQueryContext<'input>;

pub type OC_RegularQueryContext<'input> =
    BaseParserRuleContext<'input, OC_RegularQueryContextExt<'input>>;

#[derive(Clone)]
pub struct OC_RegularQueryContextExt<'input> {
    ph: PhantomData<&'input str>,
}

impl<'input> CypherParserContext<'input> for OC_RegularQueryContext<'input> {}

impl<'input, 'a> Listenable<dyn CypherListener<'input> + 'a> for OC_RegularQueryContext<'input> {
    fn enter(&self, listener: &mut (dyn CypherListener<'input> + 'a)) -> Result<(), ANTLRError> {
        listener.enter_every_rule(self)?;
        listener.enter_oC_RegularQuery(self);
        Ok(())
    }
    fn exit(&self, listener: &mut (dyn CypherListener<'input> + 'a)) -> Result<(), ANTLRError> {
        listener.exit_oC_RegularQuery(self);
        listener.exit_every_rule(self)?;
        Ok(())
    }
}

impl<'input, 'a> Visitable<dyn CypherVisitor<'input> + 'a> for OC_RegularQueryContext<'input> {
    fn accept(&self, visitor: &mut (dyn CypherVisitor<'input> + 'a)) {
        visitor.visit_oC_RegularQuery(self);
    }
}

impl<'input> CustomRuleContext<'input> for OC_RegularQueryContextExt<'input> {
    type TF = LocalTokenFactory<'input>;
    type Ctx = CypherParserContextType;
    fn get_rule_index(&self) -> usize {
        RULE_oC_RegularQuery
    }
    //fn type_rule_index() -> usize where Self: Sized { RULE_oC_RegularQuery }
}
antlr4rust::tid! {OC_RegularQueryContextExt<'a>}

impl<'input> OC_RegularQueryContextExt<'input> {
    fn new(
        parent: Option<Rc<dyn CypherParserContext<'input> + 'input>>,
        invoking_state: i32,
    ) -> Rc<OC_RegularQueryContextAll<'input>> {
        Rc::new(BaseParserRuleContext::new_parser_ctx(
            parent,
            invoking_state,
            OC_RegularQueryContextExt { ph: PhantomData },
        ))
    }
}

pub trait OC_RegularQueryContextAttrs<'input>:
    CypherParserContext<'input> + BorrowMut<OC_RegularQueryContextExt<'input>>
{
    fn oC_SingleQuery(&self) -> Option<Rc<OC_SingleQueryContextAll<'input>>>
    where
        Self: Sized,
    {
        self.child_of_type(0)
    }
    fn oC_Union_all(&self) -> Vec<Rc<OC_UnionContextAll<'input>>>
    where
        Self: Sized,
    {
        self.children_of_type()
    }
    fn oC_Union(&self, i: usize) -> Option<Rc<OC_UnionContextAll<'input>>>
    where
        Self: Sized,
    {
        self.child_of_type(i)
    }
    /// Retrieves all `TerminalNode`s corresponding to token SP in current rule
    fn SP_all(&self) -> Vec<Rc<TerminalNode<'input, CypherParserContextType>>>
    where
        Self: Sized,
    {
        self.children_of_type()
    }
    /// Retrieves 'i's TerminalNode corresponding to token SP, starting from 0.
    /// Returns `None` if number of children corresponding to token SP is less or equal than `i`.
    fn SP(&self, i: usize) -> Option<Rc<TerminalNode<'input, CypherParserContextType>>>
    where
        Self: Sized,
    {
        self.get_token(Cypher_SP, i)
    }
}

impl<'input> OC_RegularQueryContextAttrs<'input> for OC_RegularQueryContext<'input> {}

impl<'input, I> CypherParser<'input, I>
where
    I: TokenStream<'input, TF = LocalTokenFactory<'input>> + TidAble<'input>,
{
    pub fn oC_RegularQuery(&mut self) -> Result<Rc<OC_RegularQueryContextAll<'input>>, ANTLRError> {
        let mut recog = self;
        let _parentctx = recog.ctx.take();
        let mut _localctx =
            OC_RegularQueryContextExt::new(_parentctx.clone(), recog.base.get_state());
        recog
            .base
            .enter_rule(_localctx.clone(), 6, RULE_oC_RegularQuery);
        let mut _localctx: Rc<OC_RegularQueryContextAll> = _localctx;
        let mut _la: i32 = -1;
        let result: Result<(), ANTLRError> = (|| {
            let mut _alt: i32;
            //recog.base.enter_outer_alt(_localctx.clone(), 1)?;
            recog.base.enter_outer_alt(None, 1)?;
            {
                /*InvokeRule oC_SingleQuery*/
                recog.base.set_state(239);
                recog.oC_SingleQuery()?;

                recog.base.set_state(246);
                recog.err_handler.sync(&mut recog.base)?;
                _alt = recog.interpreter.adaptive_predict(6, &mut recog.base)?;
                while { _alt != 2 && _alt != INVALID_ALT } {
                    if _alt == 1 {
                        {
                            {
                                recog.base.set_state(241);
                                recog.err_handler.sync(&mut recog.base)?;
                                _la = recog.base.input.la(1);
                                if _la == Cypher_SP {
                                    {
                                        recog.base.set_state(240);
                                        recog
                                            .base
                                            .match_token(Cypher_SP, &mut recog.err_handler)?;
                                    }
                                }

                                /*InvokeRule oC_Union*/
                                recog.base.set_state(243);
                                recog.oC_Union()?;
                            }
                        }
                    }
                    recog.base.set_state(248);
                    recog.err_handler.sync(&mut recog.base)?;
                    _alt = recog.interpreter.adaptive_predict(6, &mut recog.base)?;
                }
            }
            Ok(())
        })();
        match result {
            Ok(_) => {}
            Err(e @ ANTLRError::FallThrough(_)) => return Err(e),
            Err(ref re) => {
                //_localctx.exception = re;
                recog.err_handler.report_error(&mut recog.base, re);
                recog.err_handler.recover(&mut recog.base, re)?;
            }
        }
        recog.base.exit_rule()?;

        Ok(_localctx)
    }
}
//------------------- oC_Union ----------------
pub type OC_UnionContextAll<'input> = OC_UnionContext<'input>;

pub type OC_UnionContext<'input> = BaseParserRuleContext<'input, OC_UnionContextExt<'input>>;

#[derive(Clone)]
pub struct OC_UnionContextExt<'input> {
    ph: PhantomData<&'input str>,
}

impl<'input> CypherParserContext<'input> for OC_UnionContext<'input> {}

impl<'input, 'a> Listenable<dyn CypherListener<'input> + 'a> for OC_UnionContext<'input> {
    fn enter(&self, listener: &mut (dyn CypherListener<'input> + 'a)) -> Result<(), ANTLRError> {
        listener.enter_every_rule(self)?;
        listener.enter_oC_Union(self);
        Ok(())
    }
    fn exit(&self, listener: &mut (dyn CypherListener<'input> + 'a)) -> Result<(), ANTLRError> {
        listener.exit_oC_Union(self);
        listener.exit_every_rule(self)?;
        Ok(())
    }
}

impl<'input, 'a> Visitable<dyn CypherVisitor<'input> + 'a> for OC_UnionContext<'input> {
    fn accept(&self, visitor: &mut (dyn CypherVisitor<'input> + 'a)) {
        visitor.visit_oC_Union(self);
    }
}

impl<'input> CustomRuleContext<'input> for OC_UnionContextExt<'input> {
    type TF = LocalTokenFactory<'input>;
    type Ctx = CypherParserContextType;
    fn get_rule_index(&self) -> usize {
        RULE_oC_Union
    }
    //fn type_rule_index() -> usize where Self: Sized { RULE_oC_Union }
}
antlr4rust::tid! {OC_UnionContextExt<'a>}

impl<'input> OC_UnionContextExt<'input> {
    fn new(
        parent: Option<Rc<dyn CypherParserContext<'input> + 'input>>,
        invoking_state: i32,
    ) -> Rc<OC_UnionContextAll<'input>> {
        Rc::new(BaseParserRuleContext::new_parser_ctx(
            parent,
            invoking_state,
            OC_UnionContextExt { ph: PhantomData },
        ))
    }
}

pub trait OC_UnionContextAttrs<'input>:
    CypherParserContext<'input> + BorrowMut<OC_UnionContextExt<'input>>
{
    /// Retrieves first TerminalNode corresponding to token UNION
    /// Returns `None` if there is no child corresponding to token UNION
    fn UNION(&self) -> Option<Rc<TerminalNode<'input, CypherParserContextType>>>
    where
        Self: Sized,
    {
        self.get_token(Cypher_UNION, 0)
    }
    /// Retrieves all `TerminalNode`s corresponding to token SP in current rule
    fn SP_all(&self) -> Vec<Rc<TerminalNode<'input, CypherParserContextType>>>
    where
        Self: Sized,
    {
        self.children_of_type()
    }
    /// Retrieves 'i's TerminalNode corresponding to token SP, starting from 0.
    /// Returns `None` if number of children corresponding to token SP is less or equal than `i`.
    fn SP(&self, i: usize) -> Option<Rc<TerminalNode<'input, CypherParserContextType>>>
    where
        Self: Sized,
    {
        self.get_token(Cypher_SP, i)
    }
    /// Retrieves first TerminalNode corresponding to token ALL
    /// Returns `None` if there is no child corresponding to token ALL
    fn ALL(&self) -> Option<Rc<TerminalNode<'input, CypherParserContextType>>>
    where
        Self: Sized,
    {
        self.get_token(Cypher_ALL, 0)
    }
    fn oC_SingleQuery(&self) -> Option<Rc<OC_SingleQueryContextAll<'input>>>
    where
        Self: Sized,
    {
        self.child_of_type(0)
    }
}

impl<'input> OC_UnionContextAttrs<'input> for OC_UnionContext<'input> {}

impl<'input, I> CypherParser<'input, I>
where
    I: TokenStream<'input, TF = LocalTokenFactory<'input>> + TidAble<'input>,
{
    pub fn oC_Union(&mut self) -> Result<Rc<OC_UnionContextAll<'input>>, ANTLRError> {
        let mut recog = self;
        let _parentctx = recog.ctx.take();
        let mut _localctx = OC_UnionContextExt::new(_parentctx.clone(), recog.base.get_state());
        recog.base.enter_rule(_localctx.clone(), 8, RULE_oC_Union);
        let mut _localctx: Rc<OC_UnionContextAll> = _localctx;
        let mut _la: i32 = -1;
        let result: Result<(), ANTLRError> = (|| {
            recog.base.set_state(261);
            recog.err_handler.sync(&mut recog.base)?;
            match recog.interpreter.adaptive_predict(9, &mut recog.base)? {
                1 => {
                    //recog.base.enter_outer_alt(_localctx.clone(), 1)?;
                    recog.base.enter_outer_alt(None, 1)?;
                    {
                        {
                            recog.base.set_state(249);
                            recog
                                .base
                                .match_token(Cypher_UNION, &mut recog.err_handler)?;

                            recog.base.set_state(250);
                            recog.base.match_token(Cypher_SP, &mut recog.err_handler)?;

                            recog.base.set_state(251);
                            recog.base.match_token(Cypher_ALL, &mut recog.err_handler)?;

                            recog.base.set_state(253);
                            recog.err_handler.sync(&mut recog.base)?;
                            _la = recog.base.input.la(1);
                            if _la == Cypher_SP {
                                {
                                    recog.base.set_state(252);
                                    recog.base.match_token(Cypher_SP, &mut recog.err_handler)?;
                                }
                            }

                            /*InvokeRule oC_SingleQuery*/
                            recog.base.set_state(255);
                            recog.oC_SingleQuery()?;
                        }
                    }
                }
                2 => {
                    //recog.base.enter_outer_alt(_localctx.clone(), 2)?;
                    recog.base.enter_outer_alt(None, 2)?;
                    {
                        {
                            recog.base.set_state(256);
                            recog
                                .base
                                .match_token(Cypher_UNION, &mut recog.err_handler)?;

                            recog.base.set_state(258);
                            recog.err_handler.sync(&mut recog.base)?;
                            _la = recog.base.input.la(1);
                            if _la == Cypher_SP {
                                {
                                    recog.base.set_state(257);
                                    recog.base.match_token(Cypher_SP, &mut recog.err_handler)?;
                                }
                            }

                            /*InvokeRule oC_SingleQuery*/
                            recog.base.set_state(260);
                            recog.oC_SingleQuery()?;
                        }
                    }
                }

                _ => {}
            }
            Ok(())
        })();
        match result {
            Ok(_) => {}
            Err(e @ ANTLRError::FallThrough(_)) => return Err(e),
            Err(ref re) => {
                //_localctx.exception = re;
                recog.err_handler.report_error(&mut recog.base, re);
                recog.err_handler.recover(&mut recog.base, re)?;
            }
        }
        recog.base.exit_rule()?;

        Ok(_localctx)
    }
}
//------------------- oC_SingleQuery ----------------
pub type OC_SingleQueryContextAll<'input> = OC_SingleQueryContext<'input>;

pub type OC_SingleQueryContext<'input> =
    BaseParserRuleContext<'input, OC_SingleQueryContextExt<'input>>;

#[derive(Clone)]
pub struct OC_SingleQueryContextExt<'input> {
    ph: PhantomData<&'input str>,
}

impl<'input> CypherParserContext<'input> for OC_SingleQueryContext<'input> {}

impl<'input, 'a> Listenable<dyn CypherListener<'input> + 'a> for OC_SingleQueryContext<'input> {
    fn enter(&self, listener: &mut (dyn CypherListener<'input> + 'a)) -> Result<(), ANTLRError> {
        listener.enter_every_rule(self)?;
        listener.enter_oC_SingleQuery(self);
        Ok(())
    }
    fn exit(&self, listener: &mut (dyn CypherListener<'input> + 'a)) -> Result<(), ANTLRError> {
        listener.exit_oC_SingleQuery(self);
        listener.exit_every_rule(self)?;
        Ok(())
    }
}

impl<'input, 'a> Visitable<dyn CypherVisitor<'input> + 'a> for OC_SingleQueryContext<'input> {
    fn accept(&self, visitor: &mut (dyn CypherVisitor<'input> + 'a)) {
        visitor.visit_oC_SingleQuery(self);
    }
}

impl<'input> CustomRuleContext<'input> for OC_SingleQueryContextExt<'input> {
    type TF = LocalTokenFactory<'input>;
    type Ctx = CypherParserContextType;
    fn get_rule_index(&self) -> usize {
        RULE_oC_SingleQuery
    }
    //fn type_rule_index() -> usize where Self: Sized { RULE_oC_SingleQuery }
}
antlr4rust::tid! {OC_SingleQueryContextExt<'a>}

impl<'input> OC_SingleQueryContextExt<'input> {
    fn new(
        parent: Option<Rc<dyn CypherParserContext<'input> + 'input>>,
        invoking_state: i32,
    ) -> Rc<OC_SingleQueryContextAll<'input>> {
        Rc::new(BaseParserRuleContext::new_parser_ctx(
            parent,
            invoking_state,
            OC_SingleQueryContextExt { ph: PhantomData },
        ))
    }
}

pub trait OC_SingleQueryContextAttrs<'input>:
    CypherParserContext<'input> + BorrowMut<OC_SingleQueryContextExt<'input>>
{
    fn oC_SinglePartQuery(&self) -> Option<Rc<OC_SinglePartQueryContextAll<'input>>>
    where
        Self: Sized,
    {
        self.child_of_type(0)
    }
    fn oC_MultiPartQuery(&self) -> Option<Rc<OC_MultiPartQueryContextAll<'input>>>
    where
        Self: Sized,
    {
        self.child_of_type(0)
    }
}

impl<'input> OC_SingleQueryContextAttrs<'input> for OC_SingleQueryContext<'input> {}

impl<'input, I> CypherParser<'input, I>
where
    I: TokenStream<'input, TF = LocalTokenFactory<'input>> + TidAble<'input>,
{
    pub fn oC_SingleQuery(&mut self) -> Result<Rc<OC_SingleQueryContextAll<'input>>, ANTLRError> {
        let mut recog = self;
        let _parentctx = recog.ctx.take();
        let mut _localctx =
            OC_SingleQueryContextExt::new(_parentctx.clone(), recog.base.get_state());
        recog
            .base
            .enter_rule(_localctx.clone(), 10, RULE_oC_SingleQuery);
        let mut _localctx: Rc<OC_SingleQueryContextAll> = _localctx;
        let result: Result<(), ANTLRError> = (|| {
            recog.base.set_state(265);
            recog.err_handler.sync(&mut recog.base)?;
            match recog.interpreter.adaptive_predict(10, &mut recog.base)? {
                1 => {
                    //recog.base.enter_outer_alt(_localctx.clone(), 1)?;
                    recog.base.enter_outer_alt(None, 1)?;
                    {
                        /*InvokeRule oC_SinglePartQuery*/
                        recog.base.set_state(263);
                        recog.oC_SinglePartQuery()?;
                    }
                }
                2 => {
                    //recog.base.enter_outer_alt(_localctx.clone(), 2)?;
                    recog.base.enter_outer_alt(None, 2)?;
                    {
                        /*InvokeRule oC_MultiPartQuery*/
                        recog.base.set_state(264);
                        recog.oC_MultiPartQuery()?;
                    }
                }

                _ => {}
            }
            Ok(())
        })();
        match result {
            Ok(_) => {}
            Err(e @ ANTLRError::FallThrough(_)) => return Err(e),
            Err(ref re) => {
                //_localctx.exception = re;
                recog.err_handler.report_error(&mut recog.base, re);
                recog.err_handler.recover(&mut recog.base, re)?;
            }
        }
        recog.base.exit_rule()?;

        Ok(_localctx)
    }
}
//------------------- oC_SinglePartQuery ----------------
pub type OC_SinglePartQueryContextAll<'input> = OC_SinglePartQueryContext<'input>;

pub type OC_SinglePartQueryContext<'input> =
    BaseParserRuleContext<'input, OC_SinglePartQueryContextExt<'input>>;

#[derive(Clone)]
pub struct OC_SinglePartQueryContextExt<'input> {
    ph: PhantomData<&'input str>,
}

impl<'input> CypherParserContext<'input> for OC_SinglePartQueryContext<'input> {}

impl<'input, 'a> Listenable<dyn CypherListener<'input> + 'a> for OC_SinglePartQueryContext<'input> {
    fn enter(&self, listener: &mut (dyn CypherListener<'input> + 'a)) -> Result<(), ANTLRError> {
        listener.enter_every_rule(self)?;
        listener.enter_oC_SinglePartQuery(self);
        Ok(())
    }
    fn exit(&self, listener: &mut (dyn CypherListener<'input> + 'a)) -> Result<(), ANTLRError> {
        listener.exit_oC_SinglePartQuery(self);
        listener.exit_every_rule(self)?;
        Ok(())
    }
}

impl<'input, 'a> Visitable<dyn CypherVisitor<'input> + 'a> for OC_SinglePartQueryContext<'input> {
    fn accept(&self, visitor: &mut (dyn CypherVisitor<'input> + 'a)) {
        visitor.visit_oC_SinglePartQuery(self);
    }
}

impl<'input> CustomRuleContext<'input> for OC_SinglePartQueryContextExt<'input> {
    type TF = LocalTokenFactory<'input>;
    type Ctx = CypherParserContextType;
    fn get_rule_index(&self) -> usize {
        RULE_oC_SinglePartQuery
    }
    //fn type_rule_index() -> usize where Self: Sized { RULE_oC_SinglePartQuery }
}
antlr4rust::tid! {OC_SinglePartQueryContextExt<'a>}

impl<'input> OC_SinglePartQueryContextExt<'input> {
    fn new(
        parent: Option<Rc<dyn CypherParserContext<'input> + 'input>>,
        invoking_state: i32,
    ) -> Rc<OC_SinglePartQueryContextAll<'input>> {
        Rc::new(BaseParserRuleContext::new_parser_ctx(
            parent,
            invoking_state,
            OC_SinglePartQueryContextExt { ph: PhantomData },
        ))
    }
}

pub trait OC_SinglePartQueryContextAttrs<'input>:
    CypherParserContext<'input> + BorrowMut<OC_SinglePartQueryContextExt<'input>>
{
    fn oC_Return(&self) -> Option<Rc<OC_ReturnContextAll<'input>>>
    where
        Self: Sized,
    {
        self.child_of_type(0)
    }
    fn oC_ReadingClause_all(&self) -> Vec<Rc<OC_ReadingClauseContextAll<'input>>>
    where
        Self: Sized,
    {
        self.children_of_type()
    }
    fn oC_ReadingClause(&self, i: usize) -> Option<Rc<OC_ReadingClauseContextAll<'input>>>
    where
        Self: Sized,
    {
        self.child_of_type(i)
    }
    /// Retrieves all `TerminalNode`s corresponding to token SP in current rule
    fn SP_all(&self) -> Vec<Rc<TerminalNode<'input, CypherParserContextType>>>
    where
        Self: Sized,
    {
        self.children_of_type()
    }
    /// Retrieves 'i's TerminalNode corresponding to token SP, starting from 0.
    /// Returns `None` if number of children corresponding to token SP is less or equal than `i`.
    fn SP(&self, i: usize) -> Option<Rc<TerminalNode<'input, CypherParserContextType>>>
    where
        Self: Sized,
    {
        self.get_token(Cypher_SP, i)
    }
    fn oC_UpdatingClause_all(&self) -> Vec<Rc<OC_UpdatingClauseContextAll<'input>>>
    where
        Self: Sized,
    {
        self.children_of_type()
    }
    fn oC_UpdatingClause(&self, i: usize) -> Option<Rc<OC_UpdatingClauseContextAll<'input>>>
    where
        Self: Sized,
    {
        self.child_of_type(i)
    }
}

impl<'input> OC_SinglePartQueryContextAttrs<'input> for OC_SinglePartQueryContext<'input> {}

impl<'input, I> CypherParser<'input, I>
where
    I: TokenStream<'input, TF = LocalTokenFactory<'input>> + TidAble<'input>,
{
    pub fn oC_SinglePartQuery(
        &mut self,
    ) -> Result<Rc<OC_SinglePartQueryContextAll<'input>>, ANTLRError> {
        let mut recog = self;
        let _parentctx = recog.ctx.take();
        let mut _localctx =
            OC_SinglePartQueryContextExt::new(_parentctx.clone(), recog.base.get_state());
        recog
            .base
            .enter_rule(_localctx.clone(), 12, RULE_oC_SinglePartQuery);
        let mut _localctx: Rc<OC_SinglePartQueryContextAll> = _localctx;
        let mut _la: i32 = -1;
        let result: Result<(), ANTLRError> = (|| {
            let mut _alt: i32;
            recog.base.set_state(302);
            recog.err_handler.sync(&mut recog.base)?;
            match recog.interpreter.adaptive_predict(19, &mut recog.base)? {
                1 => {
                    //recog.base.enter_outer_alt(_localctx.clone(), 1)?;
                    recog.base.enter_outer_alt(None, 1)?;
                    {
                        {
                            recog.base.set_state(273);
                            recog.err_handler.sync(&mut recog.base)?;
                            _la = recog.base.input.la(1);
                            while (((_la - 48) & !0x3f) == 0
                                && ((1usize << (_la - 48)) & 2055) != 0)
                            {
                                {
                                    {
                                        /*InvokeRule oC_ReadingClause*/
                                        recog.base.set_state(267);
                                        recog.oC_ReadingClause()?;

                                        recog.base.set_state(269);
                                        recog.err_handler.sync(&mut recog.base)?;
                                        _la = recog.base.input.la(1);
                                        if _la == Cypher_SP {
                                            {
                                                recog.base.set_state(268);
                                                recog.base.match_token(
                                                    Cypher_SP,
                                                    &mut recog.err_handler,
                                                )?;
                                            }
                                        }
                                    }
                                }
                                recog.base.set_state(275);
                                recog.err_handler.sync(&mut recog.base)?;
                                _la = recog.base.input.la(1);
                            }
                            /*InvokeRule oC_Return*/
                            recog.base.set_state(276);
                            recog.oC_Return()?;
                        }
                    }
                }
                2 => {
                    //recog.base.enter_outer_alt(_localctx.clone(), 2)?;
                    recog.base.enter_outer_alt(None, 2)?;
                    {
                        {
                            recog.base.set_state(283);
                            recog.err_handler.sync(&mut recog.base)?;
                            _la = recog.base.input.la(1);
                            while (((_la - 48) & !0x3f) == 0
                                && ((1usize << (_la - 48)) & 2055) != 0)
                            {
                                {
                                    {
                                        /*InvokeRule oC_ReadingClause*/
                                        recog.base.set_state(277);
                                        recog.oC_ReadingClause()?;

                                        recog.base.set_state(279);
                                        recog.err_handler.sync(&mut recog.base)?;
                                        _la = recog.base.input.la(1);
                                        if _la == Cypher_SP {
                                            {
                                                recog.base.set_state(278);
                                                recog.base.match_token(
                                                    Cypher_SP,
                                                    &mut recog.err_handler,
                                                )?;
                                            }
                                        }
                                    }
                                }
                                recog.base.set_state(285);
                                recog.err_handler.sync(&mut recog.base)?;
                                _la = recog.base.input.la(1);
                            }
                            /*InvokeRule oC_UpdatingClause*/
                            recog.base.set_state(286);
                            recog.oC_UpdatingClause()?;

                            recog.base.set_state(293);
                            recog.err_handler.sync(&mut recog.base)?;
                            _alt = recog.interpreter.adaptive_predict(16, &mut recog.base)?;
                            while { _alt != 2 && _alt != INVALID_ALT } {
                                if _alt == 1 {
                                    {
                                        {
                                            recog.base.set_state(288);
                                            recog.err_handler.sync(&mut recog.base)?;
                                            _la = recog.base.input.la(1);
                                            if _la == Cypher_SP {
                                                {
                                                    recog.base.set_state(287);
                                                    recog.base.match_token(
                                                        Cypher_SP,
                                                        &mut recog.err_handler,
                                                    )?;
                                                }
                                            }

                                            /*InvokeRule oC_UpdatingClause*/
                                            recog.base.set_state(290);
                                            recog.oC_UpdatingClause()?;
                                        }
                                    }
                                }
                                recog.base.set_state(295);
                                recog.err_handler.sync(&mut recog.base)?;
                                _alt = recog.interpreter.adaptive_predict(16, &mut recog.base)?;
                            }
                            recog.base.set_state(300);
                            recog.err_handler.sync(&mut recog.base)?;
                            match recog.interpreter.adaptive_predict(18, &mut recog.base)? {
                                x if x == 1 => {
                                    {
                                        recog.base.set_state(297);
                                        recog.err_handler.sync(&mut recog.base)?;
                                        _la = recog.base.input.la(1);
                                        if _la == Cypher_SP {
                                            {
                                                recog.base.set_state(296);
                                                recog.base.match_token(
                                                    Cypher_SP,
                                                    &mut recog.err_handler,
                                                )?;
                                            }
                                        }

                                        /*InvokeRule oC_Return*/
                                        recog.base.set_state(299);
                                        recog.oC_Return()?;
                                    }
                                }

                                _ => {}
                            }
                        }
                    }
                }

                _ => {}
            }
            Ok(())
        })();
        match result {
            Ok(_) => {}
            Err(e @ ANTLRError::FallThrough(_)) => return Err(e),
            Err(ref re) => {
                //_localctx.exception = re;
                recog.err_handler.report_error(&mut recog.base, re);
                recog.err_handler.recover(&mut recog.base, re)?;
            }
        }
        recog.base.exit_rule()?;

        Ok(_localctx)
    }
}
//------------------- oC_MultiPartQuery ----------------
pub type OC_MultiPartQueryContextAll<'input> = OC_MultiPartQueryContext<'input>;

pub type OC_MultiPartQueryContext<'input> =
    BaseParserRuleContext<'input, OC_MultiPartQueryContextExt<'input>>;

#[derive(Clone)]
pub struct OC_MultiPartQueryContextExt<'input> {
    ph: PhantomData<&'input str>,
}

impl<'input> CypherParserContext<'input> for OC_MultiPartQueryContext<'input> {}

impl<'input, 'a> Listenable<dyn CypherListener<'input> + 'a> for OC_MultiPartQueryContext<'input> {
    fn enter(&self, listener: &mut (dyn CypherListener<'input> + 'a)) -> Result<(), ANTLRError> {
        listener.enter_every_rule(self)?;
        listener.enter_oC_MultiPartQuery(self);
        Ok(())
    }
    fn exit(&self, listener: &mut (dyn CypherListener<'input> + 'a)) -> Result<(), ANTLRError> {
        listener.exit_oC_MultiPartQuery(self);
        listener.exit_every_rule(self)?;
        Ok(())
    }
}

impl<'input, 'a> Visitable<dyn CypherVisitor<'input> + 'a> for OC_MultiPartQueryContext<'input> {
    fn accept(&self, visitor: &mut (dyn CypherVisitor<'input> + 'a)) {
        visitor.visit_oC_MultiPartQuery(self);
    }
}

impl<'input> CustomRuleContext<'input> for OC_MultiPartQueryContextExt<'input> {
    type TF = LocalTokenFactory<'input>;
    type Ctx = CypherParserContextType;
    fn get_rule_index(&self) -> usize {
        RULE_oC_MultiPartQuery
    }
    //fn type_rule_index() -> usize where Self: Sized { RULE_oC_MultiPartQuery }
}
antlr4rust::tid! {OC_MultiPartQueryContextExt<'a>}

impl<'input> OC_MultiPartQueryContextExt<'input> {
    fn new(
        parent: Option<Rc<dyn CypherParserContext<'input> + 'input>>,
        invoking_state: i32,
    ) -> Rc<OC_MultiPartQueryContextAll<'input>> {
        Rc::new(BaseParserRuleContext::new_parser_ctx(
            parent,
            invoking_state,
            OC_MultiPartQueryContextExt { ph: PhantomData },
        ))
    }
}

pub trait OC_MultiPartQueryContextAttrs<'input>:
    CypherParserContext<'input> + BorrowMut<OC_MultiPartQueryContextExt<'input>>
{
    fn oC_SinglePartQuery(&self) -> Option<Rc<OC_SinglePartQueryContextAll<'input>>>
    where
        Self: Sized,
    {
        self.child_of_type(0)
    }
    fn oC_With_all(&self) -> Vec<Rc<OC_WithContextAll<'input>>>
    where
        Self: Sized,
    {
        self.children_of_type()
    }
    fn oC_With(&self, i: usize) -> Option<Rc<OC_WithContextAll<'input>>>
    where
        Self: Sized,
    {
        self.child_of_type(i)
    }
    fn oC_ReadingClause_all(&self) -> Vec<Rc<OC_ReadingClauseContextAll<'input>>>
    where
        Self: Sized,
    {
        self.children_of_type()
    }
    fn oC_ReadingClause(&self, i: usize) -> Option<Rc<OC_ReadingClauseContextAll<'input>>>
    where
        Self: Sized,
    {
        self.child_of_type(i)
    }
    fn oC_UpdatingClause_all(&self) -> Vec<Rc<OC_UpdatingClauseContextAll<'input>>>
    where
        Self: Sized,
    {
        self.children_of_type()
    }
    fn oC_UpdatingClause(&self, i: usize) -> Option<Rc<OC_UpdatingClauseContextAll<'input>>>
    where
        Self: Sized,
    {
        self.child_of_type(i)
    }
    /// Retrieves all `TerminalNode`s corresponding to token SP in current rule
    fn SP_all(&self) -> Vec<Rc<TerminalNode<'input, CypherParserContextType>>>
    where
        Self: Sized,
    {
        self.children_of_type()
    }
    /// Retrieves 'i's TerminalNode corresponding to token SP, starting from 0.
    /// Returns `None` if number of children corresponding to token SP is less or equal than `i`.
    fn SP(&self, i: usize) -> Option<Rc<TerminalNode<'input, CypherParserContextType>>>
    where
        Self: Sized,
    {
        self.get_token(Cypher_SP, i)
    }
}

impl<'input> OC_MultiPartQueryContextAttrs<'input> for OC_MultiPartQueryContext<'input> {}

impl<'input, I> CypherParser<'input, I>
where
    I: TokenStream<'input, TF = LocalTokenFactory<'input>> + TidAble<'input>,
{
    pub fn oC_MultiPartQuery(
        &mut self,
    ) -> Result<Rc<OC_MultiPartQueryContextAll<'input>>, ANTLRError> {
        let mut recog = self;
        let _parentctx = recog.ctx.take();
        let mut _localctx =
            OC_MultiPartQueryContextExt::new(_parentctx.clone(), recog.base.get_state());
        recog
            .base
            .enter_rule(_localctx.clone(), 14, RULE_oC_MultiPartQuery);
        let mut _localctx: Rc<OC_MultiPartQueryContextAll> = _localctx;
        let mut _la: i32 = -1;
        let result: Result<(), ANTLRError> = (|| {
            let mut _alt: i32;
            //recog.base.enter_outer_alt(_localctx.clone(), 1)?;
            recog.base.enter_outer_alt(None, 1)?;
            {
                recog.base.set_state(326);
                recog.err_handler.sync(&mut recog.base)?;
                _alt = 1;
                loop {
                    match _alt {
                        x if x == 1 => {
                            {
                                recog.base.set_state(310);
                                recog.err_handler.sync(&mut recog.base)?;
                                _la = recog.base.input.la(1);
                                while (((_la - 48) & !0x3f) == 0
                                    && ((1usize << (_la - 48)) & 2055) != 0)
                                {
                                    {
                                        {
                                            /*InvokeRule oC_ReadingClause*/
                                            recog.base.set_state(304);
                                            recog.oC_ReadingClause()?;

                                            recog.base.set_state(306);
                                            recog.err_handler.sync(&mut recog.base)?;
                                            _la = recog.base.input.la(1);
                                            if _la == Cypher_SP {
                                                {
                                                    recog.base.set_state(305);
                                                    recog.base.match_token(
                                                        Cypher_SP,
                                                        &mut recog.err_handler,
                                                    )?;
                                                }
                                            }
                                        }
                                    }
                                    recog.base.set_state(312);
                                    recog.err_handler.sync(&mut recog.base)?;
                                    _la = recog.base.input.la(1);
                                }
                                recog.base.set_state(319);
                                recog.err_handler.sync(&mut recog.base)?;
                                _la = recog.base.input.la(1);
                                while (((_la - 52) & !0x3f) == 0
                                    && ((1usize << (_la - 52)) & 125) != 0)
                                {
                                    {
                                        {
                                            /*InvokeRule oC_UpdatingClause*/
                                            recog.base.set_state(313);
                                            recog.oC_UpdatingClause()?;

                                            recog.base.set_state(315);
                                            recog.err_handler.sync(&mut recog.base)?;
                                            _la = recog.base.input.la(1);
                                            if _la == Cypher_SP {
                                                {
                                                    recog.base.set_state(314);
                                                    recog.base.match_token(
                                                        Cypher_SP,
                                                        &mut recog.err_handler,
                                                    )?;
                                                }
                                            }
                                        }
                                    }
                                    recog.base.set_state(321);
                                    recog.err_handler.sync(&mut recog.base)?;
                                    _la = recog.base.input.la(1);
                                }
                                /*InvokeRule oC_With*/
                                recog.base.set_state(322);
                                recog.oC_With()?;

                                recog.base.set_state(324);
                                recog.err_handler.sync(&mut recog.base)?;
                                _la = recog.base.input.la(1);
                                if _la == Cypher_SP {
                                    {
                                        recog.base.set_state(323);
                                        recog
                                            .base
                                            .match_token(Cypher_SP, &mut recog.err_handler)?;
                                    }
                                }
                            }
                        }

                        _ => Err(ANTLRError::NoAltError(NoViableAltError::new(
                            &mut recog.base,
                        )))?,
                    }
                    recog.base.set_state(328);
                    recog.err_handler.sync(&mut recog.base)?;
                    _alt = recog.interpreter.adaptive_predict(25, &mut recog.base)?;
                    if _alt == 2 || _alt == INVALID_ALT {
                        break;
                    }
                }
                /*InvokeRule oC_SinglePartQuery*/
                recog.base.set_state(330);
                recog.oC_SinglePartQuery()?;
            }
            Ok(())
        })();
        match result {
            Ok(_) => {}
            Err(e @ ANTLRError::FallThrough(_)) => return Err(e),
            Err(ref re) => {
                //_localctx.exception = re;
                recog.err_handler.report_error(&mut recog.base, re);
                recog.err_handler.recover(&mut recog.base, re)?;
            }
        }
        recog.base.exit_rule()?;

        Ok(_localctx)
    }
}
//------------------- oC_UpdatingClause ----------------
pub type OC_UpdatingClauseContextAll<'input> = OC_UpdatingClauseContext<'input>;

pub type OC_UpdatingClauseContext<'input> =
    BaseParserRuleContext<'input, OC_UpdatingClauseContextExt<'input>>;

#[derive(Clone)]
pub struct OC_UpdatingClauseContextExt<'input> {
    ph: PhantomData<&'input str>,
}

impl<'input> CypherParserContext<'input> for OC_UpdatingClauseContext<'input> {}

impl<'input, 'a> Listenable<dyn CypherListener<'input> + 'a> for OC_UpdatingClauseContext<'input> {
    fn enter(&self, listener: &mut (dyn CypherListener<'input> + 'a)) -> Result<(), ANTLRError> {
        listener.enter_every_rule(self)?;
        listener.enter_oC_UpdatingClause(self);
        Ok(())
    }
    fn exit(&self, listener: &mut (dyn CypherListener<'input> + 'a)) -> Result<(), ANTLRError> {
        listener.exit_oC_UpdatingClause(self);
        listener.exit_every_rule(self)?;
        Ok(())
    }
}

impl<'input, 'a> Visitable<dyn CypherVisitor<'input> + 'a> for OC_UpdatingClauseContext<'input> {
    fn accept(&self, visitor: &mut (dyn CypherVisitor<'input> + 'a)) {
        visitor.visit_oC_UpdatingClause(self);
    }
}

impl<'input> CustomRuleContext<'input> for OC_UpdatingClauseContextExt<'input> {
    type TF = LocalTokenFactory<'input>;
    type Ctx = CypherParserContextType;
    fn get_rule_index(&self) -> usize {
        RULE_oC_UpdatingClause
    }
    //fn type_rule_index() -> usize where Self: Sized { RULE_oC_UpdatingClause }
}
antlr4rust::tid! {OC_UpdatingClauseContextExt<'a>}

impl<'input> OC_UpdatingClauseContextExt<'input> {
    fn new(
        parent: Option<Rc<dyn CypherParserContext<'input> + 'input>>,
        invoking_state: i32,
    ) -> Rc<OC_UpdatingClauseContextAll<'input>> {
        Rc::new(BaseParserRuleContext::new_parser_ctx(
            parent,
            invoking_state,
            OC_UpdatingClauseContextExt { ph: PhantomData },
        ))
    }
}

pub trait OC_UpdatingClauseContextAttrs<'input>:
    CypherParserContext<'input> + BorrowMut<OC_UpdatingClauseContextExt<'input>>
{
    fn oC_Create(&self) -> Option<Rc<OC_CreateContextAll<'input>>>
    where
        Self: Sized,
    {
        self.child_of_type(0)
    }
    fn oC_Merge(&self) -> Option<Rc<OC_MergeContextAll<'input>>>
    where
        Self: Sized,
    {
        self.child_of_type(0)
    }
    fn oC_Delete(&self) -> Option<Rc<OC_DeleteContextAll<'input>>>
    where
        Self: Sized,
    {
        self.child_of_type(0)
    }
    fn oC_Set(&self) -> Option<Rc<OC_SetContextAll<'input>>>
    where
        Self: Sized,
    {
        self.child_of_type(0)
    }
    fn oC_Remove(&self) -> Option<Rc<OC_RemoveContextAll<'input>>>
    where
        Self: Sized,
    {
        self.child_of_type(0)
    }
}

impl<'input> OC_UpdatingClauseContextAttrs<'input> for OC_UpdatingClauseContext<'input> {}

impl<'input, I> CypherParser<'input, I>
where
    I: TokenStream<'input, TF = LocalTokenFactory<'input>> + TidAble<'input>,
{
    pub fn oC_UpdatingClause(
        &mut self,
    ) -> Result<Rc<OC_UpdatingClauseContextAll<'input>>, ANTLRError> {
        let mut recog = self;
        let _parentctx = recog.ctx.take();
        let mut _localctx =
            OC_UpdatingClauseContextExt::new(_parentctx.clone(), recog.base.get_state());
        recog
            .base
            .enter_rule(_localctx.clone(), 16, RULE_oC_UpdatingClause);
        let mut _localctx: Rc<OC_UpdatingClauseContextAll> = _localctx;
        let result: Result<(), ANTLRError> = (|| {
            recog.base.set_state(337);
            recog.err_handler.sync(&mut recog.base)?;
            match recog.base.input.la(1) {
                Cypher_CREATE => {
                    //recog.base.enter_outer_alt(_localctx.clone(), 1)?;
                    recog.base.enter_outer_alt(None, 1)?;
                    {
                        /*InvokeRule oC_Create*/
                        recog.base.set_state(332);
                        recog.oC_Create()?;
                    }
                }

                Cypher_MERGE => {
                    //recog.base.enter_outer_alt(_localctx.clone(), 2)?;
                    recog.base.enter_outer_alt(None, 2)?;
                    {
                        /*InvokeRule oC_Merge*/
                        recog.base.set_state(333);
                        recog.oC_Merge()?;
                    }
                }

                Cypher_DETACH | Cypher_DELETE => {
                    //recog.base.enter_outer_alt(_localctx.clone(), 3)?;
                    recog.base.enter_outer_alt(None, 3)?;
                    {
                        /*InvokeRule oC_Delete*/
                        recog.base.set_state(334);
                        recog.oC_Delete()?;
                    }
                }

                Cypher_SET => {
                    //recog.base.enter_outer_alt(_localctx.clone(), 4)?;
                    recog.base.enter_outer_alt(None, 4)?;
                    {
                        /*InvokeRule oC_Set*/
                        recog.base.set_state(335);
                        recog.oC_Set()?;
                    }
                }

                Cypher_REMOVE => {
                    //recog.base.enter_outer_alt(_localctx.clone(), 5)?;
                    recog.base.enter_outer_alt(None, 5)?;
                    {
                        /*InvokeRule oC_Remove*/
                        recog.base.set_state(336);
                        recog.oC_Remove()?;
                    }
                }

                _ => Err(ANTLRError::NoAltError(NoViableAltError::new(
                    &mut recog.base,
                )))?,
            }
            Ok(())
        })();
        match result {
            Ok(_) => {}
            Err(e @ ANTLRError::FallThrough(_)) => return Err(e),
            Err(ref re) => {
                //_localctx.exception = re;
                recog.err_handler.report_error(&mut recog.base, re);
                recog.err_handler.recover(&mut recog.base, re)?;
            }
        }
        recog.base.exit_rule()?;

        Ok(_localctx)
    }
}
//------------------- oC_ReadingClause ----------------
pub type OC_ReadingClauseContextAll<'input> = OC_ReadingClauseContext<'input>;

pub type OC_ReadingClauseContext<'input> =
    BaseParserRuleContext<'input, OC_ReadingClauseContextExt<'input>>;

#[derive(Clone)]
pub struct OC_ReadingClauseContextExt<'input> {
    ph: PhantomData<&'input str>,
}

impl<'input> CypherParserContext<'input> for OC_ReadingClauseContext<'input> {}

impl<'input, 'a> Listenable<dyn CypherListener<'input> + 'a> for OC_ReadingClauseContext<'input> {
    fn enter(&self, listener: &mut (dyn CypherListener<'input> + 'a)) -> Result<(), ANTLRError> {
        listener.enter_every_rule(self)?;
        listener.enter_oC_ReadingClause(self);
        Ok(())
    }
    fn exit(&self, listener: &mut (dyn CypherListener<'input> + 'a)) -> Result<(), ANTLRError> {
        listener.exit_oC_ReadingClause(self);
        listener.exit_every_rule(self)?;
        Ok(())
    }
}

impl<'input, 'a> Visitable<dyn CypherVisitor<'input> + 'a> for OC_ReadingClauseContext<'input> {
    fn accept(&self, visitor: &mut (dyn CypherVisitor<'input> + 'a)) {
        visitor.visit_oC_ReadingClause(self);
    }
}

impl<'input> CustomRuleContext<'input> for OC_ReadingClauseContextExt<'input> {
    type TF = LocalTokenFactory<'input>;
    type Ctx = CypherParserContextType;
    fn get_rule_index(&self) -> usize {
        RULE_oC_ReadingClause
    }
    //fn type_rule_index() -> usize where Self: Sized { RULE_oC_ReadingClause }
}
antlr4rust::tid! {OC_ReadingClauseContextExt<'a>}

impl<'input> OC_ReadingClauseContextExt<'input> {
    fn new(
        parent: Option<Rc<dyn CypherParserContext<'input> + 'input>>,
        invoking_state: i32,
    ) -> Rc<OC_ReadingClauseContextAll<'input>> {
        Rc::new(BaseParserRuleContext::new_parser_ctx(
            parent,
            invoking_state,
            OC_ReadingClauseContextExt { ph: PhantomData },
        ))
    }
}

pub trait OC_ReadingClauseContextAttrs<'input>:
    CypherParserContext<'input> + BorrowMut<OC_ReadingClauseContextExt<'input>>
{
    fn oC_Match(&self) -> Option<Rc<OC_MatchContextAll<'input>>>
    where
        Self: Sized,
    {
        self.child_of_type(0)
    }
    fn oC_Unwind(&self) -> Option<Rc<OC_UnwindContextAll<'input>>>
    where
        Self: Sized,
    {
        self.child_of_type(0)
    }
    fn oC_InQueryCall(&self) -> Option<Rc<OC_InQueryCallContextAll<'input>>>
    where
        Self: Sized,
    {
        self.child_of_type(0)
    }
}

impl<'input> OC_ReadingClauseContextAttrs<'input> for OC_ReadingClauseContext<'input> {}

impl<'input, I> CypherParser<'input, I>
where
    I: TokenStream<'input, TF = LocalTokenFactory<'input>> + TidAble<'input>,
{
    pub fn oC_ReadingClause(
        &mut self,
    ) -> Result<Rc<OC_ReadingClauseContextAll<'input>>, ANTLRError> {
        let mut recog = self;
        let _parentctx = recog.ctx.take();
        let mut _localctx =
            OC_ReadingClauseContextExt::new(_parentctx.clone(), recog.base.get_state());
        recog
            .base
            .enter_rule(_localctx.clone(), 18, RULE_oC_ReadingClause);
        let mut _localctx: Rc<OC_ReadingClauseContextAll> = _localctx;
        let result: Result<(), ANTLRError> = (|| {
            recog.base.set_state(342);
            recog.err_handler.sync(&mut recog.base)?;
            match recog.base.input.la(1) {
                Cypher_OPTIONAL | Cypher_MATCH => {
                    //recog.base.enter_outer_alt(_localctx.clone(), 1)?;
                    recog.base.enter_outer_alt(None, 1)?;
                    {
                        /*InvokeRule oC_Match*/
                        recog.base.set_state(339);
                        recog.oC_Match()?;
                    }
                }

                Cypher_UNWIND => {
                    //recog.base.enter_outer_alt(_localctx.clone(), 2)?;
                    recog.base.enter_outer_alt(None, 2)?;
                    {
                        /*InvokeRule oC_Unwind*/
                        recog.base.set_state(340);
                        recog.oC_Unwind()?;
                    }
                }

                Cypher_CALL => {
                    //recog.base.enter_outer_alt(_localctx.clone(), 3)?;
                    recog.base.enter_outer_alt(None, 3)?;
                    {
                        /*InvokeRule oC_InQueryCall*/
                        recog.base.set_state(341);
                        recog.oC_InQueryCall()?;
                    }
                }

                _ => Err(ANTLRError::NoAltError(NoViableAltError::new(
                    &mut recog.base,
                )))?,
            }
            Ok(())
        })();
        match result {
            Ok(_) => {}
            Err(e @ ANTLRError::FallThrough(_)) => return Err(e),
            Err(ref re) => {
                //_localctx.exception = re;
                recog.err_handler.report_error(&mut recog.base, re);
                recog.err_handler.recover(&mut recog.base, re)?;
            }
        }
        recog.base.exit_rule()?;

        Ok(_localctx)
    }
}
//------------------- oC_Match ----------------
pub type OC_MatchContextAll<'input> = OC_MatchContext<'input>;

pub type OC_MatchContext<'input> = BaseParserRuleContext<'input, OC_MatchContextExt<'input>>;

#[derive(Clone)]
pub struct OC_MatchContextExt<'input> {
    ph: PhantomData<&'input str>,
}

impl<'input> CypherParserContext<'input> for OC_MatchContext<'input> {}

impl<'input, 'a> Listenable<dyn CypherListener<'input> + 'a> for OC_MatchContext<'input> {
    fn enter(&self, listener: &mut (dyn CypherListener<'input> + 'a)) -> Result<(), ANTLRError> {
        listener.enter_every_rule(self)?;
        listener.enter_oC_Match(self);
        Ok(())
    }
    fn exit(&self, listener: &mut (dyn CypherListener<'input> + 'a)) -> Result<(), ANTLRError> {
        listener.exit_oC_Match(self);
        listener.exit_every_rule(self)?;
        Ok(())
    }
}

impl<'input, 'a> Visitable<dyn CypherVisitor<'input> + 'a> for OC_MatchContext<'input> {
    fn accept(&self, visitor: &mut (dyn CypherVisitor<'input> + 'a)) {
        visitor.visit_oC_Match(self);
    }
}

impl<'input> CustomRuleContext<'input> for OC_MatchContextExt<'input> {
    type TF = LocalTokenFactory<'input>;
    type Ctx = CypherParserContextType;
    fn get_rule_index(&self) -> usize {
        RULE_oC_Match
    }
    //fn type_rule_index() -> usize where Self: Sized { RULE_oC_Match }
}
antlr4rust::tid! {OC_MatchContextExt<'a>}

impl<'input> OC_MatchContextExt<'input> {
    fn new(
        parent: Option<Rc<dyn CypherParserContext<'input> + 'input>>,
        invoking_state: i32,
    ) -> Rc<OC_MatchContextAll<'input>> {
        Rc::new(BaseParserRuleContext::new_parser_ctx(
            parent,
            invoking_state,
            OC_MatchContextExt { ph: PhantomData },
        ))
    }
}

pub trait OC_MatchContextAttrs<'input>:
    CypherParserContext<'input> + BorrowMut<OC_MatchContextExt<'input>>
{
    /// Retrieves first TerminalNode corresponding to token MATCH
    /// Returns `None` if there is no child corresponding to token MATCH
    fn MATCH(&self) -> Option<Rc<TerminalNode<'input, CypherParserContextType>>>
    where
        Self: Sized,
    {
        self.get_token(Cypher_MATCH, 0)
    }
    fn oC_Pattern(&self) -> Option<Rc<OC_PatternContextAll<'input>>>
    where
        Self: Sized,
    {
        self.child_of_type(0)
    }
    /// Retrieves first TerminalNode corresponding to token OPTIONAL
    /// Returns `None` if there is no child corresponding to token OPTIONAL
    fn OPTIONAL(&self) -> Option<Rc<TerminalNode<'input, CypherParserContextType>>>
    where
        Self: Sized,
    {
        self.get_token(Cypher_OPTIONAL, 0)
    }
    /// Retrieves all `TerminalNode`s corresponding to token SP in current rule
    fn SP_all(&self) -> Vec<Rc<TerminalNode<'input, CypherParserContextType>>>
    where
        Self: Sized,
    {
        self.children_of_type()
    }
    /// Retrieves 'i's TerminalNode corresponding to token SP, starting from 0.
    /// Returns `None` if number of children corresponding to token SP is less or equal than `i`.
    fn SP(&self, i: usize) -> Option<Rc<TerminalNode<'input, CypherParserContextType>>>
    where
        Self: Sized,
    {
        self.get_token(Cypher_SP, i)
    }
    fn oC_Where(&self) -> Option<Rc<OC_WhereContextAll<'input>>>
    where
        Self: Sized,
    {
        self.child_of_type(0)
    }
}

impl<'input> OC_MatchContextAttrs<'input> for OC_MatchContext<'input> {}

impl<'input, I> CypherParser<'input, I>
where
    I: TokenStream<'input, TF = LocalTokenFactory<'input>> + TidAble<'input>,
{
    pub fn oC_Match(&mut self) -> Result<Rc<OC_MatchContextAll<'input>>, ANTLRError> {
        let mut recog = self;
        let _parentctx = recog.ctx.take();
        let mut _localctx = OC_MatchContextExt::new(_parentctx.clone(), recog.base.get_state());
        recog.base.enter_rule(_localctx.clone(), 20, RULE_oC_Match);
        let mut _localctx: Rc<OC_MatchContextAll> = _localctx;
        let mut _la: i32 = -1;
        let result: Result<(), ANTLRError> = (|| {
            //recog.base.enter_outer_alt(_localctx.clone(), 1)?;
            recog.base.enter_outer_alt(None, 1)?;
            {
                recog.base.set_state(346);
                recog.err_handler.sync(&mut recog.base)?;
                _la = recog.base.input.la(1);
                if _la == Cypher_OPTIONAL {
                    {
                        recog.base.set_state(344);
                        recog
                            .base
                            .match_token(Cypher_OPTIONAL, &mut recog.err_handler)?;

                        recog.base.set_state(345);
                        recog.base.match_token(Cypher_SP, &mut recog.err_handler)?;
                    }
                }

                recog.base.set_state(348);
                recog
                    .base
                    .match_token(Cypher_MATCH, &mut recog.err_handler)?;

                recog.base.set_state(350);
                recog.err_handler.sync(&mut recog.base)?;
                _la = recog.base.input.la(1);
                if _la == Cypher_SP {
                    {
                        recog.base.set_state(349);
                        recog.base.match_token(Cypher_SP, &mut recog.err_handler)?;
                    }
                }

                /*InvokeRule oC_Pattern*/
                recog.base.set_state(352);
                recog.oC_Pattern()?;

                recog.base.set_state(357);
                recog.err_handler.sync(&mut recog.base)?;
                match recog.interpreter.adaptive_predict(31, &mut recog.base)? {
                    x if x == 1 => {
                        {
                            recog.base.set_state(354);
                            recog.err_handler.sync(&mut recog.base)?;
                            _la = recog.base.input.la(1);
                            if _la == Cypher_SP {
                                {
                                    recog.base.set_state(353);
                                    recog.base.match_token(Cypher_SP, &mut recog.err_handler)?;
                                }
                            }

                            /*InvokeRule oC_Where*/
                            recog.base.set_state(356);
                            recog.oC_Where()?;
                        }
                    }

                    _ => {}
                }
            }
            Ok(())
        })();
        match result {
            Ok(_) => {}
            Err(e @ ANTLRError::FallThrough(_)) => return Err(e),
            Err(ref re) => {
                //_localctx.exception = re;
                recog.err_handler.report_error(&mut recog.base, re);
                recog.err_handler.recover(&mut recog.base, re)?;
            }
        }
        recog.base.exit_rule()?;

        Ok(_localctx)
    }
}
//------------------- oC_Unwind ----------------
pub type OC_UnwindContextAll<'input> = OC_UnwindContext<'input>;

pub type OC_UnwindContext<'input> = BaseParserRuleContext<'input, OC_UnwindContextExt<'input>>;

#[derive(Clone)]
pub struct OC_UnwindContextExt<'input> {
    ph: PhantomData<&'input str>,
}

impl<'input> CypherParserContext<'input> for OC_UnwindContext<'input> {}

impl<'input, 'a> Listenable<dyn CypherListener<'input> + 'a> for OC_UnwindContext<'input> {
    fn enter(&self, listener: &mut (dyn CypherListener<'input> + 'a)) -> Result<(), ANTLRError> {
        listener.enter_every_rule(self)?;
        listener.enter_oC_Unwind(self);
        Ok(())
    }
    fn exit(&self, listener: &mut (dyn CypherListener<'input> + 'a)) -> Result<(), ANTLRError> {
        listener.exit_oC_Unwind(self);
        listener.exit_every_rule(self)?;
        Ok(())
    }
}

impl<'input, 'a> Visitable<dyn CypherVisitor<'input> + 'a> for OC_UnwindContext<'input> {
    fn accept(&self, visitor: &mut (dyn CypherVisitor<'input> + 'a)) {
        visitor.visit_oC_Unwind(self);
    }
}

impl<'input> CustomRuleContext<'input> for OC_UnwindContextExt<'input> {
    type TF = LocalTokenFactory<'input>;
    type Ctx = CypherParserContextType;
    fn get_rule_index(&self) -> usize {
        RULE_oC_Unwind
    }
    //fn type_rule_index() -> usize where Self: Sized { RULE_oC_Unwind }
}
antlr4rust::tid! {OC_UnwindContextExt<'a>}

impl<'input> OC_UnwindContextExt<'input> {
    fn new(
        parent: Option<Rc<dyn CypherParserContext<'input> + 'input>>,
        invoking_state: i32,
    ) -> Rc<OC_UnwindContextAll<'input>> {
        Rc::new(BaseParserRuleContext::new_parser_ctx(
            parent,
            invoking_state,
            OC_UnwindContextExt { ph: PhantomData },
        ))
    }
}

pub trait OC_UnwindContextAttrs<'input>:
    CypherParserContext<'input> + BorrowMut<OC_UnwindContextExt<'input>>
{
    /// Retrieves first TerminalNode corresponding to token UNWIND
    /// Returns `None` if there is no child corresponding to token UNWIND
    fn UNWIND(&self) -> Option<Rc<TerminalNode<'input, CypherParserContextType>>>
    where
        Self: Sized,
    {
        self.get_token(Cypher_UNWIND, 0)
    }
    fn oC_Expression(&self) -> Option<Rc<OC_ExpressionContextAll<'input>>>
    where
        Self: Sized,
    {
        self.child_of_type(0)
    }
    /// Retrieves all `TerminalNode`s corresponding to token SP in current rule
    fn SP_all(&self) -> Vec<Rc<TerminalNode<'input, CypherParserContextType>>>
    where
        Self: Sized,
    {
        self.children_of_type()
    }
    /// Retrieves 'i's TerminalNode corresponding to token SP, starting from 0.
    /// Returns `None` if number of children corresponding to token SP is less or equal than `i`.
    fn SP(&self, i: usize) -> Option<Rc<TerminalNode<'input, CypherParserContextType>>>
    where
        Self: Sized,
    {
        self.get_token(Cypher_SP, i)
    }
    /// Retrieves first TerminalNode corresponding to token AS
    /// Returns `None` if there is no child corresponding to token AS
    fn AS(&self) -> Option<Rc<TerminalNode<'input, CypherParserContextType>>>
    where
        Self: Sized,
    {
        self.get_token(Cypher_AS, 0)
    }
    fn oC_Variable(&self) -> Option<Rc<OC_VariableContextAll<'input>>>
    where
        Self: Sized,
    {
        self.child_of_type(0)
    }
}

impl<'input> OC_UnwindContextAttrs<'input> for OC_UnwindContext<'input> {}

impl<'input, I> CypherParser<'input, I>
where
    I: TokenStream<'input, TF = LocalTokenFactory<'input>> + TidAble<'input>,
{
    pub fn oC_Unwind(&mut self) -> Result<Rc<OC_UnwindContextAll<'input>>, ANTLRError> {
        let mut recog = self;
        let _parentctx = recog.ctx.take();
        let mut _localctx = OC_UnwindContextExt::new(_parentctx.clone(), recog.base.get_state());
        recog.base.enter_rule(_localctx.clone(), 22, RULE_oC_Unwind);
        let mut _localctx: Rc<OC_UnwindContextAll> = _localctx;
        let mut _la: i32 = -1;
        let result: Result<(), ANTLRError> = (|| {
            //recog.base.enter_outer_alt(_localctx.clone(), 1)?;
            recog.base.enter_outer_alt(None, 1)?;
            {
                recog.base.set_state(359);
                recog
                    .base
                    .match_token(Cypher_UNWIND, &mut recog.err_handler)?;

                recog.base.set_state(361);
                recog.err_handler.sync(&mut recog.base)?;
                _la = recog.base.input.la(1);
                if _la == Cypher_SP {
                    {
                        recog.base.set_state(360);
                        recog.base.match_token(Cypher_SP, &mut recog.err_handler)?;
                    }
                }

                /*InvokeRule oC_Expression*/
                recog.base.set_state(363);
                recog.oC_Expression()?;

                recog.base.set_state(364);
                recog.base.match_token(Cypher_SP, &mut recog.err_handler)?;

                recog.base.set_state(365);
                recog.base.match_token(Cypher_AS, &mut recog.err_handler)?;

                recog.base.set_state(366);
                recog.base.match_token(Cypher_SP, &mut recog.err_handler)?;

                /*InvokeRule oC_Variable*/
                recog.base.set_state(367);
                recog.oC_Variable()?;
            }
            Ok(())
        })();
        match result {
            Ok(_) => {}
            Err(e @ ANTLRError::FallThrough(_)) => return Err(e),
            Err(ref re) => {
                //_localctx.exception = re;
                recog.err_handler.report_error(&mut recog.base, re);
                recog.err_handler.recover(&mut recog.base, re)?;
            }
        }
        recog.base.exit_rule()?;

        Ok(_localctx)
    }
}
//------------------- oC_Merge ----------------
pub type OC_MergeContextAll<'input> = OC_MergeContext<'input>;

pub type OC_MergeContext<'input> = BaseParserRuleContext<'input, OC_MergeContextExt<'input>>;

#[derive(Clone)]
pub struct OC_MergeContextExt<'input> {
    ph: PhantomData<&'input str>,
}

impl<'input> CypherParserContext<'input> for OC_MergeContext<'input> {}

impl<'input, 'a> Listenable<dyn CypherListener<'input> + 'a> for OC_MergeContext<'input> {
    fn enter(&self, listener: &mut (dyn CypherListener<'input> + 'a)) -> Result<(), ANTLRError> {
        listener.enter_every_rule(self)?;
        listener.enter_oC_Merge(self);
        Ok(())
    }
    fn exit(&self, listener: &mut (dyn CypherListener<'input> + 'a)) -> Result<(), ANTLRError> {
        listener.exit_oC_Merge(self);
        listener.exit_every_rule(self)?;
        Ok(())
    }
}

impl<'input, 'a> Visitable<dyn CypherVisitor<'input> + 'a> for OC_MergeContext<'input> {
    fn accept(&self, visitor: &mut (dyn CypherVisitor<'input> + 'a)) {
        visitor.visit_oC_Merge(self);
    }
}

impl<'input> CustomRuleContext<'input> for OC_MergeContextExt<'input> {
    type TF = LocalTokenFactory<'input>;
    type Ctx = CypherParserContextType;
    fn get_rule_index(&self) -> usize {
        RULE_oC_Merge
    }
    //fn type_rule_index() -> usize where Self: Sized { RULE_oC_Merge }
}
antlr4rust::tid! {OC_MergeContextExt<'a>}

impl<'input> OC_MergeContextExt<'input> {
    fn new(
        parent: Option<Rc<dyn CypherParserContext<'input> + 'input>>,
        invoking_state: i32,
    ) -> Rc<OC_MergeContextAll<'input>> {
        Rc::new(BaseParserRuleContext::new_parser_ctx(
            parent,
            invoking_state,
            OC_MergeContextExt { ph: PhantomData },
        ))
    }
}

pub trait OC_MergeContextAttrs<'input>:
    CypherParserContext<'input> + BorrowMut<OC_MergeContextExt<'input>>
{
    /// Retrieves first TerminalNode corresponding to token MERGE
    /// Returns `None` if there is no child corresponding to token MERGE
    fn MERGE(&self) -> Option<Rc<TerminalNode<'input, CypherParserContextType>>>
    where
        Self: Sized,
    {
        self.get_token(Cypher_MERGE, 0)
    }
    fn oC_PatternPart(&self) -> Option<Rc<OC_PatternPartContextAll<'input>>>
    where
        Self: Sized,
    {
        self.child_of_type(0)
    }
    /// Retrieves all `TerminalNode`s corresponding to token SP in current rule
    fn SP_all(&self) -> Vec<Rc<TerminalNode<'input, CypherParserContextType>>>
    where
        Self: Sized,
    {
        self.children_of_type()
    }
    /// Retrieves 'i's TerminalNode corresponding to token SP, starting from 0.
    /// Returns `None` if number of children corresponding to token SP is less or equal than `i`.
    fn SP(&self, i: usize) -> Option<Rc<TerminalNode<'input, CypherParserContextType>>>
    where
        Self: Sized,
    {
        self.get_token(Cypher_SP, i)
    }
    fn oC_MergeAction_all(&self) -> Vec<Rc<OC_MergeActionContextAll<'input>>>
    where
        Self: Sized,
    {
        self.children_of_type()
    }
    fn oC_MergeAction(&self, i: usize) -> Option<Rc<OC_MergeActionContextAll<'input>>>
    where
        Self: Sized,
    {
        self.child_of_type(i)
    }
}

impl<'input> OC_MergeContextAttrs<'input> for OC_MergeContext<'input> {}

impl<'input, I> CypherParser<'input, I>
where
    I: TokenStream<'input, TF = LocalTokenFactory<'input>> + TidAble<'input>,
{
    pub fn oC_Merge(&mut self) -> Result<Rc<OC_MergeContextAll<'input>>, ANTLRError> {
        let mut recog = self;
        let _parentctx = recog.ctx.take();
        let mut _localctx = OC_MergeContextExt::new(_parentctx.clone(), recog.base.get_state());
        recog.base.enter_rule(_localctx.clone(), 24, RULE_oC_Merge);
        let mut _localctx: Rc<OC_MergeContextAll> = _localctx;
        let mut _la: i32 = -1;
        let result: Result<(), ANTLRError> = (|| {
            let mut _alt: i32;
            //recog.base.enter_outer_alt(_localctx.clone(), 1)?;
            recog.base.enter_outer_alt(None, 1)?;
            {
                recog.base.set_state(369);
                recog
                    .base
                    .match_token(Cypher_MERGE, &mut recog.err_handler)?;

                recog.base.set_state(371);
                recog.err_handler.sync(&mut recog.base)?;
                _la = recog.base.input.la(1);
                if _la == Cypher_SP {
                    {
                        recog.base.set_state(370);
                        recog.base.match_token(Cypher_SP, &mut recog.err_handler)?;
                    }
                }

                /*InvokeRule oC_PatternPart*/
                recog.base.set_state(373);
                recog.oC_PatternPart()?;

                recog.base.set_state(378);
                recog.err_handler.sync(&mut recog.base)?;
                _alt = recog.interpreter.adaptive_predict(34, &mut recog.base)?;
                while { _alt != 2 && _alt != INVALID_ALT } {
                    if _alt == 1 {
                        {
                            {
                                recog.base.set_state(374);
                                recog.base.match_token(Cypher_SP, &mut recog.err_handler)?;

                                /*InvokeRule oC_MergeAction*/
                                recog.base.set_state(375);
                                recog.oC_MergeAction()?;
                            }
                        }
                    }
                    recog.base.set_state(380);
                    recog.err_handler.sync(&mut recog.base)?;
                    _alt = recog.interpreter.adaptive_predict(34, &mut recog.base)?;
                }
            }
            Ok(())
        })();
        match result {
            Ok(_) => {}
            Err(e @ ANTLRError::FallThrough(_)) => return Err(e),
            Err(ref re) => {
                //_localctx.exception = re;
                recog.err_handler.report_error(&mut recog.base, re);
                recog.err_handler.recover(&mut recog.base, re)?;
            }
        }
        recog.base.exit_rule()?;

        Ok(_localctx)
    }
}
//------------------- oC_MergeAction ----------------
pub type OC_MergeActionContextAll<'input> = OC_MergeActionContext<'input>;

pub type OC_MergeActionContext<'input> =
    BaseParserRuleContext<'input, OC_MergeActionContextExt<'input>>;

#[derive(Clone)]
pub struct OC_MergeActionContextExt<'input> {
    ph: PhantomData<&'input str>,
}

impl<'input> CypherParserContext<'input> for OC_MergeActionContext<'input> {}

impl<'input, 'a> Listenable<dyn CypherListener<'input> + 'a> for OC_MergeActionContext<'input> {
    fn enter(&self, listener: &mut (dyn CypherListener<'input> + 'a)) -> Result<(), ANTLRError> {
        listener.enter_every_rule(self)?;
        listener.enter_oC_MergeAction(self);
        Ok(())
    }
    fn exit(&self, listener: &mut (dyn CypherListener<'input> + 'a)) -> Result<(), ANTLRError> {
        listener.exit_oC_MergeAction(self);
        listener.exit_every_rule(self)?;
        Ok(())
    }
}

impl<'input, 'a> Visitable<dyn CypherVisitor<'input> + 'a> for OC_MergeActionContext<'input> {
    fn accept(&self, visitor: &mut (dyn CypherVisitor<'input> + 'a)) {
        visitor.visit_oC_MergeAction(self);
    }
}

impl<'input> CustomRuleContext<'input> for OC_MergeActionContextExt<'input> {
    type TF = LocalTokenFactory<'input>;
    type Ctx = CypherParserContextType;
    fn get_rule_index(&self) -> usize {
        RULE_oC_MergeAction
    }
    //fn type_rule_index() -> usize where Self: Sized { RULE_oC_MergeAction }
}
antlr4rust::tid! {OC_MergeActionContextExt<'a>}

impl<'input> OC_MergeActionContextExt<'input> {
    fn new(
        parent: Option<Rc<dyn CypherParserContext<'input> + 'input>>,
        invoking_state: i32,
    ) -> Rc<OC_MergeActionContextAll<'input>> {
        Rc::new(BaseParserRuleContext::new_parser_ctx(
            parent,
            invoking_state,
            OC_MergeActionContextExt { ph: PhantomData },
        ))
    }
}

pub trait OC_MergeActionContextAttrs<'input>:
    CypherParserContext<'input> + BorrowMut<OC_MergeActionContextExt<'input>>
{
    /// Retrieves first TerminalNode corresponding to token ON
    /// Returns `None` if there is no child corresponding to token ON
    fn ON(&self) -> Option<Rc<TerminalNode<'input, CypherParserContextType>>>
    where
        Self: Sized,
    {
        self.get_token(Cypher_ON, 0)
    }
    /// Retrieves all `TerminalNode`s corresponding to token SP in current rule
    fn SP_all(&self) -> Vec<Rc<TerminalNode<'input, CypherParserContextType>>>
    where
        Self: Sized,
    {
        self.children_of_type()
    }
    /// Retrieves 'i's TerminalNode corresponding to token SP, starting from 0.
    /// Returns `None` if number of children corresponding to token SP is less or equal than `i`.
    fn SP(&self, i: usize) -> Option<Rc<TerminalNode<'input, CypherParserContextType>>>
    where
        Self: Sized,
    {
        self.get_token(Cypher_SP, i)
    }
    /// Retrieves first TerminalNode corresponding to token MATCH
    /// Returns `None` if there is no child corresponding to token MATCH
    fn MATCH(&self) -> Option<Rc<TerminalNode<'input, CypherParserContextType>>>
    where
        Self: Sized,
    {
        self.get_token(Cypher_MATCH, 0)
    }
    fn oC_Set(&self) -> Option<Rc<OC_SetContextAll<'input>>>
    where
        Self: Sized,
    {
        self.child_of_type(0)
    }
    /// Retrieves first TerminalNode corresponding to token CREATE
    /// Returns `None` if there is no child corresponding to token CREATE
    fn CREATE(&self) -> Option<Rc<TerminalNode<'input, CypherParserContextType>>>
    where
        Self: Sized,
    {
        self.get_token(Cypher_CREATE, 0)
    }
}

impl<'input> OC_MergeActionContextAttrs<'input> for OC_MergeActionContext<'input> {}

impl<'input, I> CypherParser<'input, I>
where
    I: TokenStream<'input, TF = LocalTokenFactory<'input>> + TidAble<'input>,
{
    pub fn oC_MergeAction(&mut self) -> Result<Rc<OC_MergeActionContextAll<'input>>, ANTLRError> {
        let mut recog = self;
        let _parentctx = recog.ctx.take();
        let mut _localctx =
            OC_MergeActionContextExt::new(_parentctx.clone(), recog.base.get_state());
        recog
            .base
            .enter_rule(_localctx.clone(), 26, RULE_oC_MergeAction);
        let mut _localctx: Rc<OC_MergeActionContextAll> = _localctx;
        let result: Result<(), ANTLRError> = (|| {
            recog.base.set_state(391);
            recog.err_handler.sync(&mut recog.base)?;
            match recog.interpreter.adaptive_predict(35, &mut recog.base)? {
                1 => {
                    //recog.base.enter_outer_alt(_localctx.clone(), 1)?;
                    recog.base.enter_outer_alt(None, 1)?;
                    {
                        {
                            recog.base.set_state(381);
                            recog.base.match_token(Cypher_ON, &mut recog.err_handler)?;

                            recog.base.set_state(382);
                            recog.base.match_token(Cypher_SP, &mut recog.err_handler)?;

                            recog.base.set_state(383);
                            recog
                                .base
                                .match_token(Cypher_MATCH, &mut recog.err_handler)?;

                            recog.base.set_state(384);
                            recog.base.match_token(Cypher_SP, &mut recog.err_handler)?;

                            /*InvokeRule oC_Set*/
                            recog.base.set_state(385);
                            recog.oC_Set()?;
                        }
                    }
                }
                2 => {
                    //recog.base.enter_outer_alt(_localctx.clone(), 2)?;
                    recog.base.enter_outer_alt(None, 2)?;
                    {
                        {
                            recog.base.set_state(386);
                            recog.base.match_token(Cypher_ON, &mut recog.err_handler)?;

                            recog.base.set_state(387);
                            recog.base.match_token(Cypher_SP, &mut recog.err_handler)?;

                            recog.base.set_state(388);
                            recog
                                .base
                                .match_token(Cypher_CREATE, &mut recog.err_handler)?;

                            recog.base.set_state(389);
                            recog.base.match_token(Cypher_SP, &mut recog.err_handler)?;

                            /*InvokeRule oC_Set*/
                            recog.base.set_state(390);
                            recog.oC_Set()?;
                        }
                    }
                }

                _ => {}
            }
            Ok(())
        })();
        match result {
            Ok(_) => {}
            Err(e @ ANTLRError::FallThrough(_)) => return Err(e),
            Err(ref re) => {
                //_localctx.exception = re;
                recog.err_handler.report_error(&mut recog.base, re);
                recog.err_handler.recover(&mut recog.base, re)?;
            }
        }
        recog.base.exit_rule()?;

        Ok(_localctx)
    }
}
//------------------- oC_Create ----------------
pub type OC_CreateContextAll<'input> = OC_CreateContext<'input>;

pub type OC_CreateContext<'input> = BaseParserRuleContext<'input, OC_CreateContextExt<'input>>;

#[derive(Clone)]
pub struct OC_CreateContextExt<'input> {
    ph: PhantomData<&'input str>,
}

impl<'input> CypherParserContext<'input> for OC_CreateContext<'input> {}

impl<'input, 'a> Listenable<dyn CypherListener<'input> + 'a> for OC_CreateContext<'input> {
    fn enter(&self, listener: &mut (dyn CypherListener<'input> + 'a)) -> Result<(), ANTLRError> {
        listener.enter_every_rule(self)?;
        listener.enter_oC_Create(self);
        Ok(())
    }
    fn exit(&self, listener: &mut (dyn CypherListener<'input> + 'a)) -> Result<(), ANTLRError> {
        listener.exit_oC_Create(self);
        listener.exit_every_rule(self)?;
        Ok(())
    }
}

impl<'input, 'a> Visitable<dyn CypherVisitor<'input> + 'a> for OC_CreateContext<'input> {
    fn accept(&self, visitor: &mut (dyn CypherVisitor<'input> + 'a)) {
        visitor.visit_oC_Create(self);
    }
}

impl<'input> CustomRuleContext<'input> for OC_CreateContextExt<'input> {
    type TF = LocalTokenFactory<'input>;
    type Ctx = CypherParserContextType;
    fn get_rule_index(&self) -> usize {
        RULE_oC_Create
    }
    //fn type_rule_index() -> usize where Self: Sized { RULE_oC_Create }
}
antlr4rust::tid! {OC_CreateContextExt<'a>}

impl<'input> OC_CreateContextExt<'input> {
    fn new(
        parent: Option<Rc<dyn CypherParserContext<'input> + 'input>>,
        invoking_state: i32,
    ) -> Rc<OC_CreateContextAll<'input>> {
        Rc::new(BaseParserRuleContext::new_parser_ctx(
            parent,
            invoking_state,
            OC_CreateContextExt { ph: PhantomData },
        ))
    }
}

pub trait OC_CreateContextAttrs<'input>:
    CypherParserContext<'input> + BorrowMut<OC_CreateContextExt<'input>>
{
    /// Retrieves first TerminalNode corresponding to token CREATE
    /// Returns `None` if there is no child corresponding to token CREATE
    fn CREATE(&self) -> Option<Rc<TerminalNode<'input, CypherParserContextType>>>
    where
        Self: Sized,
    {
        self.get_token(Cypher_CREATE, 0)
    }
    fn oC_Pattern(&self) -> Option<Rc<OC_PatternContextAll<'input>>>
    where
        Self: Sized,
    {
        self.child_of_type(0)
    }
    /// Retrieves first TerminalNode corresponding to token SP
    /// Returns `None` if there is no child corresponding to token SP
    fn SP(&self) -> Option<Rc<TerminalNode<'input, CypherParserContextType>>>
    where
        Self: Sized,
    {
        self.get_token(Cypher_SP, 0)
    }
}

impl<'input> OC_CreateContextAttrs<'input> for OC_CreateContext<'input> {}

impl<'input, I> CypherParser<'input, I>
where
    I: TokenStream<'input, TF = LocalTokenFactory<'input>> + TidAble<'input>,
{
    pub fn oC_Create(&mut self) -> Result<Rc<OC_CreateContextAll<'input>>, ANTLRError> {
        let mut recog = self;
        let _parentctx = recog.ctx.take();
        let mut _localctx = OC_CreateContextExt::new(_parentctx.clone(), recog.base.get_state());
        recog.base.enter_rule(_localctx.clone(), 28, RULE_oC_Create);
        let mut _localctx: Rc<OC_CreateContextAll> = _localctx;
        let mut _la: i32 = -1;
        let result: Result<(), ANTLRError> = (|| {
            //recog.base.enter_outer_alt(_localctx.clone(), 1)?;
            recog.base.enter_outer_alt(None, 1)?;
            {
                recog.base.set_state(393);
                recog
                    .base
                    .match_token(Cypher_CREATE, &mut recog.err_handler)?;

                recog.base.set_state(395);
                recog.err_handler.sync(&mut recog.base)?;
                _la = recog.base.input.la(1);
                if _la == Cypher_SP {
                    {
                        recog.base.set_state(394);
                        recog.base.match_token(Cypher_SP, &mut recog.err_handler)?;
                    }
                }

                /*InvokeRule oC_Pattern*/
                recog.base.set_state(397);
                recog.oC_Pattern()?;
            }
            Ok(())
        })();
        match result {
            Ok(_) => {}
            Err(e @ ANTLRError::FallThrough(_)) => return Err(e),
            Err(ref re) => {
                //_localctx.exception = re;
                recog.err_handler.report_error(&mut recog.base, re);
                recog.err_handler.recover(&mut recog.base, re)?;
            }
        }
        recog.base.exit_rule()?;

        Ok(_localctx)
    }
}
//------------------- oC_Set ----------------
pub type OC_SetContextAll<'input> = OC_SetContext<'input>;

pub type OC_SetContext<'input> = BaseParserRuleContext<'input, OC_SetContextExt<'input>>;

#[derive(Clone)]
pub struct OC_SetContextExt<'input> {
    ph: PhantomData<&'input str>,
}

impl<'input> CypherParserContext<'input> for OC_SetContext<'input> {}

impl<'input, 'a> Listenable<dyn CypherListener<'input> + 'a> for OC_SetContext<'input> {
    fn enter(&self, listener: &mut (dyn CypherListener<'input> + 'a)) -> Result<(), ANTLRError> {
        listener.enter_every_rule(self)?;
        listener.enter_oC_Set(self);
        Ok(())
    }
    fn exit(&self, listener: &mut (dyn CypherListener<'input> + 'a)) -> Result<(), ANTLRError> {
        listener.exit_oC_Set(self);
        listener.exit_every_rule(self)?;
        Ok(())
    }
}

impl<'input, 'a> Visitable<dyn CypherVisitor<'input> + 'a> for OC_SetContext<'input> {
    fn accept(&self, visitor: &mut (dyn CypherVisitor<'input> + 'a)) {
        visitor.visit_oC_Set(self);
    }
}

impl<'input> CustomRuleContext<'input> for OC_SetContextExt<'input> {
    type TF = LocalTokenFactory<'input>;
    type Ctx = CypherParserContextType;
    fn get_rule_index(&self) -> usize {
        RULE_oC_Set
    }
    //fn type_rule_index() -> usize where Self: Sized { RULE_oC_Set }
}
antlr4rust::tid! {OC_SetContextExt<'a>}

impl<'input> OC_SetContextExt<'input> {
    fn new(
        parent: Option<Rc<dyn CypherParserContext<'input> + 'input>>,
        invoking_state: i32,
    ) -> Rc<OC_SetContextAll<'input>> {
        Rc::new(BaseParserRuleContext::new_parser_ctx(
            parent,
            invoking_state,
            OC_SetContextExt { ph: PhantomData },
        ))
    }
}

pub trait OC_SetContextAttrs<'input>:
    CypherParserContext<'input> + BorrowMut<OC_SetContextExt<'input>>
{
    /// Retrieves first TerminalNode corresponding to token SET
    /// Returns `None` if there is no child corresponding to token SET
    fn SET(&self) -> Option<Rc<TerminalNode<'input, CypherParserContextType>>>
    where
        Self: Sized,
    {
        self.get_token(Cypher_SET, 0)
    }
    fn oC_SetItem_all(&self) -> Vec<Rc<OC_SetItemContextAll<'input>>>
    where
        Self: Sized,
    {
        self.children_of_type()
    }
    fn oC_SetItem(&self, i: usize) -> Option<Rc<OC_SetItemContextAll<'input>>>
    where
        Self: Sized,
    {
        self.child_of_type(i)
    }
    /// Retrieves all `TerminalNode`s corresponding to token SP in current rule
    fn SP_all(&self) -> Vec<Rc<TerminalNode<'input, CypherParserContextType>>>
    where
        Self: Sized,
    {
        self.children_of_type()
    }
    /// Retrieves 'i's TerminalNode corresponding to token SP, starting from 0.
    /// Returns `None` if number of children corresponding to token SP is less or equal than `i`.
    fn SP(&self, i: usize) -> Option<Rc<TerminalNode<'input, CypherParserContextType>>>
    where
        Self: Sized,
    {
        self.get_token(Cypher_SP, i)
    }
}

impl<'input> OC_SetContextAttrs<'input> for OC_SetContext<'input> {}

impl<'input, I> CypherParser<'input, I>
where
    I: TokenStream<'input, TF = LocalTokenFactory<'input>> + TidAble<'input>,
{
    pub fn oC_Set(&mut self) -> Result<Rc<OC_SetContextAll<'input>>, ANTLRError> {
        let mut recog = self;
        let _parentctx = recog.ctx.take();
        let mut _localctx = OC_SetContextExt::new(_parentctx.clone(), recog.base.get_state());
        recog.base.enter_rule(_localctx.clone(), 30, RULE_oC_Set);
        let mut _localctx: Rc<OC_SetContextAll> = _localctx;
        let mut _la: i32 = -1;
        let result: Result<(), ANTLRError> = (|| {
            let mut _alt: i32;
            //recog.base.enter_outer_alt(_localctx.clone(), 1)?;
            recog.base.enter_outer_alt(None, 1)?;
            {
                recog.base.set_state(399);
                recog.base.match_token(Cypher_SET, &mut recog.err_handler)?;

                recog.base.set_state(401);
                recog.err_handler.sync(&mut recog.base)?;
                _la = recog.base.input.la(1);
                if _la == Cypher_SP {
                    {
                        recog.base.set_state(400);
                        recog.base.match_token(Cypher_SP, &mut recog.err_handler)?;
                    }
                }

                /*InvokeRule oC_SetItem*/
                recog.base.set_state(403);
                recog.oC_SetItem()?;

                recog.base.set_state(414);
                recog.err_handler.sync(&mut recog.base)?;
                _alt = recog.interpreter.adaptive_predict(40, &mut recog.base)?;
                while { _alt != 2 && _alt != INVALID_ALT } {
                    if _alt == 1 {
                        {
                            {
                                recog.base.set_state(405);
                                recog.err_handler.sync(&mut recog.base)?;
                                _la = recog.base.input.la(1);
                                if _la == Cypher_SP {
                                    {
                                        recog.base.set_state(404);
                                        recog
                                            .base
                                            .match_token(Cypher_SP, &mut recog.err_handler)?;
                                    }
                                }

                                recog.base.set_state(407);
                                recog
                                    .base
                                    .match_token(Cypher_T__1, &mut recog.err_handler)?;

                                recog.base.set_state(409);
                                recog.err_handler.sync(&mut recog.base)?;
                                _la = recog.base.input.la(1);
                                if _la == Cypher_SP {
                                    {
                                        recog.base.set_state(408);
                                        recog
                                            .base
                                            .match_token(Cypher_SP, &mut recog.err_handler)?;
                                    }
                                }

                                /*InvokeRule oC_SetItem*/
                                recog.base.set_state(411);
                                recog.oC_SetItem()?;
                            }
                        }
                    }
                    recog.base.set_state(416);
                    recog.err_handler.sync(&mut recog.base)?;
                    _alt = recog.interpreter.adaptive_predict(40, &mut recog.base)?;
                }
            }
            Ok(())
        })();
        match result {
            Ok(_) => {}
            Err(e @ ANTLRError::FallThrough(_)) => return Err(e),
            Err(ref re) => {
                //_localctx.exception = re;
                recog.err_handler.report_error(&mut recog.base, re);
                recog.err_handler.recover(&mut recog.base, re)?;
            }
        }
        recog.base.exit_rule()?;

        Ok(_localctx)
    }
}
//------------------- oC_SetItem ----------------
pub type OC_SetItemContextAll<'input> = OC_SetItemContext<'input>;

pub type OC_SetItemContext<'input> = BaseParserRuleContext<'input, OC_SetItemContextExt<'input>>;

#[derive(Clone)]
pub struct OC_SetItemContextExt<'input> {
    ph: PhantomData<&'input str>,
}

impl<'input> CypherParserContext<'input> for OC_SetItemContext<'input> {}

impl<'input, 'a> Listenable<dyn CypherListener<'input> + 'a> for OC_SetItemContext<'input> {
    fn enter(&self, listener: &mut (dyn CypherListener<'input> + 'a)) -> Result<(), ANTLRError> {
        listener.enter_every_rule(self)?;
        listener.enter_oC_SetItem(self);
        Ok(())
    }
    fn exit(&self, listener: &mut (dyn CypherListener<'input> + 'a)) -> Result<(), ANTLRError> {
        listener.exit_oC_SetItem(self);
        listener.exit_every_rule(self)?;
        Ok(())
    }
}

impl<'input, 'a> Visitable<dyn CypherVisitor<'input> + 'a> for OC_SetItemContext<'input> {
    fn accept(&self, visitor: &mut (dyn CypherVisitor<'input> + 'a)) {
        visitor.visit_oC_SetItem(self);
    }
}

impl<'input> CustomRuleContext<'input> for OC_SetItemContextExt<'input> {
    type TF = LocalTokenFactory<'input>;
    type Ctx = CypherParserContextType;
    fn get_rule_index(&self) -> usize {
        RULE_oC_SetItem
    }
    //fn type_rule_index() -> usize where Self: Sized { RULE_oC_SetItem }
}
antlr4rust::tid! {OC_SetItemContextExt<'a>}

impl<'input> OC_SetItemContextExt<'input> {
    fn new(
        parent: Option<Rc<dyn CypherParserContext<'input> + 'input>>,
        invoking_state: i32,
    ) -> Rc<OC_SetItemContextAll<'input>> {
        Rc::new(BaseParserRuleContext::new_parser_ctx(
            parent,
            invoking_state,
            OC_SetItemContextExt { ph: PhantomData },
        ))
    }
}

pub trait OC_SetItemContextAttrs<'input>:
    CypherParserContext<'input> + BorrowMut<OC_SetItemContextExt<'input>>
{
    fn oC_PropertyExpression(&self) -> Option<Rc<OC_PropertyExpressionContextAll<'input>>>
    where
        Self: Sized,
    {
        self.child_of_type(0)
    }
    fn oC_Expression(&self) -> Option<Rc<OC_ExpressionContextAll<'input>>>
    where
        Self: Sized,
    {
        self.child_of_type(0)
    }
    /// Retrieves all `TerminalNode`s corresponding to token SP in current rule
    fn SP_all(&self) -> Vec<Rc<TerminalNode<'input, CypherParserContextType>>>
    where
        Self: Sized,
    {
        self.children_of_type()
    }
    /// Retrieves 'i's TerminalNode corresponding to token SP, starting from 0.
    /// Returns `None` if number of children corresponding to token SP is less or equal than `i`.
    fn SP(&self, i: usize) -> Option<Rc<TerminalNode<'input, CypherParserContextType>>>
    where
        Self: Sized,
    {
        self.get_token(Cypher_SP, i)
    }
    fn oC_Variable(&self) -> Option<Rc<OC_VariableContextAll<'input>>>
    where
        Self: Sized,
    {
        self.child_of_type(0)
    }
    fn oC_NodeLabels(&self) -> Option<Rc<OC_NodeLabelsContextAll<'input>>>
    where
        Self: Sized,
    {
        self.child_of_type(0)
    }
}

impl<'input> OC_SetItemContextAttrs<'input> for OC_SetItemContext<'input> {}

impl<'input, I> CypherParser<'input, I>
where
    I: TokenStream<'input, TF = LocalTokenFactory<'input>> + TidAble<'input>,
{
    pub fn oC_SetItem(&mut self) -> Result<Rc<OC_SetItemContextAll<'input>>, ANTLRError> {
        let mut recog = self;
        let _parentctx = recog.ctx.take();
        let mut _localctx = OC_SetItemContextExt::new(_parentctx.clone(), recog.base.get_state());
        recog
            .base
            .enter_rule(_localctx.clone(), 32, RULE_oC_SetItem);
        let mut _localctx: Rc<OC_SetItemContextAll> = _localctx;
        let mut _la: i32 = -1;
        let result: Result<(), ANTLRError> = (|| {
            recog.base.set_state(453);
            recog.err_handler.sync(&mut recog.base)?;
            match recog.interpreter.adaptive_predict(48, &mut recog.base)? {
                1 => {
                    //recog.base.enter_outer_alt(_localctx.clone(), 1)?;
                    recog.base.enter_outer_alt(None, 1)?;
                    {
                        {
                            /*InvokeRule oC_PropertyExpression*/
                            recog.base.set_state(417);
                            recog.oC_PropertyExpression()?;

                            recog.base.set_state(419);
                            recog.err_handler.sync(&mut recog.base)?;
                            _la = recog.base.input.la(1);
                            if _la == Cypher_SP {
                                {
                                    recog.base.set_state(418);
                                    recog.base.match_token(Cypher_SP, &mut recog.err_handler)?;
                                }
                            }

                            recog.base.set_state(421);
                            recog
                                .base
                                .match_token(Cypher_T__2, &mut recog.err_handler)?;

                            recog.base.set_state(423);
                            recog.err_handler.sync(&mut recog.base)?;
                            _la = recog.base.input.la(1);
                            if _la == Cypher_SP {
                                {
                                    recog.base.set_state(422);
                                    recog.base.match_token(Cypher_SP, &mut recog.err_handler)?;
                                }
                            }

                            /*InvokeRule oC_Expression*/
                            recog.base.set_state(425);
                            recog.oC_Expression()?;
                        }
                    }
                }
                2 => {
                    //recog.base.enter_outer_alt(_localctx.clone(), 2)?;
                    recog.base.enter_outer_alt(None, 2)?;
                    {
                        {
                            /*InvokeRule oC_Variable*/
                            recog.base.set_state(427);
                            recog.oC_Variable()?;

                            recog.base.set_state(429);
                            recog.err_handler.sync(&mut recog.base)?;
                            _la = recog.base.input.la(1);
                            if _la == Cypher_SP {
                                {
                                    recog.base.set_state(428);
                                    recog.base.match_token(Cypher_SP, &mut recog.err_handler)?;
                                }
                            }

                            recog.base.set_state(431);
                            recog
                                .base
                                .match_token(Cypher_T__2, &mut recog.err_handler)?;

                            recog.base.set_state(433);
                            recog.err_handler.sync(&mut recog.base)?;
                            _la = recog.base.input.la(1);
                            if _la == Cypher_SP {
                                {
                                    recog.base.set_state(432);
                                    recog.base.match_token(Cypher_SP, &mut recog.err_handler)?;
                                }
                            }

                            /*InvokeRule oC_Expression*/
                            recog.base.set_state(435);
                            recog.oC_Expression()?;
                        }
                    }
                }
                3 => {
                    //recog.base.enter_outer_alt(_localctx.clone(), 3)?;
                    recog.base.enter_outer_alt(None, 3)?;
                    {
                        {
                            /*InvokeRule oC_Variable*/
                            recog.base.set_state(437);
                            recog.oC_Variable()?;

                            recog.base.set_state(439);
                            recog.err_handler.sync(&mut recog.base)?;
                            _la = recog.base.input.la(1);
                            if _la == Cypher_SP {
                                {
                                    recog.base.set_state(438);
                                    recog.base.match_token(Cypher_SP, &mut recog.err_handler)?;
                                }
                            }

                            recog.base.set_state(441);
                            recog
                                .base
                                .match_token(Cypher_T__3, &mut recog.err_handler)?;

                            recog.base.set_state(443);
                            recog.err_handler.sync(&mut recog.base)?;
                            _la = recog.base.input.la(1);
                            if _la == Cypher_SP {
                                {
                                    recog.base.set_state(442);
                                    recog.base.match_token(Cypher_SP, &mut recog.err_handler)?;
                                }
                            }

                            /*InvokeRule oC_Expression*/
                            recog.base.set_state(445);
                            recog.oC_Expression()?;
                        }
                    }
                }
                4 => {
                    //recog.base.enter_outer_alt(_localctx.clone(), 4)?;
                    recog.base.enter_outer_alt(None, 4)?;
                    {
                        {
                            /*InvokeRule oC_Variable*/
                            recog.base.set_state(447);
                            recog.oC_Variable()?;

                            recog.base.set_state(449);
                            recog.err_handler.sync(&mut recog.base)?;
                            _la = recog.base.input.la(1);
                            if _la == Cypher_SP {
                                {
                                    recog.base.set_state(448);
                                    recog.base.match_token(Cypher_SP, &mut recog.err_handler)?;
                                }
                            }

                            /*InvokeRule oC_NodeLabels*/
                            recog.base.set_state(451);
                            recog.oC_NodeLabels()?;
                        }
                    }
                }

                _ => {}
            }
            Ok(())
        })();
        match result {
            Ok(_) => {}
            Err(e @ ANTLRError::FallThrough(_)) => return Err(e),
            Err(ref re) => {
                //_localctx.exception = re;
                recog.err_handler.report_error(&mut recog.base, re);
                recog.err_handler.recover(&mut recog.base, re)?;
            }
        }
        recog.base.exit_rule()?;

        Ok(_localctx)
    }
}
//------------------- oC_Delete ----------------
pub type OC_DeleteContextAll<'input> = OC_DeleteContext<'input>;

pub type OC_DeleteContext<'input> = BaseParserRuleContext<'input, OC_DeleteContextExt<'input>>;

#[derive(Clone)]
pub struct OC_DeleteContextExt<'input> {
    ph: PhantomData<&'input str>,
}

impl<'input> CypherParserContext<'input> for OC_DeleteContext<'input> {}

impl<'input, 'a> Listenable<dyn CypherListener<'input> + 'a> for OC_DeleteContext<'input> {
    fn enter(&self, listener: &mut (dyn CypherListener<'input> + 'a)) -> Result<(), ANTLRError> {
        listener.enter_every_rule(self)?;
        listener.enter_oC_Delete(self);
        Ok(())
    }
    fn exit(&self, listener: &mut (dyn CypherListener<'input> + 'a)) -> Result<(), ANTLRError> {
        listener.exit_oC_Delete(self);
        listener.exit_every_rule(self)?;
        Ok(())
    }
}

impl<'input, 'a> Visitable<dyn CypherVisitor<'input> + 'a> for OC_DeleteContext<'input> {
    fn accept(&self, visitor: &mut (dyn CypherVisitor<'input> + 'a)) {
        visitor.visit_oC_Delete(self);
    }
}

impl<'input> CustomRuleContext<'input> for OC_DeleteContextExt<'input> {
    type TF = LocalTokenFactory<'input>;
    type Ctx = CypherParserContextType;
    fn get_rule_index(&self) -> usize {
        RULE_oC_Delete
    }
    //fn type_rule_index() -> usize where Self: Sized { RULE_oC_Delete }
}
antlr4rust::tid! {OC_DeleteContextExt<'a>}

impl<'input> OC_DeleteContextExt<'input> {
    fn new(
        parent: Option<Rc<dyn CypherParserContext<'input> + 'input>>,
        invoking_state: i32,
    ) -> Rc<OC_DeleteContextAll<'input>> {
        Rc::new(BaseParserRuleContext::new_parser_ctx(
            parent,
            invoking_state,
            OC_DeleteContextExt { ph: PhantomData },
        ))
    }
}

pub trait OC_DeleteContextAttrs<'input>:
    CypherParserContext<'input> + BorrowMut<OC_DeleteContextExt<'input>>
{
    /// Retrieves first TerminalNode corresponding to token DELETE
    /// Returns `None` if there is no child corresponding to token DELETE
    fn DELETE(&self) -> Option<Rc<TerminalNode<'input, CypherParserContextType>>>
    where
        Self: Sized,
    {
        self.get_token(Cypher_DELETE, 0)
    }
    fn oC_Expression_all(&self) -> Vec<Rc<OC_ExpressionContextAll<'input>>>
    where
        Self: Sized,
    {
        self.children_of_type()
    }
    fn oC_Expression(&self, i: usize) -> Option<Rc<OC_ExpressionContextAll<'input>>>
    where
        Self: Sized,
    {
        self.child_of_type(i)
    }
    /// Retrieves first TerminalNode corresponding to token DETACH
    /// Returns `None` if there is no child corresponding to token DETACH
    fn DETACH(&self) -> Option<Rc<TerminalNode<'input, CypherParserContextType>>>
    where
        Self: Sized,
    {
        self.get_token(Cypher_DETACH, 0)
    }
    /// Retrieves all `TerminalNode`s corresponding to token SP in current rule
    fn SP_all(&self) -> Vec<Rc<TerminalNode<'input, CypherParserContextType>>>
    where
        Self: Sized,
    {
        self.children_of_type()
    }
    /// Retrieves 'i's TerminalNode corresponding to token SP, starting from 0.
    /// Returns `None` if number of children corresponding to token SP is less or equal than `i`.
    fn SP(&self, i: usize) -> Option<Rc<TerminalNode<'input, CypherParserContextType>>>
    where
        Self: Sized,
    {
        self.get_token(Cypher_SP, i)
    }
}

impl<'input> OC_DeleteContextAttrs<'input> for OC_DeleteContext<'input> {}

impl<'input, I> CypherParser<'input, I>
where
    I: TokenStream<'input, TF = LocalTokenFactory<'input>> + TidAble<'input>,
{
    pub fn oC_Delete(&mut self) -> Result<Rc<OC_DeleteContextAll<'input>>, ANTLRError> {
        let mut recog = self;
        let _parentctx = recog.ctx.take();
        let mut _localctx = OC_DeleteContextExt::new(_parentctx.clone(), recog.base.get_state());
        recog.base.enter_rule(_localctx.clone(), 34, RULE_oC_Delete);
        let mut _localctx: Rc<OC_DeleteContextAll> = _localctx;
        let mut _la: i32 = -1;
        let result: Result<(), ANTLRError> = (|| {
            let mut _alt: i32;
            //recog.base.enter_outer_alt(_localctx.clone(), 1)?;
            recog.base.enter_outer_alt(None, 1)?;
            {
                recog.base.set_state(457);
                recog.err_handler.sync(&mut recog.base)?;
                _la = recog.base.input.la(1);
                if _la == Cypher_DETACH {
                    {
                        recog.base.set_state(455);
                        recog
                            .base
                            .match_token(Cypher_DETACH, &mut recog.err_handler)?;

                        recog.base.set_state(456);
                        recog.base.match_token(Cypher_SP, &mut recog.err_handler)?;
                    }
                }

                recog.base.set_state(459);
                recog
                    .base
                    .match_token(Cypher_DELETE, &mut recog.err_handler)?;

                recog.base.set_state(461);
                recog.err_handler.sync(&mut recog.base)?;
                _la = recog.base.input.la(1);
                if _la == Cypher_SP {
                    {
                        recog.base.set_state(460);
                        recog.base.match_token(Cypher_SP, &mut recog.err_handler)?;
                    }
                }

                /*InvokeRule oC_Expression*/
                recog.base.set_state(463);
                recog.oC_Expression()?;

                recog.base.set_state(474);
                recog.err_handler.sync(&mut recog.base)?;
                _alt = recog.interpreter.adaptive_predict(53, &mut recog.base)?;
                while { _alt != 2 && _alt != INVALID_ALT } {
                    if _alt == 1 {
                        {
                            {
                                recog.base.set_state(465);
                                recog.err_handler.sync(&mut recog.base)?;
                                _la = recog.base.input.la(1);
                                if _la == Cypher_SP {
                                    {
                                        recog.base.set_state(464);
                                        recog
                                            .base
                                            .match_token(Cypher_SP, &mut recog.err_handler)?;
                                    }
                                }

                                recog.base.set_state(467);
                                recog
                                    .base
                                    .match_token(Cypher_T__1, &mut recog.err_handler)?;

                                recog.base.set_state(469);
                                recog.err_handler.sync(&mut recog.base)?;
                                _la = recog.base.input.la(1);
                                if _la == Cypher_SP {
                                    {
                                        recog.base.set_state(468);
                                        recog
                                            .base
                                            .match_token(Cypher_SP, &mut recog.err_handler)?;
                                    }
                                }

                                /*InvokeRule oC_Expression*/
                                recog.base.set_state(471);
                                recog.oC_Expression()?;
                            }
                        }
                    }
                    recog.base.set_state(476);
                    recog.err_handler.sync(&mut recog.base)?;
                    _alt = recog.interpreter.adaptive_predict(53, &mut recog.base)?;
                }
            }
            Ok(())
        })();
        match result {
            Ok(_) => {}
            Err(e @ ANTLRError::FallThrough(_)) => return Err(e),
            Err(ref re) => {
                //_localctx.exception = re;
                recog.err_handler.report_error(&mut recog.base, re);
                recog.err_handler.recover(&mut recog.base, re)?;
            }
        }
        recog.base.exit_rule()?;

        Ok(_localctx)
    }
}
//------------------- oC_Remove ----------------
pub type OC_RemoveContextAll<'input> = OC_RemoveContext<'input>;

pub type OC_RemoveContext<'input> = BaseParserRuleContext<'input, OC_RemoveContextExt<'input>>;

#[derive(Clone)]
pub struct OC_RemoveContextExt<'input> {
    ph: PhantomData<&'input str>,
}

impl<'input> CypherParserContext<'input> for OC_RemoveContext<'input> {}

impl<'input, 'a> Listenable<dyn CypherListener<'input> + 'a> for OC_RemoveContext<'input> {
    fn enter(&self, listener: &mut (dyn CypherListener<'input> + 'a)) -> Result<(), ANTLRError> {
        listener.enter_every_rule(self)?;
        listener.enter_oC_Remove(self);
        Ok(())
    }
    fn exit(&self, listener: &mut (dyn CypherListener<'input> + 'a)) -> Result<(), ANTLRError> {
        listener.exit_oC_Remove(self);
        listener.exit_every_rule(self)?;
        Ok(())
    }
}

impl<'input, 'a> Visitable<dyn CypherVisitor<'input> + 'a> for OC_RemoveContext<'input> {
    fn accept(&self, visitor: &mut (dyn CypherVisitor<'input> + 'a)) {
        visitor.visit_oC_Remove(self);
    }
}

impl<'input> CustomRuleContext<'input> for OC_RemoveContextExt<'input> {
    type TF = LocalTokenFactory<'input>;
    type Ctx = CypherParserContextType;
    fn get_rule_index(&self) -> usize {
        RULE_oC_Remove
    }
    //fn type_rule_index() -> usize where Self: Sized { RULE_oC_Remove }
}
antlr4rust::tid! {OC_RemoveContextExt<'a>}

impl<'input> OC_RemoveContextExt<'input> {
    fn new(
        parent: Option<Rc<dyn CypherParserContext<'input> + 'input>>,
        invoking_state: i32,
    ) -> Rc<OC_RemoveContextAll<'input>> {
        Rc::new(BaseParserRuleContext::new_parser_ctx(
            parent,
            invoking_state,
            OC_RemoveContextExt { ph: PhantomData },
        ))
    }
}

pub trait OC_RemoveContextAttrs<'input>:
    CypherParserContext<'input> + BorrowMut<OC_RemoveContextExt<'input>>
{
    /// Retrieves first TerminalNode corresponding to token REMOVE
    /// Returns `None` if there is no child corresponding to token REMOVE
    fn REMOVE(&self) -> Option<Rc<TerminalNode<'input, CypherParserContextType>>>
    where
        Self: Sized,
    {
        self.get_token(Cypher_REMOVE, 0)
    }
    /// Retrieves all `TerminalNode`s corresponding to token SP in current rule
    fn SP_all(&self) -> Vec<Rc<TerminalNode<'input, CypherParserContextType>>>
    where
        Self: Sized,
    {
        self.children_of_type()
    }
    /// Retrieves 'i's TerminalNode corresponding to token SP, starting from 0.
    /// Returns `None` if number of children corresponding to token SP is less or equal than `i`.
    fn SP(&self, i: usize) -> Option<Rc<TerminalNode<'input, CypherParserContextType>>>
    where
        Self: Sized,
    {
        self.get_token(Cypher_SP, i)
    }
    fn oC_RemoveItem_all(&self) -> Vec<Rc<OC_RemoveItemContextAll<'input>>>
    where
        Self: Sized,
    {
        self.children_of_type()
    }
    fn oC_RemoveItem(&self, i: usize) -> Option<Rc<OC_RemoveItemContextAll<'input>>>
    where
        Self: Sized,
    {
        self.child_of_type(i)
    }
}

impl<'input> OC_RemoveContextAttrs<'input> for OC_RemoveContext<'input> {}

impl<'input, I> CypherParser<'input, I>
where
    I: TokenStream<'input, TF = LocalTokenFactory<'input>> + TidAble<'input>,
{
    pub fn oC_Remove(&mut self) -> Result<Rc<OC_RemoveContextAll<'input>>, ANTLRError> {
        let mut recog = self;
        let _parentctx = recog.ctx.take();
        let mut _localctx = OC_RemoveContextExt::new(_parentctx.clone(), recog.base.get_state());
        recog.base.enter_rule(_localctx.clone(), 36, RULE_oC_Remove);
        let mut _localctx: Rc<OC_RemoveContextAll> = _localctx;
        let mut _la: i32 = -1;
        let result: Result<(), ANTLRError> = (|| {
            let mut _alt: i32;
            //recog.base.enter_outer_alt(_localctx.clone(), 1)?;
            recog.base.enter_outer_alt(None, 1)?;
            {
                recog.base.set_state(477);
                recog
                    .base
                    .match_token(Cypher_REMOVE, &mut recog.err_handler)?;

                recog.base.set_state(478);
                recog.base.match_token(Cypher_SP, &mut recog.err_handler)?;

                /*InvokeRule oC_RemoveItem*/
                recog.base.set_state(479);
                recog.oC_RemoveItem()?;

                recog.base.set_state(490);
                recog.err_handler.sync(&mut recog.base)?;
                _alt = recog.interpreter.adaptive_predict(56, &mut recog.base)?;
                while { _alt != 2 && _alt != INVALID_ALT } {
                    if _alt == 1 {
                        {
                            {
                                recog.base.set_state(481);
                                recog.err_handler.sync(&mut recog.base)?;
                                _la = recog.base.input.la(1);
                                if _la == Cypher_SP {
                                    {
                                        recog.base.set_state(480);
                                        recog
                                            .base
                                            .match_token(Cypher_SP, &mut recog.err_handler)?;
                                    }
                                }

                                recog.base.set_state(483);
                                recog
                                    .base
                                    .match_token(Cypher_T__1, &mut recog.err_handler)?;

                                recog.base.set_state(485);
                                recog.err_handler.sync(&mut recog.base)?;
                                _la = recog.base.input.la(1);
                                if _la == Cypher_SP {
                                    {
                                        recog.base.set_state(484);
                                        recog
                                            .base
                                            .match_token(Cypher_SP, &mut recog.err_handler)?;
                                    }
                                }

                                /*InvokeRule oC_RemoveItem*/
                                recog.base.set_state(487);
                                recog.oC_RemoveItem()?;
                            }
                        }
                    }
                    recog.base.set_state(492);
                    recog.err_handler.sync(&mut recog.base)?;
                    _alt = recog.interpreter.adaptive_predict(56, &mut recog.base)?;
                }
            }
            Ok(())
        })();
        match result {
            Ok(_) => {}
            Err(e @ ANTLRError::FallThrough(_)) => return Err(e),
            Err(ref re) => {
                //_localctx.exception = re;
                recog.err_handler.report_error(&mut recog.base, re);
                recog.err_handler.recover(&mut recog.base, re)?;
            }
        }
        recog.base.exit_rule()?;

        Ok(_localctx)
    }
}
//------------------- oC_RemoveItem ----------------
pub type OC_RemoveItemContextAll<'input> = OC_RemoveItemContext<'input>;

pub type OC_RemoveItemContext<'input> =
    BaseParserRuleContext<'input, OC_RemoveItemContextExt<'input>>;

#[derive(Clone)]
pub struct OC_RemoveItemContextExt<'input> {
    ph: PhantomData<&'input str>,
}

impl<'input> CypherParserContext<'input> for OC_RemoveItemContext<'input> {}

impl<'input, 'a> Listenable<dyn CypherListener<'input> + 'a> for OC_RemoveItemContext<'input> {
    fn enter(&self, listener: &mut (dyn CypherListener<'input> + 'a)) -> Result<(), ANTLRError> {
        listener.enter_every_rule(self)?;
        listener.enter_oC_RemoveItem(self);
        Ok(())
    }
    fn exit(&self, listener: &mut (dyn CypherListener<'input> + 'a)) -> Result<(), ANTLRError> {
        listener.exit_oC_RemoveItem(self);
        listener.exit_every_rule(self)?;
        Ok(())
    }
}

impl<'input, 'a> Visitable<dyn CypherVisitor<'input> + 'a> for OC_RemoveItemContext<'input> {
    fn accept(&self, visitor: &mut (dyn CypherVisitor<'input> + 'a)) {
        visitor.visit_oC_RemoveItem(self);
    }
}

impl<'input> CustomRuleContext<'input> for OC_RemoveItemContextExt<'input> {
    type TF = LocalTokenFactory<'input>;
    type Ctx = CypherParserContextType;
    fn get_rule_index(&self) -> usize {
        RULE_oC_RemoveItem
    }
    //fn type_rule_index() -> usize where Self: Sized { RULE_oC_RemoveItem }
}
antlr4rust::tid! {OC_RemoveItemContextExt<'a>}

impl<'input> OC_RemoveItemContextExt<'input> {
    fn new(
        parent: Option<Rc<dyn CypherParserContext<'input> + 'input>>,
        invoking_state: i32,
    ) -> Rc<OC_RemoveItemContextAll<'input>> {
        Rc::new(BaseParserRuleContext::new_parser_ctx(
            parent,
            invoking_state,
            OC_RemoveItemContextExt { ph: PhantomData },
        ))
    }
}

pub trait OC_RemoveItemContextAttrs<'input>:
    CypherParserContext<'input> + BorrowMut<OC_RemoveItemContextExt<'input>>
{
    fn oC_Variable(&self) -> Option<Rc<OC_VariableContextAll<'input>>>
    where
        Self: Sized,
    {
        self.child_of_type(0)
    }
    fn oC_NodeLabels(&self) -> Option<Rc<OC_NodeLabelsContextAll<'input>>>
    where
        Self: Sized,
    {
        self.child_of_type(0)
    }
    fn oC_PropertyExpression(&self) -> Option<Rc<OC_PropertyExpressionContextAll<'input>>>
    where
        Self: Sized,
    {
        self.child_of_type(0)
    }
}

impl<'input> OC_RemoveItemContextAttrs<'input> for OC_RemoveItemContext<'input> {}

impl<'input, I> CypherParser<'input, I>
where
    I: TokenStream<'input, TF = LocalTokenFactory<'input>> + TidAble<'input>,
{
    pub fn oC_RemoveItem(&mut self) -> Result<Rc<OC_RemoveItemContextAll<'input>>, ANTLRError> {
        let mut recog = self;
        let _parentctx = recog.ctx.take();
        let mut _localctx =
            OC_RemoveItemContextExt::new(_parentctx.clone(), recog.base.get_state());
        recog
            .base
            .enter_rule(_localctx.clone(), 38, RULE_oC_RemoveItem);
        let mut _localctx: Rc<OC_RemoveItemContextAll> = _localctx;
        let result: Result<(), ANTLRError> = (|| {
            recog.base.set_state(497);
            recog.err_handler.sync(&mut recog.base)?;
            match recog.interpreter.adaptive_predict(57, &mut recog.base)? {
                1 => {
                    //recog.base.enter_outer_alt(_localctx.clone(), 1)?;
                    recog.base.enter_outer_alt(None, 1)?;
                    {
                        {
                            /*InvokeRule oC_Variable*/
                            recog.base.set_state(493);
                            recog.oC_Variable()?;

                            /*InvokeRule oC_NodeLabels*/
                            recog.base.set_state(494);
                            recog.oC_NodeLabels()?;
                        }
                    }
                }
                2 => {
                    //recog.base.enter_outer_alt(_localctx.clone(), 2)?;
                    recog.base.enter_outer_alt(None, 2)?;
                    {
                        /*InvokeRule oC_PropertyExpression*/
                        recog.base.set_state(496);
                        recog.oC_PropertyExpression()?;
                    }
                }

                _ => {}
            }
            Ok(())
        })();
        match result {
            Ok(_) => {}
            Err(e @ ANTLRError::FallThrough(_)) => return Err(e),
            Err(ref re) => {
                //_localctx.exception = re;
                recog.err_handler.report_error(&mut recog.base, re);
                recog.err_handler.recover(&mut recog.base, re)?;
            }
        }
        recog.base.exit_rule()?;

        Ok(_localctx)
    }
}
//------------------- oC_InQueryCall ----------------
pub type OC_InQueryCallContextAll<'input> = OC_InQueryCallContext<'input>;

pub type OC_InQueryCallContext<'input> =
    BaseParserRuleContext<'input, OC_InQueryCallContextExt<'input>>;

#[derive(Clone)]
pub struct OC_InQueryCallContextExt<'input> {
    ph: PhantomData<&'input str>,
}

impl<'input> CypherParserContext<'input> for OC_InQueryCallContext<'input> {}

impl<'input, 'a> Listenable<dyn CypherListener<'input> + 'a> for OC_InQueryCallContext<'input> {
    fn enter(&self, listener: &mut (dyn CypherListener<'input> + 'a)) -> Result<(), ANTLRError> {
        listener.enter_every_rule(self)?;
        listener.enter_oC_InQueryCall(self);
        Ok(())
    }
    fn exit(&self, listener: &mut (dyn CypherListener<'input> + 'a)) -> Result<(), ANTLRError> {
        listener.exit_oC_InQueryCall(self);
        listener.exit_every_rule(self)?;
        Ok(())
    }
}

impl<'input, 'a> Visitable<dyn CypherVisitor<'input> + 'a> for OC_InQueryCallContext<'input> {
    fn accept(&self, visitor: &mut (dyn CypherVisitor<'input> + 'a)) {
        visitor.visit_oC_InQueryCall(self);
    }
}

impl<'input> CustomRuleContext<'input> for OC_InQueryCallContextExt<'input> {
    type TF = LocalTokenFactory<'input>;
    type Ctx = CypherParserContextType;
    fn get_rule_index(&self) -> usize {
        RULE_oC_InQueryCall
    }
    //fn type_rule_index() -> usize where Self: Sized { RULE_oC_InQueryCall }
}
antlr4rust::tid! {OC_InQueryCallContextExt<'a>}

impl<'input> OC_InQueryCallContextExt<'input> {
    fn new(
        parent: Option<Rc<dyn CypherParserContext<'input> + 'input>>,
        invoking_state: i32,
    ) -> Rc<OC_InQueryCallContextAll<'input>> {
        Rc::new(BaseParserRuleContext::new_parser_ctx(
            parent,
            invoking_state,
            OC_InQueryCallContextExt { ph: PhantomData },
        ))
    }
}

pub trait OC_InQueryCallContextAttrs<'input>:
    CypherParserContext<'input> + BorrowMut<OC_InQueryCallContextExt<'input>>
{
    /// Retrieves first TerminalNode corresponding to token CALL
    /// Returns `None` if there is no child corresponding to token CALL
    fn CALL(&self) -> Option<Rc<TerminalNode<'input, CypherParserContextType>>>
    where
        Self: Sized,
    {
        self.get_token(Cypher_CALL, 0)
    }
    /// Retrieves all `TerminalNode`s corresponding to token SP in current rule
    fn SP_all(&self) -> Vec<Rc<TerminalNode<'input, CypherParserContextType>>>
    where
        Self: Sized,
    {
        self.children_of_type()
    }
    /// Retrieves 'i's TerminalNode corresponding to token SP, starting from 0.
    /// Returns `None` if number of children corresponding to token SP is less or equal than `i`.
    fn SP(&self, i: usize) -> Option<Rc<TerminalNode<'input, CypherParserContextType>>>
    where
        Self: Sized,
    {
        self.get_token(Cypher_SP, i)
    }
    fn oC_ExplicitProcedureInvocation(
        &self,
    ) -> Option<Rc<OC_ExplicitProcedureInvocationContextAll<'input>>>
    where
        Self: Sized,
    {
        self.child_of_type(0)
    }
    /// Retrieves first TerminalNode corresponding to token YIELD
    /// Returns `None` if there is no child corresponding to token YIELD
    fn YIELD(&self) -> Option<Rc<TerminalNode<'input, CypherParserContextType>>>
    where
        Self: Sized,
    {
        self.get_token(Cypher_YIELD, 0)
    }
    fn oC_YieldItems(&self) -> Option<Rc<OC_YieldItemsContextAll<'input>>>
    where
        Self: Sized,
    {
        self.child_of_type(0)
    }
}

impl<'input> OC_InQueryCallContextAttrs<'input> for OC_InQueryCallContext<'input> {}

impl<'input, I> CypherParser<'input, I>
where
    I: TokenStream<'input, TF = LocalTokenFactory<'input>> + TidAble<'input>,
{
    pub fn oC_InQueryCall(&mut self) -> Result<Rc<OC_InQueryCallContextAll<'input>>, ANTLRError> {
        let mut recog = self;
        let _parentctx = recog.ctx.take();
        let mut _localctx =
            OC_InQueryCallContextExt::new(_parentctx.clone(), recog.base.get_state());
        recog
            .base
            .enter_rule(_localctx.clone(), 40, RULE_oC_InQueryCall);
        let mut _localctx: Rc<OC_InQueryCallContextAll> = _localctx;
        let mut _la: i32 = -1;
        let result: Result<(), ANTLRError> = (|| {
            //recog.base.enter_outer_alt(_localctx.clone(), 1)?;
            recog.base.enter_outer_alt(None, 1)?;
            {
                recog.base.set_state(499);
                recog
                    .base
                    .match_token(Cypher_CALL, &mut recog.err_handler)?;

                recog.base.set_state(500);
                recog.base.match_token(Cypher_SP, &mut recog.err_handler)?;

                /*InvokeRule oC_ExplicitProcedureInvocation*/
                recog.base.set_state(501);
                recog.oC_ExplicitProcedureInvocation()?;

                recog.base.set_state(508);
                recog.err_handler.sync(&mut recog.base)?;
                match recog.interpreter.adaptive_predict(59, &mut recog.base)? {
                    x if x == 1 => {
                        {
                            recog.base.set_state(503);
                            recog.err_handler.sync(&mut recog.base)?;
                            _la = recog.base.input.la(1);
                            if _la == Cypher_SP {
                                {
                                    recog.base.set_state(502);
                                    recog.base.match_token(Cypher_SP, &mut recog.err_handler)?;
                                }
                            }

                            recog.base.set_state(505);
                            recog
                                .base
                                .match_token(Cypher_YIELD, &mut recog.err_handler)?;

                            recog.base.set_state(506);
                            recog.base.match_token(Cypher_SP, &mut recog.err_handler)?;

                            /*InvokeRule oC_YieldItems*/
                            recog.base.set_state(507);
                            recog.oC_YieldItems()?;
                        }
                    }

                    _ => {}
                }
            }
            Ok(())
        })();
        match result {
            Ok(_) => {}
            Err(e @ ANTLRError::FallThrough(_)) => return Err(e),
            Err(ref re) => {
                //_localctx.exception = re;
                recog.err_handler.report_error(&mut recog.base, re);
                recog.err_handler.recover(&mut recog.base, re)?;
            }
        }
        recog.base.exit_rule()?;

        Ok(_localctx)
    }
}
//------------------- oC_StandaloneCall ----------------
pub type OC_StandaloneCallContextAll<'input> = OC_StandaloneCallContext<'input>;

pub type OC_StandaloneCallContext<'input> =
    BaseParserRuleContext<'input, OC_StandaloneCallContextExt<'input>>;

#[derive(Clone)]
pub struct OC_StandaloneCallContextExt<'input> {
    ph: PhantomData<&'input str>,
}

impl<'input> CypherParserContext<'input> for OC_StandaloneCallContext<'input> {}

impl<'input, 'a> Listenable<dyn CypherListener<'input> + 'a> for OC_StandaloneCallContext<'input> {
    fn enter(&self, listener: &mut (dyn CypherListener<'input> + 'a)) -> Result<(), ANTLRError> {
        listener.enter_every_rule(self)?;
        listener.enter_oC_StandaloneCall(self);
        Ok(())
    }
    fn exit(&self, listener: &mut (dyn CypherListener<'input> + 'a)) -> Result<(), ANTLRError> {
        listener.exit_oC_StandaloneCall(self);
        listener.exit_every_rule(self)?;
        Ok(())
    }
}

impl<'input, 'a> Visitable<dyn CypherVisitor<'input> + 'a> for OC_StandaloneCallContext<'input> {
    fn accept(&self, visitor: &mut (dyn CypherVisitor<'input> + 'a)) {
        visitor.visit_oC_StandaloneCall(self);
    }
}

impl<'input> CustomRuleContext<'input> for OC_StandaloneCallContextExt<'input> {
    type TF = LocalTokenFactory<'input>;
    type Ctx = CypherParserContextType;
    fn get_rule_index(&self) -> usize {
        RULE_oC_StandaloneCall
    }
    //fn type_rule_index() -> usize where Self: Sized { RULE_oC_StandaloneCall }
}
antlr4rust::tid! {OC_StandaloneCallContextExt<'a>}

impl<'input> OC_StandaloneCallContextExt<'input> {
    fn new(
        parent: Option<Rc<dyn CypherParserContext<'input> + 'input>>,
        invoking_state: i32,
    ) -> Rc<OC_StandaloneCallContextAll<'input>> {
        Rc::new(BaseParserRuleContext::new_parser_ctx(
            parent,
            invoking_state,
            OC_StandaloneCallContextExt { ph: PhantomData },
        ))
    }
}

pub trait OC_StandaloneCallContextAttrs<'input>:
    CypherParserContext<'input> + BorrowMut<OC_StandaloneCallContextExt<'input>>
{
    /// Retrieves first TerminalNode corresponding to token CALL
    /// Returns `None` if there is no child corresponding to token CALL
    fn CALL(&self) -> Option<Rc<TerminalNode<'input, CypherParserContextType>>>
    where
        Self: Sized,
    {
        self.get_token(Cypher_CALL, 0)
    }
    /// Retrieves all `TerminalNode`s corresponding to token SP in current rule
    fn SP_all(&self) -> Vec<Rc<TerminalNode<'input, CypherParserContextType>>>
    where
        Self: Sized,
    {
        self.children_of_type()
    }
    /// Retrieves 'i's TerminalNode corresponding to token SP, starting from 0.
    /// Returns `None` if number of children corresponding to token SP is less or equal than `i`.
    fn SP(&self, i: usize) -> Option<Rc<TerminalNode<'input, CypherParserContextType>>>
    where
        Self: Sized,
    {
        self.get_token(Cypher_SP, i)
    }
    fn oC_ExplicitProcedureInvocation(
        &self,
    ) -> Option<Rc<OC_ExplicitProcedureInvocationContextAll<'input>>>
    where
        Self: Sized,
    {
        self.child_of_type(0)
    }
    fn oC_ImplicitProcedureInvocation(
        &self,
    ) -> Option<Rc<OC_ImplicitProcedureInvocationContextAll<'input>>>
    where
        Self: Sized,
    {
        self.child_of_type(0)
    }
    /// Retrieves first TerminalNode corresponding to token YIELD
    /// Returns `None` if there is no child corresponding to token YIELD
    fn YIELD(&self) -> Option<Rc<TerminalNode<'input, CypherParserContextType>>>
    where
        Self: Sized,
    {
        self.get_token(Cypher_YIELD, 0)
    }
    fn oC_YieldItems(&self) -> Option<Rc<OC_YieldItemsContextAll<'input>>>
    where
        Self: Sized,
    {
        self.child_of_type(0)
    }
}

impl<'input> OC_StandaloneCallContextAttrs<'input> for OC_StandaloneCallContext<'input> {}

impl<'input, I> CypherParser<'input, I>
where
    I: TokenStream<'input, TF = LocalTokenFactory<'input>> + TidAble<'input>,
{
    pub fn oC_StandaloneCall(
        &mut self,
    ) -> Result<Rc<OC_StandaloneCallContextAll<'input>>, ANTLRError> {
        let mut recog = self;
        let _parentctx = recog.ctx.take();
        let mut _localctx =
            OC_StandaloneCallContextExt::new(_parentctx.clone(), recog.base.get_state());
        recog
            .base
            .enter_rule(_localctx.clone(), 42, RULE_oC_StandaloneCall);
        let mut _localctx: Rc<OC_StandaloneCallContextAll> = _localctx;
        let mut _la: i32 = -1;
        let result: Result<(), ANTLRError> = (|| {
            //recog.base.enter_outer_alt(_localctx.clone(), 1)?;
            recog.base.enter_outer_alt(None, 1)?;
            {
                recog.base.set_state(510);
                recog
                    .base
                    .match_token(Cypher_CALL, &mut recog.err_handler)?;

                recog.base.set_state(511);
                recog.base.match_token(Cypher_SP, &mut recog.err_handler)?;

                recog.base.set_state(514);
                recog.err_handler.sync(&mut recog.base)?;
                match recog.interpreter.adaptive_predict(60, &mut recog.base)? {
                    1 => {
                        {
                            /*InvokeRule oC_ExplicitProcedureInvocation*/
                            recog.base.set_state(512);
                            recog.oC_ExplicitProcedureInvocation()?;
                        }
                    }
                    2 => {
                        {
                            /*InvokeRule oC_ImplicitProcedureInvocation*/
                            recog.base.set_state(513);
                            recog.oC_ImplicitProcedureInvocation()?;
                        }
                    }

                    _ => {}
                }
                recog.base.set_state(525);
                recog.err_handler.sync(&mut recog.base)?;
                match recog.interpreter.adaptive_predict(63, &mut recog.base)? {
                    x if x == 1 => {
                        {
                            recog.base.set_state(517);
                            recog.err_handler.sync(&mut recog.base)?;
                            _la = recog.base.input.la(1);
                            if _la == Cypher_SP {
                                {
                                    recog.base.set_state(516);
                                    recog.base.match_token(Cypher_SP, &mut recog.err_handler)?;
                                }
                            }

                            recog.base.set_state(519);
                            recog
                                .base
                                .match_token(Cypher_YIELD, &mut recog.err_handler)?;

                            recog.base.set_state(520);
                            recog.base.match_token(Cypher_SP, &mut recog.err_handler)?;

                            recog.base.set_state(523);
                            recog.err_handler.sync(&mut recog.base)?;
                            match recog.base.input.la(1) {
                                Cypher_T__4 => {
                                    recog.base.set_state(521);
                                    recog
                                        .base
                                        .match_token(Cypher_T__4, &mut recog.err_handler)?;
                                }

                                Cypher_COUNT
                                | Cypher_ANY
                                | Cypher_NONE
                                | Cypher_SINGLE
                                | Cypher_HexLetter
                                | Cypher_FILTER
                                | Cypher_EXTRACT
                                | Cypher_UnescapedSymbolicName
                                | Cypher_EscapedSymbolicName => {
                                    {
                                        /*InvokeRule oC_YieldItems*/
                                        recog.base.set_state(522);
                                        recog.oC_YieldItems()?;
                                    }
                                }

                                _ => Err(ANTLRError::NoAltError(NoViableAltError::new(
                                    &mut recog.base,
                                )))?,
                            }
                        }
                    }

                    _ => {}
                }
            }
            Ok(())
        })();
        match result {
            Ok(_) => {}
            Err(e @ ANTLRError::FallThrough(_)) => return Err(e),
            Err(ref re) => {
                //_localctx.exception = re;
                recog.err_handler.report_error(&mut recog.base, re);
                recog.err_handler.recover(&mut recog.base, re)?;
            }
        }
        recog.base.exit_rule()?;

        Ok(_localctx)
    }
}
//------------------- oC_YieldItems ----------------
pub type OC_YieldItemsContextAll<'input> = OC_YieldItemsContext<'input>;

pub type OC_YieldItemsContext<'input> =
    BaseParserRuleContext<'input, OC_YieldItemsContextExt<'input>>;

#[derive(Clone)]
pub struct OC_YieldItemsContextExt<'input> {
    ph: PhantomData<&'input str>,
}

impl<'input> CypherParserContext<'input> for OC_YieldItemsContext<'input> {}

impl<'input, 'a> Listenable<dyn CypherListener<'input> + 'a> for OC_YieldItemsContext<'input> {
    fn enter(&self, listener: &mut (dyn CypherListener<'input> + 'a)) -> Result<(), ANTLRError> {
        listener.enter_every_rule(self)?;
        listener.enter_oC_YieldItems(self);
        Ok(())
    }
    fn exit(&self, listener: &mut (dyn CypherListener<'input> + 'a)) -> Result<(), ANTLRError> {
        listener.exit_oC_YieldItems(self);
        listener.exit_every_rule(self)?;
        Ok(())
    }
}

impl<'input, 'a> Visitable<dyn CypherVisitor<'input> + 'a> for OC_YieldItemsContext<'input> {
    fn accept(&self, visitor: &mut (dyn CypherVisitor<'input> + 'a)) {
        visitor.visit_oC_YieldItems(self);
    }
}

impl<'input> CustomRuleContext<'input> for OC_YieldItemsContextExt<'input> {
    type TF = LocalTokenFactory<'input>;
    type Ctx = CypherParserContextType;
    fn get_rule_index(&self) -> usize {
        RULE_oC_YieldItems
    }
    //fn type_rule_index() -> usize where Self: Sized { RULE_oC_YieldItems }
}
antlr4rust::tid! {OC_YieldItemsContextExt<'a>}

impl<'input> OC_YieldItemsContextExt<'input> {
    fn new(
        parent: Option<Rc<dyn CypherParserContext<'input> + 'input>>,
        invoking_state: i32,
    ) -> Rc<OC_YieldItemsContextAll<'input>> {
        Rc::new(BaseParserRuleContext::new_parser_ctx(
            parent,
            invoking_state,
            OC_YieldItemsContextExt { ph: PhantomData },
        ))
    }
}

pub trait OC_YieldItemsContextAttrs<'input>:
    CypherParserContext<'input> + BorrowMut<OC_YieldItemsContextExt<'input>>
{
    fn oC_YieldItem_all(&self) -> Vec<Rc<OC_YieldItemContextAll<'input>>>
    where
        Self: Sized,
    {
        self.children_of_type()
    }
    fn oC_YieldItem(&self, i: usize) -> Option<Rc<OC_YieldItemContextAll<'input>>>
    where
        Self: Sized,
    {
        self.child_of_type(i)
    }
    fn oC_Where(&self) -> Option<Rc<OC_WhereContextAll<'input>>>
    where
        Self: Sized,
    {
        self.child_of_type(0)
    }
    /// Retrieves all `TerminalNode`s corresponding to token SP in current rule
    fn SP_all(&self) -> Vec<Rc<TerminalNode<'input, CypherParserContextType>>>
    where
        Self: Sized,
    {
        self.children_of_type()
    }
    /// Retrieves 'i's TerminalNode corresponding to token SP, starting from 0.
    /// Returns `None` if number of children corresponding to token SP is less or equal than `i`.
    fn SP(&self, i: usize) -> Option<Rc<TerminalNode<'input, CypherParserContextType>>>
    where
        Self: Sized,
    {
        self.get_token(Cypher_SP, i)
    }
}

impl<'input> OC_YieldItemsContextAttrs<'input> for OC_YieldItemsContext<'input> {}

impl<'input, I> CypherParser<'input, I>
where
    I: TokenStream<'input, TF = LocalTokenFactory<'input>> + TidAble<'input>,
{
    pub fn oC_YieldItems(&mut self) -> Result<Rc<OC_YieldItemsContextAll<'input>>, ANTLRError> {
        let mut recog = self;
        let _parentctx = recog.ctx.take();
        let mut _localctx =
            OC_YieldItemsContextExt::new(_parentctx.clone(), recog.base.get_state());
        recog
            .base
            .enter_rule(_localctx.clone(), 44, RULE_oC_YieldItems);
        let mut _localctx: Rc<OC_YieldItemsContextAll> = _localctx;
        let mut _la: i32 = -1;
        let result: Result<(), ANTLRError> = (|| {
            let mut _alt: i32;
            //recog.base.enter_outer_alt(_localctx.clone(), 1)?;
            recog.base.enter_outer_alt(None, 1)?;
            {
                /*InvokeRule oC_YieldItem*/
                recog.base.set_state(527);
                recog.oC_YieldItem()?;

                recog.base.set_state(538);
                recog.err_handler.sync(&mut recog.base)?;
                _alt = recog.interpreter.adaptive_predict(66, &mut recog.base)?;
                while { _alt != 2 && _alt != INVALID_ALT } {
                    if _alt == 1 {
                        {
                            {
                                recog.base.set_state(529);
                                recog.err_handler.sync(&mut recog.base)?;
                                _la = recog.base.input.la(1);
                                if _la == Cypher_SP {
                                    {
                                        recog.base.set_state(528);
                                        recog
                                            .base
                                            .match_token(Cypher_SP, &mut recog.err_handler)?;
                                    }
                                }

                                recog.base.set_state(531);
                                recog
                                    .base
                                    .match_token(Cypher_T__1, &mut recog.err_handler)?;

                                recog.base.set_state(533);
                                recog.err_handler.sync(&mut recog.base)?;
                                _la = recog.base.input.la(1);
                                if _la == Cypher_SP {
                                    {
                                        recog.base.set_state(532);
                                        recog
                                            .base
                                            .match_token(Cypher_SP, &mut recog.err_handler)?;
                                    }
                                }

                                /*InvokeRule oC_YieldItem*/
                                recog.base.set_state(535);
                                recog.oC_YieldItem()?;
                            }
                        }
                    }
                    recog.base.set_state(540);
                    recog.err_handler.sync(&mut recog.base)?;
                    _alt = recog.interpreter.adaptive_predict(66, &mut recog.base)?;
                }
                recog.base.set_state(545);
                recog.err_handler.sync(&mut recog.base)?;
                match recog.interpreter.adaptive_predict(68, &mut recog.base)? {
                    x if x == 1 => {
                        {
                            recog.base.set_state(542);
                            recog.err_handler.sync(&mut recog.base)?;
                            _la = recog.base.input.la(1);
                            if _la == Cypher_SP {
                                {
                                    recog.base.set_state(541);
                                    recog.base.match_token(Cypher_SP, &mut recog.err_handler)?;
                                }
                            }

                            /*InvokeRule oC_Where*/
                            recog.base.set_state(544);
                            recog.oC_Where()?;
                        }
                    }

                    _ => {}
                }
            }
            Ok(())
        })();
        match result {
            Ok(_) => {}
            Err(e @ ANTLRError::FallThrough(_)) => return Err(e),
            Err(ref re) => {
                //_localctx.exception = re;
                recog.err_handler.report_error(&mut recog.base, re);
                recog.err_handler.recover(&mut recog.base, re)?;
            }
        }
        recog.base.exit_rule()?;

        Ok(_localctx)
    }
}
//------------------- oC_YieldItem ----------------
pub type OC_YieldItemContextAll<'input> = OC_YieldItemContext<'input>;

pub type OC_YieldItemContext<'input> =
    BaseParserRuleContext<'input, OC_YieldItemContextExt<'input>>;

#[derive(Clone)]
pub struct OC_YieldItemContextExt<'input> {
    ph: PhantomData<&'input str>,
}

impl<'input> CypherParserContext<'input> for OC_YieldItemContext<'input> {}

impl<'input, 'a> Listenable<dyn CypherListener<'input> + 'a> for OC_YieldItemContext<'input> {
    fn enter(&self, listener: &mut (dyn CypherListener<'input> + 'a)) -> Result<(), ANTLRError> {
        listener.enter_every_rule(self)?;
        listener.enter_oC_YieldItem(self);
        Ok(())
    }
    fn exit(&self, listener: &mut (dyn CypherListener<'input> + 'a)) -> Result<(), ANTLRError> {
        listener.exit_oC_YieldItem(self);
        listener.exit_every_rule(self)?;
        Ok(())
    }
}

impl<'input, 'a> Visitable<dyn CypherVisitor<'input> + 'a> for OC_YieldItemContext<'input> {
    fn accept(&self, visitor: &mut (dyn CypherVisitor<'input> + 'a)) {
        visitor.visit_oC_YieldItem(self);
    }
}

impl<'input> CustomRuleContext<'input> for OC_YieldItemContextExt<'input> {
    type TF = LocalTokenFactory<'input>;
    type Ctx = CypherParserContextType;
    fn get_rule_index(&self) -> usize {
        RULE_oC_YieldItem
    }
    //fn type_rule_index() -> usize where Self: Sized { RULE_oC_YieldItem }
}
antlr4rust::tid! {OC_YieldItemContextExt<'a>}

impl<'input> OC_YieldItemContextExt<'input> {
    fn new(
        parent: Option<Rc<dyn CypherParserContext<'input> + 'input>>,
        invoking_state: i32,
    ) -> Rc<OC_YieldItemContextAll<'input>> {
        Rc::new(BaseParserRuleContext::new_parser_ctx(
            parent,
            invoking_state,
            OC_YieldItemContextExt { ph: PhantomData },
        ))
    }
}

pub trait OC_YieldItemContextAttrs<'input>:
    CypherParserContext<'input> + BorrowMut<OC_YieldItemContextExt<'input>>
{
    fn oC_Variable(&self) -> Option<Rc<OC_VariableContextAll<'input>>>
    where
        Self: Sized,
    {
        self.child_of_type(0)
    }
    fn oC_ProcedureResultField(&self) -> Option<Rc<OC_ProcedureResultFieldContextAll<'input>>>
    where
        Self: Sized,
    {
        self.child_of_type(0)
    }
    /// Retrieves all `TerminalNode`s corresponding to token SP in current rule
    fn SP_all(&self) -> Vec<Rc<TerminalNode<'input, CypherParserContextType>>>
    where
        Self: Sized,
    {
        self.children_of_type()
    }
    /// Retrieves 'i's TerminalNode corresponding to token SP, starting from 0.
    /// Returns `None` if number of children corresponding to token SP is less or equal than `i`.
    fn SP(&self, i: usize) -> Option<Rc<TerminalNode<'input, CypherParserContextType>>>
    where
        Self: Sized,
    {
        self.get_token(Cypher_SP, i)
    }
    /// Retrieves first TerminalNode corresponding to token AS
    /// Returns `None` if there is no child corresponding to token AS
    fn AS(&self) -> Option<Rc<TerminalNode<'input, CypherParserContextType>>>
    where
        Self: Sized,
    {
        self.get_token(Cypher_AS, 0)
    }
}

impl<'input> OC_YieldItemContextAttrs<'input> for OC_YieldItemContext<'input> {}

impl<'input, I> CypherParser<'input, I>
where
    I: TokenStream<'input, TF = LocalTokenFactory<'input>> + TidAble<'input>,
{
    pub fn oC_YieldItem(&mut self) -> Result<Rc<OC_YieldItemContextAll<'input>>, ANTLRError> {
        let mut recog = self;
        let _parentctx = recog.ctx.take();
        let mut _localctx = OC_YieldItemContextExt::new(_parentctx.clone(), recog.base.get_state());
        recog
            .base
            .enter_rule(_localctx.clone(), 46, RULE_oC_YieldItem);
        let mut _localctx: Rc<OC_YieldItemContextAll> = _localctx;
        let result: Result<(), ANTLRError> = (|| {
            //recog.base.enter_outer_alt(_localctx.clone(), 1)?;
            recog.base.enter_outer_alt(None, 1)?;
            {
                recog.base.set_state(552);
                recog.err_handler.sync(&mut recog.base)?;
                match recog.interpreter.adaptive_predict(69, &mut recog.base)? {
                    x if x == 1 => {
                        {
                            /*InvokeRule oC_ProcedureResultField*/
                            recog.base.set_state(547);
                            recog.oC_ProcedureResultField()?;

                            recog.base.set_state(548);
                            recog.base.match_token(Cypher_SP, &mut recog.err_handler)?;

                            recog.base.set_state(549);
                            recog.base.match_token(Cypher_AS, &mut recog.err_handler)?;

                            recog.base.set_state(550);
                            recog.base.match_token(Cypher_SP, &mut recog.err_handler)?;
                        }
                    }

                    _ => {}
                }
                /*InvokeRule oC_Variable*/
                recog.base.set_state(554);
                recog.oC_Variable()?;
            }
            Ok(())
        })();
        match result {
            Ok(_) => {}
            Err(e @ ANTLRError::FallThrough(_)) => return Err(e),
            Err(ref re) => {
                //_localctx.exception = re;
                recog.err_handler.report_error(&mut recog.base, re);
                recog.err_handler.recover(&mut recog.base, re)?;
            }
        }
        recog.base.exit_rule()?;

        Ok(_localctx)
    }
}
//------------------- oC_With ----------------
pub type OC_WithContextAll<'input> = OC_WithContext<'input>;

pub type OC_WithContext<'input> = BaseParserRuleContext<'input, OC_WithContextExt<'input>>;

#[derive(Clone)]
pub struct OC_WithContextExt<'input> {
    ph: PhantomData<&'input str>,
}

impl<'input> CypherParserContext<'input> for OC_WithContext<'input> {}

impl<'input, 'a> Listenable<dyn CypherListener<'input> + 'a> for OC_WithContext<'input> {
    fn enter(&self, listener: &mut (dyn CypherListener<'input> + 'a)) -> Result<(), ANTLRError> {
        listener.enter_every_rule(self)?;
        listener.enter_oC_With(self);
        Ok(())
    }
    fn exit(&self, listener: &mut (dyn CypherListener<'input> + 'a)) -> Result<(), ANTLRError> {
        listener.exit_oC_With(self);
        listener.exit_every_rule(self)?;
        Ok(())
    }
}

impl<'input, 'a> Visitable<dyn CypherVisitor<'input> + 'a> for OC_WithContext<'input> {
    fn accept(&self, visitor: &mut (dyn CypherVisitor<'input> + 'a)) {
        visitor.visit_oC_With(self);
    }
}

impl<'input> CustomRuleContext<'input> for OC_WithContextExt<'input> {
    type TF = LocalTokenFactory<'input>;
    type Ctx = CypherParserContextType;
    fn get_rule_index(&self) -> usize {
        RULE_oC_With
    }
    //fn type_rule_index() -> usize where Self: Sized { RULE_oC_With }
}
antlr4rust::tid! {OC_WithContextExt<'a>}

impl<'input> OC_WithContextExt<'input> {
    fn new(
        parent: Option<Rc<dyn CypherParserContext<'input> + 'input>>,
        invoking_state: i32,
    ) -> Rc<OC_WithContextAll<'input>> {
        Rc::new(BaseParserRuleContext::new_parser_ctx(
            parent,
            invoking_state,
            OC_WithContextExt { ph: PhantomData },
        ))
    }
}

pub trait OC_WithContextAttrs<'input>:
    CypherParserContext<'input> + BorrowMut<OC_WithContextExt<'input>>
{
    /// Retrieves first TerminalNode corresponding to token WITH
    /// Returns `None` if there is no child corresponding to token WITH
    fn WITH(&self) -> Option<Rc<TerminalNode<'input, CypherParserContextType>>>
    where
        Self: Sized,
    {
        self.get_token(Cypher_WITH, 0)
    }
    fn oC_ProjectionBody(&self) -> Option<Rc<OC_ProjectionBodyContextAll<'input>>>
    where
        Self: Sized,
    {
        self.child_of_type(0)
    }
    fn oC_Where(&self) -> Option<Rc<OC_WhereContextAll<'input>>>
    where
        Self: Sized,
    {
        self.child_of_type(0)
    }
    /// Retrieves first TerminalNode corresponding to token SP
    /// Returns `None` if there is no child corresponding to token SP
    fn SP(&self) -> Option<Rc<TerminalNode<'input, CypherParserContextType>>>
    where
        Self: Sized,
    {
        self.get_token(Cypher_SP, 0)
    }
}

impl<'input> OC_WithContextAttrs<'input> for OC_WithContext<'input> {}

impl<'input, I> CypherParser<'input, I>
where
    I: TokenStream<'input, TF = LocalTokenFactory<'input>> + TidAble<'input>,
{
    pub fn oC_With(&mut self) -> Result<Rc<OC_WithContextAll<'input>>, ANTLRError> {
        let mut recog = self;
        let _parentctx = recog.ctx.take();
        let mut _localctx = OC_WithContextExt::new(_parentctx.clone(), recog.base.get_state());
        recog.base.enter_rule(_localctx.clone(), 48, RULE_oC_With);
        let mut _localctx: Rc<OC_WithContextAll> = _localctx;
        let mut _la: i32 = -1;
        let result: Result<(), ANTLRError> = (|| {
            //recog.base.enter_outer_alt(_localctx.clone(), 1)?;
            recog.base.enter_outer_alt(None, 1)?;
            {
                recog.base.set_state(556);
                recog
                    .base
                    .match_token(Cypher_WITH, &mut recog.err_handler)?;

                /*InvokeRule oC_ProjectionBody*/
                recog.base.set_state(557);
                recog.oC_ProjectionBody()?;

                recog.base.set_state(562);
                recog.err_handler.sync(&mut recog.base)?;
                match recog.interpreter.adaptive_predict(71, &mut recog.base)? {
                    x if x == 1 => {
                        {
                            recog.base.set_state(559);
                            recog.err_handler.sync(&mut recog.base)?;
                            _la = recog.base.input.la(1);
                            if _la == Cypher_SP {
                                {
                                    recog.base.set_state(558);
                                    recog.base.match_token(Cypher_SP, &mut recog.err_handler)?;
                                }
                            }

                            /*InvokeRule oC_Where*/
                            recog.base.set_state(561);
                            recog.oC_Where()?;
                        }
                    }

                    _ => {}
                }
            }
            Ok(())
        })();
        match result {
            Ok(_) => {}
            Err(e @ ANTLRError::FallThrough(_)) => return Err(e),
            Err(ref re) => {
                //_localctx.exception = re;
                recog.err_handler.report_error(&mut recog.base, re);
                recog.err_handler.recover(&mut recog.base, re)?;
            }
        }
        recog.base.exit_rule()?;

        Ok(_localctx)
    }
}
//------------------- oC_Return ----------------
pub type OC_ReturnContextAll<'input> = OC_ReturnContext<'input>;

pub type OC_ReturnContext<'input> = BaseParserRuleContext<'input, OC_ReturnContextExt<'input>>;

#[derive(Clone)]
pub struct OC_ReturnContextExt<'input> {
    ph: PhantomData<&'input str>,
}

impl<'input> CypherParserContext<'input> for OC_ReturnContext<'input> {}

impl<'input, 'a> Listenable<dyn CypherListener<'input> + 'a> for OC_ReturnContext<'input> {
    fn enter(&self, listener: &mut (dyn CypherListener<'input> + 'a)) -> Result<(), ANTLRError> {
        listener.enter_every_rule(self)?;
        listener.enter_oC_Return(self);
        Ok(())
    }
    fn exit(&self, listener: &mut (dyn CypherListener<'input> + 'a)) -> Result<(), ANTLRError> {
        listener.exit_oC_Return(self);
        listener.exit_every_rule(self)?;
        Ok(())
    }
}

impl<'input, 'a> Visitable<dyn CypherVisitor<'input> + 'a> for OC_ReturnContext<'input> {
    fn accept(&self, visitor: &mut (dyn CypherVisitor<'input> + 'a)) {
        visitor.visit_oC_Return(self);
    }
}

impl<'input> CustomRuleContext<'input> for OC_ReturnContextExt<'input> {
    type TF = LocalTokenFactory<'input>;
    type Ctx = CypherParserContextType;
    fn get_rule_index(&self) -> usize {
        RULE_oC_Return
    }
    //fn type_rule_index() -> usize where Self: Sized { RULE_oC_Return }
}
antlr4rust::tid! {OC_ReturnContextExt<'a>}

impl<'input> OC_ReturnContextExt<'input> {
    fn new(
        parent: Option<Rc<dyn CypherParserContext<'input> + 'input>>,
        invoking_state: i32,
    ) -> Rc<OC_ReturnContextAll<'input>> {
        Rc::new(BaseParserRuleContext::new_parser_ctx(
            parent,
            invoking_state,
            OC_ReturnContextExt { ph: PhantomData },
        ))
    }
}

pub trait OC_ReturnContextAttrs<'input>:
    CypherParserContext<'input> + BorrowMut<OC_ReturnContextExt<'input>>
{
    /// Retrieves first TerminalNode corresponding to token RETURN
    /// Returns `None` if there is no child corresponding to token RETURN
    fn RETURN(&self) -> Option<Rc<TerminalNode<'input, CypherParserContextType>>>
    where
        Self: Sized,
    {
        self.get_token(Cypher_RETURN, 0)
    }
    fn oC_ProjectionBody(&self) -> Option<Rc<OC_ProjectionBodyContextAll<'input>>>
    where
        Self: Sized,
    {
        self.child_of_type(0)
    }
}

impl<'input> OC_ReturnContextAttrs<'input> for OC_ReturnContext<'input> {}

impl<'input, I> CypherParser<'input, I>
where
    I: TokenStream<'input, TF = LocalTokenFactory<'input>> + TidAble<'input>,
{
    pub fn oC_Return(&mut self) -> Result<Rc<OC_ReturnContextAll<'input>>, ANTLRError> {
        let mut recog = self;
        let _parentctx = recog.ctx.take();
        let mut _localctx = OC_ReturnContextExt::new(_parentctx.clone(), recog.base.get_state());
        recog.base.enter_rule(_localctx.clone(), 50, RULE_oC_Return);
        let mut _localctx: Rc<OC_ReturnContextAll> = _localctx;
        let result: Result<(), ANTLRError> = (|| {
            //recog.base.enter_outer_alt(_localctx.clone(), 1)?;
            recog.base.enter_outer_alt(None, 1)?;
            {
                recog.base.set_state(564);
                recog
                    .base
                    .match_token(Cypher_RETURN, &mut recog.err_handler)?;

                /*InvokeRule oC_ProjectionBody*/
                recog.base.set_state(565);
                recog.oC_ProjectionBody()?;
            }
            Ok(())
        })();
        match result {
            Ok(_) => {}
            Err(e @ ANTLRError::FallThrough(_)) => return Err(e),
            Err(ref re) => {
                //_localctx.exception = re;
                recog.err_handler.report_error(&mut recog.base, re);
                recog.err_handler.recover(&mut recog.base, re)?;
            }
        }
        recog.base.exit_rule()?;

        Ok(_localctx)
    }
}
//------------------- oC_ProjectionBody ----------------
pub type OC_ProjectionBodyContextAll<'input> = OC_ProjectionBodyContext<'input>;

pub type OC_ProjectionBodyContext<'input> =
    BaseParserRuleContext<'input, OC_ProjectionBodyContextExt<'input>>;

#[derive(Clone)]
pub struct OC_ProjectionBodyContextExt<'input> {
    ph: PhantomData<&'input str>,
}

impl<'input> CypherParserContext<'input> for OC_ProjectionBodyContext<'input> {}

impl<'input, 'a> Listenable<dyn CypherListener<'input> + 'a> for OC_ProjectionBodyContext<'input> {
    fn enter(&self, listener: &mut (dyn CypherListener<'input> + 'a)) -> Result<(), ANTLRError> {
        listener.enter_every_rule(self)?;
        listener.enter_oC_ProjectionBody(self);
        Ok(())
    }
    fn exit(&self, listener: &mut (dyn CypherListener<'input> + 'a)) -> Result<(), ANTLRError> {
        listener.exit_oC_ProjectionBody(self);
        listener.exit_every_rule(self)?;
        Ok(())
    }
}

impl<'input, 'a> Visitable<dyn CypherVisitor<'input> + 'a> for OC_ProjectionBodyContext<'input> {
    fn accept(&self, visitor: &mut (dyn CypherVisitor<'input> + 'a)) {
        visitor.visit_oC_ProjectionBody(self);
    }
}

impl<'input> CustomRuleContext<'input> for OC_ProjectionBodyContextExt<'input> {
    type TF = LocalTokenFactory<'input>;
    type Ctx = CypherParserContextType;
    fn get_rule_index(&self) -> usize {
        RULE_oC_ProjectionBody
    }
    //fn type_rule_index() -> usize where Self: Sized { RULE_oC_ProjectionBody }
}
antlr4rust::tid! {OC_ProjectionBodyContextExt<'a>}

impl<'input> OC_ProjectionBodyContextExt<'input> {
    fn new(
        parent: Option<Rc<dyn CypherParserContext<'input> + 'input>>,
        invoking_state: i32,
    ) -> Rc<OC_ProjectionBodyContextAll<'input>> {
        Rc::new(BaseParserRuleContext::new_parser_ctx(
            parent,
            invoking_state,
            OC_ProjectionBodyContextExt { ph: PhantomData },
        ))
    }
}

pub trait OC_ProjectionBodyContextAttrs<'input>:
    CypherParserContext<'input> + BorrowMut<OC_ProjectionBodyContextExt<'input>>
{
    /// Retrieves all `TerminalNode`s corresponding to token SP in current rule
    fn SP_all(&self) -> Vec<Rc<TerminalNode<'input, CypherParserContextType>>>
    where
        Self: Sized,
    {
        self.children_of_type()
    }
    /// Retrieves 'i's TerminalNode corresponding to token SP, starting from 0.
    /// Returns `None` if number of children corresponding to token SP is less or equal than `i`.
    fn SP(&self, i: usize) -> Option<Rc<TerminalNode<'input, CypherParserContextType>>>
    where
        Self: Sized,
    {
        self.get_token(Cypher_SP, i)
    }
    fn oC_ProjectionItems(&self) -> Option<Rc<OC_ProjectionItemsContextAll<'input>>>
    where
        Self: Sized,
    {
        self.child_of_type(0)
    }
    /// Retrieves first TerminalNode corresponding to token DISTINCT
    /// Returns `None` if there is no child corresponding to token DISTINCT
    fn DISTINCT(&self) -> Option<Rc<TerminalNode<'input, CypherParserContextType>>>
    where
        Self: Sized,
    {
        self.get_token(Cypher_DISTINCT, 0)
    }
    fn oC_Order(&self) -> Option<Rc<OC_OrderContextAll<'input>>>
    where
        Self: Sized,
    {
        self.child_of_type(0)
    }
    fn oC_Skip(&self) -> Option<Rc<OC_SkipContextAll<'input>>>
    where
        Self: Sized,
    {
        self.child_of_type(0)
    }
    fn oC_Limit(&self) -> Option<Rc<OC_LimitContextAll<'input>>>
    where
        Self: Sized,
    {
        self.child_of_type(0)
    }
}

impl<'input> OC_ProjectionBodyContextAttrs<'input> for OC_ProjectionBodyContext<'input> {}

impl<'input, I> CypherParser<'input, I>
where
    I: TokenStream<'input, TF = LocalTokenFactory<'input>> + TidAble<'input>,
{
    pub fn oC_ProjectionBody(
        &mut self,
    ) -> Result<Rc<OC_ProjectionBodyContextAll<'input>>, ANTLRError> {
        let mut recog = self;
        let _parentctx = recog.ctx.take();
        let mut _localctx =
            OC_ProjectionBodyContextExt::new(_parentctx.clone(), recog.base.get_state());
        recog
            .base
            .enter_rule(_localctx.clone(), 52, RULE_oC_ProjectionBody);
        let mut _localctx: Rc<OC_ProjectionBodyContextAll> = _localctx;
        let mut _la: i32 = -1;
        let result: Result<(), ANTLRError> = (|| {
            //recog.base.enter_outer_alt(_localctx.clone(), 1)?;
            recog.base.enter_outer_alt(None, 1)?;
            {
                recog.base.set_state(571);
                recog.err_handler.sync(&mut recog.base)?;
                match recog.interpreter.adaptive_predict(73, &mut recog.base)? {
                    x if x == 1 => {
                        recog.base.set_state(568);
                        recog.err_handler.sync(&mut recog.base)?;
                        _la = recog.base.input.la(1);
                        if _la == Cypher_SP {
                            {
                                recog.base.set_state(567);
                                recog.base.match_token(Cypher_SP, &mut recog.err_handler)?;
                            }
                        }

                        recog.base.set_state(570);
                        recog
                            .base
                            .match_token(Cypher_DISTINCT, &mut recog.err_handler)?;
                    }

                    _ => {}
                }
                recog.base.set_state(573);
                recog.base.match_token(Cypher_SP, &mut recog.err_handler)?;

                /*InvokeRule oC_ProjectionItems*/
                recog.base.set_state(574);
                recog.oC_ProjectionItems()?;

                recog.base.set_state(577);
                recog.err_handler.sync(&mut recog.base)?;
                match recog.interpreter.adaptive_predict(74, &mut recog.base)? {
                    x if x == 1 => {
                        {
                            recog.base.set_state(575);
                            recog.base.match_token(Cypher_SP, &mut recog.err_handler)?;

                            /*InvokeRule oC_Order*/
                            recog.base.set_state(576);
                            recog.oC_Order()?;
                        }
                    }

                    _ => {}
                }
                recog.base.set_state(581);
                recog.err_handler.sync(&mut recog.base)?;
                match recog.interpreter.adaptive_predict(75, &mut recog.base)? {
                    x if x == 1 => {
                        {
                            recog.base.set_state(579);
                            recog.base.match_token(Cypher_SP, &mut recog.err_handler)?;

                            /*InvokeRule oC_Skip*/
                            recog.base.set_state(580);
                            recog.oC_Skip()?;
                        }
                    }

                    _ => {}
                }
                recog.base.set_state(585);
                recog.err_handler.sync(&mut recog.base)?;
                match recog.interpreter.adaptive_predict(76, &mut recog.base)? {
                    x if x == 1 => {
                        {
                            recog.base.set_state(583);
                            recog.base.match_token(Cypher_SP, &mut recog.err_handler)?;

                            /*InvokeRule oC_Limit*/
                            recog.base.set_state(584);
                            recog.oC_Limit()?;
                        }
                    }

                    _ => {}
                }
            }
            Ok(())
        })();
        match result {
            Ok(_) => {}
            Err(e @ ANTLRError::FallThrough(_)) => return Err(e),
            Err(ref re) => {
                //_localctx.exception = re;
                recog.err_handler.report_error(&mut recog.base, re);
                recog.err_handler.recover(&mut recog.base, re)?;
            }
        }
        recog.base.exit_rule()?;

        Ok(_localctx)
    }
}
//------------------- oC_ProjectionItems ----------------
pub type OC_ProjectionItemsContextAll<'input> = OC_ProjectionItemsContext<'input>;

pub type OC_ProjectionItemsContext<'input> =
    BaseParserRuleContext<'input, OC_ProjectionItemsContextExt<'input>>;

#[derive(Clone)]
pub struct OC_ProjectionItemsContextExt<'input> {
    ph: PhantomData<&'input str>,
}

impl<'input> CypherParserContext<'input> for OC_ProjectionItemsContext<'input> {}

impl<'input, 'a> Listenable<dyn CypherListener<'input> + 'a> for OC_ProjectionItemsContext<'input> {
    fn enter(&self, listener: &mut (dyn CypherListener<'input> + 'a)) -> Result<(), ANTLRError> {
        listener.enter_every_rule(self)?;
        listener.enter_oC_ProjectionItems(self);
        Ok(())
    }
    fn exit(&self, listener: &mut (dyn CypherListener<'input> + 'a)) -> Result<(), ANTLRError> {
        listener.exit_oC_ProjectionItems(self);
        listener.exit_every_rule(self)?;
        Ok(())
    }
}

impl<'input, 'a> Visitable<dyn CypherVisitor<'input> + 'a> for OC_ProjectionItemsContext<'input> {
    fn accept(&self, visitor: &mut (dyn CypherVisitor<'input> + 'a)) {
        visitor.visit_oC_ProjectionItems(self);
    }
}

impl<'input> CustomRuleContext<'input> for OC_ProjectionItemsContextExt<'input> {
    type TF = LocalTokenFactory<'input>;
    type Ctx = CypherParserContextType;
    fn get_rule_index(&self) -> usize {
        RULE_oC_ProjectionItems
    }
    //fn type_rule_index() -> usize where Self: Sized { RULE_oC_ProjectionItems }
}
antlr4rust::tid! {OC_ProjectionItemsContextExt<'a>}

impl<'input> OC_ProjectionItemsContextExt<'input> {
    fn new(
        parent: Option<Rc<dyn CypherParserContext<'input> + 'input>>,
        invoking_state: i32,
    ) -> Rc<OC_ProjectionItemsContextAll<'input>> {
        Rc::new(BaseParserRuleContext::new_parser_ctx(
            parent,
            invoking_state,
            OC_ProjectionItemsContextExt { ph: PhantomData },
        ))
    }
}

pub trait OC_ProjectionItemsContextAttrs<'input>:
    CypherParserContext<'input> + BorrowMut<OC_ProjectionItemsContextExt<'input>>
{
    fn oC_ProjectionItem_all(&self) -> Vec<Rc<OC_ProjectionItemContextAll<'input>>>
    where
        Self: Sized,
    {
        self.children_of_type()
    }
    fn oC_ProjectionItem(&self, i: usize) -> Option<Rc<OC_ProjectionItemContextAll<'input>>>
    where
        Self: Sized,
    {
        self.child_of_type(i)
    }
    /// Retrieves all `TerminalNode`s corresponding to token SP in current rule
    fn SP_all(&self) -> Vec<Rc<TerminalNode<'input, CypherParserContextType>>>
    where
        Self: Sized,
    {
        self.children_of_type()
    }
    /// Retrieves 'i's TerminalNode corresponding to token SP, starting from 0.
    /// Returns `None` if number of children corresponding to token SP is less or equal than `i`.
    fn SP(&self, i: usize) -> Option<Rc<TerminalNode<'input, CypherParserContextType>>>
    where
        Self: Sized,
    {
        self.get_token(Cypher_SP, i)
    }
}

impl<'input> OC_ProjectionItemsContextAttrs<'input> for OC_ProjectionItemsContext<'input> {}

impl<'input, I> CypherParser<'input, I>
where
    I: TokenStream<'input, TF = LocalTokenFactory<'input>> + TidAble<'input>,
{
    pub fn oC_ProjectionItems(
        &mut self,
    ) -> Result<Rc<OC_ProjectionItemsContextAll<'input>>, ANTLRError> {
        let mut recog = self;
        let _parentctx = recog.ctx.take();
        let mut _localctx =
            OC_ProjectionItemsContextExt::new(_parentctx.clone(), recog.base.get_state());
        recog
            .base
            .enter_rule(_localctx.clone(), 54, RULE_oC_ProjectionItems);
        let mut _localctx: Rc<OC_ProjectionItemsContextAll> = _localctx;
        let mut _la: i32 = -1;
        let result: Result<(), ANTLRError> = (|| {
            let mut _alt: i32;
            recog.base.set_state(615);
            recog.err_handler.sync(&mut recog.base)?;
            match recog.base.input.la(1) {
                Cypher_T__4 => {
                    //recog.base.enter_outer_alt(_localctx.clone(), 1)?;
                    recog.base.enter_outer_alt(None, 1)?;
                    {
                        {
                            recog.base.set_state(587);
                            recog
                                .base
                                .match_token(Cypher_T__4, &mut recog.err_handler)?;

                            recog.base.set_state(598);
                            recog.err_handler.sync(&mut recog.base)?;
                            _alt = recog.interpreter.adaptive_predict(79, &mut recog.base)?;
                            while { _alt != 2 && _alt != INVALID_ALT } {
                                if _alt == 1 {
                                    {
                                        {
                                            recog.base.set_state(589);
                                            recog.err_handler.sync(&mut recog.base)?;
                                            _la = recog.base.input.la(1);
                                            if _la == Cypher_SP {
                                                {
                                                    recog.base.set_state(588);
                                                    recog.base.match_token(
                                                        Cypher_SP,
                                                        &mut recog.err_handler,
                                                    )?;
                                                }
                                            }

                                            recog.base.set_state(591);
                                            recog
                                                .base
                                                .match_token(Cypher_T__1, &mut recog.err_handler)?;

                                            recog.base.set_state(593);
                                            recog.err_handler.sync(&mut recog.base)?;
                                            _la = recog.base.input.la(1);
                                            if _la == Cypher_SP {
                                                {
                                                    recog.base.set_state(592);
                                                    recog.base.match_token(
                                                        Cypher_SP,
                                                        &mut recog.err_handler,
                                                    )?;
                                                }
                                            }

                                            /*InvokeRule oC_ProjectionItem*/
                                            recog.base.set_state(595);
                                            recog.oC_ProjectionItem()?;
                                        }
                                    }
                                }
                                recog.base.set_state(600);
                                recog.err_handler.sync(&mut recog.base)?;
                                _alt = recog.interpreter.adaptive_predict(79, &mut recog.base)?;
                            }
                        }
                    }
                }

                Cypher_T__5
                | Cypher_T__7
                | Cypher_T__17
                | Cypher_T__18
                | Cypher_T__23
                | Cypher_T__25
                | Cypher_ALL
                | Cypher_NOT
                | Cypher_NULL
                | Cypher_COUNT
                | Cypher_CASE
                | Cypher_ANY
                | Cypher_NONE
                | Cypher_SINGLE
                | Cypher_CAST
                | Cypher_EXISTS
                | Cypher_TRUE
                | Cypher_FALSE
                | Cypher_HexInteger
                | Cypher_DecimalInteger
                | Cypher_OctalInteger
                | Cypher_HexLetter
                | Cypher_ExponentDecimalReal
                | Cypher_RegularDecimalReal
                | Cypher_StringLiteral
                | Cypher_FILTER
                | Cypher_EXTRACT
                | Cypher_UnescapedSymbolicName
                | Cypher_EscapedSymbolicName => {
                    //recog.base.enter_outer_alt(_localctx.clone(), 2)?;
                    recog.base.enter_outer_alt(None, 2)?;
                    {
                        {
                            /*InvokeRule oC_ProjectionItem*/
                            recog.base.set_state(601);
                            recog.oC_ProjectionItem()?;

                            recog.base.set_state(612);
                            recog.err_handler.sync(&mut recog.base)?;
                            _alt = recog.interpreter.adaptive_predict(82, &mut recog.base)?;
                            while { _alt != 2 && _alt != INVALID_ALT } {
                                if _alt == 1 {
                                    {
                                        {
                                            recog.base.set_state(603);
                                            recog.err_handler.sync(&mut recog.base)?;
                                            _la = recog.base.input.la(1);
                                            if _la == Cypher_SP {
                                                {
                                                    recog.base.set_state(602);
                                                    recog.base.match_token(
                                                        Cypher_SP,
                                                        &mut recog.err_handler,
                                                    )?;
                                                }
                                            }

                                            recog.base.set_state(605);
                                            recog
                                                .base
                                                .match_token(Cypher_T__1, &mut recog.err_handler)?;

                                            recog.base.set_state(607);
                                            recog.err_handler.sync(&mut recog.base)?;
                                            _la = recog.base.input.la(1);
                                            if _la == Cypher_SP {
                                                {
                                                    recog.base.set_state(606);
                                                    recog.base.match_token(
                                                        Cypher_SP,
                                                        &mut recog.err_handler,
                                                    )?;
                                                }
                                            }

                                            /*InvokeRule oC_ProjectionItem*/
                                            recog.base.set_state(609);
                                            recog.oC_ProjectionItem()?;
                                        }
                                    }
                                }
                                recog.base.set_state(614);
                                recog.err_handler.sync(&mut recog.base)?;
                                _alt = recog.interpreter.adaptive_predict(82, &mut recog.base)?;
                            }
                        }
                    }
                }

                _ => Err(ANTLRError::NoAltError(NoViableAltError::new(
                    &mut recog.base,
                )))?,
            }
            Ok(())
        })();
        match result {
            Ok(_) => {}
            Err(e @ ANTLRError::FallThrough(_)) => return Err(e),
            Err(ref re) => {
                //_localctx.exception = re;
                recog.err_handler.report_error(&mut recog.base, re);
                recog.err_handler.recover(&mut recog.base, re)?;
            }
        }
        recog.base.exit_rule()?;

        Ok(_localctx)
    }
}
//------------------- oC_ProjectionItem ----------------
pub type OC_ProjectionItemContextAll<'input> = OC_ProjectionItemContext<'input>;

pub type OC_ProjectionItemContext<'input> =
    BaseParserRuleContext<'input, OC_ProjectionItemContextExt<'input>>;

#[derive(Clone)]
pub struct OC_ProjectionItemContextExt<'input> {
    ph: PhantomData<&'input str>,
}

impl<'input> CypherParserContext<'input> for OC_ProjectionItemContext<'input> {}

impl<'input, 'a> Listenable<dyn CypherListener<'input> + 'a> for OC_ProjectionItemContext<'input> {
    fn enter(&self, listener: &mut (dyn CypherListener<'input> + 'a)) -> Result<(), ANTLRError> {
        listener.enter_every_rule(self)?;
        listener.enter_oC_ProjectionItem(self);
        Ok(())
    }
    fn exit(&self, listener: &mut (dyn CypherListener<'input> + 'a)) -> Result<(), ANTLRError> {
        listener.exit_oC_ProjectionItem(self);
        listener.exit_every_rule(self)?;
        Ok(())
    }
}

impl<'input, 'a> Visitable<dyn CypherVisitor<'input> + 'a> for OC_ProjectionItemContext<'input> {
    fn accept(&self, visitor: &mut (dyn CypherVisitor<'input> + 'a)) {
        visitor.visit_oC_ProjectionItem(self);
    }
}

impl<'input> CustomRuleContext<'input> for OC_ProjectionItemContextExt<'input> {
    type TF = LocalTokenFactory<'input>;
    type Ctx = CypherParserContextType;
    fn get_rule_index(&self) -> usize {
        RULE_oC_ProjectionItem
    }
    //fn type_rule_index() -> usize where Self: Sized { RULE_oC_ProjectionItem }
}
antlr4rust::tid! {OC_ProjectionItemContextExt<'a>}

impl<'input> OC_ProjectionItemContextExt<'input> {
    fn new(
        parent: Option<Rc<dyn CypherParserContext<'input> + 'input>>,
        invoking_state: i32,
    ) -> Rc<OC_ProjectionItemContextAll<'input>> {
        Rc::new(BaseParserRuleContext::new_parser_ctx(
            parent,
            invoking_state,
            OC_ProjectionItemContextExt { ph: PhantomData },
        ))
    }
}

pub trait OC_ProjectionItemContextAttrs<'input>:
    CypherParserContext<'input> + BorrowMut<OC_ProjectionItemContextExt<'input>>
{
    fn oC_Expression(&self) -> Option<Rc<OC_ExpressionContextAll<'input>>>
    where
        Self: Sized,
    {
        self.child_of_type(0)
    }
    /// Retrieves all `TerminalNode`s corresponding to token SP in current rule
    fn SP_all(&self) -> Vec<Rc<TerminalNode<'input, CypherParserContextType>>>
    where
        Self: Sized,
    {
        self.children_of_type()
    }
    /// Retrieves 'i's TerminalNode corresponding to token SP, starting from 0.
    /// Returns `None` if number of children corresponding to token SP is less or equal than `i`.
    fn SP(&self, i: usize) -> Option<Rc<TerminalNode<'input, CypherParserContextType>>>
    where
        Self: Sized,
    {
        self.get_token(Cypher_SP, i)
    }
    /// Retrieves first TerminalNode corresponding to token AS
    /// Returns `None` if there is no child corresponding to token AS
    fn AS(&self) -> Option<Rc<TerminalNode<'input, CypherParserContextType>>>
    where
        Self: Sized,
    {
        self.get_token(Cypher_AS, 0)
    }
    fn oC_Variable(&self) -> Option<Rc<OC_VariableContextAll<'input>>>
    where
        Self: Sized,
    {
        self.child_of_type(0)
    }
}

impl<'input> OC_ProjectionItemContextAttrs<'input> for OC_ProjectionItemContext<'input> {}

impl<'input, I> CypherParser<'input, I>
where
    I: TokenStream<'input, TF = LocalTokenFactory<'input>> + TidAble<'input>,
{
    pub fn oC_ProjectionItem(
        &mut self,
    ) -> Result<Rc<OC_ProjectionItemContextAll<'input>>, ANTLRError> {
        let mut recog = self;
        let _parentctx = recog.ctx.take();
        let mut _localctx =
            OC_ProjectionItemContextExt::new(_parentctx.clone(), recog.base.get_state());
        recog
            .base
            .enter_rule(_localctx.clone(), 56, RULE_oC_ProjectionItem);
        let mut _localctx: Rc<OC_ProjectionItemContextAll> = _localctx;
        let result: Result<(), ANTLRError> = (|| {
            recog.base.set_state(624);
            recog.err_handler.sync(&mut recog.base)?;
            match recog.interpreter.adaptive_predict(84, &mut recog.base)? {
                1 => {
                    //recog.base.enter_outer_alt(_localctx.clone(), 1)?;
                    recog.base.enter_outer_alt(None, 1)?;
                    {
                        {
                            /*InvokeRule oC_Expression*/
                            recog.base.set_state(617);
                            recog.oC_Expression()?;

                            recog.base.set_state(618);
                            recog.base.match_token(Cypher_SP, &mut recog.err_handler)?;

                            recog.base.set_state(619);
                            recog.base.match_token(Cypher_AS, &mut recog.err_handler)?;

                            recog.base.set_state(620);
                            recog.base.match_token(Cypher_SP, &mut recog.err_handler)?;

                            /*InvokeRule oC_Variable*/
                            recog.base.set_state(621);
                            recog.oC_Variable()?;
                        }
                    }
                }
                2 => {
                    //recog.base.enter_outer_alt(_localctx.clone(), 2)?;
                    recog.base.enter_outer_alt(None, 2)?;
                    {
                        /*InvokeRule oC_Expression*/
                        recog.base.set_state(623);
                        recog.oC_Expression()?;
                    }
                }

                _ => {}
            }
            Ok(())
        })();
        match result {
            Ok(_) => {}
            Err(e @ ANTLRError::FallThrough(_)) => return Err(e),
            Err(ref re) => {
                //_localctx.exception = re;
                recog.err_handler.report_error(&mut recog.base, re);
                recog.err_handler.recover(&mut recog.base, re)?;
            }
        }
        recog.base.exit_rule()?;

        Ok(_localctx)
    }
}
//------------------- oC_Order ----------------
pub type OC_OrderContextAll<'input> = OC_OrderContext<'input>;

pub type OC_OrderContext<'input> = BaseParserRuleContext<'input, OC_OrderContextExt<'input>>;

#[derive(Clone)]
pub struct OC_OrderContextExt<'input> {
    ph: PhantomData<&'input str>,
}

impl<'input> CypherParserContext<'input> for OC_OrderContext<'input> {}

impl<'input, 'a> Listenable<dyn CypherListener<'input> + 'a> for OC_OrderContext<'input> {
    fn enter(&self, listener: &mut (dyn CypherListener<'input> + 'a)) -> Result<(), ANTLRError> {
        listener.enter_every_rule(self)?;
        listener.enter_oC_Order(self);
        Ok(())
    }
    fn exit(&self, listener: &mut (dyn CypherListener<'input> + 'a)) -> Result<(), ANTLRError> {
        listener.exit_oC_Order(self);
        listener.exit_every_rule(self)?;
        Ok(())
    }
}

impl<'input, 'a> Visitable<dyn CypherVisitor<'input> + 'a> for OC_OrderContext<'input> {
    fn accept(&self, visitor: &mut (dyn CypherVisitor<'input> + 'a)) {
        visitor.visit_oC_Order(self);
    }
}

impl<'input> CustomRuleContext<'input> for OC_OrderContextExt<'input> {
    type TF = LocalTokenFactory<'input>;
    type Ctx = CypherParserContextType;
    fn get_rule_index(&self) -> usize {
        RULE_oC_Order
    }
    //fn type_rule_index() -> usize where Self: Sized { RULE_oC_Order }
}
antlr4rust::tid! {OC_OrderContextExt<'a>}

impl<'input> OC_OrderContextExt<'input> {
    fn new(
        parent: Option<Rc<dyn CypherParserContext<'input> + 'input>>,
        invoking_state: i32,
    ) -> Rc<OC_OrderContextAll<'input>> {
        Rc::new(BaseParserRuleContext::new_parser_ctx(
            parent,
            invoking_state,
            OC_OrderContextExt { ph: PhantomData },
        ))
    }
}

pub trait OC_OrderContextAttrs<'input>:
    CypherParserContext<'input> + BorrowMut<OC_OrderContextExt<'input>>
{
    /// Retrieves first TerminalNode corresponding to token ORDER
    /// Returns `None` if there is no child corresponding to token ORDER
    fn ORDER(&self) -> Option<Rc<TerminalNode<'input, CypherParserContextType>>>
    where
        Self: Sized,
    {
        self.get_token(Cypher_ORDER, 0)
    }
    /// Retrieves all `TerminalNode`s corresponding to token SP in current rule
    fn SP_all(&self) -> Vec<Rc<TerminalNode<'input, CypherParserContextType>>>
    where
        Self: Sized,
    {
        self.children_of_type()
    }
    /// Retrieves 'i's TerminalNode corresponding to token SP, starting from 0.
    /// Returns `None` if number of children corresponding to token SP is less or equal than `i`.
    fn SP(&self, i: usize) -> Option<Rc<TerminalNode<'input, CypherParserContextType>>>
    where
        Self: Sized,
    {
        self.get_token(Cypher_SP, i)
    }
    /// Retrieves first TerminalNode corresponding to token BY
    /// Returns `None` if there is no child corresponding to token BY
    fn BY(&self) -> Option<Rc<TerminalNode<'input, CypherParserContextType>>>
    where
        Self: Sized,
    {
        self.get_token(Cypher_BY, 0)
    }
    fn oC_SortItem_all(&self) -> Vec<Rc<OC_SortItemContextAll<'input>>>
    where
        Self: Sized,
    {
        self.children_of_type()
    }
    fn oC_SortItem(&self, i: usize) -> Option<Rc<OC_SortItemContextAll<'input>>>
    where
        Self: Sized,
    {
        self.child_of_type(i)
    }
}

impl<'input> OC_OrderContextAttrs<'input> for OC_OrderContext<'input> {}

impl<'input, I> CypherParser<'input, I>
where
    I: TokenStream<'input, TF = LocalTokenFactory<'input>> + TidAble<'input>,
{
    pub fn oC_Order(&mut self) -> Result<Rc<OC_OrderContextAll<'input>>, ANTLRError> {
        let mut recog = self;
        let _parentctx = recog.ctx.take();
        let mut _localctx = OC_OrderContextExt::new(_parentctx.clone(), recog.base.get_state());
        recog.base.enter_rule(_localctx.clone(), 58, RULE_oC_Order);
        let mut _localctx: Rc<OC_OrderContextAll> = _localctx;
        let mut _la: i32 = -1;
        let result: Result<(), ANTLRError> = (|| {
            //recog.base.enter_outer_alt(_localctx.clone(), 1)?;
            recog.base.enter_outer_alt(None, 1)?;
            {
                recog.base.set_state(626);
                recog
                    .base
                    .match_token(Cypher_ORDER, &mut recog.err_handler)?;

                recog.base.set_state(627);
                recog.base.match_token(Cypher_SP, &mut recog.err_handler)?;

                recog.base.set_state(628);
                recog.base.match_token(Cypher_BY, &mut recog.err_handler)?;

                recog.base.set_state(629);
                recog.base.match_token(Cypher_SP, &mut recog.err_handler)?;

                /*InvokeRule oC_SortItem*/
                recog.base.set_state(630);
                recog.oC_SortItem()?;

                recog.base.set_state(638);
                recog.err_handler.sync(&mut recog.base)?;
                _la = recog.base.input.la(1);
                while _la == Cypher_T__1 {
                    {
                        {
                            recog.base.set_state(631);
                            recog
                                .base
                                .match_token(Cypher_T__1, &mut recog.err_handler)?;

                            recog.base.set_state(633);
                            recog.err_handler.sync(&mut recog.base)?;
                            _la = recog.base.input.la(1);
                            if _la == Cypher_SP {
                                {
                                    recog.base.set_state(632);
                                    recog.base.match_token(Cypher_SP, &mut recog.err_handler)?;
                                }
                            }

                            /*InvokeRule oC_SortItem*/
                            recog.base.set_state(635);
                            recog.oC_SortItem()?;
                        }
                    }
                    recog.base.set_state(640);
                    recog.err_handler.sync(&mut recog.base)?;
                    _la = recog.base.input.la(1);
                }
            }
            Ok(())
        })();
        match result {
            Ok(_) => {}
            Err(e @ ANTLRError::FallThrough(_)) => return Err(e),
            Err(ref re) => {
                //_localctx.exception = re;
                recog.err_handler.report_error(&mut recog.base, re);
                recog.err_handler.recover(&mut recog.base, re)?;
            }
        }
        recog.base.exit_rule()?;

        Ok(_localctx)
    }
}
//------------------- oC_Skip ----------------
pub type OC_SkipContextAll<'input> = OC_SkipContext<'input>;

pub type OC_SkipContext<'input> = BaseParserRuleContext<'input, OC_SkipContextExt<'input>>;

#[derive(Clone)]
pub struct OC_SkipContextExt<'input> {
    ph: PhantomData<&'input str>,
}

impl<'input> CypherParserContext<'input> for OC_SkipContext<'input> {}

impl<'input, 'a> Listenable<dyn CypherListener<'input> + 'a> for OC_SkipContext<'input> {
    fn enter(&self, listener: &mut (dyn CypherListener<'input> + 'a)) -> Result<(), ANTLRError> {
        listener.enter_every_rule(self)?;
        listener.enter_oC_Skip(self);
        Ok(())
    }
    fn exit(&self, listener: &mut (dyn CypherListener<'input> + 'a)) -> Result<(), ANTLRError> {
        listener.exit_oC_Skip(self);
        listener.exit_every_rule(self)?;
        Ok(())
    }
}

impl<'input, 'a> Visitable<dyn CypherVisitor<'input> + 'a> for OC_SkipContext<'input> {
    fn accept(&self, visitor: &mut (dyn CypherVisitor<'input> + 'a)) {
        visitor.visit_oC_Skip(self);
    }
}

impl<'input> CustomRuleContext<'input> for OC_SkipContextExt<'input> {
    type TF = LocalTokenFactory<'input>;
    type Ctx = CypherParserContextType;
    fn get_rule_index(&self) -> usize {
        RULE_oC_Skip
    }
    //fn type_rule_index() -> usize where Self: Sized { RULE_oC_Skip }
}
antlr4rust::tid! {OC_SkipContextExt<'a>}

impl<'input> OC_SkipContextExt<'input> {
    fn new(
        parent: Option<Rc<dyn CypherParserContext<'input> + 'input>>,
        invoking_state: i32,
    ) -> Rc<OC_SkipContextAll<'input>> {
        Rc::new(BaseParserRuleContext::new_parser_ctx(
            parent,
            invoking_state,
            OC_SkipContextExt { ph: PhantomData },
        ))
    }
}

pub trait OC_SkipContextAttrs<'input>:
    CypherParserContext<'input> + BorrowMut<OC_SkipContextExt<'input>>
{
    /// Retrieves first TerminalNode corresponding to token L_SKIP
    /// Returns `None` if there is no child corresponding to token L_SKIP
    fn L_SKIP(&self) -> Option<Rc<TerminalNode<'input, CypherParserContextType>>>
    where
        Self: Sized,
    {
        self.get_token(Cypher_L_SKIP, 0)
    }
    /// Retrieves first TerminalNode corresponding to token SP
    /// Returns `None` if there is no child corresponding to token SP
    fn SP(&self) -> Option<Rc<TerminalNode<'input, CypherParserContextType>>>
    where
        Self: Sized,
    {
        self.get_token(Cypher_SP, 0)
    }
    fn oC_Expression(&self) -> Option<Rc<OC_ExpressionContextAll<'input>>>
    where
        Self: Sized,
    {
        self.child_of_type(0)
    }
}

impl<'input> OC_SkipContextAttrs<'input> for OC_SkipContext<'input> {}

impl<'input, I> CypherParser<'input, I>
where
    I: TokenStream<'input, TF = LocalTokenFactory<'input>> + TidAble<'input>,
{
    pub fn oC_Skip(&mut self) -> Result<Rc<OC_SkipContextAll<'input>>, ANTLRError> {
        let mut recog = self;
        let _parentctx = recog.ctx.take();
        let mut _localctx = OC_SkipContextExt::new(_parentctx.clone(), recog.base.get_state());
        recog.base.enter_rule(_localctx.clone(), 60, RULE_oC_Skip);
        let mut _localctx: Rc<OC_SkipContextAll> = _localctx;
        let result: Result<(), ANTLRError> = (|| {
            //recog.base.enter_outer_alt(_localctx.clone(), 1)?;
            recog.base.enter_outer_alt(None, 1)?;
            {
                recog.base.set_state(641);
                recog
                    .base
                    .match_token(Cypher_L_SKIP, &mut recog.err_handler)?;

                recog.base.set_state(642);
                recog.base.match_token(Cypher_SP, &mut recog.err_handler)?;

                /*InvokeRule oC_Expression*/
                recog.base.set_state(643);
                recog.oC_Expression()?;
            }
            Ok(())
        })();
        match result {
            Ok(_) => {}
            Err(e @ ANTLRError::FallThrough(_)) => return Err(e),
            Err(ref re) => {
                //_localctx.exception = re;
                recog.err_handler.report_error(&mut recog.base, re);
                recog.err_handler.recover(&mut recog.base, re)?;
            }
        }
        recog.base.exit_rule()?;

        Ok(_localctx)
    }
}
//------------------- oC_Limit ----------------
pub type OC_LimitContextAll<'input> = OC_LimitContext<'input>;

pub type OC_LimitContext<'input> = BaseParserRuleContext<'input, OC_LimitContextExt<'input>>;

#[derive(Clone)]
pub struct OC_LimitContextExt<'input> {
    ph: PhantomData<&'input str>,
}

impl<'input> CypherParserContext<'input> for OC_LimitContext<'input> {}

impl<'input, 'a> Listenable<dyn CypherListener<'input> + 'a> for OC_LimitContext<'input> {
    fn enter(&self, listener: &mut (dyn CypherListener<'input> + 'a)) -> Result<(), ANTLRError> {
        listener.enter_every_rule(self)?;
        listener.enter_oC_Limit(self);
        Ok(())
    }
    fn exit(&self, listener: &mut (dyn CypherListener<'input> + 'a)) -> Result<(), ANTLRError> {
        listener.exit_oC_Limit(self);
        listener.exit_every_rule(self)?;
        Ok(())
    }
}

impl<'input, 'a> Visitable<dyn CypherVisitor<'input> + 'a> for OC_LimitContext<'input> {
    fn accept(&self, visitor: &mut (dyn CypherVisitor<'input> + 'a)) {
        visitor.visit_oC_Limit(self);
    }
}

impl<'input> CustomRuleContext<'input> for OC_LimitContextExt<'input> {
    type TF = LocalTokenFactory<'input>;
    type Ctx = CypherParserContextType;
    fn get_rule_index(&self) -> usize {
        RULE_oC_Limit
    }
    //fn type_rule_index() -> usize where Self: Sized { RULE_oC_Limit }
}
antlr4rust::tid! {OC_LimitContextExt<'a>}

impl<'input> OC_LimitContextExt<'input> {
    fn new(
        parent: Option<Rc<dyn CypherParserContext<'input> + 'input>>,
        invoking_state: i32,
    ) -> Rc<OC_LimitContextAll<'input>> {
        Rc::new(BaseParserRuleContext::new_parser_ctx(
            parent,
            invoking_state,
            OC_LimitContextExt { ph: PhantomData },
        ))
    }
}

pub trait OC_LimitContextAttrs<'input>:
    CypherParserContext<'input> + BorrowMut<OC_LimitContextExt<'input>>
{
    /// Retrieves first TerminalNode corresponding to token LIMIT
    /// Returns `None` if there is no child corresponding to token LIMIT
    fn LIMIT(&self) -> Option<Rc<TerminalNode<'input, CypherParserContextType>>>
    where
        Self: Sized,
    {
        self.get_token(Cypher_LIMIT, 0)
    }
    /// Retrieves first TerminalNode corresponding to token SP
    /// Returns `None` if there is no child corresponding to token SP
    fn SP(&self) -> Option<Rc<TerminalNode<'input, CypherParserContextType>>>
    where
        Self: Sized,
    {
        self.get_token(Cypher_SP, 0)
    }
    fn oC_Expression(&self) -> Option<Rc<OC_ExpressionContextAll<'input>>>
    where
        Self: Sized,
    {
        self.child_of_type(0)
    }
}

impl<'input> OC_LimitContextAttrs<'input> for OC_LimitContext<'input> {}

impl<'input, I> CypherParser<'input, I>
where
    I: TokenStream<'input, TF = LocalTokenFactory<'input>> + TidAble<'input>,
{
    pub fn oC_Limit(&mut self) -> Result<Rc<OC_LimitContextAll<'input>>, ANTLRError> {
        let mut recog = self;
        let _parentctx = recog.ctx.take();
        let mut _localctx = OC_LimitContextExt::new(_parentctx.clone(), recog.base.get_state());
        recog.base.enter_rule(_localctx.clone(), 62, RULE_oC_Limit);
        let mut _localctx: Rc<OC_LimitContextAll> = _localctx;
        let result: Result<(), ANTLRError> = (|| {
            //recog.base.enter_outer_alt(_localctx.clone(), 1)?;
            recog.base.enter_outer_alt(None, 1)?;
            {
                recog.base.set_state(645);
                recog
                    .base
                    .match_token(Cypher_LIMIT, &mut recog.err_handler)?;

                recog.base.set_state(646);
                recog.base.match_token(Cypher_SP, &mut recog.err_handler)?;

                /*InvokeRule oC_Expression*/
                recog.base.set_state(647);
                recog.oC_Expression()?;
            }
            Ok(())
        })();
        match result {
            Ok(_) => {}
            Err(e @ ANTLRError::FallThrough(_)) => return Err(e),
            Err(ref re) => {
                //_localctx.exception = re;
                recog.err_handler.report_error(&mut recog.base, re);
                recog.err_handler.recover(&mut recog.base, re)?;
            }
        }
        recog.base.exit_rule()?;

        Ok(_localctx)
    }
}
//------------------- oC_SortItem ----------------
pub type OC_SortItemContextAll<'input> = OC_SortItemContext<'input>;

pub type OC_SortItemContext<'input> = BaseParserRuleContext<'input, OC_SortItemContextExt<'input>>;

#[derive(Clone)]
pub struct OC_SortItemContextExt<'input> {
    ph: PhantomData<&'input str>,
}

impl<'input> CypherParserContext<'input> for OC_SortItemContext<'input> {}

impl<'input, 'a> Listenable<dyn CypherListener<'input> + 'a> for OC_SortItemContext<'input> {
    fn enter(&self, listener: &mut (dyn CypherListener<'input> + 'a)) -> Result<(), ANTLRError> {
        listener.enter_every_rule(self)?;
        listener.enter_oC_SortItem(self);
        Ok(())
    }
    fn exit(&self, listener: &mut (dyn CypherListener<'input> + 'a)) -> Result<(), ANTLRError> {
        listener.exit_oC_SortItem(self);
        listener.exit_every_rule(self)?;
        Ok(())
    }
}

impl<'input, 'a> Visitable<dyn CypherVisitor<'input> + 'a> for OC_SortItemContext<'input> {
    fn accept(&self, visitor: &mut (dyn CypherVisitor<'input> + 'a)) {
        visitor.visit_oC_SortItem(self);
    }
}

impl<'input> CustomRuleContext<'input> for OC_SortItemContextExt<'input> {
    type TF = LocalTokenFactory<'input>;
    type Ctx = CypherParserContextType;
    fn get_rule_index(&self) -> usize {
        RULE_oC_SortItem
    }
    //fn type_rule_index() -> usize where Self: Sized { RULE_oC_SortItem }
}
antlr4rust::tid! {OC_SortItemContextExt<'a>}

impl<'input> OC_SortItemContextExt<'input> {
    fn new(
        parent: Option<Rc<dyn CypherParserContext<'input> + 'input>>,
        invoking_state: i32,
    ) -> Rc<OC_SortItemContextAll<'input>> {
        Rc::new(BaseParserRuleContext::new_parser_ctx(
            parent,
            invoking_state,
            OC_SortItemContextExt { ph: PhantomData },
        ))
    }
}

pub trait OC_SortItemContextAttrs<'input>:
    CypherParserContext<'input> + BorrowMut<OC_SortItemContextExt<'input>>
{
    fn oC_Expression(&self) -> Option<Rc<OC_ExpressionContextAll<'input>>>
    where
        Self: Sized,
    {
        self.child_of_type(0)
    }
    /// Retrieves first TerminalNode corresponding to token ASCENDING
    /// Returns `None` if there is no child corresponding to token ASCENDING
    fn ASCENDING(&self) -> Option<Rc<TerminalNode<'input, CypherParserContextType>>>
    where
        Self: Sized,
    {
        self.get_token(Cypher_ASCENDING, 0)
    }
    /// Retrieves first TerminalNode corresponding to token ASC
    /// Returns `None` if there is no child corresponding to token ASC
    fn ASC(&self) -> Option<Rc<TerminalNode<'input, CypherParserContextType>>>
    where
        Self: Sized,
    {
        self.get_token(Cypher_ASC, 0)
    }
    /// Retrieves first TerminalNode corresponding to token DESCENDING
    /// Returns `None` if there is no child corresponding to token DESCENDING
    fn DESCENDING(&self) -> Option<Rc<TerminalNode<'input, CypherParserContextType>>>
    where
        Self: Sized,
    {
        self.get_token(Cypher_DESCENDING, 0)
    }
    /// Retrieves first TerminalNode corresponding to token DESC
    /// Returns `None` if there is no child corresponding to token DESC
    fn DESC(&self) -> Option<Rc<TerminalNode<'input, CypherParserContextType>>>
    where
        Self: Sized,
    {
        self.get_token(Cypher_DESC, 0)
    }
    /// Retrieves first TerminalNode corresponding to token SP
    /// Returns `None` if there is no child corresponding to token SP
    fn SP(&self) -> Option<Rc<TerminalNode<'input, CypherParserContextType>>>
    where
        Self: Sized,
    {
        self.get_token(Cypher_SP, 0)
    }
}

impl<'input> OC_SortItemContextAttrs<'input> for OC_SortItemContext<'input> {}

impl<'input, I> CypherParser<'input, I>
where
    I: TokenStream<'input, TF = LocalTokenFactory<'input>> + TidAble<'input>,
{
    pub fn oC_SortItem(&mut self) -> Result<Rc<OC_SortItemContextAll<'input>>, ANTLRError> {
        let mut recog = self;
        let _parentctx = recog.ctx.take();
        let mut _localctx = OC_SortItemContextExt::new(_parentctx.clone(), recog.base.get_state());
        recog
            .base
            .enter_rule(_localctx.clone(), 64, RULE_oC_SortItem);
        let mut _localctx: Rc<OC_SortItemContextAll> = _localctx;
        let mut _la: i32 = -1;
        let result: Result<(), ANTLRError> = (|| {
            //recog.base.enter_outer_alt(_localctx.clone(), 1)?;
            recog.base.enter_outer_alt(None, 1)?;
            {
                /*InvokeRule oC_Expression*/
                recog.base.set_state(649);
                recog.oC_Expression()?;

                recog.base.set_state(654);
                recog.err_handler.sync(&mut recog.base)?;
                match recog.interpreter.adaptive_predict(88, &mut recog.base)? {
                    x if x == 1 => {
                        recog.base.set_state(651);
                        recog.err_handler.sync(&mut recog.base)?;
                        _la = recog.base.input.la(1);
                        if _la == Cypher_SP {
                            {
                                recog.base.set_state(650);
                                recog.base.match_token(Cypher_SP, &mut recog.err_handler)?;
                            }
                        }

                        recog.base.set_state(653);
                        _la = recog.base.input.la(1);
                        if { !(((_la - 68) & !0x3f) == 0 && ((1usize << (_la - 68)) & 15) != 0) } {
                            recog.err_handler.recover_inline(&mut recog.base)?;
                        } else {
                            if recog.base.input.la(1) == TOKEN_EOF {
                                recog.base.matched_eof = true
                            };
                            recog.err_handler.report_match(&mut recog.base);
                            recog.base.consume(&mut recog.err_handler);
                        }
                    }

                    _ => {}
                }
            }
            Ok(())
        })();
        match result {
            Ok(_) => {}
            Err(e @ ANTLRError::FallThrough(_)) => return Err(e),
            Err(ref re) => {
                //_localctx.exception = re;
                recog.err_handler.report_error(&mut recog.base, re);
                recog.err_handler.recover(&mut recog.base, re)?;
            }
        }
        recog.base.exit_rule()?;

        Ok(_localctx)
    }
}
//------------------- oC_Where ----------------
pub type OC_WhereContextAll<'input> = OC_WhereContext<'input>;

pub type OC_WhereContext<'input> = BaseParserRuleContext<'input, OC_WhereContextExt<'input>>;

#[derive(Clone)]
pub struct OC_WhereContextExt<'input> {
    ph: PhantomData<&'input str>,
}

impl<'input> CypherParserContext<'input> for OC_WhereContext<'input> {}

impl<'input, 'a> Listenable<dyn CypherListener<'input> + 'a> for OC_WhereContext<'input> {
    fn enter(&self, listener: &mut (dyn CypherListener<'input> + 'a)) -> Result<(), ANTLRError> {
        listener.enter_every_rule(self)?;
        listener.enter_oC_Where(self);
        Ok(())
    }
    fn exit(&self, listener: &mut (dyn CypherListener<'input> + 'a)) -> Result<(), ANTLRError> {
        listener.exit_oC_Where(self);
        listener.exit_every_rule(self)?;
        Ok(())
    }
}

impl<'input, 'a> Visitable<dyn CypherVisitor<'input> + 'a> for OC_WhereContext<'input> {
    fn accept(&self, visitor: &mut (dyn CypherVisitor<'input> + 'a)) {
        visitor.visit_oC_Where(self);
    }
}

impl<'input> CustomRuleContext<'input> for OC_WhereContextExt<'input> {
    type TF = LocalTokenFactory<'input>;
    type Ctx = CypherParserContextType;
    fn get_rule_index(&self) -> usize {
        RULE_oC_Where
    }
    //fn type_rule_index() -> usize where Self: Sized { RULE_oC_Where }
}
antlr4rust::tid! {OC_WhereContextExt<'a>}

impl<'input> OC_WhereContextExt<'input> {
    fn new(
        parent: Option<Rc<dyn CypherParserContext<'input> + 'input>>,
        invoking_state: i32,
    ) -> Rc<OC_WhereContextAll<'input>> {
        Rc::new(BaseParserRuleContext::new_parser_ctx(
            parent,
            invoking_state,
            OC_WhereContextExt { ph: PhantomData },
        ))
    }
}

pub trait OC_WhereContextAttrs<'input>:
    CypherParserContext<'input> + BorrowMut<OC_WhereContextExt<'input>>
{
    /// Retrieves first TerminalNode corresponding to token WHERE
    /// Returns `None` if there is no child corresponding to token WHERE
    fn WHERE(&self) -> Option<Rc<TerminalNode<'input, CypherParserContextType>>>
    where
        Self: Sized,
    {
        self.get_token(Cypher_WHERE, 0)
    }
    /// Retrieves first TerminalNode corresponding to token SP
    /// Returns `None` if there is no child corresponding to token SP
    fn SP(&self) -> Option<Rc<TerminalNode<'input, CypherParserContextType>>>
    where
        Self: Sized,
    {
        self.get_token(Cypher_SP, 0)
    }
    fn oC_Expression(&self) -> Option<Rc<OC_ExpressionContextAll<'input>>>
    where
        Self: Sized,
    {
        self.child_of_type(0)
    }
}

impl<'input> OC_WhereContextAttrs<'input> for OC_WhereContext<'input> {}

impl<'input, I> CypherParser<'input, I>
where
    I: TokenStream<'input, TF = LocalTokenFactory<'input>> + TidAble<'input>,
{
    pub fn oC_Where(&mut self) -> Result<Rc<OC_WhereContextAll<'input>>, ANTLRError> {
        let mut recog = self;
        let _parentctx = recog.ctx.take();
        let mut _localctx = OC_WhereContextExt::new(_parentctx.clone(), recog.base.get_state());
        recog.base.enter_rule(_localctx.clone(), 66, RULE_oC_Where);
        let mut _localctx: Rc<OC_WhereContextAll> = _localctx;
        let result: Result<(), ANTLRError> = (|| {
            //recog.base.enter_outer_alt(_localctx.clone(), 1)?;
            recog.base.enter_outer_alt(None, 1)?;
            {
                recog.base.set_state(656);
                recog
                    .base
                    .match_token(Cypher_WHERE, &mut recog.err_handler)?;

                recog.base.set_state(657);
                recog.base.match_token(Cypher_SP, &mut recog.err_handler)?;

                /*InvokeRule oC_Expression*/
                recog.base.set_state(658);
                recog.oC_Expression()?;
            }
            Ok(())
        })();
        match result {
            Ok(_) => {}
            Err(e @ ANTLRError::FallThrough(_)) => return Err(e),
            Err(ref re) => {
                //_localctx.exception = re;
                recog.err_handler.report_error(&mut recog.base, re);
                recog.err_handler.recover(&mut recog.base, re)?;
            }
        }
        recog.base.exit_rule()?;

        Ok(_localctx)
    }
}
//------------------- oC_Pattern ----------------
pub type OC_PatternContextAll<'input> = OC_PatternContext<'input>;

pub type OC_PatternContext<'input> = BaseParserRuleContext<'input, OC_PatternContextExt<'input>>;

#[derive(Clone)]
pub struct OC_PatternContextExt<'input> {
    ph: PhantomData<&'input str>,
}

impl<'input> CypherParserContext<'input> for OC_PatternContext<'input> {}

impl<'input, 'a> Listenable<dyn CypherListener<'input> + 'a> for OC_PatternContext<'input> {
    fn enter(&self, listener: &mut (dyn CypherListener<'input> + 'a)) -> Result<(), ANTLRError> {
        listener.enter_every_rule(self)?;
        listener.enter_oC_Pattern(self);
        Ok(())
    }
    fn exit(&self, listener: &mut (dyn CypherListener<'input> + 'a)) -> Result<(), ANTLRError> {
        listener.exit_oC_Pattern(self);
        listener.exit_every_rule(self)?;
        Ok(())
    }
}

impl<'input, 'a> Visitable<dyn CypherVisitor<'input> + 'a> for OC_PatternContext<'input> {
    fn accept(&self, visitor: &mut (dyn CypherVisitor<'input> + 'a)) {
        visitor.visit_oC_Pattern(self);
    }
}

impl<'input> CustomRuleContext<'input> for OC_PatternContextExt<'input> {
    type TF = LocalTokenFactory<'input>;
    type Ctx = CypherParserContextType;
    fn get_rule_index(&self) -> usize {
        RULE_oC_Pattern
    }
    //fn type_rule_index() -> usize where Self: Sized { RULE_oC_Pattern }
}
antlr4rust::tid! {OC_PatternContextExt<'a>}

impl<'input> OC_PatternContextExt<'input> {
    fn new(
        parent: Option<Rc<dyn CypherParserContext<'input> + 'input>>,
        invoking_state: i32,
    ) -> Rc<OC_PatternContextAll<'input>> {
        Rc::new(BaseParserRuleContext::new_parser_ctx(
            parent,
            invoking_state,
            OC_PatternContextExt { ph: PhantomData },
        ))
    }
}

pub trait OC_PatternContextAttrs<'input>:
    CypherParserContext<'input> + BorrowMut<OC_PatternContextExt<'input>>
{
    fn oC_PatternPart_all(&self) -> Vec<Rc<OC_PatternPartContextAll<'input>>>
    where
        Self: Sized,
    {
        self.children_of_type()
    }
    fn oC_PatternPart(&self, i: usize) -> Option<Rc<OC_PatternPartContextAll<'input>>>
    where
        Self: Sized,
    {
        self.child_of_type(i)
    }
    /// Retrieves all `TerminalNode`s corresponding to token SP in current rule
    fn SP_all(&self) -> Vec<Rc<TerminalNode<'input, CypherParserContextType>>>
    where
        Self: Sized,
    {
        self.children_of_type()
    }
    /// Retrieves 'i's TerminalNode corresponding to token SP, starting from 0.
    /// Returns `None` if number of children corresponding to token SP is less or equal than `i`.
    fn SP(&self, i: usize) -> Option<Rc<TerminalNode<'input, CypherParserContextType>>>
    where
        Self: Sized,
    {
        self.get_token(Cypher_SP, i)
    }
}

impl<'input> OC_PatternContextAttrs<'input> for OC_PatternContext<'input> {}

impl<'input, I> CypherParser<'input, I>
where
    I: TokenStream<'input, TF = LocalTokenFactory<'input>> + TidAble<'input>,
{
    pub fn oC_Pattern(&mut self) -> Result<Rc<OC_PatternContextAll<'input>>, ANTLRError> {
        let mut recog = self;
        let _parentctx = recog.ctx.take();
        let mut _localctx = OC_PatternContextExt::new(_parentctx.clone(), recog.base.get_state());
        recog
            .base
            .enter_rule(_localctx.clone(), 68, RULE_oC_Pattern);
        let mut _localctx: Rc<OC_PatternContextAll> = _localctx;
        let mut _la: i32 = -1;
        let result: Result<(), ANTLRError> = (|| {
            let mut _alt: i32;
            //recog.base.enter_outer_alt(_localctx.clone(), 1)?;
            recog.base.enter_outer_alt(None, 1)?;
            {
                /*InvokeRule oC_PatternPart*/
                recog.base.set_state(660);
                recog.oC_PatternPart()?;

                recog.base.set_state(671);
                recog.err_handler.sync(&mut recog.base)?;
                _alt = recog.interpreter.adaptive_predict(91, &mut recog.base)?;
                while { _alt != 2 && _alt != INVALID_ALT } {
                    if _alt == 1 {
                        {
                            {
                                recog.base.set_state(662);
                                recog.err_handler.sync(&mut recog.base)?;
                                _la = recog.base.input.la(1);
                                if _la == Cypher_SP {
                                    {
                                        recog.base.set_state(661);
                                        recog
                                            .base
                                            .match_token(Cypher_SP, &mut recog.err_handler)?;
                                    }
                                }

                                recog.base.set_state(664);
                                recog
                                    .base
                                    .match_token(Cypher_T__1, &mut recog.err_handler)?;

                                recog.base.set_state(666);
                                recog.err_handler.sync(&mut recog.base)?;
                                _la = recog.base.input.la(1);
                                if _la == Cypher_SP {
                                    {
                                        recog.base.set_state(665);
                                        recog
                                            .base
                                            .match_token(Cypher_SP, &mut recog.err_handler)?;
                                    }
                                }

                                /*InvokeRule oC_PatternPart*/
                                recog.base.set_state(668);
                                recog.oC_PatternPart()?;
                            }
                        }
                    }
                    recog.base.set_state(673);
                    recog.err_handler.sync(&mut recog.base)?;
                    _alt = recog.interpreter.adaptive_predict(91, &mut recog.base)?;
                }
            }
            Ok(())
        })();
        match result {
            Ok(_) => {}
            Err(e @ ANTLRError::FallThrough(_)) => return Err(e),
            Err(ref re) => {
                //_localctx.exception = re;
                recog.err_handler.report_error(&mut recog.base, re);
                recog.err_handler.recover(&mut recog.base, re)?;
            }
        }
        recog.base.exit_rule()?;

        Ok(_localctx)
    }
}
//------------------- oC_PatternPart ----------------
pub type OC_PatternPartContextAll<'input> = OC_PatternPartContext<'input>;

pub type OC_PatternPartContext<'input> =
    BaseParserRuleContext<'input, OC_PatternPartContextExt<'input>>;

#[derive(Clone)]
pub struct OC_PatternPartContextExt<'input> {
    ph: PhantomData<&'input str>,
}

impl<'input> CypherParserContext<'input> for OC_PatternPartContext<'input> {}

impl<'input, 'a> Listenable<dyn CypherListener<'input> + 'a> for OC_PatternPartContext<'input> {
    fn enter(&self, listener: &mut (dyn CypherListener<'input> + 'a)) -> Result<(), ANTLRError> {
        listener.enter_every_rule(self)?;
        listener.enter_oC_PatternPart(self);
        Ok(())
    }
    fn exit(&self, listener: &mut (dyn CypherListener<'input> + 'a)) -> Result<(), ANTLRError> {
        listener.exit_oC_PatternPart(self);
        listener.exit_every_rule(self)?;
        Ok(())
    }
}

impl<'input, 'a> Visitable<dyn CypherVisitor<'input> + 'a> for OC_PatternPartContext<'input> {
    fn accept(&self, visitor: &mut (dyn CypherVisitor<'input> + 'a)) {
        visitor.visit_oC_PatternPart(self);
    }
}

impl<'input> CustomRuleContext<'input> for OC_PatternPartContextExt<'input> {
    type TF = LocalTokenFactory<'input>;
    type Ctx = CypherParserContextType;
    fn get_rule_index(&self) -> usize {
        RULE_oC_PatternPart
    }
    //fn type_rule_index() -> usize where Self: Sized { RULE_oC_PatternPart }
}
antlr4rust::tid! {OC_PatternPartContextExt<'a>}

impl<'input> OC_PatternPartContextExt<'input> {
    fn new(
        parent: Option<Rc<dyn CypherParserContext<'input> + 'input>>,
        invoking_state: i32,
    ) -> Rc<OC_PatternPartContextAll<'input>> {
        Rc::new(BaseParserRuleContext::new_parser_ctx(
            parent,
            invoking_state,
            OC_PatternPartContextExt { ph: PhantomData },
        ))
    }
}

pub trait OC_PatternPartContextAttrs<'input>:
    CypherParserContext<'input> + BorrowMut<OC_PatternPartContextExt<'input>>
{
    fn oC_Variable(&self) -> Option<Rc<OC_VariableContextAll<'input>>>
    where
        Self: Sized,
    {
        self.child_of_type(0)
    }
    fn oC_AnonymousPatternPart(&self) -> Option<Rc<OC_AnonymousPatternPartContextAll<'input>>>
    where
        Self: Sized,
    {
        self.child_of_type(0)
    }
    /// Retrieves all `TerminalNode`s corresponding to token SP in current rule
    fn SP_all(&self) -> Vec<Rc<TerminalNode<'input, CypherParserContextType>>>
    where
        Self: Sized,
    {
        self.children_of_type()
    }
    /// Retrieves 'i's TerminalNode corresponding to token SP, starting from 0.
    /// Returns `None` if number of children corresponding to token SP is less or equal than `i`.
    fn SP(&self, i: usize) -> Option<Rc<TerminalNode<'input, CypherParserContextType>>>
    where
        Self: Sized,
    {
        self.get_token(Cypher_SP, i)
    }
}

impl<'input> OC_PatternPartContextAttrs<'input> for OC_PatternPartContext<'input> {}

impl<'input, I> CypherParser<'input, I>
where
    I: TokenStream<'input, TF = LocalTokenFactory<'input>> + TidAble<'input>,
{
    pub fn oC_PatternPart(&mut self) -> Result<Rc<OC_PatternPartContextAll<'input>>, ANTLRError> {
        let mut recog = self;
        let _parentctx = recog.ctx.take();
        let mut _localctx =
            OC_PatternPartContextExt::new(_parentctx.clone(), recog.base.get_state());
        recog
            .base
            .enter_rule(_localctx.clone(), 70, RULE_oC_PatternPart);
        let mut _localctx: Rc<OC_PatternPartContextAll> = _localctx;
        let mut _la: i32 = -1;
        let result: Result<(), ANTLRError> = (|| {
            recog.base.set_state(685);
            recog.err_handler.sync(&mut recog.base)?;
            match recog.base.input.la(1) {
                Cypher_COUNT
                | Cypher_ANY
                | Cypher_NONE
                | Cypher_SINGLE
                | Cypher_HexLetter
                | Cypher_FILTER
                | Cypher_EXTRACT
                | Cypher_UnescapedSymbolicName
                | Cypher_EscapedSymbolicName => {
                    //recog.base.enter_outer_alt(_localctx.clone(), 1)?;
                    recog.base.enter_outer_alt(None, 1)?;
                    {
                        {
                            /*InvokeRule oC_Variable*/
                            recog.base.set_state(674);
                            recog.oC_Variable()?;

                            recog.base.set_state(676);
                            recog.err_handler.sync(&mut recog.base)?;
                            _la = recog.base.input.la(1);
                            if _la == Cypher_SP {
                                {
                                    recog.base.set_state(675);
                                    recog.base.match_token(Cypher_SP, &mut recog.err_handler)?;
                                }
                            }

                            recog.base.set_state(678);
                            recog
                                .base
                                .match_token(Cypher_T__2, &mut recog.err_handler)?;

                            recog.base.set_state(680);
                            recog.err_handler.sync(&mut recog.base)?;
                            _la = recog.base.input.la(1);
                            if _la == Cypher_SP {
                                {
                                    recog.base.set_state(679);
                                    recog.base.match_token(Cypher_SP, &mut recog.err_handler)?;
                                }
                            }

                            /*InvokeRule oC_AnonymousPatternPart*/
                            recog.base.set_state(682);
                            recog.oC_AnonymousPatternPart()?;
                        }
                    }
                }

                Cypher_T__5 => {
                    //recog.base.enter_outer_alt(_localctx.clone(), 2)?;
                    recog.base.enter_outer_alt(None, 2)?;
                    {
                        /*InvokeRule oC_AnonymousPatternPart*/
                        recog.base.set_state(684);
                        recog.oC_AnonymousPatternPart()?;
                    }
                }

                _ => Err(ANTLRError::NoAltError(NoViableAltError::new(
                    &mut recog.base,
                )))?,
            }
            Ok(())
        })();
        match result {
            Ok(_) => {}
            Err(e @ ANTLRError::FallThrough(_)) => return Err(e),
            Err(ref re) => {
                //_localctx.exception = re;
                recog.err_handler.report_error(&mut recog.base, re);
                recog.err_handler.recover(&mut recog.base, re)?;
            }
        }
        recog.base.exit_rule()?;

        Ok(_localctx)
    }
}
//------------------- oC_AnonymousPatternPart ----------------
pub type OC_AnonymousPatternPartContextAll<'input> = OC_AnonymousPatternPartContext<'input>;

pub type OC_AnonymousPatternPartContext<'input> =
    BaseParserRuleContext<'input, OC_AnonymousPatternPartContextExt<'input>>;

#[derive(Clone)]
pub struct OC_AnonymousPatternPartContextExt<'input> {
    ph: PhantomData<&'input str>,
}

impl<'input> CypherParserContext<'input> for OC_AnonymousPatternPartContext<'input> {}

impl<'input, 'a> Listenable<dyn CypherListener<'input> + 'a>
    for OC_AnonymousPatternPartContext<'input>
{
    fn enter(&self, listener: &mut (dyn CypherListener<'input> + 'a)) -> Result<(), ANTLRError> {
        listener.enter_every_rule(self)?;
        listener.enter_oC_AnonymousPatternPart(self);
        Ok(())
    }
    fn exit(&self, listener: &mut (dyn CypherListener<'input> + 'a)) -> Result<(), ANTLRError> {
        listener.exit_oC_AnonymousPatternPart(self);
        listener.exit_every_rule(self)?;
        Ok(())
    }
}

impl<'input, 'a> Visitable<dyn CypherVisitor<'input> + 'a>
    for OC_AnonymousPatternPartContext<'input>
{
    fn accept(&self, visitor: &mut (dyn CypherVisitor<'input> + 'a)) {
        visitor.visit_oC_AnonymousPatternPart(self);
    }
}

impl<'input> CustomRuleContext<'input> for OC_AnonymousPatternPartContextExt<'input> {
    type TF = LocalTokenFactory<'input>;
    type Ctx = CypherParserContextType;
    fn get_rule_index(&self) -> usize {
        RULE_oC_AnonymousPatternPart
    }
    //fn type_rule_index() -> usize where Self: Sized { RULE_oC_AnonymousPatternPart }
}
antlr4rust::tid! {OC_AnonymousPatternPartContextExt<'a>}

impl<'input> OC_AnonymousPatternPartContextExt<'input> {
    fn new(
        parent: Option<Rc<dyn CypherParserContext<'input> + 'input>>,
        invoking_state: i32,
    ) -> Rc<OC_AnonymousPatternPartContextAll<'input>> {
        Rc::new(BaseParserRuleContext::new_parser_ctx(
            parent,
            invoking_state,
            OC_AnonymousPatternPartContextExt { ph: PhantomData },
        ))
    }
}

pub trait OC_AnonymousPatternPartContextAttrs<'input>:
    CypherParserContext<'input> + BorrowMut<OC_AnonymousPatternPartContextExt<'input>>
{
    fn oC_PatternElement(&self) -> Option<Rc<OC_PatternElementContextAll<'input>>>
    where
        Self: Sized,
    {
        self.child_of_type(0)
    }
}

impl<'input> OC_AnonymousPatternPartContextAttrs<'input>
    for OC_AnonymousPatternPartContext<'input>
{
}

impl<'input, I> CypherParser<'input, I>
where
    I: TokenStream<'input, TF = LocalTokenFactory<'input>> + TidAble<'input>,
{
    pub fn oC_AnonymousPatternPart(
        &mut self,
    ) -> Result<Rc<OC_AnonymousPatternPartContextAll<'input>>, ANTLRError> {
        let mut recog = self;
        let _parentctx = recog.ctx.take();
        let mut _localctx =
            OC_AnonymousPatternPartContextExt::new(_parentctx.clone(), recog.base.get_state());
        recog
            .base
            .enter_rule(_localctx.clone(), 72, RULE_oC_AnonymousPatternPart);
        let mut _localctx: Rc<OC_AnonymousPatternPartContextAll> = _localctx;
        let result: Result<(), ANTLRError> = (|| {
            //recog.base.enter_outer_alt(_localctx.clone(), 1)?;
            recog.base.enter_outer_alt(None, 1)?;
            {
                /*InvokeRule oC_PatternElement*/
                recog.base.set_state(687);
                recog.oC_PatternElement()?;
            }
            Ok(())
        })();
        match result {
            Ok(_) => {}
            Err(e @ ANTLRError::FallThrough(_)) => return Err(e),
            Err(ref re) => {
                //_localctx.exception = re;
                recog.err_handler.report_error(&mut recog.base, re);
                recog.err_handler.recover(&mut recog.base, re)?;
            }
        }
        recog.base.exit_rule()?;

        Ok(_localctx)
    }
}
//------------------- oC_PatternElement ----------------
pub type OC_PatternElementContextAll<'input> = OC_PatternElementContext<'input>;

pub type OC_PatternElementContext<'input> =
    BaseParserRuleContext<'input, OC_PatternElementContextExt<'input>>;

#[derive(Clone)]
pub struct OC_PatternElementContextExt<'input> {
    ph: PhantomData<&'input str>,
}

impl<'input> CypherParserContext<'input> for OC_PatternElementContext<'input> {}

impl<'input, 'a> Listenable<dyn CypherListener<'input> + 'a> for OC_PatternElementContext<'input> {
    fn enter(&self, listener: &mut (dyn CypherListener<'input> + 'a)) -> Result<(), ANTLRError> {
        listener.enter_every_rule(self)?;
        listener.enter_oC_PatternElement(self);
        Ok(())
    }
    fn exit(&self, listener: &mut (dyn CypherListener<'input> + 'a)) -> Result<(), ANTLRError> {
        listener.exit_oC_PatternElement(self);
        listener.exit_every_rule(self)?;
        Ok(())
    }
}

impl<'input, 'a> Visitable<dyn CypherVisitor<'input> + 'a> for OC_PatternElementContext<'input> {
    fn accept(&self, visitor: &mut (dyn CypherVisitor<'input> + 'a)) {
        visitor.visit_oC_PatternElement(self);
    }
}

impl<'input> CustomRuleContext<'input> for OC_PatternElementContextExt<'input> {
    type TF = LocalTokenFactory<'input>;
    type Ctx = CypherParserContextType;
    fn get_rule_index(&self) -> usize {
        RULE_oC_PatternElement
    }
    //fn type_rule_index() -> usize where Self: Sized { RULE_oC_PatternElement }
}
antlr4rust::tid! {OC_PatternElementContextExt<'a>}

impl<'input> OC_PatternElementContextExt<'input> {
    fn new(
        parent: Option<Rc<dyn CypherParserContext<'input> + 'input>>,
        invoking_state: i32,
    ) -> Rc<OC_PatternElementContextAll<'input>> {
        Rc::new(BaseParserRuleContext::new_parser_ctx(
            parent,
            invoking_state,
            OC_PatternElementContextExt { ph: PhantomData },
        ))
    }
}

pub trait OC_PatternElementContextAttrs<'input>:
    CypherParserContext<'input> + BorrowMut<OC_PatternElementContextExt<'input>>
{
    fn oC_NodePattern(&self) -> Option<Rc<OC_NodePatternContextAll<'input>>>
    where
        Self: Sized,
    {
        self.child_of_type(0)
    }
    fn oC_PatternElementChain_all(&self) -> Vec<Rc<OC_PatternElementChainContextAll<'input>>>
    where
        Self: Sized,
    {
        self.children_of_type()
    }
    fn oC_PatternElementChain(
        &self,
        i: usize,
    ) -> Option<Rc<OC_PatternElementChainContextAll<'input>>>
    where
        Self: Sized,
    {
        self.child_of_type(i)
    }
    /// Retrieves all `TerminalNode`s corresponding to token SP in current rule
    fn SP_all(&self) -> Vec<Rc<TerminalNode<'input, CypherParserContextType>>>
    where
        Self: Sized,
    {
        self.children_of_type()
    }
    /// Retrieves 'i's TerminalNode corresponding to token SP, starting from 0.
    /// Returns `None` if number of children corresponding to token SP is less or equal than `i`.
    fn SP(&self, i: usize) -> Option<Rc<TerminalNode<'input, CypherParserContextType>>>
    where
        Self: Sized,
    {
        self.get_token(Cypher_SP, i)
    }
    fn oC_PatternElement(&self) -> Option<Rc<OC_PatternElementContextAll<'input>>>
    where
        Self: Sized,
    {
        self.child_of_type(0)
    }
}

impl<'input> OC_PatternElementContextAttrs<'input> for OC_PatternElementContext<'input> {}

impl<'input, I> CypherParser<'input, I>
where
    I: TokenStream<'input, TF = LocalTokenFactory<'input>> + TidAble<'input>,
{
    pub fn oC_PatternElement(
        &mut self,
    ) -> Result<Rc<OC_PatternElementContextAll<'input>>, ANTLRError> {
        let mut recog = self;
        let _parentctx = recog.ctx.take();
        let mut _localctx =
            OC_PatternElementContextExt::new(_parentctx.clone(), recog.base.get_state());
        recog
            .base
            .enter_rule(_localctx.clone(), 74, RULE_oC_PatternElement);
        let mut _localctx: Rc<OC_PatternElementContextAll> = _localctx;
        let mut _la: i32 = -1;
        let result: Result<(), ANTLRError> = (|| {
            let mut _alt: i32;
            recog.base.set_state(703);
            recog.err_handler.sync(&mut recog.base)?;
            match recog.interpreter.adaptive_predict(97, &mut recog.base)? {
                1 => {
                    //recog.base.enter_outer_alt(_localctx.clone(), 1)?;
                    recog.base.enter_outer_alt(None, 1)?;
                    {
                        {
                            /*InvokeRule oC_NodePattern*/
                            recog.base.set_state(689);
                            recog.oC_NodePattern()?;

                            recog.base.set_state(696);
                            recog.err_handler.sync(&mut recog.base)?;
                            _alt = recog.interpreter.adaptive_predict(96, &mut recog.base)?;
                            while { _alt != 2 && _alt != INVALID_ALT } {
                                if _alt == 1 {
                                    {
                                        {
                                            recog.base.set_state(691);
                                            recog.err_handler.sync(&mut recog.base)?;
                                            _la = recog.base.input.la(1);
                                            if _la == Cypher_SP {
                                                {
                                                    recog.base.set_state(690);
                                                    recog.base.match_token(
                                                        Cypher_SP,
                                                        &mut recog.err_handler,
                                                    )?;
                                                }
                                            }

                                            /*InvokeRule oC_PatternElementChain*/
                                            recog.base.set_state(693);
                                            recog.oC_PatternElementChain()?;
                                        }
                                    }
                                }
                                recog.base.set_state(698);
                                recog.err_handler.sync(&mut recog.base)?;
                                _alt = recog.interpreter.adaptive_predict(96, &mut recog.base)?;
                            }
                        }
                    }
                }
                2 => {
                    //recog.base.enter_outer_alt(_localctx.clone(), 2)?;
                    recog.base.enter_outer_alt(None, 2)?;
                    {
                        {
                            recog.base.set_state(699);
                            recog
                                .base
                                .match_token(Cypher_T__5, &mut recog.err_handler)?;

                            /*InvokeRule oC_PatternElement*/
                            recog.base.set_state(700);
                            recog.oC_PatternElement()?;

                            recog.base.set_state(701);
                            recog
                                .base
                                .match_token(Cypher_T__6, &mut recog.err_handler)?;
                        }
                    }
                }

                _ => {}
            }
            Ok(())
        })();
        match result {
            Ok(_) => {}
            Err(e @ ANTLRError::FallThrough(_)) => return Err(e),
            Err(ref re) => {
                //_localctx.exception = re;
                recog.err_handler.report_error(&mut recog.base, re);
                recog.err_handler.recover(&mut recog.base, re)?;
            }
        }
        recog.base.exit_rule()?;

        Ok(_localctx)
    }
}
//------------------- oC_RelationshipsPattern ----------------
pub type OC_RelationshipsPatternContextAll<'input> = OC_RelationshipsPatternContext<'input>;

pub type OC_RelationshipsPatternContext<'input> =
    BaseParserRuleContext<'input, OC_RelationshipsPatternContextExt<'input>>;

#[derive(Clone)]
pub struct OC_RelationshipsPatternContextExt<'input> {
    ph: PhantomData<&'input str>,
}

impl<'input> CypherParserContext<'input> for OC_RelationshipsPatternContext<'input> {}

impl<'input, 'a> Listenable<dyn CypherListener<'input> + 'a>
    for OC_RelationshipsPatternContext<'input>
{
    fn enter(&self, listener: &mut (dyn CypherListener<'input> + 'a)) -> Result<(), ANTLRError> {
        listener.enter_every_rule(self)?;
        listener.enter_oC_RelationshipsPattern(self);
        Ok(())
    }
    fn exit(&self, listener: &mut (dyn CypherListener<'input> + 'a)) -> Result<(), ANTLRError> {
        listener.exit_oC_RelationshipsPattern(self);
        listener.exit_every_rule(self)?;
        Ok(())
    }
}

impl<'input, 'a> Visitable<dyn CypherVisitor<'input> + 'a>
    for OC_RelationshipsPatternContext<'input>
{
    fn accept(&self, visitor: &mut (dyn CypherVisitor<'input> + 'a)) {
        visitor.visit_oC_RelationshipsPattern(self);
    }
}

impl<'input> CustomRuleContext<'input> for OC_RelationshipsPatternContextExt<'input> {
    type TF = LocalTokenFactory<'input>;
    type Ctx = CypherParserContextType;
    fn get_rule_index(&self) -> usize {
        RULE_oC_RelationshipsPattern
    }
    //fn type_rule_index() -> usize where Self: Sized { RULE_oC_RelationshipsPattern }
}
antlr4rust::tid! {OC_RelationshipsPatternContextExt<'a>}

impl<'input> OC_RelationshipsPatternContextExt<'input> {
    fn new(
        parent: Option<Rc<dyn CypherParserContext<'input> + 'input>>,
        invoking_state: i32,
    ) -> Rc<OC_RelationshipsPatternContextAll<'input>> {
        Rc::new(BaseParserRuleContext::new_parser_ctx(
            parent,
            invoking_state,
            OC_RelationshipsPatternContextExt { ph: PhantomData },
        ))
    }
}

pub trait OC_RelationshipsPatternContextAttrs<'input>:
    CypherParserContext<'input> + BorrowMut<OC_RelationshipsPatternContextExt<'input>>
{
    fn oC_NodePattern(&self) -> Option<Rc<OC_NodePatternContextAll<'input>>>
    where
        Self: Sized,
    {
        self.child_of_type(0)
    }
    fn oC_PatternElementChain_all(&self) -> Vec<Rc<OC_PatternElementChainContextAll<'input>>>
    where
        Self: Sized,
    {
        self.children_of_type()
    }
    fn oC_PatternElementChain(
        &self,
        i: usize,
    ) -> Option<Rc<OC_PatternElementChainContextAll<'input>>>
    where
        Self: Sized,
    {
        self.child_of_type(i)
    }
    /// Retrieves all `TerminalNode`s corresponding to token SP in current rule
    fn SP_all(&self) -> Vec<Rc<TerminalNode<'input, CypherParserContextType>>>
    where
        Self: Sized,
    {
        self.children_of_type()
    }
    /// Retrieves 'i's TerminalNode corresponding to token SP, starting from 0.
    /// Returns `None` if number of children corresponding to token SP is less or equal than `i`.
    fn SP(&self, i: usize) -> Option<Rc<TerminalNode<'input, CypherParserContextType>>>
    where
        Self: Sized,
    {
        self.get_token(Cypher_SP, i)
    }
}

impl<'input> OC_RelationshipsPatternContextAttrs<'input>
    for OC_RelationshipsPatternContext<'input>
{
}

impl<'input, I> CypherParser<'input, I>
where
    I: TokenStream<'input, TF = LocalTokenFactory<'input>> + TidAble<'input>,
{
    pub fn oC_RelationshipsPattern(
        &mut self,
    ) -> Result<Rc<OC_RelationshipsPatternContextAll<'input>>, ANTLRError> {
        let mut recog = self;
        let _parentctx = recog.ctx.take();
        let mut _localctx =
            OC_RelationshipsPatternContextExt::new(_parentctx.clone(), recog.base.get_state());
        recog
            .base
            .enter_rule(_localctx.clone(), 76, RULE_oC_RelationshipsPattern);
        let mut _localctx: Rc<OC_RelationshipsPatternContextAll> = _localctx;
        let mut _la: i32 = -1;
        let result: Result<(), ANTLRError> = (|| {
            let mut _alt: i32;
            //recog.base.enter_outer_alt(_localctx.clone(), 1)?;
            recog.base.enter_outer_alt(None, 1)?;
            {
                /*InvokeRule oC_NodePattern*/
                recog.base.set_state(705);
                recog.oC_NodePattern()?;

                recog.base.set_state(710);
                recog.err_handler.sync(&mut recog.base)?;
                _alt = 1;
                loop {
                    match _alt {
                        x if x == 1 => {
                            {
                                recog.base.set_state(707);
                                recog.err_handler.sync(&mut recog.base)?;
                                _la = recog.base.input.la(1);
                                if _la == Cypher_SP {
                                    {
                                        recog.base.set_state(706);
                                        recog
                                            .base
                                            .match_token(Cypher_SP, &mut recog.err_handler)?;
                                    }
                                }

                                /*InvokeRule oC_PatternElementChain*/
                                recog.base.set_state(709);
                                recog.oC_PatternElementChain()?;
                            }
                        }

                        _ => Err(ANTLRError::NoAltError(NoViableAltError::new(
                            &mut recog.base,
                        )))?,
                    }
                    recog.base.set_state(712);
                    recog.err_handler.sync(&mut recog.base)?;
                    _alt = recog.interpreter.adaptive_predict(99, &mut recog.base)?;
                    if _alt == 2 || _alt == INVALID_ALT {
                        break;
                    }
                }
            }
            Ok(())
        })();
        match result {
            Ok(_) => {}
            Err(e @ ANTLRError::FallThrough(_)) => return Err(e),
            Err(ref re) => {
                //_localctx.exception = re;
                recog.err_handler.report_error(&mut recog.base, re);
                recog.err_handler.recover(&mut recog.base, re)?;
            }
        }
        recog.base.exit_rule()?;

        Ok(_localctx)
    }
}
//------------------- oC_NodePattern ----------------
pub type OC_NodePatternContextAll<'input> = OC_NodePatternContext<'input>;

pub type OC_NodePatternContext<'input> =
    BaseParserRuleContext<'input, OC_NodePatternContextExt<'input>>;

#[derive(Clone)]
pub struct OC_NodePatternContextExt<'input> {
    ph: PhantomData<&'input str>,
}

impl<'input> CypherParserContext<'input> for OC_NodePatternContext<'input> {}

impl<'input, 'a> Listenable<dyn CypherListener<'input> + 'a> for OC_NodePatternContext<'input> {
    fn enter(&self, listener: &mut (dyn CypherListener<'input> + 'a)) -> Result<(), ANTLRError> {
        listener.enter_every_rule(self)?;
        listener.enter_oC_NodePattern(self);
        Ok(())
    }
    fn exit(&self, listener: &mut (dyn CypherListener<'input> + 'a)) -> Result<(), ANTLRError> {
        listener.exit_oC_NodePattern(self);
        listener.exit_every_rule(self)?;
        Ok(())
    }
}

impl<'input, 'a> Visitable<dyn CypherVisitor<'input> + 'a> for OC_NodePatternContext<'input> {
    fn accept(&self, visitor: &mut (dyn CypherVisitor<'input> + 'a)) {
        visitor.visit_oC_NodePattern(self);
    }
}

impl<'input> CustomRuleContext<'input> for OC_NodePatternContextExt<'input> {
    type TF = LocalTokenFactory<'input>;
    type Ctx = CypherParserContextType;
    fn get_rule_index(&self) -> usize {
        RULE_oC_NodePattern
    }
    //fn type_rule_index() -> usize where Self: Sized { RULE_oC_NodePattern }
}
antlr4rust::tid! {OC_NodePatternContextExt<'a>}

impl<'input> OC_NodePatternContextExt<'input> {
    fn new(
        parent: Option<Rc<dyn CypherParserContext<'input> + 'input>>,
        invoking_state: i32,
    ) -> Rc<OC_NodePatternContextAll<'input>> {
        Rc::new(BaseParserRuleContext::new_parser_ctx(
            parent,
            invoking_state,
            OC_NodePatternContextExt { ph: PhantomData },
        ))
    }
}

pub trait OC_NodePatternContextAttrs<'input>:
    CypherParserContext<'input> + BorrowMut<OC_NodePatternContextExt<'input>>
{
    /// Retrieves all `TerminalNode`s corresponding to token SP in current rule
    fn SP_all(&self) -> Vec<Rc<TerminalNode<'input, CypherParserContextType>>>
    where
        Self: Sized,
    {
        self.children_of_type()
    }
    /// Retrieves 'i's TerminalNode corresponding to token SP, starting from 0.
    /// Returns `None` if number of children corresponding to token SP is less or equal than `i`.
    fn SP(&self, i: usize) -> Option<Rc<TerminalNode<'input, CypherParserContextType>>>
    where
        Self: Sized,
    {
        self.get_token(Cypher_SP, i)
    }
    fn oC_Variable(&self) -> Option<Rc<OC_VariableContextAll<'input>>>
    where
        Self: Sized,
    {
        self.child_of_type(0)
    }
    fn oC_NodeLabels(&self) -> Option<Rc<OC_NodeLabelsContextAll<'input>>>
    where
        Self: Sized,
    {
        self.child_of_type(0)
    }
    fn oC_Properties(&self) -> Option<Rc<OC_PropertiesContextAll<'input>>>
    where
        Self: Sized,
    {
        self.child_of_type(0)
    }
}

impl<'input> OC_NodePatternContextAttrs<'input> for OC_NodePatternContext<'input> {}

impl<'input, I> CypherParser<'input, I>
where
    I: TokenStream<'input, TF = LocalTokenFactory<'input>> + TidAble<'input>,
{
    pub fn oC_NodePattern(&mut self) -> Result<Rc<OC_NodePatternContextAll<'input>>, ANTLRError> {
        let mut recog = self;
        let _parentctx = recog.ctx.take();
        let mut _localctx =
            OC_NodePatternContextExt::new(_parentctx.clone(), recog.base.get_state());
        recog
            .base
            .enter_rule(_localctx.clone(), 78, RULE_oC_NodePattern);
        let mut _localctx: Rc<OC_NodePatternContextAll> = _localctx;
        let mut _la: i32 = -1;
        let result: Result<(), ANTLRError> = (|| {
            //recog.base.enter_outer_alt(_localctx.clone(), 1)?;
            recog.base.enter_outer_alt(None, 1)?;
            {
                recog.base.set_state(714);
                recog
                    .base
                    .match_token(Cypher_T__5, &mut recog.err_handler)?;

                recog.base.set_state(716);
                recog.err_handler.sync(&mut recog.base)?;
                _la = recog.base.input.la(1);
                if _la == Cypher_SP {
                    {
                        recog.base.set_state(715);
                        recog.base.match_token(Cypher_SP, &mut recog.err_handler)?;
                    }
                }

                recog.base.set_state(722);
                recog.err_handler.sync(&mut recog.base)?;
                _la = recog.base.input.la(1);
                if (((_la - 83) & !0x3f) == 0 && ((1usize << (_la - 83)) & 65985) != 0)
                    || (((_la - 120) & !0x3f) == 0 && ((1usize << (_la - 120)) & 39) != 0)
                {
                    {
                        /*InvokeRule oC_Variable*/
                        recog.base.set_state(718);
                        recog.oC_Variable()?;

                        recog.base.set_state(720);
                        recog.err_handler.sync(&mut recog.base)?;
                        _la = recog.base.input.la(1);
                        if _la == Cypher_SP {
                            {
                                recog.base.set_state(719);
                                recog.base.match_token(Cypher_SP, &mut recog.err_handler)?;
                            }
                        }
                    }
                }

                recog.base.set_state(728);
                recog.err_handler.sync(&mut recog.base)?;
                _la = recog.base.input.la(1);
                if _la == Cypher_T__9 {
                    {
                        /*InvokeRule oC_NodeLabels*/
                        recog.base.set_state(724);
                        recog.oC_NodeLabels()?;

                        recog.base.set_state(726);
                        recog.err_handler.sync(&mut recog.base)?;
                        _la = recog.base.input.la(1);
                        if _la == Cypher_SP {
                            {
                                recog.base.set_state(725);
                                recog.base.match_token(Cypher_SP, &mut recog.err_handler)?;
                            }
                        }
                    }
                }

                recog.base.set_state(734);
                recog.err_handler.sync(&mut recog.base)?;
                _la = recog.base.input.la(1);
                if _la == Cypher_T__23 || _la == Cypher_T__25 {
                    {
                        /*InvokeRule oC_Properties*/
                        recog.base.set_state(730);
                        recog.oC_Properties()?;

                        recog.base.set_state(732);
                        recog.err_handler.sync(&mut recog.base)?;
                        _la = recog.base.input.la(1);
                        if _la == Cypher_SP {
                            {
                                recog.base.set_state(731);
                                recog.base.match_token(Cypher_SP, &mut recog.err_handler)?;
                            }
                        }
                    }
                }

                recog.base.set_state(736);
                recog
                    .base
                    .match_token(Cypher_T__6, &mut recog.err_handler)?;
            }
            Ok(())
        })();
        match result {
            Ok(_) => {}
            Err(e @ ANTLRError::FallThrough(_)) => return Err(e),
            Err(ref re) => {
                //_localctx.exception = re;
                recog.err_handler.report_error(&mut recog.base, re);
                recog.err_handler.recover(&mut recog.base, re)?;
            }
        }
        recog.base.exit_rule()?;

        Ok(_localctx)
    }
}
//------------------- oC_PatternElementChain ----------------
pub type OC_PatternElementChainContextAll<'input> = OC_PatternElementChainContext<'input>;

pub type OC_PatternElementChainContext<'input> =
    BaseParserRuleContext<'input, OC_PatternElementChainContextExt<'input>>;

#[derive(Clone)]
pub struct OC_PatternElementChainContextExt<'input> {
    ph: PhantomData<&'input str>,
}

impl<'input> CypherParserContext<'input> for OC_PatternElementChainContext<'input> {}

impl<'input, 'a> Listenable<dyn CypherListener<'input> + 'a>
    for OC_PatternElementChainContext<'input>
{
    fn enter(&self, listener: &mut (dyn CypherListener<'input> + 'a)) -> Result<(), ANTLRError> {
        listener.enter_every_rule(self)?;
        listener.enter_oC_PatternElementChain(self);
        Ok(())
    }
    fn exit(&self, listener: &mut (dyn CypherListener<'input> + 'a)) -> Result<(), ANTLRError> {
        listener.exit_oC_PatternElementChain(self);
        listener.exit_every_rule(self)?;
        Ok(())
    }
}

impl<'input, 'a> Visitable<dyn CypherVisitor<'input> + 'a>
    for OC_PatternElementChainContext<'input>
{
    fn accept(&self, visitor: &mut (dyn CypherVisitor<'input> + 'a)) {
        visitor.visit_oC_PatternElementChain(self);
    }
}

impl<'input> CustomRuleContext<'input> for OC_PatternElementChainContextExt<'input> {
    type TF = LocalTokenFactory<'input>;
    type Ctx = CypherParserContextType;
    fn get_rule_index(&self) -> usize {
        RULE_oC_PatternElementChain
    }
    //fn type_rule_index() -> usize where Self: Sized { RULE_oC_PatternElementChain }
}
antlr4rust::tid! {OC_PatternElementChainContextExt<'a>}

impl<'input> OC_PatternElementChainContextExt<'input> {
    fn new(
        parent: Option<Rc<dyn CypherParserContext<'input> + 'input>>,
        invoking_state: i32,
    ) -> Rc<OC_PatternElementChainContextAll<'input>> {
        Rc::new(BaseParserRuleContext::new_parser_ctx(
            parent,
            invoking_state,
            OC_PatternElementChainContextExt { ph: PhantomData },
        ))
    }
}

pub trait OC_PatternElementChainContextAttrs<'input>:
    CypherParserContext<'input> + BorrowMut<OC_PatternElementChainContextExt<'input>>
{
    fn oC_RelationshipPattern(&self) -> Option<Rc<OC_RelationshipPatternContextAll<'input>>>
    where
        Self: Sized,
    {
        self.child_of_type(0)
    }
    fn oC_NodePattern(&self) -> Option<Rc<OC_NodePatternContextAll<'input>>>
    where
        Self: Sized,
    {
        self.child_of_type(0)
    }
    /// Retrieves first TerminalNode corresponding to token SP
    /// Returns `None` if there is no child corresponding to token SP
    fn SP(&self) -> Option<Rc<TerminalNode<'input, CypherParserContextType>>>
    where
        Self: Sized,
    {
        self.get_token(Cypher_SP, 0)
    }
}

impl<'input> OC_PatternElementChainContextAttrs<'input> for OC_PatternElementChainContext<'input> {}

impl<'input, I> CypherParser<'input, I>
where
    I: TokenStream<'input, TF = LocalTokenFactory<'input>> + TidAble<'input>,
{
    pub fn oC_PatternElementChain(
        &mut self,
    ) -> Result<Rc<OC_PatternElementChainContextAll<'input>>, ANTLRError> {
        let mut recog = self;
        let _parentctx = recog.ctx.take();
        let mut _localctx =
            OC_PatternElementChainContextExt::new(_parentctx.clone(), recog.base.get_state());
        recog
            .base
            .enter_rule(_localctx.clone(), 80, RULE_oC_PatternElementChain);
        let mut _localctx: Rc<OC_PatternElementChainContextAll> = _localctx;
        let mut _la: i32 = -1;
        let result: Result<(), ANTLRError> = (|| {
            //recog.base.enter_outer_alt(_localctx.clone(), 1)?;
            recog.base.enter_outer_alt(None, 1)?;
            {
                /*InvokeRule oC_RelationshipPattern*/
                recog.base.set_state(738);
                recog.oC_RelationshipPattern()?;

                recog.base.set_state(740);
                recog.err_handler.sync(&mut recog.base)?;
                _la = recog.base.input.la(1);
                if _la == Cypher_SP {
                    {
                        recog.base.set_state(739);
                        recog.base.match_token(Cypher_SP, &mut recog.err_handler)?;
                    }
                }

                /*InvokeRule oC_NodePattern*/
                recog.base.set_state(742);
                recog.oC_NodePattern()?;
            }
            Ok(())
        })();
        match result {
            Ok(_) => {}
            Err(e @ ANTLRError::FallThrough(_)) => return Err(e),
            Err(ref re) => {
                //_localctx.exception = re;
                recog.err_handler.report_error(&mut recog.base, re);
                recog.err_handler.recover(&mut recog.base, re)?;
            }
        }
        recog.base.exit_rule()?;

        Ok(_localctx)
    }
}
//------------------- oC_RelationshipPattern ----------------
pub type OC_RelationshipPatternContextAll<'input> = OC_RelationshipPatternContext<'input>;

pub type OC_RelationshipPatternContext<'input> =
    BaseParserRuleContext<'input, OC_RelationshipPatternContextExt<'input>>;

#[derive(Clone)]
pub struct OC_RelationshipPatternContextExt<'input> {
    ph: PhantomData<&'input str>,
}

impl<'input> CypherParserContext<'input> for OC_RelationshipPatternContext<'input> {}

impl<'input, 'a> Listenable<dyn CypherListener<'input> + 'a>
    for OC_RelationshipPatternContext<'input>
{
    fn enter(&self, listener: &mut (dyn CypherListener<'input> + 'a)) -> Result<(), ANTLRError> {
        listener.enter_every_rule(self)?;
        listener.enter_oC_RelationshipPattern(self);
        Ok(())
    }
    fn exit(&self, listener: &mut (dyn CypherListener<'input> + 'a)) -> Result<(), ANTLRError> {
        listener.exit_oC_RelationshipPattern(self);
        listener.exit_every_rule(self)?;
        Ok(())
    }
}

impl<'input, 'a> Visitable<dyn CypherVisitor<'input> + 'a>
    for OC_RelationshipPatternContext<'input>
{
    fn accept(&self, visitor: &mut (dyn CypherVisitor<'input> + 'a)) {
        visitor.visit_oC_RelationshipPattern(self);
    }
}

impl<'input> CustomRuleContext<'input> for OC_RelationshipPatternContextExt<'input> {
    type TF = LocalTokenFactory<'input>;
    type Ctx = CypherParserContextType;
    fn get_rule_index(&self) -> usize {
        RULE_oC_RelationshipPattern
    }
    //fn type_rule_index() -> usize where Self: Sized { RULE_oC_RelationshipPattern }
}
antlr4rust::tid! {OC_RelationshipPatternContextExt<'a>}

impl<'input> OC_RelationshipPatternContextExt<'input> {
    fn new(
        parent: Option<Rc<dyn CypherParserContext<'input> + 'input>>,
        invoking_state: i32,
    ) -> Rc<OC_RelationshipPatternContextAll<'input>> {
        Rc::new(BaseParserRuleContext::new_parser_ctx(
            parent,
            invoking_state,
            OC_RelationshipPatternContextExt { ph: PhantomData },
        ))
    }
}

pub trait OC_RelationshipPatternContextAttrs<'input>:
    CypherParserContext<'input> + BorrowMut<OC_RelationshipPatternContextExt<'input>>
{
    fn oC_LeftArrowHead(&self) -> Option<Rc<OC_LeftArrowHeadContextAll<'input>>>
    where
        Self: Sized,
    {
        self.child_of_type(0)
    }
    fn oC_Dash_all(&self) -> Vec<Rc<OC_DashContextAll<'input>>>
    where
        Self: Sized,
    {
        self.children_of_type()
    }
    fn oC_Dash(&self, i: usize) -> Option<Rc<OC_DashContextAll<'input>>>
    where
        Self: Sized,
    {
        self.child_of_type(i)
    }
    fn oC_RightArrowHead(&self) -> Option<Rc<OC_RightArrowHeadContextAll<'input>>>
    where
        Self: Sized,
    {
        self.child_of_type(0)
    }
    /// Retrieves all `TerminalNode`s corresponding to token SP in current rule
    fn SP_all(&self) -> Vec<Rc<TerminalNode<'input, CypherParserContextType>>>
    where
        Self: Sized,
    {
        self.children_of_type()
    }
    /// Retrieves 'i's TerminalNode corresponding to token SP, starting from 0.
    /// Returns `None` if number of children corresponding to token SP is less or equal than `i`.
    fn SP(&self, i: usize) -> Option<Rc<TerminalNode<'input, CypherParserContextType>>>
    where
        Self: Sized,
    {
        self.get_token(Cypher_SP, i)
    }
    fn oC_RelationshipDetail(&self) -> Option<Rc<OC_RelationshipDetailContextAll<'input>>>
    where
        Self: Sized,
    {
        self.child_of_type(0)
    }
}

impl<'input> OC_RelationshipPatternContextAttrs<'input> for OC_RelationshipPatternContext<'input> {}

impl<'input, I> CypherParser<'input, I>
where
    I: TokenStream<'input, TF = LocalTokenFactory<'input>> + TidAble<'input>,
{
    pub fn oC_RelationshipPattern(
        &mut self,
    ) -> Result<Rc<OC_RelationshipPatternContextAll<'input>>, ANTLRError> {
        let mut recog = self;
        let _parentctx = recog.ctx.take();
        let mut _localctx =
            OC_RelationshipPatternContextExt::new(_parentctx.clone(), recog.base.get_state());
        recog
            .base
            .enter_rule(_localctx.clone(), 82, RULE_oC_RelationshipPattern);
        let mut _localctx: Rc<OC_RelationshipPatternContextAll> = _localctx;
        let mut _la: i32 = -1;
        let result: Result<(), ANTLRError> = (|| {
            recog.base.set_state(808);
            recog.err_handler.sync(&mut recog.base)?;
            match recog.interpreter.adaptive_predict(124, &mut recog.base)? {
                1 => {
                    //recog.base.enter_outer_alt(_localctx.clone(), 1)?;
                    recog.base.enter_outer_alt(None, 1)?;
                    {
                        {
                            /*InvokeRule oC_LeftArrowHead*/
                            recog.base.set_state(744);
                            recog.oC_LeftArrowHead()?;

                            recog.base.set_state(746);
                            recog.err_handler.sync(&mut recog.base)?;
                            _la = recog.base.input.la(1);
                            if _la == Cypher_SP {
                                {
                                    recog.base.set_state(745);
                                    recog.base.match_token(Cypher_SP, &mut recog.err_handler)?;
                                }
                            }

                            /*InvokeRule oC_Dash*/
                            recog.base.set_state(748);
                            recog.oC_Dash()?;

                            recog.base.set_state(750);
                            recog.err_handler.sync(&mut recog.base)?;
                            match recog.interpreter.adaptive_predict(109, &mut recog.base)? {
                                x if x == 1 => {
                                    recog.base.set_state(749);
                                    recog.base.match_token(Cypher_SP, &mut recog.err_handler)?;
                                }

                                _ => {}
                            }
                            recog.base.set_state(753);
                            recog.err_handler.sync(&mut recog.base)?;
                            _la = recog.base.input.la(1);
                            if _la == Cypher_T__7 {
                                {
                                    /*InvokeRule oC_RelationshipDetail*/
                                    recog.base.set_state(752);
                                    recog.oC_RelationshipDetail()?;
                                }
                            }

                            recog.base.set_state(756);
                            recog.err_handler.sync(&mut recog.base)?;
                            _la = recog.base.input.la(1);
                            if _la == Cypher_SP {
                                {
                                    recog.base.set_state(755);
                                    recog.base.match_token(Cypher_SP, &mut recog.err_handler)?;
                                }
                            }

                            /*InvokeRule oC_Dash*/
                            recog.base.set_state(758);
                            recog.oC_Dash()?;

                            recog.base.set_state(760);
                            recog.err_handler.sync(&mut recog.base)?;
                            _la = recog.base.input.la(1);
                            if _la == Cypher_SP {
                                {
                                    recog.base.set_state(759);
                                    recog.base.match_token(Cypher_SP, &mut recog.err_handler)?;
                                }
                            }

                            /*InvokeRule oC_RightArrowHead*/
                            recog.base.set_state(762);
                            recog.oC_RightArrowHead()?;
                        }
                    }
                }
                2 => {
                    //recog.base.enter_outer_alt(_localctx.clone(), 2)?;
                    recog.base.enter_outer_alt(None, 2)?;
                    {
                        {
                            /*InvokeRule oC_LeftArrowHead*/
                            recog.base.set_state(764);
                            recog.oC_LeftArrowHead()?;

                            recog.base.set_state(766);
                            recog.err_handler.sync(&mut recog.base)?;
                            _la = recog.base.input.la(1);
                            if _la == Cypher_SP {
                                {
                                    recog.base.set_state(765);
                                    recog.base.match_token(Cypher_SP, &mut recog.err_handler)?;
                                }
                            }

                            /*InvokeRule oC_Dash*/
                            recog.base.set_state(768);
                            recog.oC_Dash()?;

                            recog.base.set_state(770);
                            recog.err_handler.sync(&mut recog.base)?;
                            match recog.interpreter.adaptive_predict(114, &mut recog.base)? {
                                x if x == 1 => {
                                    recog.base.set_state(769);
                                    recog.base.match_token(Cypher_SP, &mut recog.err_handler)?;
                                }

                                _ => {}
                            }
                            recog.base.set_state(773);
                            recog.err_handler.sync(&mut recog.base)?;
                            _la = recog.base.input.la(1);
                            if _la == Cypher_T__7 {
                                {
                                    /*InvokeRule oC_RelationshipDetail*/
                                    recog.base.set_state(772);
                                    recog.oC_RelationshipDetail()?;
                                }
                            }

                            recog.base.set_state(776);
                            recog.err_handler.sync(&mut recog.base)?;
                            _la = recog.base.input.la(1);
                            if _la == Cypher_SP {
                                {
                                    recog.base.set_state(775);
                                    recog.base.match_token(Cypher_SP, &mut recog.err_handler)?;
                                }
                            }

                            /*InvokeRule oC_Dash*/
                            recog.base.set_state(778);
                            recog.oC_Dash()?;
                        }
                    }
                }
                3 => {
                    //recog.base.enter_outer_alt(_localctx.clone(), 3)?;
                    recog.base.enter_outer_alt(None, 3)?;
                    {
                        {
                            /*InvokeRule oC_Dash*/
                            recog.base.set_state(780);
                            recog.oC_Dash()?;

                            recog.base.set_state(782);
                            recog.err_handler.sync(&mut recog.base)?;
                            match recog.interpreter.adaptive_predict(117, &mut recog.base)? {
                                x if x == 1 => {
                                    recog.base.set_state(781);
                                    recog.base.match_token(Cypher_SP, &mut recog.err_handler)?;
                                }

                                _ => {}
                            }
                            recog.base.set_state(785);
                            recog.err_handler.sync(&mut recog.base)?;
                            _la = recog.base.input.la(1);
                            if _la == Cypher_T__7 {
                                {
                                    /*InvokeRule oC_RelationshipDetail*/
                                    recog.base.set_state(784);
                                    recog.oC_RelationshipDetail()?;
                                }
                            }

                            recog.base.set_state(788);
                            recog.err_handler.sync(&mut recog.base)?;
                            _la = recog.base.input.la(1);
                            if _la == Cypher_SP {
                                {
                                    recog.base.set_state(787);
                                    recog.base.match_token(Cypher_SP, &mut recog.err_handler)?;
                                }
                            }

                            /*InvokeRule oC_Dash*/
                            recog.base.set_state(790);
                            recog.oC_Dash()?;

                            recog.base.set_state(792);
                            recog.err_handler.sync(&mut recog.base)?;
                            _la = recog.base.input.la(1);
                            if _la == Cypher_SP {
                                {
                                    recog.base.set_state(791);
                                    recog.base.match_token(Cypher_SP, &mut recog.err_handler)?;
                                }
                            }

                            /*InvokeRule oC_RightArrowHead*/
                            recog.base.set_state(794);
                            recog.oC_RightArrowHead()?;
                        }
                    }
                }
                4 => {
                    //recog.base.enter_outer_alt(_localctx.clone(), 4)?;
                    recog.base.enter_outer_alt(None, 4)?;
                    {
                        {
                            /*InvokeRule oC_Dash*/
                            recog.base.set_state(796);
                            recog.oC_Dash()?;

                            recog.base.set_state(798);
                            recog.err_handler.sync(&mut recog.base)?;
                            match recog.interpreter.adaptive_predict(121, &mut recog.base)? {
                                x if x == 1 => {
                                    recog.base.set_state(797);
                                    recog.base.match_token(Cypher_SP, &mut recog.err_handler)?;
                                }

                                _ => {}
                            }
                            recog.base.set_state(801);
                            recog.err_handler.sync(&mut recog.base)?;
                            _la = recog.base.input.la(1);
                            if _la == Cypher_T__7 {
                                {
                                    /*InvokeRule oC_RelationshipDetail*/
                                    recog.base.set_state(800);
                                    recog.oC_RelationshipDetail()?;
                                }
                            }

                            recog.base.set_state(804);
                            recog.err_handler.sync(&mut recog.base)?;
                            _la = recog.base.input.la(1);
                            if _la == Cypher_SP {
                                {
                                    recog.base.set_state(803);
                                    recog.base.match_token(Cypher_SP, &mut recog.err_handler)?;
                                }
                            }

                            /*InvokeRule oC_Dash*/
                            recog.base.set_state(806);
                            recog.oC_Dash()?;
                        }
                    }
                }

                _ => {}
            }
            Ok(())
        })();
        match result {
            Ok(_) => {}
            Err(e @ ANTLRError::FallThrough(_)) => return Err(e),
            Err(ref re) => {
                //_localctx.exception = re;
                recog.err_handler.report_error(&mut recog.base, re);
                recog.err_handler.recover(&mut recog.base, re)?;
            }
        }
        recog.base.exit_rule()?;

        Ok(_localctx)
    }
}
//------------------- oC_RelationshipDetail ----------------
pub type OC_RelationshipDetailContextAll<'input> = OC_RelationshipDetailContext<'input>;

pub type OC_RelationshipDetailContext<'input> =
    BaseParserRuleContext<'input, OC_RelationshipDetailContextExt<'input>>;

#[derive(Clone)]
pub struct OC_RelationshipDetailContextExt<'input> {
    ph: PhantomData<&'input str>,
}

impl<'input> CypherParserContext<'input> for OC_RelationshipDetailContext<'input> {}

impl<'input, 'a> Listenable<dyn CypherListener<'input> + 'a>
    for OC_RelationshipDetailContext<'input>
{
    fn enter(&self, listener: &mut (dyn CypherListener<'input> + 'a)) -> Result<(), ANTLRError> {
        listener.enter_every_rule(self)?;
        listener.enter_oC_RelationshipDetail(self);
        Ok(())
    }
    fn exit(&self, listener: &mut (dyn CypherListener<'input> + 'a)) -> Result<(), ANTLRError> {
        listener.exit_oC_RelationshipDetail(self);
        listener.exit_every_rule(self)?;
        Ok(())
    }
}

impl<'input, 'a> Visitable<dyn CypherVisitor<'input> + 'a>
    for OC_RelationshipDetailContext<'input>
{
    fn accept(&self, visitor: &mut (dyn CypherVisitor<'input> + 'a)) {
        visitor.visit_oC_RelationshipDetail(self);
    }
}

impl<'input> CustomRuleContext<'input> for OC_RelationshipDetailContextExt<'input> {
    type TF = LocalTokenFactory<'input>;
    type Ctx = CypherParserContextType;
    fn get_rule_index(&self) -> usize {
        RULE_oC_RelationshipDetail
    }
    //fn type_rule_index() -> usize where Self: Sized { RULE_oC_RelationshipDetail }
}
antlr4rust::tid! {OC_RelationshipDetailContextExt<'a>}

impl<'input> OC_RelationshipDetailContextExt<'input> {
    fn new(
        parent: Option<Rc<dyn CypherParserContext<'input> + 'input>>,
        invoking_state: i32,
    ) -> Rc<OC_RelationshipDetailContextAll<'input>> {
        Rc::new(BaseParserRuleContext::new_parser_ctx(
            parent,
            invoking_state,
            OC_RelationshipDetailContextExt { ph: PhantomData },
        ))
    }
}

pub trait OC_RelationshipDetailContextAttrs<'input>:
    CypherParserContext<'input> + BorrowMut<OC_RelationshipDetailContextExt<'input>>
{
    /// Retrieves all `TerminalNode`s corresponding to token SP in current rule
    fn SP_all(&self) -> Vec<Rc<TerminalNode<'input, CypherParserContextType>>>
    where
        Self: Sized,
    {
        self.children_of_type()
    }
    /// Retrieves 'i's TerminalNode corresponding to token SP, starting from 0.
    /// Returns `None` if number of children corresponding to token SP is less or equal than `i`.
    fn SP(&self, i: usize) -> Option<Rc<TerminalNode<'input, CypherParserContextType>>>
    where
        Self: Sized,
    {
        self.get_token(Cypher_SP, i)
    }
    fn oC_Variable(&self) -> Option<Rc<OC_VariableContextAll<'input>>>
    where
        Self: Sized,
    {
        self.child_of_type(0)
    }
    fn oC_RelationshipTypes(&self) -> Option<Rc<OC_RelationshipTypesContextAll<'input>>>
    where
        Self: Sized,
    {
        self.child_of_type(0)
    }
    fn oC_RangeLiteral(&self) -> Option<Rc<OC_RangeLiteralContextAll<'input>>>
    where
        Self: Sized,
    {
        self.child_of_type(0)
    }
    fn oC_RecursiveRelationshipFilter(
        &self,
    ) -> Option<Rc<OC_RecursiveRelationshipFilterContextAll<'input>>>
    where
        Self: Sized,
    {
        self.child_of_type(0)
    }
    fn oC_Properties(&self) -> Option<Rc<OC_PropertiesContextAll<'input>>>
    where
        Self: Sized,
    {
        self.child_of_type(0)
    }
}

impl<'input> OC_RelationshipDetailContextAttrs<'input> for OC_RelationshipDetailContext<'input> {}

impl<'input, I> CypherParser<'input, I>
where
    I: TokenStream<'input, TF = LocalTokenFactory<'input>> + TidAble<'input>,
{
    pub fn oC_RelationshipDetail(
        &mut self,
    ) -> Result<Rc<OC_RelationshipDetailContextAll<'input>>, ANTLRError> {
        let mut recog = self;
        let _parentctx = recog.ctx.take();
        let mut _localctx =
            OC_RelationshipDetailContextExt::new(_parentctx.clone(), recog.base.get_state());
        recog
            .base
            .enter_rule(_localctx.clone(), 84, RULE_oC_RelationshipDetail);
        let mut _localctx: Rc<OC_RelationshipDetailContextAll> = _localctx;
        let mut _la: i32 = -1;
        let result: Result<(), ANTLRError> = (|| {
            //recog.base.enter_outer_alt(_localctx.clone(), 1)?;
            recog.base.enter_outer_alt(None, 1)?;
            {
                recog.base.set_state(810);
                recog
                    .base
                    .match_token(Cypher_T__7, &mut recog.err_handler)?;

                recog.base.set_state(812);
                recog.err_handler.sync(&mut recog.base)?;
                match recog.interpreter.adaptive_predict(125, &mut recog.base)? {
                    x if x == 1 => {
                        recog.base.set_state(811);
                        recog.base.match_token(Cypher_SP, &mut recog.err_handler)?;
                    }

                    _ => {}
                }
                recog.base.set_state(818);
                recog.err_handler.sync(&mut recog.base)?;
                _la = recog.base.input.la(1);
                if (((_la - 83) & !0x3f) == 0 && ((1usize << (_la - 83)) & 65985) != 0)
                    || (((_la - 120) & !0x3f) == 0 && ((1usize << (_la - 120)) & 39) != 0)
                {
                    {
                        /*InvokeRule oC_Variable*/
                        recog.base.set_state(814);
                        recog.oC_Variable()?;

                        recog.base.set_state(816);
                        recog.err_handler.sync(&mut recog.base)?;
                        match recog.interpreter.adaptive_predict(126, &mut recog.base)? {
                            x if x == 1 => {
                                recog.base.set_state(815);
                                recog.base.match_token(Cypher_SP, &mut recog.err_handler)?;
                            }

                            _ => {}
                        }
                    }
                }

                recog.base.set_state(824);
                recog.err_handler.sync(&mut recog.base)?;
                _la = recog.base.input.la(1);
                if _la == Cypher_T__9 {
                    {
                        /*InvokeRule oC_RelationshipTypes*/
                        recog.base.set_state(820);
                        recog.oC_RelationshipTypes()?;

                        recog.base.set_state(822);
                        recog.err_handler.sync(&mut recog.base)?;
                        match recog.interpreter.adaptive_predict(128, &mut recog.base)? {
                            x if x == 1 => {
                                recog.base.set_state(821);
                                recog.base.match_token(Cypher_SP, &mut recog.err_handler)?;
                            }

                            _ => {}
                        }
                    }
                }

                recog.base.set_state(827);
                recog.err_handler.sync(&mut recog.base)?;
                _la = recog.base.input.la(1);
                if _la == Cypher_T__4 {
                    {
                        /*InvokeRule oC_RangeLiteral*/
                        recog.base.set_state(826);
                        recog.oC_RangeLiteral()?;
                    }
                }

                recog.base.set_state(833);
                recog.err_handler.sync(&mut recog.base)?;
                _la = recog.base.input.la(1);
                if _la == Cypher_T__5 || _la == Cypher_SP {
                    {
                        recog.base.set_state(830);
                        recog.err_handler.sync(&mut recog.base)?;
                        _la = recog.base.input.la(1);
                        if _la == Cypher_SP {
                            {
                                recog.base.set_state(829);
                                recog.base.match_token(Cypher_SP, &mut recog.err_handler)?;
                            }
                        }

                        /*InvokeRule oC_RecursiveRelationshipFilter*/
                        recog.base.set_state(832);
                        recog.oC_RecursiveRelationshipFilter()?;
                    }
                }

                recog.base.set_state(839);
                recog.err_handler.sync(&mut recog.base)?;
                _la = recog.base.input.la(1);
                if _la == Cypher_T__23 || _la == Cypher_T__25 {
                    {
                        /*InvokeRule oC_Properties*/
                        recog.base.set_state(835);
                        recog.oC_Properties()?;

                        recog.base.set_state(837);
                        recog.err_handler.sync(&mut recog.base)?;
                        _la = recog.base.input.la(1);
                        if _la == Cypher_SP {
                            {
                                recog.base.set_state(836);
                                recog.base.match_token(Cypher_SP, &mut recog.err_handler)?;
                            }
                        }
                    }
                }

                recog.base.set_state(841);
                recog
                    .base
                    .match_token(Cypher_T__8, &mut recog.err_handler)?;
            }
            Ok(())
        })();
        match result {
            Ok(_) => {}
            Err(e @ ANTLRError::FallThrough(_)) => return Err(e),
            Err(ref re) => {
                //_localctx.exception = re;
                recog.err_handler.report_error(&mut recog.base, re);
                recog.err_handler.recover(&mut recog.base, re)?;
            }
        }
        recog.base.exit_rule()?;

        Ok(_localctx)
    }
}
//------------------- oC_RecursiveRelationshipFilter ----------------
pub type OC_RecursiveRelationshipFilterContextAll<'input> =
    OC_RecursiveRelationshipFilterContext<'input>;

pub type OC_RecursiveRelationshipFilterContext<'input> =
    BaseParserRuleContext<'input, OC_RecursiveRelationshipFilterContextExt<'input>>;

#[derive(Clone)]
pub struct OC_RecursiveRelationshipFilterContextExt<'input> {
    ph: PhantomData<&'input str>,
}

impl<'input> CypherParserContext<'input> for OC_RecursiveRelationshipFilterContext<'input> {}

impl<'input, 'a> Listenable<dyn CypherListener<'input> + 'a>
    for OC_RecursiveRelationshipFilterContext<'input>
{
    fn enter(&self, listener: &mut (dyn CypherListener<'input> + 'a)) -> Result<(), ANTLRError> {
        listener.enter_every_rule(self)?;
        listener.enter_oC_RecursiveRelationshipFilter(self);
        Ok(())
    }
    fn exit(&self, listener: &mut (dyn CypherListener<'input> + 'a)) -> Result<(), ANTLRError> {
        listener.exit_oC_RecursiveRelationshipFilter(self);
        listener.exit_every_rule(self)?;
        Ok(())
    }
}

impl<'input, 'a> Visitable<dyn CypherVisitor<'input> + 'a>
    for OC_RecursiveRelationshipFilterContext<'input>
{
    fn accept(&self, visitor: &mut (dyn CypherVisitor<'input> + 'a)) {
        visitor.visit_oC_RecursiveRelationshipFilter(self);
    }
}

impl<'input> CustomRuleContext<'input> for OC_RecursiveRelationshipFilterContextExt<'input> {
    type TF = LocalTokenFactory<'input>;
    type Ctx = CypherParserContextType;
    fn get_rule_index(&self) -> usize {
        RULE_oC_RecursiveRelationshipFilter
    }
    //fn type_rule_index() -> usize where Self: Sized { RULE_oC_RecursiveRelationshipFilter }
}
antlr4rust::tid! {OC_RecursiveRelationshipFilterContextExt<'a>}

impl<'input> OC_RecursiveRelationshipFilterContextExt<'input> {
    fn new(
        parent: Option<Rc<dyn CypherParserContext<'input> + 'input>>,
        invoking_state: i32,
    ) -> Rc<OC_RecursiveRelationshipFilterContextAll<'input>> {
        Rc::new(BaseParserRuleContext::new_parser_ctx(
            parent,
            invoking_state,
            OC_RecursiveRelationshipFilterContextExt { ph: PhantomData },
        ))
    }
}

pub trait OC_RecursiveRelationshipFilterContextAttrs<'input>:
    CypherParserContext<'input> + BorrowMut<OC_RecursiveRelationshipFilterContextExt<'input>>
{
    fn oC_RecursiveRelationshipFilter_all(
        &self,
    ) -> Vec<Rc<OC_RecursiveRelationshipFilterContextAll<'input>>>
    where
        Self: Sized,
    {
        self.children_of_type()
    }
    fn oC_RecursiveRelationshipFilter(
        &self,
        i: usize,
    ) -> Option<Rc<OC_RecursiveRelationshipFilterContextAll<'input>>>
    where
        Self: Sized,
    {
        self.child_of_type(i)
    }
}

impl<'input> OC_RecursiveRelationshipFilterContextAttrs<'input>
    for OC_RecursiveRelationshipFilterContext<'input>
{
}

impl<'input, I> CypherParser<'input, I>
where
    I: TokenStream<'input, TF = LocalTokenFactory<'input>> + TidAble<'input>,
{
    pub fn oC_RecursiveRelationshipFilter(
        &mut self,
    ) -> Result<Rc<OC_RecursiveRelationshipFilterContextAll<'input>>, ANTLRError> {
        let mut recog = self;
        let _parentctx = recog.ctx.take();
        let mut _localctx = OC_RecursiveRelationshipFilterContextExt::new(
            _parentctx.clone(),
            recog.base.get_state(),
        );
        recog
            .base
            .enter_rule(_localctx.clone(), 86, RULE_oC_RecursiveRelationshipFilter);
        let mut _localctx: Rc<OC_RecursiveRelationshipFilterContextAll> = _localctx;
        let mut _la: i32 = -1;
        let result: Result<(), ANTLRError> = (|| {
            //recog.base.enter_outer_alt(_localctx.clone(), 1)?;
            recog.base.enter_outer_alt(None, 1)?;
            {
                recog.base.set_state(843);
                recog
                    .base
                    .match_token(Cypher_T__5, &mut recog.err_handler)?;

                recog.base.set_state(848);
                recog.err_handler.sync(&mut recog.base)?;
                _la = recog.base.input.la(1);
                while (((_la - 1) & !0x3f) == 0 && ((1usize << (_la - 1)) & 4294967231) != 0)
                    || (((_la - 33) & !0x3f) == 0 && ((1usize << (_la - 33)) & 4294967295) != 0)
                    || (((_la - 65) & !0x3f) == 0 && ((1usize << (_la - 65)) & 4294967295) != 0)
                    || (((_la - 97) & !0x3f) == 0 && ((1usize << (_la - 97)) & 4294967295) != 0)
                {
                    {
                        recog.base.set_state(846);
                        recog.err_handler.sync(&mut recog.base)?;
                        match recog.base.input.la(1) {
                            Cypher_T__5 => {
                                {
                                    /*InvokeRule oC_RecursiveRelationshipFilter*/
                                    recog.base.set_state(844);
                                    recog.oC_RecursiveRelationshipFilter()?;
                                }
                            }

                            Cypher_T__0
                            | Cypher_T__1
                            | Cypher_T__2
                            | Cypher_T__3
                            | Cypher_T__4
                            | Cypher_T__7
                            | Cypher_T__8
                            | Cypher_T__9
                            | Cypher_T__10
                            | Cypher_T__11
                            | Cypher_T__12
                            | Cypher_T__13
                            | Cypher_T__14
                            | Cypher_T__15
                            | Cypher_T__16
                            | Cypher_T__17
                            | Cypher_T__18
                            | Cypher_T__19
                            | Cypher_T__20
                            | Cypher_T__21
                            | Cypher_T__22
                            | Cypher_T__23
                            | Cypher_T__24
                            | Cypher_T__25
                            | Cypher_T__26
                            | Cypher_T__27
                            | Cypher_T__28
                            | Cypher_T__29
                            | Cypher_T__30
                            | Cypher_T__31
                            | Cypher_T__32
                            | Cypher_T__33
                            | Cypher_T__34
                            | Cypher_T__35
                            | Cypher_T__36
                            | Cypher_T__37
                            | Cypher_T__38
                            | Cypher_T__39
                            | Cypher_T__40
                            | Cypher_T__41
                            | Cypher_T__42
                            | Cypher_T__43
                            | Cypher_T__44
                            | Cypher_UNION
                            | Cypher_ALL
                            | Cypher_OPTIONAL
                            | Cypher_MATCH
                            | Cypher_UNWIND
                            | Cypher_AS
                            | Cypher_MERGE
                            | Cypher_ON
                            | Cypher_CREATE
                            | Cypher_SET
                            | Cypher_DETACH
                            | Cypher_DELETE
                            | Cypher_REMOVE
                            | Cypher_CALL
                            | Cypher_YIELD
                            | Cypher_WITH
                            | Cypher_RETURN
                            | Cypher_DISTINCT
                            | Cypher_ORDER
                            | Cypher_BY
                            | Cypher_L_SKIP
                            | Cypher_LIMIT
                            | Cypher_ASCENDING
                            | Cypher_ASC
                            | Cypher_DESCENDING
                            | Cypher_DESC
                            | Cypher_WHERE
                            | Cypher_OR
                            | Cypher_XOR
                            | Cypher_AND
                            | Cypher_NOT
                            | Cypher_STARTS
                            | Cypher_ENDS
                            | Cypher_CONTAINS
                            | Cypher_IN
                            | Cypher_IS
                            | Cypher_NULL
                            | Cypher_COUNT
                            | Cypher_CASE
                            | Cypher_ELSE
                            | Cypher_END
                            | Cypher_WHEN
                            | Cypher_THEN
                            | Cypher_ANY
                            | Cypher_NONE
                            | Cypher_SINGLE
                            | Cypher_CAST
                            | Cypher_EXISTS
                            | Cypher_TRUE
                            | Cypher_FALSE
                            | Cypher_HexInteger
                            | Cypher_DecimalInteger
                            | Cypher_OctalInteger
                            | Cypher_HexLetter
                            | Cypher_HexDigit
                            | Cypher_Digit
                            | Cypher_NonZeroDigit
                            | Cypher_NonZeroOctDigit
                            | Cypher_OctDigit
                            | Cypher_ZeroDigit
                            | Cypher_ExponentDecimalReal
                            | Cypher_RegularDecimalReal
                            | Cypher_StringLiteral
                            | Cypher_EscapedChar
                            | Cypher_CONSTRAINT
                            | Cypher_DO
                            | Cypher_FOR
                            | Cypher_REQUIRE
                            | Cypher_UNIQUE
                            | Cypher_MANDATORY
                            | Cypher_SCALAR
                            | Cypher_OF
                            | Cypher_ADD
                            | Cypher_DROP
                            | Cypher_FILTER
                            | Cypher_EXTRACT
                            | Cypher_UnescapedSymbolicName
                            | Cypher_IdentifierStart
                            | Cypher_IdentifierPart
                            | Cypher_EscapedSymbolicName
                            | Cypher_SP
                            | Cypher_WHITESPACE
                            | Cypher_Comment => {
                                recog.base.set_state(845);
                                _la = recog.base.input.la(1);
                                if { _la <= 0 || (_la == Cypher_T__5 || _la == Cypher_T__6) } {
                                    recog.err_handler.recover_inline(&mut recog.base)?;
                                } else {
                                    if recog.base.input.la(1) == TOKEN_EOF {
                                        recog.base.matched_eof = true
                                    };
                                    recog.err_handler.report_match(&mut recog.base);
                                    recog.base.consume(&mut recog.err_handler);
                                }
                            }

                            _ => Err(ANTLRError::NoAltError(NoViableAltError::new(
                                &mut recog.base,
                            )))?,
                        }
                    }
                    recog.base.set_state(850);
                    recog.err_handler.sync(&mut recog.base)?;
                    _la = recog.base.input.la(1);
                }
                recog.base.set_state(851);
                recog
                    .base
                    .match_token(Cypher_T__6, &mut recog.err_handler)?;
            }
            Ok(())
        })();
        match result {
            Ok(_) => {}
            Err(e @ ANTLRError::FallThrough(_)) => return Err(e),
            Err(ref re) => {
                //_localctx.exception = re;
                recog.err_handler.report_error(&mut recog.base, re);
                recog.err_handler.recover(&mut recog.base, re)?;
            }
        }
        recog.base.exit_rule()?;

        Ok(_localctx)
    }
}
//------------------- oC_Properties ----------------
pub type OC_PropertiesContextAll<'input> = OC_PropertiesContext<'input>;

pub type OC_PropertiesContext<'input> =
    BaseParserRuleContext<'input, OC_PropertiesContextExt<'input>>;

#[derive(Clone)]
pub struct OC_PropertiesContextExt<'input> {
    ph: PhantomData<&'input str>,
}

impl<'input> CypherParserContext<'input> for OC_PropertiesContext<'input> {}

impl<'input, 'a> Listenable<dyn CypherListener<'input> + 'a> for OC_PropertiesContext<'input> {
    fn enter(&self, listener: &mut (dyn CypherListener<'input> + 'a)) -> Result<(), ANTLRError> {
        listener.enter_every_rule(self)?;
        listener.enter_oC_Properties(self);
        Ok(())
    }
    fn exit(&self, listener: &mut (dyn CypherListener<'input> + 'a)) -> Result<(), ANTLRError> {
        listener.exit_oC_Properties(self);
        listener.exit_every_rule(self)?;
        Ok(())
    }
}

impl<'input, 'a> Visitable<dyn CypherVisitor<'input> + 'a> for OC_PropertiesContext<'input> {
    fn accept(&self, visitor: &mut (dyn CypherVisitor<'input> + 'a)) {
        visitor.visit_oC_Properties(self);
    }
}

impl<'input> CustomRuleContext<'input> for OC_PropertiesContextExt<'input> {
    type TF = LocalTokenFactory<'input>;
    type Ctx = CypherParserContextType;
    fn get_rule_index(&self) -> usize {
        RULE_oC_Properties
    }
    //fn type_rule_index() -> usize where Self: Sized { RULE_oC_Properties }
}
antlr4rust::tid! {OC_PropertiesContextExt<'a>}

impl<'input> OC_PropertiesContextExt<'input> {
    fn new(
        parent: Option<Rc<dyn CypherParserContext<'input> + 'input>>,
        invoking_state: i32,
    ) -> Rc<OC_PropertiesContextAll<'input>> {
        Rc::new(BaseParserRuleContext::new_parser_ctx(
            parent,
            invoking_state,
            OC_PropertiesContextExt { ph: PhantomData },
        ))
    }
}

pub trait OC_PropertiesContextAttrs<'input>:
    CypherParserContext<'input> + BorrowMut<OC_PropertiesContextExt<'input>>
{
    fn oC_MapLiteral(&self) -> Option<Rc<OC_MapLiteralContextAll<'input>>>
    where
        Self: Sized,
    {
        self.child_of_type(0)
    }
    fn oC_Parameter(&self) -> Option<Rc<OC_ParameterContextAll<'input>>>
    where
        Self: Sized,
    {
        self.child_of_type(0)
    }
}

impl<'input> OC_PropertiesContextAttrs<'input> for OC_PropertiesContext<'input> {}

impl<'input, I> CypherParser<'input, I>
where
    I: TokenStream<'input, TF = LocalTokenFactory<'input>> + TidAble<'input>,
{
    pub fn oC_Properties(&mut self) -> Result<Rc<OC_PropertiesContextAll<'input>>, ANTLRError> {
        let mut recog = self;
        let _parentctx = recog.ctx.take();
        let mut _localctx =
            OC_PropertiesContextExt::new(_parentctx.clone(), recog.base.get_state());
        recog
            .base
            .enter_rule(_localctx.clone(), 88, RULE_oC_Properties);
        let mut _localctx: Rc<OC_PropertiesContextAll> = _localctx;
        let result: Result<(), ANTLRError> = (|| {
            recog.base.set_state(855);
            recog.err_handler.sync(&mut recog.base)?;
            match recog.base.input.la(1) {
                Cypher_T__23 => {
                    //recog.base.enter_outer_alt(_localctx.clone(), 1)?;
                    recog.base.enter_outer_alt(None, 1)?;
                    {
                        /*InvokeRule oC_MapLiteral*/
                        recog.base.set_state(853);
                        recog.oC_MapLiteral()?;
                    }
                }

                Cypher_T__25 => {
                    //recog.base.enter_outer_alt(_localctx.clone(), 2)?;
                    recog.base.enter_outer_alt(None, 2)?;
                    {
                        /*InvokeRule oC_Parameter*/
                        recog.base.set_state(854);
                        recog.oC_Parameter()?;
                    }
                }

                _ => Err(ANTLRError::NoAltError(NoViableAltError::new(
                    &mut recog.base,
                )))?,
            }
            Ok(())
        })();
        match result {
            Ok(_) => {}
            Err(e @ ANTLRError::FallThrough(_)) => return Err(e),
            Err(ref re) => {
                //_localctx.exception = re;
                recog.err_handler.report_error(&mut recog.base, re);
                recog.err_handler.recover(&mut recog.base, re)?;
            }
        }
        recog.base.exit_rule()?;

        Ok(_localctx)
    }
}
//------------------- oC_RelationshipTypes ----------------
pub type OC_RelationshipTypesContextAll<'input> = OC_RelationshipTypesContext<'input>;

pub type OC_RelationshipTypesContext<'input> =
    BaseParserRuleContext<'input, OC_RelationshipTypesContextExt<'input>>;

#[derive(Clone)]
pub struct OC_RelationshipTypesContextExt<'input> {
    ph: PhantomData<&'input str>,
}

impl<'input> CypherParserContext<'input> for OC_RelationshipTypesContext<'input> {}

impl<'input, 'a> Listenable<dyn CypherListener<'input> + 'a>
    for OC_RelationshipTypesContext<'input>
{
    fn enter(&self, listener: &mut (dyn CypherListener<'input> + 'a)) -> Result<(), ANTLRError> {
        listener.enter_every_rule(self)?;
        listener.enter_oC_RelationshipTypes(self);
        Ok(())
    }
    fn exit(&self, listener: &mut (dyn CypherListener<'input> + 'a)) -> Result<(), ANTLRError> {
        listener.exit_oC_RelationshipTypes(self);
        listener.exit_every_rule(self)?;
        Ok(())
    }
}

impl<'input, 'a> Visitable<dyn CypherVisitor<'input> + 'a> for OC_RelationshipTypesContext<'input> {
    fn accept(&self, visitor: &mut (dyn CypherVisitor<'input> + 'a)) {
        visitor.visit_oC_RelationshipTypes(self);
    }
}

impl<'input> CustomRuleContext<'input> for OC_RelationshipTypesContextExt<'input> {
    type TF = LocalTokenFactory<'input>;
    type Ctx = CypherParserContextType;
    fn get_rule_index(&self) -> usize {
        RULE_oC_RelationshipTypes
    }
    //fn type_rule_index() -> usize where Self: Sized { RULE_oC_RelationshipTypes }
}
antlr4rust::tid! {OC_RelationshipTypesContextExt<'a>}

impl<'input> OC_RelationshipTypesContextExt<'input> {
    fn new(
        parent: Option<Rc<dyn CypherParserContext<'input> + 'input>>,
        invoking_state: i32,
    ) -> Rc<OC_RelationshipTypesContextAll<'input>> {
        Rc::new(BaseParserRuleContext::new_parser_ctx(
            parent,
            invoking_state,
            OC_RelationshipTypesContextExt { ph: PhantomData },
        ))
    }
}

pub trait OC_RelationshipTypesContextAttrs<'input>:
    CypherParserContext<'input> + BorrowMut<OC_RelationshipTypesContextExt<'input>>
{
    fn oC_RelTypeName_all(&self) -> Vec<Rc<OC_RelTypeNameContextAll<'input>>>
    where
        Self: Sized,
    {
        self.children_of_type()
    }
    fn oC_RelTypeName(&self, i: usize) -> Option<Rc<OC_RelTypeNameContextAll<'input>>>
    where
        Self: Sized,
    {
        self.child_of_type(i)
    }
    /// Retrieves all `TerminalNode`s corresponding to token SP in current rule
    fn SP_all(&self) -> Vec<Rc<TerminalNode<'input, CypherParserContextType>>>
    where
        Self: Sized,
    {
        self.children_of_type()
    }
    /// Retrieves 'i's TerminalNode corresponding to token SP, starting from 0.
    /// Returns `None` if number of children corresponding to token SP is less or equal than `i`.
    fn SP(&self, i: usize) -> Option<Rc<TerminalNode<'input, CypherParserContextType>>>
    where
        Self: Sized,
    {
        self.get_token(Cypher_SP, i)
    }
}

impl<'input> OC_RelationshipTypesContextAttrs<'input> for OC_RelationshipTypesContext<'input> {}

impl<'input, I> CypherParser<'input, I>
where
    I: TokenStream<'input, TF = LocalTokenFactory<'input>> + TidAble<'input>,
{
    pub fn oC_RelationshipTypes(
        &mut self,
    ) -> Result<Rc<OC_RelationshipTypesContextAll<'input>>, ANTLRError> {
        let mut recog = self;
        let _parentctx = recog.ctx.take();
        let mut _localctx =
            OC_RelationshipTypesContextExt::new(_parentctx.clone(), recog.base.get_state());
        recog
            .base
            .enter_rule(_localctx.clone(), 90, RULE_oC_RelationshipTypes);
        let mut _localctx: Rc<OC_RelationshipTypesContextAll> = _localctx;
        let mut _la: i32 = -1;
        let result: Result<(), ANTLRError> = (|| {
            let mut _alt: i32;
            //recog.base.enter_outer_alt(_localctx.clone(), 1)?;
            recog.base.enter_outer_alt(None, 1)?;
            {
                recog.base.set_state(857);
                recog
                    .base
                    .match_token(Cypher_T__9, &mut recog.err_handler)?;

                recog.base.set_state(859);
                recog.err_handler.sync(&mut recog.base)?;
                _la = recog.base.input.la(1);
                if _la == Cypher_SP {
                    {
                        recog.base.set_state(858);
                        recog.base.match_token(Cypher_SP, &mut recog.err_handler)?;
                    }
                }

                /*InvokeRule oC_RelTypeName*/
                recog.base.set_state(861);
                recog.oC_RelTypeName()?;

                recog.base.set_state(875);
                recog.err_handler.sync(&mut recog.base)?;
                _alt = recog.interpreter.adaptive_predict(142, &mut recog.base)?;
                while { _alt != 2 && _alt != INVALID_ALT } {
                    if _alt == 1 {
                        {
                            {
                                recog.base.set_state(863);
                                recog.err_handler.sync(&mut recog.base)?;
                                _la = recog.base.input.la(1);
                                if _la == Cypher_SP {
                                    {
                                        recog.base.set_state(862);
                                        recog
                                            .base
                                            .match_token(Cypher_SP, &mut recog.err_handler)?;
                                    }
                                }

                                recog.base.set_state(865);
                                recog
                                    .base
                                    .match_token(Cypher_T__10, &mut recog.err_handler)?;

                                recog.base.set_state(867);
                                recog.err_handler.sync(&mut recog.base)?;
                                _la = recog.base.input.la(1);
                                if _la == Cypher_T__9 {
                                    {
                                        recog.base.set_state(866);
                                        recog
                                            .base
                                            .match_token(Cypher_T__9, &mut recog.err_handler)?;
                                    }
                                }

                                recog.base.set_state(870);
                                recog.err_handler.sync(&mut recog.base)?;
                                _la = recog.base.input.la(1);
                                if _la == Cypher_SP {
                                    {
                                        recog.base.set_state(869);
                                        recog
                                            .base
                                            .match_token(Cypher_SP, &mut recog.err_handler)?;
                                    }
                                }

                                /*InvokeRule oC_RelTypeName*/
                                recog.base.set_state(872);
                                recog.oC_RelTypeName()?;
                            }
                        }
                    }
                    recog.base.set_state(877);
                    recog.err_handler.sync(&mut recog.base)?;
                    _alt = recog.interpreter.adaptive_predict(142, &mut recog.base)?;
                }
            }
            Ok(())
        })();
        match result {
            Ok(_) => {}
            Err(e @ ANTLRError::FallThrough(_)) => return Err(e),
            Err(ref re) => {
                //_localctx.exception = re;
                recog.err_handler.report_error(&mut recog.base, re);
                recog.err_handler.recover(&mut recog.base, re)?;
            }
        }
        recog.base.exit_rule()?;

        Ok(_localctx)
    }
}
//------------------- oC_NodeLabels ----------------
pub type OC_NodeLabelsContextAll<'input> = OC_NodeLabelsContext<'input>;

pub type OC_NodeLabelsContext<'input> =
    BaseParserRuleContext<'input, OC_NodeLabelsContextExt<'input>>;

#[derive(Clone)]
pub struct OC_NodeLabelsContextExt<'input> {
    ph: PhantomData<&'input str>,
}

impl<'input> CypherParserContext<'input> for OC_NodeLabelsContext<'input> {}

impl<'input, 'a> Listenable<dyn CypherListener<'input> + 'a> for OC_NodeLabelsContext<'input> {
    fn enter(&self, listener: &mut (dyn CypherListener<'input> + 'a)) -> Result<(), ANTLRError> {
        listener.enter_every_rule(self)?;
        listener.enter_oC_NodeLabels(self);
        Ok(())
    }
    fn exit(&self, listener: &mut (dyn CypherListener<'input> + 'a)) -> Result<(), ANTLRError> {
        listener.exit_oC_NodeLabels(self);
        listener.exit_every_rule(self)?;
        Ok(())
    }
}

impl<'input, 'a> Visitable<dyn CypherVisitor<'input> + 'a> for OC_NodeLabelsContext<'input> {
    fn accept(&self, visitor: &mut (dyn CypherVisitor<'input> + 'a)) {
        visitor.visit_oC_NodeLabels(self);
    }
}

impl<'input> CustomRuleContext<'input> for OC_NodeLabelsContextExt<'input> {
    type TF = LocalTokenFactory<'input>;
    type Ctx = CypherParserContextType;
    fn get_rule_index(&self) -> usize {
        RULE_oC_NodeLabels
    }
    //fn type_rule_index() -> usize where Self: Sized { RULE_oC_NodeLabels }
}
antlr4rust::tid! {OC_NodeLabelsContextExt<'a>}

impl<'input> OC_NodeLabelsContextExt<'input> {
    fn new(
        parent: Option<Rc<dyn CypherParserContext<'input> + 'input>>,
        invoking_state: i32,
    ) -> Rc<OC_NodeLabelsContextAll<'input>> {
        Rc::new(BaseParserRuleContext::new_parser_ctx(
            parent,
            invoking_state,
            OC_NodeLabelsContextExt { ph: PhantomData },
        ))
    }
}

pub trait OC_NodeLabelsContextAttrs<'input>:
    CypherParserContext<'input> + BorrowMut<OC_NodeLabelsContextExt<'input>>
{
    fn oC_NodeLabel_all(&self) -> Vec<Rc<OC_NodeLabelContextAll<'input>>>
    where
        Self: Sized,
    {
        self.children_of_type()
    }
    fn oC_NodeLabel(&self, i: usize) -> Option<Rc<OC_NodeLabelContextAll<'input>>>
    where
        Self: Sized,
    {
        self.child_of_type(i)
    }
    /// Retrieves all `TerminalNode`s corresponding to token SP in current rule
    fn SP_all(&self) -> Vec<Rc<TerminalNode<'input, CypherParserContextType>>>
    where
        Self: Sized,
    {
        self.children_of_type()
    }
    /// Retrieves 'i's TerminalNode corresponding to token SP, starting from 0.
    /// Returns `None` if number of children corresponding to token SP is less or equal than `i`.
    fn SP(&self, i: usize) -> Option<Rc<TerminalNode<'input, CypherParserContextType>>>
    where
        Self: Sized,
    {
        self.get_token(Cypher_SP, i)
    }
}

impl<'input> OC_NodeLabelsContextAttrs<'input> for OC_NodeLabelsContext<'input> {}

impl<'input, I> CypherParser<'input, I>
where
    I: TokenStream<'input, TF = LocalTokenFactory<'input>> + TidAble<'input>,
{
    pub fn oC_NodeLabels(&mut self) -> Result<Rc<OC_NodeLabelsContextAll<'input>>, ANTLRError> {
        let mut recog = self;
        let _parentctx = recog.ctx.take();
        let mut _localctx =
            OC_NodeLabelsContextExt::new(_parentctx.clone(), recog.base.get_state());
        recog
            .base
            .enter_rule(_localctx.clone(), 92, RULE_oC_NodeLabels);
        let mut _localctx: Rc<OC_NodeLabelsContextAll> = _localctx;
        let mut _la: i32 = -1;
        let result: Result<(), ANTLRError> = (|| {
            let mut _alt: i32;
            //recog.base.enter_outer_alt(_localctx.clone(), 1)?;
            recog.base.enter_outer_alt(None, 1)?;
            {
                /*InvokeRule oC_NodeLabel*/
                recog.base.set_state(878);
                recog.oC_NodeLabel()?;

                recog.base.set_state(885);
                recog.err_handler.sync(&mut recog.base)?;
                _alt = recog.interpreter.adaptive_predict(144, &mut recog.base)?;
                while { _alt != 2 && _alt != INVALID_ALT } {
                    if _alt == 1 {
                        {
                            {
                                recog.base.set_state(880);
                                recog.err_handler.sync(&mut recog.base)?;
                                _la = recog.base.input.la(1);
                                if _la == Cypher_SP {
                                    {
                                        recog.base.set_state(879);
                                        recog
                                            .base
                                            .match_token(Cypher_SP, &mut recog.err_handler)?;
                                    }
                                }

                                /*InvokeRule oC_NodeLabel*/
                                recog.base.set_state(882);
                                recog.oC_NodeLabel()?;
                            }
                        }
                    }
                    recog.base.set_state(887);
                    recog.err_handler.sync(&mut recog.base)?;
                    _alt = recog.interpreter.adaptive_predict(144, &mut recog.base)?;
                }
            }
            Ok(())
        })();
        match result {
            Ok(_) => {}
            Err(e @ ANTLRError::FallThrough(_)) => return Err(e),
            Err(ref re) => {
                //_localctx.exception = re;
                recog.err_handler.report_error(&mut recog.base, re);
                recog.err_handler.recover(&mut recog.base, re)?;
            }
        }
        recog.base.exit_rule()?;

        Ok(_localctx)
    }
}
//------------------- oC_NodeLabel ----------------
pub type OC_NodeLabelContextAll<'input> = OC_NodeLabelContext<'input>;

pub type OC_NodeLabelContext<'input> =
    BaseParserRuleContext<'input, OC_NodeLabelContextExt<'input>>;

#[derive(Clone)]
pub struct OC_NodeLabelContextExt<'input> {
    ph: PhantomData<&'input str>,
}

impl<'input> CypherParserContext<'input> for OC_NodeLabelContext<'input> {}

impl<'input, 'a> Listenable<dyn CypherListener<'input> + 'a> for OC_NodeLabelContext<'input> {
    fn enter(&self, listener: &mut (dyn CypherListener<'input> + 'a)) -> Result<(), ANTLRError> {
        listener.enter_every_rule(self)?;
        listener.enter_oC_NodeLabel(self);
        Ok(())
    }
    fn exit(&self, listener: &mut (dyn CypherListener<'input> + 'a)) -> Result<(), ANTLRError> {
        listener.exit_oC_NodeLabel(self);
        listener.exit_every_rule(self)?;
        Ok(())
    }
}

impl<'input, 'a> Visitable<dyn CypherVisitor<'input> + 'a> for OC_NodeLabelContext<'input> {
    fn accept(&self, visitor: &mut (dyn CypherVisitor<'input> + 'a)) {
        visitor.visit_oC_NodeLabel(self);
    }
}

impl<'input> CustomRuleContext<'input> for OC_NodeLabelContextExt<'input> {
    type TF = LocalTokenFactory<'input>;
    type Ctx = CypherParserContextType;
    fn get_rule_index(&self) -> usize {
        RULE_oC_NodeLabel
    }
    //fn type_rule_index() -> usize where Self: Sized { RULE_oC_NodeLabel }
}
antlr4rust::tid! {OC_NodeLabelContextExt<'a>}

impl<'input> OC_NodeLabelContextExt<'input> {
    fn new(
        parent: Option<Rc<dyn CypherParserContext<'input> + 'input>>,
        invoking_state: i32,
    ) -> Rc<OC_NodeLabelContextAll<'input>> {
        Rc::new(BaseParserRuleContext::new_parser_ctx(
            parent,
            invoking_state,
            OC_NodeLabelContextExt { ph: PhantomData },
        ))
    }
}

pub trait OC_NodeLabelContextAttrs<'input>:
    CypherParserContext<'input> + BorrowMut<OC_NodeLabelContextExt<'input>>
{
    fn oC_LabelName_all(&self) -> Vec<Rc<OC_LabelNameContextAll<'input>>>
    where
        Self: Sized,
    {
        self.children_of_type()
    }
    fn oC_LabelName(&self, i: usize) -> Option<Rc<OC_LabelNameContextAll<'input>>>
    where
        Self: Sized,
    {
        self.child_of_type(i)
    }
    /// Retrieves all `TerminalNode`s corresponding to token SP in current rule
    fn SP_all(&self) -> Vec<Rc<TerminalNode<'input, CypherParserContextType>>>
    where
        Self: Sized,
    {
        self.children_of_type()
    }
    /// Retrieves 'i's TerminalNode corresponding to token SP, starting from 0.
    /// Returns `None` if number of children corresponding to token SP is less or equal than `i`.
    fn SP(&self, i: usize) -> Option<Rc<TerminalNode<'input, CypherParserContextType>>>
    where
        Self: Sized,
    {
        self.get_token(Cypher_SP, i)
    }
}

impl<'input> OC_NodeLabelContextAttrs<'input> for OC_NodeLabelContext<'input> {}

impl<'input, I> CypherParser<'input, I>
where
    I: TokenStream<'input, TF = LocalTokenFactory<'input>> + TidAble<'input>,
{
    pub fn oC_NodeLabel(&mut self) -> Result<Rc<OC_NodeLabelContextAll<'input>>, ANTLRError> {
        let mut recog = self;
        let _parentctx = recog.ctx.take();
        let mut _localctx = OC_NodeLabelContextExt::new(_parentctx.clone(), recog.base.get_state());
        recog
            .base
            .enter_rule(_localctx.clone(), 94, RULE_oC_NodeLabel);
        let mut _localctx: Rc<OC_NodeLabelContextAll> = _localctx;
        let mut _la: i32 = -1;
        let result: Result<(), ANTLRError> = (|| {
            let mut _alt: i32;
            //recog.base.enter_outer_alt(_localctx.clone(), 1)?;
            recog.base.enter_outer_alt(None, 1)?;
            {
                recog.base.set_state(888);
                recog
                    .base
                    .match_token(Cypher_T__9, &mut recog.err_handler)?;

                recog.base.set_state(890);
                recog.err_handler.sync(&mut recog.base)?;
                _la = recog.base.input.la(1);
                if _la == Cypher_SP {
                    {
                        recog.base.set_state(889);
                        recog.base.match_token(Cypher_SP, &mut recog.err_handler)?;
                    }
                }

                /*InvokeRule oC_LabelName*/
                recog.base.set_state(892);
                recog.oC_LabelName()?;

                recog.base.set_state(906);
                recog.err_handler.sync(&mut recog.base)?;
                _alt = recog.interpreter.adaptive_predict(149, &mut recog.base)?;
                while { _alt != 2 && _alt != INVALID_ALT } {
                    if _alt == 1 {
                        {
                            {
                                recog.base.set_state(894);
                                recog.err_handler.sync(&mut recog.base)?;
                                _la = recog.base.input.la(1);
                                if _la == Cypher_SP {
                                    {
                                        recog.base.set_state(893);
                                        recog
                                            .base
                                            .match_token(Cypher_SP, &mut recog.err_handler)?;
                                    }
                                }

                                recog.base.set_state(896);
                                recog
                                    .base
                                    .match_token(Cypher_T__10, &mut recog.err_handler)?;

                                recog.base.set_state(898);
                                recog.err_handler.sync(&mut recog.base)?;
                                _la = recog.base.input.la(1);
                                if _la == Cypher_T__9 {
                                    {
                                        recog.base.set_state(897);
                                        recog
                                            .base
                                            .match_token(Cypher_T__9, &mut recog.err_handler)?;
                                    }
                                }

                                recog.base.set_state(901);
                                recog.err_handler.sync(&mut recog.base)?;
                                _la = recog.base.input.la(1);
                                if _la == Cypher_SP {
                                    {
                                        recog.base.set_state(900);
                                        recog
                                            .base
                                            .match_token(Cypher_SP, &mut recog.err_handler)?;
                                    }
                                }

                                /*InvokeRule oC_LabelName*/
                                recog.base.set_state(903);
                                recog.oC_LabelName()?;
                            }
                        }
                    }
                    recog.base.set_state(908);
                    recog.err_handler.sync(&mut recog.base)?;
                    _alt = recog.interpreter.adaptive_predict(149, &mut recog.base)?;
                }
            }
            Ok(())
        })();
        match result {
            Ok(_) => {}
            Err(e @ ANTLRError::FallThrough(_)) => return Err(e),
            Err(ref re) => {
                //_localctx.exception = re;
                recog.err_handler.report_error(&mut recog.base, re);
                recog.err_handler.recover(&mut recog.base, re)?;
            }
        }
        recog.base.exit_rule()?;

        Ok(_localctx)
    }
}
//------------------- oC_RangeLiteral ----------------
pub type OC_RangeLiteralContextAll<'input> = OC_RangeLiteralContext<'input>;

pub type OC_RangeLiteralContext<'input> =
    BaseParserRuleContext<'input, OC_RangeLiteralContextExt<'input>>;

#[derive(Clone)]
pub struct OC_RangeLiteralContextExt<'input> {
    ph: PhantomData<&'input str>,
}

impl<'input> CypherParserContext<'input> for OC_RangeLiteralContext<'input> {}

impl<'input, 'a> Listenable<dyn CypherListener<'input> + 'a> for OC_RangeLiteralContext<'input> {
    fn enter(&self, listener: &mut (dyn CypherListener<'input> + 'a)) -> Result<(), ANTLRError> {
        listener.enter_every_rule(self)?;
        listener.enter_oC_RangeLiteral(self);
        Ok(())
    }
    fn exit(&self, listener: &mut (dyn CypherListener<'input> + 'a)) -> Result<(), ANTLRError> {
        listener.exit_oC_RangeLiteral(self);
        listener.exit_every_rule(self)?;
        Ok(())
    }
}

impl<'input, 'a> Visitable<dyn CypherVisitor<'input> + 'a> for OC_RangeLiteralContext<'input> {
    fn accept(&self, visitor: &mut (dyn CypherVisitor<'input> + 'a)) {
        visitor.visit_oC_RangeLiteral(self);
    }
}

impl<'input> CustomRuleContext<'input> for OC_RangeLiteralContextExt<'input> {
    type TF = LocalTokenFactory<'input>;
    type Ctx = CypherParserContextType;
    fn get_rule_index(&self) -> usize {
        RULE_oC_RangeLiteral
    }
    //fn type_rule_index() -> usize where Self: Sized { RULE_oC_RangeLiteral }
}
antlr4rust::tid! {OC_RangeLiteralContextExt<'a>}

impl<'input> OC_RangeLiteralContextExt<'input> {
    fn new(
        parent: Option<Rc<dyn CypherParserContext<'input> + 'input>>,
        invoking_state: i32,
    ) -> Rc<OC_RangeLiteralContextAll<'input>> {
        Rc::new(BaseParserRuleContext::new_parser_ctx(
            parent,
            invoking_state,
            OC_RangeLiteralContextExt { ph: PhantomData },
        ))
    }
}

pub trait OC_RangeLiteralContextAttrs<'input>:
    CypherParserContext<'input> + BorrowMut<OC_RangeLiteralContextExt<'input>>
{
    /// Retrieves all `TerminalNode`s corresponding to token SP in current rule
    fn SP_all(&self) -> Vec<Rc<TerminalNode<'input, CypherParserContextType>>>
    where
        Self: Sized,
    {
        self.children_of_type()
    }
    /// Retrieves 'i's TerminalNode corresponding to token SP, starting from 0.
    /// Returns `None` if number of children corresponding to token SP is less or equal than `i`.
    fn SP(&self, i: usize) -> Option<Rc<TerminalNode<'input, CypherParserContextType>>>
    where
        Self: Sized,
    {
        self.get_token(Cypher_SP, i)
    }
    fn oC_IntegerLiteral_all(&self) -> Vec<Rc<OC_IntegerLiteralContextAll<'input>>>
    where
        Self: Sized,
    {
        self.children_of_type()
    }
    fn oC_IntegerLiteral(&self, i: usize) -> Option<Rc<OC_IntegerLiteralContextAll<'input>>>
    where
        Self: Sized,
    {
        self.child_of_type(i)
    }
}

impl<'input> OC_RangeLiteralContextAttrs<'input> for OC_RangeLiteralContext<'input> {}

impl<'input, I> CypherParser<'input, I>
where
    I: TokenStream<'input, TF = LocalTokenFactory<'input>> + TidAble<'input>,
{
    pub fn oC_RangeLiteral(&mut self) -> Result<Rc<OC_RangeLiteralContextAll<'input>>, ANTLRError> {
        let mut recog = self;
        let _parentctx = recog.ctx.take();
        let mut _localctx =
            OC_RangeLiteralContextExt::new(_parentctx.clone(), recog.base.get_state());
        recog
            .base
            .enter_rule(_localctx.clone(), 96, RULE_oC_RangeLiteral);
        let mut _localctx: Rc<OC_RangeLiteralContextAll> = _localctx;
        let mut _la: i32 = -1;
        let result: Result<(), ANTLRError> = (|| {
            //recog.base.enter_outer_alt(_localctx.clone(), 1)?;
            recog.base.enter_outer_alt(None, 1)?;
            {
                recog.base.set_state(909);
                recog
                    .base
                    .match_token(Cypher_T__4, &mut recog.err_handler)?;

                recog.base.set_state(911);
                recog.err_handler.sync(&mut recog.base)?;
                match recog.interpreter.adaptive_predict(150, &mut recog.base)? {
                    x if x == 1 => {
                        recog.base.set_state(910);
                        recog.base.match_token(Cypher_SP, &mut recog.err_handler)?;
                    }

                    _ => {}
                }
                recog.base.set_state(917);
                recog.err_handler.sync(&mut recog.base)?;
                _la = recog.base.input.la(1);
                if (((_la - 96) & !0x3f) == 0 && ((1usize << (_la - 96)) & 7) != 0) {
                    {
                        /*InvokeRule oC_IntegerLiteral*/
                        recog.base.set_state(913);
                        recog.oC_IntegerLiteral()?;

                        recog.base.set_state(915);
                        recog.err_handler.sync(&mut recog.base)?;
                        match recog.interpreter.adaptive_predict(151, &mut recog.base)? {
                            x if x == 1 => {
                                recog.base.set_state(914);
                                recog.base.match_token(Cypher_SP, &mut recog.err_handler)?;
                            }

                            _ => {}
                        }
                    }
                }

                recog.base.set_state(929);
                recog.err_handler.sync(&mut recog.base)?;
                _la = recog.base.input.la(1);
                if _la == Cypher_T__11 {
                    {
                        recog.base.set_state(919);
                        recog
                            .base
                            .match_token(Cypher_T__11, &mut recog.err_handler)?;

                        recog.base.set_state(921);
                        recog.err_handler.sync(&mut recog.base)?;
                        match recog.interpreter.adaptive_predict(153, &mut recog.base)? {
                            x if x == 1 => {
                                recog.base.set_state(920);
                                recog.base.match_token(Cypher_SP, &mut recog.err_handler)?;
                            }

                            _ => {}
                        }
                        recog.base.set_state(927);
                        recog.err_handler.sync(&mut recog.base)?;
                        _la = recog.base.input.la(1);
                        if (((_la - 96) & !0x3f) == 0 && ((1usize << (_la - 96)) & 7) != 0) {
                            {
                                /*InvokeRule oC_IntegerLiteral*/
                                recog.base.set_state(923);
                                recog.oC_IntegerLiteral()?;

                                recog.base.set_state(925);
                                recog.err_handler.sync(&mut recog.base)?;
                                match recog.interpreter.adaptive_predict(154, &mut recog.base)? {
                                    x if x == 1 => {
                                        recog.base.set_state(924);
                                        recog
                                            .base
                                            .match_token(Cypher_SP, &mut recog.err_handler)?;
                                    }

                                    _ => {}
                                }
                            }
                        }
                    }
                }
            }
            Ok(())
        })();
        match result {
            Ok(_) => {}
            Err(e @ ANTLRError::FallThrough(_)) => return Err(e),
            Err(ref re) => {
                //_localctx.exception = re;
                recog.err_handler.report_error(&mut recog.base, re);
                recog.err_handler.recover(&mut recog.base, re)?;
            }
        }
        recog.base.exit_rule()?;

        Ok(_localctx)
    }
}
//------------------- oC_LabelName ----------------
pub type OC_LabelNameContextAll<'input> = OC_LabelNameContext<'input>;

pub type OC_LabelNameContext<'input> =
    BaseParserRuleContext<'input, OC_LabelNameContextExt<'input>>;

#[derive(Clone)]
pub struct OC_LabelNameContextExt<'input> {
    ph: PhantomData<&'input str>,
}

impl<'input> CypherParserContext<'input> for OC_LabelNameContext<'input> {}

impl<'input, 'a> Listenable<dyn CypherListener<'input> + 'a> for OC_LabelNameContext<'input> {
    fn enter(&self, listener: &mut (dyn CypherListener<'input> + 'a)) -> Result<(), ANTLRError> {
        listener.enter_every_rule(self)?;
        listener.enter_oC_LabelName(self);
        Ok(())
    }
    fn exit(&self, listener: &mut (dyn CypherListener<'input> + 'a)) -> Result<(), ANTLRError> {
        listener.exit_oC_LabelName(self);
        listener.exit_every_rule(self)?;
        Ok(())
    }
}

impl<'input, 'a> Visitable<dyn CypherVisitor<'input> + 'a> for OC_LabelNameContext<'input> {
    fn accept(&self, visitor: &mut (dyn CypherVisitor<'input> + 'a)) {
        visitor.visit_oC_LabelName(self);
    }
}

impl<'input> CustomRuleContext<'input> for OC_LabelNameContextExt<'input> {
    type TF = LocalTokenFactory<'input>;
    type Ctx = CypherParserContextType;
    fn get_rule_index(&self) -> usize {
        RULE_oC_LabelName
    }
    //fn type_rule_index() -> usize where Self: Sized { RULE_oC_LabelName }
}
antlr4rust::tid! {OC_LabelNameContextExt<'a>}

impl<'input> OC_LabelNameContextExt<'input> {
    fn new(
        parent: Option<Rc<dyn CypherParserContext<'input> + 'input>>,
        invoking_state: i32,
    ) -> Rc<OC_LabelNameContextAll<'input>> {
        Rc::new(BaseParserRuleContext::new_parser_ctx(
            parent,
            invoking_state,
            OC_LabelNameContextExt { ph: PhantomData },
        ))
    }
}

pub trait OC_LabelNameContextAttrs<'input>:
    CypherParserContext<'input> + BorrowMut<OC_LabelNameContextExt<'input>>
{
    fn oC_SchemaName(&self) -> Option<Rc<OC_SchemaNameContextAll<'input>>>
    where
        Self: Sized,
    {
        self.child_of_type(0)
    }
}

impl<'input> OC_LabelNameContextAttrs<'input> for OC_LabelNameContext<'input> {}

impl<'input, I> CypherParser<'input, I>
where
    I: TokenStream<'input, TF = LocalTokenFactory<'input>> + TidAble<'input>,
{
    pub fn oC_LabelName(&mut self) -> Result<Rc<OC_LabelNameContextAll<'input>>, ANTLRError> {
        let mut recog = self;
        let _parentctx = recog.ctx.take();
        let mut _localctx = OC_LabelNameContextExt::new(_parentctx.clone(), recog.base.get_state());
        recog
            .base
            .enter_rule(_localctx.clone(), 98, RULE_oC_LabelName);
        let mut _localctx: Rc<OC_LabelNameContextAll> = _localctx;
        let result: Result<(), ANTLRError> = (|| {
            //recog.base.enter_outer_alt(_localctx.clone(), 1)?;
            recog.base.enter_outer_alt(None, 1)?;
            {
                /*InvokeRule oC_SchemaName*/
                recog.base.set_state(931);
                recog.oC_SchemaName()?;
            }
            Ok(())
        })();
        match result {
            Ok(_) => {}
            Err(e @ ANTLRError::FallThrough(_)) => return Err(e),
            Err(ref re) => {
                //_localctx.exception = re;
                recog.err_handler.report_error(&mut recog.base, re);
                recog.err_handler.recover(&mut recog.base, re)?;
            }
        }
        recog.base.exit_rule()?;

        Ok(_localctx)
    }
}
//------------------- oC_RelTypeName ----------------
pub type OC_RelTypeNameContextAll<'input> = OC_RelTypeNameContext<'input>;

pub type OC_RelTypeNameContext<'input> =
    BaseParserRuleContext<'input, OC_RelTypeNameContextExt<'input>>;

#[derive(Clone)]
pub struct OC_RelTypeNameContextExt<'input> {
    ph: PhantomData<&'input str>,
}

impl<'input> CypherParserContext<'input> for OC_RelTypeNameContext<'input> {}

impl<'input, 'a> Listenable<dyn CypherListener<'input> + 'a> for OC_RelTypeNameContext<'input> {
    fn enter(&self, listener: &mut (dyn CypherListener<'input> + 'a)) -> Result<(), ANTLRError> {
        listener.enter_every_rule(self)?;
        listener.enter_oC_RelTypeName(self);
        Ok(())
    }
    fn exit(&self, listener: &mut (dyn CypherListener<'input> + 'a)) -> Result<(), ANTLRError> {
        listener.exit_oC_RelTypeName(self);
        listener.exit_every_rule(self)?;
        Ok(())
    }
}

impl<'input, 'a> Visitable<dyn CypherVisitor<'input> + 'a> for OC_RelTypeNameContext<'input> {
    fn accept(&self, visitor: &mut (dyn CypherVisitor<'input> + 'a)) {
        visitor.visit_oC_RelTypeName(self);
    }
}

impl<'input> CustomRuleContext<'input> for OC_RelTypeNameContextExt<'input> {
    type TF = LocalTokenFactory<'input>;
    type Ctx = CypherParserContextType;
    fn get_rule_index(&self) -> usize {
        RULE_oC_RelTypeName
    }
    //fn type_rule_index() -> usize where Self: Sized { RULE_oC_RelTypeName }
}
antlr4rust::tid! {OC_RelTypeNameContextExt<'a>}

impl<'input> OC_RelTypeNameContextExt<'input> {
    fn new(
        parent: Option<Rc<dyn CypherParserContext<'input> + 'input>>,
        invoking_state: i32,
    ) -> Rc<OC_RelTypeNameContextAll<'input>> {
        Rc::new(BaseParserRuleContext::new_parser_ctx(
            parent,
            invoking_state,
            OC_RelTypeNameContextExt { ph: PhantomData },
        ))
    }
}

pub trait OC_RelTypeNameContextAttrs<'input>:
    CypherParserContext<'input> + BorrowMut<OC_RelTypeNameContextExt<'input>>
{
    fn oC_SchemaName(&self) -> Option<Rc<OC_SchemaNameContextAll<'input>>>
    where
        Self: Sized,
    {
        self.child_of_type(0)
    }
}

impl<'input> OC_RelTypeNameContextAttrs<'input> for OC_RelTypeNameContext<'input> {}

impl<'input, I> CypherParser<'input, I>
where
    I: TokenStream<'input, TF = LocalTokenFactory<'input>> + TidAble<'input>,
{
    pub fn oC_RelTypeName(&mut self) -> Result<Rc<OC_RelTypeNameContextAll<'input>>, ANTLRError> {
        let mut recog = self;
        let _parentctx = recog.ctx.take();
        let mut _localctx =
            OC_RelTypeNameContextExt::new(_parentctx.clone(), recog.base.get_state());
        recog
            .base
            .enter_rule(_localctx.clone(), 100, RULE_oC_RelTypeName);
        let mut _localctx: Rc<OC_RelTypeNameContextAll> = _localctx;
        let result: Result<(), ANTLRError> = (|| {
            //recog.base.enter_outer_alt(_localctx.clone(), 1)?;
            recog.base.enter_outer_alt(None, 1)?;
            {
                /*InvokeRule oC_SchemaName*/
                recog.base.set_state(933);
                recog.oC_SchemaName()?;
            }
            Ok(())
        })();
        match result {
            Ok(_) => {}
            Err(e @ ANTLRError::FallThrough(_)) => return Err(e),
            Err(ref re) => {
                //_localctx.exception = re;
                recog.err_handler.report_error(&mut recog.base, re);
                recog.err_handler.recover(&mut recog.base, re)?;
            }
        }
        recog.base.exit_rule()?;

        Ok(_localctx)
    }
}
//------------------- oC_PropertyExpression ----------------
pub type OC_PropertyExpressionContextAll<'input> = OC_PropertyExpressionContext<'input>;

pub type OC_PropertyExpressionContext<'input> =
    BaseParserRuleContext<'input, OC_PropertyExpressionContextExt<'input>>;

#[derive(Clone)]
pub struct OC_PropertyExpressionContextExt<'input> {
    ph: PhantomData<&'input str>,
}

impl<'input> CypherParserContext<'input> for OC_PropertyExpressionContext<'input> {}

impl<'input, 'a> Listenable<dyn CypherListener<'input> + 'a>
    for OC_PropertyExpressionContext<'input>
{
    fn enter(&self, listener: &mut (dyn CypherListener<'input> + 'a)) -> Result<(), ANTLRError> {
        listener.enter_every_rule(self)?;
        listener.enter_oC_PropertyExpression(self);
        Ok(())
    }
    fn exit(&self, listener: &mut (dyn CypherListener<'input> + 'a)) -> Result<(), ANTLRError> {
        listener.exit_oC_PropertyExpression(self);
        listener.exit_every_rule(self)?;
        Ok(())
    }
}

impl<'input, 'a> Visitable<dyn CypherVisitor<'input> + 'a>
    for OC_PropertyExpressionContext<'input>
{
    fn accept(&self, visitor: &mut (dyn CypherVisitor<'input> + 'a)) {
        visitor.visit_oC_PropertyExpression(self);
    }
}

impl<'input> CustomRuleContext<'input> for OC_PropertyExpressionContextExt<'input> {
    type TF = LocalTokenFactory<'input>;
    type Ctx = CypherParserContextType;
    fn get_rule_index(&self) -> usize {
        RULE_oC_PropertyExpression
    }
    //fn type_rule_index() -> usize where Self: Sized { RULE_oC_PropertyExpression }
}
antlr4rust::tid! {OC_PropertyExpressionContextExt<'a>}

impl<'input> OC_PropertyExpressionContextExt<'input> {
    fn new(
        parent: Option<Rc<dyn CypherParserContext<'input> + 'input>>,
        invoking_state: i32,
    ) -> Rc<OC_PropertyExpressionContextAll<'input>> {
        Rc::new(BaseParserRuleContext::new_parser_ctx(
            parent,
            invoking_state,
            OC_PropertyExpressionContextExt { ph: PhantomData },
        ))
    }
}

pub trait OC_PropertyExpressionContextAttrs<'input>:
    CypherParserContext<'input> + BorrowMut<OC_PropertyExpressionContextExt<'input>>
{
    fn oC_Atom(&self) -> Option<Rc<OC_AtomContextAll<'input>>>
    where
        Self: Sized,
    {
        self.child_of_type(0)
    }
    fn oC_PropertyLookup_all(&self) -> Vec<Rc<OC_PropertyLookupContextAll<'input>>>
    where
        Self: Sized,
    {
        self.children_of_type()
    }
    fn oC_PropertyLookup(&self, i: usize) -> Option<Rc<OC_PropertyLookupContextAll<'input>>>
    where
        Self: Sized,
    {
        self.child_of_type(i)
    }
    /// Retrieves all `TerminalNode`s corresponding to token SP in current rule
    fn SP_all(&self) -> Vec<Rc<TerminalNode<'input, CypherParserContextType>>>
    where
        Self: Sized,
    {
        self.children_of_type()
    }
    /// Retrieves 'i's TerminalNode corresponding to token SP, starting from 0.
    /// Returns `None` if number of children corresponding to token SP is less or equal than `i`.
    fn SP(&self, i: usize) -> Option<Rc<TerminalNode<'input, CypherParserContextType>>>
    where
        Self: Sized,
    {
        self.get_token(Cypher_SP, i)
    }
}

impl<'input> OC_PropertyExpressionContextAttrs<'input> for OC_PropertyExpressionContext<'input> {}

impl<'input, I> CypherParser<'input, I>
where
    I: TokenStream<'input, TF = LocalTokenFactory<'input>> + TidAble<'input>,
{
    pub fn oC_PropertyExpression(
        &mut self,
    ) -> Result<Rc<OC_PropertyExpressionContextAll<'input>>, ANTLRError> {
        let mut recog = self;
        let _parentctx = recog.ctx.take();
        let mut _localctx =
            OC_PropertyExpressionContextExt::new(_parentctx.clone(), recog.base.get_state());
        recog
            .base
            .enter_rule(_localctx.clone(), 102, RULE_oC_PropertyExpression);
        let mut _localctx: Rc<OC_PropertyExpressionContextAll> = _localctx;
        let mut _la: i32 = -1;
        let result: Result<(), ANTLRError> = (|| {
            let mut _alt: i32;
            //recog.base.enter_outer_alt(_localctx.clone(), 1)?;
            recog.base.enter_outer_alt(None, 1)?;
            {
                /*InvokeRule oC_Atom*/
                recog.base.set_state(935);
                recog.oC_Atom()?;

                recog.base.set_state(940);
                recog.err_handler.sync(&mut recog.base)?;
                _alt = 1;
                loop {
                    match _alt {
                        x if x == 1 => {
                            {
                                recog.base.set_state(937);
                                recog.err_handler.sync(&mut recog.base)?;
                                _la = recog.base.input.la(1);
                                if _la == Cypher_SP {
                                    {
                                        recog.base.set_state(936);
                                        recog
                                            .base
                                            .match_token(Cypher_SP, &mut recog.err_handler)?;
                                    }
                                }

                                /*InvokeRule oC_PropertyLookup*/
                                recog.base.set_state(939);
                                recog.oC_PropertyLookup()?;
                            }
                        }

                        _ => Err(ANTLRError::NoAltError(NoViableAltError::new(
                            &mut recog.base,
                        )))?,
                    }
                    recog.base.set_state(942);
                    recog.err_handler.sync(&mut recog.base)?;
                    _alt = recog.interpreter.adaptive_predict(158, &mut recog.base)?;
                    if _alt == 2 || _alt == INVALID_ALT {
                        break;
                    }
                }
            }
            Ok(())
        })();
        match result {
            Ok(_) => {}
            Err(e @ ANTLRError::FallThrough(_)) => return Err(e),
            Err(ref re) => {
                //_localctx.exception = re;
                recog.err_handler.report_error(&mut recog.base, re);
                recog.err_handler.recover(&mut recog.base, re)?;
            }
        }
        recog.base.exit_rule()?;

        Ok(_localctx)
    }
}
//------------------- oC_Expression ----------------
pub type OC_ExpressionContextAll<'input> = OC_ExpressionContext<'input>;

pub type OC_ExpressionContext<'input> =
    BaseParserRuleContext<'input, OC_ExpressionContextExt<'input>>;

#[derive(Clone)]
pub struct OC_ExpressionContextExt<'input> {
    ph: PhantomData<&'input str>,
}

impl<'input> CypherParserContext<'input> for OC_ExpressionContext<'input> {}

impl<'input, 'a> Listenable<dyn CypherListener<'input> + 'a> for OC_ExpressionContext<'input> {
    fn enter(&self, listener: &mut (dyn CypherListener<'input> + 'a)) -> Result<(), ANTLRError> {
        listener.enter_every_rule(self)?;
        listener.enter_oC_Expression(self);
        Ok(())
    }
    fn exit(&self, listener: &mut (dyn CypherListener<'input> + 'a)) -> Result<(), ANTLRError> {
        listener.exit_oC_Expression(self);
        listener.exit_every_rule(self)?;
        Ok(())
    }
}

impl<'input, 'a> Visitable<dyn CypherVisitor<'input> + 'a> for OC_ExpressionContext<'input> {
    fn accept(&self, visitor: &mut (dyn CypherVisitor<'input> + 'a)) {
        visitor.visit_oC_Expression(self);
    }
}

impl<'input> CustomRuleContext<'input> for OC_ExpressionContextExt<'input> {
    type TF = LocalTokenFactory<'input>;
    type Ctx = CypherParserContextType;
    fn get_rule_index(&self) -> usize {
        RULE_oC_Expression
    }
    //fn type_rule_index() -> usize where Self: Sized { RULE_oC_Expression }
}
antlr4rust::tid! {OC_ExpressionContextExt<'a>}

impl<'input> OC_ExpressionContextExt<'input> {
    fn new(
        parent: Option<Rc<dyn CypherParserContext<'input> + 'input>>,
        invoking_state: i32,
    ) -> Rc<OC_ExpressionContextAll<'input>> {
        Rc::new(BaseParserRuleContext::new_parser_ctx(
            parent,
            invoking_state,
            OC_ExpressionContextExt { ph: PhantomData },
        ))
    }
}

pub trait OC_ExpressionContextAttrs<'input>:
    CypherParserContext<'input> + BorrowMut<OC_ExpressionContextExt<'input>>
{
    fn oC_OrExpression(&self) -> Option<Rc<OC_OrExpressionContextAll<'input>>>
    where
        Self: Sized,
    {
        self.child_of_type(0)
    }
}

impl<'input> OC_ExpressionContextAttrs<'input> for OC_ExpressionContext<'input> {}

impl<'input, I> CypherParser<'input, I>
where
    I: TokenStream<'input, TF = LocalTokenFactory<'input>> + TidAble<'input>,
{
    pub fn oC_Expression(&mut self) -> Result<Rc<OC_ExpressionContextAll<'input>>, ANTLRError> {
        let mut recog = self;
        let _parentctx = recog.ctx.take();
        let mut _localctx =
            OC_ExpressionContextExt::new(_parentctx.clone(), recog.base.get_state());
        recog
            .base
            .enter_rule(_localctx.clone(), 104, RULE_oC_Expression);
        let mut _localctx: Rc<OC_ExpressionContextAll> = _localctx;
        let result: Result<(), ANTLRError> = (|| {
            //recog.base.enter_outer_alt(_localctx.clone(), 1)?;
            recog.base.enter_outer_alt(None, 1)?;
            {
                /*InvokeRule oC_OrExpression*/
                recog.base.set_state(944);
                recog.oC_OrExpression()?;
            }
            Ok(())
        })();
        match result {
            Ok(_) => {}
            Err(e @ ANTLRError::FallThrough(_)) => return Err(e),
            Err(ref re) => {
                //_localctx.exception = re;
                recog.err_handler.report_error(&mut recog.base, re);
                recog.err_handler.recover(&mut recog.base, re)?;
            }
        }
        recog.base.exit_rule()?;

        Ok(_localctx)
    }
}
//------------------- oC_OrExpression ----------------
pub type OC_OrExpressionContextAll<'input> = OC_OrExpressionContext<'input>;

pub type OC_OrExpressionContext<'input> =
    BaseParserRuleContext<'input, OC_OrExpressionContextExt<'input>>;

#[derive(Clone)]
pub struct OC_OrExpressionContextExt<'input> {
    ph: PhantomData<&'input str>,
}

impl<'input> CypherParserContext<'input> for OC_OrExpressionContext<'input> {}

impl<'input, 'a> Listenable<dyn CypherListener<'input> + 'a> for OC_OrExpressionContext<'input> {
    fn enter(&self, listener: &mut (dyn CypherListener<'input> + 'a)) -> Result<(), ANTLRError> {
        listener.enter_every_rule(self)?;
        listener.enter_oC_OrExpression(self);
        Ok(())
    }
    fn exit(&self, listener: &mut (dyn CypherListener<'input> + 'a)) -> Result<(), ANTLRError> {
        listener.exit_oC_OrExpression(self);
        listener.exit_every_rule(self)?;
        Ok(())
    }
}

impl<'input, 'a> Visitable<dyn CypherVisitor<'input> + 'a> for OC_OrExpressionContext<'input> {
    fn accept(&self, visitor: &mut (dyn CypherVisitor<'input> + 'a)) {
        visitor.visit_oC_OrExpression(self);
    }
}

impl<'input> CustomRuleContext<'input> for OC_OrExpressionContextExt<'input> {
    type TF = LocalTokenFactory<'input>;
    type Ctx = CypherParserContextType;
    fn get_rule_index(&self) -> usize {
        RULE_oC_OrExpression
    }
    //fn type_rule_index() -> usize where Self: Sized { RULE_oC_OrExpression }
}
antlr4rust::tid! {OC_OrExpressionContextExt<'a>}

impl<'input> OC_OrExpressionContextExt<'input> {
    fn new(
        parent: Option<Rc<dyn CypherParserContext<'input> + 'input>>,
        invoking_state: i32,
    ) -> Rc<OC_OrExpressionContextAll<'input>> {
        Rc::new(BaseParserRuleContext::new_parser_ctx(
            parent,
            invoking_state,
            OC_OrExpressionContextExt { ph: PhantomData },
        ))
    }
}

pub trait OC_OrExpressionContextAttrs<'input>:
    CypherParserContext<'input> + BorrowMut<OC_OrExpressionContextExt<'input>>
{
    fn oC_XorExpression_all(&self) -> Vec<Rc<OC_XorExpressionContextAll<'input>>>
    where
        Self: Sized,
    {
        self.children_of_type()
    }
    fn oC_XorExpression(&self, i: usize) -> Option<Rc<OC_XorExpressionContextAll<'input>>>
    where
        Self: Sized,
    {
        self.child_of_type(i)
    }
    /// Retrieves all `TerminalNode`s corresponding to token SP in current rule
    fn SP_all(&self) -> Vec<Rc<TerminalNode<'input, CypherParserContextType>>>
    where
        Self: Sized,
    {
        self.children_of_type()
    }
    /// Retrieves 'i's TerminalNode corresponding to token SP, starting from 0.
    /// Returns `None` if number of children corresponding to token SP is less or equal than `i`.
    fn SP(&self, i: usize) -> Option<Rc<TerminalNode<'input, CypherParserContextType>>>
    where
        Self: Sized,
    {
        self.get_token(Cypher_SP, i)
    }
    /// Retrieves all `TerminalNode`s corresponding to token OR in current rule
    fn OR_all(&self) -> Vec<Rc<TerminalNode<'input, CypherParserContextType>>>
    where
        Self: Sized,
    {
        self.children_of_type()
    }
    /// Retrieves 'i's TerminalNode corresponding to token OR, starting from 0.
    /// Returns `None` if number of children corresponding to token OR is less or equal than `i`.
    fn OR(&self, i: usize) -> Option<Rc<TerminalNode<'input, CypherParserContextType>>>
    where
        Self: Sized,
    {
        self.get_token(Cypher_OR, i)
    }
}

impl<'input> OC_OrExpressionContextAttrs<'input> for OC_OrExpressionContext<'input> {}

impl<'input, I> CypherParser<'input, I>
where
    I: TokenStream<'input, TF = LocalTokenFactory<'input>> + TidAble<'input>,
{
    pub fn oC_OrExpression(&mut self) -> Result<Rc<OC_OrExpressionContextAll<'input>>, ANTLRError> {
        let mut recog = self;
        let _parentctx = recog.ctx.take();
        let mut _localctx =
            OC_OrExpressionContextExt::new(_parentctx.clone(), recog.base.get_state());
        recog
            .base
            .enter_rule(_localctx.clone(), 106, RULE_oC_OrExpression);
        let mut _localctx: Rc<OC_OrExpressionContextAll> = _localctx;
        let result: Result<(), ANTLRError> = (|| {
            let mut _alt: i32;
            //recog.base.enter_outer_alt(_localctx.clone(), 1)?;
            recog.base.enter_outer_alt(None, 1)?;
            {
                /*InvokeRule oC_XorExpression*/
                recog.base.set_state(946);
                recog.oC_XorExpression()?;

                recog.base.set_state(953);
                recog.err_handler.sync(&mut recog.base)?;
                _alt = recog.interpreter.adaptive_predict(159, &mut recog.base)?;
                while { _alt != 2 && _alt != INVALID_ALT } {
                    if _alt == 1 {
                        {
                            {
                                recog.base.set_state(947);
                                recog.base.match_token(Cypher_SP, &mut recog.err_handler)?;

                                recog.base.set_state(948);
                                recog.base.match_token(Cypher_OR, &mut recog.err_handler)?;

                                recog.base.set_state(949);
                                recog.base.match_token(Cypher_SP, &mut recog.err_handler)?;

                                /*InvokeRule oC_XorExpression*/
                                recog.base.set_state(950);
                                recog.oC_XorExpression()?;
                            }
                        }
                    }
                    recog.base.set_state(955);
                    recog.err_handler.sync(&mut recog.base)?;
                    _alt = recog.interpreter.adaptive_predict(159, &mut recog.base)?;
                }
            }
            Ok(())
        })();
        match result {
            Ok(_) => {}
            Err(e @ ANTLRError::FallThrough(_)) => return Err(e),
            Err(ref re) => {
                //_localctx.exception = re;
                recog.err_handler.report_error(&mut recog.base, re);
                recog.err_handler.recover(&mut recog.base, re)?;
            }
        }
        recog.base.exit_rule()?;

        Ok(_localctx)
    }
}
//------------------- oC_XorExpression ----------------
pub type OC_XorExpressionContextAll<'input> = OC_XorExpressionContext<'input>;

pub type OC_XorExpressionContext<'input> =
    BaseParserRuleContext<'input, OC_XorExpressionContextExt<'input>>;

#[derive(Clone)]
pub struct OC_XorExpressionContextExt<'input> {
    ph: PhantomData<&'input str>,
}

impl<'input> CypherParserContext<'input> for OC_XorExpressionContext<'input> {}

impl<'input, 'a> Listenable<dyn CypherListener<'input> + 'a> for OC_XorExpressionContext<'input> {
    fn enter(&self, listener: &mut (dyn CypherListener<'input> + 'a)) -> Result<(), ANTLRError> {
        listener.enter_every_rule(self)?;
        listener.enter_oC_XorExpression(self);
        Ok(())
    }
    fn exit(&self, listener: &mut (dyn CypherListener<'input> + 'a)) -> Result<(), ANTLRError> {
        listener.exit_oC_XorExpression(self);
        listener.exit_every_rule(self)?;
        Ok(())
    }
}

impl<'input, 'a> Visitable<dyn CypherVisitor<'input> + 'a> for OC_XorExpressionContext<'input> {
    fn accept(&self, visitor: &mut (dyn CypherVisitor<'input> + 'a)) {
        visitor.visit_oC_XorExpression(self);
    }
}

impl<'input> CustomRuleContext<'input> for OC_XorExpressionContextExt<'input> {
    type TF = LocalTokenFactory<'input>;
    type Ctx = CypherParserContextType;
    fn get_rule_index(&self) -> usize {
        RULE_oC_XorExpression
    }
    //fn type_rule_index() -> usize where Self: Sized { RULE_oC_XorExpression }
}
antlr4rust::tid! {OC_XorExpressionContextExt<'a>}

impl<'input> OC_XorExpressionContextExt<'input> {
    fn new(
        parent: Option<Rc<dyn CypherParserContext<'input> + 'input>>,
        invoking_state: i32,
    ) -> Rc<OC_XorExpressionContextAll<'input>> {
        Rc::new(BaseParserRuleContext::new_parser_ctx(
            parent,
            invoking_state,
            OC_XorExpressionContextExt { ph: PhantomData },
        ))
    }
}

pub trait OC_XorExpressionContextAttrs<'input>:
    CypherParserContext<'input> + BorrowMut<OC_XorExpressionContextExt<'input>>
{
    fn oC_AndExpression_all(&self) -> Vec<Rc<OC_AndExpressionContextAll<'input>>>
    where
        Self: Sized,
    {
        self.children_of_type()
    }
    fn oC_AndExpression(&self, i: usize) -> Option<Rc<OC_AndExpressionContextAll<'input>>>
    where
        Self: Sized,
    {
        self.child_of_type(i)
    }
    /// Retrieves all `TerminalNode`s corresponding to token SP in current rule
    fn SP_all(&self) -> Vec<Rc<TerminalNode<'input, CypherParserContextType>>>
    where
        Self: Sized,
    {
        self.children_of_type()
    }
    /// Retrieves 'i's TerminalNode corresponding to token SP, starting from 0.
    /// Returns `None` if number of children corresponding to token SP is less or equal than `i`.
    fn SP(&self, i: usize) -> Option<Rc<TerminalNode<'input, CypherParserContextType>>>
    where
        Self: Sized,
    {
        self.get_token(Cypher_SP, i)
    }
    /// Retrieves all `TerminalNode`s corresponding to token XOR in current rule
    fn XOR_all(&self) -> Vec<Rc<TerminalNode<'input, CypherParserContextType>>>
    where
        Self: Sized,
    {
        self.children_of_type()
    }
    /// Retrieves 'i's TerminalNode corresponding to token XOR, starting from 0.
    /// Returns `None` if number of children corresponding to token XOR is less or equal than `i`.
    fn XOR(&self, i: usize) -> Option<Rc<TerminalNode<'input, CypherParserContextType>>>
    where
        Self: Sized,
    {
        self.get_token(Cypher_XOR, i)
    }
}

impl<'input> OC_XorExpressionContextAttrs<'input> for OC_XorExpressionContext<'input> {}

impl<'input, I> CypherParser<'input, I>
where
    I: TokenStream<'input, TF = LocalTokenFactory<'input>> + TidAble<'input>,
{
    pub fn oC_XorExpression(
        &mut self,
    ) -> Result<Rc<OC_XorExpressionContextAll<'input>>, ANTLRError> {
        let mut recog = self;
        let _parentctx = recog.ctx.take();
        let mut _localctx =
            OC_XorExpressionContextExt::new(_parentctx.clone(), recog.base.get_state());
        recog
            .base
            .enter_rule(_localctx.clone(), 108, RULE_oC_XorExpression);
        let mut _localctx: Rc<OC_XorExpressionContextAll> = _localctx;
        let result: Result<(), ANTLRError> = (|| {
            let mut _alt: i32;
            //recog.base.enter_outer_alt(_localctx.clone(), 1)?;
            recog.base.enter_outer_alt(None, 1)?;
            {
                /*InvokeRule oC_AndExpression*/
                recog.base.set_state(956);
                recog.oC_AndExpression()?;

                recog.base.set_state(963);
                recog.err_handler.sync(&mut recog.base)?;
                _alt = recog.interpreter.adaptive_predict(160, &mut recog.base)?;
                while { _alt != 2 && _alt != INVALID_ALT } {
                    if _alt == 1 {
                        {
                            {
                                recog.base.set_state(957);
                                recog.base.match_token(Cypher_SP, &mut recog.err_handler)?;

                                recog.base.set_state(958);
                                recog.base.match_token(Cypher_XOR, &mut recog.err_handler)?;

                                recog.base.set_state(959);
                                recog.base.match_token(Cypher_SP, &mut recog.err_handler)?;

                                /*InvokeRule oC_AndExpression*/
                                recog.base.set_state(960);
                                recog.oC_AndExpression()?;
                            }
                        }
                    }
                    recog.base.set_state(965);
                    recog.err_handler.sync(&mut recog.base)?;
                    _alt = recog.interpreter.adaptive_predict(160, &mut recog.base)?;
                }
            }
            Ok(())
        })();
        match result {
            Ok(_) => {}
            Err(e @ ANTLRError::FallThrough(_)) => return Err(e),
            Err(ref re) => {
                //_localctx.exception = re;
                recog.err_handler.report_error(&mut recog.base, re);
                recog.err_handler.recover(&mut recog.base, re)?;
            }
        }
        recog.base.exit_rule()?;

        Ok(_localctx)
    }
}
//------------------- oC_AndExpression ----------------
pub type OC_AndExpressionContextAll<'input> = OC_AndExpressionContext<'input>;

pub type OC_AndExpressionContext<'input> =
    BaseParserRuleContext<'input, OC_AndExpressionContextExt<'input>>;

#[derive(Clone)]
pub struct OC_AndExpressionContextExt<'input> {
    ph: PhantomData<&'input str>,
}

impl<'input> CypherParserContext<'input> for OC_AndExpressionContext<'input> {}

impl<'input, 'a> Listenable<dyn CypherListener<'input> + 'a> for OC_AndExpressionContext<'input> {
    fn enter(&self, listener: &mut (dyn CypherListener<'input> + 'a)) -> Result<(), ANTLRError> {
        listener.enter_every_rule(self)?;
        listener.enter_oC_AndExpression(self);
        Ok(())
    }
    fn exit(&self, listener: &mut (dyn CypherListener<'input> + 'a)) -> Result<(), ANTLRError> {
        listener.exit_oC_AndExpression(self);
        listener.exit_every_rule(self)?;
        Ok(())
    }
}

impl<'input, 'a> Visitable<dyn CypherVisitor<'input> + 'a> for OC_AndExpressionContext<'input> {
    fn accept(&self, visitor: &mut (dyn CypherVisitor<'input> + 'a)) {
        visitor.visit_oC_AndExpression(self);
    }
}

impl<'input> CustomRuleContext<'input> for OC_AndExpressionContextExt<'input> {
    type TF = LocalTokenFactory<'input>;
    type Ctx = CypherParserContextType;
    fn get_rule_index(&self) -> usize {
        RULE_oC_AndExpression
    }
    //fn type_rule_index() -> usize where Self: Sized { RULE_oC_AndExpression }
}
antlr4rust::tid! {OC_AndExpressionContextExt<'a>}

impl<'input> OC_AndExpressionContextExt<'input> {
    fn new(
        parent: Option<Rc<dyn CypherParserContext<'input> + 'input>>,
        invoking_state: i32,
    ) -> Rc<OC_AndExpressionContextAll<'input>> {
        Rc::new(BaseParserRuleContext::new_parser_ctx(
            parent,
            invoking_state,
            OC_AndExpressionContextExt { ph: PhantomData },
        ))
    }
}

pub trait OC_AndExpressionContextAttrs<'input>:
    CypherParserContext<'input> + BorrowMut<OC_AndExpressionContextExt<'input>>
{
    fn oC_NotExpression_all(&self) -> Vec<Rc<OC_NotExpressionContextAll<'input>>>
    where
        Self: Sized,
    {
        self.children_of_type()
    }
    fn oC_NotExpression(&self, i: usize) -> Option<Rc<OC_NotExpressionContextAll<'input>>>
    where
        Self: Sized,
    {
        self.child_of_type(i)
    }
    /// Retrieves all `TerminalNode`s corresponding to token SP in current rule
    fn SP_all(&self) -> Vec<Rc<TerminalNode<'input, CypherParserContextType>>>
    where
        Self: Sized,
    {
        self.children_of_type()
    }
    /// Retrieves 'i's TerminalNode corresponding to token SP, starting from 0.
    /// Returns `None` if number of children corresponding to token SP is less or equal than `i`.
    fn SP(&self, i: usize) -> Option<Rc<TerminalNode<'input, CypherParserContextType>>>
    where
        Self: Sized,
    {
        self.get_token(Cypher_SP, i)
    }
    /// Retrieves all `TerminalNode`s corresponding to token AND in current rule
    fn AND_all(&self) -> Vec<Rc<TerminalNode<'input, CypherParserContextType>>>
    where
        Self: Sized,
    {
        self.children_of_type()
    }
    /// Retrieves 'i's TerminalNode corresponding to token AND, starting from 0.
    /// Returns `None` if number of children corresponding to token AND is less or equal than `i`.
    fn AND(&self, i: usize) -> Option<Rc<TerminalNode<'input, CypherParserContextType>>>
    where
        Self: Sized,
    {
        self.get_token(Cypher_AND, i)
    }
}

impl<'input> OC_AndExpressionContextAttrs<'input> for OC_AndExpressionContext<'input> {}

impl<'input, I> CypherParser<'input, I>
where
    I: TokenStream<'input, TF = LocalTokenFactory<'input>> + TidAble<'input>,
{
    pub fn oC_AndExpression(
        &mut self,
    ) -> Result<Rc<OC_AndExpressionContextAll<'input>>, ANTLRError> {
        let mut recog = self;
        let _parentctx = recog.ctx.take();
        let mut _localctx =
            OC_AndExpressionContextExt::new(_parentctx.clone(), recog.base.get_state());
        recog
            .base
            .enter_rule(_localctx.clone(), 110, RULE_oC_AndExpression);
        let mut _localctx: Rc<OC_AndExpressionContextAll> = _localctx;
        let result: Result<(), ANTLRError> = (|| {
            let mut _alt: i32;
            //recog.base.enter_outer_alt(_localctx.clone(), 1)?;
            recog.base.enter_outer_alt(None, 1)?;
            {
                /*InvokeRule oC_NotExpression*/
                recog.base.set_state(966);
                recog.oC_NotExpression()?;

                recog.base.set_state(973);
                recog.err_handler.sync(&mut recog.base)?;
                _alt = recog.interpreter.adaptive_predict(161, &mut recog.base)?;
                while { _alt != 2 && _alt != INVALID_ALT } {
                    if _alt == 1 {
                        {
                            {
                                recog.base.set_state(967);
                                recog.base.match_token(Cypher_SP, &mut recog.err_handler)?;

                                recog.base.set_state(968);
                                recog.base.match_token(Cypher_AND, &mut recog.err_handler)?;

                                recog.base.set_state(969);
                                recog.base.match_token(Cypher_SP, &mut recog.err_handler)?;

                                /*InvokeRule oC_NotExpression*/
                                recog.base.set_state(970);
                                recog.oC_NotExpression()?;
                            }
                        }
                    }
                    recog.base.set_state(975);
                    recog.err_handler.sync(&mut recog.base)?;
                    _alt = recog.interpreter.adaptive_predict(161, &mut recog.base)?;
                }
            }
            Ok(())
        })();
        match result {
            Ok(_) => {}
            Err(e @ ANTLRError::FallThrough(_)) => return Err(e),
            Err(ref re) => {
                //_localctx.exception = re;
                recog.err_handler.report_error(&mut recog.base, re);
                recog.err_handler.recover(&mut recog.base, re)?;
            }
        }
        recog.base.exit_rule()?;

        Ok(_localctx)
    }
}
//------------------- oC_NotExpression ----------------
pub type OC_NotExpressionContextAll<'input> = OC_NotExpressionContext<'input>;

pub type OC_NotExpressionContext<'input> =
    BaseParserRuleContext<'input, OC_NotExpressionContextExt<'input>>;

#[derive(Clone)]
pub struct OC_NotExpressionContextExt<'input> {
    ph: PhantomData<&'input str>,
}

impl<'input> CypherParserContext<'input> for OC_NotExpressionContext<'input> {}

impl<'input, 'a> Listenable<dyn CypherListener<'input> + 'a> for OC_NotExpressionContext<'input> {
    fn enter(&self, listener: &mut (dyn CypherListener<'input> + 'a)) -> Result<(), ANTLRError> {
        listener.enter_every_rule(self)?;
        listener.enter_oC_NotExpression(self);
        Ok(())
    }
    fn exit(&self, listener: &mut (dyn CypherListener<'input> + 'a)) -> Result<(), ANTLRError> {
        listener.exit_oC_NotExpression(self);
        listener.exit_every_rule(self)?;
        Ok(())
    }
}

impl<'input, 'a> Visitable<dyn CypherVisitor<'input> + 'a> for OC_NotExpressionContext<'input> {
    fn accept(&self, visitor: &mut (dyn CypherVisitor<'input> + 'a)) {
        visitor.visit_oC_NotExpression(self);
    }
}

impl<'input> CustomRuleContext<'input> for OC_NotExpressionContextExt<'input> {
    type TF = LocalTokenFactory<'input>;
    type Ctx = CypherParserContextType;
    fn get_rule_index(&self) -> usize {
        RULE_oC_NotExpression
    }
    //fn type_rule_index() -> usize where Self: Sized { RULE_oC_NotExpression }
}
antlr4rust::tid! {OC_NotExpressionContextExt<'a>}

impl<'input> OC_NotExpressionContextExt<'input> {
    fn new(
        parent: Option<Rc<dyn CypherParserContext<'input> + 'input>>,
        invoking_state: i32,
    ) -> Rc<OC_NotExpressionContextAll<'input>> {
        Rc::new(BaseParserRuleContext::new_parser_ctx(
            parent,
            invoking_state,
            OC_NotExpressionContextExt { ph: PhantomData },
        ))
    }
}

pub trait OC_NotExpressionContextAttrs<'input>:
    CypherParserContext<'input> + BorrowMut<OC_NotExpressionContextExt<'input>>
{
    fn oC_ComparisonExpression(&self) -> Option<Rc<OC_ComparisonExpressionContextAll<'input>>>
    where
        Self: Sized,
    {
        self.child_of_type(0)
    }
    /// Retrieves all `TerminalNode`s corresponding to token NOT in current rule
    fn NOT_all(&self) -> Vec<Rc<TerminalNode<'input, CypherParserContextType>>>
    where
        Self: Sized,
    {
        self.children_of_type()
    }
    /// Retrieves 'i's TerminalNode corresponding to token NOT, starting from 0.
    /// Returns `None` if number of children corresponding to token NOT is less or equal than `i`.
    fn NOT(&self, i: usize) -> Option<Rc<TerminalNode<'input, CypherParserContextType>>>
    where
        Self: Sized,
    {
        self.get_token(Cypher_NOT, i)
    }
    /// Retrieves all `TerminalNode`s corresponding to token SP in current rule
    fn SP_all(&self) -> Vec<Rc<TerminalNode<'input, CypherParserContextType>>>
    where
        Self: Sized,
    {
        self.children_of_type()
    }
    /// Retrieves 'i's TerminalNode corresponding to token SP, starting from 0.
    /// Returns `None` if number of children corresponding to token SP is less or equal than `i`.
    fn SP(&self, i: usize) -> Option<Rc<TerminalNode<'input, CypherParserContextType>>>
    where
        Self: Sized,
    {
        self.get_token(Cypher_SP, i)
    }
}

impl<'input> OC_NotExpressionContextAttrs<'input> for OC_NotExpressionContext<'input> {}

impl<'input, I> CypherParser<'input, I>
where
    I: TokenStream<'input, TF = LocalTokenFactory<'input>> + TidAble<'input>,
{
    pub fn oC_NotExpression(
        &mut self,
    ) -> Result<Rc<OC_NotExpressionContextAll<'input>>, ANTLRError> {
        let mut recog = self;
        let _parentctx = recog.ctx.take();
        let mut _localctx =
            OC_NotExpressionContextExt::new(_parentctx.clone(), recog.base.get_state());
        recog
            .base
            .enter_rule(_localctx.clone(), 112, RULE_oC_NotExpression);
        let mut _localctx: Rc<OC_NotExpressionContextAll> = _localctx;
        let mut _la: i32 = -1;
        let result: Result<(), ANTLRError> = (|| {
            //recog.base.enter_outer_alt(_localctx.clone(), 1)?;
            recog.base.enter_outer_alt(None, 1)?;
            {
                recog.base.set_state(982);
                recog.err_handler.sync(&mut recog.base)?;
                _la = recog.base.input.la(1);
                while _la == Cypher_NOT {
                    {
                        {
                            recog.base.set_state(976);
                            recog.base.match_token(Cypher_NOT, &mut recog.err_handler)?;

                            recog.base.set_state(978);
                            recog.err_handler.sync(&mut recog.base)?;
                            _la = recog.base.input.la(1);
                            if _la == Cypher_SP {
                                {
                                    recog.base.set_state(977);
                                    recog.base.match_token(Cypher_SP, &mut recog.err_handler)?;
                                }
                            }
                        }
                    }
                    recog.base.set_state(984);
                    recog.err_handler.sync(&mut recog.base)?;
                    _la = recog.base.input.la(1);
                }
                /*InvokeRule oC_ComparisonExpression*/
                recog.base.set_state(985);
                recog.oC_ComparisonExpression()?;
            }
            Ok(())
        })();
        match result {
            Ok(_) => {}
            Err(e @ ANTLRError::FallThrough(_)) => return Err(e),
            Err(ref re) => {
                //_localctx.exception = re;
                recog.err_handler.report_error(&mut recog.base, re);
                recog.err_handler.recover(&mut recog.base, re)?;
            }
        }
        recog.base.exit_rule()?;

        Ok(_localctx)
    }
}
//------------------- oC_ComparisonExpression ----------------
pub type OC_ComparisonExpressionContextAll<'input> = OC_ComparisonExpressionContext<'input>;

pub type OC_ComparisonExpressionContext<'input> =
    BaseParserRuleContext<'input, OC_ComparisonExpressionContextExt<'input>>;

#[derive(Clone)]
pub struct OC_ComparisonExpressionContextExt<'input> {
    ph: PhantomData<&'input str>,
}

impl<'input> CypherParserContext<'input> for OC_ComparisonExpressionContext<'input> {}

impl<'input, 'a> Listenable<dyn CypherListener<'input> + 'a>
    for OC_ComparisonExpressionContext<'input>
{
    fn enter(&self, listener: &mut (dyn CypherListener<'input> + 'a)) -> Result<(), ANTLRError> {
        listener.enter_every_rule(self)?;
        listener.enter_oC_ComparisonExpression(self);
        Ok(())
    }
    fn exit(&self, listener: &mut (dyn CypherListener<'input> + 'a)) -> Result<(), ANTLRError> {
        listener.exit_oC_ComparisonExpression(self);
        listener.exit_every_rule(self)?;
        Ok(())
    }
}

impl<'input, 'a> Visitable<dyn CypherVisitor<'input> + 'a>
    for OC_ComparisonExpressionContext<'input>
{
    fn accept(&self, visitor: &mut (dyn CypherVisitor<'input> + 'a)) {
        visitor.visit_oC_ComparisonExpression(self);
    }
}

impl<'input> CustomRuleContext<'input> for OC_ComparisonExpressionContextExt<'input> {
    type TF = LocalTokenFactory<'input>;
    type Ctx = CypherParserContextType;
    fn get_rule_index(&self) -> usize {
        RULE_oC_ComparisonExpression
    }
    //fn type_rule_index() -> usize where Self: Sized { RULE_oC_ComparisonExpression }
}
antlr4rust::tid! {OC_ComparisonExpressionContextExt<'a>}

impl<'input> OC_ComparisonExpressionContextExt<'input> {
    fn new(
        parent: Option<Rc<dyn CypherParserContext<'input> + 'input>>,
        invoking_state: i32,
    ) -> Rc<OC_ComparisonExpressionContextAll<'input>> {
        Rc::new(BaseParserRuleContext::new_parser_ctx(
            parent,
            invoking_state,
            OC_ComparisonExpressionContextExt { ph: PhantomData },
        ))
    }
}

pub trait OC_ComparisonExpressionContextAttrs<'input>:
    CypherParserContext<'input> + BorrowMut<OC_ComparisonExpressionContextExt<'input>>
{
    fn oC_StringListNullPredicateExpression(
        &self,
    ) -> Option<Rc<OC_StringListNullPredicateExpressionContextAll<'input>>>
    where
        Self: Sized,
    {
        self.child_of_type(0)
    }
    fn oC_PartialComparisonExpression_all(
        &self,
    ) -> Vec<Rc<OC_PartialComparisonExpressionContextAll<'input>>>
    where
        Self: Sized,
    {
        self.children_of_type()
    }
    fn oC_PartialComparisonExpression(
        &self,
        i: usize,
    ) -> Option<Rc<OC_PartialComparisonExpressionContextAll<'input>>>
    where
        Self: Sized,
    {
        self.child_of_type(i)
    }
    /// Retrieves all `TerminalNode`s corresponding to token SP in current rule
    fn SP_all(&self) -> Vec<Rc<TerminalNode<'input, CypherParserContextType>>>
    where
        Self: Sized,
    {
        self.children_of_type()
    }
    /// Retrieves 'i's TerminalNode corresponding to token SP, starting from 0.
    /// Returns `None` if number of children corresponding to token SP is less or equal than `i`.
    fn SP(&self, i: usize) -> Option<Rc<TerminalNode<'input, CypherParserContextType>>>
    where
        Self: Sized,
    {
        self.get_token(Cypher_SP, i)
    }
}

impl<'input> OC_ComparisonExpressionContextAttrs<'input>
    for OC_ComparisonExpressionContext<'input>
{
}

impl<'input, I> CypherParser<'input, I>
where
    I: TokenStream<'input, TF = LocalTokenFactory<'input>> + TidAble<'input>,
{
    pub fn oC_ComparisonExpression(
        &mut self,
    ) -> Result<Rc<OC_ComparisonExpressionContextAll<'input>>, ANTLRError> {
        let mut recog = self;
        let _parentctx = recog.ctx.take();
        let mut _localctx =
            OC_ComparisonExpressionContextExt::new(_parentctx.clone(), recog.base.get_state());
        recog
            .base
            .enter_rule(_localctx.clone(), 114, RULE_oC_ComparisonExpression);
        let mut _localctx: Rc<OC_ComparisonExpressionContextAll> = _localctx;
        let mut _la: i32 = -1;
        let result: Result<(), ANTLRError> = (|| {
            let mut _alt: i32;
            //recog.base.enter_outer_alt(_localctx.clone(), 1)?;
            recog.base.enter_outer_alt(None, 1)?;
            {
                /*InvokeRule oC_StringListNullPredicateExpression*/
                recog.base.set_state(987);
                recog.oC_StringListNullPredicateExpression()?;

                recog.base.set_state(994);
                recog.err_handler.sync(&mut recog.base)?;
                _alt = recog.interpreter.adaptive_predict(165, &mut recog.base)?;
                while { _alt != 2 && _alt != INVALID_ALT } {
                    if _alt == 1 {
                        {
                            {
                                recog.base.set_state(989);
                                recog.err_handler.sync(&mut recog.base)?;
                                _la = recog.base.input.la(1);
                                if _la == Cypher_SP {
                                    {
                                        recog.base.set_state(988);
                                        recog
                                            .base
                                            .match_token(Cypher_SP, &mut recog.err_handler)?;
                                    }
                                }

                                /*InvokeRule oC_PartialComparisonExpression*/
                                recog.base.set_state(991);
                                recog.oC_PartialComparisonExpression()?;
                            }
                        }
                    }
                    recog.base.set_state(996);
                    recog.err_handler.sync(&mut recog.base)?;
                    _alt = recog.interpreter.adaptive_predict(165, &mut recog.base)?;
                }
            }
            Ok(())
        })();
        match result {
            Ok(_) => {}
            Err(e @ ANTLRError::FallThrough(_)) => return Err(e),
            Err(ref re) => {
                //_localctx.exception = re;
                recog.err_handler.report_error(&mut recog.base, re);
                recog.err_handler.recover(&mut recog.base, re)?;
            }
        }
        recog.base.exit_rule()?;

        Ok(_localctx)
    }
}
//------------------- oC_PartialComparisonExpression ----------------
pub type OC_PartialComparisonExpressionContextAll<'input> =
    OC_PartialComparisonExpressionContext<'input>;

pub type OC_PartialComparisonExpressionContext<'input> =
    BaseParserRuleContext<'input, OC_PartialComparisonExpressionContextExt<'input>>;

#[derive(Clone)]
pub struct OC_PartialComparisonExpressionContextExt<'input> {
    ph: PhantomData<&'input str>,
}

impl<'input> CypherParserContext<'input> for OC_PartialComparisonExpressionContext<'input> {}

impl<'input, 'a> Listenable<dyn CypherListener<'input> + 'a>
    for OC_PartialComparisonExpressionContext<'input>
{
    fn enter(&self, listener: &mut (dyn CypherListener<'input> + 'a)) -> Result<(), ANTLRError> {
        listener.enter_every_rule(self)?;
        listener.enter_oC_PartialComparisonExpression(self);
        Ok(())
    }
    fn exit(&self, listener: &mut (dyn CypherListener<'input> + 'a)) -> Result<(), ANTLRError> {
        listener.exit_oC_PartialComparisonExpression(self);
        listener.exit_every_rule(self)?;
        Ok(())
    }
}

impl<'input, 'a> Visitable<dyn CypherVisitor<'input> + 'a>
    for OC_PartialComparisonExpressionContext<'input>
{
    fn accept(&self, visitor: &mut (dyn CypherVisitor<'input> + 'a)) {
        visitor.visit_oC_PartialComparisonExpression(self);
    }
}

impl<'input> CustomRuleContext<'input> for OC_PartialComparisonExpressionContextExt<'input> {
    type TF = LocalTokenFactory<'input>;
    type Ctx = CypherParserContextType;
    fn get_rule_index(&self) -> usize {
        RULE_oC_PartialComparisonExpression
    }
    //fn type_rule_index() -> usize where Self: Sized { RULE_oC_PartialComparisonExpression }
}
antlr4rust::tid! {OC_PartialComparisonExpressionContextExt<'a>}

impl<'input> OC_PartialComparisonExpressionContextExt<'input> {
    fn new(
        parent: Option<Rc<dyn CypherParserContext<'input> + 'input>>,
        invoking_state: i32,
    ) -> Rc<OC_PartialComparisonExpressionContextAll<'input>> {
        Rc::new(BaseParserRuleContext::new_parser_ctx(
            parent,
            invoking_state,
            OC_PartialComparisonExpressionContextExt { ph: PhantomData },
        ))
    }
}

pub trait OC_PartialComparisonExpressionContextAttrs<'input>:
    CypherParserContext<'input> + BorrowMut<OC_PartialComparisonExpressionContextExt<'input>>
{
    fn oC_StringListNullPredicateExpression(
        &self,
    ) -> Option<Rc<OC_StringListNullPredicateExpressionContextAll<'input>>>
    where
        Self: Sized,
    {
        self.child_of_type(0)
    }
    /// Retrieves first TerminalNode corresponding to token SP
    /// Returns `None` if there is no child corresponding to token SP
    fn SP(&self) -> Option<Rc<TerminalNode<'input, CypherParserContextType>>>
    where
        Self: Sized,
    {
        self.get_token(Cypher_SP, 0)
    }
}

impl<'input> OC_PartialComparisonExpressionContextAttrs<'input>
    for OC_PartialComparisonExpressionContext<'input>
{
}

impl<'input, I> CypherParser<'input, I>
where
    I: TokenStream<'input, TF = LocalTokenFactory<'input>> + TidAble<'input>,
{
    pub fn oC_PartialComparisonExpression(
        &mut self,
    ) -> Result<Rc<OC_PartialComparisonExpressionContextAll<'input>>, ANTLRError> {
        let mut recog = self;
        let _parentctx = recog.ctx.take();
        let mut _localctx = OC_PartialComparisonExpressionContextExt::new(
            _parentctx.clone(),
            recog.base.get_state(),
        );
        recog
            .base
            .enter_rule(_localctx.clone(), 116, RULE_oC_PartialComparisonExpression);
        let mut _localctx: Rc<OC_PartialComparisonExpressionContextAll> = _localctx;
        let mut _la: i32 = -1;
        let result: Result<(), ANTLRError> = (|| {
            recog.base.set_state(1027);
            recog.err_handler.sync(&mut recog.base)?;
            match recog.base.input.la(1) {
                Cypher_T__2 => {
                    //recog.base.enter_outer_alt(_localctx.clone(), 1)?;
                    recog.base.enter_outer_alt(None, 1)?;
                    {
                        {
                            recog.base.set_state(997);
                            recog
                                .base
                                .match_token(Cypher_T__2, &mut recog.err_handler)?;

                            recog.base.set_state(999);
                            recog.err_handler.sync(&mut recog.base)?;
                            _la = recog.base.input.la(1);
                            if _la == Cypher_SP {
                                {
                                    recog.base.set_state(998);
                                    recog.base.match_token(Cypher_SP, &mut recog.err_handler)?;
                                }
                            }

                            /*InvokeRule oC_StringListNullPredicateExpression*/
                            recog.base.set_state(1001);
                            recog.oC_StringListNullPredicateExpression()?;
                        }
                    }
                }

                Cypher_T__12 => {
                    //recog.base.enter_outer_alt(_localctx.clone(), 2)?;
                    recog.base.enter_outer_alt(None, 2)?;
                    {
                        {
                            recog.base.set_state(1002);
                            recog
                                .base
                                .match_token(Cypher_T__12, &mut recog.err_handler)?;

                            recog.base.set_state(1004);
                            recog.err_handler.sync(&mut recog.base)?;
                            _la = recog.base.input.la(1);
                            if _la == Cypher_SP {
                                {
                                    recog.base.set_state(1003);
                                    recog.base.match_token(Cypher_SP, &mut recog.err_handler)?;
                                }
                            }

                            /*InvokeRule oC_StringListNullPredicateExpression*/
                            recog.base.set_state(1006);
                            recog.oC_StringListNullPredicateExpression()?;
                        }
                    }
                }

                Cypher_T__13 => {
                    //recog.base.enter_outer_alt(_localctx.clone(), 3)?;
                    recog.base.enter_outer_alt(None, 3)?;
                    {
                        {
                            recog.base.set_state(1007);
                            recog
                                .base
                                .match_token(Cypher_T__13, &mut recog.err_handler)?;

                            recog.base.set_state(1009);
                            recog.err_handler.sync(&mut recog.base)?;
                            _la = recog.base.input.la(1);
                            if _la == Cypher_SP {
                                {
                                    recog.base.set_state(1008);
                                    recog.base.match_token(Cypher_SP, &mut recog.err_handler)?;
                                }
                            }

                            /*InvokeRule oC_StringListNullPredicateExpression*/
                            recog.base.set_state(1011);
                            recog.oC_StringListNullPredicateExpression()?;
                        }
                    }
                }

                Cypher_T__14 => {
                    //recog.base.enter_outer_alt(_localctx.clone(), 4)?;
                    recog.base.enter_outer_alt(None, 4)?;
                    {
                        {
                            recog.base.set_state(1012);
                            recog
                                .base
                                .match_token(Cypher_T__14, &mut recog.err_handler)?;

                            recog.base.set_state(1014);
                            recog.err_handler.sync(&mut recog.base)?;
                            _la = recog.base.input.la(1);
                            if _la == Cypher_SP {
                                {
                                    recog.base.set_state(1013);
                                    recog.base.match_token(Cypher_SP, &mut recog.err_handler)?;
                                }
                            }

                            /*InvokeRule oC_StringListNullPredicateExpression*/
                            recog.base.set_state(1016);
                            recog.oC_StringListNullPredicateExpression()?;
                        }
                    }
                }

                Cypher_T__15 => {
                    //recog.base.enter_outer_alt(_localctx.clone(), 5)?;
                    recog.base.enter_outer_alt(None, 5)?;
                    {
                        {
                            recog.base.set_state(1017);
                            recog
                                .base
                                .match_token(Cypher_T__15, &mut recog.err_handler)?;

                            recog.base.set_state(1019);
                            recog.err_handler.sync(&mut recog.base)?;
                            _la = recog.base.input.la(1);
                            if _la == Cypher_SP {
                                {
                                    recog.base.set_state(1018);
                                    recog.base.match_token(Cypher_SP, &mut recog.err_handler)?;
                                }
                            }

                            /*InvokeRule oC_StringListNullPredicateExpression*/
                            recog.base.set_state(1021);
                            recog.oC_StringListNullPredicateExpression()?;
                        }
                    }
                }

                Cypher_T__16 => {
                    //recog.base.enter_outer_alt(_localctx.clone(), 6)?;
                    recog.base.enter_outer_alt(None, 6)?;
                    {
                        {
                            recog.base.set_state(1022);
                            recog
                                .base
                                .match_token(Cypher_T__16, &mut recog.err_handler)?;

                            recog.base.set_state(1024);
                            recog.err_handler.sync(&mut recog.base)?;
                            _la = recog.base.input.la(1);
                            if _la == Cypher_SP {
                                {
                                    recog.base.set_state(1023);
                                    recog.base.match_token(Cypher_SP, &mut recog.err_handler)?;
                                }
                            }

                            /*InvokeRule oC_StringListNullPredicateExpression*/
                            recog.base.set_state(1026);
                            recog.oC_StringListNullPredicateExpression()?;
                        }
                    }
                }

                _ => Err(ANTLRError::NoAltError(NoViableAltError::new(
                    &mut recog.base,
                )))?,
            }
            Ok(())
        })();
        match result {
            Ok(_) => {}
            Err(e @ ANTLRError::FallThrough(_)) => return Err(e),
            Err(ref re) => {
                //_localctx.exception = re;
                recog.err_handler.report_error(&mut recog.base, re);
                recog.err_handler.recover(&mut recog.base, re)?;
            }
        }
        recog.base.exit_rule()?;

        Ok(_localctx)
    }
}
//------------------- oC_StringListNullPredicateExpression ----------------
pub type OC_StringListNullPredicateExpressionContextAll<'input> =
    OC_StringListNullPredicateExpressionContext<'input>;

pub type OC_StringListNullPredicateExpressionContext<'input> =
    BaseParserRuleContext<'input, OC_StringListNullPredicateExpressionContextExt<'input>>;

#[derive(Clone)]
pub struct OC_StringListNullPredicateExpressionContextExt<'input> {
    ph: PhantomData<&'input str>,
}

impl<'input> CypherParserContext<'input> for OC_StringListNullPredicateExpressionContext<'input> {}

impl<'input, 'a> Listenable<dyn CypherListener<'input> + 'a>
    for OC_StringListNullPredicateExpressionContext<'input>
{
    fn enter(&self, listener: &mut (dyn CypherListener<'input> + 'a)) -> Result<(), ANTLRError> {
        listener.enter_every_rule(self)?;
        listener.enter_oC_StringListNullPredicateExpression(self);
        Ok(())
    }
    fn exit(&self, listener: &mut (dyn CypherListener<'input> + 'a)) -> Result<(), ANTLRError> {
        listener.exit_oC_StringListNullPredicateExpression(self);
        listener.exit_every_rule(self)?;
        Ok(())
    }
}

impl<'input, 'a> Visitable<dyn CypherVisitor<'input> + 'a>
    for OC_StringListNullPredicateExpressionContext<'input>
{
    fn accept(&self, visitor: &mut (dyn CypherVisitor<'input> + 'a)) {
        visitor.visit_oC_StringListNullPredicateExpression(self);
    }
}

impl<'input> CustomRuleContext<'input> for OC_StringListNullPredicateExpressionContextExt<'input> {
    type TF = LocalTokenFactory<'input>;
    type Ctx = CypherParserContextType;
    fn get_rule_index(&self) -> usize {
        RULE_oC_StringListNullPredicateExpression
    }
    //fn type_rule_index() -> usize where Self: Sized { RULE_oC_StringListNullPredicateExpression }
}
antlr4rust::tid! {OC_StringListNullPredicateExpressionContextExt<'a>}

impl<'input> OC_StringListNullPredicateExpressionContextExt<'input> {
    fn new(
        parent: Option<Rc<dyn CypherParserContext<'input> + 'input>>,
        invoking_state: i32,
    ) -> Rc<OC_StringListNullPredicateExpressionContextAll<'input>> {
        Rc::new(BaseParserRuleContext::new_parser_ctx(
            parent,
            invoking_state,
            OC_StringListNullPredicateExpressionContextExt { ph: PhantomData },
        ))
    }
}

pub trait OC_StringListNullPredicateExpressionContextAttrs<'input>:
    CypherParserContext<'input> + BorrowMut<OC_StringListNullPredicateExpressionContextExt<'input>>
{
    fn oC_AddOrSubtractExpression(&self) -> Option<Rc<OC_AddOrSubtractExpressionContextAll<'input>>>
    where
        Self: Sized,
    {
        self.child_of_type(0)
    }
    fn oC_StringPredicateExpression_all(
        &self,
    ) -> Vec<Rc<OC_StringPredicateExpressionContextAll<'input>>>
    where
        Self: Sized,
    {
        self.children_of_type()
    }
    fn oC_StringPredicateExpression(
        &self,
        i: usize,
    ) -> Option<Rc<OC_StringPredicateExpressionContextAll<'input>>>
    where
        Self: Sized,
    {
        self.child_of_type(i)
    }
    fn oC_ListPredicateExpression_all(
        &self,
    ) -> Vec<Rc<OC_ListPredicateExpressionContextAll<'input>>>
    where
        Self: Sized,
    {
        self.children_of_type()
    }
    fn oC_ListPredicateExpression(
        &self,
        i: usize,
    ) -> Option<Rc<OC_ListPredicateExpressionContextAll<'input>>>
    where
        Self: Sized,
    {
        self.child_of_type(i)
    }
    fn oC_NullPredicateExpression_all(
        &self,
    ) -> Vec<Rc<OC_NullPredicateExpressionContextAll<'input>>>
    where
        Self: Sized,
    {
        self.children_of_type()
    }
    fn oC_NullPredicateExpression(
        &self,
        i: usize,
    ) -> Option<Rc<OC_NullPredicateExpressionContextAll<'input>>>
    where
        Self: Sized,
    {
        self.child_of_type(i)
    }
}

impl<'input> OC_StringListNullPredicateExpressionContextAttrs<'input>
    for OC_StringListNullPredicateExpressionContext<'input>
{
}

impl<'input, I> CypherParser<'input, I>
where
    I: TokenStream<'input, TF = LocalTokenFactory<'input>> + TidAble<'input>,
{
    pub fn oC_StringListNullPredicateExpression(
        &mut self,
    ) -> Result<Rc<OC_StringListNullPredicateExpressionContextAll<'input>>, ANTLRError> {
        let mut recog = self;
        let _parentctx = recog.ctx.take();
        let mut _localctx = OC_StringListNullPredicateExpressionContextExt::new(
            _parentctx.clone(),
            recog.base.get_state(),
        );
        recog.base.enter_rule(
            _localctx.clone(),
            118,
            RULE_oC_StringListNullPredicateExpression,
        );
        let mut _localctx: Rc<OC_StringListNullPredicateExpressionContextAll> = _localctx;
        let result: Result<(), ANTLRError> = (|| {
            let mut _alt: i32;
            //recog.base.enter_outer_alt(_localctx.clone(), 1)?;
            recog.base.enter_outer_alt(None, 1)?;
            {
                /*InvokeRule oC_AddOrSubtractExpression*/
                recog.base.set_state(1029);
                recog.oC_AddOrSubtractExpression()?;

                recog.base.set_state(1035);
                recog.err_handler.sync(&mut recog.base)?;
                _alt = recog.interpreter.adaptive_predict(174, &mut recog.base)?;
                while { _alt != 2 && _alt != INVALID_ALT } {
                    if _alt == 1 {
                        {
                            recog.base.set_state(1033);
                            recog.err_handler.sync(&mut recog.base)?;
                            match recog.interpreter.adaptive_predict(173, &mut recog.base)? {
                                1 => {
                                    {
                                        /*InvokeRule oC_StringPredicateExpression*/
                                        recog.base.set_state(1030);
                                        recog.oC_StringPredicateExpression()?;
                                    }
                                }
                                2 => {
                                    {
                                        /*InvokeRule oC_ListPredicateExpression*/
                                        recog.base.set_state(1031);
                                        recog.oC_ListPredicateExpression()?;
                                    }
                                }
                                3 => {
                                    {
                                        /*InvokeRule oC_NullPredicateExpression*/
                                        recog.base.set_state(1032);
                                        recog.oC_NullPredicateExpression()?;
                                    }
                                }

                                _ => {}
                            }
                        }
                    }
                    recog.base.set_state(1037);
                    recog.err_handler.sync(&mut recog.base)?;
                    _alt = recog.interpreter.adaptive_predict(174, &mut recog.base)?;
                }
            }
            Ok(())
        })();
        match result {
            Ok(_) => {}
            Err(e @ ANTLRError::FallThrough(_)) => return Err(e),
            Err(ref re) => {
                //_localctx.exception = re;
                recog.err_handler.report_error(&mut recog.base, re);
                recog.err_handler.recover(&mut recog.base, re)?;
            }
        }
        recog.base.exit_rule()?;

        Ok(_localctx)
    }
}
//------------------- oC_StringPredicateExpression ----------------
pub type OC_StringPredicateExpressionContextAll<'input> =
    OC_StringPredicateExpressionContext<'input>;

pub type OC_StringPredicateExpressionContext<'input> =
    BaseParserRuleContext<'input, OC_StringPredicateExpressionContextExt<'input>>;

#[derive(Clone)]
pub struct OC_StringPredicateExpressionContextExt<'input> {
    ph: PhantomData<&'input str>,
}

impl<'input> CypherParserContext<'input> for OC_StringPredicateExpressionContext<'input> {}

impl<'input, 'a> Listenable<dyn CypherListener<'input> + 'a>
    for OC_StringPredicateExpressionContext<'input>
{
    fn enter(&self, listener: &mut (dyn CypherListener<'input> + 'a)) -> Result<(), ANTLRError> {
        listener.enter_every_rule(self)?;
        listener.enter_oC_StringPredicateExpression(self);
        Ok(())
    }
    fn exit(&self, listener: &mut (dyn CypherListener<'input> + 'a)) -> Result<(), ANTLRError> {
        listener.exit_oC_StringPredicateExpression(self);
        listener.exit_every_rule(self)?;
        Ok(())
    }
}

impl<'input, 'a> Visitable<dyn CypherVisitor<'input> + 'a>
    for OC_StringPredicateExpressionContext<'input>
{
    fn accept(&self, visitor: &mut (dyn CypherVisitor<'input> + 'a)) {
        visitor.visit_oC_StringPredicateExpression(self);
    }
}

impl<'input> CustomRuleContext<'input> for OC_StringPredicateExpressionContextExt<'input> {
    type TF = LocalTokenFactory<'input>;
    type Ctx = CypherParserContextType;
    fn get_rule_index(&self) -> usize {
        RULE_oC_StringPredicateExpression
    }
    //fn type_rule_index() -> usize where Self: Sized { RULE_oC_StringPredicateExpression }
}
antlr4rust::tid! {OC_StringPredicateExpressionContextExt<'a>}

impl<'input> OC_StringPredicateExpressionContextExt<'input> {
    fn new(
        parent: Option<Rc<dyn CypherParserContext<'input> + 'input>>,
        invoking_state: i32,
    ) -> Rc<OC_StringPredicateExpressionContextAll<'input>> {
        Rc::new(BaseParserRuleContext::new_parser_ctx(
            parent,
            invoking_state,
            OC_StringPredicateExpressionContextExt { ph: PhantomData },
        ))
    }
}

pub trait OC_StringPredicateExpressionContextAttrs<'input>:
    CypherParserContext<'input> + BorrowMut<OC_StringPredicateExpressionContextExt<'input>>
{
    fn oC_AddOrSubtractExpression(&self) -> Option<Rc<OC_AddOrSubtractExpressionContextAll<'input>>>
    where
        Self: Sized,
    {
        self.child_of_type(0)
    }
    /// Retrieves all `TerminalNode`s corresponding to token SP in current rule
    fn SP_all(&self) -> Vec<Rc<TerminalNode<'input, CypherParserContextType>>>
    where
        Self: Sized,
    {
        self.children_of_type()
    }
    /// Retrieves 'i's TerminalNode corresponding to token SP, starting from 0.
    /// Returns `None` if number of children corresponding to token SP is less or equal than `i`.
    fn SP(&self, i: usize) -> Option<Rc<TerminalNode<'input, CypherParserContextType>>>
    where
        Self: Sized,
    {
        self.get_token(Cypher_SP, i)
    }
    /// Retrieves first TerminalNode corresponding to token STARTS
    /// Returns `None` if there is no child corresponding to token STARTS
    fn STARTS(&self) -> Option<Rc<TerminalNode<'input, CypherParserContextType>>>
    where
        Self: Sized,
    {
        self.get_token(Cypher_STARTS, 0)
    }
    /// Retrieves first TerminalNode corresponding to token WITH
    /// Returns `None` if there is no child corresponding to token WITH
    fn WITH(&self) -> Option<Rc<TerminalNode<'input, CypherParserContextType>>>
    where
        Self: Sized,
    {
        self.get_token(Cypher_WITH, 0)
    }
    /// Retrieves first TerminalNode corresponding to token ENDS
    /// Returns `None` if there is no child corresponding to token ENDS
    fn ENDS(&self) -> Option<Rc<TerminalNode<'input, CypherParserContextType>>>
    where
        Self: Sized,
    {
        self.get_token(Cypher_ENDS, 0)
    }
    /// Retrieves first TerminalNode corresponding to token CONTAINS
    /// Returns `None` if there is no child corresponding to token CONTAINS
    fn CONTAINS(&self) -> Option<Rc<TerminalNode<'input, CypherParserContextType>>>
    where
        Self: Sized,
    {
        self.get_token(Cypher_CONTAINS, 0)
    }
}

impl<'input> OC_StringPredicateExpressionContextAttrs<'input>
    for OC_StringPredicateExpressionContext<'input>
{
}

impl<'input, I> CypherParser<'input, I>
where
    I: TokenStream<'input, TF = LocalTokenFactory<'input>> + TidAble<'input>,
{
    pub fn oC_StringPredicateExpression(
        &mut self,
    ) -> Result<Rc<OC_StringPredicateExpressionContextAll<'input>>, ANTLRError> {
        let mut recog = self;
        let _parentctx = recog.ctx.take();
        let mut _localctx =
            OC_StringPredicateExpressionContextExt::new(_parentctx.clone(), recog.base.get_state());
        recog
            .base
            .enter_rule(_localctx.clone(), 120, RULE_oC_StringPredicateExpression);
        let mut _localctx: Rc<OC_StringPredicateExpressionContextAll> = _localctx;
        let mut _la: i32 = -1;
        let result: Result<(), ANTLRError> = (|| {
            //recog.base.enter_outer_alt(_localctx.clone(), 1)?;
            recog.base.enter_outer_alt(None, 1)?;
            {
                recog.base.set_state(1048);
                recog.err_handler.sync(&mut recog.base)?;
                match recog.interpreter.adaptive_predict(175, &mut recog.base)? {
                    1 => {
                        recog.base.set_state(1038);
                        recog.base.match_token(Cypher_SP, &mut recog.err_handler)?;

                        recog.base.set_state(1039);
                        recog
                            .base
                            .match_token(Cypher_STARTS, &mut recog.err_handler)?;

                        recog.base.set_state(1040);
                        recog.base.match_token(Cypher_SP, &mut recog.err_handler)?;

                        recog.base.set_state(1041);
                        recog
                            .base
                            .match_token(Cypher_WITH, &mut recog.err_handler)?;
                    }
                    2 => {
                        recog.base.set_state(1042);
                        recog.base.match_token(Cypher_SP, &mut recog.err_handler)?;

                        recog.base.set_state(1043);
                        recog
                            .base
                            .match_token(Cypher_ENDS, &mut recog.err_handler)?;

                        recog.base.set_state(1044);
                        recog.base.match_token(Cypher_SP, &mut recog.err_handler)?;

                        recog.base.set_state(1045);
                        recog
                            .base
                            .match_token(Cypher_WITH, &mut recog.err_handler)?;
                    }
                    3 => {
                        recog.base.set_state(1046);
                        recog.base.match_token(Cypher_SP, &mut recog.err_handler)?;

                        recog.base.set_state(1047);
                        recog
                            .base
                            .match_token(Cypher_CONTAINS, &mut recog.err_handler)?;
                    }

                    _ => {}
                }
                recog.base.set_state(1051);
                recog.err_handler.sync(&mut recog.base)?;
                _la = recog.base.input.la(1);
                if _la == Cypher_SP {
                    {
                        recog.base.set_state(1050);
                        recog.base.match_token(Cypher_SP, &mut recog.err_handler)?;
                    }
                }

                /*InvokeRule oC_AddOrSubtractExpression*/
                recog.base.set_state(1053);
                recog.oC_AddOrSubtractExpression()?;
            }
            Ok(())
        })();
        match result {
            Ok(_) => {}
            Err(e @ ANTLRError::FallThrough(_)) => return Err(e),
            Err(ref re) => {
                //_localctx.exception = re;
                recog.err_handler.report_error(&mut recog.base, re);
                recog.err_handler.recover(&mut recog.base, re)?;
            }
        }
        recog.base.exit_rule()?;

        Ok(_localctx)
    }
}
//------------------- oC_ListPredicateExpression ----------------
pub type OC_ListPredicateExpressionContextAll<'input> = OC_ListPredicateExpressionContext<'input>;

pub type OC_ListPredicateExpressionContext<'input> =
    BaseParserRuleContext<'input, OC_ListPredicateExpressionContextExt<'input>>;

#[derive(Clone)]
pub struct OC_ListPredicateExpressionContextExt<'input> {
    ph: PhantomData<&'input str>,
}

impl<'input> CypherParserContext<'input> for OC_ListPredicateExpressionContext<'input> {}

impl<'input, 'a> Listenable<dyn CypherListener<'input> + 'a>
    for OC_ListPredicateExpressionContext<'input>
{
    fn enter(&self, listener: &mut (dyn CypherListener<'input> + 'a)) -> Result<(), ANTLRError> {
        listener.enter_every_rule(self)?;
        listener.enter_oC_ListPredicateExpression(self);
        Ok(())
    }
    fn exit(&self, listener: &mut (dyn CypherListener<'input> + 'a)) -> Result<(), ANTLRError> {
        listener.exit_oC_ListPredicateExpression(self);
        listener.exit_every_rule(self)?;
        Ok(())
    }
}

impl<'input, 'a> Visitable<dyn CypherVisitor<'input> + 'a>
    for OC_ListPredicateExpressionContext<'input>
{
    fn accept(&self, visitor: &mut (dyn CypherVisitor<'input> + 'a)) {
        visitor.visit_oC_ListPredicateExpression(self);
    }
}

impl<'input> CustomRuleContext<'input> for OC_ListPredicateExpressionContextExt<'input> {
    type TF = LocalTokenFactory<'input>;
    type Ctx = CypherParserContextType;
    fn get_rule_index(&self) -> usize {
        RULE_oC_ListPredicateExpression
    }
    //fn type_rule_index() -> usize where Self: Sized { RULE_oC_ListPredicateExpression }
}
antlr4rust::tid! {OC_ListPredicateExpressionContextExt<'a>}

impl<'input> OC_ListPredicateExpressionContextExt<'input> {
    fn new(
        parent: Option<Rc<dyn CypherParserContext<'input> + 'input>>,
        invoking_state: i32,
    ) -> Rc<OC_ListPredicateExpressionContextAll<'input>> {
        Rc::new(BaseParserRuleContext::new_parser_ctx(
            parent,
            invoking_state,
            OC_ListPredicateExpressionContextExt { ph: PhantomData },
        ))
    }
}

pub trait OC_ListPredicateExpressionContextAttrs<'input>:
    CypherParserContext<'input> + BorrowMut<OC_ListPredicateExpressionContextExt<'input>>
{
    /// Retrieves all `TerminalNode`s corresponding to token SP in current rule
    fn SP_all(&self) -> Vec<Rc<TerminalNode<'input, CypherParserContextType>>>
    where
        Self: Sized,
    {
        self.children_of_type()
    }
    /// Retrieves 'i's TerminalNode corresponding to token SP, starting from 0.
    /// Returns `None` if number of children corresponding to token SP is less or equal than `i`.
    fn SP(&self, i: usize) -> Option<Rc<TerminalNode<'input, CypherParserContextType>>>
    where
        Self: Sized,
    {
        self.get_token(Cypher_SP, i)
    }
    /// Retrieves first TerminalNode corresponding to token IN
    /// Returns `None` if there is no child corresponding to token IN
    fn IN(&self) -> Option<Rc<TerminalNode<'input, CypherParserContextType>>>
    where
        Self: Sized,
    {
        self.get_token(Cypher_IN, 0)
    }
    fn oC_AddOrSubtractExpression(&self) -> Option<Rc<OC_AddOrSubtractExpressionContextAll<'input>>>
    where
        Self: Sized,
    {
        self.child_of_type(0)
    }
}

impl<'input> OC_ListPredicateExpressionContextAttrs<'input>
    for OC_ListPredicateExpressionContext<'input>
{
}

impl<'input, I> CypherParser<'input, I>
where
    I: TokenStream<'input, TF = LocalTokenFactory<'input>> + TidAble<'input>,
{
    pub fn oC_ListPredicateExpression(
        &mut self,
    ) -> Result<Rc<OC_ListPredicateExpressionContextAll<'input>>, ANTLRError> {
        let mut recog = self;
        let _parentctx = recog.ctx.take();
        let mut _localctx =
            OC_ListPredicateExpressionContextExt::new(_parentctx.clone(), recog.base.get_state());
        recog
            .base
            .enter_rule(_localctx.clone(), 122, RULE_oC_ListPredicateExpression);
        let mut _localctx: Rc<OC_ListPredicateExpressionContextAll> = _localctx;
        let mut _la: i32 = -1;
        let result: Result<(), ANTLRError> = (|| {
            //recog.base.enter_outer_alt(_localctx.clone(), 1)?;
            recog.base.enter_outer_alt(None, 1)?;
            {
                recog.base.set_state(1055);
                recog.base.match_token(Cypher_SP, &mut recog.err_handler)?;

                recog.base.set_state(1056);
                recog.base.match_token(Cypher_IN, &mut recog.err_handler)?;

                recog.base.set_state(1058);
                recog.err_handler.sync(&mut recog.base)?;
                _la = recog.base.input.la(1);
                if _la == Cypher_SP {
                    {
                        recog.base.set_state(1057);
                        recog.base.match_token(Cypher_SP, &mut recog.err_handler)?;
                    }
                }

                /*InvokeRule oC_AddOrSubtractExpression*/
                recog.base.set_state(1060);
                recog.oC_AddOrSubtractExpression()?;
            }
            Ok(())
        })();
        match result {
            Ok(_) => {}
            Err(e @ ANTLRError::FallThrough(_)) => return Err(e),
            Err(ref re) => {
                //_localctx.exception = re;
                recog.err_handler.report_error(&mut recog.base, re);
                recog.err_handler.recover(&mut recog.base, re)?;
            }
        }
        recog.base.exit_rule()?;

        Ok(_localctx)
    }
}
//------------------- oC_NullPredicateExpression ----------------
pub type OC_NullPredicateExpressionContextAll<'input> = OC_NullPredicateExpressionContext<'input>;

pub type OC_NullPredicateExpressionContext<'input> =
    BaseParserRuleContext<'input, OC_NullPredicateExpressionContextExt<'input>>;

#[derive(Clone)]
pub struct OC_NullPredicateExpressionContextExt<'input> {
    ph: PhantomData<&'input str>,
}

impl<'input> CypherParserContext<'input> for OC_NullPredicateExpressionContext<'input> {}

impl<'input, 'a> Listenable<dyn CypherListener<'input> + 'a>
    for OC_NullPredicateExpressionContext<'input>
{
    fn enter(&self, listener: &mut (dyn CypherListener<'input> + 'a)) -> Result<(), ANTLRError> {
        listener.enter_every_rule(self)?;
        listener.enter_oC_NullPredicateExpression(self);
        Ok(())
    }
    fn exit(&self, listener: &mut (dyn CypherListener<'input> + 'a)) -> Result<(), ANTLRError> {
        listener.exit_oC_NullPredicateExpression(self);
        listener.exit_every_rule(self)?;
        Ok(())
    }
}

impl<'input, 'a> Visitable<dyn CypherVisitor<'input> + 'a>
    for OC_NullPredicateExpressionContext<'input>
{
    fn accept(&self, visitor: &mut (dyn CypherVisitor<'input> + 'a)) {
        visitor.visit_oC_NullPredicateExpression(self);
    }
}

impl<'input> CustomRuleContext<'input> for OC_NullPredicateExpressionContextExt<'input> {
    type TF = LocalTokenFactory<'input>;
    type Ctx = CypherParserContextType;
    fn get_rule_index(&self) -> usize {
        RULE_oC_NullPredicateExpression
    }
    //fn type_rule_index() -> usize where Self: Sized { RULE_oC_NullPredicateExpression }
}
antlr4rust::tid! {OC_NullPredicateExpressionContextExt<'a>}

impl<'input> OC_NullPredicateExpressionContextExt<'input> {
    fn new(
        parent: Option<Rc<dyn CypherParserContext<'input> + 'input>>,
        invoking_state: i32,
    ) -> Rc<OC_NullPredicateExpressionContextAll<'input>> {
        Rc::new(BaseParserRuleContext::new_parser_ctx(
            parent,
            invoking_state,
            OC_NullPredicateExpressionContextExt { ph: PhantomData },
        ))
    }
}

pub trait OC_NullPredicateExpressionContextAttrs<'input>:
    CypherParserContext<'input> + BorrowMut<OC_NullPredicateExpressionContextExt<'input>>
{
    /// Retrieves all `TerminalNode`s corresponding to token SP in current rule
    fn SP_all(&self) -> Vec<Rc<TerminalNode<'input, CypherParserContextType>>>
    where
        Self: Sized,
    {
        self.children_of_type()
    }
    /// Retrieves 'i's TerminalNode corresponding to token SP, starting from 0.
    /// Returns `None` if number of children corresponding to token SP is less or equal than `i`.
    fn SP(&self, i: usize) -> Option<Rc<TerminalNode<'input, CypherParserContextType>>>
    where
        Self: Sized,
    {
        self.get_token(Cypher_SP, i)
    }
    /// Retrieves first TerminalNode corresponding to token IS
    /// Returns `None` if there is no child corresponding to token IS
    fn IS(&self) -> Option<Rc<TerminalNode<'input, CypherParserContextType>>>
    where
        Self: Sized,
    {
        self.get_token(Cypher_IS, 0)
    }
    /// Retrieves first TerminalNode corresponding to token NULL
    /// Returns `None` if there is no child corresponding to token NULL
    fn NULL(&self) -> Option<Rc<TerminalNode<'input, CypherParserContextType>>>
    where
        Self: Sized,
    {
        self.get_token(Cypher_NULL, 0)
    }
    /// Retrieves first TerminalNode corresponding to token NOT
    /// Returns `None` if there is no child corresponding to token NOT
    fn NOT(&self) -> Option<Rc<TerminalNode<'input, CypherParserContextType>>>
    where
        Self: Sized,
    {
        self.get_token(Cypher_NOT, 0)
    }
}

impl<'input> OC_NullPredicateExpressionContextAttrs<'input>
    for OC_NullPredicateExpressionContext<'input>
{
}

impl<'input, I> CypherParser<'input, I>
where
    I: TokenStream<'input, TF = LocalTokenFactory<'input>> + TidAble<'input>,
{
    pub fn oC_NullPredicateExpression(
        &mut self,
    ) -> Result<Rc<OC_NullPredicateExpressionContextAll<'input>>, ANTLRError> {
        let mut recog = self;
        let _parentctx = recog.ctx.take();
        let mut _localctx =
            OC_NullPredicateExpressionContextExt::new(_parentctx.clone(), recog.base.get_state());
        recog
            .base
            .enter_rule(_localctx.clone(), 124, RULE_oC_NullPredicateExpression);
        let mut _localctx: Rc<OC_NullPredicateExpressionContextAll> = _localctx;
        let result: Result<(), ANTLRError> = (|| {
            recog.base.set_state(1072);
            recog.err_handler.sync(&mut recog.base)?;
            match recog.interpreter.adaptive_predict(178, &mut recog.base)? {
                1 => {
                    //recog.base.enter_outer_alt(_localctx.clone(), 1)?;
                    recog.base.enter_outer_alt(None, 1)?;
                    {
                        {
                            recog.base.set_state(1062);
                            recog.base.match_token(Cypher_SP, &mut recog.err_handler)?;

                            recog.base.set_state(1063);
                            recog.base.match_token(Cypher_IS, &mut recog.err_handler)?;

                            recog.base.set_state(1064);
                            recog.base.match_token(Cypher_SP, &mut recog.err_handler)?;

                            recog.base.set_state(1065);
                            recog
                                .base
                                .match_token(Cypher_NULL, &mut recog.err_handler)?;
                        }
                    }
                }
                2 => {
                    //recog.base.enter_outer_alt(_localctx.clone(), 2)?;
                    recog.base.enter_outer_alt(None, 2)?;
                    {
                        {
                            recog.base.set_state(1066);
                            recog.base.match_token(Cypher_SP, &mut recog.err_handler)?;

                            recog.base.set_state(1067);
                            recog.base.match_token(Cypher_IS, &mut recog.err_handler)?;

                            recog.base.set_state(1068);
                            recog.base.match_token(Cypher_SP, &mut recog.err_handler)?;

                            recog.base.set_state(1069);
                            recog.base.match_token(Cypher_NOT, &mut recog.err_handler)?;

                            recog.base.set_state(1070);
                            recog.base.match_token(Cypher_SP, &mut recog.err_handler)?;

                            recog.base.set_state(1071);
                            recog
                                .base
                                .match_token(Cypher_NULL, &mut recog.err_handler)?;
                        }
                    }
                }

                _ => {}
            }
            Ok(())
        })();
        match result {
            Ok(_) => {}
            Err(e @ ANTLRError::FallThrough(_)) => return Err(e),
            Err(ref re) => {
                //_localctx.exception = re;
                recog.err_handler.report_error(&mut recog.base, re);
                recog.err_handler.recover(&mut recog.base, re)?;
            }
        }
        recog.base.exit_rule()?;

        Ok(_localctx)
    }
}
//------------------- oC_AddOrSubtractExpression ----------------
pub type OC_AddOrSubtractExpressionContextAll<'input> = OC_AddOrSubtractExpressionContext<'input>;

pub type OC_AddOrSubtractExpressionContext<'input> =
    BaseParserRuleContext<'input, OC_AddOrSubtractExpressionContextExt<'input>>;

#[derive(Clone)]
pub struct OC_AddOrSubtractExpressionContextExt<'input> {
    ph: PhantomData<&'input str>,
}

impl<'input> CypherParserContext<'input> for OC_AddOrSubtractExpressionContext<'input> {}

impl<'input, 'a> Listenable<dyn CypherListener<'input> + 'a>
    for OC_AddOrSubtractExpressionContext<'input>
{
    fn enter(&self, listener: &mut (dyn CypherListener<'input> + 'a)) -> Result<(), ANTLRError> {
        listener.enter_every_rule(self)?;
        listener.enter_oC_AddOrSubtractExpression(self);
        Ok(())
    }
    fn exit(&self, listener: &mut (dyn CypherListener<'input> + 'a)) -> Result<(), ANTLRError> {
        listener.exit_oC_AddOrSubtractExpression(self);
        listener.exit_every_rule(self)?;
        Ok(())
    }
}

impl<'input, 'a> Visitable<dyn CypherVisitor<'input> + 'a>
    for OC_AddOrSubtractExpressionContext<'input>
{
    fn accept(&self, visitor: &mut (dyn CypherVisitor<'input> + 'a)) {
        visitor.visit_oC_AddOrSubtractExpression(self);
    }
}

impl<'input> CustomRuleContext<'input> for OC_AddOrSubtractExpressionContextExt<'input> {
    type TF = LocalTokenFactory<'input>;
    type Ctx = CypherParserContextType;
    fn get_rule_index(&self) -> usize {
        RULE_oC_AddOrSubtractExpression
    }
    //fn type_rule_index() -> usize where Self: Sized { RULE_oC_AddOrSubtractExpression }
}
antlr4rust::tid! {OC_AddOrSubtractExpressionContextExt<'a>}

impl<'input> OC_AddOrSubtractExpressionContextExt<'input> {
    fn new(
        parent: Option<Rc<dyn CypherParserContext<'input> + 'input>>,
        invoking_state: i32,
    ) -> Rc<OC_AddOrSubtractExpressionContextAll<'input>> {
        Rc::new(BaseParserRuleContext::new_parser_ctx(
            parent,
            invoking_state,
            OC_AddOrSubtractExpressionContextExt { ph: PhantomData },
        ))
    }
}

pub trait OC_AddOrSubtractExpressionContextAttrs<'input>:
    CypherParserContext<'input> + BorrowMut<OC_AddOrSubtractExpressionContextExt<'input>>
{
    fn oC_MultiplyDivideModuloExpression_all(
        &self,
    ) -> Vec<Rc<OC_MultiplyDivideModuloExpressionContextAll<'input>>>
    where
        Self: Sized,
    {
        self.children_of_type()
    }
    fn oC_MultiplyDivideModuloExpression(
        &self,
        i: usize,
    ) -> Option<Rc<OC_MultiplyDivideModuloExpressionContextAll<'input>>>
    where
        Self: Sized,
    {
        self.child_of_type(i)
    }
    /// Retrieves all `TerminalNode`s corresponding to token SP in current rule
    fn SP_all(&self) -> Vec<Rc<TerminalNode<'input, CypherParserContextType>>>
    where
        Self: Sized,
    {
        self.children_of_type()
    }
    /// Retrieves 'i's TerminalNode corresponding to token SP, starting from 0.
    /// Returns `None` if number of children corresponding to token SP is less or equal than `i`.
    fn SP(&self, i: usize) -> Option<Rc<TerminalNode<'input, CypherParserContextType>>>
    where
        Self: Sized,
    {
        self.get_token(Cypher_SP, i)
    }
}

impl<'input> OC_AddOrSubtractExpressionContextAttrs<'input>
    for OC_AddOrSubtractExpressionContext<'input>
{
}

impl<'input, I> CypherParser<'input, I>
where
    I: TokenStream<'input, TF = LocalTokenFactory<'input>> + TidAble<'input>,
{
    pub fn oC_AddOrSubtractExpression(
        &mut self,
    ) -> Result<Rc<OC_AddOrSubtractExpressionContextAll<'input>>, ANTLRError> {
        let mut recog = self;
        let _parentctx = recog.ctx.take();
        let mut _localctx =
            OC_AddOrSubtractExpressionContextExt::new(_parentctx.clone(), recog.base.get_state());
        recog
            .base
            .enter_rule(_localctx.clone(), 126, RULE_oC_AddOrSubtractExpression);
        let mut _localctx: Rc<OC_AddOrSubtractExpressionContextAll> = _localctx;
        let mut _la: i32 = -1;
        let result: Result<(), ANTLRError> = (|| {
            let mut _alt: i32;
            //recog.base.enter_outer_alt(_localctx.clone(), 1)?;
            recog.base.enter_outer_alt(None, 1)?;
            {
                /*InvokeRule oC_MultiplyDivideModuloExpression*/
                recog.base.set_state(1074);
                recog.oC_MultiplyDivideModuloExpression()?;

                recog.base.set_state(1093);
                recog.err_handler.sync(&mut recog.base)?;
                _alt = recog.interpreter.adaptive_predict(184, &mut recog.base)?;
                while { _alt != 2 && _alt != INVALID_ALT } {
                    if _alt == 1 {
                        {
                            recog.base.set_state(1091);
                            recog.err_handler.sync(&mut recog.base)?;
                            match recog.interpreter.adaptive_predict(183, &mut recog.base)? {
                                1 => {
                                    {
                                        {
                                            recog.base.set_state(1076);
                                            recog.err_handler.sync(&mut recog.base)?;
                                            _la = recog.base.input.la(1);
                                            if _la == Cypher_SP {
                                                {
                                                    recog.base.set_state(1075);
                                                    recog.base.match_token(
                                                        Cypher_SP,
                                                        &mut recog.err_handler,
                                                    )?;
                                                }
                                            }

                                            recog.base.set_state(1078);
                                            recog.base.match_token(
                                                Cypher_T__17,
                                                &mut recog.err_handler,
                                            )?;

                                            recog.base.set_state(1080);
                                            recog.err_handler.sync(&mut recog.base)?;
                                            _la = recog.base.input.la(1);
                                            if _la == Cypher_SP {
                                                {
                                                    recog.base.set_state(1079);
                                                    recog.base.match_token(
                                                        Cypher_SP,
                                                        &mut recog.err_handler,
                                                    )?;
                                                }
                                            }

                                            /*InvokeRule oC_MultiplyDivideModuloExpression*/
                                            recog.base.set_state(1082);
                                            recog.oC_MultiplyDivideModuloExpression()?;
                                        }
                                    }
                                }
                                2 => {
                                    {
                                        {
                                            recog.base.set_state(1084);
                                            recog.err_handler.sync(&mut recog.base)?;
                                            _la = recog.base.input.la(1);
                                            if _la == Cypher_SP {
                                                {
                                                    recog.base.set_state(1083);
                                                    recog.base.match_token(
                                                        Cypher_SP,
                                                        &mut recog.err_handler,
                                                    )?;
                                                }
                                            }

                                            recog.base.set_state(1086);
                                            recog.base.match_token(
                                                Cypher_T__18,
                                                &mut recog.err_handler,
                                            )?;

                                            recog.base.set_state(1088);
                                            recog.err_handler.sync(&mut recog.base)?;
                                            _la = recog.base.input.la(1);
                                            if _la == Cypher_SP {
                                                {
                                                    recog.base.set_state(1087);
                                                    recog.base.match_token(
                                                        Cypher_SP,
                                                        &mut recog.err_handler,
                                                    )?;
                                                }
                                            }

                                            /*InvokeRule oC_MultiplyDivideModuloExpression*/
                                            recog.base.set_state(1090);
                                            recog.oC_MultiplyDivideModuloExpression()?;
                                        }
                                    }
                                }

                                _ => {}
                            }
                        }
                    }
                    recog.base.set_state(1095);
                    recog.err_handler.sync(&mut recog.base)?;
                    _alt = recog.interpreter.adaptive_predict(184, &mut recog.base)?;
                }
            }
            Ok(())
        })();
        match result {
            Ok(_) => {}
            Err(e @ ANTLRError::FallThrough(_)) => return Err(e),
            Err(ref re) => {
                //_localctx.exception = re;
                recog.err_handler.report_error(&mut recog.base, re);
                recog.err_handler.recover(&mut recog.base, re)?;
            }
        }
        recog.base.exit_rule()?;

        Ok(_localctx)
    }
}
//------------------- oC_MultiplyDivideModuloExpression ----------------
pub type OC_MultiplyDivideModuloExpressionContextAll<'input> =
    OC_MultiplyDivideModuloExpressionContext<'input>;

pub type OC_MultiplyDivideModuloExpressionContext<'input> =
    BaseParserRuleContext<'input, OC_MultiplyDivideModuloExpressionContextExt<'input>>;

#[derive(Clone)]
pub struct OC_MultiplyDivideModuloExpressionContextExt<'input> {
    ph: PhantomData<&'input str>,
}

impl<'input> CypherParserContext<'input> for OC_MultiplyDivideModuloExpressionContext<'input> {}

impl<'input, 'a> Listenable<dyn CypherListener<'input> + 'a>
    for OC_MultiplyDivideModuloExpressionContext<'input>
{
    fn enter(&self, listener: &mut (dyn CypherListener<'input> + 'a)) -> Result<(), ANTLRError> {
        listener.enter_every_rule(self)?;
        listener.enter_oC_MultiplyDivideModuloExpression(self);
        Ok(())
    }
    fn exit(&self, listener: &mut (dyn CypherListener<'input> + 'a)) -> Result<(), ANTLRError> {
        listener.exit_oC_MultiplyDivideModuloExpression(self);
        listener.exit_every_rule(self)?;
        Ok(())
    }
}

impl<'input, 'a> Visitable<dyn CypherVisitor<'input> + 'a>
    for OC_MultiplyDivideModuloExpressionContext<'input>
{
    fn accept(&self, visitor: &mut (dyn CypherVisitor<'input> + 'a)) {
        visitor.visit_oC_MultiplyDivideModuloExpression(self);
    }
}

impl<'input> CustomRuleContext<'input> for OC_MultiplyDivideModuloExpressionContextExt<'input> {
    type TF = LocalTokenFactory<'input>;
    type Ctx = CypherParserContextType;
    fn get_rule_index(&self) -> usize {
        RULE_oC_MultiplyDivideModuloExpression
    }
    //fn type_rule_index() -> usize where Self: Sized { RULE_oC_MultiplyDivideModuloExpression }
}
antlr4rust::tid! {OC_MultiplyDivideModuloExpressionContextExt<'a>}

impl<'input> OC_MultiplyDivideModuloExpressionContextExt<'input> {
    fn new(
        parent: Option<Rc<dyn CypherParserContext<'input> + 'input>>,
        invoking_state: i32,
    ) -> Rc<OC_MultiplyDivideModuloExpressionContextAll<'input>> {
        Rc::new(BaseParserRuleContext::new_parser_ctx(
            parent,
            invoking_state,
            OC_MultiplyDivideModuloExpressionContextExt { ph: PhantomData },
        ))
    }
}

pub trait OC_MultiplyDivideModuloExpressionContextAttrs<'input>:
    CypherParserContext<'input> + BorrowMut<OC_MultiplyDivideModuloExpressionContextExt<'input>>
{
    fn oC_PowerOfExpression_all(&self) -> Vec<Rc<OC_PowerOfExpressionContextAll<'input>>>
    where
        Self: Sized,
    {
        self.children_of_type()
    }
    fn oC_PowerOfExpression(&self, i: usize) -> Option<Rc<OC_PowerOfExpressionContextAll<'input>>>
    where
        Self: Sized,
    {
        self.child_of_type(i)
    }
    /// Retrieves all `TerminalNode`s corresponding to token SP in current rule
    fn SP_all(&self) -> Vec<Rc<TerminalNode<'input, CypherParserContextType>>>
    where
        Self: Sized,
    {
        self.children_of_type()
    }
    /// Retrieves 'i's TerminalNode corresponding to token SP, starting from 0.
    /// Returns `None` if number of children corresponding to token SP is less or equal than `i`.
    fn SP(&self, i: usize) -> Option<Rc<TerminalNode<'input, CypherParserContextType>>>
    where
        Self: Sized,
    {
        self.get_token(Cypher_SP, i)
    }
}

impl<'input> OC_MultiplyDivideModuloExpressionContextAttrs<'input>
    for OC_MultiplyDivideModuloExpressionContext<'input>
{
}

impl<'input, I> CypherParser<'input, I>
where
    I: TokenStream<'input, TF = LocalTokenFactory<'input>> + TidAble<'input>,
{
    pub fn oC_MultiplyDivideModuloExpression(
        &mut self,
    ) -> Result<Rc<OC_MultiplyDivideModuloExpressionContextAll<'input>>, ANTLRError> {
        let mut recog = self;
        let _parentctx = recog.ctx.take();
        let mut _localctx = OC_MultiplyDivideModuloExpressionContextExt::new(
            _parentctx.clone(),
            recog.base.get_state(),
        );
        recog.base.enter_rule(
            _localctx.clone(),
            128,
            RULE_oC_MultiplyDivideModuloExpression,
        );
        let mut _localctx: Rc<OC_MultiplyDivideModuloExpressionContextAll> = _localctx;
        let mut _la: i32 = -1;
        let result: Result<(), ANTLRError> = (|| {
            let mut _alt: i32;
            //recog.base.enter_outer_alt(_localctx.clone(), 1)?;
            recog.base.enter_outer_alt(None, 1)?;
            {
                /*InvokeRule oC_PowerOfExpression*/
                recog.base.set_state(1096);
                recog.oC_PowerOfExpression()?;

                recog.base.set_state(1123);
                recog.err_handler.sync(&mut recog.base)?;
                _alt = recog.interpreter.adaptive_predict(192, &mut recog.base)?;
                while { _alt != 2 && _alt != INVALID_ALT } {
                    if _alt == 1 {
                        {
                            recog.base.set_state(1121);
                            recog.err_handler.sync(&mut recog.base)?;
                            match recog.interpreter.adaptive_predict(191, &mut recog.base)? {
                                1 => {
                                    {
                                        {
                                            recog.base.set_state(1098);
                                            recog.err_handler.sync(&mut recog.base)?;
                                            _la = recog.base.input.la(1);
                                            if _la == Cypher_SP {
                                                {
                                                    recog.base.set_state(1097);
                                                    recog.base.match_token(
                                                        Cypher_SP,
                                                        &mut recog.err_handler,
                                                    )?;
                                                }
                                            }

                                            recog.base.set_state(1100);
                                            recog
                                                .base
                                                .match_token(Cypher_T__4, &mut recog.err_handler)?;

                                            recog.base.set_state(1102);
                                            recog.err_handler.sync(&mut recog.base)?;
                                            _la = recog.base.input.la(1);
                                            if _la == Cypher_SP {
                                                {
                                                    recog.base.set_state(1101);
                                                    recog.base.match_token(
                                                        Cypher_SP,
                                                        &mut recog.err_handler,
                                                    )?;
                                                }
                                            }

                                            /*InvokeRule oC_PowerOfExpression*/
                                            recog.base.set_state(1104);
                                            recog.oC_PowerOfExpression()?;
                                        }
                                    }
                                }
                                2 => {
                                    {
                                        {
                                            recog.base.set_state(1106);
                                            recog.err_handler.sync(&mut recog.base)?;
                                            _la = recog.base.input.la(1);
                                            if _la == Cypher_SP {
                                                {
                                                    recog.base.set_state(1105);
                                                    recog.base.match_token(
                                                        Cypher_SP,
                                                        &mut recog.err_handler,
                                                    )?;
                                                }
                                            }

                                            recog.base.set_state(1108);
                                            recog.base.match_token(
                                                Cypher_T__19,
                                                &mut recog.err_handler,
                                            )?;

                                            recog.base.set_state(1110);
                                            recog.err_handler.sync(&mut recog.base)?;
                                            _la = recog.base.input.la(1);
                                            if _la == Cypher_SP {
                                                {
                                                    recog.base.set_state(1109);
                                                    recog.base.match_token(
                                                        Cypher_SP,
                                                        &mut recog.err_handler,
                                                    )?;
                                                }
                                            }

                                            /*InvokeRule oC_PowerOfExpression*/
                                            recog.base.set_state(1112);
                                            recog.oC_PowerOfExpression()?;
                                        }
                                    }
                                }
                                3 => {
                                    {
                                        {
                                            recog.base.set_state(1114);
                                            recog.err_handler.sync(&mut recog.base)?;
                                            _la = recog.base.input.la(1);
                                            if _la == Cypher_SP {
                                                {
                                                    recog.base.set_state(1113);
                                                    recog.base.match_token(
                                                        Cypher_SP,
                                                        &mut recog.err_handler,
                                                    )?;
                                                }
                                            }

                                            recog.base.set_state(1116);
                                            recog.base.match_token(
                                                Cypher_T__20,
                                                &mut recog.err_handler,
                                            )?;

                                            recog.base.set_state(1118);
                                            recog.err_handler.sync(&mut recog.base)?;
                                            _la = recog.base.input.la(1);
                                            if _la == Cypher_SP {
                                                {
                                                    recog.base.set_state(1117);
                                                    recog.base.match_token(
                                                        Cypher_SP,
                                                        &mut recog.err_handler,
                                                    )?;
                                                }
                                            }

                                            /*InvokeRule oC_PowerOfExpression*/
                                            recog.base.set_state(1120);
                                            recog.oC_PowerOfExpression()?;
                                        }
                                    }
                                }

                                _ => {}
                            }
                        }
                    }
                    recog.base.set_state(1125);
                    recog.err_handler.sync(&mut recog.base)?;
                    _alt = recog.interpreter.adaptive_predict(192, &mut recog.base)?;
                }
            }
            Ok(())
        })();
        match result {
            Ok(_) => {}
            Err(e @ ANTLRError::FallThrough(_)) => return Err(e),
            Err(ref re) => {
                //_localctx.exception = re;
                recog.err_handler.report_error(&mut recog.base, re);
                recog.err_handler.recover(&mut recog.base, re)?;
            }
        }
        recog.base.exit_rule()?;

        Ok(_localctx)
    }
}
//------------------- oC_PowerOfExpression ----------------
pub type OC_PowerOfExpressionContextAll<'input> = OC_PowerOfExpressionContext<'input>;

pub type OC_PowerOfExpressionContext<'input> =
    BaseParserRuleContext<'input, OC_PowerOfExpressionContextExt<'input>>;

#[derive(Clone)]
pub struct OC_PowerOfExpressionContextExt<'input> {
    ph: PhantomData<&'input str>,
}

impl<'input> CypherParserContext<'input> for OC_PowerOfExpressionContext<'input> {}

impl<'input, 'a> Listenable<dyn CypherListener<'input> + 'a>
    for OC_PowerOfExpressionContext<'input>
{
    fn enter(&self, listener: &mut (dyn CypherListener<'input> + 'a)) -> Result<(), ANTLRError> {
        listener.enter_every_rule(self)?;
        listener.enter_oC_PowerOfExpression(self);
        Ok(())
    }
    fn exit(&self, listener: &mut (dyn CypherListener<'input> + 'a)) -> Result<(), ANTLRError> {
        listener.exit_oC_PowerOfExpression(self);
        listener.exit_every_rule(self)?;
        Ok(())
    }
}

impl<'input, 'a> Visitable<dyn CypherVisitor<'input> + 'a> for OC_PowerOfExpressionContext<'input> {
    fn accept(&self, visitor: &mut (dyn CypherVisitor<'input> + 'a)) {
        visitor.visit_oC_PowerOfExpression(self);
    }
}

impl<'input> CustomRuleContext<'input> for OC_PowerOfExpressionContextExt<'input> {
    type TF = LocalTokenFactory<'input>;
    type Ctx = CypherParserContextType;
    fn get_rule_index(&self) -> usize {
        RULE_oC_PowerOfExpression
    }
    //fn type_rule_index() -> usize where Self: Sized { RULE_oC_PowerOfExpression }
}
antlr4rust::tid! {OC_PowerOfExpressionContextExt<'a>}

impl<'input> OC_PowerOfExpressionContextExt<'input> {
    fn new(
        parent: Option<Rc<dyn CypherParserContext<'input> + 'input>>,
        invoking_state: i32,
    ) -> Rc<OC_PowerOfExpressionContextAll<'input>> {
        Rc::new(BaseParserRuleContext::new_parser_ctx(
            parent,
            invoking_state,
            OC_PowerOfExpressionContextExt { ph: PhantomData },
        ))
    }
}

pub trait OC_PowerOfExpressionContextAttrs<'input>:
    CypherParserContext<'input> + BorrowMut<OC_PowerOfExpressionContextExt<'input>>
{
    fn oC_UnaryAddOrSubtractExpression_all(
        &self,
    ) -> Vec<Rc<OC_UnaryAddOrSubtractExpressionContextAll<'input>>>
    where
        Self: Sized,
    {
        self.children_of_type()
    }
    fn oC_UnaryAddOrSubtractExpression(
        &self,
        i: usize,
    ) -> Option<Rc<OC_UnaryAddOrSubtractExpressionContextAll<'input>>>
    where
        Self: Sized,
    {
        self.child_of_type(i)
    }
    /// Retrieves all `TerminalNode`s corresponding to token SP in current rule
    fn SP_all(&self) -> Vec<Rc<TerminalNode<'input, CypherParserContextType>>>
    where
        Self: Sized,
    {
        self.children_of_type()
    }
    /// Retrieves 'i's TerminalNode corresponding to token SP, starting from 0.
    /// Returns `None` if number of children corresponding to token SP is less or equal than `i`.
    fn SP(&self, i: usize) -> Option<Rc<TerminalNode<'input, CypherParserContextType>>>
    where
        Self: Sized,
    {
        self.get_token(Cypher_SP, i)
    }
}

impl<'input> OC_PowerOfExpressionContextAttrs<'input> for OC_PowerOfExpressionContext<'input> {}

impl<'input, I> CypherParser<'input, I>
where
    I: TokenStream<'input, TF = LocalTokenFactory<'input>> + TidAble<'input>,
{
    pub fn oC_PowerOfExpression(
        &mut self,
    ) -> Result<Rc<OC_PowerOfExpressionContextAll<'input>>, ANTLRError> {
        let mut recog = self;
        let _parentctx = recog.ctx.take();
        let mut _localctx =
            OC_PowerOfExpressionContextExt::new(_parentctx.clone(), recog.base.get_state());
        recog
            .base
            .enter_rule(_localctx.clone(), 130, RULE_oC_PowerOfExpression);
        let mut _localctx: Rc<OC_PowerOfExpressionContextAll> = _localctx;
        let mut _la: i32 = -1;
        let result: Result<(), ANTLRError> = (|| {
            let mut _alt: i32;
            //recog.base.enter_outer_alt(_localctx.clone(), 1)?;
            recog.base.enter_outer_alt(None, 1)?;
            {
                /*InvokeRule oC_UnaryAddOrSubtractExpression*/
                recog.base.set_state(1126);
                recog.oC_UnaryAddOrSubtractExpression()?;

                recog.base.set_state(1137);
                recog.err_handler.sync(&mut recog.base)?;
                _alt = recog.interpreter.adaptive_predict(195, &mut recog.base)?;
                while { _alt != 2 && _alt != INVALID_ALT } {
                    if _alt == 1 {
                        {
                            {
                                recog.base.set_state(1128);
                                recog.err_handler.sync(&mut recog.base)?;
                                _la = recog.base.input.la(1);
                                if _la == Cypher_SP {
                                    {
                                        recog.base.set_state(1127);
                                        recog
                                            .base
                                            .match_token(Cypher_SP, &mut recog.err_handler)?;
                                    }
                                }

                                recog.base.set_state(1130);
                                recog
                                    .base
                                    .match_token(Cypher_T__21, &mut recog.err_handler)?;

                                recog.base.set_state(1132);
                                recog.err_handler.sync(&mut recog.base)?;
                                _la = recog.base.input.la(1);
                                if _la == Cypher_SP {
                                    {
                                        recog.base.set_state(1131);
                                        recog
                                            .base
                                            .match_token(Cypher_SP, &mut recog.err_handler)?;
                                    }
                                }

                                /*InvokeRule oC_UnaryAddOrSubtractExpression*/
                                recog.base.set_state(1134);
                                recog.oC_UnaryAddOrSubtractExpression()?;
                            }
                        }
                    }
                    recog.base.set_state(1139);
                    recog.err_handler.sync(&mut recog.base)?;
                    _alt = recog.interpreter.adaptive_predict(195, &mut recog.base)?;
                }
            }
            Ok(())
        })();
        match result {
            Ok(_) => {}
            Err(e @ ANTLRError::FallThrough(_)) => return Err(e),
            Err(ref re) => {
                //_localctx.exception = re;
                recog.err_handler.report_error(&mut recog.base, re);
                recog.err_handler.recover(&mut recog.base, re)?;
            }
        }
        recog.base.exit_rule()?;

        Ok(_localctx)
    }
}
//------------------- oC_UnaryAddOrSubtractExpression ----------------
pub type OC_UnaryAddOrSubtractExpressionContextAll<'input> =
    OC_UnaryAddOrSubtractExpressionContext<'input>;

pub type OC_UnaryAddOrSubtractExpressionContext<'input> =
    BaseParserRuleContext<'input, OC_UnaryAddOrSubtractExpressionContextExt<'input>>;

#[derive(Clone)]
pub struct OC_UnaryAddOrSubtractExpressionContextExt<'input> {
    ph: PhantomData<&'input str>,
}

impl<'input> CypherParserContext<'input> for OC_UnaryAddOrSubtractExpressionContext<'input> {}

impl<'input, 'a> Listenable<dyn CypherListener<'input> + 'a>
    for OC_UnaryAddOrSubtractExpressionContext<'input>
{
    fn enter(&self, listener: &mut (dyn CypherListener<'input> + 'a)) -> Result<(), ANTLRError> {
        listener.enter_every_rule(self)?;
        listener.enter_oC_UnaryAddOrSubtractExpression(self);
        Ok(())
    }
    fn exit(&self, listener: &mut (dyn CypherListener<'input> + 'a)) -> Result<(), ANTLRError> {
        listener.exit_oC_UnaryAddOrSubtractExpression(self);
        listener.exit_every_rule(self)?;
        Ok(())
    }
}

impl<'input, 'a> Visitable<dyn CypherVisitor<'input> + 'a>
    for OC_UnaryAddOrSubtractExpressionContext<'input>
{
    fn accept(&self, visitor: &mut (dyn CypherVisitor<'input> + 'a)) {
        visitor.visit_oC_UnaryAddOrSubtractExpression(self);
    }
}

impl<'input> CustomRuleContext<'input> for OC_UnaryAddOrSubtractExpressionContextExt<'input> {
    type TF = LocalTokenFactory<'input>;
    type Ctx = CypherParserContextType;
    fn get_rule_index(&self) -> usize {
        RULE_oC_UnaryAddOrSubtractExpression
    }
    //fn type_rule_index() -> usize where Self: Sized { RULE_oC_UnaryAddOrSubtractExpression }
}
antlr4rust::tid! {OC_UnaryAddOrSubtractExpressionContextExt<'a>}

impl<'input> OC_UnaryAddOrSubtractExpressionContextExt<'input> {
    fn new(
        parent: Option<Rc<dyn CypherParserContext<'input> + 'input>>,
        invoking_state: i32,
    ) -> Rc<OC_UnaryAddOrSubtractExpressionContextAll<'input>> {
        Rc::new(BaseParserRuleContext::new_parser_ctx(
            parent,
            invoking_state,
            OC_UnaryAddOrSubtractExpressionContextExt { ph: PhantomData },
        ))
    }
}

pub trait OC_UnaryAddOrSubtractExpressionContextAttrs<'input>:
    CypherParserContext<'input> + BorrowMut<OC_UnaryAddOrSubtractExpressionContextExt<'input>>
{
    fn oC_NonArithmeticOperatorExpression(
        &self,
    ) -> Option<Rc<OC_NonArithmeticOperatorExpressionContextAll<'input>>>
    where
        Self: Sized,
    {
        self.child_of_type(0)
    }
    /// Retrieves first TerminalNode corresponding to token SP
    /// Returns `None` if there is no child corresponding to token SP
    fn SP(&self) -> Option<Rc<TerminalNode<'input, CypherParserContextType>>>
    where
        Self: Sized,
    {
        self.get_token(Cypher_SP, 0)
    }
}

impl<'input> OC_UnaryAddOrSubtractExpressionContextAttrs<'input>
    for OC_UnaryAddOrSubtractExpressionContext<'input>
{
}

impl<'input, I> CypherParser<'input, I>
where
    I: TokenStream<'input, TF = LocalTokenFactory<'input>> + TidAble<'input>,
{
    pub fn oC_UnaryAddOrSubtractExpression(
        &mut self,
    ) -> Result<Rc<OC_UnaryAddOrSubtractExpressionContextAll<'input>>, ANTLRError> {
        let mut recog = self;
        let _parentctx = recog.ctx.take();
        let mut _localctx = OC_UnaryAddOrSubtractExpressionContextExt::new(
            _parentctx.clone(),
            recog.base.get_state(),
        );
        recog
            .base
            .enter_rule(_localctx.clone(), 132, RULE_oC_UnaryAddOrSubtractExpression);
        let mut _localctx: Rc<OC_UnaryAddOrSubtractExpressionContextAll> = _localctx;
        let mut _la: i32 = -1;
        let result: Result<(), ANTLRError> = (|| {
            recog.base.set_state(1146);
            recog.err_handler.sync(&mut recog.base)?;
            match recog.base.input.la(1) {
                Cypher_T__5
                | Cypher_T__7
                | Cypher_T__23
                | Cypher_T__25
                | Cypher_ALL
                | Cypher_NULL
                | Cypher_COUNT
                | Cypher_CASE
                | Cypher_ANY
                | Cypher_NONE
                | Cypher_SINGLE
                | Cypher_CAST
                | Cypher_EXISTS
                | Cypher_TRUE
                | Cypher_FALSE
                | Cypher_HexInteger
                | Cypher_DecimalInteger
                | Cypher_OctalInteger
                | Cypher_HexLetter
                | Cypher_ExponentDecimalReal
                | Cypher_RegularDecimalReal
                | Cypher_StringLiteral
                | Cypher_FILTER
                | Cypher_EXTRACT
                | Cypher_UnescapedSymbolicName
                | Cypher_EscapedSymbolicName => {
                    //recog.base.enter_outer_alt(_localctx.clone(), 1)?;
                    recog.base.enter_outer_alt(None, 1)?;
                    {
                        /*InvokeRule oC_NonArithmeticOperatorExpression*/
                        recog.base.set_state(1140);
                        recog.oC_NonArithmeticOperatorExpression()?;
                    }
                }

                Cypher_T__17 | Cypher_T__18 => {
                    //recog.base.enter_outer_alt(_localctx.clone(), 2)?;
                    recog.base.enter_outer_alt(None, 2)?;
                    {
                        {
                            recog.base.set_state(1141);
                            _la = recog.base.input.la(1);
                            if { !(_la == Cypher_T__17 || _la == Cypher_T__18) } {
                                recog.err_handler.recover_inline(&mut recog.base)?;
                            } else {
                                if recog.base.input.la(1) == TOKEN_EOF {
                                    recog.base.matched_eof = true
                                };
                                recog.err_handler.report_match(&mut recog.base);
                                recog.base.consume(&mut recog.err_handler);
                            }
                            recog.base.set_state(1143);
                            recog.err_handler.sync(&mut recog.base)?;
                            _la = recog.base.input.la(1);
                            if _la == Cypher_SP {
                                {
                                    recog.base.set_state(1142);
                                    recog.base.match_token(Cypher_SP, &mut recog.err_handler)?;
                                }
                            }

                            /*InvokeRule oC_NonArithmeticOperatorExpression*/
                            recog.base.set_state(1145);
                            recog.oC_NonArithmeticOperatorExpression()?;
                        }
                    }
                }

                _ => Err(ANTLRError::NoAltError(NoViableAltError::new(
                    &mut recog.base,
                )))?,
            }
            Ok(())
        })();
        match result {
            Ok(_) => {}
            Err(e @ ANTLRError::FallThrough(_)) => return Err(e),
            Err(ref re) => {
                //_localctx.exception = re;
                recog.err_handler.report_error(&mut recog.base, re);
                recog.err_handler.recover(&mut recog.base, re)?;
            }
        }
        recog.base.exit_rule()?;

        Ok(_localctx)
    }
}
//------------------- oC_NonArithmeticOperatorExpression ----------------
pub type OC_NonArithmeticOperatorExpressionContextAll<'input> =
    OC_NonArithmeticOperatorExpressionContext<'input>;

pub type OC_NonArithmeticOperatorExpressionContext<'input> =
    BaseParserRuleContext<'input, OC_NonArithmeticOperatorExpressionContextExt<'input>>;

#[derive(Clone)]
pub struct OC_NonArithmeticOperatorExpressionContextExt<'input> {
    ph: PhantomData<&'input str>,
}

impl<'input> CypherParserContext<'input> for OC_NonArithmeticOperatorExpressionContext<'input> {}

impl<'input, 'a> Listenable<dyn CypherListener<'input> + 'a>
    for OC_NonArithmeticOperatorExpressionContext<'input>
{
    fn enter(&self, listener: &mut (dyn CypherListener<'input> + 'a)) -> Result<(), ANTLRError> {
        listener.enter_every_rule(self)?;
        listener.enter_oC_NonArithmeticOperatorExpression(self);
        Ok(())
    }
    fn exit(&self, listener: &mut (dyn CypherListener<'input> + 'a)) -> Result<(), ANTLRError> {
        listener.exit_oC_NonArithmeticOperatorExpression(self);
        listener.exit_every_rule(self)?;
        Ok(())
    }
}

impl<'input, 'a> Visitable<dyn CypherVisitor<'input> + 'a>
    for OC_NonArithmeticOperatorExpressionContext<'input>
{
    fn accept(&self, visitor: &mut (dyn CypherVisitor<'input> + 'a)) {
        visitor.visit_oC_NonArithmeticOperatorExpression(self);
    }
}

impl<'input> CustomRuleContext<'input> for OC_NonArithmeticOperatorExpressionContextExt<'input> {
    type TF = LocalTokenFactory<'input>;
    type Ctx = CypherParserContextType;
    fn get_rule_index(&self) -> usize {
        RULE_oC_NonArithmeticOperatorExpression
    }
    //fn type_rule_index() -> usize where Self: Sized { RULE_oC_NonArithmeticOperatorExpression }
}
antlr4rust::tid! {OC_NonArithmeticOperatorExpressionContextExt<'a>}

impl<'input> OC_NonArithmeticOperatorExpressionContextExt<'input> {
    fn new(
        parent: Option<Rc<dyn CypherParserContext<'input> + 'input>>,
        invoking_state: i32,
    ) -> Rc<OC_NonArithmeticOperatorExpressionContextAll<'input>> {
        Rc::new(BaseParserRuleContext::new_parser_ctx(
            parent,
            invoking_state,
            OC_NonArithmeticOperatorExpressionContextExt { ph: PhantomData },
        ))
    }
}

pub trait OC_NonArithmeticOperatorExpressionContextAttrs<'input>:
    CypherParserContext<'input> + BorrowMut<OC_NonArithmeticOperatorExpressionContextExt<'input>>
{
    fn oC_Atom(&self) -> Option<Rc<OC_AtomContextAll<'input>>>
    where
        Self: Sized,
    {
        self.child_of_type(0)
    }
    fn oC_NodeLabels(&self) -> Option<Rc<OC_NodeLabelsContextAll<'input>>>
    where
        Self: Sized,
    {
        self.child_of_type(0)
    }
    fn oC_ListOperatorExpression_all(&self) -> Vec<Rc<OC_ListOperatorExpressionContextAll<'input>>>
    where
        Self: Sized,
    {
        self.children_of_type()
    }
    fn oC_ListOperatorExpression(
        &self,
        i: usize,
    ) -> Option<Rc<OC_ListOperatorExpressionContextAll<'input>>>
    where
        Self: Sized,
    {
        self.child_of_type(i)
    }
    fn oC_PropertyLookup_all(&self) -> Vec<Rc<OC_PropertyLookupContextAll<'input>>>
    where
        Self: Sized,
    {
        self.children_of_type()
    }
    fn oC_PropertyLookup(&self, i: usize) -> Option<Rc<OC_PropertyLookupContextAll<'input>>>
    where
        Self: Sized,
    {
        self.child_of_type(i)
    }
    /// Retrieves all `TerminalNode`s corresponding to token SP in current rule
    fn SP_all(&self) -> Vec<Rc<TerminalNode<'input, CypherParserContextType>>>
    where
        Self: Sized,
    {
        self.children_of_type()
    }
    /// Retrieves 'i's TerminalNode corresponding to token SP, starting from 0.
    /// Returns `None` if number of children corresponding to token SP is less or equal than `i`.
    fn SP(&self, i: usize) -> Option<Rc<TerminalNode<'input, CypherParserContextType>>>
    where
        Self: Sized,
    {
        self.get_token(Cypher_SP, i)
    }
}

impl<'input> OC_NonArithmeticOperatorExpressionContextAttrs<'input>
    for OC_NonArithmeticOperatorExpressionContext<'input>
{
}

impl<'input, I> CypherParser<'input, I>
where
    I: TokenStream<'input, TF = LocalTokenFactory<'input>> + TidAble<'input>,
{
    pub fn oC_NonArithmeticOperatorExpression(
        &mut self,
    ) -> Result<Rc<OC_NonArithmeticOperatorExpressionContextAll<'input>>, ANTLRError> {
        let mut recog = self;
        let _parentctx = recog.ctx.take();
        let mut _localctx = OC_NonArithmeticOperatorExpressionContextExt::new(
            _parentctx.clone(),
            recog.base.get_state(),
        );
        recog.base.enter_rule(
            _localctx.clone(),
            134,
            RULE_oC_NonArithmeticOperatorExpression,
        );
        let mut _localctx: Rc<OC_NonArithmeticOperatorExpressionContextAll> = _localctx;
        let mut _la: i32 = -1;
        let result: Result<(), ANTLRError> = (|| {
            let mut _alt: i32;
            //recog.base.enter_outer_alt(_localctx.clone(), 1)?;
            recog.base.enter_outer_alt(None, 1)?;
            {
                /*InvokeRule oC_Atom*/
                recog.base.set_state(1148);
                recog.oC_Atom()?;

                recog.base.set_state(1159);
                recog.err_handler.sync(&mut recog.base)?;
                _alt = recog.interpreter.adaptive_predict(201, &mut recog.base)?;
                while { _alt != 2 && _alt != INVALID_ALT } {
                    if _alt == 1 {
                        {
                            recog.base.set_state(1157);
                            recog.err_handler.sync(&mut recog.base)?;
                            match recog.interpreter.adaptive_predict(200, &mut recog.base)? {
                                1 => {
                                    {
                                        {
                                            recog.base.set_state(1150);
                                            recog.err_handler.sync(&mut recog.base)?;
                                            _la = recog.base.input.la(1);
                                            if _la == Cypher_SP {
                                                {
                                                    recog.base.set_state(1149);
                                                    recog.base.match_token(
                                                        Cypher_SP,
                                                        &mut recog.err_handler,
                                                    )?;
                                                }
                                            }

                                            /*InvokeRule oC_ListOperatorExpression*/
                                            recog.base.set_state(1152);
                                            recog.oC_ListOperatorExpression()?;
                                        }
                                    }
                                }
                                2 => {
                                    {
                                        {
                                            recog.base.set_state(1154);
                                            recog.err_handler.sync(&mut recog.base)?;
                                            _la = recog.base.input.la(1);
                                            if _la == Cypher_SP {
                                                {
                                                    recog.base.set_state(1153);
                                                    recog.base.match_token(
                                                        Cypher_SP,
                                                        &mut recog.err_handler,
                                                    )?;
                                                }
                                            }

                                            /*InvokeRule oC_PropertyLookup*/
                                            recog.base.set_state(1156);
                                            recog.oC_PropertyLookup()?;
                                        }
                                    }
                                }

                                _ => {}
                            }
                        }
                    }
                    recog.base.set_state(1161);
                    recog.err_handler.sync(&mut recog.base)?;
                    _alt = recog.interpreter.adaptive_predict(201, &mut recog.base)?;
                }
                recog.base.set_state(1166);
                recog.err_handler.sync(&mut recog.base)?;
                match recog.interpreter.adaptive_predict(203, &mut recog.base)? {
                    x if x == 1 => {
                        {
                            recog.base.set_state(1163);
                            recog.err_handler.sync(&mut recog.base)?;
                            _la = recog.base.input.la(1);
                            if _la == Cypher_SP {
                                {
                                    recog.base.set_state(1162);
                                    recog.base.match_token(Cypher_SP, &mut recog.err_handler)?;
                                }
                            }

                            /*InvokeRule oC_NodeLabels*/
                            recog.base.set_state(1165);
                            recog.oC_NodeLabels()?;
                        }
                    }

                    _ => {}
                }
            }
            Ok(())
        })();
        match result {
            Ok(_) => {}
            Err(e @ ANTLRError::FallThrough(_)) => return Err(e),
            Err(ref re) => {
                //_localctx.exception = re;
                recog.err_handler.report_error(&mut recog.base, re);
                recog.err_handler.recover(&mut recog.base, re)?;
            }
        }
        recog.base.exit_rule()?;

        Ok(_localctx)
    }
}
//------------------- oC_ListOperatorExpression ----------------
pub type OC_ListOperatorExpressionContextAll<'input> = OC_ListOperatorExpressionContext<'input>;

pub type OC_ListOperatorExpressionContext<'input> =
    BaseParserRuleContext<'input, OC_ListOperatorExpressionContextExt<'input>>;

#[derive(Clone)]
pub struct OC_ListOperatorExpressionContextExt<'input> {
    ph: PhantomData<&'input str>,
}

impl<'input> CypherParserContext<'input> for OC_ListOperatorExpressionContext<'input> {}

impl<'input, 'a> Listenable<dyn CypherListener<'input> + 'a>
    for OC_ListOperatorExpressionContext<'input>
{
    fn enter(&self, listener: &mut (dyn CypherListener<'input> + 'a)) -> Result<(), ANTLRError> {
        listener.enter_every_rule(self)?;
        listener.enter_oC_ListOperatorExpression(self);
        Ok(())
    }
    fn exit(&self, listener: &mut (dyn CypherListener<'input> + 'a)) -> Result<(), ANTLRError> {
        listener.exit_oC_ListOperatorExpression(self);
        listener.exit_every_rule(self)?;
        Ok(())
    }
}

impl<'input, 'a> Visitable<dyn CypherVisitor<'input> + 'a>
    for OC_ListOperatorExpressionContext<'input>
{
    fn accept(&self, visitor: &mut (dyn CypherVisitor<'input> + 'a)) {
        visitor.visit_oC_ListOperatorExpression(self);
    }
}

impl<'input> CustomRuleContext<'input> for OC_ListOperatorExpressionContextExt<'input> {
    type TF = LocalTokenFactory<'input>;
    type Ctx = CypherParserContextType;
    fn get_rule_index(&self) -> usize {
        RULE_oC_ListOperatorExpression
    }
    //fn type_rule_index() -> usize where Self: Sized { RULE_oC_ListOperatorExpression }
}
antlr4rust::tid! {OC_ListOperatorExpressionContextExt<'a>}

impl<'input> OC_ListOperatorExpressionContextExt<'input> {
    fn new(
        parent: Option<Rc<dyn CypherParserContext<'input> + 'input>>,
        invoking_state: i32,
    ) -> Rc<OC_ListOperatorExpressionContextAll<'input>> {
        Rc::new(BaseParserRuleContext::new_parser_ctx(
            parent,
            invoking_state,
            OC_ListOperatorExpressionContextExt { ph: PhantomData },
        ))
    }
}

pub trait OC_ListOperatorExpressionContextAttrs<'input>:
    CypherParserContext<'input> + BorrowMut<OC_ListOperatorExpressionContextExt<'input>>
{
    fn oC_Expression_all(&self) -> Vec<Rc<OC_ExpressionContextAll<'input>>>
    where
        Self: Sized,
    {
        self.children_of_type()
    }
    fn oC_Expression(&self, i: usize) -> Option<Rc<OC_ExpressionContextAll<'input>>>
    where
        Self: Sized,
    {
        self.child_of_type(i)
    }
}

impl<'input> OC_ListOperatorExpressionContextAttrs<'input>
    for OC_ListOperatorExpressionContext<'input>
{
}

impl<'input, I> CypherParser<'input, I>
where
    I: TokenStream<'input, TF = LocalTokenFactory<'input>> + TidAble<'input>,
{
    pub fn oC_ListOperatorExpression(
        &mut self,
    ) -> Result<Rc<OC_ListOperatorExpressionContextAll<'input>>, ANTLRError> {
        let mut recog = self;
        let _parentctx = recog.ctx.take();
        let mut _localctx =
            OC_ListOperatorExpressionContextExt::new(_parentctx.clone(), recog.base.get_state());
        recog
            .base
            .enter_rule(_localctx.clone(), 136, RULE_oC_ListOperatorExpression);
        let mut _localctx: Rc<OC_ListOperatorExpressionContextAll> = _localctx;
        let mut _la: i32 = -1;
        let result: Result<(), ANTLRError> = (|| {
            recog.base.set_state(1181);
            recog.err_handler.sync(&mut recog.base)?;
            match recog.interpreter.adaptive_predict(206, &mut recog.base)? {
                1 => {
                    //recog.base.enter_outer_alt(_localctx.clone(), 1)?;
                    recog.base.enter_outer_alt(None, 1)?;
                    {
                        {
                            recog.base.set_state(1168);
                            recog
                                .base
                                .match_token(Cypher_T__7, &mut recog.err_handler)?;

                            /*InvokeRule oC_Expression*/
                            recog.base.set_state(1169);
                            recog.oC_Expression()?;

                            recog.base.set_state(1170);
                            recog
                                .base
                                .match_token(Cypher_T__8, &mut recog.err_handler)?;
                        }
                    }
                }
                2 => {
                    //recog.base.enter_outer_alt(_localctx.clone(), 2)?;
                    recog.base.enter_outer_alt(None, 2)?;
                    {
                        {
                            recog.base.set_state(1172);
                            recog
                                .base
                                .match_token(Cypher_T__7, &mut recog.err_handler)?;

                            recog.base.set_state(1174);
                            recog.err_handler.sync(&mut recog.base)?;
                            _la = recog.base.input.la(1);
                            if (((_la) & !0x3f) == 0 && ((1usize << _la) & 84672832) != 0)
                                || _la == Cypher_ALL
                                || _la == Cypher_NOT
                                || (((_la - 82) & !0x3f) == 0
                                    && ((1usize << (_la - 82)) & 117702535) != 0)
                                || (((_la - 120) & !0x3f) == 0
                                    && ((1usize << (_la - 120)) & 39) != 0)
                            {
                                {
                                    /*InvokeRule oC_Expression*/
                                    recog.base.set_state(1173);
                                    recog.oC_Expression()?;
                                }
                            }

                            recog.base.set_state(1176);
                            recog
                                .base
                                .match_token(Cypher_T__11, &mut recog.err_handler)?;

                            recog.base.set_state(1178);
                            recog.err_handler.sync(&mut recog.base)?;
                            _la = recog.base.input.la(1);
                            if (((_la) & !0x3f) == 0 && ((1usize << _la) & 84672832) != 0)
                                || _la == Cypher_ALL
                                || _la == Cypher_NOT
                                || (((_la - 82) & !0x3f) == 0
                                    && ((1usize << (_la - 82)) & 117702535) != 0)
                                || (((_la - 120) & !0x3f) == 0
                                    && ((1usize << (_la - 120)) & 39) != 0)
                            {
                                {
                                    /*InvokeRule oC_Expression*/
                                    recog.base.set_state(1177);
                                    recog.oC_Expression()?;
                                }
                            }

                            recog.base.set_state(1180);
                            recog
                                .base
                                .match_token(Cypher_T__8, &mut recog.err_handler)?;
                        }
                    }
                }

                _ => {}
            }
            Ok(())
        })();
        match result {
            Ok(_) => {}
            Err(e @ ANTLRError::FallThrough(_)) => return Err(e),
            Err(ref re) => {
                //_localctx.exception = re;
                recog.err_handler.report_error(&mut recog.base, re);
                recog.err_handler.recover(&mut recog.base, re)?;
            }
        }
        recog.base.exit_rule()?;

        Ok(_localctx)
    }
}
//------------------- oC_PropertyLookup ----------------
pub type OC_PropertyLookupContextAll<'input> = OC_PropertyLookupContext<'input>;

pub type OC_PropertyLookupContext<'input> =
    BaseParserRuleContext<'input, OC_PropertyLookupContextExt<'input>>;

#[derive(Clone)]
pub struct OC_PropertyLookupContextExt<'input> {
    ph: PhantomData<&'input str>,
}

impl<'input> CypherParserContext<'input> for OC_PropertyLookupContext<'input> {}

impl<'input, 'a> Listenable<dyn CypherListener<'input> + 'a> for OC_PropertyLookupContext<'input> {
    fn enter(&self, listener: &mut (dyn CypherListener<'input> + 'a)) -> Result<(), ANTLRError> {
        listener.enter_every_rule(self)?;
        listener.enter_oC_PropertyLookup(self);
        Ok(())
    }
    fn exit(&self, listener: &mut (dyn CypherListener<'input> + 'a)) -> Result<(), ANTLRError> {
        listener.exit_oC_PropertyLookup(self);
        listener.exit_every_rule(self)?;
        Ok(())
    }
}

impl<'input, 'a> Visitable<dyn CypherVisitor<'input> + 'a> for OC_PropertyLookupContext<'input> {
    fn accept(&self, visitor: &mut (dyn CypherVisitor<'input> + 'a)) {
        visitor.visit_oC_PropertyLookup(self);
    }
}

impl<'input> CustomRuleContext<'input> for OC_PropertyLookupContextExt<'input> {
    type TF = LocalTokenFactory<'input>;
    type Ctx = CypherParserContextType;
    fn get_rule_index(&self) -> usize {
        RULE_oC_PropertyLookup
    }
    //fn type_rule_index() -> usize where Self: Sized { RULE_oC_PropertyLookup }
}
antlr4rust::tid! {OC_PropertyLookupContextExt<'a>}

impl<'input> OC_PropertyLookupContextExt<'input> {
    fn new(
        parent: Option<Rc<dyn CypherParserContext<'input> + 'input>>,
        invoking_state: i32,
    ) -> Rc<OC_PropertyLookupContextAll<'input>> {
        Rc::new(BaseParserRuleContext::new_parser_ctx(
            parent,
            invoking_state,
            OC_PropertyLookupContextExt { ph: PhantomData },
        ))
    }
}

pub trait OC_PropertyLookupContextAttrs<'input>:
    CypherParserContext<'input> + BorrowMut<OC_PropertyLookupContextExt<'input>>
{
    fn oC_PropertyKeyName(&self) -> Option<Rc<OC_PropertyKeyNameContextAll<'input>>>
    where
        Self: Sized,
    {
        self.child_of_type(0)
    }
    /// Retrieves first TerminalNode corresponding to token SP
    /// Returns `None` if there is no child corresponding to token SP
    fn SP(&self) -> Option<Rc<TerminalNode<'input, CypherParserContextType>>>
    where
        Self: Sized,
    {
        self.get_token(Cypher_SP, 0)
    }
}

impl<'input> OC_PropertyLookupContextAttrs<'input> for OC_PropertyLookupContext<'input> {}

impl<'input, I> CypherParser<'input, I>
where
    I: TokenStream<'input, TF = LocalTokenFactory<'input>> + TidAble<'input>,
{
    pub fn oC_PropertyLookup(
        &mut self,
    ) -> Result<Rc<OC_PropertyLookupContextAll<'input>>, ANTLRError> {
        let mut recog = self;
        let _parentctx = recog.ctx.take();
        let mut _localctx =
            OC_PropertyLookupContextExt::new(_parentctx.clone(), recog.base.get_state());
        recog
            .base
            .enter_rule(_localctx.clone(), 138, RULE_oC_PropertyLookup);
        let mut _localctx: Rc<OC_PropertyLookupContextAll> = _localctx;
        let mut _la: i32 = -1;
        let result: Result<(), ANTLRError> = (|| {
            //recog.base.enter_outer_alt(_localctx.clone(), 1)?;
            recog.base.enter_outer_alt(None, 1)?;
            {
                recog.base.set_state(1183);
                recog
                    .base
                    .match_token(Cypher_T__22, &mut recog.err_handler)?;

                recog.base.set_state(1185);
                recog.err_handler.sync(&mut recog.base)?;
                _la = recog.base.input.la(1);
                if _la == Cypher_SP {
                    {
                        recog.base.set_state(1184);
                        recog.base.match_token(Cypher_SP, &mut recog.err_handler)?;
                    }
                }

                recog.base.set_state(1189);
                recog.err_handler.sync(&mut recog.base)?;
                match recog.base.input.la(1) {
                    Cypher_UNION
                    | Cypher_ALL
                    | Cypher_OPTIONAL
                    | Cypher_MATCH
                    | Cypher_UNWIND
                    | Cypher_AS
                    | Cypher_MERGE
                    | Cypher_ON
                    | Cypher_CREATE
                    | Cypher_SET
                    | Cypher_DETACH
                    | Cypher_DELETE
                    | Cypher_REMOVE
                    | Cypher_WITH
                    | Cypher_RETURN
                    | Cypher_DISTINCT
                    | Cypher_ORDER
                    | Cypher_BY
                    | Cypher_L_SKIP
                    | Cypher_LIMIT
                    | Cypher_ASCENDING
                    | Cypher_ASC
                    | Cypher_DESCENDING
                    | Cypher_DESC
                    | Cypher_WHERE
                    | Cypher_OR
                    | Cypher_XOR
                    | Cypher_AND
                    | Cypher_NOT
                    | Cypher_STARTS
                    | Cypher_ENDS
                    | Cypher_CONTAINS
                    | Cypher_IN
                    | Cypher_IS
                    | Cypher_NULL
                    | Cypher_COUNT
                    | Cypher_CASE
                    | Cypher_ELSE
                    | Cypher_END
                    | Cypher_WHEN
                    | Cypher_THEN
                    | Cypher_ANY
                    | Cypher_NONE
                    | Cypher_SINGLE
                    | Cypher_CAST
                    | Cypher_EXISTS
                    | Cypher_TRUE
                    | Cypher_FALSE
                    | Cypher_HexLetter
                    | Cypher_CONSTRAINT
                    | Cypher_DO
                    | Cypher_FOR
                    | Cypher_REQUIRE
                    | Cypher_UNIQUE
                    | Cypher_MANDATORY
                    | Cypher_SCALAR
                    | Cypher_OF
                    | Cypher_ADD
                    | Cypher_DROP
                    | Cypher_FILTER
                    | Cypher_EXTRACT
                    | Cypher_UnescapedSymbolicName
                    | Cypher_EscapedSymbolicName => {
                        {
                            /*InvokeRule oC_PropertyKeyName*/
                            recog.base.set_state(1187);
                            recog.oC_PropertyKeyName()?;
                        }
                    }

                    Cypher_T__4 => {
                        recog.base.set_state(1188);
                        recog
                            .base
                            .match_token(Cypher_T__4, &mut recog.err_handler)?;
                    }

                    _ => Err(ANTLRError::NoAltError(NoViableAltError::new(
                        &mut recog.base,
                    )))?,
                }
            }
            Ok(())
        })();
        match result {
            Ok(_) => {}
            Err(e @ ANTLRError::FallThrough(_)) => return Err(e),
            Err(ref re) => {
                //_localctx.exception = re;
                recog.err_handler.report_error(&mut recog.base, re);
                recog.err_handler.recover(&mut recog.base, re)?;
            }
        }
        recog.base.exit_rule()?;

        Ok(_localctx)
    }
}
//------------------- oC_Atom ----------------
pub type OC_AtomContextAll<'input> = OC_AtomContext<'input>;

pub type OC_AtomContext<'input> = BaseParserRuleContext<'input, OC_AtomContextExt<'input>>;

#[derive(Clone)]
pub struct OC_AtomContextExt<'input> {
    ph: PhantomData<&'input str>,
}

impl<'input> CypherParserContext<'input> for OC_AtomContext<'input> {}

impl<'input, 'a> Listenable<dyn CypherListener<'input> + 'a> for OC_AtomContext<'input> {
    fn enter(&self, listener: &mut (dyn CypherListener<'input> + 'a)) -> Result<(), ANTLRError> {
        listener.enter_every_rule(self)?;
        listener.enter_oC_Atom(self);
        Ok(())
    }
    fn exit(&self, listener: &mut (dyn CypherListener<'input> + 'a)) -> Result<(), ANTLRError> {
        listener.exit_oC_Atom(self);
        listener.exit_every_rule(self)?;
        Ok(())
    }
}

impl<'input, 'a> Visitable<dyn CypherVisitor<'input> + 'a> for OC_AtomContext<'input> {
    fn accept(&self, visitor: &mut (dyn CypherVisitor<'input> + 'a)) {
        visitor.visit_oC_Atom(self);
    }
}

impl<'input> CustomRuleContext<'input> for OC_AtomContextExt<'input> {
    type TF = LocalTokenFactory<'input>;
    type Ctx = CypherParserContextType;
    fn get_rule_index(&self) -> usize {
        RULE_oC_Atom
    }
    //fn type_rule_index() -> usize where Self: Sized { RULE_oC_Atom }
}
antlr4rust::tid! {OC_AtomContextExt<'a>}

impl<'input> OC_AtomContextExt<'input> {
    fn new(
        parent: Option<Rc<dyn CypherParserContext<'input> + 'input>>,
        invoking_state: i32,
    ) -> Rc<OC_AtomContextAll<'input>> {
        Rc::new(BaseParserRuleContext::new_parser_ctx(
            parent,
            invoking_state,
            OC_AtomContextExt { ph: PhantomData },
        ))
    }
}

pub trait OC_AtomContextAttrs<'input>:
    CypherParserContext<'input> + BorrowMut<OC_AtomContextExt<'input>>
{
    fn oC_Literal(&self) -> Option<Rc<OC_LiteralContextAll<'input>>>
    where
        Self: Sized,
    {
        self.child_of_type(0)
    }
    fn oC_Parameter(&self) -> Option<Rc<OC_ParameterContextAll<'input>>>
    where
        Self: Sized,
    {
        self.child_of_type(0)
    }
    fn oC_CaseExpression(&self) -> Option<Rc<OC_CaseExpressionContextAll<'input>>>
    where
        Self: Sized,
    {
        self.child_of_type(0)
    }
    fn oC_CastExpression(&self) -> Option<Rc<OC_CastExpressionContextAll<'input>>>
    where
        Self: Sized,
    {
        self.child_of_type(0)
    }
    /// Retrieves first TerminalNode corresponding to token COUNT
    /// Returns `None` if there is no child corresponding to token COUNT
    fn COUNT(&self) -> Option<Rc<TerminalNode<'input, CypherParserContextType>>>
    where
        Self: Sized,
    {
        self.get_token(Cypher_COUNT, 0)
    }
    /// Retrieves all `TerminalNode`s corresponding to token SP in current rule
    fn SP_all(&self) -> Vec<Rc<TerminalNode<'input, CypherParserContextType>>>
    where
        Self: Sized,
    {
        self.children_of_type()
    }
    /// Retrieves 'i's TerminalNode corresponding to token SP, starting from 0.
    /// Returns `None` if number of children corresponding to token SP is less or equal than `i`.
    fn SP(&self, i: usize) -> Option<Rc<TerminalNode<'input, CypherParserContextType>>>
    where
        Self: Sized,
    {
        self.get_token(Cypher_SP, i)
    }
    fn oC_ListComprehension(&self) -> Option<Rc<OC_ListComprehensionContextAll<'input>>>
    where
        Self: Sized,
    {
        self.child_of_type(0)
    }
    fn oC_PatternComprehension(&self) -> Option<Rc<OC_PatternComprehensionContextAll<'input>>>
    where
        Self: Sized,
    {
        self.child_of_type(0)
    }
    fn oC_Quantifier(&self) -> Option<Rc<OC_QuantifierContextAll<'input>>>
    where
        Self: Sized,
    {
        self.child_of_type(0)
    }
    fn oC_PatternPredicate(&self) -> Option<Rc<OC_PatternPredicateContextAll<'input>>>
    where
        Self: Sized,
    {
        self.child_of_type(0)
    }
    fn oC_ParenthesizedExpression(&self) -> Option<Rc<OC_ParenthesizedExpressionContextAll<'input>>>
    where
        Self: Sized,
    {
        self.child_of_type(0)
    }
    fn oC_FunctionInvocation(&self) -> Option<Rc<OC_FunctionInvocationContextAll<'input>>>
    where
        Self: Sized,
    {
        self.child_of_type(0)
    }
    fn oC_ExistentialSubquery(&self) -> Option<Rc<OC_ExistentialSubqueryContextAll<'input>>>
    where
        Self: Sized,
    {
        self.child_of_type(0)
    }
    fn oC_Variable(&self) -> Option<Rc<OC_VariableContextAll<'input>>>
    where
        Self: Sized,
    {
        self.child_of_type(0)
    }
}

impl<'input> OC_AtomContextAttrs<'input> for OC_AtomContext<'input> {}

impl<'input, I> CypherParser<'input, I>
where
    I: TokenStream<'input, TF = LocalTokenFactory<'input>> + TidAble<'input>,
{
    pub fn oC_Atom(&mut self) -> Result<Rc<OC_AtomContextAll<'input>>, ANTLRError> {
        let mut recog = self;
        let _parentctx = recog.ctx.take();
        let mut _localctx = OC_AtomContextExt::new(_parentctx.clone(), recog.base.get_state());
        recog.base.enter_rule(_localctx.clone(), 140, RULE_oC_Atom);
        let mut _localctx: Rc<OC_AtomContextAll> = _localctx;
        let mut _la: i32 = -1;
        let result: Result<(), ANTLRError> = (|| {
            recog.base.set_state(1216);
            recog.err_handler.sync(&mut recog.base)?;
            match recog.interpreter.adaptive_predict(212, &mut recog.base)? {
                1 => {
                    //recog.base.enter_outer_alt(_localctx.clone(), 1)?;
                    recog.base.enter_outer_alt(None, 1)?;
                    {
                        /*InvokeRule oC_Literal*/
                        recog.base.set_state(1191);
                        recog.oC_Literal()?;
                    }
                }
                2 => {
                    //recog.base.enter_outer_alt(_localctx.clone(), 2)?;
                    recog.base.enter_outer_alt(None, 2)?;
                    {
                        /*InvokeRule oC_Parameter*/
                        recog.base.set_state(1192);
                        recog.oC_Parameter()?;
                    }
                }
                3 => {
                    //recog.base.enter_outer_alt(_localctx.clone(), 3)?;
                    recog.base.enter_outer_alt(None, 3)?;
                    {
                        /*InvokeRule oC_CaseExpression*/
                        recog.base.set_state(1193);
                        recog.oC_CaseExpression()?;
                    }
                }
                4 => {
                    //recog.base.enter_outer_alt(_localctx.clone(), 4)?;
                    recog.base.enter_outer_alt(None, 4)?;
                    {
                        /*InvokeRule oC_CastExpression*/
                        recog.base.set_state(1194);
                        recog.oC_CastExpression()?;
                    }
                }
                5 => {
                    //recog.base.enter_outer_alt(_localctx.clone(), 5)?;
                    recog.base.enter_outer_alt(None, 5)?;
                    {
                        {
                            recog.base.set_state(1195);
                            recog
                                .base
                                .match_token(Cypher_COUNT, &mut recog.err_handler)?;

                            recog.base.set_state(1197);
                            recog.err_handler.sync(&mut recog.base)?;
                            _la = recog.base.input.la(1);
                            if _la == Cypher_SP {
                                {
                                    recog.base.set_state(1196);
                                    recog.base.match_token(Cypher_SP, &mut recog.err_handler)?;
                                }
                            }

                            recog.base.set_state(1199);
                            recog
                                .base
                                .match_token(Cypher_T__5, &mut recog.err_handler)?;

                            recog.base.set_state(1201);
                            recog.err_handler.sync(&mut recog.base)?;
                            _la = recog.base.input.la(1);
                            if _la == Cypher_SP {
                                {
                                    recog.base.set_state(1200);
                                    recog.base.match_token(Cypher_SP, &mut recog.err_handler)?;
                                }
                            }

                            recog.base.set_state(1203);
                            recog
                                .base
                                .match_token(Cypher_T__4, &mut recog.err_handler)?;

                            recog.base.set_state(1205);
                            recog.err_handler.sync(&mut recog.base)?;
                            _la = recog.base.input.la(1);
                            if _la == Cypher_SP {
                                {
                                    recog.base.set_state(1204);
                                    recog.base.match_token(Cypher_SP, &mut recog.err_handler)?;
                                }
                            }

                            recog.base.set_state(1207);
                            recog
                                .base
                                .match_token(Cypher_T__6, &mut recog.err_handler)?;
                        }
                    }
                }
                6 => {
                    //recog.base.enter_outer_alt(_localctx.clone(), 6)?;
                    recog.base.enter_outer_alt(None, 6)?;
                    {
                        /*InvokeRule oC_ListComprehension*/
                        recog.base.set_state(1208);
                        recog.oC_ListComprehension()?;
                    }
                }
                7 => {
                    //recog.base.enter_outer_alt(_localctx.clone(), 7)?;
                    recog.base.enter_outer_alt(None, 7)?;
                    {
                        /*InvokeRule oC_PatternComprehension*/
                        recog.base.set_state(1209);
                        recog.oC_PatternComprehension()?;
                    }
                }
                8 => {
                    //recog.base.enter_outer_alt(_localctx.clone(), 8)?;
                    recog.base.enter_outer_alt(None, 8)?;
                    {
                        /*InvokeRule oC_Quantifier*/
                        recog.base.set_state(1210);
                        recog.oC_Quantifier()?;
                    }
                }
                9 => {
                    //recog.base.enter_outer_alt(_localctx.clone(), 9)?;
                    recog.base.enter_outer_alt(None, 9)?;
                    {
                        /*InvokeRule oC_PatternPredicate*/
                        recog.base.set_state(1211);
                        recog.oC_PatternPredicate()?;
                    }
                }
                10 => {
                    //recog.base.enter_outer_alt(_localctx.clone(), 10)?;
                    recog.base.enter_outer_alt(None, 10)?;
                    {
                        /*InvokeRule oC_ParenthesizedExpression*/
                        recog.base.set_state(1212);
                        recog.oC_ParenthesizedExpression()?;
                    }
                }
                11 => {
                    //recog.base.enter_outer_alt(_localctx.clone(), 11)?;
                    recog.base.enter_outer_alt(None, 11)?;
                    {
                        /*InvokeRule oC_FunctionInvocation*/
                        recog.base.set_state(1213);
                        recog.oC_FunctionInvocation()?;
                    }
                }
                12 => {
                    //recog.base.enter_outer_alt(_localctx.clone(), 12)?;
                    recog.base.enter_outer_alt(None, 12)?;
                    {
                        /*InvokeRule oC_ExistentialSubquery*/
                        recog.base.set_state(1214);
                        recog.oC_ExistentialSubquery()?;
                    }
                }
                13 => {
                    //recog.base.enter_outer_alt(_localctx.clone(), 13)?;
                    recog.base.enter_outer_alt(None, 13)?;
                    {
                        /*InvokeRule oC_Variable*/
                        recog.base.set_state(1215);
                        recog.oC_Variable()?;
                    }
                }

                _ => {}
            }
            Ok(())
        })();
        match result {
            Ok(_) => {}
            Err(e @ ANTLRError::FallThrough(_)) => return Err(e),
            Err(ref re) => {
                //_localctx.exception = re;
                recog.err_handler.report_error(&mut recog.base, re);
                recog.err_handler.recover(&mut recog.base, re)?;
            }
        }
        recog.base.exit_rule()?;

        Ok(_localctx)
    }
}
//------------------- oC_CaseExpression ----------------
pub type OC_CaseExpressionContextAll<'input> = OC_CaseExpressionContext<'input>;

pub type OC_CaseExpressionContext<'input> =
    BaseParserRuleContext<'input, OC_CaseExpressionContextExt<'input>>;

#[derive(Clone)]
pub struct OC_CaseExpressionContextExt<'input> {
    ph: PhantomData<&'input str>,
}

impl<'input> CypherParserContext<'input> for OC_CaseExpressionContext<'input> {}

impl<'input, 'a> Listenable<dyn CypherListener<'input> + 'a> for OC_CaseExpressionContext<'input> {
    fn enter(&self, listener: &mut (dyn CypherListener<'input> + 'a)) -> Result<(), ANTLRError> {
        listener.enter_every_rule(self)?;
        listener.enter_oC_CaseExpression(self);
        Ok(())
    }
    fn exit(&self, listener: &mut (dyn CypherListener<'input> + 'a)) -> Result<(), ANTLRError> {
        listener.exit_oC_CaseExpression(self);
        listener.exit_every_rule(self)?;
        Ok(())
    }
}

impl<'input, 'a> Visitable<dyn CypherVisitor<'input> + 'a> for OC_CaseExpressionContext<'input> {
    fn accept(&self, visitor: &mut (dyn CypherVisitor<'input> + 'a)) {
        visitor.visit_oC_CaseExpression(self);
    }
}

impl<'input> CustomRuleContext<'input> for OC_CaseExpressionContextExt<'input> {
    type TF = LocalTokenFactory<'input>;
    type Ctx = CypherParserContextType;
    fn get_rule_index(&self) -> usize {
        RULE_oC_CaseExpression
    }
    //fn type_rule_index() -> usize where Self: Sized { RULE_oC_CaseExpression }
}
antlr4rust::tid! {OC_CaseExpressionContextExt<'a>}

impl<'input> OC_CaseExpressionContextExt<'input> {
    fn new(
        parent: Option<Rc<dyn CypherParserContext<'input> + 'input>>,
        invoking_state: i32,
    ) -> Rc<OC_CaseExpressionContextAll<'input>> {
        Rc::new(BaseParserRuleContext::new_parser_ctx(
            parent,
            invoking_state,
            OC_CaseExpressionContextExt { ph: PhantomData },
        ))
    }
}

pub trait OC_CaseExpressionContextAttrs<'input>:
    CypherParserContext<'input> + BorrowMut<OC_CaseExpressionContextExt<'input>>
{
    /// Retrieves first TerminalNode corresponding to token END
    /// Returns `None` if there is no child corresponding to token END
    fn END(&self) -> Option<Rc<TerminalNode<'input, CypherParserContextType>>>
    where
        Self: Sized,
    {
        self.get_token(Cypher_END, 0)
    }
    /// Retrieves first TerminalNode corresponding to token ELSE
    /// Returns `None` if there is no child corresponding to token ELSE
    fn ELSE(&self) -> Option<Rc<TerminalNode<'input, CypherParserContextType>>>
    where
        Self: Sized,
    {
        self.get_token(Cypher_ELSE, 0)
    }
    fn oC_Expression_all(&self) -> Vec<Rc<OC_ExpressionContextAll<'input>>>
    where
        Self: Sized,
    {
        self.children_of_type()
    }
    fn oC_Expression(&self, i: usize) -> Option<Rc<OC_ExpressionContextAll<'input>>>
    where
        Self: Sized,
    {
        self.child_of_type(i)
    }
    /// Retrieves all `TerminalNode`s corresponding to token SP in current rule
    fn SP_all(&self) -> Vec<Rc<TerminalNode<'input, CypherParserContextType>>>
    where
        Self: Sized,
    {
        self.children_of_type()
    }
    /// Retrieves 'i's TerminalNode corresponding to token SP, starting from 0.
    /// Returns `None` if number of children corresponding to token SP is less or equal than `i`.
    fn SP(&self, i: usize) -> Option<Rc<TerminalNode<'input, CypherParserContextType>>>
    where
        Self: Sized,
    {
        self.get_token(Cypher_SP, i)
    }
    /// Retrieves first TerminalNode corresponding to token CASE
    /// Returns `None` if there is no child corresponding to token CASE
    fn CASE(&self) -> Option<Rc<TerminalNode<'input, CypherParserContextType>>>
    where
        Self: Sized,
    {
        self.get_token(Cypher_CASE, 0)
    }
    fn oC_CaseAlternative_all(&self) -> Vec<Rc<OC_CaseAlternativeContextAll<'input>>>
    where
        Self: Sized,
    {
        self.children_of_type()
    }
    fn oC_CaseAlternative(&self, i: usize) -> Option<Rc<OC_CaseAlternativeContextAll<'input>>>
    where
        Self: Sized,
    {
        self.child_of_type(i)
    }
}

impl<'input> OC_CaseExpressionContextAttrs<'input> for OC_CaseExpressionContext<'input> {}

impl<'input, I> CypherParser<'input, I>
where
    I: TokenStream<'input, TF = LocalTokenFactory<'input>> + TidAble<'input>,
{
    pub fn oC_CaseExpression(
        &mut self,
    ) -> Result<Rc<OC_CaseExpressionContextAll<'input>>, ANTLRError> {
        let mut recog = self;
        let _parentctx = recog.ctx.take();
        let mut _localctx =
            OC_CaseExpressionContextExt::new(_parentctx.clone(), recog.base.get_state());
        recog
            .base
            .enter_rule(_localctx.clone(), 142, RULE_oC_CaseExpression);
        let mut _localctx: Rc<OC_CaseExpressionContextAll> = _localctx;
        let mut _la: i32 = -1;
        let result: Result<(), ANTLRError> = (|| {
            let mut _alt: i32;
            //recog.base.enter_outer_alt(_localctx.clone(), 1)?;
            recog.base.enter_outer_alt(None, 1)?;
            {
                recog.base.set_state(1240);
                recog.err_handler.sync(&mut recog.base)?;
                match recog.interpreter.adaptive_predict(218, &mut recog.base)? {
                    1 => {
                        {
                            {
                                recog.base.set_state(1218);
                                recog
                                    .base
                                    .match_token(Cypher_CASE, &mut recog.err_handler)?;

                                recog.base.set_state(1223);
                                recog.err_handler.sync(&mut recog.base)?;
                                _alt = 1;
                                loop {
                                    match _alt {
                                        x if x == 1 => {
                                            {
                                                recog.base.set_state(1220);
                                                recog.err_handler.sync(&mut recog.base)?;
                                                _la = recog.base.input.la(1);
                                                if _la == Cypher_SP {
                                                    {
                                                        recog.base.set_state(1219);
                                                        recog.base.match_token(
                                                            Cypher_SP,
                                                            &mut recog.err_handler,
                                                        )?;
                                                    }
                                                }

                                                /*InvokeRule oC_CaseAlternative*/
                                                recog.base.set_state(1222);
                                                recog.oC_CaseAlternative()?;
                                            }
                                        }

                                        _ => Err(ANTLRError::NoAltError(NoViableAltError::new(
                                            &mut recog.base,
                                        )))?,
                                    }
                                    recog.base.set_state(1225);
                                    recog.err_handler.sync(&mut recog.base)?;
                                    _alt =
                                        recog.interpreter.adaptive_predict(214, &mut recog.base)?;
                                    if _alt == 2 || _alt == INVALID_ALT {
                                        break;
                                    }
                                }
                            }
                        }
                    }
                    2 => {
                        {
                            {
                                recog.base.set_state(1227);
                                recog
                                    .base
                                    .match_token(Cypher_CASE, &mut recog.err_handler)?;

                                recog.base.set_state(1229);
                                recog.err_handler.sync(&mut recog.base)?;
                                _la = recog.base.input.la(1);
                                if _la == Cypher_SP {
                                    {
                                        recog.base.set_state(1228);
                                        recog
                                            .base
                                            .match_token(Cypher_SP, &mut recog.err_handler)?;
                                    }
                                }

                                /*InvokeRule oC_Expression*/
                                recog.base.set_state(1231);
                                recog.oC_Expression()?;

                                recog.base.set_state(1236);
                                recog.err_handler.sync(&mut recog.base)?;
                                _alt = 1;
                                loop {
                                    match _alt {
                                        x if x == 1 => {
                                            {
                                                recog.base.set_state(1233);
                                                recog.err_handler.sync(&mut recog.base)?;
                                                _la = recog.base.input.la(1);
                                                if _la == Cypher_SP {
                                                    {
                                                        recog.base.set_state(1232);
                                                        recog.base.match_token(
                                                            Cypher_SP,
                                                            &mut recog.err_handler,
                                                        )?;
                                                    }
                                                }

                                                /*InvokeRule oC_CaseAlternative*/
                                                recog.base.set_state(1235);
                                                recog.oC_CaseAlternative()?;
                                            }
                                        }

                                        _ => Err(ANTLRError::NoAltError(NoViableAltError::new(
                                            &mut recog.base,
                                        )))?,
                                    }
                                    recog.base.set_state(1238);
                                    recog.err_handler.sync(&mut recog.base)?;
                                    _alt =
                                        recog.interpreter.adaptive_predict(217, &mut recog.base)?;
                                    if _alt == 2 || _alt == INVALID_ALT {
                                        break;
                                    }
                                }
                            }
                        }
                    }

                    _ => {}
                }
                recog.base.set_state(1250);
                recog.err_handler.sync(&mut recog.base)?;
                match recog.interpreter.adaptive_predict(221, &mut recog.base)? {
                    x if x == 1 => {
                        {
                            recog.base.set_state(1243);
                            recog.err_handler.sync(&mut recog.base)?;
                            _la = recog.base.input.la(1);
                            if _la == Cypher_SP {
                                {
                                    recog.base.set_state(1242);
                                    recog.base.match_token(Cypher_SP, &mut recog.err_handler)?;
                                }
                            }

                            recog.base.set_state(1245);
                            recog
                                .base
                                .match_token(Cypher_ELSE, &mut recog.err_handler)?;

                            recog.base.set_state(1247);
                            recog.err_handler.sync(&mut recog.base)?;
                            _la = recog.base.input.la(1);
                            if _la == Cypher_SP {
                                {
                                    recog.base.set_state(1246);
                                    recog.base.match_token(Cypher_SP, &mut recog.err_handler)?;
                                }
                            }

                            /*InvokeRule oC_Expression*/
                            recog.base.set_state(1249);
                            recog.oC_Expression()?;
                        }
                    }

                    _ => {}
                }
                recog.base.set_state(1253);
                recog.err_handler.sync(&mut recog.base)?;
                _la = recog.base.input.la(1);
                if _la == Cypher_SP {
                    {
                        recog.base.set_state(1252);
                        recog.base.match_token(Cypher_SP, &mut recog.err_handler)?;
                    }
                }

                recog.base.set_state(1255);
                recog.base.match_token(Cypher_END, &mut recog.err_handler)?;
            }
            Ok(())
        })();
        match result {
            Ok(_) => {}
            Err(e @ ANTLRError::FallThrough(_)) => return Err(e),
            Err(ref re) => {
                //_localctx.exception = re;
                recog.err_handler.report_error(&mut recog.base, re);
                recog.err_handler.recover(&mut recog.base, re)?;
            }
        }
        recog.base.exit_rule()?;

        Ok(_localctx)
    }
}
//------------------- oC_CaseAlternative ----------------
pub type OC_CaseAlternativeContextAll<'input> = OC_CaseAlternativeContext<'input>;

pub type OC_CaseAlternativeContext<'input> =
    BaseParserRuleContext<'input, OC_CaseAlternativeContextExt<'input>>;

#[derive(Clone)]
pub struct OC_CaseAlternativeContextExt<'input> {
    ph: PhantomData<&'input str>,
}

impl<'input> CypherParserContext<'input> for OC_CaseAlternativeContext<'input> {}

impl<'input, 'a> Listenable<dyn CypherListener<'input> + 'a> for OC_CaseAlternativeContext<'input> {
    fn enter(&self, listener: &mut (dyn CypherListener<'input> + 'a)) -> Result<(), ANTLRError> {
        listener.enter_every_rule(self)?;
        listener.enter_oC_CaseAlternative(self);
        Ok(())
    }
    fn exit(&self, listener: &mut (dyn CypherListener<'input> + 'a)) -> Result<(), ANTLRError> {
        listener.exit_oC_CaseAlternative(self);
        listener.exit_every_rule(self)?;
        Ok(())
    }
}

impl<'input, 'a> Visitable<dyn CypherVisitor<'input> + 'a> for OC_CaseAlternativeContext<'input> {
    fn accept(&self, visitor: &mut (dyn CypherVisitor<'input> + 'a)) {
        visitor.visit_oC_CaseAlternative(self);
    }
}

impl<'input> CustomRuleContext<'input> for OC_CaseAlternativeContextExt<'input> {
    type TF = LocalTokenFactory<'input>;
    type Ctx = CypherParserContextType;
    fn get_rule_index(&self) -> usize {
        RULE_oC_CaseAlternative
    }
    //fn type_rule_index() -> usize where Self: Sized { RULE_oC_CaseAlternative }
}
antlr4rust::tid! {OC_CaseAlternativeContextExt<'a>}

impl<'input> OC_CaseAlternativeContextExt<'input> {
    fn new(
        parent: Option<Rc<dyn CypherParserContext<'input> + 'input>>,
        invoking_state: i32,
    ) -> Rc<OC_CaseAlternativeContextAll<'input>> {
        Rc::new(BaseParserRuleContext::new_parser_ctx(
            parent,
            invoking_state,
            OC_CaseAlternativeContextExt { ph: PhantomData },
        ))
    }
}

pub trait OC_CaseAlternativeContextAttrs<'input>:
    CypherParserContext<'input> + BorrowMut<OC_CaseAlternativeContextExt<'input>>
{
    /// Retrieves first TerminalNode corresponding to token WHEN
    /// Returns `None` if there is no child corresponding to token WHEN
    fn WHEN(&self) -> Option<Rc<TerminalNode<'input, CypherParserContextType>>>
    where
        Self: Sized,
    {
        self.get_token(Cypher_WHEN, 0)
    }
    fn oC_Expression_all(&self) -> Vec<Rc<OC_ExpressionContextAll<'input>>>
    where
        Self: Sized,
    {
        self.children_of_type()
    }
    fn oC_Expression(&self, i: usize) -> Option<Rc<OC_ExpressionContextAll<'input>>>
    where
        Self: Sized,
    {
        self.child_of_type(i)
    }
    /// Retrieves first TerminalNode corresponding to token THEN
    /// Returns `None` if there is no child corresponding to token THEN
    fn THEN(&self) -> Option<Rc<TerminalNode<'input, CypherParserContextType>>>
    where
        Self: Sized,
    {
        self.get_token(Cypher_THEN, 0)
    }
    /// Retrieves all `TerminalNode`s corresponding to token SP in current rule
    fn SP_all(&self) -> Vec<Rc<TerminalNode<'input, CypherParserContextType>>>
    where
        Self: Sized,
    {
        self.children_of_type()
    }
    /// Retrieves 'i's TerminalNode corresponding to token SP, starting from 0.
    /// Returns `None` if number of children corresponding to token SP is less or equal than `i`.
    fn SP(&self, i: usize) -> Option<Rc<TerminalNode<'input, CypherParserContextType>>>
    where
        Self: Sized,
    {
        self.get_token(Cypher_SP, i)
    }
}

impl<'input> OC_CaseAlternativeContextAttrs<'input> for OC_CaseAlternativeContext<'input> {}

impl<'input, I> CypherParser<'input, I>
where
    I: TokenStream<'input, TF = LocalTokenFactory<'input>> + TidAble<'input>,
{
    pub fn oC_CaseAlternative(
        &mut self,
    ) -> Result<Rc<OC_CaseAlternativeContextAll<'input>>, ANTLRError> {
        let mut recog = self;
        let _parentctx = recog.ctx.take();
        let mut _localctx =
            OC_CaseAlternativeContextExt::new(_parentctx.clone(), recog.base.get_state());
        recog
            .base
            .enter_rule(_localctx.clone(), 144, RULE_oC_CaseAlternative);
        let mut _localctx: Rc<OC_CaseAlternativeContextAll> = _localctx;
        let mut _la: i32 = -1;
        let result: Result<(), ANTLRError> = (|| {
            //recog.base.enter_outer_alt(_localctx.clone(), 1)?;
            recog.base.enter_outer_alt(None, 1)?;
            {
                recog.base.set_state(1257);
                recog
                    .base
                    .match_token(Cypher_WHEN, &mut recog.err_handler)?;

                recog.base.set_state(1259);
                recog.err_handler.sync(&mut recog.base)?;
                _la = recog.base.input.la(1);
                if _la == Cypher_SP {
                    {
                        recog.base.set_state(1258);
                        recog.base.match_token(Cypher_SP, &mut recog.err_handler)?;
                    }
                }

                /*InvokeRule oC_Expression*/
                recog.base.set_state(1261);
                recog.oC_Expression()?;

                recog.base.set_state(1263);
                recog.err_handler.sync(&mut recog.base)?;
                _la = recog.base.input.la(1);
                if _la == Cypher_SP {
                    {
                        recog.base.set_state(1262);
                        recog.base.match_token(Cypher_SP, &mut recog.err_handler)?;
                    }
                }

                recog.base.set_state(1265);
                recog
                    .base
                    .match_token(Cypher_THEN, &mut recog.err_handler)?;

                recog.base.set_state(1267);
                recog.err_handler.sync(&mut recog.base)?;
                _la = recog.base.input.la(1);
                if _la == Cypher_SP {
                    {
                        recog.base.set_state(1266);
                        recog.base.match_token(Cypher_SP, &mut recog.err_handler)?;
                    }
                }

                /*InvokeRule oC_Expression*/
                recog.base.set_state(1269);
                recog.oC_Expression()?;
            }
            Ok(())
        })();
        match result {
            Ok(_) => {}
            Err(e @ ANTLRError::FallThrough(_)) => return Err(e),
            Err(ref re) => {
                //_localctx.exception = re;
                recog.err_handler.report_error(&mut recog.base, re);
                recog.err_handler.recover(&mut recog.base, re)?;
            }
        }
        recog.base.exit_rule()?;

        Ok(_localctx)
    }
}
//------------------- oC_ListComprehension ----------------
pub type OC_ListComprehensionContextAll<'input> = OC_ListComprehensionContext<'input>;

pub type OC_ListComprehensionContext<'input> =
    BaseParserRuleContext<'input, OC_ListComprehensionContextExt<'input>>;

#[derive(Clone)]
pub struct OC_ListComprehensionContextExt<'input> {
    ph: PhantomData<&'input str>,
}

impl<'input> CypherParserContext<'input> for OC_ListComprehensionContext<'input> {}

impl<'input, 'a> Listenable<dyn CypherListener<'input> + 'a>
    for OC_ListComprehensionContext<'input>
{
    fn enter(&self, listener: &mut (dyn CypherListener<'input> + 'a)) -> Result<(), ANTLRError> {
        listener.enter_every_rule(self)?;
        listener.enter_oC_ListComprehension(self);
        Ok(())
    }
    fn exit(&self, listener: &mut (dyn CypherListener<'input> + 'a)) -> Result<(), ANTLRError> {
        listener.exit_oC_ListComprehension(self);
        listener.exit_every_rule(self)?;
        Ok(())
    }
}

impl<'input, 'a> Visitable<dyn CypherVisitor<'input> + 'a> for OC_ListComprehensionContext<'input> {
    fn accept(&self, visitor: &mut (dyn CypherVisitor<'input> + 'a)) {
        visitor.visit_oC_ListComprehension(self);
    }
}

impl<'input> CustomRuleContext<'input> for OC_ListComprehensionContextExt<'input> {
    type TF = LocalTokenFactory<'input>;
    type Ctx = CypherParserContextType;
    fn get_rule_index(&self) -> usize {
        RULE_oC_ListComprehension
    }
    //fn type_rule_index() -> usize where Self: Sized { RULE_oC_ListComprehension }
}
antlr4rust::tid! {OC_ListComprehensionContextExt<'a>}

impl<'input> OC_ListComprehensionContextExt<'input> {
    fn new(
        parent: Option<Rc<dyn CypherParserContext<'input> + 'input>>,
        invoking_state: i32,
    ) -> Rc<OC_ListComprehensionContextAll<'input>> {
        Rc::new(BaseParserRuleContext::new_parser_ctx(
            parent,
            invoking_state,
            OC_ListComprehensionContextExt { ph: PhantomData },
        ))
    }
}

pub trait OC_ListComprehensionContextAttrs<'input>:
    CypherParserContext<'input> + BorrowMut<OC_ListComprehensionContextExt<'input>>
{
    fn oC_FilterExpression(&self) -> Option<Rc<OC_FilterExpressionContextAll<'input>>>
    where
        Self: Sized,
    {
        self.child_of_type(0)
    }
    /// Retrieves all `TerminalNode`s corresponding to token SP in current rule
    fn SP_all(&self) -> Vec<Rc<TerminalNode<'input, CypherParserContextType>>>
    where
        Self: Sized,
    {
        self.children_of_type()
    }
    /// Retrieves 'i's TerminalNode corresponding to token SP, starting from 0.
    /// Returns `None` if number of children corresponding to token SP is less or equal than `i`.
    fn SP(&self, i: usize) -> Option<Rc<TerminalNode<'input, CypherParserContextType>>>
    where
        Self: Sized,
    {
        self.get_token(Cypher_SP, i)
    }
    fn oC_Expression(&self) -> Option<Rc<OC_ExpressionContextAll<'input>>>
    where
        Self: Sized,
    {
        self.child_of_type(0)
    }
}

impl<'input> OC_ListComprehensionContextAttrs<'input> for OC_ListComprehensionContext<'input> {}

impl<'input, I> CypherParser<'input, I>
where
    I: TokenStream<'input, TF = LocalTokenFactory<'input>> + TidAble<'input>,
{
    pub fn oC_ListComprehension(
        &mut self,
    ) -> Result<Rc<OC_ListComprehensionContextAll<'input>>, ANTLRError> {
        let mut recog = self;
        let _parentctx = recog.ctx.take();
        let mut _localctx =
            OC_ListComprehensionContextExt::new(_parentctx.clone(), recog.base.get_state());
        recog
            .base
            .enter_rule(_localctx.clone(), 146, RULE_oC_ListComprehension);
        let mut _localctx: Rc<OC_ListComprehensionContextAll> = _localctx;
        let mut _la: i32 = -1;
        let result: Result<(), ANTLRError> = (|| {
            //recog.base.enter_outer_alt(_localctx.clone(), 1)?;
            recog.base.enter_outer_alt(None, 1)?;
            {
                recog.base.set_state(1271);
                recog
                    .base
                    .match_token(Cypher_T__7, &mut recog.err_handler)?;

                recog.base.set_state(1273);
                recog.err_handler.sync(&mut recog.base)?;
                _la = recog.base.input.la(1);
                if _la == Cypher_SP {
                    {
                        recog.base.set_state(1272);
                        recog.base.match_token(Cypher_SP, &mut recog.err_handler)?;
                    }
                }

                /*InvokeRule oC_FilterExpression*/
                recog.base.set_state(1275);
                recog.oC_FilterExpression()?;

                recog.base.set_state(1284);
                recog.err_handler.sync(&mut recog.base)?;
                match recog.interpreter.adaptive_predict(229, &mut recog.base)? {
                    x if x == 1 => {
                        {
                            recog.base.set_state(1277);
                            recog.err_handler.sync(&mut recog.base)?;
                            _la = recog.base.input.la(1);
                            if _la == Cypher_SP {
                                {
                                    recog.base.set_state(1276);
                                    recog.base.match_token(Cypher_SP, &mut recog.err_handler)?;
                                }
                            }

                            recog.base.set_state(1279);
                            recog
                                .base
                                .match_token(Cypher_T__10, &mut recog.err_handler)?;

                            recog.base.set_state(1281);
                            recog.err_handler.sync(&mut recog.base)?;
                            _la = recog.base.input.la(1);
                            if _la == Cypher_SP {
                                {
                                    recog.base.set_state(1280);
                                    recog.base.match_token(Cypher_SP, &mut recog.err_handler)?;
                                }
                            }

                            /*InvokeRule oC_Expression*/
                            recog.base.set_state(1283);
                            recog.oC_Expression()?;
                        }
                    }

                    _ => {}
                }
                recog.base.set_state(1287);
                recog.err_handler.sync(&mut recog.base)?;
                _la = recog.base.input.la(1);
                if _la == Cypher_SP {
                    {
                        recog.base.set_state(1286);
                        recog.base.match_token(Cypher_SP, &mut recog.err_handler)?;
                    }
                }

                recog.base.set_state(1289);
                recog
                    .base
                    .match_token(Cypher_T__8, &mut recog.err_handler)?;
            }
            Ok(())
        })();
        match result {
            Ok(_) => {}
            Err(e @ ANTLRError::FallThrough(_)) => return Err(e),
            Err(ref re) => {
                //_localctx.exception = re;
                recog.err_handler.report_error(&mut recog.base, re);
                recog.err_handler.recover(&mut recog.base, re)?;
            }
        }
        recog.base.exit_rule()?;

        Ok(_localctx)
    }
}
//------------------- oC_PatternComprehension ----------------
pub type OC_PatternComprehensionContextAll<'input> = OC_PatternComprehensionContext<'input>;

pub type OC_PatternComprehensionContext<'input> =
    BaseParserRuleContext<'input, OC_PatternComprehensionContextExt<'input>>;

#[derive(Clone)]
pub struct OC_PatternComprehensionContextExt<'input> {
    ph: PhantomData<&'input str>,
}

impl<'input> CypherParserContext<'input> for OC_PatternComprehensionContext<'input> {}

impl<'input, 'a> Listenable<dyn CypherListener<'input> + 'a>
    for OC_PatternComprehensionContext<'input>
{
    fn enter(&self, listener: &mut (dyn CypherListener<'input> + 'a)) -> Result<(), ANTLRError> {
        listener.enter_every_rule(self)?;
        listener.enter_oC_PatternComprehension(self);
        Ok(())
    }
    fn exit(&self, listener: &mut (dyn CypherListener<'input> + 'a)) -> Result<(), ANTLRError> {
        listener.exit_oC_PatternComprehension(self);
        listener.exit_every_rule(self)?;
        Ok(())
    }
}

impl<'input, 'a> Visitable<dyn CypherVisitor<'input> + 'a>
    for OC_PatternComprehensionContext<'input>
{
    fn accept(&self, visitor: &mut (dyn CypherVisitor<'input> + 'a)) {
        visitor.visit_oC_PatternComprehension(self);
    }
}

impl<'input> CustomRuleContext<'input> for OC_PatternComprehensionContextExt<'input> {
    type TF = LocalTokenFactory<'input>;
    type Ctx = CypherParserContextType;
    fn get_rule_index(&self) -> usize {
        RULE_oC_PatternComprehension
    }
    //fn type_rule_index() -> usize where Self: Sized { RULE_oC_PatternComprehension }
}
antlr4rust::tid! {OC_PatternComprehensionContextExt<'a>}

impl<'input> OC_PatternComprehensionContextExt<'input> {
    fn new(
        parent: Option<Rc<dyn CypherParserContext<'input> + 'input>>,
        invoking_state: i32,
    ) -> Rc<OC_PatternComprehensionContextAll<'input>> {
        Rc::new(BaseParserRuleContext::new_parser_ctx(
            parent,
            invoking_state,
            OC_PatternComprehensionContextExt { ph: PhantomData },
        ))
    }
}

pub trait OC_PatternComprehensionContextAttrs<'input>:
    CypherParserContext<'input> + BorrowMut<OC_PatternComprehensionContextExt<'input>>
{
    fn oC_RelationshipsPattern(&self) -> Option<Rc<OC_RelationshipsPatternContextAll<'input>>>
    where
        Self: Sized,
    {
        self.child_of_type(0)
    }
    fn oC_Expression(&self) -> Option<Rc<OC_ExpressionContextAll<'input>>>
    where
        Self: Sized,
    {
        self.child_of_type(0)
    }
    /// Retrieves all `TerminalNode`s corresponding to token SP in current rule
    fn SP_all(&self) -> Vec<Rc<TerminalNode<'input, CypherParserContextType>>>
    where
        Self: Sized,
    {
        self.children_of_type()
    }
    /// Retrieves 'i's TerminalNode corresponding to token SP, starting from 0.
    /// Returns `None` if number of children corresponding to token SP is less or equal than `i`.
    fn SP(&self, i: usize) -> Option<Rc<TerminalNode<'input, CypherParserContextType>>>
    where
        Self: Sized,
    {
        self.get_token(Cypher_SP, i)
    }
    fn oC_Variable(&self) -> Option<Rc<OC_VariableContextAll<'input>>>
    where
        Self: Sized,
    {
        self.child_of_type(0)
    }
    fn oC_Where(&self) -> Option<Rc<OC_WhereContextAll<'input>>>
    where
        Self: Sized,
    {
        self.child_of_type(0)
    }
}

impl<'input> OC_PatternComprehensionContextAttrs<'input>
    for OC_PatternComprehensionContext<'input>
{
}

impl<'input, I> CypherParser<'input, I>
where
    I: TokenStream<'input, TF = LocalTokenFactory<'input>> + TidAble<'input>,
{
    pub fn oC_PatternComprehension(
        &mut self,
    ) -> Result<Rc<OC_PatternComprehensionContextAll<'input>>, ANTLRError> {
        let mut recog = self;
        let _parentctx = recog.ctx.take();
        let mut _localctx =
            OC_PatternComprehensionContextExt::new(_parentctx.clone(), recog.base.get_state());
        recog
            .base
            .enter_rule(_localctx.clone(), 148, RULE_oC_PatternComprehension);
        let mut _localctx: Rc<OC_PatternComprehensionContextAll> = _localctx;
        let mut _la: i32 = -1;
        let result: Result<(), ANTLRError> = (|| {
            //recog.base.enter_outer_alt(_localctx.clone(), 1)?;
            recog.base.enter_outer_alt(None, 1)?;
            {
                recog.base.set_state(1291);
                recog
                    .base
                    .match_token(Cypher_T__7, &mut recog.err_handler)?;

                recog.base.set_state(1293);
                recog.err_handler.sync(&mut recog.base)?;
                _la = recog.base.input.la(1);
                if _la == Cypher_SP {
                    {
                        recog.base.set_state(1292);
                        recog.base.match_token(Cypher_SP, &mut recog.err_handler)?;
                    }
                }

                recog.base.set_state(1303);
                recog.err_handler.sync(&mut recog.base)?;
                _la = recog.base.input.la(1);
                if (((_la - 83) & !0x3f) == 0 && ((1usize << (_la - 83)) & 65985) != 0)
                    || (((_la - 120) & !0x3f) == 0 && ((1usize << (_la - 120)) & 39) != 0)
                {
                    {
                        /*InvokeRule oC_Variable*/
                        recog.base.set_state(1295);
                        recog.oC_Variable()?;

                        recog.base.set_state(1297);
                        recog.err_handler.sync(&mut recog.base)?;
                        _la = recog.base.input.la(1);
                        if _la == Cypher_SP {
                            {
                                recog.base.set_state(1296);
                                recog.base.match_token(Cypher_SP, &mut recog.err_handler)?;
                            }
                        }

                        recog.base.set_state(1299);
                        recog
                            .base
                            .match_token(Cypher_T__2, &mut recog.err_handler)?;

                        recog.base.set_state(1301);
                        recog.err_handler.sync(&mut recog.base)?;
                        _la = recog.base.input.la(1);
                        if _la == Cypher_SP {
                            {
                                recog.base.set_state(1300);
                                recog.base.match_token(Cypher_SP, &mut recog.err_handler)?;
                            }
                        }
                    }
                }

                /*InvokeRule oC_RelationshipsPattern*/
                recog.base.set_state(1305);
                recog.oC_RelationshipsPattern()?;

                recog.base.set_state(1307);
                recog.err_handler.sync(&mut recog.base)?;
                _la = recog.base.input.la(1);
                if _la == Cypher_SP {
                    {
                        recog.base.set_state(1306);
                        recog.base.match_token(Cypher_SP, &mut recog.err_handler)?;
                    }
                }

                recog.base.set_state(1313);
                recog.err_handler.sync(&mut recog.base)?;
                _la = recog.base.input.la(1);
                if _la == Cypher_WHERE {
                    {
                        /*InvokeRule oC_Where*/
                        recog.base.set_state(1309);
                        recog.oC_Where()?;

                        recog.base.set_state(1311);
                        recog.err_handler.sync(&mut recog.base)?;
                        _la = recog.base.input.la(1);
                        if _la == Cypher_SP {
                            {
                                recog.base.set_state(1310);
                                recog.base.match_token(Cypher_SP, &mut recog.err_handler)?;
                            }
                        }
                    }
                }

                recog.base.set_state(1315);
                recog
                    .base
                    .match_token(Cypher_T__10, &mut recog.err_handler)?;

                recog.base.set_state(1317);
                recog.err_handler.sync(&mut recog.base)?;
                _la = recog.base.input.la(1);
                if _la == Cypher_SP {
                    {
                        recog.base.set_state(1316);
                        recog.base.match_token(Cypher_SP, &mut recog.err_handler)?;
                    }
                }

                /*InvokeRule oC_Expression*/
                recog.base.set_state(1319);
                recog.oC_Expression()?;

                recog.base.set_state(1321);
                recog.err_handler.sync(&mut recog.base)?;
                _la = recog.base.input.la(1);
                if _la == Cypher_SP {
                    {
                        recog.base.set_state(1320);
                        recog.base.match_token(Cypher_SP, &mut recog.err_handler)?;
                    }
                }

                recog.base.set_state(1323);
                recog
                    .base
                    .match_token(Cypher_T__8, &mut recog.err_handler)?;
            }
            Ok(())
        })();
        match result {
            Ok(_) => {}
            Err(e @ ANTLRError::FallThrough(_)) => return Err(e),
            Err(ref re) => {
                //_localctx.exception = re;
                recog.err_handler.report_error(&mut recog.base, re);
                recog.err_handler.recover(&mut recog.base, re)?;
            }
        }
        recog.base.exit_rule()?;

        Ok(_localctx)
    }
}
//------------------- oC_Quantifier ----------------
pub type OC_QuantifierContextAll<'input> = OC_QuantifierContext<'input>;

pub type OC_QuantifierContext<'input> =
    BaseParserRuleContext<'input, OC_QuantifierContextExt<'input>>;

#[derive(Clone)]
pub struct OC_QuantifierContextExt<'input> {
    ph: PhantomData<&'input str>,
}

impl<'input> CypherParserContext<'input> for OC_QuantifierContext<'input> {}

impl<'input, 'a> Listenable<dyn CypherListener<'input> + 'a> for OC_QuantifierContext<'input> {
    fn enter(&self, listener: &mut (dyn CypherListener<'input> + 'a)) -> Result<(), ANTLRError> {
        listener.enter_every_rule(self)?;
        listener.enter_oC_Quantifier(self);
        Ok(())
    }
    fn exit(&self, listener: &mut (dyn CypherListener<'input> + 'a)) -> Result<(), ANTLRError> {
        listener.exit_oC_Quantifier(self);
        listener.exit_every_rule(self)?;
        Ok(())
    }
}

impl<'input, 'a> Visitable<dyn CypherVisitor<'input> + 'a> for OC_QuantifierContext<'input> {
    fn accept(&self, visitor: &mut (dyn CypherVisitor<'input> + 'a)) {
        visitor.visit_oC_Quantifier(self);
    }
}

impl<'input> CustomRuleContext<'input> for OC_QuantifierContextExt<'input> {
    type TF = LocalTokenFactory<'input>;
    type Ctx = CypherParserContextType;
    fn get_rule_index(&self) -> usize {
        RULE_oC_Quantifier
    }
    //fn type_rule_index() -> usize where Self: Sized { RULE_oC_Quantifier }
}
antlr4rust::tid! {OC_QuantifierContextExt<'a>}

impl<'input> OC_QuantifierContextExt<'input> {
    fn new(
        parent: Option<Rc<dyn CypherParserContext<'input> + 'input>>,
        invoking_state: i32,
    ) -> Rc<OC_QuantifierContextAll<'input>> {
        Rc::new(BaseParserRuleContext::new_parser_ctx(
            parent,
            invoking_state,
            OC_QuantifierContextExt { ph: PhantomData },
        ))
    }
}

pub trait OC_QuantifierContextAttrs<'input>:
    CypherParserContext<'input> + BorrowMut<OC_QuantifierContextExt<'input>>
{
    /// Retrieves first TerminalNode corresponding to token ALL
    /// Returns `None` if there is no child corresponding to token ALL
    fn ALL(&self) -> Option<Rc<TerminalNode<'input, CypherParserContextType>>>
    where
        Self: Sized,
    {
        self.get_token(Cypher_ALL, 0)
    }
    fn oC_FilterExpression(&self) -> Option<Rc<OC_FilterExpressionContextAll<'input>>>
    where
        Self: Sized,
    {
        self.child_of_type(0)
    }
    /// Retrieves all `TerminalNode`s corresponding to token SP in current rule
    fn SP_all(&self) -> Vec<Rc<TerminalNode<'input, CypherParserContextType>>>
    where
        Self: Sized,
    {
        self.children_of_type()
    }
    /// Retrieves 'i's TerminalNode corresponding to token SP, starting from 0.
    /// Returns `None` if number of children corresponding to token SP is less or equal than `i`.
    fn SP(&self, i: usize) -> Option<Rc<TerminalNode<'input, CypherParserContextType>>>
    where
        Self: Sized,
    {
        self.get_token(Cypher_SP, i)
    }
    /// Retrieves first TerminalNode corresponding to token ANY
    /// Returns `None` if there is no child corresponding to token ANY
    fn ANY(&self) -> Option<Rc<TerminalNode<'input, CypherParserContextType>>>
    where
        Self: Sized,
    {
        self.get_token(Cypher_ANY, 0)
    }
    /// Retrieves first TerminalNode corresponding to token NONE
    /// Returns `None` if there is no child corresponding to token NONE
    fn NONE(&self) -> Option<Rc<TerminalNode<'input, CypherParserContextType>>>
    where
        Self: Sized,
    {
        self.get_token(Cypher_NONE, 0)
    }
    /// Retrieves first TerminalNode corresponding to token SINGLE
    /// Returns `None` if there is no child corresponding to token SINGLE
    fn SINGLE(&self) -> Option<Rc<TerminalNode<'input, CypherParserContextType>>>
    where
        Self: Sized,
    {
        self.get_token(Cypher_SINGLE, 0)
    }
}

impl<'input> OC_QuantifierContextAttrs<'input> for OC_QuantifierContext<'input> {}

impl<'input, I> CypherParser<'input, I>
where
    I: TokenStream<'input, TF = LocalTokenFactory<'input>> + TidAble<'input>,
{
    pub fn oC_Quantifier(&mut self) -> Result<Rc<OC_QuantifierContextAll<'input>>, ANTLRError> {
        let mut recog = self;
        let _parentctx = recog.ctx.take();
        let mut _localctx =
            OC_QuantifierContextExt::new(_parentctx.clone(), recog.base.get_state());
        recog
            .base
            .enter_rule(_localctx.clone(), 150, RULE_oC_Quantifier);
        let mut _localctx: Rc<OC_QuantifierContextAll> = _localctx;
        let mut _la: i32 = -1;
        let result: Result<(), ANTLRError> = (|| {
            recog.base.set_state(1381);
            recog.err_handler.sync(&mut recog.base)?;
            match recog.base.input.la(1) {
                Cypher_ALL => {
                    //recog.base.enter_outer_alt(_localctx.clone(), 1)?;
                    recog.base.enter_outer_alt(None, 1)?;
                    {
                        {
                            recog.base.set_state(1325);
                            recog.base.match_token(Cypher_ALL, &mut recog.err_handler)?;

                            recog.base.set_state(1327);
                            recog.err_handler.sync(&mut recog.base)?;
                            _la = recog.base.input.la(1);
                            if _la == Cypher_SP {
                                {
                                    recog.base.set_state(1326);
                                    recog.base.match_token(Cypher_SP, &mut recog.err_handler)?;
                                }
                            }

                            recog.base.set_state(1329);
                            recog
                                .base
                                .match_token(Cypher_T__5, &mut recog.err_handler)?;

                            recog.base.set_state(1331);
                            recog.err_handler.sync(&mut recog.base)?;
                            _la = recog.base.input.la(1);
                            if _la == Cypher_SP {
                                {
                                    recog.base.set_state(1330);
                                    recog.base.match_token(Cypher_SP, &mut recog.err_handler)?;
                                }
                            }

                            /*InvokeRule oC_FilterExpression*/
                            recog.base.set_state(1333);
                            recog.oC_FilterExpression()?;

                            recog.base.set_state(1335);
                            recog.err_handler.sync(&mut recog.base)?;
                            _la = recog.base.input.la(1);
                            if _la == Cypher_SP {
                                {
                                    recog.base.set_state(1334);
                                    recog.base.match_token(Cypher_SP, &mut recog.err_handler)?;
                                }
                            }

                            recog.base.set_state(1337);
                            recog
                                .base
                                .match_token(Cypher_T__6, &mut recog.err_handler)?;
                        }
                    }
                }

                Cypher_ANY => {
                    //recog.base.enter_outer_alt(_localctx.clone(), 2)?;
                    recog.base.enter_outer_alt(None, 2)?;
                    {
                        {
                            recog.base.set_state(1339);
                            recog.base.match_token(Cypher_ANY, &mut recog.err_handler)?;

                            recog.base.set_state(1341);
                            recog.err_handler.sync(&mut recog.base)?;
                            _la = recog.base.input.la(1);
                            if _la == Cypher_SP {
                                {
                                    recog.base.set_state(1340);
                                    recog.base.match_token(Cypher_SP, &mut recog.err_handler)?;
                                }
                            }

                            recog.base.set_state(1343);
                            recog
                                .base
                                .match_token(Cypher_T__5, &mut recog.err_handler)?;

                            recog.base.set_state(1345);
                            recog.err_handler.sync(&mut recog.base)?;
                            _la = recog.base.input.la(1);
                            if _la == Cypher_SP {
                                {
                                    recog.base.set_state(1344);
                                    recog.base.match_token(Cypher_SP, &mut recog.err_handler)?;
                                }
                            }

                            /*InvokeRule oC_FilterExpression*/
                            recog.base.set_state(1347);
                            recog.oC_FilterExpression()?;

                            recog.base.set_state(1349);
                            recog.err_handler.sync(&mut recog.base)?;
                            _la = recog.base.input.la(1);
                            if _la == Cypher_SP {
                                {
                                    recog.base.set_state(1348);
                                    recog.base.match_token(Cypher_SP, &mut recog.err_handler)?;
                                }
                            }

                            recog.base.set_state(1351);
                            recog
                                .base
                                .match_token(Cypher_T__6, &mut recog.err_handler)?;
                        }
                    }
                }

                Cypher_NONE => {
                    //recog.base.enter_outer_alt(_localctx.clone(), 3)?;
                    recog.base.enter_outer_alt(None, 3)?;
                    {
                        {
                            recog.base.set_state(1353);
                            recog
                                .base
                                .match_token(Cypher_NONE, &mut recog.err_handler)?;

                            recog.base.set_state(1355);
                            recog.err_handler.sync(&mut recog.base)?;
                            _la = recog.base.input.la(1);
                            if _la == Cypher_SP {
                                {
                                    recog.base.set_state(1354);
                                    recog.base.match_token(Cypher_SP, &mut recog.err_handler)?;
                                }
                            }

                            recog.base.set_state(1357);
                            recog
                                .base
                                .match_token(Cypher_T__5, &mut recog.err_handler)?;

                            recog.base.set_state(1359);
                            recog.err_handler.sync(&mut recog.base)?;
                            _la = recog.base.input.la(1);
                            if _la == Cypher_SP {
                                {
                                    recog.base.set_state(1358);
                                    recog.base.match_token(Cypher_SP, &mut recog.err_handler)?;
                                }
                            }

                            /*InvokeRule oC_FilterExpression*/
                            recog.base.set_state(1361);
                            recog.oC_FilterExpression()?;

                            recog.base.set_state(1363);
                            recog.err_handler.sync(&mut recog.base)?;
                            _la = recog.base.input.la(1);
                            if _la == Cypher_SP {
                                {
                                    recog.base.set_state(1362);
                                    recog.base.match_token(Cypher_SP, &mut recog.err_handler)?;
                                }
                            }

                            recog.base.set_state(1365);
                            recog
                                .base
                                .match_token(Cypher_T__6, &mut recog.err_handler)?;
                        }
                    }
                }

                Cypher_SINGLE => {
                    //recog.base.enter_outer_alt(_localctx.clone(), 4)?;
                    recog.base.enter_outer_alt(None, 4)?;
                    {
                        {
                            recog.base.set_state(1367);
                            recog
                                .base
                                .match_token(Cypher_SINGLE, &mut recog.err_handler)?;

                            recog.base.set_state(1369);
                            recog.err_handler.sync(&mut recog.base)?;
                            _la = recog.base.input.la(1);
                            if _la == Cypher_SP {
                                {
                                    recog.base.set_state(1368);
                                    recog.base.match_token(Cypher_SP, &mut recog.err_handler)?;
                                }
                            }

                            recog.base.set_state(1371);
                            recog
                                .base
                                .match_token(Cypher_T__5, &mut recog.err_handler)?;

                            recog.base.set_state(1373);
                            recog.err_handler.sync(&mut recog.base)?;
                            _la = recog.base.input.la(1);
                            if _la == Cypher_SP {
                                {
                                    recog.base.set_state(1372);
                                    recog.base.match_token(Cypher_SP, &mut recog.err_handler)?;
                                }
                            }

                            /*InvokeRule oC_FilterExpression*/
                            recog.base.set_state(1375);
                            recog.oC_FilterExpression()?;

                            recog.base.set_state(1377);
                            recog.err_handler.sync(&mut recog.base)?;
                            _la = recog.base.input.la(1);
                            if _la == Cypher_SP {
                                {
                                    recog.base.set_state(1376);
                                    recog.base.match_token(Cypher_SP, &mut recog.err_handler)?;
                                }
                            }

                            recog.base.set_state(1379);
                            recog
                                .base
                                .match_token(Cypher_T__6, &mut recog.err_handler)?;
                        }
                    }
                }

                _ => Err(ANTLRError::NoAltError(NoViableAltError::new(
                    &mut recog.base,
                )))?,
            }
            Ok(())
        })();
        match result {
            Ok(_) => {}
            Err(e @ ANTLRError::FallThrough(_)) => return Err(e),
            Err(ref re) => {
                //_localctx.exception = re;
                recog.err_handler.report_error(&mut recog.base, re);
                recog.err_handler.recover(&mut recog.base, re)?;
            }
        }
        recog.base.exit_rule()?;

        Ok(_localctx)
    }
}
//------------------- oC_FilterExpression ----------------
pub type OC_FilterExpressionContextAll<'input> = OC_FilterExpressionContext<'input>;

pub type OC_FilterExpressionContext<'input> =
    BaseParserRuleContext<'input, OC_FilterExpressionContextExt<'input>>;

#[derive(Clone)]
pub struct OC_FilterExpressionContextExt<'input> {
    ph: PhantomData<&'input str>,
}

impl<'input> CypherParserContext<'input> for OC_FilterExpressionContext<'input> {}

impl<'input, 'a> Listenable<dyn CypherListener<'input> + 'a>
    for OC_FilterExpressionContext<'input>
{
    fn enter(&self, listener: &mut (dyn CypherListener<'input> + 'a)) -> Result<(), ANTLRError> {
        listener.enter_every_rule(self)?;
        listener.enter_oC_FilterExpression(self);
        Ok(())
    }
    fn exit(&self, listener: &mut (dyn CypherListener<'input> + 'a)) -> Result<(), ANTLRError> {
        listener.exit_oC_FilterExpression(self);
        listener.exit_every_rule(self)?;
        Ok(())
    }
}

impl<'input, 'a> Visitable<dyn CypherVisitor<'input> + 'a> for OC_FilterExpressionContext<'input> {
    fn accept(&self, visitor: &mut (dyn CypherVisitor<'input> + 'a)) {
        visitor.visit_oC_FilterExpression(self);
    }
}

impl<'input> CustomRuleContext<'input> for OC_FilterExpressionContextExt<'input> {
    type TF = LocalTokenFactory<'input>;
    type Ctx = CypherParserContextType;
    fn get_rule_index(&self) -> usize {
        RULE_oC_FilterExpression
    }
    //fn type_rule_index() -> usize where Self: Sized { RULE_oC_FilterExpression }
}
antlr4rust::tid! {OC_FilterExpressionContextExt<'a>}

impl<'input> OC_FilterExpressionContextExt<'input> {
    fn new(
        parent: Option<Rc<dyn CypherParserContext<'input> + 'input>>,
        invoking_state: i32,
    ) -> Rc<OC_FilterExpressionContextAll<'input>> {
        Rc::new(BaseParserRuleContext::new_parser_ctx(
            parent,
            invoking_state,
            OC_FilterExpressionContextExt { ph: PhantomData },
        ))
    }
}

pub trait OC_FilterExpressionContextAttrs<'input>:
    CypherParserContext<'input> + BorrowMut<OC_FilterExpressionContextExt<'input>>
{
    fn oC_IdInColl(&self) -> Option<Rc<OC_IdInCollContextAll<'input>>>
    where
        Self: Sized,
    {
        self.child_of_type(0)
    }
    fn oC_Where(&self) -> Option<Rc<OC_WhereContextAll<'input>>>
    where
        Self: Sized,
    {
        self.child_of_type(0)
    }
    /// Retrieves first TerminalNode corresponding to token SP
    /// Returns `None` if there is no child corresponding to token SP
    fn SP(&self) -> Option<Rc<TerminalNode<'input, CypherParserContextType>>>
    where
        Self: Sized,
    {
        self.get_token(Cypher_SP, 0)
    }
}

impl<'input> OC_FilterExpressionContextAttrs<'input> for OC_FilterExpressionContext<'input> {}

impl<'input, I> CypherParser<'input, I>
where
    I: TokenStream<'input, TF = LocalTokenFactory<'input>> + TidAble<'input>,
{
    pub fn oC_FilterExpression(
        &mut self,
    ) -> Result<Rc<OC_FilterExpressionContextAll<'input>>, ANTLRError> {
        let mut recog = self;
        let _parentctx = recog.ctx.take();
        let mut _localctx =
            OC_FilterExpressionContextExt::new(_parentctx.clone(), recog.base.get_state());
        recog
            .base
            .enter_rule(_localctx.clone(), 152, RULE_oC_FilterExpression);
        let mut _localctx: Rc<OC_FilterExpressionContextAll> = _localctx;
        let mut _la: i32 = -1;
        let result: Result<(), ANTLRError> = (|| {
            //recog.base.enter_outer_alt(_localctx.clone(), 1)?;
            recog.base.enter_outer_alt(None, 1)?;
            {
                /*InvokeRule oC_IdInColl*/
                recog.base.set_state(1383);
                recog.oC_IdInColl()?;

                recog.base.set_state(1388);
                recog.err_handler.sync(&mut recog.base)?;
                match recog.interpreter.adaptive_predict(254, &mut recog.base)? {
                    x if x == 1 => {
                        {
                            recog.base.set_state(1385);
                            recog.err_handler.sync(&mut recog.base)?;
                            _la = recog.base.input.la(1);
                            if _la == Cypher_SP {
                                {
                                    recog.base.set_state(1384);
                                    recog.base.match_token(Cypher_SP, &mut recog.err_handler)?;
                                }
                            }

                            /*InvokeRule oC_Where*/
                            recog.base.set_state(1387);
                            recog.oC_Where()?;
                        }
                    }

                    _ => {}
                }
            }
            Ok(())
        })();
        match result {
            Ok(_) => {}
            Err(e @ ANTLRError::FallThrough(_)) => return Err(e),
            Err(ref re) => {
                //_localctx.exception = re;
                recog.err_handler.report_error(&mut recog.base, re);
                recog.err_handler.recover(&mut recog.base, re)?;
            }
        }
        recog.base.exit_rule()?;

        Ok(_localctx)
    }
}
//------------------- oC_PatternPredicate ----------------
pub type OC_PatternPredicateContextAll<'input> = OC_PatternPredicateContext<'input>;

pub type OC_PatternPredicateContext<'input> =
    BaseParserRuleContext<'input, OC_PatternPredicateContextExt<'input>>;

#[derive(Clone)]
pub struct OC_PatternPredicateContextExt<'input> {
    ph: PhantomData<&'input str>,
}

impl<'input> CypherParserContext<'input> for OC_PatternPredicateContext<'input> {}

impl<'input, 'a> Listenable<dyn CypherListener<'input> + 'a>
    for OC_PatternPredicateContext<'input>
{
    fn enter(&self, listener: &mut (dyn CypherListener<'input> + 'a)) -> Result<(), ANTLRError> {
        listener.enter_every_rule(self)?;
        listener.enter_oC_PatternPredicate(self);
        Ok(())
    }
    fn exit(&self, listener: &mut (dyn CypherListener<'input> + 'a)) -> Result<(), ANTLRError> {
        listener.exit_oC_PatternPredicate(self);
        listener.exit_every_rule(self)?;
        Ok(())
    }
}

impl<'input, 'a> Visitable<dyn CypherVisitor<'input> + 'a> for OC_PatternPredicateContext<'input> {
    fn accept(&self, visitor: &mut (dyn CypherVisitor<'input> + 'a)) {
        visitor.visit_oC_PatternPredicate(self);
    }
}

impl<'input> CustomRuleContext<'input> for OC_PatternPredicateContextExt<'input> {
    type TF = LocalTokenFactory<'input>;
    type Ctx = CypherParserContextType;
    fn get_rule_index(&self) -> usize {
        RULE_oC_PatternPredicate
    }
    //fn type_rule_index() -> usize where Self: Sized { RULE_oC_PatternPredicate }
}
antlr4rust::tid! {OC_PatternPredicateContextExt<'a>}

impl<'input> OC_PatternPredicateContextExt<'input> {
    fn new(
        parent: Option<Rc<dyn CypherParserContext<'input> + 'input>>,
        invoking_state: i32,
    ) -> Rc<OC_PatternPredicateContextAll<'input>> {
        Rc::new(BaseParserRuleContext::new_parser_ctx(
            parent,
            invoking_state,
            OC_PatternPredicateContextExt { ph: PhantomData },
        ))
    }
}

pub trait OC_PatternPredicateContextAttrs<'input>:
    CypherParserContext<'input> + BorrowMut<OC_PatternPredicateContextExt<'input>>
{
    fn oC_RelationshipsPattern(&self) -> Option<Rc<OC_RelationshipsPatternContextAll<'input>>>
    where
        Self: Sized,
    {
        self.child_of_type(0)
    }
}

impl<'input> OC_PatternPredicateContextAttrs<'input> for OC_PatternPredicateContext<'input> {}

impl<'input, I> CypherParser<'input, I>
where
    I: TokenStream<'input, TF = LocalTokenFactory<'input>> + TidAble<'input>,
{
    pub fn oC_PatternPredicate(
        &mut self,
    ) -> Result<Rc<OC_PatternPredicateContextAll<'input>>, ANTLRError> {
        let mut recog = self;
        let _parentctx = recog.ctx.take();
        let mut _localctx =
            OC_PatternPredicateContextExt::new(_parentctx.clone(), recog.base.get_state());
        recog
            .base
            .enter_rule(_localctx.clone(), 154, RULE_oC_PatternPredicate);
        let mut _localctx: Rc<OC_PatternPredicateContextAll> = _localctx;
        let result: Result<(), ANTLRError> = (|| {
            //recog.base.enter_outer_alt(_localctx.clone(), 1)?;
            recog.base.enter_outer_alt(None, 1)?;
            {
                /*InvokeRule oC_RelationshipsPattern*/
                recog.base.set_state(1390);
                recog.oC_RelationshipsPattern()?;
            }
            Ok(())
        })();
        match result {
            Ok(_) => {}
            Err(e @ ANTLRError::FallThrough(_)) => return Err(e),
            Err(ref re) => {
                //_localctx.exception = re;
                recog.err_handler.report_error(&mut recog.base, re);
                recog.err_handler.recover(&mut recog.base, re)?;
            }
        }
        recog.base.exit_rule()?;

        Ok(_localctx)
    }
}
//------------------- oC_ParenthesizedExpression ----------------
pub type OC_ParenthesizedExpressionContextAll<'input> = OC_ParenthesizedExpressionContext<'input>;

pub type OC_ParenthesizedExpressionContext<'input> =
    BaseParserRuleContext<'input, OC_ParenthesizedExpressionContextExt<'input>>;

#[derive(Clone)]
pub struct OC_ParenthesizedExpressionContextExt<'input> {
    ph: PhantomData<&'input str>,
}

impl<'input> CypherParserContext<'input> for OC_ParenthesizedExpressionContext<'input> {}

impl<'input, 'a> Listenable<dyn CypherListener<'input> + 'a>
    for OC_ParenthesizedExpressionContext<'input>
{
    fn enter(&self, listener: &mut (dyn CypherListener<'input> + 'a)) -> Result<(), ANTLRError> {
        listener.enter_every_rule(self)?;
        listener.enter_oC_ParenthesizedExpression(self);
        Ok(())
    }
    fn exit(&self, listener: &mut (dyn CypherListener<'input> + 'a)) -> Result<(), ANTLRError> {
        listener.exit_oC_ParenthesizedExpression(self);
        listener.exit_every_rule(self)?;
        Ok(())
    }
}

impl<'input, 'a> Visitable<dyn CypherVisitor<'input> + 'a>
    for OC_ParenthesizedExpressionContext<'input>
{
    fn accept(&self, visitor: &mut (dyn CypherVisitor<'input> + 'a)) {
        visitor.visit_oC_ParenthesizedExpression(self);
    }
}

impl<'input> CustomRuleContext<'input> for OC_ParenthesizedExpressionContextExt<'input> {
    type TF = LocalTokenFactory<'input>;
    type Ctx = CypherParserContextType;
    fn get_rule_index(&self) -> usize {
        RULE_oC_ParenthesizedExpression
    }
    //fn type_rule_index() -> usize where Self: Sized { RULE_oC_ParenthesizedExpression }
}
antlr4rust::tid! {OC_ParenthesizedExpressionContextExt<'a>}

impl<'input> OC_ParenthesizedExpressionContextExt<'input> {
    fn new(
        parent: Option<Rc<dyn CypherParserContext<'input> + 'input>>,
        invoking_state: i32,
    ) -> Rc<OC_ParenthesizedExpressionContextAll<'input>> {
        Rc::new(BaseParserRuleContext::new_parser_ctx(
            parent,
            invoking_state,
            OC_ParenthesizedExpressionContextExt { ph: PhantomData },
        ))
    }
}

pub trait OC_ParenthesizedExpressionContextAttrs<'input>:
    CypherParserContext<'input> + BorrowMut<OC_ParenthesizedExpressionContextExt<'input>>
{
    fn oC_Expression(&self) -> Option<Rc<OC_ExpressionContextAll<'input>>>
    where
        Self: Sized,
    {
        self.child_of_type(0)
    }
    /// Retrieves all `TerminalNode`s corresponding to token SP in current rule
    fn SP_all(&self) -> Vec<Rc<TerminalNode<'input, CypherParserContextType>>>
    where
        Self: Sized,
    {
        self.children_of_type()
    }
    /// Retrieves 'i's TerminalNode corresponding to token SP, starting from 0.
    /// Returns `None` if number of children corresponding to token SP is less or equal than `i`.
    fn SP(&self, i: usize) -> Option<Rc<TerminalNode<'input, CypherParserContextType>>>
    where
        Self: Sized,
    {
        self.get_token(Cypher_SP, i)
    }
}

impl<'input> OC_ParenthesizedExpressionContextAttrs<'input>
    for OC_ParenthesizedExpressionContext<'input>
{
}

impl<'input, I> CypherParser<'input, I>
where
    I: TokenStream<'input, TF = LocalTokenFactory<'input>> + TidAble<'input>,
{
    pub fn oC_ParenthesizedExpression(
        &mut self,
    ) -> Result<Rc<OC_ParenthesizedExpressionContextAll<'input>>, ANTLRError> {
        let mut recog = self;
        let _parentctx = recog.ctx.take();
        let mut _localctx =
            OC_ParenthesizedExpressionContextExt::new(_parentctx.clone(), recog.base.get_state());
        recog
            .base
            .enter_rule(_localctx.clone(), 156, RULE_oC_ParenthesizedExpression);
        let mut _localctx: Rc<OC_ParenthesizedExpressionContextAll> = _localctx;
        let mut _la: i32 = -1;
        let result: Result<(), ANTLRError> = (|| {
            //recog.base.enter_outer_alt(_localctx.clone(), 1)?;
            recog.base.enter_outer_alt(None, 1)?;
            {
                recog.base.set_state(1392);
                recog
                    .base
                    .match_token(Cypher_T__5, &mut recog.err_handler)?;

                recog.base.set_state(1394);
                recog.err_handler.sync(&mut recog.base)?;
                _la = recog.base.input.la(1);
                if _la == Cypher_SP {
                    {
                        recog.base.set_state(1393);
                        recog.base.match_token(Cypher_SP, &mut recog.err_handler)?;
                    }
                }

                /*InvokeRule oC_Expression*/
                recog.base.set_state(1396);
                recog.oC_Expression()?;

                recog.base.set_state(1398);
                recog.err_handler.sync(&mut recog.base)?;
                _la = recog.base.input.la(1);
                if _la == Cypher_SP {
                    {
                        recog.base.set_state(1397);
                        recog.base.match_token(Cypher_SP, &mut recog.err_handler)?;
                    }
                }

                recog.base.set_state(1400);
                recog
                    .base
                    .match_token(Cypher_T__6, &mut recog.err_handler)?;
            }
            Ok(())
        })();
        match result {
            Ok(_) => {}
            Err(e @ ANTLRError::FallThrough(_)) => return Err(e),
            Err(ref re) => {
                //_localctx.exception = re;
                recog.err_handler.report_error(&mut recog.base, re);
                recog.err_handler.recover(&mut recog.base, re)?;
            }
        }
        recog.base.exit_rule()?;

        Ok(_localctx)
    }
}
//------------------- oC_IdInColl ----------------
pub type OC_IdInCollContextAll<'input> = OC_IdInCollContext<'input>;

pub type OC_IdInCollContext<'input> = BaseParserRuleContext<'input, OC_IdInCollContextExt<'input>>;

#[derive(Clone)]
pub struct OC_IdInCollContextExt<'input> {
    ph: PhantomData<&'input str>,
}

impl<'input> CypherParserContext<'input> for OC_IdInCollContext<'input> {}

impl<'input, 'a> Listenable<dyn CypherListener<'input> + 'a> for OC_IdInCollContext<'input> {
    fn enter(&self, listener: &mut (dyn CypherListener<'input> + 'a)) -> Result<(), ANTLRError> {
        listener.enter_every_rule(self)?;
        listener.enter_oC_IdInColl(self);
        Ok(())
    }
    fn exit(&self, listener: &mut (dyn CypherListener<'input> + 'a)) -> Result<(), ANTLRError> {
        listener.exit_oC_IdInColl(self);
        listener.exit_every_rule(self)?;
        Ok(())
    }
}

impl<'input, 'a> Visitable<dyn CypherVisitor<'input> + 'a> for OC_IdInCollContext<'input> {
    fn accept(&self, visitor: &mut (dyn CypherVisitor<'input> + 'a)) {
        visitor.visit_oC_IdInColl(self);
    }
}

impl<'input> CustomRuleContext<'input> for OC_IdInCollContextExt<'input> {
    type TF = LocalTokenFactory<'input>;
    type Ctx = CypherParserContextType;
    fn get_rule_index(&self) -> usize {
        RULE_oC_IdInColl
    }
    //fn type_rule_index() -> usize where Self: Sized { RULE_oC_IdInColl }
}
antlr4rust::tid! {OC_IdInCollContextExt<'a>}

impl<'input> OC_IdInCollContextExt<'input> {
    fn new(
        parent: Option<Rc<dyn CypherParserContext<'input> + 'input>>,
        invoking_state: i32,
    ) -> Rc<OC_IdInCollContextAll<'input>> {
        Rc::new(BaseParserRuleContext::new_parser_ctx(
            parent,
            invoking_state,
            OC_IdInCollContextExt { ph: PhantomData },
        ))
    }
}

pub trait OC_IdInCollContextAttrs<'input>:
    CypherParserContext<'input> + BorrowMut<OC_IdInCollContextExt<'input>>
{
    fn oC_Variable(&self) -> Option<Rc<OC_VariableContextAll<'input>>>
    where
        Self: Sized,
    {
        self.child_of_type(0)
    }
    /// Retrieves all `TerminalNode`s corresponding to token SP in current rule
    fn SP_all(&self) -> Vec<Rc<TerminalNode<'input, CypherParserContextType>>>
    where
        Self: Sized,
    {
        self.children_of_type()
    }
    /// Retrieves 'i's TerminalNode corresponding to token SP, starting from 0.
    /// Returns `None` if number of children corresponding to token SP is less or equal than `i`.
    fn SP(&self, i: usize) -> Option<Rc<TerminalNode<'input, CypherParserContextType>>>
    where
        Self: Sized,
    {
        self.get_token(Cypher_SP, i)
    }
    /// Retrieves first TerminalNode corresponding to token IN
    /// Returns `None` if there is no child corresponding to token IN
    fn IN(&self) -> Option<Rc<TerminalNode<'input, CypherParserContextType>>>
    where
        Self: Sized,
    {
        self.get_token(Cypher_IN, 0)
    }
    fn oC_Expression(&self) -> Option<Rc<OC_ExpressionContextAll<'input>>>
    where
        Self: Sized,
    {
        self.child_of_type(0)
    }
}

impl<'input> OC_IdInCollContextAttrs<'input> for OC_IdInCollContext<'input> {}

impl<'input, I> CypherParser<'input, I>
where
    I: TokenStream<'input, TF = LocalTokenFactory<'input>> + TidAble<'input>,
{
    pub fn oC_IdInColl(&mut self) -> Result<Rc<OC_IdInCollContextAll<'input>>, ANTLRError> {
        let mut recog = self;
        let _parentctx = recog.ctx.take();
        let mut _localctx = OC_IdInCollContextExt::new(_parentctx.clone(), recog.base.get_state());
        recog
            .base
            .enter_rule(_localctx.clone(), 158, RULE_oC_IdInColl);
        let mut _localctx: Rc<OC_IdInCollContextAll> = _localctx;
        let result: Result<(), ANTLRError> = (|| {
            //recog.base.enter_outer_alt(_localctx.clone(), 1)?;
            recog.base.enter_outer_alt(None, 1)?;
            {
                /*InvokeRule oC_Variable*/
                recog.base.set_state(1402);
                recog.oC_Variable()?;

                recog.base.set_state(1403);
                recog.base.match_token(Cypher_SP, &mut recog.err_handler)?;

                recog.base.set_state(1404);
                recog.base.match_token(Cypher_IN, &mut recog.err_handler)?;

                recog.base.set_state(1405);
                recog.base.match_token(Cypher_SP, &mut recog.err_handler)?;

                /*InvokeRule oC_Expression*/
                recog.base.set_state(1406);
                recog.oC_Expression()?;
            }
            Ok(())
        })();
        match result {
            Ok(_) => {}
            Err(e @ ANTLRError::FallThrough(_)) => return Err(e),
            Err(ref re) => {
                //_localctx.exception = re;
                recog.err_handler.report_error(&mut recog.base, re);
                recog.err_handler.recover(&mut recog.base, re)?;
            }
        }
        recog.base.exit_rule()?;

        Ok(_localctx)
    }
}
//------------------- oC_FunctionInvocation ----------------
pub type OC_FunctionInvocationContextAll<'input> = OC_FunctionInvocationContext<'input>;

pub type OC_FunctionInvocationContext<'input> =
    BaseParserRuleContext<'input, OC_FunctionInvocationContextExt<'input>>;

#[derive(Clone)]
pub struct OC_FunctionInvocationContextExt<'input> {
    ph: PhantomData<&'input str>,
}

impl<'input> CypherParserContext<'input> for OC_FunctionInvocationContext<'input> {}

impl<'input, 'a> Listenable<dyn CypherListener<'input> + 'a>
    for OC_FunctionInvocationContext<'input>
{
    fn enter(&self, listener: &mut (dyn CypherListener<'input> + 'a)) -> Result<(), ANTLRError> {
        listener.enter_every_rule(self)?;
        listener.enter_oC_FunctionInvocation(self);
        Ok(())
    }
    fn exit(&self, listener: &mut (dyn CypherListener<'input> + 'a)) -> Result<(), ANTLRError> {
        listener.exit_oC_FunctionInvocation(self);
        listener.exit_every_rule(self)?;
        Ok(())
    }
}

impl<'input, 'a> Visitable<dyn CypherVisitor<'input> + 'a>
    for OC_FunctionInvocationContext<'input>
{
    fn accept(&self, visitor: &mut (dyn CypherVisitor<'input> + 'a)) {
        visitor.visit_oC_FunctionInvocation(self);
    }
}

impl<'input> CustomRuleContext<'input> for OC_FunctionInvocationContextExt<'input> {
    type TF = LocalTokenFactory<'input>;
    type Ctx = CypherParserContextType;
    fn get_rule_index(&self) -> usize {
        RULE_oC_FunctionInvocation
    }
    //fn type_rule_index() -> usize where Self: Sized { RULE_oC_FunctionInvocation }
}
antlr4rust::tid! {OC_FunctionInvocationContextExt<'a>}

impl<'input> OC_FunctionInvocationContextExt<'input> {
    fn new(
        parent: Option<Rc<dyn CypherParserContext<'input> + 'input>>,
        invoking_state: i32,
    ) -> Rc<OC_FunctionInvocationContextAll<'input>> {
        Rc::new(BaseParserRuleContext::new_parser_ctx(
            parent,
            invoking_state,
            OC_FunctionInvocationContextExt { ph: PhantomData },
        ))
    }
}

pub trait OC_FunctionInvocationContextAttrs<'input>:
    CypherParserContext<'input> + BorrowMut<OC_FunctionInvocationContextExt<'input>>
{
    fn oC_FunctionName(&self) -> Option<Rc<OC_FunctionNameContextAll<'input>>>
    where
        Self: Sized,
    {
        self.child_of_type(0)
    }
    /// Retrieves all `TerminalNode`s corresponding to token SP in current rule
    fn SP_all(&self) -> Vec<Rc<TerminalNode<'input, CypherParserContextType>>>
    where
        Self: Sized,
    {
        self.children_of_type()
    }
    /// Retrieves 'i's TerminalNode corresponding to token SP, starting from 0.
    /// Returns `None` if number of children corresponding to token SP is less or equal than `i`.
    fn SP(&self, i: usize) -> Option<Rc<TerminalNode<'input, CypherParserContextType>>>
    where
        Self: Sized,
    {
        self.get_token(Cypher_SP, i)
    }
    /// Retrieves first TerminalNode corresponding to token DISTINCT
    /// Returns `None` if there is no child corresponding to token DISTINCT
    fn DISTINCT(&self) -> Option<Rc<TerminalNode<'input, CypherParserContextType>>>
    where
        Self: Sized,
    {
        self.get_token(Cypher_DISTINCT, 0)
    }
    fn oC_Expression_all(&self) -> Vec<Rc<OC_ExpressionContextAll<'input>>>
    where
        Self: Sized,
    {
        self.children_of_type()
    }
    fn oC_Expression(&self, i: usize) -> Option<Rc<OC_ExpressionContextAll<'input>>>
    where
        Self: Sized,
    {
        self.child_of_type(i)
    }
}

impl<'input> OC_FunctionInvocationContextAttrs<'input> for OC_FunctionInvocationContext<'input> {}

impl<'input, I> CypherParser<'input, I>
where
    I: TokenStream<'input, TF = LocalTokenFactory<'input>> + TidAble<'input>,
{
    pub fn oC_FunctionInvocation(
        &mut self,
    ) -> Result<Rc<OC_FunctionInvocationContextAll<'input>>, ANTLRError> {
        let mut recog = self;
        let _parentctx = recog.ctx.take();
        let mut _localctx =
            OC_FunctionInvocationContextExt::new(_parentctx.clone(), recog.base.get_state());
        recog
            .base
            .enter_rule(_localctx.clone(), 160, RULE_oC_FunctionInvocation);
        let mut _localctx: Rc<OC_FunctionInvocationContextAll> = _localctx;
        let mut _la: i32 = -1;
        let result: Result<(), ANTLRError> = (|| {
            //recog.base.enter_outer_alt(_localctx.clone(), 1)?;
            recog.base.enter_outer_alt(None, 1)?;
            {
                /*InvokeRule oC_FunctionName*/
                recog.base.set_state(1408);
                recog.oC_FunctionName()?;

                recog.base.set_state(1410);
                recog.err_handler.sync(&mut recog.base)?;
                _la = recog.base.input.la(1);
                if _la == Cypher_SP {
                    {
                        recog.base.set_state(1409);
                        recog.base.match_token(Cypher_SP, &mut recog.err_handler)?;
                    }
                }

                recog.base.set_state(1412);
                recog
                    .base
                    .match_token(Cypher_T__5, &mut recog.err_handler)?;

                recog.base.set_state(1414);
                recog.err_handler.sync(&mut recog.base)?;
                _la = recog.base.input.la(1);
                if _la == Cypher_SP {
                    {
                        recog.base.set_state(1413);
                        recog.base.match_token(Cypher_SP, &mut recog.err_handler)?;
                    }
                }

                recog.base.set_state(1420);
                recog.err_handler.sync(&mut recog.base)?;
                _la = recog.base.input.la(1);
                if _la == Cypher_DISTINCT {
                    {
                        recog.base.set_state(1416);
                        recog
                            .base
                            .match_token(Cypher_DISTINCT, &mut recog.err_handler)?;

                        recog.base.set_state(1418);
                        recog.err_handler.sync(&mut recog.base)?;
                        _la = recog.base.input.la(1);
                        if _la == Cypher_SP {
                            {
                                recog.base.set_state(1417);
                                recog.base.match_token(Cypher_SP, &mut recog.err_handler)?;
                            }
                        }
                    }
                }

                recog.base.set_state(1439);
                recog.err_handler.sync(&mut recog.base)?;
                _la = recog.base.input.la(1);
                if (((_la) & !0x3f) == 0 && ((1usize << _la) & 84672832) != 0)
                    || _la == Cypher_ALL
                    || _la == Cypher_NOT
                    || (((_la - 82) & !0x3f) == 0 && ((1usize << (_la - 82)) & 117702535) != 0)
                    || (((_la - 120) & !0x3f) == 0 && ((1usize << (_la - 120)) & 39) != 0)
                {
                    {
                        /*InvokeRule oC_Expression*/
                        recog.base.set_state(1422);
                        recog.oC_Expression()?;

                        recog.base.set_state(1424);
                        recog.err_handler.sync(&mut recog.base)?;
                        _la = recog.base.input.la(1);
                        if _la == Cypher_SP {
                            {
                                recog.base.set_state(1423);
                                recog.base.match_token(Cypher_SP, &mut recog.err_handler)?;
                            }
                        }

                        recog.base.set_state(1436);
                        recog.err_handler.sync(&mut recog.base)?;
                        _la = recog.base.input.la(1);
                        while _la == Cypher_T__1 {
                            {
                                {
                                    recog.base.set_state(1426);
                                    recog
                                        .base
                                        .match_token(Cypher_T__1, &mut recog.err_handler)?;

                                    recog.base.set_state(1428);
                                    recog.err_handler.sync(&mut recog.base)?;
                                    _la = recog.base.input.la(1);
                                    if _la == Cypher_SP {
                                        {
                                            recog.base.set_state(1427);
                                            recog
                                                .base
                                                .match_token(Cypher_SP, &mut recog.err_handler)?;
                                        }
                                    }

                                    /*InvokeRule oC_Expression*/
                                    recog.base.set_state(1430);
                                    recog.oC_Expression()?;

                                    recog.base.set_state(1432);
                                    recog.err_handler.sync(&mut recog.base)?;
                                    _la = recog.base.input.la(1);
                                    if _la == Cypher_SP {
                                        {
                                            recog.base.set_state(1431);
                                            recog
                                                .base
                                                .match_token(Cypher_SP, &mut recog.err_handler)?;
                                        }
                                    }
                                }
                            }
                            recog.base.set_state(1438);
                            recog.err_handler.sync(&mut recog.base)?;
                            _la = recog.base.input.la(1);
                        }
                    }
                }

                recog.base.set_state(1441);
                recog
                    .base
                    .match_token(Cypher_T__6, &mut recog.err_handler)?;
            }
            Ok(())
        })();
        match result {
            Ok(_) => {}
            Err(e @ ANTLRError::FallThrough(_)) => return Err(e),
            Err(ref re) => {
                //_localctx.exception = re;
                recog.err_handler.report_error(&mut recog.base, re);
                recog.err_handler.recover(&mut recog.base, re)?;
            }
        }
        recog.base.exit_rule()?;

        Ok(_localctx)
    }
}
//------------------- oC_CastExpression ----------------
pub type OC_CastExpressionContextAll<'input> = OC_CastExpressionContext<'input>;

pub type OC_CastExpressionContext<'input> =
    BaseParserRuleContext<'input, OC_CastExpressionContextExt<'input>>;

#[derive(Clone)]
pub struct OC_CastExpressionContextExt<'input> {
    ph: PhantomData<&'input str>,
}

impl<'input> CypherParserContext<'input> for OC_CastExpressionContext<'input> {}

impl<'input, 'a> Listenable<dyn CypherListener<'input> + 'a> for OC_CastExpressionContext<'input> {
    fn enter(&self, listener: &mut (dyn CypherListener<'input> + 'a)) -> Result<(), ANTLRError> {
        listener.enter_every_rule(self)?;
        listener.enter_oC_CastExpression(self);
        Ok(())
    }
    fn exit(&self, listener: &mut (dyn CypherListener<'input> + 'a)) -> Result<(), ANTLRError> {
        listener.exit_oC_CastExpression(self);
        listener.exit_every_rule(self)?;
        Ok(())
    }
}

impl<'input, 'a> Visitable<dyn CypherVisitor<'input> + 'a> for OC_CastExpressionContext<'input> {
    fn accept(&self, visitor: &mut (dyn CypherVisitor<'input> + 'a)) {
        visitor.visit_oC_CastExpression(self);
    }
}

impl<'input> CustomRuleContext<'input> for OC_CastExpressionContextExt<'input> {
    type TF = LocalTokenFactory<'input>;
    type Ctx = CypherParserContextType;
    fn get_rule_index(&self) -> usize {
        RULE_oC_CastExpression
    }
    //fn type_rule_index() -> usize where Self: Sized { RULE_oC_CastExpression }
}
antlr4rust::tid! {OC_CastExpressionContextExt<'a>}

impl<'input> OC_CastExpressionContextExt<'input> {
    fn new(
        parent: Option<Rc<dyn CypherParserContext<'input> + 'input>>,
        invoking_state: i32,
    ) -> Rc<OC_CastExpressionContextAll<'input>> {
        Rc::new(BaseParserRuleContext::new_parser_ctx(
            parent,
            invoking_state,
            OC_CastExpressionContextExt { ph: PhantomData },
        ))
    }
}

pub trait OC_CastExpressionContextAttrs<'input>:
    CypherParserContext<'input> + BorrowMut<OC_CastExpressionContextExt<'input>>
{
    /// Retrieves first TerminalNode corresponding to token CAST
    /// Returns `None` if there is no child corresponding to token CAST
    fn CAST(&self) -> Option<Rc<TerminalNode<'input, CypherParserContextType>>>
    where
        Self: Sized,
    {
        self.get_token(Cypher_CAST, 0)
    }
    fn oC_Expression_all(&self) -> Vec<Rc<OC_ExpressionContextAll<'input>>>
    where
        Self: Sized,
    {
        self.children_of_type()
    }
    fn oC_Expression(&self, i: usize) -> Option<Rc<OC_ExpressionContextAll<'input>>>
    where
        Self: Sized,
    {
        self.child_of_type(i)
    }
    /// Retrieves all `TerminalNode`s corresponding to token SP in current rule
    fn SP_all(&self) -> Vec<Rc<TerminalNode<'input, CypherParserContextType>>>
    where
        Self: Sized,
    {
        self.children_of_type()
    }
    /// Retrieves 'i's TerminalNode corresponding to token SP, starting from 0.
    /// Returns `None` if number of children corresponding to token SP is less or equal than `i`.
    fn SP(&self, i: usize) -> Option<Rc<TerminalNode<'input, CypherParserContextType>>>
    where
        Self: Sized,
    {
        self.get_token(Cypher_SP, i)
    }
    /// Retrieves first TerminalNode corresponding to token AS
    /// Returns `None` if there is no child corresponding to token AS
    fn AS(&self) -> Option<Rc<TerminalNode<'input, CypherParserContextType>>>
    where
        Self: Sized,
    {
        self.get_token(Cypher_AS, 0)
    }
    fn oC_CastType(&self) -> Option<Rc<OC_CastTypeContextAll<'input>>>
    where
        Self: Sized,
    {
        self.child_of_type(0)
    }
}

impl<'input> OC_CastExpressionContextAttrs<'input> for OC_CastExpressionContext<'input> {}

impl<'input, I> CypherParser<'input, I>
where
    I: TokenStream<'input, TF = LocalTokenFactory<'input>> + TidAble<'input>,
{
    pub fn oC_CastExpression(
        &mut self,
    ) -> Result<Rc<OC_CastExpressionContextAll<'input>>, ANTLRError> {
        let mut recog = self;
        let _parentctx = recog.ctx.take();
        let mut _localctx =
            OC_CastExpressionContextExt::new(_parentctx.clone(), recog.base.get_state());
        recog
            .base
            .enter_rule(_localctx.clone(), 162, RULE_oC_CastExpression);
        let mut _localctx: Rc<OC_CastExpressionContextAll> = _localctx;
        let mut _la: i32 = -1;
        let result: Result<(), ANTLRError> = (|| {
            //recog.base.enter_outer_alt(_localctx.clone(), 1)?;
            recog.base.enter_outer_alt(None, 1)?;
            {
                recog.base.set_state(1443);
                recog
                    .base
                    .match_token(Cypher_CAST, &mut recog.err_handler)?;

                recog.base.set_state(1445);
                recog.err_handler.sync(&mut recog.base)?;
                _la = recog.base.input.la(1);
                if _la == Cypher_SP {
                    {
                        recog.base.set_state(1444);
                        recog.base.match_token(Cypher_SP, &mut recog.err_handler)?;
                    }
                }

                recog.base.set_state(1447);
                recog
                    .base
                    .match_token(Cypher_T__5, &mut recog.err_handler)?;

                recog.base.set_state(1449);
                recog.err_handler.sync(&mut recog.base)?;
                _la = recog.base.input.la(1);
                if _la == Cypher_SP {
                    {
                        recog.base.set_state(1448);
                        recog.base.match_token(Cypher_SP, &mut recog.err_handler)?;
                    }
                }

                /*InvokeRule oC_Expression*/
                recog.base.set_state(1451);
                recog.oC_Expression()?;

                recog.base.set_state(1453);
                recog.err_handler.sync(&mut recog.base)?;
                _la = recog.base.input.la(1);
                if _la == Cypher_SP {
                    {
                        recog.base.set_state(1452);
                        recog.base.match_token(Cypher_SP, &mut recog.err_handler)?;
                    }
                }

                recog.base.set_state(1463);
                recog.err_handler.sync(&mut recog.base)?;
                match recog.base.input.la(1) {
                    Cypher_T__1 => {
                        {
                            {
                                recog.base.set_state(1455);
                                recog
                                    .base
                                    .match_token(Cypher_T__1, &mut recog.err_handler)?;

                                recog.base.set_state(1457);
                                recog.err_handler.sync(&mut recog.base)?;
                                _la = recog.base.input.la(1);
                                if _la == Cypher_SP {
                                    {
                                        recog.base.set_state(1456);
                                        recog
                                            .base
                                            .match_token(Cypher_SP, &mut recog.err_handler)?;
                                    }
                                }

                                /*InvokeRule oC_Expression*/
                                recog.base.set_state(1459);
                                recog.oC_Expression()?;
                            }
                        }
                    }

                    Cypher_AS => {
                        {
                            {
                                recog.base.set_state(1460);
                                recog.base.match_token(Cypher_AS, &mut recog.err_handler)?;

                                recog.base.set_state(1461);
                                recog.base.match_token(Cypher_SP, &mut recog.err_handler)?;

                                /*InvokeRule oC_CastType*/
                                recog.base.set_state(1462);
                                recog.oC_CastType()?;
                            }
                        }
                    }

                    _ => Err(ANTLRError::NoAltError(NoViableAltError::new(
                        &mut recog.base,
                    )))?,
                }
                recog.base.set_state(1466);
                recog.err_handler.sync(&mut recog.base)?;
                _la = recog.base.input.la(1);
                if _la == Cypher_SP {
                    {
                        recog.base.set_state(1465);
                        recog.base.match_token(Cypher_SP, &mut recog.err_handler)?;
                    }
                }

                recog.base.set_state(1468);
                recog
                    .base
                    .match_token(Cypher_T__6, &mut recog.err_handler)?;
            }
            Ok(())
        })();
        match result {
            Ok(_) => {}
            Err(e @ ANTLRError::FallThrough(_)) => return Err(e),
            Err(ref re) => {
                //_localctx.exception = re;
                recog.err_handler.report_error(&mut recog.base, re);
                recog.err_handler.recover(&mut recog.base, re)?;
            }
        }
        recog.base.exit_rule()?;

        Ok(_localctx)
    }
}
//------------------- oC_CastType ----------------
pub type OC_CastTypeContextAll<'input> = OC_CastTypeContext<'input>;

pub type OC_CastTypeContext<'input> = BaseParserRuleContext<'input, OC_CastTypeContextExt<'input>>;

#[derive(Clone)]
pub struct OC_CastTypeContextExt<'input> {
    ph: PhantomData<&'input str>,
}

impl<'input> CypherParserContext<'input> for OC_CastTypeContext<'input> {}

impl<'input, 'a> Listenable<dyn CypherListener<'input> + 'a> for OC_CastTypeContext<'input> {
    fn enter(&self, listener: &mut (dyn CypherListener<'input> + 'a)) -> Result<(), ANTLRError> {
        listener.enter_every_rule(self)?;
        listener.enter_oC_CastType(self);
        Ok(())
    }
    fn exit(&self, listener: &mut (dyn CypherListener<'input> + 'a)) -> Result<(), ANTLRError> {
        listener.exit_oC_CastType(self);
        listener.exit_every_rule(self)?;
        Ok(())
    }
}

impl<'input, 'a> Visitable<dyn CypherVisitor<'input> + 'a> for OC_CastTypeContext<'input> {
    fn accept(&self, visitor: &mut (dyn CypherVisitor<'input> + 'a)) {
        visitor.visit_oC_CastType(self);
    }
}

impl<'input> CustomRuleContext<'input> for OC_CastTypeContextExt<'input> {
    type TF = LocalTokenFactory<'input>;
    type Ctx = CypherParserContextType;
    fn get_rule_index(&self) -> usize {
        RULE_oC_CastType
    }
    //fn type_rule_index() -> usize where Self: Sized { RULE_oC_CastType }
}
antlr4rust::tid! {OC_CastTypeContextExt<'a>}

impl<'input> OC_CastTypeContextExt<'input> {
    fn new(
        parent: Option<Rc<dyn CypherParserContext<'input> + 'input>>,
        invoking_state: i32,
    ) -> Rc<OC_CastTypeContextAll<'input>> {
        Rc::new(BaseParserRuleContext::new_parser_ctx(
            parent,
            invoking_state,
            OC_CastTypeContextExt { ph: PhantomData },
        ))
    }
}

pub trait OC_CastTypeContextAttrs<'input>:
    CypherParserContext<'input> + BorrowMut<OC_CastTypeContextExt<'input>>
{
    fn oC_CastTypeName(&self) -> Option<Rc<OC_CastTypeNameContextAll<'input>>>
    where
        Self: Sized,
    {
        self.child_of_type(0)
    }
    fn oC_CastTypeArgument_all(&self) -> Vec<Rc<OC_CastTypeArgumentContextAll<'input>>>
    where
        Self: Sized,
    {
        self.children_of_type()
    }
    fn oC_CastTypeArgument(&self, i: usize) -> Option<Rc<OC_CastTypeArgumentContextAll<'input>>>
    where
        Self: Sized,
    {
        self.child_of_type(i)
    }
    /// Retrieves all `TerminalNode`s corresponding to token SP in current rule
    fn SP_all(&self) -> Vec<Rc<TerminalNode<'input, CypherParserContextType>>>
    where
        Self: Sized,
    {
        self.children_of_type()
    }
    /// Retrieves 'i's TerminalNode corresponding to token SP, starting from 0.
    /// Returns `None` if number of children corresponding to token SP is less or equal than `i`.
    fn SP(&self, i: usize) -> Option<Rc<TerminalNode<'input, CypherParserContextType>>>
    where
        Self: Sized,
    {
        self.get_token(Cypher_SP, i)
    }
    /// Retrieves all `TerminalNode`s corresponding to token DecimalInteger in current rule
    fn DecimalInteger_all(&self) -> Vec<Rc<TerminalNode<'input, CypherParserContextType>>>
    where
        Self: Sized,
    {
        self.children_of_type()
    }
    /// Retrieves 'i's TerminalNode corresponding to token DecimalInteger, starting from 0.
    /// Returns `None` if number of children corresponding to token DecimalInteger is less or equal than `i`.
    fn DecimalInteger(&self, i: usize) -> Option<Rc<TerminalNode<'input, CypherParserContextType>>>
    where
        Self: Sized,
    {
        self.get_token(Cypher_DecimalInteger, i)
    }
}

impl<'input> OC_CastTypeContextAttrs<'input> for OC_CastTypeContext<'input> {}

impl<'input, I> CypherParser<'input, I>
where
    I: TokenStream<'input, TF = LocalTokenFactory<'input>> + TidAble<'input>,
{
    pub fn oC_CastType(&mut self) -> Result<Rc<OC_CastTypeContextAll<'input>>, ANTLRError> {
        let mut recog = self;
        let _parentctx = recog.ctx.take();
        let mut _localctx = OC_CastTypeContextExt::new(_parentctx.clone(), recog.base.get_state());
        recog
            .base
            .enter_rule(_localctx.clone(), 164, RULE_oC_CastType);
        let mut _localctx: Rc<OC_CastTypeContextAll> = _localctx;
        let mut _la: i32 = -1;
        let result: Result<(), ANTLRError> = (|| {
            let mut _alt: i32;
            //recog.base.enter_outer_alt(_localctx.clone(), 1)?;
            recog.base.enter_outer_alt(None, 1)?;
            {
                /*InvokeRule oC_CastTypeName*/
                recog.base.set_state(1470);
                recog.oC_CastTypeName()?;

                recog.base.set_state(1497);
                recog.err_handler.sync(&mut recog.base)?;
                match recog.interpreter.adaptive_predict(278, &mut recog.base)? {
                    x if x == 1 => {
                        {
                            recog.base.set_state(1472);
                            recog.err_handler.sync(&mut recog.base)?;
                            _la = recog.base.input.la(1);
                            if _la == Cypher_SP {
                                {
                                    recog.base.set_state(1471);
                                    recog.base.match_token(Cypher_SP, &mut recog.err_handler)?;
                                }
                            }

                            recog.base.set_state(1474);
                            recog
                                .base
                                .match_token(Cypher_T__5, &mut recog.err_handler)?;

                            recog.base.set_state(1476);
                            recog.err_handler.sync(&mut recog.base)?;
                            _la = recog.base.input.la(1);
                            if _la == Cypher_SP {
                                {
                                    recog.base.set_state(1475);
                                    recog.base.match_token(Cypher_SP, &mut recog.err_handler)?;
                                }
                            }

                            /*InvokeRule oC_CastTypeArgument*/
                            recog.base.set_state(1478);
                            recog.oC_CastTypeArgument()?;

                            recog.base.set_state(1489);
                            recog.err_handler.sync(&mut recog.base)?;
                            _alt = recog.interpreter.adaptive_predict(276, &mut recog.base)?;
                            while { _alt != 2 && _alt != INVALID_ALT } {
                                if _alt == 1 {
                                    {
                                        {
                                            recog.base.set_state(1480);
                                            recog.err_handler.sync(&mut recog.base)?;
                                            _la = recog.base.input.la(1);
                                            if _la == Cypher_SP {
                                                {
                                                    recog.base.set_state(1479);
                                                    recog.base.match_token(
                                                        Cypher_SP,
                                                        &mut recog.err_handler,
                                                    )?;
                                                }
                                            }

                                            recog.base.set_state(1482);
                                            recog
                                                .base
                                                .match_token(Cypher_T__1, &mut recog.err_handler)?;

                                            recog.base.set_state(1484);
                                            recog.err_handler.sync(&mut recog.base)?;
                                            _la = recog.base.input.la(1);
                                            if _la == Cypher_SP {
                                                {
                                                    recog.base.set_state(1483);
                                                    recog.base.match_token(
                                                        Cypher_SP,
                                                        &mut recog.err_handler,
                                                    )?;
                                                }
                                            }

                                            /*InvokeRule oC_CastTypeArgument*/
                                            recog.base.set_state(1486);
                                            recog.oC_CastTypeArgument()?;
                                        }
                                    }
                                }
                                recog.base.set_state(1491);
                                recog.err_handler.sync(&mut recog.base)?;
                                _alt = recog.interpreter.adaptive_predict(276, &mut recog.base)?;
                            }
                            recog.base.set_state(1493);
                            recog.err_handler.sync(&mut recog.base)?;
                            _la = recog.base.input.la(1);
                            if _la == Cypher_SP {
                                {
                                    recog.base.set_state(1492);
                                    recog.base.match_token(Cypher_SP, &mut recog.err_handler)?;
                                }
                            }

                            recog.base.set_state(1495);
                            recog
                                .base
                                .match_token(Cypher_T__6, &mut recog.err_handler)?;
                        }
                    }

                    _ => {}
                }
                recog.base.set_state(1515);
                recog.err_handler.sync(&mut recog.base)?;
                _alt = recog.interpreter.adaptive_predict(283, &mut recog.base)?;
                while { _alt != 2 && _alt != INVALID_ALT } {
                    if _alt == 1 {
                        {
                            {
                                recog.base.set_state(1500);
                                recog.err_handler.sync(&mut recog.base)?;
                                _la = recog.base.input.la(1);
                                if _la == Cypher_SP {
                                    {
                                        recog.base.set_state(1499);
                                        recog
                                            .base
                                            .match_token(Cypher_SP, &mut recog.err_handler)?;
                                    }
                                }

                                recog.base.set_state(1502);
                                recog
                                    .base
                                    .match_token(Cypher_T__7, &mut recog.err_handler)?;

                                recog.base.set_state(1504);
                                recog.err_handler.sync(&mut recog.base)?;
                                match recog.interpreter.adaptive_predict(280, &mut recog.base)? {
                                    x if x == 1 => {
                                        recog.base.set_state(1503);
                                        recog
                                            .base
                                            .match_token(Cypher_SP, &mut recog.err_handler)?;
                                    }

                                    _ => {}
                                }
                                recog.base.set_state(1507);
                                recog.err_handler.sync(&mut recog.base)?;
                                _la = recog.base.input.la(1);
                                if _la == Cypher_DecimalInteger {
                                    {
                                        recog.base.set_state(1506);
                                        recog.base.match_token(
                                            Cypher_DecimalInteger,
                                            &mut recog.err_handler,
                                        )?;
                                    }
                                }

                                recog.base.set_state(1510);
                                recog.err_handler.sync(&mut recog.base)?;
                                _la = recog.base.input.la(1);
                                if _la == Cypher_SP {
                                    {
                                        recog.base.set_state(1509);
                                        recog
                                            .base
                                            .match_token(Cypher_SP, &mut recog.err_handler)?;
                                    }
                                }

                                recog.base.set_state(1512);
                                recog
                                    .base
                                    .match_token(Cypher_T__8, &mut recog.err_handler)?;
                            }
                        }
                    }
                    recog.base.set_state(1517);
                    recog.err_handler.sync(&mut recog.base)?;
                    _alt = recog.interpreter.adaptive_predict(283, &mut recog.base)?;
                }
            }
            Ok(())
        })();
        match result {
            Ok(_) => {}
            Err(e @ ANTLRError::FallThrough(_)) => return Err(e),
            Err(ref re) => {
                //_localctx.exception = re;
                recog.err_handler.report_error(&mut recog.base, re);
                recog.err_handler.recover(&mut recog.base, re)?;
            }
        }
        recog.base.exit_rule()?;

        Ok(_localctx)
    }
}
//------------------- oC_CastTypeArgument ----------------
pub type OC_CastTypeArgumentContextAll<'input> = OC_CastTypeArgumentContext<'input>;

pub type OC_CastTypeArgumentContext<'input> =
    BaseParserRuleContext<'input, OC_CastTypeArgumentContextExt<'input>>;

#[derive(Clone)]
pub struct OC_CastTypeArgumentContextExt<'input> {
    ph: PhantomData<&'input str>,
}

impl<'input> CypherParserContext<'input> for OC_CastTypeArgumentContext<'input> {}

impl<'input, 'a> Listenable<dyn CypherListener<'input> + 'a>
    for OC_CastTypeArgumentContext<'input>
{
    fn enter(&self, listener: &mut (dyn CypherListener<'input> + 'a)) -> Result<(), ANTLRError> {
        listener.enter_every_rule(self)?;
        listener.enter_oC_CastTypeArgument(self);
        Ok(())
    }
    fn exit(&self, listener: &mut (dyn CypherListener<'input> + 'a)) -> Result<(), ANTLRError> {
        listener.exit_oC_CastTypeArgument(self);
        listener.exit_every_rule(self)?;
        Ok(())
    }
}

impl<'input, 'a> Visitable<dyn CypherVisitor<'input> + 'a> for OC_CastTypeArgumentContext<'input> {
    fn accept(&self, visitor: &mut (dyn CypherVisitor<'input> + 'a)) {
        visitor.visit_oC_CastTypeArgument(self);
    }
}

impl<'input> CustomRuleContext<'input> for OC_CastTypeArgumentContextExt<'input> {
    type TF = LocalTokenFactory<'input>;
    type Ctx = CypherParserContextType;
    fn get_rule_index(&self) -> usize {
        RULE_oC_CastTypeArgument
    }
    //fn type_rule_index() -> usize where Self: Sized { RULE_oC_CastTypeArgument }
}
antlr4rust::tid! {OC_CastTypeArgumentContextExt<'a>}

impl<'input> OC_CastTypeArgumentContextExt<'input> {
    fn new(
        parent: Option<Rc<dyn CypherParserContext<'input> + 'input>>,
        invoking_state: i32,
    ) -> Rc<OC_CastTypeArgumentContextAll<'input>> {
        Rc::new(BaseParserRuleContext::new_parser_ctx(
            parent,
            invoking_state,
            OC_CastTypeArgumentContextExt { ph: PhantomData },
        ))
    }
}

pub trait OC_CastTypeArgumentContextAttrs<'input>:
    CypherParserContext<'input> + BorrowMut<OC_CastTypeArgumentContextExt<'input>>
{
    /// Retrieves first TerminalNode corresponding to token DecimalInteger
    /// Returns `None` if there is no child corresponding to token DecimalInteger
    fn DecimalInteger(&self) -> Option<Rc<TerminalNode<'input, CypherParserContextType>>>
    where
        Self: Sized,
    {
        self.get_token(Cypher_DecimalInteger, 0)
    }
    fn oC_CastTypeField(&self) -> Option<Rc<OC_CastTypeFieldContextAll<'input>>>
    where
        Self: Sized,
    {
        self.child_of_type(0)
    }
    fn oC_CastType(&self) -> Option<Rc<OC_CastTypeContextAll<'input>>>
    where
        Self: Sized,
    {
        self.child_of_type(0)
    }
}

impl<'input> OC_CastTypeArgumentContextAttrs<'input> for OC_CastTypeArgumentContext<'input> {}

impl<'input, I> CypherParser<'input, I>
where
    I: TokenStream<'input, TF = LocalTokenFactory<'input>> + TidAble<'input>,
{
    pub fn oC_CastTypeArgument(
        &mut self,
    ) -> Result<Rc<OC_CastTypeArgumentContextAll<'input>>, ANTLRError> {
        let mut recog = self;
        let _parentctx = recog.ctx.take();
        let mut _localctx =
            OC_CastTypeArgumentContextExt::new(_parentctx.clone(), recog.base.get_state());
        recog
            .base
            .enter_rule(_localctx.clone(), 166, RULE_oC_CastTypeArgument);
        let mut _localctx: Rc<OC_CastTypeArgumentContextAll> = _localctx;
        let result: Result<(), ANTLRError> = (|| {
            recog.base.set_state(1521);
            recog.err_handler.sync(&mut recog.base)?;
            match recog.interpreter.adaptive_predict(284, &mut recog.base)? {
                1 => {
                    //recog.base.enter_outer_alt(_localctx.clone(), 1)?;
                    recog.base.enter_outer_alt(None, 1)?;
                    {
                        recog.base.set_state(1518);
                        recog
                            .base
                            .match_token(Cypher_DecimalInteger, &mut recog.err_handler)?;
                    }
                }
                2 => {
                    //recog.base.enter_outer_alt(_localctx.clone(), 2)?;
                    recog.base.enter_outer_alt(None, 2)?;
                    {
                        /*InvokeRule oC_CastTypeField*/
                        recog.base.set_state(1519);
                        recog.oC_CastTypeField()?;
                    }
                }
                3 => {
                    //recog.base.enter_outer_alt(_localctx.clone(), 3)?;
                    recog.base.enter_outer_alt(None, 3)?;
                    {
                        /*InvokeRule oC_CastType*/
                        recog.base.set_state(1520);
                        recog.oC_CastType()?;
                    }
                }

                _ => {}
            }
            Ok(())
        })();
        match result {
            Ok(_) => {}
            Err(e @ ANTLRError::FallThrough(_)) => return Err(e),
            Err(ref re) => {
                //_localctx.exception = re;
                recog.err_handler.report_error(&mut recog.base, re);
                recog.err_handler.recover(&mut recog.base, re)?;
            }
        }
        recog.base.exit_rule()?;

        Ok(_localctx)
    }
}
//------------------- oC_CastTypeField ----------------
pub type OC_CastTypeFieldContextAll<'input> = OC_CastTypeFieldContext<'input>;

pub type OC_CastTypeFieldContext<'input> =
    BaseParserRuleContext<'input, OC_CastTypeFieldContextExt<'input>>;

#[derive(Clone)]
pub struct OC_CastTypeFieldContextExt<'input> {
    ph: PhantomData<&'input str>,
}

impl<'input> CypherParserContext<'input> for OC_CastTypeFieldContext<'input> {}

impl<'input, 'a> Listenable<dyn CypherListener<'input> + 'a> for OC_CastTypeFieldContext<'input> {
    fn enter(&self, listener: &mut (dyn CypherListener<'input> + 'a)) -> Result<(), ANTLRError> {
        listener.enter_every_rule(self)?;
        listener.enter_oC_CastTypeField(self);
        Ok(())
    }
    fn exit(&self, listener: &mut (dyn CypherListener<'input> + 'a)) -> Result<(), ANTLRError> {
        listener.exit_oC_CastTypeField(self);
        listener.exit_every_rule(self)?;
        Ok(())
    }
}

impl<'input, 'a> Visitable<dyn CypherVisitor<'input> + 'a> for OC_CastTypeFieldContext<'input> {
    fn accept(&self, visitor: &mut (dyn CypherVisitor<'input> + 'a)) {
        visitor.visit_oC_CastTypeField(self);
    }
}

impl<'input> CustomRuleContext<'input> for OC_CastTypeFieldContextExt<'input> {
    type TF = LocalTokenFactory<'input>;
    type Ctx = CypherParserContextType;
    fn get_rule_index(&self) -> usize {
        RULE_oC_CastTypeField
    }
    //fn type_rule_index() -> usize where Self: Sized { RULE_oC_CastTypeField }
}
antlr4rust::tid! {OC_CastTypeFieldContextExt<'a>}

impl<'input> OC_CastTypeFieldContextExt<'input> {
    fn new(
        parent: Option<Rc<dyn CypherParserContext<'input> + 'input>>,
        invoking_state: i32,
    ) -> Rc<OC_CastTypeFieldContextAll<'input>> {
        Rc::new(BaseParserRuleContext::new_parser_ctx(
            parent,
            invoking_state,
            OC_CastTypeFieldContextExt { ph: PhantomData },
        ))
    }
}

pub trait OC_CastTypeFieldContextAttrs<'input>:
    CypherParserContext<'input> + BorrowMut<OC_CastTypeFieldContextExt<'input>>
{
    fn oC_CastTypeName(&self) -> Option<Rc<OC_CastTypeNameContextAll<'input>>>
    where
        Self: Sized,
    {
        self.child_of_type(0)
    }
    /// Retrieves first TerminalNode corresponding to token SP
    /// Returns `None` if there is no child corresponding to token SP
    fn SP(&self) -> Option<Rc<TerminalNode<'input, CypherParserContextType>>>
    where
        Self: Sized,
    {
        self.get_token(Cypher_SP, 0)
    }
    fn oC_CastType(&self) -> Option<Rc<OC_CastTypeContextAll<'input>>>
    where
        Self: Sized,
    {
        self.child_of_type(0)
    }
}

impl<'input> OC_CastTypeFieldContextAttrs<'input> for OC_CastTypeFieldContext<'input> {}

impl<'input, I> CypherParser<'input, I>
where
    I: TokenStream<'input, TF = LocalTokenFactory<'input>> + TidAble<'input>,
{
    pub fn oC_CastTypeField(
        &mut self,
    ) -> Result<Rc<OC_CastTypeFieldContextAll<'input>>, ANTLRError> {
        let mut recog = self;
        let _parentctx = recog.ctx.take();
        let mut _localctx =
            OC_CastTypeFieldContextExt::new(_parentctx.clone(), recog.base.get_state());
        recog
            .base
            .enter_rule(_localctx.clone(), 168, RULE_oC_CastTypeField);
        let mut _localctx: Rc<OC_CastTypeFieldContextAll> = _localctx;
        let result: Result<(), ANTLRError> = (|| {
            //recog.base.enter_outer_alt(_localctx.clone(), 1)?;
            recog.base.enter_outer_alt(None, 1)?;
            {
                /*InvokeRule oC_CastTypeName*/
                recog.base.set_state(1523);
                recog.oC_CastTypeName()?;

                recog.base.set_state(1524);
                recog.base.match_token(Cypher_SP, &mut recog.err_handler)?;

                /*InvokeRule oC_CastType*/
                recog.base.set_state(1525);
                recog.oC_CastType()?;
            }
            Ok(())
        })();
        match result {
            Ok(_) => {}
            Err(e @ ANTLRError::FallThrough(_)) => return Err(e),
            Err(ref re) => {
                //_localctx.exception = re;
                recog.err_handler.report_error(&mut recog.base, re);
                recog.err_handler.recover(&mut recog.base, re)?;
            }
        }
        recog.base.exit_rule()?;

        Ok(_localctx)
    }
}
//------------------- oC_CastTypeName ----------------
pub type OC_CastTypeNameContextAll<'input> = OC_CastTypeNameContext<'input>;

pub type OC_CastTypeNameContext<'input> =
    BaseParserRuleContext<'input, OC_CastTypeNameContextExt<'input>>;

#[derive(Clone)]
pub struct OC_CastTypeNameContextExt<'input> {
    ph: PhantomData<&'input str>,
}

impl<'input> CypherParserContext<'input> for OC_CastTypeNameContext<'input> {}

impl<'input, 'a> Listenable<dyn CypherListener<'input> + 'a> for OC_CastTypeNameContext<'input> {
    fn enter(&self, listener: &mut (dyn CypherListener<'input> + 'a)) -> Result<(), ANTLRError> {
        listener.enter_every_rule(self)?;
        listener.enter_oC_CastTypeName(self);
        Ok(())
    }
    fn exit(&self, listener: &mut (dyn CypherListener<'input> + 'a)) -> Result<(), ANTLRError> {
        listener.exit_oC_CastTypeName(self);
        listener.exit_every_rule(self)?;
        Ok(())
    }
}

impl<'input, 'a> Visitable<dyn CypherVisitor<'input> + 'a> for OC_CastTypeNameContext<'input> {
    fn accept(&self, visitor: &mut (dyn CypherVisitor<'input> + 'a)) {
        visitor.visit_oC_CastTypeName(self);
    }
}

impl<'input> CustomRuleContext<'input> for OC_CastTypeNameContextExt<'input> {
    type TF = LocalTokenFactory<'input>;
    type Ctx = CypherParserContextType;
    fn get_rule_index(&self) -> usize {
        RULE_oC_CastTypeName
    }
    //fn type_rule_index() -> usize where Self: Sized { RULE_oC_CastTypeName }
}
antlr4rust::tid! {OC_CastTypeNameContextExt<'a>}

impl<'input> OC_CastTypeNameContextExt<'input> {
    fn new(
        parent: Option<Rc<dyn CypherParserContext<'input> + 'input>>,
        invoking_state: i32,
    ) -> Rc<OC_CastTypeNameContextAll<'input>> {
        Rc::new(BaseParserRuleContext::new_parser_ctx(
            parent,
            invoking_state,
            OC_CastTypeNameContextExt { ph: PhantomData },
        ))
    }
}

pub trait OC_CastTypeNameContextAttrs<'input>:
    CypherParserContext<'input> + BorrowMut<OC_CastTypeNameContextExt<'input>>
{
    fn oC_SchemaName(&self) -> Option<Rc<OC_SchemaNameContextAll<'input>>>
    where
        Self: Sized,
    {
        self.child_of_type(0)
    }
}

impl<'input> OC_CastTypeNameContextAttrs<'input> for OC_CastTypeNameContext<'input> {}

impl<'input, I> CypherParser<'input, I>
where
    I: TokenStream<'input, TF = LocalTokenFactory<'input>> + TidAble<'input>,
{
    pub fn oC_CastTypeName(&mut self) -> Result<Rc<OC_CastTypeNameContextAll<'input>>, ANTLRError> {
        let mut recog = self;
        let _parentctx = recog.ctx.take();
        let mut _localctx =
            OC_CastTypeNameContextExt::new(_parentctx.clone(), recog.base.get_state());
        recog
            .base
            .enter_rule(_localctx.clone(), 170, RULE_oC_CastTypeName);
        let mut _localctx: Rc<OC_CastTypeNameContextAll> = _localctx;
        let result: Result<(), ANTLRError> = (|| {
            //recog.base.enter_outer_alt(_localctx.clone(), 1)?;
            recog.base.enter_outer_alt(None, 1)?;
            {
                /*InvokeRule oC_SchemaName*/
                recog.base.set_state(1527);
                recog.oC_SchemaName()?;
            }
            Ok(())
        })();
        match result {
            Ok(_) => {}
            Err(e @ ANTLRError::FallThrough(_)) => return Err(e),
            Err(ref re) => {
                //_localctx.exception = re;
                recog.err_handler.report_error(&mut recog.base, re);
                recog.err_handler.recover(&mut recog.base, re)?;
            }
        }
        recog.base.exit_rule()?;

        Ok(_localctx)
    }
}
//------------------- oC_FunctionName ----------------
pub type OC_FunctionNameContextAll<'input> = OC_FunctionNameContext<'input>;

pub type OC_FunctionNameContext<'input> =
    BaseParserRuleContext<'input, OC_FunctionNameContextExt<'input>>;

#[derive(Clone)]
pub struct OC_FunctionNameContextExt<'input> {
    ph: PhantomData<&'input str>,
}

impl<'input> CypherParserContext<'input> for OC_FunctionNameContext<'input> {}

impl<'input, 'a> Listenable<dyn CypherListener<'input> + 'a> for OC_FunctionNameContext<'input> {
    fn enter(&self, listener: &mut (dyn CypherListener<'input> + 'a)) -> Result<(), ANTLRError> {
        listener.enter_every_rule(self)?;
        listener.enter_oC_FunctionName(self);
        Ok(())
    }
    fn exit(&self, listener: &mut (dyn CypherListener<'input> + 'a)) -> Result<(), ANTLRError> {
        listener.exit_oC_FunctionName(self);
        listener.exit_every_rule(self)?;
        Ok(())
    }
}

impl<'input, 'a> Visitable<dyn CypherVisitor<'input> + 'a> for OC_FunctionNameContext<'input> {
    fn accept(&self, visitor: &mut (dyn CypherVisitor<'input> + 'a)) {
        visitor.visit_oC_FunctionName(self);
    }
}

impl<'input> CustomRuleContext<'input> for OC_FunctionNameContextExt<'input> {
    type TF = LocalTokenFactory<'input>;
    type Ctx = CypherParserContextType;
    fn get_rule_index(&self) -> usize {
        RULE_oC_FunctionName
    }
    //fn type_rule_index() -> usize where Self: Sized { RULE_oC_FunctionName }
}
antlr4rust::tid! {OC_FunctionNameContextExt<'a>}

impl<'input> OC_FunctionNameContextExt<'input> {
    fn new(
        parent: Option<Rc<dyn CypherParserContext<'input> + 'input>>,
        invoking_state: i32,
    ) -> Rc<OC_FunctionNameContextAll<'input>> {
        Rc::new(BaseParserRuleContext::new_parser_ctx(
            parent,
            invoking_state,
            OC_FunctionNameContextExt { ph: PhantomData },
        ))
    }
}

pub trait OC_FunctionNameContextAttrs<'input>:
    CypherParserContext<'input> + BorrowMut<OC_FunctionNameContextExt<'input>>
{
    fn oC_Namespace(&self) -> Option<Rc<OC_NamespaceContextAll<'input>>>
    where
        Self: Sized,
    {
        self.child_of_type(0)
    }
    fn oC_SymbolicName(&self) -> Option<Rc<OC_SymbolicNameContextAll<'input>>>
    where
        Self: Sized,
    {
        self.child_of_type(0)
    }
}

impl<'input> OC_FunctionNameContextAttrs<'input> for OC_FunctionNameContext<'input> {}

impl<'input, I> CypherParser<'input, I>
where
    I: TokenStream<'input, TF = LocalTokenFactory<'input>> + TidAble<'input>,
{
    pub fn oC_FunctionName(&mut self) -> Result<Rc<OC_FunctionNameContextAll<'input>>, ANTLRError> {
        let mut recog = self;
        let _parentctx = recog.ctx.take();
        let mut _localctx =
            OC_FunctionNameContextExt::new(_parentctx.clone(), recog.base.get_state());
        recog
            .base
            .enter_rule(_localctx.clone(), 172, RULE_oC_FunctionName);
        let mut _localctx: Rc<OC_FunctionNameContextAll> = _localctx;
        let result: Result<(), ANTLRError> = (|| {
            //recog.base.enter_outer_alt(_localctx.clone(), 1)?;
            recog.base.enter_outer_alt(None, 1)?;
            {
                /*InvokeRule oC_Namespace*/
                recog.base.set_state(1529);
                recog.oC_Namespace()?;

                /*InvokeRule oC_SymbolicName*/
                recog.base.set_state(1530);
                recog.oC_SymbolicName()?;
            }
            Ok(())
        })();
        match result {
            Ok(_) => {}
            Err(e @ ANTLRError::FallThrough(_)) => return Err(e),
            Err(ref re) => {
                //_localctx.exception = re;
                recog.err_handler.report_error(&mut recog.base, re);
                recog.err_handler.recover(&mut recog.base, re)?;
            }
        }
        recog.base.exit_rule()?;

        Ok(_localctx)
    }
}
//------------------- oC_ExistentialSubquery ----------------
pub type OC_ExistentialSubqueryContextAll<'input> = OC_ExistentialSubqueryContext<'input>;

pub type OC_ExistentialSubqueryContext<'input> =
    BaseParserRuleContext<'input, OC_ExistentialSubqueryContextExt<'input>>;

#[derive(Clone)]
pub struct OC_ExistentialSubqueryContextExt<'input> {
    ph: PhantomData<&'input str>,
}

impl<'input> CypherParserContext<'input> for OC_ExistentialSubqueryContext<'input> {}

impl<'input, 'a> Listenable<dyn CypherListener<'input> + 'a>
    for OC_ExistentialSubqueryContext<'input>
{
    fn enter(&self, listener: &mut (dyn CypherListener<'input> + 'a)) -> Result<(), ANTLRError> {
        listener.enter_every_rule(self)?;
        listener.enter_oC_ExistentialSubquery(self);
        Ok(())
    }
    fn exit(&self, listener: &mut (dyn CypherListener<'input> + 'a)) -> Result<(), ANTLRError> {
        listener.exit_oC_ExistentialSubquery(self);
        listener.exit_every_rule(self)?;
        Ok(())
    }
}

impl<'input, 'a> Visitable<dyn CypherVisitor<'input> + 'a>
    for OC_ExistentialSubqueryContext<'input>
{
    fn accept(&self, visitor: &mut (dyn CypherVisitor<'input> + 'a)) {
        visitor.visit_oC_ExistentialSubquery(self);
    }
}

impl<'input> CustomRuleContext<'input> for OC_ExistentialSubqueryContextExt<'input> {
    type TF = LocalTokenFactory<'input>;
    type Ctx = CypherParserContextType;
    fn get_rule_index(&self) -> usize {
        RULE_oC_ExistentialSubquery
    }
    //fn type_rule_index() -> usize where Self: Sized { RULE_oC_ExistentialSubquery }
}
antlr4rust::tid! {OC_ExistentialSubqueryContextExt<'a>}

impl<'input> OC_ExistentialSubqueryContextExt<'input> {
    fn new(
        parent: Option<Rc<dyn CypherParserContext<'input> + 'input>>,
        invoking_state: i32,
    ) -> Rc<OC_ExistentialSubqueryContextAll<'input>> {
        Rc::new(BaseParserRuleContext::new_parser_ctx(
            parent,
            invoking_state,
            OC_ExistentialSubqueryContextExt { ph: PhantomData },
        ))
    }
}

pub trait OC_ExistentialSubqueryContextAttrs<'input>:
    CypherParserContext<'input> + BorrowMut<OC_ExistentialSubqueryContextExt<'input>>
{
    /// Retrieves first TerminalNode corresponding to token EXISTS
    /// Returns `None` if there is no child corresponding to token EXISTS
    fn EXISTS(&self) -> Option<Rc<TerminalNode<'input, CypherParserContextType>>>
    where
        Self: Sized,
    {
        self.get_token(Cypher_EXISTS, 0)
    }
    fn oC_RegularQuery(&self) -> Option<Rc<OC_RegularQueryContextAll<'input>>>
    where
        Self: Sized,
    {
        self.child_of_type(0)
    }
    /// Retrieves all `TerminalNode`s corresponding to token SP in current rule
    fn SP_all(&self) -> Vec<Rc<TerminalNode<'input, CypherParserContextType>>>
    where
        Self: Sized,
    {
        self.children_of_type()
    }
    /// Retrieves 'i's TerminalNode corresponding to token SP, starting from 0.
    /// Returns `None` if number of children corresponding to token SP is less or equal than `i`.
    fn SP(&self, i: usize) -> Option<Rc<TerminalNode<'input, CypherParserContextType>>>
    where
        Self: Sized,
    {
        self.get_token(Cypher_SP, i)
    }
    /// Retrieves first TerminalNode corresponding to token MATCH
    /// Returns `None` if there is no child corresponding to token MATCH
    fn MATCH(&self) -> Option<Rc<TerminalNode<'input, CypherParserContextType>>>
    where
        Self: Sized,
    {
        self.get_token(Cypher_MATCH, 0)
    }
    fn oC_Pattern(&self) -> Option<Rc<OC_PatternContextAll<'input>>>
    where
        Self: Sized,
    {
        self.child_of_type(0)
    }
    fn oC_Where(&self) -> Option<Rc<OC_WhereContextAll<'input>>>
    where
        Self: Sized,
    {
        self.child_of_type(0)
    }
}

impl<'input> OC_ExistentialSubqueryContextAttrs<'input> for OC_ExistentialSubqueryContext<'input> {}

impl<'input, I> CypherParser<'input, I>
where
    I: TokenStream<'input, TF = LocalTokenFactory<'input>> + TidAble<'input>,
{
    pub fn oC_ExistentialSubquery(
        &mut self,
    ) -> Result<Rc<OC_ExistentialSubqueryContextAll<'input>>, ANTLRError> {
        let mut recog = self;
        let _parentctx = recog.ctx.take();
        let mut _localctx =
            OC_ExistentialSubqueryContextExt::new(_parentctx.clone(), recog.base.get_state());
        recog
            .base
            .enter_rule(_localctx.clone(), 174, RULE_oC_ExistentialSubquery);
        let mut _localctx: Rc<OC_ExistentialSubqueryContextAll> = _localctx;
        let mut _la: i32 = -1;
        let result: Result<(), ANTLRError> = (|| {
            //recog.base.enter_outer_alt(_localctx.clone(), 1)?;
            recog.base.enter_outer_alt(None, 1)?;
            {
                recog.base.set_state(1532);
                recog
                    .base
                    .match_token(Cypher_EXISTS, &mut recog.err_handler)?;

                recog.base.set_state(1534);
                recog.err_handler.sync(&mut recog.base)?;
                _la = recog.base.input.la(1);
                if _la == Cypher_SP {
                    {
                        recog.base.set_state(1533);
                        recog.base.match_token(Cypher_SP, &mut recog.err_handler)?;
                    }
                }

                recog.base.set_state(1536);
                recog
                    .base
                    .match_token(Cypher_T__23, &mut recog.err_handler)?;

                recog.base.set_state(1538);
                recog.err_handler.sync(&mut recog.base)?;
                _la = recog.base.input.la(1);
                if _la == Cypher_SP {
                    {
                        recog.base.set_state(1537);
                        recog.base.match_token(Cypher_SP, &mut recog.err_handler)?;
                    }
                }

                recog.base.set_state(1559);
                recog.err_handler.sync(&mut recog.base)?;
                match recog.interpreter.adaptive_predict(292, &mut recog.base)? {
                    1 => {
                        {
                            {
                                recog.base.set_state(1540);
                                recog
                                    .base
                                    .match_token(Cypher_MATCH, &mut recog.err_handler)?;

                                recog.base.set_state(1542);
                                recog.err_handler.sync(&mut recog.base)?;
                                _la = recog.base.input.la(1);
                                if _la == Cypher_SP {
                                    {
                                        recog.base.set_state(1541);
                                        recog
                                            .base
                                            .match_token(Cypher_SP, &mut recog.err_handler)?;
                                    }
                                }

                                /*InvokeRule oC_Pattern*/
                                recog.base.set_state(1544);
                                recog.oC_Pattern()?;

                                recog.base.set_state(1549);
                                recog.err_handler.sync(&mut recog.base)?;
                                match recog.interpreter.adaptive_predict(289, &mut recog.base)? {
                                    x if x == 1 => {
                                        {
                                            recog.base.set_state(1546);
                                            recog.err_handler.sync(&mut recog.base)?;
                                            _la = recog.base.input.la(1);
                                            if _la == Cypher_SP {
                                                {
                                                    recog.base.set_state(1545);
                                                    recog.base.match_token(
                                                        Cypher_SP,
                                                        &mut recog.err_handler,
                                                    )?;
                                                }
                                            }

                                            /*InvokeRule oC_Where*/
                                            recog.base.set_state(1548);
                                            recog.oC_Where()?;
                                        }
                                    }

                                    _ => {}
                                }
                            }
                        }
                    }
                    2 => {
                        {
                            /*InvokeRule oC_RegularQuery*/
                            recog.base.set_state(1551);
                            recog.oC_RegularQuery()?;
                        }
                    }
                    3 => {
                        {
                            {
                                /*InvokeRule oC_Pattern*/
                                recog.base.set_state(1552);
                                recog.oC_Pattern()?;

                                recog.base.set_state(1557);
                                recog.err_handler.sync(&mut recog.base)?;
                                match recog.interpreter.adaptive_predict(291, &mut recog.base)? {
                                    x if x == 1 => {
                                        {
                                            recog.base.set_state(1554);
                                            recog.err_handler.sync(&mut recog.base)?;
                                            _la = recog.base.input.la(1);
                                            if _la == Cypher_SP {
                                                {
                                                    recog.base.set_state(1553);
                                                    recog.base.match_token(
                                                        Cypher_SP,
                                                        &mut recog.err_handler,
                                                    )?;
                                                }
                                            }

                                            /*InvokeRule oC_Where*/
                                            recog.base.set_state(1556);
                                            recog.oC_Where()?;
                                        }
                                    }

                                    _ => {}
                                }
                            }
                        }
                    }

                    _ => {}
                }
                recog.base.set_state(1562);
                recog.err_handler.sync(&mut recog.base)?;
                _la = recog.base.input.la(1);
                if _la == Cypher_SP {
                    {
                        recog.base.set_state(1561);
                        recog.base.match_token(Cypher_SP, &mut recog.err_handler)?;
                    }
                }

                recog.base.set_state(1564);
                recog
                    .base
                    .match_token(Cypher_T__24, &mut recog.err_handler)?;
            }
            Ok(())
        })();
        match result {
            Ok(_) => {}
            Err(e @ ANTLRError::FallThrough(_)) => return Err(e),
            Err(ref re) => {
                //_localctx.exception = re;
                recog.err_handler.report_error(&mut recog.base, re);
                recog.err_handler.recover(&mut recog.base, re)?;
            }
        }
        recog.base.exit_rule()?;

        Ok(_localctx)
    }
}
//------------------- oC_ExplicitProcedureInvocation ----------------
pub type OC_ExplicitProcedureInvocationContextAll<'input> =
    OC_ExplicitProcedureInvocationContext<'input>;

pub type OC_ExplicitProcedureInvocationContext<'input> =
    BaseParserRuleContext<'input, OC_ExplicitProcedureInvocationContextExt<'input>>;

#[derive(Clone)]
pub struct OC_ExplicitProcedureInvocationContextExt<'input> {
    ph: PhantomData<&'input str>,
}

impl<'input> CypherParserContext<'input> for OC_ExplicitProcedureInvocationContext<'input> {}

impl<'input, 'a> Listenable<dyn CypherListener<'input> + 'a>
    for OC_ExplicitProcedureInvocationContext<'input>
{
    fn enter(&self, listener: &mut (dyn CypherListener<'input> + 'a)) -> Result<(), ANTLRError> {
        listener.enter_every_rule(self)?;
        listener.enter_oC_ExplicitProcedureInvocation(self);
        Ok(())
    }
    fn exit(&self, listener: &mut (dyn CypherListener<'input> + 'a)) -> Result<(), ANTLRError> {
        listener.exit_oC_ExplicitProcedureInvocation(self);
        listener.exit_every_rule(self)?;
        Ok(())
    }
}

impl<'input, 'a> Visitable<dyn CypherVisitor<'input> + 'a>
    for OC_ExplicitProcedureInvocationContext<'input>
{
    fn accept(&self, visitor: &mut (dyn CypherVisitor<'input> + 'a)) {
        visitor.visit_oC_ExplicitProcedureInvocation(self);
    }
}

impl<'input> CustomRuleContext<'input> for OC_ExplicitProcedureInvocationContextExt<'input> {
    type TF = LocalTokenFactory<'input>;
    type Ctx = CypherParserContextType;
    fn get_rule_index(&self) -> usize {
        RULE_oC_ExplicitProcedureInvocation
    }
    //fn type_rule_index() -> usize where Self: Sized { RULE_oC_ExplicitProcedureInvocation }
}
antlr4rust::tid! {OC_ExplicitProcedureInvocationContextExt<'a>}

impl<'input> OC_ExplicitProcedureInvocationContextExt<'input> {
    fn new(
        parent: Option<Rc<dyn CypherParserContext<'input> + 'input>>,
        invoking_state: i32,
    ) -> Rc<OC_ExplicitProcedureInvocationContextAll<'input>> {
        Rc::new(BaseParserRuleContext::new_parser_ctx(
            parent,
            invoking_state,
            OC_ExplicitProcedureInvocationContextExt { ph: PhantomData },
        ))
    }
}

pub trait OC_ExplicitProcedureInvocationContextAttrs<'input>:
    CypherParserContext<'input> + BorrowMut<OC_ExplicitProcedureInvocationContextExt<'input>>
{
    fn oC_ProcedureName(&self) -> Option<Rc<OC_ProcedureNameContextAll<'input>>>
    where
        Self: Sized,
    {
        self.child_of_type(0)
    }
    /// Retrieves all `TerminalNode`s corresponding to token SP in current rule
    fn SP_all(&self) -> Vec<Rc<TerminalNode<'input, CypherParserContextType>>>
    where
        Self: Sized,
    {
        self.children_of_type()
    }
    /// Retrieves 'i's TerminalNode corresponding to token SP, starting from 0.
    /// Returns `None` if number of children corresponding to token SP is less or equal than `i`.
    fn SP(&self, i: usize) -> Option<Rc<TerminalNode<'input, CypherParserContextType>>>
    where
        Self: Sized,
    {
        self.get_token(Cypher_SP, i)
    }
    fn oC_Expression_all(&self) -> Vec<Rc<OC_ExpressionContextAll<'input>>>
    where
        Self: Sized,
    {
        self.children_of_type()
    }
    fn oC_Expression(&self, i: usize) -> Option<Rc<OC_ExpressionContextAll<'input>>>
    where
        Self: Sized,
    {
        self.child_of_type(i)
    }
}

impl<'input> OC_ExplicitProcedureInvocationContextAttrs<'input>
    for OC_ExplicitProcedureInvocationContext<'input>
{
}

impl<'input, I> CypherParser<'input, I>
where
    I: TokenStream<'input, TF = LocalTokenFactory<'input>> + TidAble<'input>,
{
    pub fn oC_ExplicitProcedureInvocation(
        &mut self,
    ) -> Result<Rc<OC_ExplicitProcedureInvocationContextAll<'input>>, ANTLRError> {
        let mut recog = self;
        let _parentctx = recog.ctx.take();
        let mut _localctx = OC_ExplicitProcedureInvocationContextExt::new(
            _parentctx.clone(),
            recog.base.get_state(),
        );
        recog
            .base
            .enter_rule(_localctx.clone(), 176, RULE_oC_ExplicitProcedureInvocation);
        let mut _localctx: Rc<OC_ExplicitProcedureInvocationContextAll> = _localctx;
        let mut _la: i32 = -1;
        let result: Result<(), ANTLRError> = (|| {
            //recog.base.enter_outer_alt(_localctx.clone(), 1)?;
            recog.base.enter_outer_alt(None, 1)?;
            {
                /*InvokeRule oC_ProcedureName*/
                recog.base.set_state(1566);
                recog.oC_ProcedureName()?;

                recog.base.set_state(1568);
                recog.err_handler.sync(&mut recog.base)?;
                _la = recog.base.input.la(1);
                if _la == Cypher_SP {
                    {
                        recog.base.set_state(1567);
                        recog.base.match_token(Cypher_SP, &mut recog.err_handler)?;
                    }
                }

                recog.base.set_state(1570);
                recog
                    .base
                    .match_token(Cypher_T__5, &mut recog.err_handler)?;

                recog.base.set_state(1572);
                recog.err_handler.sync(&mut recog.base)?;
                _la = recog.base.input.la(1);
                if _la == Cypher_SP {
                    {
                        recog.base.set_state(1571);
                        recog.base.match_token(Cypher_SP, &mut recog.err_handler)?;
                    }
                }

                recog.base.set_state(1591);
                recog.err_handler.sync(&mut recog.base)?;
                _la = recog.base.input.la(1);
                if (((_la) & !0x3f) == 0 && ((1usize << _la) & 84672832) != 0)
                    || _la == Cypher_ALL
                    || _la == Cypher_NOT
                    || (((_la - 82) & !0x3f) == 0 && ((1usize << (_la - 82)) & 117702535) != 0)
                    || (((_la - 120) & !0x3f) == 0 && ((1usize << (_la - 120)) & 39) != 0)
                {
                    {
                        /*InvokeRule oC_Expression*/
                        recog.base.set_state(1574);
                        recog.oC_Expression()?;

                        recog.base.set_state(1576);
                        recog.err_handler.sync(&mut recog.base)?;
                        _la = recog.base.input.la(1);
                        if _la == Cypher_SP {
                            {
                                recog.base.set_state(1575);
                                recog.base.match_token(Cypher_SP, &mut recog.err_handler)?;
                            }
                        }

                        recog.base.set_state(1588);
                        recog.err_handler.sync(&mut recog.base)?;
                        _la = recog.base.input.la(1);
                        while _la == Cypher_T__1 {
                            {
                                {
                                    recog.base.set_state(1578);
                                    recog
                                        .base
                                        .match_token(Cypher_T__1, &mut recog.err_handler)?;

                                    recog.base.set_state(1580);
                                    recog.err_handler.sync(&mut recog.base)?;
                                    _la = recog.base.input.la(1);
                                    if _la == Cypher_SP {
                                        {
                                            recog.base.set_state(1579);
                                            recog
                                                .base
                                                .match_token(Cypher_SP, &mut recog.err_handler)?;
                                        }
                                    }

                                    /*InvokeRule oC_Expression*/
                                    recog.base.set_state(1582);
                                    recog.oC_Expression()?;

                                    recog.base.set_state(1584);
                                    recog.err_handler.sync(&mut recog.base)?;
                                    _la = recog.base.input.la(1);
                                    if _la == Cypher_SP {
                                        {
                                            recog.base.set_state(1583);
                                            recog
                                                .base
                                                .match_token(Cypher_SP, &mut recog.err_handler)?;
                                        }
                                    }
                                }
                            }
                            recog.base.set_state(1590);
                            recog.err_handler.sync(&mut recog.base)?;
                            _la = recog.base.input.la(1);
                        }
                    }
                }

                recog.base.set_state(1593);
                recog
                    .base
                    .match_token(Cypher_T__6, &mut recog.err_handler)?;
            }
            Ok(())
        })();
        match result {
            Ok(_) => {}
            Err(e @ ANTLRError::FallThrough(_)) => return Err(e),
            Err(ref re) => {
                //_localctx.exception = re;
                recog.err_handler.report_error(&mut recog.base, re);
                recog.err_handler.recover(&mut recog.base, re)?;
            }
        }
        recog.base.exit_rule()?;

        Ok(_localctx)
    }
}
//------------------- oC_ImplicitProcedureInvocation ----------------
pub type OC_ImplicitProcedureInvocationContextAll<'input> =
    OC_ImplicitProcedureInvocationContext<'input>;

pub type OC_ImplicitProcedureInvocationContext<'input> =
    BaseParserRuleContext<'input, OC_ImplicitProcedureInvocationContextExt<'input>>;

#[derive(Clone)]
pub struct OC_ImplicitProcedureInvocationContextExt<'input> {
    ph: PhantomData<&'input str>,
}

impl<'input> CypherParserContext<'input> for OC_ImplicitProcedureInvocationContext<'input> {}

impl<'input, 'a> Listenable<dyn CypherListener<'input> + 'a>
    for OC_ImplicitProcedureInvocationContext<'input>
{
    fn enter(&self, listener: &mut (dyn CypherListener<'input> + 'a)) -> Result<(), ANTLRError> {
        listener.enter_every_rule(self)?;
        listener.enter_oC_ImplicitProcedureInvocation(self);
        Ok(())
    }
    fn exit(&self, listener: &mut (dyn CypherListener<'input> + 'a)) -> Result<(), ANTLRError> {
        listener.exit_oC_ImplicitProcedureInvocation(self);
        listener.exit_every_rule(self)?;
        Ok(())
    }
}

impl<'input, 'a> Visitable<dyn CypherVisitor<'input> + 'a>
    for OC_ImplicitProcedureInvocationContext<'input>
{
    fn accept(&self, visitor: &mut (dyn CypherVisitor<'input> + 'a)) {
        visitor.visit_oC_ImplicitProcedureInvocation(self);
    }
}

impl<'input> CustomRuleContext<'input> for OC_ImplicitProcedureInvocationContextExt<'input> {
    type TF = LocalTokenFactory<'input>;
    type Ctx = CypherParserContextType;
    fn get_rule_index(&self) -> usize {
        RULE_oC_ImplicitProcedureInvocation
    }
    //fn type_rule_index() -> usize where Self: Sized { RULE_oC_ImplicitProcedureInvocation }
}
antlr4rust::tid! {OC_ImplicitProcedureInvocationContextExt<'a>}

impl<'input> OC_ImplicitProcedureInvocationContextExt<'input> {
    fn new(
        parent: Option<Rc<dyn CypherParserContext<'input> + 'input>>,
        invoking_state: i32,
    ) -> Rc<OC_ImplicitProcedureInvocationContextAll<'input>> {
        Rc::new(BaseParserRuleContext::new_parser_ctx(
            parent,
            invoking_state,
            OC_ImplicitProcedureInvocationContextExt { ph: PhantomData },
        ))
    }
}

pub trait OC_ImplicitProcedureInvocationContextAttrs<'input>:
    CypherParserContext<'input> + BorrowMut<OC_ImplicitProcedureInvocationContextExt<'input>>
{
    fn oC_ProcedureName(&self) -> Option<Rc<OC_ProcedureNameContextAll<'input>>>
    where
        Self: Sized,
    {
        self.child_of_type(0)
    }
}

impl<'input> OC_ImplicitProcedureInvocationContextAttrs<'input>
    for OC_ImplicitProcedureInvocationContext<'input>
{
}

impl<'input, I> CypherParser<'input, I>
where
    I: TokenStream<'input, TF = LocalTokenFactory<'input>> + TidAble<'input>,
{
    pub fn oC_ImplicitProcedureInvocation(
        &mut self,
    ) -> Result<Rc<OC_ImplicitProcedureInvocationContextAll<'input>>, ANTLRError> {
        let mut recog = self;
        let _parentctx = recog.ctx.take();
        let mut _localctx = OC_ImplicitProcedureInvocationContextExt::new(
            _parentctx.clone(),
            recog.base.get_state(),
        );
        recog
            .base
            .enter_rule(_localctx.clone(), 178, RULE_oC_ImplicitProcedureInvocation);
        let mut _localctx: Rc<OC_ImplicitProcedureInvocationContextAll> = _localctx;
        let result: Result<(), ANTLRError> = (|| {
            //recog.base.enter_outer_alt(_localctx.clone(), 1)?;
            recog.base.enter_outer_alt(None, 1)?;
            {
                /*InvokeRule oC_ProcedureName*/
                recog.base.set_state(1595);
                recog.oC_ProcedureName()?;
            }
            Ok(())
        })();
        match result {
            Ok(_) => {}
            Err(e @ ANTLRError::FallThrough(_)) => return Err(e),
            Err(ref re) => {
                //_localctx.exception = re;
                recog.err_handler.report_error(&mut recog.base, re);
                recog.err_handler.recover(&mut recog.base, re)?;
            }
        }
        recog.base.exit_rule()?;

        Ok(_localctx)
    }
}
//------------------- oC_ProcedureResultField ----------------
pub type OC_ProcedureResultFieldContextAll<'input> = OC_ProcedureResultFieldContext<'input>;

pub type OC_ProcedureResultFieldContext<'input> =
    BaseParserRuleContext<'input, OC_ProcedureResultFieldContextExt<'input>>;

#[derive(Clone)]
pub struct OC_ProcedureResultFieldContextExt<'input> {
    ph: PhantomData<&'input str>,
}

impl<'input> CypherParserContext<'input> for OC_ProcedureResultFieldContext<'input> {}

impl<'input, 'a> Listenable<dyn CypherListener<'input> + 'a>
    for OC_ProcedureResultFieldContext<'input>
{
    fn enter(&self, listener: &mut (dyn CypherListener<'input> + 'a)) -> Result<(), ANTLRError> {
        listener.enter_every_rule(self)?;
        listener.enter_oC_ProcedureResultField(self);
        Ok(())
    }
    fn exit(&self, listener: &mut (dyn CypherListener<'input> + 'a)) -> Result<(), ANTLRError> {
        listener.exit_oC_ProcedureResultField(self);
        listener.exit_every_rule(self)?;
        Ok(())
    }
}

impl<'input, 'a> Visitable<dyn CypherVisitor<'input> + 'a>
    for OC_ProcedureResultFieldContext<'input>
{
    fn accept(&self, visitor: &mut (dyn CypherVisitor<'input> + 'a)) {
        visitor.visit_oC_ProcedureResultField(self);
    }
}

impl<'input> CustomRuleContext<'input> for OC_ProcedureResultFieldContextExt<'input> {
    type TF = LocalTokenFactory<'input>;
    type Ctx = CypherParserContextType;
    fn get_rule_index(&self) -> usize {
        RULE_oC_ProcedureResultField
    }
    //fn type_rule_index() -> usize where Self: Sized { RULE_oC_ProcedureResultField }
}
antlr4rust::tid! {OC_ProcedureResultFieldContextExt<'a>}

impl<'input> OC_ProcedureResultFieldContextExt<'input> {
    fn new(
        parent: Option<Rc<dyn CypherParserContext<'input> + 'input>>,
        invoking_state: i32,
    ) -> Rc<OC_ProcedureResultFieldContextAll<'input>> {
        Rc::new(BaseParserRuleContext::new_parser_ctx(
            parent,
            invoking_state,
            OC_ProcedureResultFieldContextExt { ph: PhantomData },
        ))
    }
}

pub trait OC_ProcedureResultFieldContextAttrs<'input>:
    CypherParserContext<'input> + BorrowMut<OC_ProcedureResultFieldContextExt<'input>>
{
    fn oC_SymbolicName(&self) -> Option<Rc<OC_SymbolicNameContextAll<'input>>>
    where
        Self: Sized,
    {
        self.child_of_type(0)
    }
}

impl<'input> OC_ProcedureResultFieldContextAttrs<'input>
    for OC_ProcedureResultFieldContext<'input>
{
}

impl<'input, I> CypherParser<'input, I>
where
    I: TokenStream<'input, TF = LocalTokenFactory<'input>> + TidAble<'input>,
{
    pub fn oC_ProcedureResultField(
        &mut self,
    ) -> Result<Rc<OC_ProcedureResultFieldContextAll<'input>>, ANTLRError> {
        let mut recog = self;
        let _parentctx = recog.ctx.take();
        let mut _localctx =
            OC_ProcedureResultFieldContextExt::new(_parentctx.clone(), recog.base.get_state());
        recog
            .base
            .enter_rule(_localctx.clone(), 180, RULE_oC_ProcedureResultField);
        let mut _localctx: Rc<OC_ProcedureResultFieldContextAll> = _localctx;
        let result: Result<(), ANTLRError> = (|| {
            //recog.base.enter_outer_alt(_localctx.clone(), 1)?;
            recog.base.enter_outer_alt(None, 1)?;
            {
                /*InvokeRule oC_SymbolicName*/
                recog.base.set_state(1597);
                recog.oC_SymbolicName()?;
            }
            Ok(())
        })();
        match result {
            Ok(_) => {}
            Err(e @ ANTLRError::FallThrough(_)) => return Err(e),
            Err(ref re) => {
                //_localctx.exception = re;
                recog.err_handler.report_error(&mut recog.base, re);
                recog.err_handler.recover(&mut recog.base, re)?;
            }
        }
        recog.base.exit_rule()?;

        Ok(_localctx)
    }
}
//------------------- oC_ProcedureName ----------------
pub type OC_ProcedureNameContextAll<'input> = OC_ProcedureNameContext<'input>;

pub type OC_ProcedureNameContext<'input> =
    BaseParserRuleContext<'input, OC_ProcedureNameContextExt<'input>>;

#[derive(Clone)]
pub struct OC_ProcedureNameContextExt<'input> {
    ph: PhantomData<&'input str>,
}

impl<'input> CypherParserContext<'input> for OC_ProcedureNameContext<'input> {}

impl<'input, 'a> Listenable<dyn CypherListener<'input> + 'a> for OC_ProcedureNameContext<'input> {
    fn enter(&self, listener: &mut (dyn CypherListener<'input> + 'a)) -> Result<(), ANTLRError> {
        listener.enter_every_rule(self)?;
        listener.enter_oC_ProcedureName(self);
        Ok(())
    }
    fn exit(&self, listener: &mut (dyn CypherListener<'input> + 'a)) -> Result<(), ANTLRError> {
        listener.exit_oC_ProcedureName(self);
        listener.exit_every_rule(self)?;
        Ok(())
    }
}

impl<'input, 'a> Visitable<dyn CypherVisitor<'input> + 'a> for OC_ProcedureNameContext<'input> {
    fn accept(&self, visitor: &mut (dyn CypherVisitor<'input> + 'a)) {
        visitor.visit_oC_ProcedureName(self);
    }
}

impl<'input> CustomRuleContext<'input> for OC_ProcedureNameContextExt<'input> {
    type TF = LocalTokenFactory<'input>;
    type Ctx = CypherParserContextType;
    fn get_rule_index(&self) -> usize {
        RULE_oC_ProcedureName
    }
    //fn type_rule_index() -> usize where Self: Sized { RULE_oC_ProcedureName }
}
antlr4rust::tid! {OC_ProcedureNameContextExt<'a>}

impl<'input> OC_ProcedureNameContextExt<'input> {
    fn new(
        parent: Option<Rc<dyn CypherParserContext<'input> + 'input>>,
        invoking_state: i32,
    ) -> Rc<OC_ProcedureNameContextAll<'input>> {
        Rc::new(BaseParserRuleContext::new_parser_ctx(
            parent,
            invoking_state,
            OC_ProcedureNameContextExt { ph: PhantomData },
        ))
    }
}

pub trait OC_ProcedureNameContextAttrs<'input>:
    CypherParserContext<'input> + BorrowMut<OC_ProcedureNameContextExt<'input>>
{
    fn oC_Namespace(&self) -> Option<Rc<OC_NamespaceContextAll<'input>>>
    where
        Self: Sized,
    {
        self.child_of_type(0)
    }
    fn oC_SymbolicName(&self) -> Option<Rc<OC_SymbolicNameContextAll<'input>>>
    where
        Self: Sized,
    {
        self.child_of_type(0)
    }
}

impl<'input> OC_ProcedureNameContextAttrs<'input> for OC_ProcedureNameContext<'input> {}

impl<'input, I> CypherParser<'input, I>
where
    I: TokenStream<'input, TF = LocalTokenFactory<'input>> + TidAble<'input>,
{
    pub fn oC_ProcedureName(
        &mut self,
    ) -> Result<Rc<OC_ProcedureNameContextAll<'input>>, ANTLRError> {
        let mut recog = self;
        let _parentctx = recog.ctx.take();
        let mut _localctx =
            OC_ProcedureNameContextExt::new(_parentctx.clone(), recog.base.get_state());
        recog
            .base
            .enter_rule(_localctx.clone(), 182, RULE_oC_ProcedureName);
        let mut _localctx: Rc<OC_ProcedureNameContextAll> = _localctx;
        let result: Result<(), ANTLRError> = (|| {
            //recog.base.enter_outer_alt(_localctx.clone(), 1)?;
            recog.base.enter_outer_alt(None, 1)?;
            {
                /*InvokeRule oC_Namespace*/
                recog.base.set_state(1599);
                recog.oC_Namespace()?;

                /*InvokeRule oC_SymbolicName*/
                recog.base.set_state(1600);
                recog.oC_SymbolicName()?;
            }
            Ok(())
        })();
        match result {
            Ok(_) => {}
            Err(e @ ANTLRError::FallThrough(_)) => return Err(e),
            Err(ref re) => {
                //_localctx.exception = re;
                recog.err_handler.report_error(&mut recog.base, re);
                recog.err_handler.recover(&mut recog.base, re)?;
            }
        }
        recog.base.exit_rule()?;

        Ok(_localctx)
    }
}
//------------------- oC_Namespace ----------------
pub type OC_NamespaceContextAll<'input> = OC_NamespaceContext<'input>;

pub type OC_NamespaceContext<'input> =
    BaseParserRuleContext<'input, OC_NamespaceContextExt<'input>>;

#[derive(Clone)]
pub struct OC_NamespaceContextExt<'input> {
    ph: PhantomData<&'input str>,
}

impl<'input> CypherParserContext<'input> for OC_NamespaceContext<'input> {}

impl<'input, 'a> Listenable<dyn CypherListener<'input> + 'a> for OC_NamespaceContext<'input> {
    fn enter(&self, listener: &mut (dyn CypherListener<'input> + 'a)) -> Result<(), ANTLRError> {
        listener.enter_every_rule(self)?;
        listener.enter_oC_Namespace(self);
        Ok(())
    }
    fn exit(&self, listener: &mut (dyn CypherListener<'input> + 'a)) -> Result<(), ANTLRError> {
        listener.exit_oC_Namespace(self);
        listener.exit_every_rule(self)?;
        Ok(())
    }
}

impl<'input, 'a> Visitable<dyn CypherVisitor<'input> + 'a> for OC_NamespaceContext<'input> {
    fn accept(&self, visitor: &mut (dyn CypherVisitor<'input> + 'a)) {
        visitor.visit_oC_Namespace(self);
    }
}

impl<'input> CustomRuleContext<'input> for OC_NamespaceContextExt<'input> {
    type TF = LocalTokenFactory<'input>;
    type Ctx = CypherParserContextType;
    fn get_rule_index(&self) -> usize {
        RULE_oC_Namespace
    }
    //fn type_rule_index() -> usize where Self: Sized { RULE_oC_Namespace }
}
antlr4rust::tid! {OC_NamespaceContextExt<'a>}

impl<'input> OC_NamespaceContextExt<'input> {
    fn new(
        parent: Option<Rc<dyn CypherParserContext<'input> + 'input>>,
        invoking_state: i32,
    ) -> Rc<OC_NamespaceContextAll<'input>> {
        Rc::new(BaseParserRuleContext::new_parser_ctx(
            parent,
            invoking_state,
            OC_NamespaceContextExt { ph: PhantomData },
        ))
    }
}

pub trait OC_NamespaceContextAttrs<'input>:
    CypherParserContext<'input> + BorrowMut<OC_NamespaceContextExt<'input>>
{
    fn oC_SymbolicName_all(&self) -> Vec<Rc<OC_SymbolicNameContextAll<'input>>>
    where
        Self: Sized,
    {
        self.children_of_type()
    }
    fn oC_SymbolicName(&self, i: usize) -> Option<Rc<OC_SymbolicNameContextAll<'input>>>
    where
        Self: Sized,
    {
        self.child_of_type(i)
    }
}

impl<'input> OC_NamespaceContextAttrs<'input> for OC_NamespaceContext<'input> {}

impl<'input, I> CypherParser<'input, I>
where
    I: TokenStream<'input, TF = LocalTokenFactory<'input>> + TidAble<'input>,
{
    pub fn oC_Namespace(&mut self) -> Result<Rc<OC_NamespaceContextAll<'input>>, ANTLRError> {
        let mut recog = self;
        let _parentctx = recog.ctx.take();
        let mut _localctx = OC_NamespaceContextExt::new(_parentctx.clone(), recog.base.get_state());
        recog
            .base
            .enter_rule(_localctx.clone(), 184, RULE_oC_Namespace);
        let mut _localctx: Rc<OC_NamespaceContextAll> = _localctx;
        let result: Result<(), ANTLRError> = (|| {
            let mut _alt: i32;
            //recog.base.enter_outer_alt(_localctx.clone(), 1)?;
            recog.base.enter_outer_alt(None, 1)?;
            {
                recog.base.set_state(1607);
                recog.err_handler.sync(&mut recog.base)?;
                _alt = recog.interpreter.adaptive_predict(301, &mut recog.base)?;
                while { _alt != 2 && _alt != INVALID_ALT } {
                    if _alt == 1 {
                        {
                            {
                                /*InvokeRule oC_SymbolicName*/
                                recog.base.set_state(1602);
                                recog.oC_SymbolicName()?;

                                recog.base.set_state(1603);
                                recog
                                    .base
                                    .match_token(Cypher_T__22, &mut recog.err_handler)?;
                            }
                        }
                    }
                    recog.base.set_state(1609);
                    recog.err_handler.sync(&mut recog.base)?;
                    _alt = recog.interpreter.adaptive_predict(301, &mut recog.base)?;
                }
            }
            Ok(())
        })();
        match result {
            Ok(_) => {}
            Err(e @ ANTLRError::FallThrough(_)) => return Err(e),
            Err(ref re) => {
                //_localctx.exception = re;
                recog.err_handler.report_error(&mut recog.base, re);
                recog.err_handler.recover(&mut recog.base, re)?;
            }
        }
        recog.base.exit_rule()?;

        Ok(_localctx)
    }
}
//------------------- oC_Variable ----------------
pub type OC_VariableContextAll<'input> = OC_VariableContext<'input>;

pub type OC_VariableContext<'input> = BaseParserRuleContext<'input, OC_VariableContextExt<'input>>;

#[derive(Clone)]
pub struct OC_VariableContextExt<'input> {
    ph: PhantomData<&'input str>,
}

impl<'input> CypherParserContext<'input> for OC_VariableContext<'input> {}

impl<'input, 'a> Listenable<dyn CypherListener<'input> + 'a> for OC_VariableContext<'input> {
    fn enter(&self, listener: &mut (dyn CypherListener<'input> + 'a)) -> Result<(), ANTLRError> {
        listener.enter_every_rule(self)?;
        listener.enter_oC_Variable(self);
        Ok(())
    }
    fn exit(&self, listener: &mut (dyn CypherListener<'input> + 'a)) -> Result<(), ANTLRError> {
        listener.exit_oC_Variable(self);
        listener.exit_every_rule(self)?;
        Ok(())
    }
}

impl<'input, 'a> Visitable<dyn CypherVisitor<'input> + 'a> for OC_VariableContext<'input> {
    fn accept(&self, visitor: &mut (dyn CypherVisitor<'input> + 'a)) {
        visitor.visit_oC_Variable(self);
    }
}

impl<'input> CustomRuleContext<'input> for OC_VariableContextExt<'input> {
    type TF = LocalTokenFactory<'input>;
    type Ctx = CypherParserContextType;
    fn get_rule_index(&self) -> usize {
        RULE_oC_Variable
    }
    //fn type_rule_index() -> usize where Self: Sized { RULE_oC_Variable }
}
antlr4rust::tid! {OC_VariableContextExt<'a>}

impl<'input> OC_VariableContextExt<'input> {
    fn new(
        parent: Option<Rc<dyn CypherParserContext<'input> + 'input>>,
        invoking_state: i32,
    ) -> Rc<OC_VariableContextAll<'input>> {
        Rc::new(BaseParserRuleContext::new_parser_ctx(
            parent,
            invoking_state,
            OC_VariableContextExt { ph: PhantomData },
        ))
    }
}

pub trait OC_VariableContextAttrs<'input>:
    CypherParserContext<'input> + BorrowMut<OC_VariableContextExt<'input>>
{
    fn oC_SymbolicName(&self) -> Option<Rc<OC_SymbolicNameContextAll<'input>>>
    where
        Self: Sized,
    {
        self.child_of_type(0)
    }
}

impl<'input> OC_VariableContextAttrs<'input> for OC_VariableContext<'input> {}

impl<'input, I> CypherParser<'input, I>
where
    I: TokenStream<'input, TF = LocalTokenFactory<'input>> + TidAble<'input>,
{
    pub fn oC_Variable(&mut self) -> Result<Rc<OC_VariableContextAll<'input>>, ANTLRError> {
        let mut recog = self;
        let _parentctx = recog.ctx.take();
        let mut _localctx = OC_VariableContextExt::new(_parentctx.clone(), recog.base.get_state());
        recog
            .base
            .enter_rule(_localctx.clone(), 186, RULE_oC_Variable);
        let mut _localctx: Rc<OC_VariableContextAll> = _localctx;
        let result: Result<(), ANTLRError> = (|| {
            //recog.base.enter_outer_alt(_localctx.clone(), 1)?;
            recog.base.enter_outer_alt(None, 1)?;
            {
                /*InvokeRule oC_SymbolicName*/
                recog.base.set_state(1610);
                recog.oC_SymbolicName()?;
            }
            Ok(())
        })();
        match result {
            Ok(_) => {}
            Err(e @ ANTLRError::FallThrough(_)) => return Err(e),
            Err(ref re) => {
                //_localctx.exception = re;
                recog.err_handler.report_error(&mut recog.base, re);
                recog.err_handler.recover(&mut recog.base, re)?;
            }
        }
        recog.base.exit_rule()?;

        Ok(_localctx)
    }
}
//------------------- oC_Literal ----------------
pub type OC_LiteralContextAll<'input> = OC_LiteralContext<'input>;

pub type OC_LiteralContext<'input> = BaseParserRuleContext<'input, OC_LiteralContextExt<'input>>;

#[derive(Clone)]
pub struct OC_LiteralContextExt<'input> {
    ph: PhantomData<&'input str>,
}

impl<'input> CypherParserContext<'input> for OC_LiteralContext<'input> {}

impl<'input, 'a> Listenable<dyn CypherListener<'input> + 'a> for OC_LiteralContext<'input> {
    fn enter(&self, listener: &mut (dyn CypherListener<'input> + 'a)) -> Result<(), ANTLRError> {
        listener.enter_every_rule(self)?;
        listener.enter_oC_Literal(self);
        Ok(())
    }
    fn exit(&self, listener: &mut (dyn CypherListener<'input> + 'a)) -> Result<(), ANTLRError> {
        listener.exit_oC_Literal(self);
        listener.exit_every_rule(self)?;
        Ok(())
    }
}

impl<'input, 'a> Visitable<dyn CypherVisitor<'input> + 'a> for OC_LiteralContext<'input> {
    fn accept(&self, visitor: &mut (dyn CypherVisitor<'input> + 'a)) {
        visitor.visit_oC_Literal(self);
    }
}

impl<'input> CustomRuleContext<'input> for OC_LiteralContextExt<'input> {
    type TF = LocalTokenFactory<'input>;
    type Ctx = CypherParserContextType;
    fn get_rule_index(&self) -> usize {
        RULE_oC_Literal
    }
    //fn type_rule_index() -> usize where Self: Sized { RULE_oC_Literal }
}
antlr4rust::tid! {OC_LiteralContextExt<'a>}

impl<'input> OC_LiteralContextExt<'input> {
    fn new(
        parent: Option<Rc<dyn CypherParserContext<'input> + 'input>>,
        invoking_state: i32,
    ) -> Rc<OC_LiteralContextAll<'input>> {
        Rc::new(BaseParserRuleContext::new_parser_ctx(
            parent,
            invoking_state,
            OC_LiteralContextExt { ph: PhantomData },
        ))
    }
}

pub trait OC_LiteralContextAttrs<'input>:
    CypherParserContext<'input> + BorrowMut<OC_LiteralContextExt<'input>>
{
    fn oC_BooleanLiteral(&self) -> Option<Rc<OC_BooleanLiteralContextAll<'input>>>
    where
        Self: Sized,
    {
        self.child_of_type(0)
    }
    /// Retrieves first TerminalNode corresponding to token NULL
    /// Returns `None` if there is no child corresponding to token NULL
    fn NULL(&self) -> Option<Rc<TerminalNode<'input, CypherParserContextType>>>
    where
        Self: Sized,
    {
        self.get_token(Cypher_NULL, 0)
    }
    fn oC_NumberLiteral(&self) -> Option<Rc<OC_NumberLiteralContextAll<'input>>>
    where
        Self: Sized,
    {
        self.child_of_type(0)
    }
    /// Retrieves first TerminalNode corresponding to token StringLiteral
    /// Returns `None` if there is no child corresponding to token StringLiteral
    fn StringLiteral(&self) -> Option<Rc<TerminalNode<'input, CypherParserContextType>>>
    where
        Self: Sized,
    {
        self.get_token(Cypher_StringLiteral, 0)
    }
    fn oC_ListLiteral(&self) -> Option<Rc<OC_ListLiteralContextAll<'input>>>
    where
        Self: Sized,
    {
        self.child_of_type(0)
    }
    fn oC_MapLiteral(&self) -> Option<Rc<OC_MapLiteralContextAll<'input>>>
    where
        Self: Sized,
    {
        self.child_of_type(0)
    }
}

impl<'input> OC_LiteralContextAttrs<'input> for OC_LiteralContext<'input> {}

impl<'input, I> CypherParser<'input, I>
where
    I: TokenStream<'input, TF = LocalTokenFactory<'input>> + TidAble<'input>,
{
    pub fn oC_Literal(&mut self) -> Result<Rc<OC_LiteralContextAll<'input>>, ANTLRError> {
        let mut recog = self;
        let _parentctx = recog.ctx.take();
        let mut _localctx = OC_LiteralContextExt::new(_parentctx.clone(), recog.base.get_state());
        recog
            .base
            .enter_rule(_localctx.clone(), 188, RULE_oC_Literal);
        let mut _localctx: Rc<OC_LiteralContextAll> = _localctx;
        let result: Result<(), ANTLRError> = (|| {
            recog.base.set_state(1618);
            recog.err_handler.sync(&mut recog.base)?;
            match recog.base.input.la(1) {
                Cypher_TRUE | Cypher_FALSE => {
                    //recog.base.enter_outer_alt(_localctx.clone(), 1)?;
                    recog.base.enter_outer_alt(None, 1)?;
                    {
                        /*InvokeRule oC_BooleanLiteral*/
                        recog.base.set_state(1612);
                        recog.oC_BooleanLiteral()?;
                    }
                }

                Cypher_NULL => {
                    //recog.base.enter_outer_alt(_localctx.clone(), 2)?;
                    recog.base.enter_outer_alt(None, 2)?;
                    {
                        recog.base.set_state(1613);
                        recog
                            .base
                            .match_token(Cypher_NULL, &mut recog.err_handler)?;
                    }
                }

                Cypher_HexInteger
                | Cypher_DecimalInteger
                | Cypher_OctalInteger
                | Cypher_ExponentDecimalReal
                | Cypher_RegularDecimalReal => {
                    //recog.base.enter_outer_alt(_localctx.clone(), 3)?;
                    recog.base.enter_outer_alt(None, 3)?;
                    {
                        /*InvokeRule oC_NumberLiteral*/
                        recog.base.set_state(1614);
                        recog.oC_NumberLiteral()?;
                    }
                }

                Cypher_StringLiteral => {
                    //recog.base.enter_outer_alt(_localctx.clone(), 4)?;
                    recog.base.enter_outer_alt(None, 4)?;
                    {
                        recog.base.set_state(1615);
                        recog
                            .base
                            .match_token(Cypher_StringLiteral, &mut recog.err_handler)?;
                    }
                }

                Cypher_T__7 => {
                    //recog.base.enter_outer_alt(_localctx.clone(), 5)?;
                    recog.base.enter_outer_alt(None, 5)?;
                    {
                        /*InvokeRule oC_ListLiteral*/
                        recog.base.set_state(1616);
                        recog.oC_ListLiteral()?;
                    }
                }

                Cypher_T__23 => {
                    //recog.base.enter_outer_alt(_localctx.clone(), 6)?;
                    recog.base.enter_outer_alt(None, 6)?;
                    {
                        /*InvokeRule oC_MapLiteral*/
                        recog.base.set_state(1617);
                        recog.oC_MapLiteral()?;
                    }
                }

                _ => Err(ANTLRError::NoAltError(NoViableAltError::new(
                    &mut recog.base,
                )))?,
            }
            Ok(())
        })();
        match result {
            Ok(_) => {}
            Err(e @ ANTLRError::FallThrough(_)) => return Err(e),
            Err(ref re) => {
                //_localctx.exception = re;
                recog.err_handler.report_error(&mut recog.base, re);
                recog.err_handler.recover(&mut recog.base, re)?;
            }
        }
        recog.base.exit_rule()?;

        Ok(_localctx)
    }
}
//------------------- oC_BooleanLiteral ----------------
pub type OC_BooleanLiteralContextAll<'input> = OC_BooleanLiteralContext<'input>;

pub type OC_BooleanLiteralContext<'input> =
    BaseParserRuleContext<'input, OC_BooleanLiteralContextExt<'input>>;

#[derive(Clone)]
pub struct OC_BooleanLiteralContextExt<'input> {
    ph: PhantomData<&'input str>,
}

impl<'input> CypherParserContext<'input> for OC_BooleanLiteralContext<'input> {}

impl<'input, 'a> Listenable<dyn CypherListener<'input> + 'a> for OC_BooleanLiteralContext<'input> {
    fn enter(&self, listener: &mut (dyn CypherListener<'input> + 'a)) -> Result<(), ANTLRError> {
        listener.enter_every_rule(self)?;
        listener.enter_oC_BooleanLiteral(self);
        Ok(())
    }
    fn exit(&self, listener: &mut (dyn CypherListener<'input> + 'a)) -> Result<(), ANTLRError> {
        listener.exit_oC_BooleanLiteral(self);
        listener.exit_every_rule(self)?;
        Ok(())
    }
}

impl<'input, 'a> Visitable<dyn CypherVisitor<'input> + 'a> for OC_BooleanLiteralContext<'input> {
    fn accept(&self, visitor: &mut (dyn CypherVisitor<'input> + 'a)) {
        visitor.visit_oC_BooleanLiteral(self);
    }
}

impl<'input> CustomRuleContext<'input> for OC_BooleanLiteralContextExt<'input> {
    type TF = LocalTokenFactory<'input>;
    type Ctx = CypherParserContextType;
    fn get_rule_index(&self) -> usize {
        RULE_oC_BooleanLiteral
    }
    //fn type_rule_index() -> usize where Self: Sized { RULE_oC_BooleanLiteral }
}
antlr4rust::tid! {OC_BooleanLiteralContextExt<'a>}

impl<'input> OC_BooleanLiteralContextExt<'input> {
    fn new(
        parent: Option<Rc<dyn CypherParserContext<'input> + 'input>>,
        invoking_state: i32,
    ) -> Rc<OC_BooleanLiteralContextAll<'input>> {
        Rc::new(BaseParserRuleContext::new_parser_ctx(
            parent,
            invoking_state,
            OC_BooleanLiteralContextExt { ph: PhantomData },
        ))
    }
}

pub trait OC_BooleanLiteralContextAttrs<'input>:
    CypherParserContext<'input> + BorrowMut<OC_BooleanLiteralContextExt<'input>>
{
    /// Retrieves first TerminalNode corresponding to token TRUE
    /// Returns `None` if there is no child corresponding to token TRUE
    fn TRUE(&self) -> Option<Rc<TerminalNode<'input, CypherParserContextType>>>
    where
        Self: Sized,
    {
        self.get_token(Cypher_TRUE, 0)
    }
    /// Retrieves first TerminalNode corresponding to token FALSE
    /// Returns `None` if there is no child corresponding to token FALSE
    fn FALSE(&self) -> Option<Rc<TerminalNode<'input, CypherParserContextType>>>
    where
        Self: Sized,
    {
        self.get_token(Cypher_FALSE, 0)
    }
}

impl<'input> OC_BooleanLiteralContextAttrs<'input> for OC_BooleanLiteralContext<'input> {}

impl<'input, I> CypherParser<'input, I>
where
    I: TokenStream<'input, TF = LocalTokenFactory<'input>> + TidAble<'input>,
{
    pub fn oC_BooleanLiteral(
        &mut self,
    ) -> Result<Rc<OC_BooleanLiteralContextAll<'input>>, ANTLRError> {
        let mut recog = self;
        let _parentctx = recog.ctx.take();
        let mut _localctx =
            OC_BooleanLiteralContextExt::new(_parentctx.clone(), recog.base.get_state());
        recog
            .base
            .enter_rule(_localctx.clone(), 190, RULE_oC_BooleanLiteral);
        let mut _localctx: Rc<OC_BooleanLiteralContextAll> = _localctx;
        let mut _la: i32 = -1;
        let result: Result<(), ANTLRError> = (|| {
            //recog.base.enter_outer_alt(_localctx.clone(), 1)?;
            recog.base.enter_outer_alt(None, 1)?;
            {
                recog.base.set_state(1620);
                _la = recog.base.input.la(1);
                if { !(_la == Cypher_TRUE || _la == Cypher_FALSE) } {
                    recog.err_handler.recover_inline(&mut recog.base)?;
                } else {
                    if recog.base.input.la(1) == TOKEN_EOF {
                        recog.base.matched_eof = true
                    };
                    recog.err_handler.report_match(&mut recog.base);
                    recog.base.consume(&mut recog.err_handler);
                }
            }
            Ok(())
        })();
        match result {
            Ok(_) => {}
            Err(e @ ANTLRError::FallThrough(_)) => return Err(e),
            Err(ref re) => {
                //_localctx.exception = re;
                recog.err_handler.report_error(&mut recog.base, re);
                recog.err_handler.recover(&mut recog.base, re)?;
            }
        }
        recog.base.exit_rule()?;

        Ok(_localctx)
    }
}
//------------------- oC_NumberLiteral ----------------
pub type OC_NumberLiteralContextAll<'input> = OC_NumberLiteralContext<'input>;

pub type OC_NumberLiteralContext<'input> =
    BaseParserRuleContext<'input, OC_NumberLiteralContextExt<'input>>;

#[derive(Clone)]
pub struct OC_NumberLiteralContextExt<'input> {
    ph: PhantomData<&'input str>,
}

impl<'input> CypherParserContext<'input> for OC_NumberLiteralContext<'input> {}

impl<'input, 'a> Listenable<dyn CypherListener<'input> + 'a> for OC_NumberLiteralContext<'input> {
    fn enter(&self, listener: &mut (dyn CypherListener<'input> + 'a)) -> Result<(), ANTLRError> {
        listener.enter_every_rule(self)?;
        listener.enter_oC_NumberLiteral(self);
        Ok(())
    }
    fn exit(&self, listener: &mut (dyn CypherListener<'input> + 'a)) -> Result<(), ANTLRError> {
        listener.exit_oC_NumberLiteral(self);
        listener.exit_every_rule(self)?;
        Ok(())
    }
}

impl<'input, 'a> Visitable<dyn CypherVisitor<'input> + 'a> for OC_NumberLiteralContext<'input> {
    fn accept(&self, visitor: &mut (dyn CypherVisitor<'input> + 'a)) {
        visitor.visit_oC_NumberLiteral(self);
    }
}

impl<'input> CustomRuleContext<'input> for OC_NumberLiteralContextExt<'input> {
    type TF = LocalTokenFactory<'input>;
    type Ctx = CypherParserContextType;
    fn get_rule_index(&self) -> usize {
        RULE_oC_NumberLiteral
    }
    //fn type_rule_index() -> usize where Self: Sized { RULE_oC_NumberLiteral }
}
antlr4rust::tid! {OC_NumberLiteralContextExt<'a>}

impl<'input> OC_NumberLiteralContextExt<'input> {
    fn new(
        parent: Option<Rc<dyn CypherParserContext<'input> + 'input>>,
        invoking_state: i32,
    ) -> Rc<OC_NumberLiteralContextAll<'input>> {
        Rc::new(BaseParserRuleContext::new_parser_ctx(
            parent,
            invoking_state,
            OC_NumberLiteralContextExt { ph: PhantomData },
        ))
    }
}

pub trait OC_NumberLiteralContextAttrs<'input>:
    CypherParserContext<'input> + BorrowMut<OC_NumberLiteralContextExt<'input>>
{
    fn oC_DoubleLiteral(&self) -> Option<Rc<OC_DoubleLiteralContextAll<'input>>>
    where
        Self: Sized,
    {
        self.child_of_type(0)
    }
    fn oC_IntegerLiteral(&self) -> Option<Rc<OC_IntegerLiteralContextAll<'input>>>
    where
        Self: Sized,
    {
        self.child_of_type(0)
    }
}

impl<'input> OC_NumberLiteralContextAttrs<'input> for OC_NumberLiteralContext<'input> {}

impl<'input, I> CypherParser<'input, I>
where
    I: TokenStream<'input, TF = LocalTokenFactory<'input>> + TidAble<'input>,
{
    pub fn oC_NumberLiteral(
        &mut self,
    ) -> Result<Rc<OC_NumberLiteralContextAll<'input>>, ANTLRError> {
        let mut recog = self;
        let _parentctx = recog.ctx.take();
        let mut _localctx =
            OC_NumberLiteralContextExt::new(_parentctx.clone(), recog.base.get_state());
        recog
            .base
            .enter_rule(_localctx.clone(), 192, RULE_oC_NumberLiteral);
        let mut _localctx: Rc<OC_NumberLiteralContextAll> = _localctx;
        let result: Result<(), ANTLRError> = (|| {
            recog.base.set_state(1624);
            recog.err_handler.sync(&mut recog.base)?;
            match recog.base.input.la(1) {
                Cypher_ExponentDecimalReal | Cypher_RegularDecimalReal => {
                    //recog.base.enter_outer_alt(_localctx.clone(), 1)?;
                    recog.base.enter_outer_alt(None, 1)?;
                    {
                        /*InvokeRule oC_DoubleLiteral*/
                        recog.base.set_state(1622);
                        recog.oC_DoubleLiteral()?;
                    }
                }

                Cypher_HexInteger | Cypher_DecimalInteger | Cypher_OctalInteger => {
                    //recog.base.enter_outer_alt(_localctx.clone(), 2)?;
                    recog.base.enter_outer_alt(None, 2)?;
                    {
                        /*InvokeRule oC_IntegerLiteral*/
                        recog.base.set_state(1623);
                        recog.oC_IntegerLiteral()?;
                    }
                }

                _ => Err(ANTLRError::NoAltError(NoViableAltError::new(
                    &mut recog.base,
                )))?,
            }
            Ok(())
        })();
        match result {
            Ok(_) => {}
            Err(e @ ANTLRError::FallThrough(_)) => return Err(e),
            Err(ref re) => {
                //_localctx.exception = re;
                recog.err_handler.report_error(&mut recog.base, re);
                recog.err_handler.recover(&mut recog.base, re)?;
            }
        }
        recog.base.exit_rule()?;

        Ok(_localctx)
    }
}
//------------------- oC_IntegerLiteral ----------------
pub type OC_IntegerLiteralContextAll<'input> = OC_IntegerLiteralContext<'input>;

pub type OC_IntegerLiteralContext<'input> =
    BaseParserRuleContext<'input, OC_IntegerLiteralContextExt<'input>>;

#[derive(Clone)]
pub struct OC_IntegerLiteralContextExt<'input> {
    ph: PhantomData<&'input str>,
}

impl<'input> CypherParserContext<'input> for OC_IntegerLiteralContext<'input> {}

impl<'input, 'a> Listenable<dyn CypherListener<'input> + 'a> for OC_IntegerLiteralContext<'input> {
    fn enter(&self, listener: &mut (dyn CypherListener<'input> + 'a)) -> Result<(), ANTLRError> {
        listener.enter_every_rule(self)?;
        listener.enter_oC_IntegerLiteral(self);
        Ok(())
    }
    fn exit(&self, listener: &mut (dyn CypherListener<'input> + 'a)) -> Result<(), ANTLRError> {
        listener.exit_oC_IntegerLiteral(self);
        listener.exit_every_rule(self)?;
        Ok(())
    }
}

impl<'input, 'a> Visitable<dyn CypherVisitor<'input> + 'a> for OC_IntegerLiteralContext<'input> {
    fn accept(&self, visitor: &mut (dyn CypherVisitor<'input> + 'a)) {
        visitor.visit_oC_IntegerLiteral(self);
    }
}

impl<'input> CustomRuleContext<'input> for OC_IntegerLiteralContextExt<'input> {
    type TF = LocalTokenFactory<'input>;
    type Ctx = CypherParserContextType;
    fn get_rule_index(&self) -> usize {
        RULE_oC_IntegerLiteral
    }
    //fn type_rule_index() -> usize where Self: Sized { RULE_oC_IntegerLiteral }
}
antlr4rust::tid! {OC_IntegerLiteralContextExt<'a>}

impl<'input> OC_IntegerLiteralContextExt<'input> {
    fn new(
        parent: Option<Rc<dyn CypherParserContext<'input> + 'input>>,
        invoking_state: i32,
    ) -> Rc<OC_IntegerLiteralContextAll<'input>> {
        Rc::new(BaseParserRuleContext::new_parser_ctx(
            parent,
            invoking_state,
            OC_IntegerLiteralContextExt { ph: PhantomData },
        ))
    }
}

pub trait OC_IntegerLiteralContextAttrs<'input>:
    CypherParserContext<'input> + BorrowMut<OC_IntegerLiteralContextExt<'input>>
{
    /// Retrieves first TerminalNode corresponding to token HexInteger
    /// Returns `None` if there is no child corresponding to token HexInteger
    fn HexInteger(&self) -> Option<Rc<TerminalNode<'input, CypherParserContextType>>>
    where
        Self: Sized,
    {
        self.get_token(Cypher_HexInteger, 0)
    }
    /// Retrieves first TerminalNode corresponding to token OctalInteger
    /// Returns `None` if there is no child corresponding to token OctalInteger
    fn OctalInteger(&self) -> Option<Rc<TerminalNode<'input, CypherParserContextType>>>
    where
        Self: Sized,
    {
        self.get_token(Cypher_OctalInteger, 0)
    }
    /// Retrieves first TerminalNode corresponding to token DecimalInteger
    /// Returns `None` if there is no child corresponding to token DecimalInteger
    fn DecimalInteger(&self) -> Option<Rc<TerminalNode<'input, CypherParserContextType>>>
    where
        Self: Sized,
    {
        self.get_token(Cypher_DecimalInteger, 0)
    }
}

impl<'input> OC_IntegerLiteralContextAttrs<'input> for OC_IntegerLiteralContext<'input> {}

impl<'input, I> CypherParser<'input, I>
where
    I: TokenStream<'input, TF = LocalTokenFactory<'input>> + TidAble<'input>,
{
    pub fn oC_IntegerLiteral(
        &mut self,
    ) -> Result<Rc<OC_IntegerLiteralContextAll<'input>>, ANTLRError> {
        let mut recog = self;
        let _parentctx = recog.ctx.take();
        let mut _localctx =
            OC_IntegerLiteralContextExt::new(_parentctx.clone(), recog.base.get_state());
        recog
            .base
            .enter_rule(_localctx.clone(), 194, RULE_oC_IntegerLiteral);
        let mut _localctx: Rc<OC_IntegerLiteralContextAll> = _localctx;
        let mut _la: i32 = -1;
        let result: Result<(), ANTLRError> = (|| {
            //recog.base.enter_outer_alt(_localctx.clone(), 1)?;
            recog.base.enter_outer_alt(None, 1)?;
            {
                recog.base.set_state(1626);
                _la = recog.base.input.la(1);
                if { !(((_la - 96) & !0x3f) == 0 && ((1usize << (_la - 96)) & 7) != 0) } {
                    recog.err_handler.recover_inline(&mut recog.base)?;
                } else {
                    if recog.base.input.la(1) == TOKEN_EOF {
                        recog.base.matched_eof = true
                    };
                    recog.err_handler.report_match(&mut recog.base);
                    recog.base.consume(&mut recog.err_handler);
                }
            }
            Ok(())
        })();
        match result {
            Ok(_) => {}
            Err(e @ ANTLRError::FallThrough(_)) => return Err(e),
            Err(ref re) => {
                //_localctx.exception = re;
                recog.err_handler.report_error(&mut recog.base, re);
                recog.err_handler.recover(&mut recog.base, re)?;
            }
        }
        recog.base.exit_rule()?;

        Ok(_localctx)
    }
}
//------------------- oC_DoubleLiteral ----------------
pub type OC_DoubleLiteralContextAll<'input> = OC_DoubleLiteralContext<'input>;

pub type OC_DoubleLiteralContext<'input> =
    BaseParserRuleContext<'input, OC_DoubleLiteralContextExt<'input>>;

#[derive(Clone)]
pub struct OC_DoubleLiteralContextExt<'input> {
    ph: PhantomData<&'input str>,
}

impl<'input> CypherParserContext<'input> for OC_DoubleLiteralContext<'input> {}

impl<'input, 'a> Listenable<dyn CypherListener<'input> + 'a> for OC_DoubleLiteralContext<'input> {
    fn enter(&self, listener: &mut (dyn CypherListener<'input> + 'a)) -> Result<(), ANTLRError> {
        listener.enter_every_rule(self)?;
        listener.enter_oC_DoubleLiteral(self);
        Ok(())
    }
    fn exit(&self, listener: &mut (dyn CypherListener<'input> + 'a)) -> Result<(), ANTLRError> {
        listener.exit_oC_DoubleLiteral(self);
        listener.exit_every_rule(self)?;
        Ok(())
    }
}

impl<'input, 'a> Visitable<dyn CypherVisitor<'input> + 'a> for OC_DoubleLiteralContext<'input> {
    fn accept(&self, visitor: &mut (dyn CypherVisitor<'input> + 'a)) {
        visitor.visit_oC_DoubleLiteral(self);
    }
}

impl<'input> CustomRuleContext<'input> for OC_DoubleLiteralContextExt<'input> {
    type TF = LocalTokenFactory<'input>;
    type Ctx = CypherParserContextType;
    fn get_rule_index(&self) -> usize {
        RULE_oC_DoubleLiteral
    }
    //fn type_rule_index() -> usize where Self: Sized { RULE_oC_DoubleLiteral }
}
antlr4rust::tid! {OC_DoubleLiteralContextExt<'a>}

impl<'input> OC_DoubleLiteralContextExt<'input> {
    fn new(
        parent: Option<Rc<dyn CypherParserContext<'input> + 'input>>,
        invoking_state: i32,
    ) -> Rc<OC_DoubleLiteralContextAll<'input>> {
        Rc::new(BaseParserRuleContext::new_parser_ctx(
            parent,
            invoking_state,
            OC_DoubleLiteralContextExt { ph: PhantomData },
        ))
    }
}

pub trait OC_DoubleLiteralContextAttrs<'input>:
    CypherParserContext<'input> + BorrowMut<OC_DoubleLiteralContextExt<'input>>
{
    /// Retrieves first TerminalNode corresponding to token ExponentDecimalReal
    /// Returns `None` if there is no child corresponding to token ExponentDecimalReal
    fn ExponentDecimalReal(&self) -> Option<Rc<TerminalNode<'input, CypherParserContextType>>>
    where
        Self: Sized,
    {
        self.get_token(Cypher_ExponentDecimalReal, 0)
    }
    /// Retrieves first TerminalNode corresponding to token RegularDecimalReal
    /// Returns `None` if there is no child corresponding to token RegularDecimalReal
    fn RegularDecimalReal(&self) -> Option<Rc<TerminalNode<'input, CypherParserContextType>>>
    where
        Self: Sized,
    {
        self.get_token(Cypher_RegularDecimalReal, 0)
    }
}

impl<'input> OC_DoubleLiteralContextAttrs<'input> for OC_DoubleLiteralContext<'input> {}

impl<'input, I> CypherParser<'input, I>
where
    I: TokenStream<'input, TF = LocalTokenFactory<'input>> + TidAble<'input>,
{
    pub fn oC_DoubleLiteral(
        &mut self,
    ) -> Result<Rc<OC_DoubleLiteralContextAll<'input>>, ANTLRError> {
        let mut recog = self;
        let _parentctx = recog.ctx.take();
        let mut _localctx =
            OC_DoubleLiteralContextExt::new(_parentctx.clone(), recog.base.get_state());
        recog
            .base
            .enter_rule(_localctx.clone(), 196, RULE_oC_DoubleLiteral);
        let mut _localctx: Rc<OC_DoubleLiteralContextAll> = _localctx;
        let mut _la: i32 = -1;
        let result: Result<(), ANTLRError> = (|| {
            //recog.base.enter_outer_alt(_localctx.clone(), 1)?;
            recog.base.enter_outer_alt(None, 1)?;
            {
                recog.base.set_state(1628);
                _la = recog.base.input.la(1);
                if { !(_la == Cypher_ExponentDecimalReal || _la == Cypher_RegularDecimalReal) } {
                    recog.err_handler.recover_inline(&mut recog.base)?;
                } else {
                    if recog.base.input.la(1) == TOKEN_EOF {
                        recog.base.matched_eof = true
                    };
                    recog.err_handler.report_match(&mut recog.base);
                    recog.base.consume(&mut recog.err_handler);
                }
            }
            Ok(())
        })();
        match result {
            Ok(_) => {}
            Err(e @ ANTLRError::FallThrough(_)) => return Err(e),
            Err(ref re) => {
                //_localctx.exception = re;
                recog.err_handler.report_error(&mut recog.base, re);
                recog.err_handler.recover(&mut recog.base, re)?;
            }
        }
        recog.base.exit_rule()?;

        Ok(_localctx)
    }
}
//------------------- oC_ListLiteral ----------------
pub type OC_ListLiteralContextAll<'input> = OC_ListLiteralContext<'input>;

pub type OC_ListLiteralContext<'input> =
    BaseParserRuleContext<'input, OC_ListLiteralContextExt<'input>>;

#[derive(Clone)]
pub struct OC_ListLiteralContextExt<'input> {
    ph: PhantomData<&'input str>,
}

impl<'input> CypherParserContext<'input> for OC_ListLiteralContext<'input> {}

impl<'input, 'a> Listenable<dyn CypherListener<'input> + 'a> for OC_ListLiteralContext<'input> {
    fn enter(&self, listener: &mut (dyn CypherListener<'input> + 'a)) -> Result<(), ANTLRError> {
        listener.enter_every_rule(self)?;
        listener.enter_oC_ListLiteral(self);
        Ok(())
    }
    fn exit(&self, listener: &mut (dyn CypherListener<'input> + 'a)) -> Result<(), ANTLRError> {
        listener.exit_oC_ListLiteral(self);
        listener.exit_every_rule(self)?;
        Ok(())
    }
}

impl<'input, 'a> Visitable<dyn CypherVisitor<'input> + 'a> for OC_ListLiteralContext<'input> {
    fn accept(&self, visitor: &mut (dyn CypherVisitor<'input> + 'a)) {
        visitor.visit_oC_ListLiteral(self);
    }
}

impl<'input> CustomRuleContext<'input> for OC_ListLiteralContextExt<'input> {
    type TF = LocalTokenFactory<'input>;
    type Ctx = CypherParserContextType;
    fn get_rule_index(&self) -> usize {
        RULE_oC_ListLiteral
    }
    //fn type_rule_index() -> usize where Self: Sized { RULE_oC_ListLiteral }
}
antlr4rust::tid! {OC_ListLiteralContextExt<'a>}

impl<'input> OC_ListLiteralContextExt<'input> {
    fn new(
        parent: Option<Rc<dyn CypherParserContext<'input> + 'input>>,
        invoking_state: i32,
    ) -> Rc<OC_ListLiteralContextAll<'input>> {
        Rc::new(BaseParserRuleContext::new_parser_ctx(
            parent,
            invoking_state,
            OC_ListLiteralContextExt { ph: PhantomData },
        ))
    }
}

pub trait OC_ListLiteralContextAttrs<'input>:
    CypherParserContext<'input> + BorrowMut<OC_ListLiteralContextExt<'input>>
{
    /// Retrieves all `TerminalNode`s corresponding to token SP in current rule
    fn SP_all(&self) -> Vec<Rc<TerminalNode<'input, CypherParserContextType>>>
    where
        Self: Sized,
    {
        self.children_of_type()
    }
    /// Retrieves 'i's TerminalNode corresponding to token SP, starting from 0.
    /// Returns `None` if number of children corresponding to token SP is less or equal than `i`.
    fn SP(&self, i: usize) -> Option<Rc<TerminalNode<'input, CypherParserContextType>>>
    where
        Self: Sized,
    {
        self.get_token(Cypher_SP, i)
    }
    fn oC_Expression_all(&self) -> Vec<Rc<OC_ExpressionContextAll<'input>>>
    where
        Self: Sized,
    {
        self.children_of_type()
    }
    fn oC_Expression(&self, i: usize) -> Option<Rc<OC_ExpressionContextAll<'input>>>
    where
        Self: Sized,
    {
        self.child_of_type(i)
    }
}

impl<'input> OC_ListLiteralContextAttrs<'input> for OC_ListLiteralContext<'input> {}

impl<'input, I> CypherParser<'input, I>
where
    I: TokenStream<'input, TF = LocalTokenFactory<'input>> + TidAble<'input>,
{
    pub fn oC_ListLiteral(&mut self) -> Result<Rc<OC_ListLiteralContextAll<'input>>, ANTLRError> {
        let mut recog = self;
        let _parentctx = recog.ctx.take();
        let mut _localctx =
            OC_ListLiteralContextExt::new(_parentctx.clone(), recog.base.get_state());
        recog
            .base
            .enter_rule(_localctx.clone(), 198, RULE_oC_ListLiteral);
        let mut _localctx: Rc<OC_ListLiteralContextAll> = _localctx;
        let mut _la: i32 = -1;
        let result: Result<(), ANTLRError> = (|| {
            //recog.base.enter_outer_alt(_localctx.clone(), 1)?;
            recog.base.enter_outer_alt(None, 1)?;
            {
                recog.base.set_state(1630);
                recog
                    .base
                    .match_token(Cypher_T__7, &mut recog.err_handler)?;

                recog.base.set_state(1632);
                recog.err_handler.sync(&mut recog.base)?;
                _la = recog.base.input.la(1);
                if _la == Cypher_SP {
                    {
                        recog.base.set_state(1631);
                        recog.base.match_token(Cypher_SP, &mut recog.err_handler)?;
                    }
                }

                recog.base.set_state(1651);
                recog.err_handler.sync(&mut recog.base)?;
                _la = recog.base.input.la(1);
                if (((_la) & !0x3f) == 0 && ((1usize << _la) & 84672832) != 0)
                    || _la == Cypher_ALL
                    || _la == Cypher_NOT
                    || (((_la - 82) & !0x3f) == 0 && ((1usize << (_la - 82)) & 117702535) != 0)
                    || (((_la - 120) & !0x3f) == 0 && ((1usize << (_la - 120)) & 39) != 0)
                {
                    {
                        /*InvokeRule oC_Expression*/
                        recog.base.set_state(1634);
                        recog.oC_Expression()?;

                        recog.base.set_state(1636);
                        recog.err_handler.sync(&mut recog.base)?;
                        _la = recog.base.input.la(1);
                        if _la == Cypher_SP {
                            {
                                recog.base.set_state(1635);
                                recog.base.match_token(Cypher_SP, &mut recog.err_handler)?;
                            }
                        }

                        recog.base.set_state(1648);
                        recog.err_handler.sync(&mut recog.base)?;
                        _la = recog.base.input.la(1);
                        while _la == Cypher_T__1 {
                            {
                                {
                                    recog.base.set_state(1638);
                                    recog
                                        .base
                                        .match_token(Cypher_T__1, &mut recog.err_handler)?;

                                    recog.base.set_state(1640);
                                    recog.err_handler.sync(&mut recog.base)?;
                                    _la = recog.base.input.la(1);
                                    if _la == Cypher_SP {
                                        {
                                            recog.base.set_state(1639);
                                            recog
                                                .base
                                                .match_token(Cypher_SP, &mut recog.err_handler)?;
                                        }
                                    }

                                    /*InvokeRule oC_Expression*/
                                    recog.base.set_state(1642);
                                    recog.oC_Expression()?;

                                    recog.base.set_state(1644);
                                    recog.err_handler.sync(&mut recog.base)?;
                                    _la = recog.base.input.la(1);
                                    if _la == Cypher_SP {
                                        {
                                            recog.base.set_state(1643);
                                            recog
                                                .base
                                                .match_token(Cypher_SP, &mut recog.err_handler)?;
                                        }
                                    }
                                }
                            }
                            recog.base.set_state(1650);
                            recog.err_handler.sync(&mut recog.base)?;
                            _la = recog.base.input.la(1);
                        }
                    }
                }

                recog.base.set_state(1653);
                recog
                    .base
                    .match_token(Cypher_T__8, &mut recog.err_handler)?;
            }
            Ok(())
        })();
        match result {
            Ok(_) => {}
            Err(e @ ANTLRError::FallThrough(_)) => return Err(e),
            Err(ref re) => {
                //_localctx.exception = re;
                recog.err_handler.report_error(&mut recog.base, re);
                recog.err_handler.recover(&mut recog.base, re)?;
            }
        }
        recog.base.exit_rule()?;

        Ok(_localctx)
    }
}
//------------------- oC_MapLiteral ----------------
pub type OC_MapLiteralContextAll<'input> = OC_MapLiteralContext<'input>;

pub type OC_MapLiteralContext<'input> =
    BaseParserRuleContext<'input, OC_MapLiteralContextExt<'input>>;

#[derive(Clone)]
pub struct OC_MapLiteralContextExt<'input> {
    ph: PhantomData<&'input str>,
}

impl<'input> CypherParserContext<'input> for OC_MapLiteralContext<'input> {}

impl<'input, 'a> Listenable<dyn CypherListener<'input> + 'a> for OC_MapLiteralContext<'input> {
    fn enter(&self, listener: &mut (dyn CypherListener<'input> + 'a)) -> Result<(), ANTLRError> {
        listener.enter_every_rule(self)?;
        listener.enter_oC_MapLiteral(self);
        Ok(())
    }
    fn exit(&self, listener: &mut (dyn CypherListener<'input> + 'a)) -> Result<(), ANTLRError> {
        listener.exit_oC_MapLiteral(self);
        listener.exit_every_rule(self)?;
        Ok(())
    }
}

impl<'input, 'a> Visitable<dyn CypherVisitor<'input> + 'a> for OC_MapLiteralContext<'input> {
    fn accept(&self, visitor: &mut (dyn CypherVisitor<'input> + 'a)) {
        visitor.visit_oC_MapLiteral(self);
    }
}

impl<'input> CustomRuleContext<'input> for OC_MapLiteralContextExt<'input> {
    type TF = LocalTokenFactory<'input>;
    type Ctx = CypherParserContextType;
    fn get_rule_index(&self) -> usize {
        RULE_oC_MapLiteral
    }
    //fn type_rule_index() -> usize where Self: Sized { RULE_oC_MapLiteral }
}
antlr4rust::tid! {OC_MapLiteralContextExt<'a>}

impl<'input> OC_MapLiteralContextExt<'input> {
    fn new(
        parent: Option<Rc<dyn CypherParserContext<'input> + 'input>>,
        invoking_state: i32,
    ) -> Rc<OC_MapLiteralContextAll<'input>> {
        Rc::new(BaseParserRuleContext::new_parser_ctx(
            parent,
            invoking_state,
            OC_MapLiteralContextExt { ph: PhantomData },
        ))
    }
}

pub trait OC_MapLiteralContextAttrs<'input>:
    CypherParserContext<'input> + BorrowMut<OC_MapLiteralContextExt<'input>>
{
    /// Retrieves all `TerminalNode`s corresponding to token SP in current rule
    fn SP_all(&self) -> Vec<Rc<TerminalNode<'input, CypherParserContextType>>>
    where
        Self: Sized,
    {
        self.children_of_type()
    }
    /// Retrieves 'i's TerminalNode corresponding to token SP, starting from 0.
    /// Returns `None` if number of children corresponding to token SP is less or equal than `i`.
    fn SP(&self, i: usize) -> Option<Rc<TerminalNode<'input, CypherParserContextType>>>
    where
        Self: Sized,
    {
        self.get_token(Cypher_SP, i)
    }
    fn oC_PropertyKeyName_all(&self) -> Vec<Rc<OC_PropertyKeyNameContextAll<'input>>>
    where
        Self: Sized,
    {
        self.children_of_type()
    }
    fn oC_PropertyKeyName(&self, i: usize) -> Option<Rc<OC_PropertyKeyNameContextAll<'input>>>
    where
        Self: Sized,
    {
        self.child_of_type(i)
    }
    fn oC_Expression_all(&self) -> Vec<Rc<OC_ExpressionContextAll<'input>>>
    where
        Self: Sized,
    {
        self.children_of_type()
    }
    fn oC_Expression(&self, i: usize) -> Option<Rc<OC_ExpressionContextAll<'input>>>
    where
        Self: Sized,
    {
        self.child_of_type(i)
    }
}

impl<'input> OC_MapLiteralContextAttrs<'input> for OC_MapLiteralContext<'input> {}

impl<'input, I> CypherParser<'input, I>
where
    I: TokenStream<'input, TF = LocalTokenFactory<'input>> + TidAble<'input>,
{
    pub fn oC_MapLiteral(&mut self) -> Result<Rc<OC_MapLiteralContextAll<'input>>, ANTLRError> {
        let mut recog = self;
        let _parentctx = recog.ctx.take();
        let mut _localctx =
            OC_MapLiteralContextExt::new(_parentctx.clone(), recog.base.get_state());
        recog
            .base
            .enter_rule(_localctx.clone(), 200, RULE_oC_MapLiteral);
        let mut _localctx: Rc<OC_MapLiteralContextAll> = _localctx;
        let mut _la: i32 = -1;
        let result: Result<(), ANTLRError> = (|| {
            //recog.base.enter_outer_alt(_localctx.clone(), 1)?;
            recog.base.enter_outer_alt(None, 1)?;
            {
                recog.base.set_state(1655);
                recog
                    .base
                    .match_token(Cypher_T__23, &mut recog.err_handler)?;

                recog.base.set_state(1657);
                recog.err_handler.sync(&mut recog.base)?;
                _la = recog.base.input.la(1);
                if _la == Cypher_SP {
                    {
                        recog.base.set_state(1656);
                        recog.base.match_token(Cypher_SP, &mut recog.err_handler)?;
                    }
                }

                recog.base.set_state(1692);
                recog.err_handler.sync(&mut recog.base)?;
                _la = recog.base.input.la(1);
                if (((_la - 46) & !0x3f) == 0 && ((1usize << (_la - 46)) & 4294942719) != 0)
                    || (((_la - 78) & !0x3f) == 0 && ((1usize << (_la - 78)) & 2359295) != 0)
                    || (((_la - 110) & !0x3f) == 0 && ((1usize << (_la - 110)) & 40959) != 0)
                {
                    {
                        /*InvokeRule oC_PropertyKeyName*/
                        recog.base.set_state(1659);
                        recog.oC_PropertyKeyName()?;

                        recog.base.set_state(1661);
                        recog.err_handler.sync(&mut recog.base)?;
                        _la = recog.base.input.la(1);
                        if _la == Cypher_SP {
                            {
                                recog.base.set_state(1660);
                                recog.base.match_token(Cypher_SP, &mut recog.err_handler)?;
                            }
                        }

                        recog.base.set_state(1663);
                        recog
                            .base
                            .match_token(Cypher_T__9, &mut recog.err_handler)?;

                        recog.base.set_state(1665);
                        recog.err_handler.sync(&mut recog.base)?;
                        _la = recog.base.input.la(1);
                        if _la == Cypher_SP {
                            {
                                recog.base.set_state(1664);
                                recog.base.match_token(Cypher_SP, &mut recog.err_handler)?;
                            }
                        }

                        /*InvokeRule oC_Expression*/
                        recog.base.set_state(1667);
                        recog.oC_Expression()?;

                        recog.base.set_state(1669);
                        recog.err_handler.sync(&mut recog.base)?;
                        _la = recog.base.input.la(1);
                        if _la == Cypher_SP {
                            {
                                recog.base.set_state(1668);
                                recog.base.match_token(Cypher_SP, &mut recog.err_handler)?;
                            }
                        }

                        recog.base.set_state(1689);
                        recog.err_handler.sync(&mut recog.base)?;
                        _la = recog.base.input.la(1);
                        while _la == Cypher_T__1 {
                            {
                                {
                                    recog.base.set_state(1671);
                                    recog
                                        .base
                                        .match_token(Cypher_T__1, &mut recog.err_handler)?;

                                    recog.base.set_state(1673);
                                    recog.err_handler.sync(&mut recog.base)?;
                                    _la = recog.base.input.la(1);
                                    if _la == Cypher_SP {
                                        {
                                            recog.base.set_state(1672);
                                            recog
                                                .base
                                                .match_token(Cypher_SP, &mut recog.err_handler)?;
                                        }
                                    }

                                    /*InvokeRule oC_PropertyKeyName*/
                                    recog.base.set_state(1675);
                                    recog.oC_PropertyKeyName()?;

                                    recog.base.set_state(1677);
                                    recog.err_handler.sync(&mut recog.base)?;
                                    _la = recog.base.input.la(1);
                                    if _la == Cypher_SP {
                                        {
                                            recog.base.set_state(1676);
                                            recog
                                                .base
                                                .match_token(Cypher_SP, &mut recog.err_handler)?;
                                        }
                                    }

                                    recog.base.set_state(1679);
                                    recog
                                        .base
                                        .match_token(Cypher_T__9, &mut recog.err_handler)?;

                                    recog.base.set_state(1681);
                                    recog.err_handler.sync(&mut recog.base)?;
                                    _la = recog.base.input.la(1);
                                    if _la == Cypher_SP {
                                        {
                                            recog.base.set_state(1680);
                                            recog
                                                .base
                                                .match_token(Cypher_SP, &mut recog.err_handler)?;
                                        }
                                    }

                                    /*InvokeRule oC_Expression*/
                                    recog.base.set_state(1683);
                                    recog.oC_Expression()?;

                                    recog.base.set_state(1685);
                                    recog.err_handler.sync(&mut recog.base)?;
                                    _la = recog.base.input.la(1);
                                    if _la == Cypher_SP {
                                        {
                                            recog.base.set_state(1684);
                                            recog
                                                .base
                                                .match_token(Cypher_SP, &mut recog.err_handler)?;
                                        }
                                    }
                                }
                            }
                            recog.base.set_state(1691);
                            recog.err_handler.sync(&mut recog.base)?;
                            _la = recog.base.input.la(1);
                        }
                    }
                }

                recog.base.set_state(1694);
                recog
                    .base
                    .match_token(Cypher_T__24, &mut recog.err_handler)?;
            }
            Ok(())
        })();
        match result {
            Ok(_) => {}
            Err(e @ ANTLRError::FallThrough(_)) => return Err(e),
            Err(ref re) => {
                //_localctx.exception = re;
                recog.err_handler.report_error(&mut recog.base, re);
                recog.err_handler.recover(&mut recog.base, re)?;
            }
        }
        recog.base.exit_rule()?;

        Ok(_localctx)
    }
}
//------------------- oC_PropertyKeyName ----------------
pub type OC_PropertyKeyNameContextAll<'input> = OC_PropertyKeyNameContext<'input>;

pub type OC_PropertyKeyNameContext<'input> =
    BaseParserRuleContext<'input, OC_PropertyKeyNameContextExt<'input>>;

#[derive(Clone)]
pub struct OC_PropertyKeyNameContextExt<'input> {
    ph: PhantomData<&'input str>,
}

impl<'input> CypherParserContext<'input> for OC_PropertyKeyNameContext<'input> {}

impl<'input, 'a> Listenable<dyn CypherListener<'input> + 'a> for OC_PropertyKeyNameContext<'input> {
    fn enter(&self, listener: &mut (dyn CypherListener<'input> + 'a)) -> Result<(), ANTLRError> {
        listener.enter_every_rule(self)?;
        listener.enter_oC_PropertyKeyName(self);
        Ok(())
    }
    fn exit(&self, listener: &mut (dyn CypherListener<'input> + 'a)) -> Result<(), ANTLRError> {
        listener.exit_oC_PropertyKeyName(self);
        listener.exit_every_rule(self)?;
        Ok(())
    }
}

impl<'input, 'a> Visitable<dyn CypherVisitor<'input> + 'a> for OC_PropertyKeyNameContext<'input> {
    fn accept(&self, visitor: &mut (dyn CypherVisitor<'input> + 'a)) {
        visitor.visit_oC_PropertyKeyName(self);
    }
}

impl<'input> CustomRuleContext<'input> for OC_PropertyKeyNameContextExt<'input> {
    type TF = LocalTokenFactory<'input>;
    type Ctx = CypherParserContextType;
    fn get_rule_index(&self) -> usize {
        RULE_oC_PropertyKeyName
    }
    //fn type_rule_index() -> usize where Self: Sized { RULE_oC_PropertyKeyName }
}
antlr4rust::tid! {OC_PropertyKeyNameContextExt<'a>}

impl<'input> OC_PropertyKeyNameContextExt<'input> {
    fn new(
        parent: Option<Rc<dyn CypherParserContext<'input> + 'input>>,
        invoking_state: i32,
    ) -> Rc<OC_PropertyKeyNameContextAll<'input>> {
        Rc::new(BaseParserRuleContext::new_parser_ctx(
            parent,
            invoking_state,
            OC_PropertyKeyNameContextExt { ph: PhantomData },
        ))
    }
}

pub trait OC_PropertyKeyNameContextAttrs<'input>:
    CypherParserContext<'input> + BorrowMut<OC_PropertyKeyNameContextExt<'input>>
{
    fn oC_SchemaName(&self) -> Option<Rc<OC_SchemaNameContextAll<'input>>>
    where
        Self: Sized,
    {
        self.child_of_type(0)
    }
}

impl<'input> OC_PropertyKeyNameContextAttrs<'input> for OC_PropertyKeyNameContext<'input> {}

impl<'input, I> CypherParser<'input, I>
where
    I: TokenStream<'input, TF = LocalTokenFactory<'input>> + TidAble<'input>,
{
    pub fn oC_PropertyKeyName(
        &mut self,
    ) -> Result<Rc<OC_PropertyKeyNameContextAll<'input>>, ANTLRError> {
        let mut recog = self;
        let _parentctx = recog.ctx.take();
        let mut _localctx =
            OC_PropertyKeyNameContextExt::new(_parentctx.clone(), recog.base.get_state());
        recog
            .base
            .enter_rule(_localctx.clone(), 202, RULE_oC_PropertyKeyName);
        let mut _localctx: Rc<OC_PropertyKeyNameContextAll> = _localctx;
        let result: Result<(), ANTLRError> = (|| {
            //recog.base.enter_outer_alt(_localctx.clone(), 1)?;
            recog.base.enter_outer_alt(None, 1)?;
            {
                /*InvokeRule oC_SchemaName*/
                recog.base.set_state(1696);
                recog.oC_SchemaName()?;
            }
            Ok(())
        })();
        match result {
            Ok(_) => {}
            Err(e @ ANTLRError::FallThrough(_)) => return Err(e),
            Err(ref re) => {
                //_localctx.exception = re;
                recog.err_handler.report_error(&mut recog.base, re);
                recog.err_handler.recover(&mut recog.base, re)?;
            }
        }
        recog.base.exit_rule()?;

        Ok(_localctx)
    }
}
//------------------- oC_Parameter ----------------
pub type OC_ParameterContextAll<'input> = OC_ParameterContext<'input>;

pub type OC_ParameterContext<'input> =
    BaseParserRuleContext<'input, OC_ParameterContextExt<'input>>;

#[derive(Clone)]
pub struct OC_ParameterContextExt<'input> {
    ph: PhantomData<&'input str>,
}

impl<'input> CypherParserContext<'input> for OC_ParameterContext<'input> {}

impl<'input, 'a> Listenable<dyn CypherListener<'input> + 'a> for OC_ParameterContext<'input> {
    fn enter(&self, listener: &mut (dyn CypherListener<'input> + 'a)) -> Result<(), ANTLRError> {
        listener.enter_every_rule(self)?;
        listener.enter_oC_Parameter(self);
        Ok(())
    }
    fn exit(&self, listener: &mut (dyn CypherListener<'input> + 'a)) -> Result<(), ANTLRError> {
        listener.exit_oC_Parameter(self);
        listener.exit_every_rule(self)?;
        Ok(())
    }
}

impl<'input, 'a> Visitable<dyn CypherVisitor<'input> + 'a> for OC_ParameterContext<'input> {
    fn accept(&self, visitor: &mut (dyn CypherVisitor<'input> + 'a)) {
        visitor.visit_oC_Parameter(self);
    }
}

impl<'input> CustomRuleContext<'input> for OC_ParameterContextExt<'input> {
    type TF = LocalTokenFactory<'input>;
    type Ctx = CypherParserContextType;
    fn get_rule_index(&self) -> usize {
        RULE_oC_Parameter
    }
    //fn type_rule_index() -> usize where Self: Sized { RULE_oC_Parameter }
}
antlr4rust::tid! {OC_ParameterContextExt<'a>}

impl<'input> OC_ParameterContextExt<'input> {
    fn new(
        parent: Option<Rc<dyn CypherParserContext<'input> + 'input>>,
        invoking_state: i32,
    ) -> Rc<OC_ParameterContextAll<'input>> {
        Rc::new(BaseParserRuleContext::new_parser_ctx(
            parent,
            invoking_state,
            OC_ParameterContextExt { ph: PhantomData },
        ))
    }
}

pub trait OC_ParameterContextAttrs<'input>:
    CypherParserContext<'input> + BorrowMut<OC_ParameterContextExt<'input>>
{
    fn oC_SymbolicName(&self) -> Option<Rc<OC_SymbolicNameContextAll<'input>>>
    where
        Self: Sized,
    {
        self.child_of_type(0)
    }
    /// Retrieves first TerminalNode corresponding to token DecimalInteger
    /// Returns `None` if there is no child corresponding to token DecimalInteger
    fn DecimalInteger(&self) -> Option<Rc<TerminalNode<'input, CypherParserContextType>>>
    where
        Self: Sized,
    {
        self.get_token(Cypher_DecimalInteger, 0)
    }
}

impl<'input> OC_ParameterContextAttrs<'input> for OC_ParameterContext<'input> {}

impl<'input, I> CypherParser<'input, I>
where
    I: TokenStream<'input, TF = LocalTokenFactory<'input>> + TidAble<'input>,
{
    pub fn oC_Parameter(&mut self) -> Result<Rc<OC_ParameterContextAll<'input>>, ANTLRError> {
        let mut recog = self;
        let _parentctx = recog.ctx.take();
        let mut _localctx = OC_ParameterContextExt::new(_parentctx.clone(), recog.base.get_state());
        recog
            .base
            .enter_rule(_localctx.clone(), 204, RULE_oC_Parameter);
        let mut _localctx: Rc<OC_ParameterContextAll> = _localctx;
        let result: Result<(), ANTLRError> = (|| {
            //recog.base.enter_outer_alt(_localctx.clone(), 1)?;
            recog.base.enter_outer_alt(None, 1)?;
            {
                recog.base.set_state(1698);
                recog
                    .base
                    .match_token(Cypher_T__25, &mut recog.err_handler)?;

                recog.base.set_state(1701);
                recog.err_handler.sync(&mut recog.base)?;
                match recog.base.input.la(1) {
                    Cypher_COUNT
                    | Cypher_ANY
                    | Cypher_NONE
                    | Cypher_SINGLE
                    | Cypher_HexLetter
                    | Cypher_FILTER
                    | Cypher_EXTRACT
                    | Cypher_UnescapedSymbolicName
                    | Cypher_EscapedSymbolicName => {
                        {
                            /*InvokeRule oC_SymbolicName*/
                            recog.base.set_state(1699);
                            recog.oC_SymbolicName()?;
                        }
                    }

                    Cypher_DecimalInteger => {
                        recog.base.set_state(1700);
                        recog
                            .base
                            .match_token(Cypher_DecimalInteger, &mut recog.err_handler)?;
                    }

                    _ => Err(ANTLRError::NoAltError(NoViableAltError::new(
                        &mut recog.base,
                    )))?,
                }
            }
            Ok(())
        })();
        match result {
            Ok(_) => {}
            Err(e @ ANTLRError::FallThrough(_)) => return Err(e),
            Err(ref re) => {
                //_localctx.exception = re;
                recog.err_handler.report_error(&mut recog.base, re);
                recog.err_handler.recover(&mut recog.base, re)?;
            }
        }
        recog.base.exit_rule()?;

        Ok(_localctx)
    }
}
//------------------- oC_SchemaName ----------------
pub type OC_SchemaNameContextAll<'input> = OC_SchemaNameContext<'input>;

pub type OC_SchemaNameContext<'input> =
    BaseParserRuleContext<'input, OC_SchemaNameContextExt<'input>>;

#[derive(Clone)]
pub struct OC_SchemaNameContextExt<'input> {
    ph: PhantomData<&'input str>,
}

impl<'input> CypherParserContext<'input> for OC_SchemaNameContext<'input> {}

impl<'input, 'a> Listenable<dyn CypherListener<'input> + 'a> for OC_SchemaNameContext<'input> {
    fn enter(&self, listener: &mut (dyn CypherListener<'input> + 'a)) -> Result<(), ANTLRError> {
        listener.enter_every_rule(self)?;
        listener.enter_oC_SchemaName(self);
        Ok(())
    }
    fn exit(&self, listener: &mut (dyn CypherListener<'input> + 'a)) -> Result<(), ANTLRError> {
        listener.exit_oC_SchemaName(self);
        listener.exit_every_rule(self)?;
        Ok(())
    }
}

impl<'input, 'a> Visitable<dyn CypherVisitor<'input> + 'a> for OC_SchemaNameContext<'input> {
    fn accept(&self, visitor: &mut (dyn CypherVisitor<'input> + 'a)) {
        visitor.visit_oC_SchemaName(self);
    }
}

impl<'input> CustomRuleContext<'input> for OC_SchemaNameContextExt<'input> {
    type TF = LocalTokenFactory<'input>;
    type Ctx = CypherParserContextType;
    fn get_rule_index(&self) -> usize {
        RULE_oC_SchemaName
    }
    //fn type_rule_index() -> usize where Self: Sized { RULE_oC_SchemaName }
}
antlr4rust::tid! {OC_SchemaNameContextExt<'a>}

impl<'input> OC_SchemaNameContextExt<'input> {
    fn new(
        parent: Option<Rc<dyn CypherParserContext<'input> + 'input>>,
        invoking_state: i32,
    ) -> Rc<OC_SchemaNameContextAll<'input>> {
        Rc::new(BaseParserRuleContext::new_parser_ctx(
            parent,
            invoking_state,
            OC_SchemaNameContextExt { ph: PhantomData },
        ))
    }
}

pub trait OC_SchemaNameContextAttrs<'input>:
    CypherParserContext<'input> + BorrowMut<OC_SchemaNameContextExt<'input>>
{
    fn oC_SymbolicName(&self) -> Option<Rc<OC_SymbolicNameContextAll<'input>>>
    where
        Self: Sized,
    {
        self.child_of_type(0)
    }
    fn oC_ReservedWord(&self) -> Option<Rc<OC_ReservedWordContextAll<'input>>>
    where
        Self: Sized,
    {
        self.child_of_type(0)
    }
}

impl<'input> OC_SchemaNameContextAttrs<'input> for OC_SchemaNameContext<'input> {}

impl<'input, I> CypherParser<'input, I>
where
    I: TokenStream<'input, TF = LocalTokenFactory<'input>> + TidAble<'input>,
{
    pub fn oC_SchemaName(&mut self) -> Result<Rc<OC_SchemaNameContextAll<'input>>, ANTLRError> {
        let mut recog = self;
        let _parentctx = recog.ctx.take();
        let mut _localctx =
            OC_SchemaNameContextExt::new(_parentctx.clone(), recog.base.get_state());
        recog
            .base
            .enter_rule(_localctx.clone(), 206, RULE_oC_SchemaName);
        let mut _localctx: Rc<OC_SchemaNameContextAll> = _localctx;
        let result: Result<(), ANTLRError> = (|| {
            recog.base.set_state(1705);
            recog.err_handler.sync(&mut recog.base)?;
            match recog.base.input.la(1) {
                Cypher_COUNT
                | Cypher_ANY
                | Cypher_NONE
                | Cypher_SINGLE
                | Cypher_HexLetter
                | Cypher_FILTER
                | Cypher_EXTRACT
                | Cypher_UnescapedSymbolicName
                | Cypher_EscapedSymbolicName => {
                    //recog.base.enter_outer_alt(_localctx.clone(), 1)?;
                    recog.base.enter_outer_alt(None, 1)?;
                    {
                        /*InvokeRule oC_SymbolicName*/
                        recog.base.set_state(1703);
                        recog.oC_SymbolicName()?;
                    }
                }

                Cypher_UNION | Cypher_ALL | Cypher_OPTIONAL | Cypher_MATCH | Cypher_UNWIND
                | Cypher_AS | Cypher_MERGE | Cypher_ON | Cypher_CREATE | Cypher_SET
                | Cypher_DETACH | Cypher_DELETE | Cypher_REMOVE | Cypher_WITH | Cypher_RETURN
                | Cypher_DISTINCT | Cypher_ORDER | Cypher_BY | Cypher_L_SKIP | Cypher_LIMIT
                | Cypher_ASCENDING | Cypher_ASC | Cypher_DESCENDING | Cypher_DESC
                | Cypher_WHERE | Cypher_OR | Cypher_XOR | Cypher_AND | Cypher_NOT
                | Cypher_STARTS | Cypher_ENDS | Cypher_CONTAINS | Cypher_IN | Cypher_IS
                | Cypher_NULL | Cypher_CASE | Cypher_ELSE | Cypher_END | Cypher_WHEN
                | Cypher_THEN | Cypher_CAST | Cypher_EXISTS | Cypher_TRUE | Cypher_FALSE
                | Cypher_CONSTRAINT | Cypher_DO | Cypher_FOR | Cypher_REQUIRE | Cypher_UNIQUE
                | Cypher_MANDATORY | Cypher_SCALAR | Cypher_OF | Cypher_ADD | Cypher_DROP => {
                    //recog.base.enter_outer_alt(_localctx.clone(), 2)?;
                    recog.base.enter_outer_alt(None, 2)?;
                    {
                        /*InvokeRule oC_ReservedWord*/
                        recog.base.set_state(1704);
                        recog.oC_ReservedWord()?;
                    }
                }

                _ => Err(ANTLRError::NoAltError(NoViableAltError::new(
                    &mut recog.base,
                )))?,
            }
            Ok(())
        })();
        match result {
            Ok(_) => {}
            Err(e @ ANTLRError::FallThrough(_)) => return Err(e),
            Err(ref re) => {
                //_localctx.exception = re;
                recog.err_handler.report_error(&mut recog.base, re);
                recog.err_handler.recover(&mut recog.base, re)?;
            }
        }
        recog.base.exit_rule()?;

        Ok(_localctx)
    }
}
//------------------- oC_ReservedWord ----------------
pub type OC_ReservedWordContextAll<'input> = OC_ReservedWordContext<'input>;

pub type OC_ReservedWordContext<'input> =
    BaseParserRuleContext<'input, OC_ReservedWordContextExt<'input>>;

#[derive(Clone)]
pub struct OC_ReservedWordContextExt<'input> {
    ph: PhantomData<&'input str>,
}

impl<'input> CypherParserContext<'input> for OC_ReservedWordContext<'input> {}

impl<'input, 'a> Listenable<dyn CypherListener<'input> + 'a> for OC_ReservedWordContext<'input> {
    fn enter(&self, listener: &mut (dyn CypherListener<'input> + 'a)) -> Result<(), ANTLRError> {
        listener.enter_every_rule(self)?;
        listener.enter_oC_ReservedWord(self);
        Ok(())
    }
    fn exit(&self, listener: &mut (dyn CypherListener<'input> + 'a)) -> Result<(), ANTLRError> {
        listener.exit_oC_ReservedWord(self);
        listener.exit_every_rule(self)?;
        Ok(())
    }
}

impl<'input, 'a> Visitable<dyn CypherVisitor<'input> + 'a> for OC_ReservedWordContext<'input> {
    fn accept(&self, visitor: &mut (dyn CypherVisitor<'input> + 'a)) {
        visitor.visit_oC_ReservedWord(self);
    }
}

impl<'input> CustomRuleContext<'input> for OC_ReservedWordContextExt<'input> {
    type TF = LocalTokenFactory<'input>;
    type Ctx = CypherParserContextType;
    fn get_rule_index(&self) -> usize {
        RULE_oC_ReservedWord
    }
    //fn type_rule_index() -> usize where Self: Sized { RULE_oC_ReservedWord }
}
antlr4rust::tid! {OC_ReservedWordContextExt<'a>}

impl<'input> OC_ReservedWordContextExt<'input> {
    fn new(
        parent: Option<Rc<dyn CypherParserContext<'input> + 'input>>,
        invoking_state: i32,
    ) -> Rc<OC_ReservedWordContextAll<'input>> {
        Rc::new(BaseParserRuleContext::new_parser_ctx(
            parent,
            invoking_state,
            OC_ReservedWordContextExt { ph: PhantomData },
        ))
    }
}

pub trait OC_ReservedWordContextAttrs<'input>:
    CypherParserContext<'input> + BorrowMut<OC_ReservedWordContextExt<'input>>
{
    /// Retrieves first TerminalNode corresponding to token ALL
    /// Returns `None` if there is no child corresponding to token ALL
    fn ALL(&self) -> Option<Rc<TerminalNode<'input, CypherParserContextType>>>
    where
        Self: Sized,
    {
        self.get_token(Cypher_ALL, 0)
    }
    /// Retrieves first TerminalNode corresponding to token ASC
    /// Returns `None` if there is no child corresponding to token ASC
    fn ASC(&self) -> Option<Rc<TerminalNode<'input, CypherParserContextType>>>
    where
        Self: Sized,
    {
        self.get_token(Cypher_ASC, 0)
    }
    /// Retrieves first TerminalNode corresponding to token ASCENDING
    /// Returns `None` if there is no child corresponding to token ASCENDING
    fn ASCENDING(&self) -> Option<Rc<TerminalNode<'input, CypherParserContextType>>>
    where
        Self: Sized,
    {
        self.get_token(Cypher_ASCENDING, 0)
    }
    /// Retrieves first TerminalNode corresponding to token BY
    /// Returns `None` if there is no child corresponding to token BY
    fn BY(&self) -> Option<Rc<TerminalNode<'input, CypherParserContextType>>>
    where
        Self: Sized,
    {
        self.get_token(Cypher_BY, 0)
    }
    /// Retrieves first TerminalNode corresponding to token CREATE
    /// Returns `None` if there is no child corresponding to token CREATE
    fn CREATE(&self) -> Option<Rc<TerminalNode<'input, CypherParserContextType>>>
    where
        Self: Sized,
    {
        self.get_token(Cypher_CREATE, 0)
    }
    /// Retrieves first TerminalNode corresponding to token DELETE
    /// Returns `None` if there is no child corresponding to token DELETE
    fn DELETE(&self) -> Option<Rc<TerminalNode<'input, CypherParserContextType>>>
    where
        Self: Sized,
    {
        self.get_token(Cypher_DELETE, 0)
    }
    /// Retrieves first TerminalNode corresponding to token DESC
    /// Returns `None` if there is no child corresponding to token DESC
    fn DESC(&self) -> Option<Rc<TerminalNode<'input, CypherParserContextType>>>
    where
        Self: Sized,
    {
        self.get_token(Cypher_DESC, 0)
    }
    /// Retrieves first TerminalNode corresponding to token DESCENDING
    /// Returns `None` if there is no child corresponding to token DESCENDING
    fn DESCENDING(&self) -> Option<Rc<TerminalNode<'input, CypherParserContextType>>>
    where
        Self: Sized,
    {
        self.get_token(Cypher_DESCENDING, 0)
    }
    /// Retrieves first TerminalNode corresponding to token DETACH
    /// Returns `None` if there is no child corresponding to token DETACH
    fn DETACH(&self) -> Option<Rc<TerminalNode<'input, CypherParserContextType>>>
    where
        Self: Sized,
    {
        self.get_token(Cypher_DETACH, 0)
    }
    /// Retrieves first TerminalNode corresponding to token EXISTS
    /// Returns `None` if there is no child corresponding to token EXISTS
    fn EXISTS(&self) -> Option<Rc<TerminalNode<'input, CypherParserContextType>>>
    where
        Self: Sized,
    {
        self.get_token(Cypher_EXISTS, 0)
    }
    /// Retrieves first TerminalNode corresponding to token LIMIT
    /// Returns `None` if there is no child corresponding to token LIMIT
    fn LIMIT(&self) -> Option<Rc<TerminalNode<'input, CypherParserContextType>>>
    where
        Self: Sized,
    {
        self.get_token(Cypher_LIMIT, 0)
    }
    /// Retrieves first TerminalNode corresponding to token MATCH
    /// Returns `None` if there is no child corresponding to token MATCH
    fn MATCH(&self) -> Option<Rc<TerminalNode<'input, CypherParserContextType>>>
    where
        Self: Sized,
    {
        self.get_token(Cypher_MATCH, 0)
    }
    /// Retrieves first TerminalNode corresponding to token MERGE
    /// Returns `None` if there is no child corresponding to token MERGE
    fn MERGE(&self) -> Option<Rc<TerminalNode<'input, CypherParserContextType>>>
    where
        Self: Sized,
    {
        self.get_token(Cypher_MERGE, 0)
    }
    /// Retrieves first TerminalNode corresponding to token ON
    /// Returns `None` if there is no child corresponding to token ON
    fn ON(&self) -> Option<Rc<TerminalNode<'input, CypherParserContextType>>>
    where
        Self: Sized,
    {
        self.get_token(Cypher_ON, 0)
    }
    /// Retrieves first TerminalNode corresponding to token OPTIONAL
    /// Returns `None` if there is no child corresponding to token OPTIONAL
    fn OPTIONAL(&self) -> Option<Rc<TerminalNode<'input, CypherParserContextType>>>
    where
        Self: Sized,
    {
        self.get_token(Cypher_OPTIONAL, 0)
    }
    /// Retrieves first TerminalNode corresponding to token ORDER
    /// Returns `None` if there is no child corresponding to token ORDER
    fn ORDER(&self) -> Option<Rc<TerminalNode<'input, CypherParserContextType>>>
    where
        Self: Sized,
    {
        self.get_token(Cypher_ORDER, 0)
    }
    /// Retrieves first TerminalNode corresponding to token REMOVE
    /// Returns `None` if there is no child corresponding to token REMOVE
    fn REMOVE(&self) -> Option<Rc<TerminalNode<'input, CypherParserContextType>>>
    where
        Self: Sized,
    {
        self.get_token(Cypher_REMOVE, 0)
    }
    /// Retrieves first TerminalNode corresponding to token RETURN
    /// Returns `None` if there is no child corresponding to token RETURN
    fn RETURN(&self) -> Option<Rc<TerminalNode<'input, CypherParserContextType>>>
    where
        Self: Sized,
    {
        self.get_token(Cypher_RETURN, 0)
    }
    /// Retrieves first TerminalNode corresponding to token SET
    /// Returns `None` if there is no child corresponding to token SET
    fn SET(&self) -> Option<Rc<TerminalNode<'input, CypherParserContextType>>>
    where
        Self: Sized,
    {
        self.get_token(Cypher_SET, 0)
    }
    /// Retrieves first TerminalNode corresponding to token L_SKIP
    /// Returns `None` if there is no child corresponding to token L_SKIP
    fn L_SKIP(&self) -> Option<Rc<TerminalNode<'input, CypherParserContextType>>>
    where
        Self: Sized,
    {
        self.get_token(Cypher_L_SKIP, 0)
    }
    /// Retrieves first TerminalNode corresponding to token WHERE
    /// Returns `None` if there is no child corresponding to token WHERE
    fn WHERE(&self) -> Option<Rc<TerminalNode<'input, CypherParserContextType>>>
    where
        Self: Sized,
    {
        self.get_token(Cypher_WHERE, 0)
    }
    /// Retrieves first TerminalNode corresponding to token WITH
    /// Returns `None` if there is no child corresponding to token WITH
    fn WITH(&self) -> Option<Rc<TerminalNode<'input, CypherParserContextType>>>
    where
        Self: Sized,
    {
        self.get_token(Cypher_WITH, 0)
    }
    /// Retrieves first TerminalNode corresponding to token UNION
    /// Returns `None` if there is no child corresponding to token UNION
    fn UNION(&self) -> Option<Rc<TerminalNode<'input, CypherParserContextType>>>
    where
        Self: Sized,
    {
        self.get_token(Cypher_UNION, 0)
    }
    /// Retrieves first TerminalNode corresponding to token UNWIND
    /// Returns `None` if there is no child corresponding to token UNWIND
    fn UNWIND(&self) -> Option<Rc<TerminalNode<'input, CypherParserContextType>>>
    where
        Self: Sized,
    {
        self.get_token(Cypher_UNWIND, 0)
    }
    /// Retrieves first TerminalNode corresponding to token AND
    /// Returns `None` if there is no child corresponding to token AND
    fn AND(&self) -> Option<Rc<TerminalNode<'input, CypherParserContextType>>>
    where
        Self: Sized,
    {
        self.get_token(Cypher_AND, 0)
    }
    /// Retrieves first TerminalNode corresponding to token AS
    /// Returns `None` if there is no child corresponding to token AS
    fn AS(&self) -> Option<Rc<TerminalNode<'input, CypherParserContextType>>>
    where
        Self: Sized,
    {
        self.get_token(Cypher_AS, 0)
    }
    /// Retrieves first TerminalNode corresponding to token CONTAINS
    /// Returns `None` if there is no child corresponding to token CONTAINS
    fn CONTAINS(&self) -> Option<Rc<TerminalNode<'input, CypherParserContextType>>>
    where
        Self: Sized,
    {
        self.get_token(Cypher_CONTAINS, 0)
    }
    /// Retrieves first TerminalNode corresponding to token DISTINCT
    /// Returns `None` if there is no child corresponding to token DISTINCT
    fn DISTINCT(&self) -> Option<Rc<TerminalNode<'input, CypherParserContextType>>>
    where
        Self: Sized,
    {
        self.get_token(Cypher_DISTINCT, 0)
    }
    /// Retrieves first TerminalNode corresponding to token ENDS
    /// Returns `None` if there is no child corresponding to token ENDS
    fn ENDS(&self) -> Option<Rc<TerminalNode<'input, CypherParserContextType>>>
    where
        Self: Sized,
    {
        self.get_token(Cypher_ENDS, 0)
    }
    /// Retrieves first TerminalNode corresponding to token IN
    /// Returns `None` if there is no child corresponding to token IN
    fn IN(&self) -> Option<Rc<TerminalNode<'input, CypherParserContextType>>>
    where
        Self: Sized,
    {
        self.get_token(Cypher_IN, 0)
    }
    /// Retrieves first TerminalNode corresponding to token IS
    /// Returns `None` if there is no child corresponding to token IS
    fn IS(&self) -> Option<Rc<TerminalNode<'input, CypherParserContextType>>>
    where
        Self: Sized,
    {
        self.get_token(Cypher_IS, 0)
    }
    /// Retrieves first TerminalNode corresponding to token NOT
    /// Returns `None` if there is no child corresponding to token NOT
    fn NOT(&self) -> Option<Rc<TerminalNode<'input, CypherParserContextType>>>
    where
        Self: Sized,
    {
        self.get_token(Cypher_NOT, 0)
    }
    /// Retrieves first TerminalNode corresponding to token OR
    /// Returns `None` if there is no child corresponding to token OR
    fn OR(&self) -> Option<Rc<TerminalNode<'input, CypherParserContextType>>>
    where
        Self: Sized,
    {
        self.get_token(Cypher_OR, 0)
    }
    /// Retrieves first TerminalNode corresponding to token STARTS
    /// Returns `None` if there is no child corresponding to token STARTS
    fn STARTS(&self) -> Option<Rc<TerminalNode<'input, CypherParserContextType>>>
    where
        Self: Sized,
    {
        self.get_token(Cypher_STARTS, 0)
    }
    /// Retrieves first TerminalNode corresponding to token XOR
    /// Returns `None` if there is no child corresponding to token XOR
    fn XOR(&self) -> Option<Rc<TerminalNode<'input, CypherParserContextType>>>
    where
        Self: Sized,
    {
        self.get_token(Cypher_XOR, 0)
    }
    /// Retrieves first TerminalNode corresponding to token FALSE
    /// Returns `None` if there is no child corresponding to token FALSE
    fn FALSE(&self) -> Option<Rc<TerminalNode<'input, CypherParserContextType>>>
    where
        Self: Sized,
    {
        self.get_token(Cypher_FALSE, 0)
    }
    /// Retrieves first TerminalNode corresponding to token TRUE
    /// Returns `None` if there is no child corresponding to token TRUE
    fn TRUE(&self) -> Option<Rc<TerminalNode<'input, CypherParserContextType>>>
    where
        Self: Sized,
    {
        self.get_token(Cypher_TRUE, 0)
    }
    /// Retrieves first TerminalNode corresponding to token NULL
    /// Returns `None` if there is no child corresponding to token NULL
    fn NULL(&self) -> Option<Rc<TerminalNode<'input, CypherParserContextType>>>
    where
        Self: Sized,
    {
        self.get_token(Cypher_NULL, 0)
    }
    /// Retrieves first TerminalNode corresponding to token CONSTRAINT
    /// Returns `None` if there is no child corresponding to token CONSTRAINT
    fn CONSTRAINT(&self) -> Option<Rc<TerminalNode<'input, CypherParserContextType>>>
    where
        Self: Sized,
    {
        self.get_token(Cypher_CONSTRAINT, 0)
    }
    /// Retrieves first TerminalNode corresponding to token CAST
    /// Returns `None` if there is no child corresponding to token CAST
    fn CAST(&self) -> Option<Rc<TerminalNode<'input, CypherParserContextType>>>
    where
        Self: Sized,
    {
        self.get_token(Cypher_CAST, 0)
    }
    /// Retrieves first TerminalNode corresponding to token DO
    /// Returns `None` if there is no child corresponding to token DO
    fn DO(&self) -> Option<Rc<TerminalNode<'input, CypherParserContextType>>>
    where
        Self: Sized,
    {
        self.get_token(Cypher_DO, 0)
    }
    /// Retrieves first TerminalNode corresponding to token FOR
    /// Returns `None` if there is no child corresponding to token FOR
    fn FOR(&self) -> Option<Rc<TerminalNode<'input, CypherParserContextType>>>
    where
        Self: Sized,
    {
        self.get_token(Cypher_FOR, 0)
    }
    /// Retrieves first TerminalNode corresponding to token REQUIRE
    /// Returns `None` if there is no child corresponding to token REQUIRE
    fn REQUIRE(&self) -> Option<Rc<TerminalNode<'input, CypherParserContextType>>>
    where
        Self: Sized,
    {
        self.get_token(Cypher_REQUIRE, 0)
    }
    /// Retrieves first TerminalNode corresponding to token UNIQUE
    /// Returns `None` if there is no child corresponding to token UNIQUE
    fn UNIQUE(&self) -> Option<Rc<TerminalNode<'input, CypherParserContextType>>>
    where
        Self: Sized,
    {
        self.get_token(Cypher_UNIQUE, 0)
    }
    /// Retrieves first TerminalNode corresponding to token CASE
    /// Returns `None` if there is no child corresponding to token CASE
    fn CASE(&self) -> Option<Rc<TerminalNode<'input, CypherParserContextType>>>
    where
        Self: Sized,
    {
        self.get_token(Cypher_CASE, 0)
    }
    /// Retrieves first TerminalNode corresponding to token WHEN
    /// Returns `None` if there is no child corresponding to token WHEN
    fn WHEN(&self) -> Option<Rc<TerminalNode<'input, CypherParserContextType>>>
    where
        Self: Sized,
    {
        self.get_token(Cypher_WHEN, 0)
    }
    /// Retrieves first TerminalNode corresponding to token THEN
    /// Returns `None` if there is no child corresponding to token THEN
    fn THEN(&self) -> Option<Rc<TerminalNode<'input, CypherParserContextType>>>
    where
        Self: Sized,
    {
        self.get_token(Cypher_THEN, 0)
    }
    /// Retrieves first TerminalNode corresponding to token ELSE
    /// Returns `None` if there is no child corresponding to token ELSE
    fn ELSE(&self) -> Option<Rc<TerminalNode<'input, CypherParserContextType>>>
    where
        Self: Sized,
    {
        self.get_token(Cypher_ELSE, 0)
    }
    /// Retrieves first TerminalNode corresponding to token END
    /// Returns `None` if there is no child corresponding to token END
    fn END(&self) -> Option<Rc<TerminalNode<'input, CypherParserContextType>>>
    where
        Self: Sized,
    {
        self.get_token(Cypher_END, 0)
    }
    /// Retrieves first TerminalNode corresponding to token MANDATORY
    /// Returns `None` if there is no child corresponding to token MANDATORY
    fn MANDATORY(&self) -> Option<Rc<TerminalNode<'input, CypherParserContextType>>>
    where
        Self: Sized,
    {
        self.get_token(Cypher_MANDATORY, 0)
    }
    /// Retrieves first TerminalNode corresponding to token SCALAR
    /// Returns `None` if there is no child corresponding to token SCALAR
    fn SCALAR(&self) -> Option<Rc<TerminalNode<'input, CypherParserContextType>>>
    where
        Self: Sized,
    {
        self.get_token(Cypher_SCALAR, 0)
    }
    /// Retrieves first TerminalNode corresponding to token OF
    /// Returns `None` if there is no child corresponding to token OF
    fn OF(&self) -> Option<Rc<TerminalNode<'input, CypherParserContextType>>>
    where
        Self: Sized,
    {
        self.get_token(Cypher_OF, 0)
    }
    /// Retrieves first TerminalNode corresponding to token ADD
    /// Returns `None` if there is no child corresponding to token ADD
    fn ADD(&self) -> Option<Rc<TerminalNode<'input, CypherParserContextType>>>
    where
        Self: Sized,
    {
        self.get_token(Cypher_ADD, 0)
    }
    /// Retrieves first TerminalNode corresponding to token DROP
    /// Returns `None` if there is no child corresponding to token DROP
    fn DROP(&self) -> Option<Rc<TerminalNode<'input, CypherParserContextType>>>
    where
        Self: Sized,
    {
        self.get_token(Cypher_DROP, 0)
    }
}

impl<'input> OC_ReservedWordContextAttrs<'input> for OC_ReservedWordContext<'input> {}

impl<'input, I> CypherParser<'input, I>
where
    I: TokenStream<'input, TF = LocalTokenFactory<'input>> + TidAble<'input>,
{
    pub fn oC_ReservedWord(&mut self) -> Result<Rc<OC_ReservedWordContextAll<'input>>, ANTLRError> {
        let mut recog = self;
        let _parentctx = recog.ctx.take();
        let mut _localctx =
            OC_ReservedWordContextExt::new(_parentctx.clone(), recog.base.get_state());
        recog
            .base
            .enter_rule(_localctx.clone(), 208, RULE_oC_ReservedWord);
        let mut _localctx: Rc<OC_ReservedWordContextAll> = _localctx;
        let mut _la: i32 = -1;
        let result: Result<(), ANTLRError> = (|| {
            //recog.base.enter_outer_alt(_localctx.clone(), 1)?;
            recog.base.enter_outer_alt(None, 1)?;
            {
                recog.base.set_state(1707);
                _la = recog.base.input.la(1);
                if {
                    !((((_la - 46) & !0x3f) == 0 && ((1usize << (_la - 46)) & 4294942719) != 0)
                        || (((_la - 78) & !0x3f) == 0 && ((1usize << (_la - 78)) & 247775) != 0)
                        || (((_la - 110) & !0x3f) == 0 && ((1usize << (_la - 110)) & 1023) != 0))
                } {
                    recog.err_handler.recover_inline(&mut recog.base)?;
                } else {
                    if recog.base.input.la(1) == TOKEN_EOF {
                        recog.base.matched_eof = true
                    };
                    recog.err_handler.report_match(&mut recog.base);
                    recog.base.consume(&mut recog.err_handler);
                }
            }
            Ok(())
        })();
        match result {
            Ok(_) => {}
            Err(e @ ANTLRError::FallThrough(_)) => return Err(e),
            Err(ref re) => {
                //_localctx.exception = re;
                recog.err_handler.report_error(&mut recog.base, re);
                recog.err_handler.recover(&mut recog.base, re)?;
            }
        }
        recog.base.exit_rule()?;

        Ok(_localctx)
    }
}
//------------------- oC_SymbolicName ----------------
pub type OC_SymbolicNameContextAll<'input> = OC_SymbolicNameContext<'input>;

pub type OC_SymbolicNameContext<'input> =
    BaseParserRuleContext<'input, OC_SymbolicNameContextExt<'input>>;

#[derive(Clone)]
pub struct OC_SymbolicNameContextExt<'input> {
    ph: PhantomData<&'input str>,
}

impl<'input> CypherParserContext<'input> for OC_SymbolicNameContext<'input> {}

impl<'input, 'a> Listenable<dyn CypherListener<'input> + 'a> for OC_SymbolicNameContext<'input> {
    fn enter(&self, listener: &mut (dyn CypherListener<'input> + 'a)) -> Result<(), ANTLRError> {
        listener.enter_every_rule(self)?;
        listener.enter_oC_SymbolicName(self);
        Ok(())
    }
    fn exit(&self, listener: &mut (dyn CypherListener<'input> + 'a)) -> Result<(), ANTLRError> {
        listener.exit_oC_SymbolicName(self);
        listener.exit_every_rule(self)?;
        Ok(())
    }
}

impl<'input, 'a> Visitable<dyn CypherVisitor<'input> + 'a> for OC_SymbolicNameContext<'input> {
    fn accept(&self, visitor: &mut (dyn CypherVisitor<'input> + 'a)) {
        visitor.visit_oC_SymbolicName(self);
    }
}

impl<'input> CustomRuleContext<'input> for OC_SymbolicNameContextExt<'input> {
    type TF = LocalTokenFactory<'input>;
    type Ctx = CypherParserContextType;
    fn get_rule_index(&self) -> usize {
        RULE_oC_SymbolicName
    }
    //fn type_rule_index() -> usize where Self: Sized { RULE_oC_SymbolicName }
}
antlr4rust::tid! {OC_SymbolicNameContextExt<'a>}

impl<'input> OC_SymbolicNameContextExt<'input> {
    fn new(
        parent: Option<Rc<dyn CypherParserContext<'input> + 'input>>,
        invoking_state: i32,
    ) -> Rc<OC_SymbolicNameContextAll<'input>> {
        Rc::new(BaseParserRuleContext::new_parser_ctx(
            parent,
            invoking_state,
            OC_SymbolicNameContextExt { ph: PhantomData },
        ))
    }
}

pub trait OC_SymbolicNameContextAttrs<'input>:
    CypherParserContext<'input> + BorrowMut<OC_SymbolicNameContextExt<'input>>
{
    /// Retrieves first TerminalNode corresponding to token UnescapedSymbolicName
    /// Returns `None` if there is no child corresponding to token UnescapedSymbolicName
    fn UnescapedSymbolicName(&self) -> Option<Rc<TerminalNode<'input, CypherParserContextType>>>
    where
        Self: Sized,
    {
        self.get_token(Cypher_UnescapedSymbolicName, 0)
    }
    /// Retrieves first TerminalNode corresponding to token EscapedSymbolicName
    /// Returns `None` if there is no child corresponding to token EscapedSymbolicName
    fn EscapedSymbolicName(&self) -> Option<Rc<TerminalNode<'input, CypherParserContextType>>>
    where
        Self: Sized,
    {
        self.get_token(Cypher_EscapedSymbolicName, 0)
    }
    /// Retrieves first TerminalNode corresponding to token HexLetter
    /// Returns `None` if there is no child corresponding to token HexLetter
    fn HexLetter(&self) -> Option<Rc<TerminalNode<'input, CypherParserContextType>>>
    where
        Self: Sized,
    {
        self.get_token(Cypher_HexLetter, 0)
    }
    /// Retrieves first TerminalNode corresponding to token COUNT
    /// Returns `None` if there is no child corresponding to token COUNT
    fn COUNT(&self) -> Option<Rc<TerminalNode<'input, CypherParserContextType>>>
    where
        Self: Sized,
    {
        self.get_token(Cypher_COUNT, 0)
    }
    /// Retrieves first TerminalNode corresponding to token FILTER
    /// Returns `None` if there is no child corresponding to token FILTER
    fn FILTER(&self) -> Option<Rc<TerminalNode<'input, CypherParserContextType>>>
    where
        Self: Sized,
    {
        self.get_token(Cypher_FILTER, 0)
    }
    /// Retrieves first TerminalNode corresponding to token EXTRACT
    /// Returns `None` if there is no child corresponding to token EXTRACT
    fn EXTRACT(&self) -> Option<Rc<TerminalNode<'input, CypherParserContextType>>>
    where
        Self: Sized,
    {
        self.get_token(Cypher_EXTRACT, 0)
    }
    /// Retrieves first TerminalNode corresponding to token ANY
    /// Returns `None` if there is no child corresponding to token ANY
    fn ANY(&self) -> Option<Rc<TerminalNode<'input, CypherParserContextType>>>
    where
        Self: Sized,
    {
        self.get_token(Cypher_ANY, 0)
    }
    /// Retrieves first TerminalNode corresponding to token NONE
    /// Returns `None` if there is no child corresponding to token NONE
    fn NONE(&self) -> Option<Rc<TerminalNode<'input, CypherParserContextType>>>
    where
        Self: Sized,
    {
        self.get_token(Cypher_NONE, 0)
    }
    /// Retrieves first TerminalNode corresponding to token SINGLE
    /// Returns `None` if there is no child corresponding to token SINGLE
    fn SINGLE(&self) -> Option<Rc<TerminalNode<'input, CypherParserContextType>>>
    where
        Self: Sized,
    {
        self.get_token(Cypher_SINGLE, 0)
    }
}

impl<'input> OC_SymbolicNameContextAttrs<'input> for OC_SymbolicNameContext<'input> {}

impl<'input, I> CypherParser<'input, I>
where
    I: TokenStream<'input, TF = LocalTokenFactory<'input>> + TidAble<'input>,
{
    pub fn oC_SymbolicName(&mut self) -> Result<Rc<OC_SymbolicNameContextAll<'input>>, ANTLRError> {
        let mut recog = self;
        let _parentctx = recog.ctx.take();
        let mut _localctx =
            OC_SymbolicNameContextExt::new(_parentctx.clone(), recog.base.get_state());
        recog
            .base
            .enter_rule(_localctx.clone(), 210, RULE_oC_SymbolicName);
        let mut _localctx: Rc<OC_SymbolicNameContextAll> = _localctx;
        let mut _la: i32 = -1;
        let result: Result<(), ANTLRError> = (|| {
            //recog.base.enter_outer_alt(_localctx.clone(), 1)?;
            recog.base.enter_outer_alt(None, 1)?;
            {
                recog.base.set_state(1709);
                _la = recog.base.input.la(1);
                if {
                    !((((_la - 83) & !0x3f) == 0 && ((1usize << (_la - 83)) & 65985) != 0)
                        || (((_la - 120) & !0x3f) == 0 && ((1usize << (_la - 120)) & 39) != 0))
                } {
                    recog.err_handler.recover_inline(&mut recog.base)?;
                } else {
                    if recog.base.input.la(1) == TOKEN_EOF {
                        recog.base.matched_eof = true
                    };
                    recog.err_handler.report_match(&mut recog.base);
                    recog.base.consume(&mut recog.err_handler);
                }
            }
            Ok(())
        })();
        match result {
            Ok(_) => {}
            Err(e @ ANTLRError::FallThrough(_)) => return Err(e),
            Err(ref re) => {
                //_localctx.exception = re;
                recog.err_handler.report_error(&mut recog.base, re);
                recog.err_handler.recover(&mut recog.base, re)?;
            }
        }
        recog.base.exit_rule()?;

        Ok(_localctx)
    }
}
//------------------- oC_LeftArrowHead ----------------
pub type OC_LeftArrowHeadContextAll<'input> = OC_LeftArrowHeadContext<'input>;

pub type OC_LeftArrowHeadContext<'input> =
    BaseParserRuleContext<'input, OC_LeftArrowHeadContextExt<'input>>;

#[derive(Clone)]
pub struct OC_LeftArrowHeadContextExt<'input> {
    ph: PhantomData<&'input str>,
}

impl<'input> CypherParserContext<'input> for OC_LeftArrowHeadContext<'input> {}

impl<'input, 'a> Listenable<dyn CypherListener<'input> + 'a> for OC_LeftArrowHeadContext<'input> {
    fn enter(&self, listener: &mut (dyn CypherListener<'input> + 'a)) -> Result<(), ANTLRError> {
        listener.enter_every_rule(self)?;
        listener.enter_oC_LeftArrowHead(self);
        Ok(())
    }
    fn exit(&self, listener: &mut (dyn CypherListener<'input> + 'a)) -> Result<(), ANTLRError> {
        listener.exit_oC_LeftArrowHead(self);
        listener.exit_every_rule(self)?;
        Ok(())
    }
}

impl<'input, 'a> Visitable<dyn CypherVisitor<'input> + 'a> for OC_LeftArrowHeadContext<'input> {
    fn accept(&self, visitor: &mut (dyn CypherVisitor<'input> + 'a)) {
        visitor.visit_oC_LeftArrowHead(self);
    }
}

impl<'input> CustomRuleContext<'input> for OC_LeftArrowHeadContextExt<'input> {
    type TF = LocalTokenFactory<'input>;
    type Ctx = CypherParserContextType;
    fn get_rule_index(&self) -> usize {
        RULE_oC_LeftArrowHead
    }
    //fn type_rule_index() -> usize where Self: Sized { RULE_oC_LeftArrowHead }
}
antlr4rust::tid! {OC_LeftArrowHeadContextExt<'a>}

impl<'input> OC_LeftArrowHeadContextExt<'input> {
    fn new(
        parent: Option<Rc<dyn CypherParserContext<'input> + 'input>>,
        invoking_state: i32,
    ) -> Rc<OC_LeftArrowHeadContextAll<'input>> {
        Rc::new(BaseParserRuleContext::new_parser_ctx(
            parent,
            invoking_state,
            OC_LeftArrowHeadContextExt { ph: PhantomData },
        ))
    }
}

pub trait OC_LeftArrowHeadContextAttrs<'input>:
    CypherParserContext<'input> + BorrowMut<OC_LeftArrowHeadContextExt<'input>>
{
}

impl<'input> OC_LeftArrowHeadContextAttrs<'input> for OC_LeftArrowHeadContext<'input> {}

impl<'input, I> CypherParser<'input, I>
where
    I: TokenStream<'input, TF = LocalTokenFactory<'input>> + TidAble<'input>,
{
    pub fn oC_LeftArrowHead(
        &mut self,
    ) -> Result<Rc<OC_LeftArrowHeadContextAll<'input>>, ANTLRError> {
        let mut recog = self;
        let _parentctx = recog.ctx.take();
        let mut _localctx =
            OC_LeftArrowHeadContextExt::new(_parentctx.clone(), recog.base.get_state());
        recog
            .base
            .enter_rule(_localctx.clone(), 212, RULE_oC_LeftArrowHead);
        let mut _localctx: Rc<OC_LeftArrowHeadContextAll> = _localctx;
        let mut _la: i32 = -1;
        let result: Result<(), ANTLRError> = (|| {
            //recog.base.enter_outer_alt(_localctx.clone(), 1)?;
            recog.base.enter_outer_alt(None, 1)?;
            {
                recog.base.set_state(1711);
                _la = recog.base.input.la(1);
                if { !(((_la) & !0x3f) == 0 && ((1usize << _la) & 2013282304) != 0) } {
                    recog.err_handler.recover_inline(&mut recog.base)?;
                } else {
                    if recog.base.input.la(1) == TOKEN_EOF {
                        recog.base.matched_eof = true
                    };
                    recog.err_handler.report_match(&mut recog.base);
                    recog.base.consume(&mut recog.err_handler);
                }
            }
            Ok(())
        })();
        match result {
            Ok(_) => {}
            Err(e @ ANTLRError::FallThrough(_)) => return Err(e),
            Err(ref re) => {
                //_localctx.exception = re;
                recog.err_handler.report_error(&mut recog.base, re);
                recog.err_handler.recover(&mut recog.base, re)?;
            }
        }
        recog.base.exit_rule()?;

        Ok(_localctx)
    }
}
//------------------- oC_RightArrowHead ----------------
pub type OC_RightArrowHeadContextAll<'input> = OC_RightArrowHeadContext<'input>;

pub type OC_RightArrowHeadContext<'input> =
    BaseParserRuleContext<'input, OC_RightArrowHeadContextExt<'input>>;

#[derive(Clone)]
pub struct OC_RightArrowHeadContextExt<'input> {
    ph: PhantomData<&'input str>,
}

impl<'input> CypherParserContext<'input> for OC_RightArrowHeadContext<'input> {}

impl<'input, 'a> Listenable<dyn CypherListener<'input> + 'a> for OC_RightArrowHeadContext<'input> {
    fn enter(&self, listener: &mut (dyn CypherListener<'input> + 'a)) -> Result<(), ANTLRError> {
        listener.enter_every_rule(self)?;
        listener.enter_oC_RightArrowHead(self);
        Ok(())
    }
    fn exit(&self, listener: &mut (dyn CypherListener<'input> + 'a)) -> Result<(), ANTLRError> {
        listener.exit_oC_RightArrowHead(self);
        listener.exit_every_rule(self)?;
        Ok(())
    }
}

impl<'input, 'a> Visitable<dyn CypherVisitor<'input> + 'a> for OC_RightArrowHeadContext<'input> {
    fn accept(&self, visitor: &mut (dyn CypherVisitor<'input> + 'a)) {
        visitor.visit_oC_RightArrowHead(self);
    }
}

impl<'input> CustomRuleContext<'input> for OC_RightArrowHeadContextExt<'input> {
    type TF = LocalTokenFactory<'input>;
    type Ctx = CypherParserContextType;
    fn get_rule_index(&self) -> usize {
        RULE_oC_RightArrowHead
    }
    //fn type_rule_index() -> usize where Self: Sized { RULE_oC_RightArrowHead }
}
antlr4rust::tid! {OC_RightArrowHeadContextExt<'a>}

impl<'input> OC_RightArrowHeadContextExt<'input> {
    fn new(
        parent: Option<Rc<dyn CypherParserContext<'input> + 'input>>,
        invoking_state: i32,
    ) -> Rc<OC_RightArrowHeadContextAll<'input>> {
        Rc::new(BaseParserRuleContext::new_parser_ctx(
            parent,
            invoking_state,
            OC_RightArrowHeadContextExt { ph: PhantomData },
        ))
    }
}

pub trait OC_RightArrowHeadContextAttrs<'input>:
    CypherParserContext<'input> + BorrowMut<OC_RightArrowHeadContextExt<'input>>
{
}

impl<'input> OC_RightArrowHeadContextAttrs<'input> for OC_RightArrowHeadContext<'input> {}

impl<'input, I> CypherParser<'input, I>
where
    I: TokenStream<'input, TF = LocalTokenFactory<'input>> + TidAble<'input>,
{
    pub fn oC_RightArrowHead(
        &mut self,
    ) -> Result<Rc<OC_RightArrowHeadContextAll<'input>>, ANTLRError> {
        let mut recog = self;
        let _parentctx = recog.ctx.take();
        let mut _localctx =
            OC_RightArrowHeadContextExt::new(_parentctx.clone(), recog.base.get_state());
        recog
            .base
            .enter_rule(_localctx.clone(), 214, RULE_oC_RightArrowHead);
        let mut _localctx: Rc<OC_RightArrowHeadContextAll> = _localctx;
        let mut _la: i32 = -1;
        let result: Result<(), ANTLRError> = (|| {
            //recog.base.enter_outer_alt(_localctx.clone(), 1)?;
            recog.base.enter_outer_alt(None, 1)?;
            {
                recog.base.set_state(1713);
                _la = recog.base.input.la(1);
                if { !(((_la - 15) & !0x3f) == 0 && ((1usize << (_la - 15)) & 983041) != 0) } {
                    recog.err_handler.recover_inline(&mut recog.base)?;
                } else {
                    if recog.base.input.la(1) == TOKEN_EOF {
                        recog.base.matched_eof = true
                    };
                    recog.err_handler.report_match(&mut recog.base);
                    recog.base.consume(&mut recog.err_handler);
                }
            }
            Ok(())
        })();
        match result {
            Ok(_) => {}
            Err(e @ ANTLRError::FallThrough(_)) => return Err(e),
            Err(ref re) => {
                //_localctx.exception = re;
                recog.err_handler.report_error(&mut recog.base, re);
                recog.err_handler.recover(&mut recog.base, re)?;
            }
        }
        recog.base.exit_rule()?;

        Ok(_localctx)
    }
}
//------------------- oC_Dash ----------------
pub type OC_DashContextAll<'input> = OC_DashContext<'input>;

pub type OC_DashContext<'input> = BaseParserRuleContext<'input, OC_DashContextExt<'input>>;

#[derive(Clone)]
pub struct OC_DashContextExt<'input> {
    ph: PhantomData<&'input str>,
}

impl<'input> CypherParserContext<'input> for OC_DashContext<'input> {}

impl<'input, 'a> Listenable<dyn CypherListener<'input> + 'a> for OC_DashContext<'input> {
    fn enter(&self, listener: &mut (dyn CypherListener<'input> + 'a)) -> Result<(), ANTLRError> {
        listener.enter_every_rule(self)?;
        listener.enter_oC_Dash(self);
        Ok(())
    }
    fn exit(&self, listener: &mut (dyn CypherListener<'input> + 'a)) -> Result<(), ANTLRError> {
        listener.exit_oC_Dash(self);
        listener.exit_every_rule(self)?;
        Ok(())
    }
}

impl<'input, 'a> Visitable<dyn CypherVisitor<'input> + 'a> for OC_DashContext<'input> {
    fn accept(&self, visitor: &mut (dyn CypherVisitor<'input> + 'a)) {
        visitor.visit_oC_Dash(self);
    }
}

impl<'input> CustomRuleContext<'input> for OC_DashContextExt<'input> {
    type TF = LocalTokenFactory<'input>;
    type Ctx = CypherParserContextType;
    fn get_rule_index(&self) -> usize {
        RULE_oC_Dash
    }
    //fn type_rule_index() -> usize where Self: Sized { RULE_oC_Dash }
}
antlr4rust::tid! {OC_DashContextExt<'a>}

impl<'input> OC_DashContextExt<'input> {
    fn new(
        parent: Option<Rc<dyn CypherParserContext<'input> + 'input>>,
        invoking_state: i32,
    ) -> Rc<OC_DashContextAll<'input>> {
        Rc::new(BaseParserRuleContext::new_parser_ctx(
            parent,
            invoking_state,
            OC_DashContextExt { ph: PhantomData },
        ))
    }
}

pub trait OC_DashContextAttrs<'input>:
    CypherParserContext<'input> + BorrowMut<OC_DashContextExt<'input>>
{
}

impl<'input> OC_DashContextAttrs<'input> for OC_DashContext<'input> {}

impl<'input, I> CypherParser<'input, I>
where
    I: TokenStream<'input, TF = LocalTokenFactory<'input>> + TidAble<'input>,
{
    pub fn oC_Dash(&mut self) -> Result<Rc<OC_DashContextAll<'input>>, ANTLRError> {
        let mut recog = self;
        let _parentctx = recog.ctx.take();
        let mut _localctx = OC_DashContextExt::new(_parentctx.clone(), recog.base.get_state());
        recog.base.enter_rule(_localctx.clone(), 216, RULE_oC_Dash);
        let mut _localctx: Rc<OC_DashContextAll> = _localctx;
        let mut _la: i32 = -1;
        let result: Result<(), ANTLRError> = (|| {
            //recog.base.enter_outer_alt(_localctx.clone(), 1)?;
            recog.base.enter_outer_alt(None, 1)?;
            {
                recog.base.set_state(1715);
                _la = recog.base.input.la(1);
                if { !(((_la - 19) & !0x3f) == 0 && ((1usize << (_la - 19)) & 134152193) != 0) } {
                    recog.err_handler.recover_inline(&mut recog.base)?;
                } else {
                    if recog.base.input.la(1) == TOKEN_EOF {
                        recog.base.matched_eof = true
                    };
                    recog.err_handler.report_match(&mut recog.base);
                    recog.base.consume(&mut recog.err_handler);
                }
            }
            Ok(())
        })();
        match result {
            Ok(_) => {}
            Err(e @ ANTLRError::FallThrough(_)) => return Err(e),
            Err(ref re) => {
                //_localctx.exception = re;
                recog.err_handler.report_error(&mut recog.base, re);
                recog.err_handler.recover(&mut recog.base, re)?;
            }
        }
        recog.base.exit_rule()?;

        Ok(_localctx)
    }
}
lazy_static! {
    static ref _ATN: Arc<ATN> =
        Arc::new(ATNDeserializer::new(None).deserialize(&mut _serializedATN.iter()));
    static ref _decision_to_DFA: Arc<Vec<antlr4rust::RwLock<DFA>>> = {
        let mut dfa = Vec::new();
        let size = _ATN.decision_to_state.len() as i32;
        for i in 0..size {
            dfa.push(DFA::new(_ATN.clone(), _ATN.get_decision_state(i), i).into())
        }
        Arc::new(dfa)
    };
    static ref _serializedATN: Vec<i32> = vec![
        4, 1, 128, 1718, 2, 0, 7, 0, 2, 1, 7, 1, 2, 2, 7, 2, 2, 3, 7, 3, 2, 4, 7, 4, 2, 5, 7, 5, 2,
        6, 7, 6, 2, 7, 7, 7, 2, 8, 7, 8, 2, 9, 7, 9, 2, 10, 7, 10, 2, 11, 7, 11, 2, 12, 7, 12, 2,
        13, 7, 13, 2, 14, 7, 14, 2, 15, 7, 15, 2, 16, 7, 16, 2, 17, 7, 17, 2, 18, 7, 18, 2, 19, 7,
        19, 2, 20, 7, 20, 2, 21, 7, 21, 2, 22, 7, 22, 2, 23, 7, 23, 2, 24, 7, 24, 2, 25, 7, 25, 2,
        26, 7, 26, 2, 27, 7, 27, 2, 28, 7, 28, 2, 29, 7, 29, 2, 30, 7, 30, 2, 31, 7, 31, 2, 32, 7,
        32, 2, 33, 7, 33, 2, 34, 7, 34, 2, 35, 7, 35, 2, 36, 7, 36, 2, 37, 7, 37, 2, 38, 7, 38, 2,
        39, 7, 39, 2, 40, 7, 40, 2, 41, 7, 41, 2, 42, 7, 42, 2, 43, 7, 43, 2, 44, 7, 44, 2, 45, 7,
        45, 2, 46, 7, 46, 2, 47, 7, 47, 2, 48, 7, 48, 2, 49, 7, 49, 2, 50, 7, 50, 2, 51, 7, 51, 2,
        52, 7, 52, 2, 53, 7, 53, 2, 54, 7, 54, 2, 55, 7, 55, 2, 56, 7, 56, 2, 57, 7, 57, 2, 58, 7,
        58, 2, 59, 7, 59, 2, 60, 7, 60, 2, 61, 7, 61, 2, 62, 7, 62, 2, 63, 7, 63, 2, 64, 7, 64, 2,
        65, 7, 65, 2, 66, 7, 66, 2, 67, 7, 67, 2, 68, 7, 68, 2, 69, 7, 69, 2, 70, 7, 70, 2, 71, 7,
        71, 2, 72, 7, 72, 2, 73, 7, 73, 2, 74, 7, 74, 2, 75, 7, 75, 2, 76, 7, 76, 2, 77, 7, 77, 2,
        78, 7, 78, 2, 79, 7, 79, 2, 80, 7, 80, 2, 81, 7, 81, 2, 82, 7, 82, 2, 83, 7, 83, 2, 84, 7,
        84, 2, 85, 7, 85, 2, 86, 7, 86, 2, 87, 7, 87, 2, 88, 7, 88, 2, 89, 7, 89, 2, 90, 7, 90, 2,
        91, 7, 91, 2, 92, 7, 92, 2, 93, 7, 93, 2, 94, 7, 94, 2, 95, 7, 95, 2, 96, 7, 96, 2, 97, 7,
        97, 2, 98, 7, 98, 2, 99, 7, 99, 2, 100, 7, 100, 2, 101, 7, 101, 2, 102, 7, 102, 2, 103, 7,
        103, 2, 104, 7, 104, 2, 105, 7, 105, 2, 106, 7, 106, 2, 107, 7, 107, 2, 108, 7, 108, 1, 0,
        3, 0, 220, 8, 0, 1, 0, 1, 0, 3, 0, 224, 8, 0, 1, 0, 3, 0, 227, 8, 0, 1, 0, 3, 0, 230, 8, 0,
        1, 0, 1, 0, 1, 1, 1, 1, 1, 2, 1, 2, 3, 2, 238, 8, 2, 1, 3, 1, 3, 3, 3, 242, 8, 3, 1, 3, 5,
        3, 245, 8, 3, 10, 3, 12, 3, 248, 9, 3, 1, 4, 1, 4, 1, 4, 1, 4, 3, 4, 254, 8, 4, 1, 4, 1, 4,
        1, 4, 3, 4, 259, 8, 4, 1, 4, 3, 4, 262, 8, 4, 1, 5, 1, 5, 3, 5, 266, 8, 5, 1, 6, 1, 6, 3,
        6, 270, 8, 6, 5, 6, 272, 8, 6, 10, 6, 12, 6, 275, 9, 6, 1, 6, 1, 6, 1, 6, 3, 6, 280, 8, 6,
        5, 6, 282, 8, 6, 10, 6, 12, 6, 285, 9, 6, 1, 6, 1, 6, 3, 6, 289, 8, 6, 1, 6, 5, 6, 292, 8,
        6, 10, 6, 12, 6, 295, 9, 6, 1, 6, 3, 6, 298, 8, 6, 1, 6, 3, 6, 301, 8, 6, 3, 6, 303, 8, 6,
        1, 7, 1, 7, 3, 7, 307, 8, 7, 5, 7, 309, 8, 7, 10, 7, 12, 7, 312, 9, 7, 1, 7, 1, 7, 3, 7,
        316, 8, 7, 5, 7, 318, 8, 7, 10, 7, 12, 7, 321, 9, 7, 1, 7, 1, 7, 3, 7, 325, 8, 7, 4, 7,
        327, 8, 7, 11, 7, 12, 7, 328, 1, 7, 1, 7, 1, 8, 1, 8, 1, 8, 1, 8, 1, 8, 3, 8, 338, 8, 8, 1,
        9, 1, 9, 1, 9, 3, 9, 343, 8, 9, 1, 10, 1, 10, 3, 10, 347, 8, 10, 1, 10, 1, 10, 3, 10, 351,
        8, 10, 1, 10, 1, 10, 3, 10, 355, 8, 10, 1, 10, 3, 10, 358, 8, 10, 1, 11, 1, 11, 3, 11, 362,
        8, 11, 1, 11, 1, 11, 1, 11, 1, 11, 1, 11, 1, 11, 1, 12, 1, 12, 3, 12, 372, 8, 12, 1, 12, 1,
        12, 1, 12, 5, 12, 377, 8, 12, 10, 12, 12, 12, 380, 9, 12, 1, 13, 1, 13, 1, 13, 1, 13, 1,
        13, 1, 13, 1, 13, 1, 13, 1, 13, 1, 13, 3, 13, 392, 8, 13, 1, 14, 1, 14, 3, 14, 396, 8, 14,
        1, 14, 1, 14, 1, 15, 1, 15, 3, 15, 402, 8, 15, 1, 15, 1, 15, 3, 15, 406, 8, 15, 1, 15, 1,
        15, 3, 15, 410, 8, 15, 1, 15, 5, 15, 413, 8, 15, 10, 15, 12, 15, 416, 9, 15, 1, 16, 1, 16,
        3, 16, 420, 8, 16, 1, 16, 1, 16, 3, 16, 424, 8, 16, 1, 16, 1, 16, 1, 16, 1, 16, 3, 16, 430,
        8, 16, 1, 16, 1, 16, 3, 16, 434, 8, 16, 1, 16, 1, 16, 1, 16, 1, 16, 3, 16, 440, 8, 16, 1,
        16, 1, 16, 3, 16, 444, 8, 16, 1, 16, 1, 16, 1, 16, 1, 16, 3, 16, 450, 8, 16, 1, 16, 1, 16,
        3, 16, 454, 8, 16, 1, 17, 1, 17, 3, 17, 458, 8, 17, 1, 17, 1, 17, 3, 17, 462, 8, 17, 1, 17,
        1, 17, 3, 17, 466, 8, 17, 1, 17, 1, 17, 3, 17, 470, 8, 17, 1, 17, 5, 17, 473, 8, 17, 10,
        17, 12, 17, 476, 9, 17, 1, 18, 1, 18, 1, 18, 1, 18, 3, 18, 482, 8, 18, 1, 18, 1, 18, 3, 18,
        486, 8, 18, 1, 18, 5, 18, 489, 8, 18, 10, 18, 12, 18, 492, 9, 18, 1, 19, 1, 19, 1, 19, 1,
        19, 3, 19, 498, 8, 19, 1, 20, 1, 20, 1, 20, 1, 20, 3, 20, 504, 8, 20, 1, 20, 1, 20, 1, 20,
        3, 20, 509, 8, 20, 1, 21, 1, 21, 1, 21, 1, 21, 3, 21, 515, 8, 21, 1, 21, 3, 21, 518, 8, 21,
        1, 21, 1, 21, 1, 21, 1, 21, 3, 21, 524, 8, 21, 3, 21, 526, 8, 21, 1, 22, 1, 22, 3, 22, 530,
        8, 22, 1, 22, 1, 22, 3, 22, 534, 8, 22, 1, 22, 5, 22, 537, 8, 22, 10, 22, 12, 22, 540, 9,
        22, 1, 22, 3, 22, 543, 8, 22, 1, 22, 3, 22, 546, 8, 22, 1, 23, 1, 23, 1, 23, 1, 23, 1, 23,
        3, 23, 553, 8, 23, 1, 23, 1, 23, 1, 24, 1, 24, 1, 24, 3, 24, 560, 8, 24, 1, 24, 3, 24, 563,
        8, 24, 1, 25, 1, 25, 1, 25, 1, 26, 3, 26, 569, 8, 26, 1, 26, 3, 26, 572, 8, 26, 1, 26, 1,
        26, 1, 26, 1, 26, 3, 26, 578, 8, 26, 1, 26, 1, 26, 3, 26, 582, 8, 26, 1, 26, 1, 26, 3, 26,
        586, 8, 26, 1, 27, 1, 27, 3, 27, 590, 8, 27, 1, 27, 1, 27, 3, 27, 594, 8, 27, 1, 27, 5, 27,
        597, 8, 27, 10, 27, 12, 27, 600, 9, 27, 1, 27, 1, 27, 3, 27, 604, 8, 27, 1, 27, 1, 27, 3,
        27, 608, 8, 27, 1, 27, 5, 27, 611, 8, 27, 10, 27, 12, 27, 614, 9, 27, 3, 27, 616, 8, 27, 1,
        28, 1, 28, 1, 28, 1, 28, 1, 28, 1, 28, 1, 28, 3, 28, 625, 8, 28, 1, 29, 1, 29, 1, 29, 1,
        29, 1, 29, 1, 29, 1, 29, 3, 29, 634, 8, 29, 1, 29, 5, 29, 637, 8, 29, 10, 29, 12, 29, 640,
        9, 29, 1, 30, 1, 30, 1, 30, 1, 30, 1, 31, 1, 31, 1, 31, 1, 31, 1, 32, 1, 32, 3, 32, 652, 8,
        32, 1, 32, 3, 32, 655, 8, 32, 1, 33, 1, 33, 1, 33, 1, 33, 1, 34, 1, 34, 3, 34, 663, 8, 34,
        1, 34, 1, 34, 3, 34, 667, 8, 34, 1, 34, 5, 34, 670, 8, 34, 10, 34, 12, 34, 673, 9, 34, 1,
        35, 1, 35, 3, 35, 677, 8, 35, 1, 35, 1, 35, 3, 35, 681, 8, 35, 1, 35, 1, 35, 1, 35, 3, 35,
        686, 8, 35, 1, 36, 1, 36, 1, 37, 1, 37, 3, 37, 692, 8, 37, 1, 37, 5, 37, 695, 8, 37, 10,
        37, 12, 37, 698, 9, 37, 1, 37, 1, 37, 1, 37, 1, 37, 3, 37, 704, 8, 37, 1, 38, 1, 38, 3, 38,
        708, 8, 38, 1, 38, 4, 38, 711, 8, 38, 11, 38, 12, 38, 712, 1, 39, 1, 39, 3, 39, 717, 8, 39,
        1, 39, 1, 39, 3, 39, 721, 8, 39, 3, 39, 723, 8, 39, 1, 39, 1, 39, 3, 39, 727, 8, 39, 3, 39,
        729, 8, 39, 1, 39, 1, 39, 3, 39, 733, 8, 39, 3, 39, 735, 8, 39, 1, 39, 1, 39, 1, 40, 1, 40,
        3, 40, 741, 8, 40, 1, 40, 1, 40, 1, 41, 1, 41, 3, 41, 747, 8, 41, 1, 41, 1, 41, 3, 41, 751,
        8, 41, 1, 41, 3, 41, 754, 8, 41, 1, 41, 3, 41, 757, 8, 41, 1, 41, 1, 41, 3, 41, 761, 8, 41,
        1, 41, 1, 41, 1, 41, 1, 41, 3, 41, 767, 8, 41, 1, 41, 1, 41, 3, 41, 771, 8, 41, 1, 41, 3,
        41, 774, 8, 41, 1, 41, 3, 41, 777, 8, 41, 1, 41, 1, 41, 1, 41, 1, 41, 3, 41, 783, 8, 41, 1,
        41, 3, 41, 786, 8, 41, 1, 41, 3, 41, 789, 8, 41, 1, 41, 1, 41, 3, 41, 793, 8, 41, 1, 41, 1,
        41, 1, 41, 1, 41, 3, 41, 799, 8, 41, 1, 41, 3, 41, 802, 8, 41, 1, 41, 3, 41, 805, 8, 41, 1,
        41, 1, 41, 3, 41, 809, 8, 41, 1, 42, 1, 42, 3, 42, 813, 8, 42, 1, 42, 1, 42, 3, 42, 817, 8,
        42, 3, 42, 819, 8, 42, 1, 42, 1, 42, 3, 42, 823, 8, 42, 3, 42, 825, 8, 42, 1, 42, 3, 42,
        828, 8, 42, 1, 42, 3, 42, 831, 8, 42, 1, 42, 3, 42, 834, 8, 42, 1, 42, 1, 42, 3, 42, 838,
        8, 42, 3, 42, 840, 8, 42, 1, 42, 1, 42, 1, 43, 1, 43, 1, 43, 5, 43, 847, 8, 43, 10, 43, 12,
        43, 850, 9, 43, 1, 43, 1, 43, 1, 44, 1, 44, 3, 44, 856, 8, 44, 1, 45, 1, 45, 3, 45, 860, 8,
        45, 1, 45, 1, 45, 3, 45, 864, 8, 45, 1, 45, 1, 45, 3, 45, 868, 8, 45, 1, 45, 3, 45, 871, 8,
        45, 1, 45, 5, 45, 874, 8, 45, 10, 45, 12, 45, 877, 9, 45, 1, 46, 1, 46, 3, 46, 881, 8, 46,
        1, 46, 5, 46, 884, 8, 46, 10, 46, 12, 46, 887, 9, 46, 1, 47, 1, 47, 3, 47, 891, 8, 47, 1,
        47, 1, 47, 3, 47, 895, 8, 47, 1, 47, 1, 47, 3, 47, 899, 8, 47, 1, 47, 3, 47, 902, 8, 47, 1,
        47, 5, 47, 905, 8, 47, 10, 47, 12, 47, 908, 9, 47, 1, 48, 1, 48, 3, 48, 912, 8, 48, 1, 48,
        1, 48, 3, 48, 916, 8, 48, 3, 48, 918, 8, 48, 1, 48, 1, 48, 3, 48, 922, 8, 48, 1, 48, 1, 48,
        3, 48, 926, 8, 48, 3, 48, 928, 8, 48, 3, 48, 930, 8, 48, 1, 49, 1, 49, 1, 50, 1, 50, 1, 51,
        1, 51, 3, 51, 938, 8, 51, 1, 51, 4, 51, 941, 8, 51, 11, 51, 12, 51, 942, 1, 52, 1, 52, 1,
        53, 1, 53, 1, 53, 1, 53, 1, 53, 5, 53, 952, 8, 53, 10, 53, 12, 53, 955, 9, 53, 1, 54, 1,
        54, 1, 54, 1, 54, 1, 54, 5, 54, 962, 8, 54, 10, 54, 12, 54, 965, 9, 54, 1, 55, 1, 55, 1,
        55, 1, 55, 1, 55, 5, 55, 972, 8, 55, 10, 55, 12, 55, 975, 9, 55, 1, 56, 1, 56, 3, 56, 979,
        8, 56, 5, 56, 981, 8, 56, 10, 56, 12, 56, 984, 9, 56, 1, 56, 1, 56, 1, 57, 1, 57, 3, 57,
        990, 8, 57, 1, 57, 5, 57, 993, 8, 57, 10, 57, 12, 57, 996, 9, 57, 1, 58, 1, 58, 3, 58,
        1000, 8, 58, 1, 58, 1, 58, 1, 58, 3, 58, 1005, 8, 58, 1, 58, 1, 58, 1, 58, 3, 58, 1010, 8,
        58, 1, 58, 1, 58, 1, 58, 3, 58, 1015, 8, 58, 1, 58, 1, 58, 1, 58, 3, 58, 1020, 8, 58, 1,
        58, 1, 58, 1, 58, 3, 58, 1025, 8, 58, 1, 58, 3, 58, 1028, 8, 58, 1, 59, 1, 59, 1, 59, 1,
        59, 5, 59, 1034, 8, 59, 10, 59, 12, 59, 1037, 9, 59, 1, 60, 1, 60, 1, 60, 1, 60, 1, 60, 1,
        60, 1, 60, 1, 60, 1, 60, 1, 60, 3, 60, 1049, 8, 60, 1, 60, 3, 60, 1052, 8, 60, 1, 60, 1,
        60, 1, 61, 1, 61, 1, 61, 3, 61, 1059, 8, 61, 1, 61, 1, 61, 1, 62, 1, 62, 1, 62, 1, 62, 1,
        62, 1, 62, 1, 62, 1, 62, 1, 62, 1, 62, 3, 62, 1073, 8, 62, 1, 63, 1, 63, 3, 63, 1077, 8,
        63, 1, 63, 1, 63, 3, 63, 1081, 8, 63, 1, 63, 1, 63, 3, 63, 1085, 8, 63, 1, 63, 1, 63, 3,
        63, 1089, 8, 63, 1, 63, 5, 63, 1092, 8, 63, 10, 63, 12, 63, 1095, 9, 63, 1, 64, 1, 64, 3,
        64, 1099, 8, 64, 1, 64, 1, 64, 3, 64, 1103, 8, 64, 1, 64, 1, 64, 3, 64, 1107, 8, 64, 1, 64,
        1, 64, 3, 64, 1111, 8, 64, 1, 64, 1, 64, 3, 64, 1115, 8, 64, 1, 64, 1, 64, 3, 64, 1119, 8,
        64, 1, 64, 5, 64, 1122, 8, 64, 10, 64, 12, 64, 1125, 9, 64, 1, 65, 1, 65, 3, 65, 1129, 8,
        65, 1, 65, 1, 65, 3, 65, 1133, 8, 65, 1, 65, 5, 65, 1136, 8, 65, 10, 65, 12, 65, 1139, 9,
        65, 1, 66, 1, 66, 1, 66, 3, 66, 1144, 8, 66, 1, 66, 3, 66, 1147, 8, 66, 1, 67, 1, 67, 3,
        67, 1151, 8, 67, 1, 67, 1, 67, 3, 67, 1155, 8, 67, 1, 67, 5, 67, 1158, 8, 67, 10, 67, 12,
        67, 1161, 9, 67, 1, 67, 3, 67, 1164, 8, 67, 1, 67, 3, 67, 1167, 8, 67, 1, 68, 1, 68, 1, 68,
        1, 68, 1, 68, 1, 68, 3, 68, 1175, 8, 68, 1, 68, 1, 68, 3, 68, 1179, 8, 68, 1, 68, 3, 68,
        1182, 8, 68, 1, 69, 1, 69, 3, 69, 1186, 8, 69, 1, 69, 1, 69, 3, 69, 1190, 8, 69, 1, 70, 1,
        70, 1, 70, 1, 70, 1, 70, 1, 70, 3, 70, 1198, 8, 70, 1, 70, 1, 70, 3, 70, 1202, 8, 70, 1,
        70, 1, 70, 3, 70, 1206, 8, 70, 1, 70, 1, 70, 1, 70, 1, 70, 1, 70, 1, 70, 1, 70, 1, 70, 1,
        70, 3, 70, 1217, 8, 70, 1, 71, 1, 71, 3, 71, 1221, 8, 71, 1, 71, 4, 71, 1224, 8, 71, 11,
        71, 12, 71, 1225, 1, 71, 1, 71, 3, 71, 1230, 8, 71, 1, 71, 1, 71, 3, 71, 1234, 8, 71, 1,
        71, 4, 71, 1237, 8, 71, 11, 71, 12, 71, 1238, 3, 71, 1241, 8, 71, 1, 71, 3, 71, 1244, 8,
        71, 1, 71, 1, 71, 3, 71, 1248, 8, 71, 1, 71, 3, 71, 1251, 8, 71, 1, 71, 3, 71, 1254, 8, 71,
        1, 71, 1, 71, 1, 72, 1, 72, 3, 72, 1260, 8, 72, 1, 72, 1, 72, 3, 72, 1264, 8, 72, 1, 72, 1,
        72, 3, 72, 1268, 8, 72, 1, 72, 1, 72, 1, 73, 1, 73, 3, 73, 1274, 8, 73, 1, 73, 1, 73, 3,
        73, 1278, 8, 73, 1, 73, 1, 73, 3, 73, 1282, 8, 73, 1, 73, 3, 73, 1285, 8, 73, 1, 73, 3, 73,
        1288, 8, 73, 1, 73, 1, 73, 1, 74, 1, 74, 3, 74, 1294, 8, 74, 1, 74, 1, 74, 3, 74, 1298, 8,
        74, 1, 74, 1, 74, 3, 74, 1302, 8, 74, 3, 74, 1304, 8, 74, 1, 74, 1, 74, 3, 74, 1308, 8, 74,
        1, 74, 1, 74, 3, 74, 1312, 8, 74, 3, 74, 1314, 8, 74, 1, 74, 1, 74, 3, 74, 1318, 8, 74, 1,
        74, 1, 74, 3, 74, 1322, 8, 74, 1, 74, 1, 74, 1, 75, 1, 75, 3, 75, 1328, 8, 75, 1, 75, 1,
        75, 3, 75, 1332, 8, 75, 1, 75, 1, 75, 3, 75, 1336, 8, 75, 1, 75, 1, 75, 1, 75, 1, 75, 3,
        75, 1342, 8, 75, 1, 75, 1, 75, 3, 75, 1346, 8, 75, 1, 75, 1, 75, 3, 75, 1350, 8, 75, 1, 75,
        1, 75, 1, 75, 1, 75, 3, 75, 1356, 8, 75, 1, 75, 1, 75, 3, 75, 1360, 8, 75, 1, 75, 1, 75, 3,
        75, 1364, 8, 75, 1, 75, 1, 75, 1, 75, 1, 75, 3, 75, 1370, 8, 75, 1, 75, 1, 75, 3, 75, 1374,
        8, 75, 1, 75, 1, 75, 3, 75, 1378, 8, 75, 1, 75, 1, 75, 3, 75, 1382, 8, 75, 1, 76, 1, 76, 3,
        76, 1386, 8, 76, 1, 76, 3, 76, 1389, 8, 76, 1, 77, 1, 77, 1, 78, 1, 78, 3, 78, 1395, 8, 78,
        1, 78, 1, 78, 3, 78, 1399, 8, 78, 1, 78, 1, 78, 1, 79, 1, 79, 1, 79, 1, 79, 1, 79, 1, 79,
        1, 80, 1, 80, 3, 80, 1411, 8, 80, 1, 80, 1, 80, 3, 80, 1415, 8, 80, 1, 80, 1, 80, 3, 80,
        1419, 8, 80, 3, 80, 1421, 8, 80, 1, 80, 1, 80, 3, 80, 1425, 8, 80, 1, 80, 1, 80, 3, 80,
        1429, 8, 80, 1, 80, 1, 80, 3, 80, 1433, 8, 80, 5, 80, 1435, 8, 80, 10, 80, 12, 80, 1438, 9,
        80, 3, 80, 1440, 8, 80, 1, 80, 1, 80, 1, 81, 1, 81, 3, 81, 1446, 8, 81, 1, 81, 1, 81, 3,
        81, 1450, 8, 81, 1, 81, 1, 81, 3, 81, 1454, 8, 81, 1, 81, 1, 81, 3, 81, 1458, 8, 81, 1, 81,
        1, 81, 1, 81, 1, 81, 3, 81, 1464, 8, 81, 1, 81, 3, 81, 1467, 8, 81, 1, 81, 1, 81, 1, 82, 1,
        82, 3, 82, 1473, 8, 82, 1, 82, 1, 82, 3, 82, 1477, 8, 82, 1, 82, 1, 82, 3, 82, 1481, 8, 82,
        1, 82, 1, 82, 3, 82, 1485, 8, 82, 1, 82, 5, 82, 1488, 8, 82, 10, 82, 12, 82, 1491, 9, 82,
        1, 82, 3, 82, 1494, 8, 82, 1, 82, 1, 82, 3, 82, 1498, 8, 82, 1, 82, 3, 82, 1501, 8, 82, 1,
        82, 1, 82, 3, 82, 1505, 8, 82, 1, 82, 3, 82, 1508, 8, 82, 1, 82, 3, 82, 1511, 8, 82, 1, 82,
        5, 82, 1514, 8, 82, 10, 82, 12, 82, 1517, 9, 82, 1, 83, 1, 83, 1, 83, 3, 83, 1522, 8, 83,
        1, 84, 1, 84, 1, 84, 1, 84, 1, 85, 1, 85, 1, 86, 1, 86, 1, 86, 1, 87, 1, 87, 3, 87, 1535,
        8, 87, 1, 87, 1, 87, 3, 87, 1539, 8, 87, 1, 87, 1, 87, 3, 87, 1543, 8, 87, 1, 87, 1, 87, 3,
        87, 1547, 8, 87, 1, 87, 3, 87, 1550, 8, 87, 1, 87, 1, 87, 1, 87, 3, 87, 1555, 8, 87, 1, 87,
        3, 87, 1558, 8, 87, 3, 87, 1560, 8, 87, 1, 87, 3, 87, 1563, 8, 87, 1, 87, 1, 87, 1, 88, 1,
        88, 3, 88, 1569, 8, 88, 1, 88, 1, 88, 3, 88, 1573, 8, 88, 1, 88, 1, 88, 3, 88, 1577, 8, 88,
        1, 88, 1, 88, 3, 88, 1581, 8, 88, 1, 88, 1, 88, 3, 88, 1585, 8, 88, 5, 88, 1587, 8, 88, 10,
        88, 12, 88, 1590, 9, 88, 3, 88, 1592, 8, 88, 1, 88, 1, 88, 1, 89, 1, 89, 1, 90, 1, 90, 1,
        91, 1, 91, 1, 91, 1, 92, 1, 92, 1, 92, 5, 92, 1606, 8, 92, 10, 92, 12, 92, 1609, 9, 92, 1,
        93, 1, 93, 1, 94, 1, 94, 1, 94, 1, 94, 1, 94, 1, 94, 3, 94, 1619, 8, 94, 1, 95, 1, 95, 1,
        96, 1, 96, 3, 96, 1625, 8, 96, 1, 97, 1, 97, 1, 98, 1, 98, 1, 99, 1, 99, 3, 99, 1633, 8,
        99, 1, 99, 1, 99, 3, 99, 1637, 8, 99, 1, 99, 1, 99, 3, 99, 1641, 8, 99, 1, 99, 1, 99, 3,
        99, 1645, 8, 99, 5, 99, 1647, 8, 99, 10, 99, 12, 99, 1650, 9, 99, 3, 99, 1652, 8, 99, 1,
        99, 1, 99, 1, 100, 1, 100, 3, 100, 1658, 8, 100, 1, 100, 1, 100, 3, 100, 1662, 8, 100, 1,
        100, 1, 100, 3, 100, 1666, 8, 100, 1, 100, 1, 100, 3, 100, 1670, 8, 100, 1, 100, 1, 100, 3,
        100, 1674, 8, 100, 1, 100, 1, 100, 3, 100, 1678, 8, 100, 1, 100, 1, 100, 3, 100, 1682, 8,
        100, 1, 100, 1, 100, 3, 100, 1686, 8, 100, 5, 100, 1688, 8, 100, 10, 100, 12, 100, 1691, 9,
        100, 3, 100, 1693, 8, 100, 1, 100, 1, 100, 1, 101, 1, 101, 1, 102, 1, 102, 1, 102, 3, 102,
        1702, 8, 102, 1, 103, 1, 103, 3, 103, 1706, 8, 103, 1, 104, 1, 104, 1, 105, 1, 105, 1, 106,
        1, 106, 1, 107, 1, 107, 1, 108, 1, 108, 1, 108, 0, 0, 109, 0, 2, 4, 6, 8, 10, 12, 14, 16,
        18, 20, 22, 24, 26, 28, 30, 32, 34, 36, 38, 40, 42, 44, 46, 48, 50, 52, 54, 56, 58, 60, 62,
        64, 66, 68, 70, 72, 74, 76, 78, 80, 82, 84, 86, 88, 90, 92, 94, 96, 98, 100, 102, 104, 106,
        108, 110, 112, 114, 116, 118, 120, 122, 124, 126, 128, 130, 132, 134, 136, 138, 140, 142,
        144, 146, 148, 150, 152, 154, 156, 158, 160, 162, 164, 166, 168, 170, 172, 174, 176, 178,
        180, 182, 184, 186, 188, 190, 192, 194, 196, 198, 200, 202, 204, 206, 208, 210, 212, 214,
        216, 0, 11, 1, 0, 68, 71, 1, 0, 6, 7, 1, 0, 18, 19, 1, 0, 94, 95, 1, 0, 96, 98, 1, 0, 106,
        107, 5, 0, 46, 58, 61, 82, 84, 88, 92, 95, 110, 119, 5, 0, 83, 83, 89, 91, 99, 99, 120,
        122, 125, 125, 2, 0, 14, 14, 27, 30, 2, 0, 15, 15, 31, 34, 2, 0, 19, 19, 35, 45, 1964, 0,
        219, 1, 0, 0, 0, 2, 233, 1, 0, 0, 0, 4, 237, 1, 0, 0, 0, 6, 239, 1, 0, 0, 0, 8, 261, 1, 0,
        0, 0, 10, 265, 1, 0, 0, 0, 12, 302, 1, 0, 0, 0, 14, 326, 1, 0, 0, 0, 16, 337, 1, 0, 0, 0,
        18, 342, 1, 0, 0, 0, 20, 346, 1, 0, 0, 0, 22, 359, 1, 0, 0, 0, 24, 369, 1, 0, 0, 0, 26,
        391, 1, 0, 0, 0, 28, 393, 1, 0, 0, 0, 30, 399, 1, 0, 0, 0, 32, 453, 1, 0, 0, 0, 34, 457, 1,
        0, 0, 0, 36, 477, 1, 0, 0, 0, 38, 497, 1, 0, 0, 0, 40, 499, 1, 0, 0, 0, 42, 510, 1, 0, 0,
        0, 44, 527, 1, 0, 0, 0, 46, 552, 1, 0, 0, 0, 48, 556, 1, 0, 0, 0, 50, 564, 1, 0, 0, 0, 52,
        571, 1, 0, 0, 0, 54, 615, 1, 0, 0, 0, 56, 624, 1, 0, 0, 0, 58, 626, 1, 0, 0, 0, 60, 641, 1,
        0, 0, 0, 62, 645, 1, 0, 0, 0, 64, 649, 1, 0, 0, 0, 66, 656, 1, 0, 0, 0, 68, 660, 1, 0, 0,
        0, 70, 685, 1, 0, 0, 0, 72, 687, 1, 0, 0, 0, 74, 703, 1, 0, 0, 0, 76, 705, 1, 0, 0, 0, 78,
        714, 1, 0, 0, 0, 80, 738, 1, 0, 0, 0, 82, 808, 1, 0, 0, 0, 84, 810, 1, 0, 0, 0, 86, 843, 1,
        0, 0, 0, 88, 855, 1, 0, 0, 0, 90, 857, 1, 0, 0, 0, 92, 878, 1, 0, 0, 0, 94, 888, 1, 0, 0,
        0, 96, 909, 1, 0, 0, 0, 98, 931, 1, 0, 0, 0, 100, 933, 1, 0, 0, 0, 102, 935, 1, 0, 0, 0,
        104, 944, 1, 0, 0, 0, 106, 946, 1, 0, 0, 0, 108, 956, 1, 0, 0, 0, 110, 966, 1, 0, 0, 0,
        112, 982, 1, 0, 0, 0, 114, 987, 1, 0, 0, 0, 116, 1027, 1, 0, 0, 0, 118, 1029, 1, 0, 0, 0,
        120, 1048, 1, 0, 0, 0, 122, 1055, 1, 0, 0, 0, 124, 1072, 1, 0, 0, 0, 126, 1074, 1, 0, 0, 0,
        128, 1096, 1, 0, 0, 0, 130, 1126, 1, 0, 0, 0, 132, 1146, 1, 0, 0, 0, 134, 1148, 1, 0, 0, 0,
        136, 1181, 1, 0, 0, 0, 138, 1183, 1, 0, 0, 0, 140, 1216, 1, 0, 0, 0, 142, 1240, 1, 0, 0, 0,
        144, 1257, 1, 0, 0, 0, 146, 1271, 1, 0, 0, 0, 148, 1291, 1, 0, 0, 0, 150, 1381, 1, 0, 0, 0,
        152, 1383, 1, 0, 0, 0, 154, 1390, 1, 0, 0, 0, 156, 1392, 1, 0, 0, 0, 158, 1402, 1, 0, 0, 0,
        160, 1408, 1, 0, 0, 0, 162, 1443, 1, 0, 0, 0, 164, 1470, 1, 0, 0, 0, 166, 1521, 1, 0, 0, 0,
        168, 1523, 1, 0, 0, 0, 170, 1527, 1, 0, 0, 0, 172, 1529, 1, 0, 0, 0, 174, 1532, 1, 0, 0, 0,
        176, 1566, 1, 0, 0, 0, 178, 1595, 1, 0, 0, 0, 180, 1597, 1, 0, 0, 0, 182, 1599, 1, 0, 0, 0,
        184, 1607, 1, 0, 0, 0, 186, 1610, 1, 0, 0, 0, 188, 1618, 1, 0, 0, 0, 190, 1620, 1, 0, 0, 0,
        192, 1624, 1, 0, 0, 0, 194, 1626, 1, 0, 0, 0, 196, 1628, 1, 0, 0, 0, 198, 1630, 1, 0, 0, 0,
        200, 1655, 1, 0, 0, 0, 202, 1696, 1, 0, 0, 0, 204, 1698, 1, 0, 0, 0, 206, 1705, 1, 0, 0, 0,
        208, 1707, 1, 0, 0, 0, 210, 1709, 1, 0, 0, 0, 212, 1711, 1, 0, 0, 0, 214, 1713, 1, 0, 0, 0,
        216, 1715, 1, 0, 0, 0, 218, 220, 5, 126, 0, 0, 219, 218, 1, 0, 0, 0, 219, 220, 1, 0, 0, 0,
        220, 221, 1, 0, 0, 0, 221, 226, 3, 2, 1, 0, 222, 224, 5, 126, 0, 0, 223, 222, 1, 0, 0, 0,
        223, 224, 1, 0, 0, 0, 224, 225, 1, 0, 0, 0, 225, 227, 5, 1, 0, 0, 226, 223, 1, 0, 0, 0,
        226, 227, 1, 0, 0, 0, 227, 229, 1, 0, 0, 0, 228, 230, 5, 126, 0, 0, 229, 228, 1, 0, 0, 0,
        229, 230, 1, 0, 0, 0, 230, 231, 1, 0, 0, 0, 231, 232, 5, 0, 0, 1, 232, 1, 1, 0, 0, 0, 233,
        234, 3, 4, 2, 0, 234, 3, 1, 0, 0, 0, 235, 238, 3, 6, 3, 0, 236, 238, 3, 42, 21, 0, 237,
        235, 1, 0, 0, 0, 237, 236, 1, 0, 0, 0, 238, 5, 1, 0, 0, 0, 239, 246, 3, 10, 5, 0, 240, 242,
        5, 126, 0, 0, 241, 240, 1, 0, 0, 0, 241, 242, 1, 0, 0, 0, 242, 243, 1, 0, 0, 0, 243, 245,
        3, 8, 4, 0, 244, 241, 1, 0, 0, 0, 245, 248, 1, 0, 0, 0, 246, 244, 1, 0, 0, 0, 246, 247, 1,
        0, 0, 0, 247, 7, 1, 0, 0, 0, 248, 246, 1, 0, 0, 0, 249, 250, 5, 46, 0, 0, 250, 251, 5, 126,
        0, 0, 251, 253, 5, 47, 0, 0, 252, 254, 5, 126, 0, 0, 253, 252, 1, 0, 0, 0, 253, 254, 1, 0,
        0, 0, 254, 255, 1, 0, 0, 0, 255, 262, 3, 10, 5, 0, 256, 258, 5, 46, 0, 0, 257, 259, 5, 126,
        0, 0, 258, 257, 1, 0, 0, 0, 258, 259, 1, 0, 0, 0, 259, 260, 1, 0, 0, 0, 260, 262, 3, 10, 5,
        0, 261, 249, 1, 0, 0, 0, 261, 256, 1, 0, 0, 0, 262, 9, 1, 0, 0, 0, 263, 266, 3, 12, 6, 0,
        264, 266, 3, 14, 7, 0, 265, 263, 1, 0, 0, 0, 265, 264, 1, 0, 0, 0, 266, 11, 1, 0, 0, 0,
        267, 269, 3, 18, 9, 0, 268, 270, 5, 126, 0, 0, 269, 268, 1, 0, 0, 0, 269, 270, 1, 0, 0, 0,
        270, 272, 1, 0, 0, 0, 271, 267, 1, 0, 0, 0, 272, 275, 1, 0, 0, 0, 273, 271, 1, 0, 0, 0,
        273, 274, 1, 0, 0, 0, 274, 276, 1, 0, 0, 0, 275, 273, 1, 0, 0, 0, 276, 303, 3, 50, 25, 0,
        277, 279, 3, 18, 9, 0, 278, 280, 5, 126, 0, 0, 279, 278, 1, 0, 0, 0, 279, 280, 1, 0, 0, 0,
        280, 282, 1, 0, 0, 0, 281, 277, 1, 0, 0, 0, 282, 285, 1, 0, 0, 0, 283, 281, 1, 0, 0, 0,
        283, 284, 1, 0, 0, 0, 284, 286, 1, 0, 0, 0, 285, 283, 1, 0, 0, 0, 286, 293, 3, 16, 8, 0,
        287, 289, 5, 126, 0, 0, 288, 287, 1, 0, 0, 0, 288, 289, 1, 0, 0, 0, 289, 290, 1, 0, 0, 0,
        290, 292, 3, 16, 8, 0, 291, 288, 1, 0, 0, 0, 292, 295, 1, 0, 0, 0, 293, 291, 1, 0, 0, 0,
        293, 294, 1, 0, 0, 0, 294, 300, 1, 0, 0, 0, 295, 293, 1, 0, 0, 0, 296, 298, 5, 126, 0, 0,
        297, 296, 1, 0, 0, 0, 297, 298, 1, 0, 0, 0, 298, 299, 1, 0, 0, 0, 299, 301, 3, 50, 25, 0,
        300, 297, 1, 0, 0, 0, 300, 301, 1, 0, 0, 0, 301, 303, 1, 0, 0, 0, 302, 273, 1, 0, 0, 0,
        302, 283, 1, 0, 0, 0, 303, 13, 1, 0, 0, 0, 304, 306, 3, 18, 9, 0, 305, 307, 5, 126, 0, 0,
        306, 305, 1, 0, 0, 0, 306, 307, 1, 0, 0, 0, 307, 309, 1, 0, 0, 0, 308, 304, 1, 0, 0, 0,
        309, 312, 1, 0, 0, 0, 310, 308, 1, 0, 0, 0, 310, 311, 1, 0, 0, 0, 311, 319, 1, 0, 0, 0,
        312, 310, 1, 0, 0, 0, 313, 315, 3, 16, 8, 0, 314, 316, 5, 126, 0, 0, 315, 314, 1, 0, 0, 0,
        315, 316, 1, 0, 0, 0, 316, 318, 1, 0, 0, 0, 317, 313, 1, 0, 0, 0, 318, 321, 1, 0, 0, 0,
        319, 317, 1, 0, 0, 0, 319, 320, 1, 0, 0, 0, 320, 322, 1, 0, 0, 0, 321, 319, 1, 0, 0, 0,
        322, 324, 3, 48, 24, 0, 323, 325, 5, 126, 0, 0, 324, 323, 1, 0, 0, 0, 324, 325, 1, 0, 0, 0,
        325, 327, 1, 0, 0, 0, 326, 310, 1, 0, 0, 0, 327, 328, 1, 0, 0, 0, 328, 326, 1, 0, 0, 0,
        328, 329, 1, 0, 0, 0, 329, 330, 1, 0, 0, 0, 330, 331, 3, 12, 6, 0, 331, 15, 1, 0, 0, 0,
        332, 338, 3, 28, 14, 0, 333, 338, 3, 24, 12, 0, 334, 338, 3, 34, 17, 0, 335, 338, 3, 30,
        15, 0, 336, 338, 3, 36, 18, 0, 337, 332, 1, 0, 0, 0, 337, 333, 1, 0, 0, 0, 337, 334, 1, 0,
        0, 0, 337, 335, 1, 0, 0, 0, 337, 336, 1, 0, 0, 0, 338, 17, 1, 0, 0, 0, 339, 343, 3, 20, 10,
        0, 340, 343, 3, 22, 11, 0, 341, 343, 3, 40, 20, 0, 342, 339, 1, 0, 0, 0, 342, 340, 1, 0, 0,
        0, 342, 341, 1, 0, 0, 0, 343, 19, 1, 0, 0, 0, 344, 345, 5, 48, 0, 0, 345, 347, 5, 126, 0,
        0, 346, 344, 1, 0, 0, 0, 346, 347, 1, 0, 0, 0, 347, 348, 1, 0, 0, 0, 348, 350, 5, 49, 0, 0,
        349, 351, 5, 126, 0, 0, 350, 349, 1, 0, 0, 0, 350, 351, 1, 0, 0, 0, 351, 352, 1, 0, 0, 0,
        352, 357, 3, 68, 34, 0, 353, 355, 5, 126, 0, 0, 354, 353, 1, 0, 0, 0, 354, 355, 1, 0, 0, 0,
        355, 356, 1, 0, 0, 0, 356, 358, 3, 66, 33, 0, 357, 354, 1, 0, 0, 0, 357, 358, 1, 0, 0, 0,
        358, 21, 1, 0, 0, 0, 359, 361, 5, 50, 0, 0, 360, 362, 5, 126, 0, 0, 361, 360, 1, 0, 0, 0,
        361, 362, 1, 0, 0, 0, 362, 363, 1, 0, 0, 0, 363, 364, 3, 104, 52, 0, 364, 365, 5, 126, 0,
        0, 365, 366, 5, 51, 0, 0, 366, 367, 5, 126, 0, 0, 367, 368, 3, 186, 93, 0, 368, 23, 1, 0,
        0, 0, 369, 371, 5, 52, 0, 0, 370, 372, 5, 126, 0, 0, 371, 370, 1, 0, 0, 0, 371, 372, 1, 0,
        0, 0, 372, 373, 1, 0, 0, 0, 373, 378, 3, 70, 35, 0, 374, 375, 5, 126, 0, 0, 375, 377, 3,
        26, 13, 0, 376, 374, 1, 0, 0, 0, 377, 380, 1, 0, 0, 0, 378, 376, 1, 0, 0, 0, 378, 379, 1,
        0, 0, 0, 379, 25, 1, 0, 0, 0, 380, 378, 1, 0, 0, 0, 381, 382, 5, 53, 0, 0, 382, 383, 5,
        126, 0, 0, 383, 384, 5, 49, 0, 0, 384, 385, 5, 126, 0, 0, 385, 392, 3, 30, 15, 0, 386, 387,
        5, 53, 0, 0, 387, 388, 5, 126, 0, 0, 388, 389, 5, 54, 0, 0, 389, 390, 5, 126, 0, 0, 390,
        392, 3, 30, 15, 0, 391, 381, 1, 0, 0, 0, 391, 386, 1, 0, 0, 0, 392, 27, 1, 0, 0, 0, 393,
        395, 5, 54, 0, 0, 394, 396, 5, 126, 0, 0, 395, 394, 1, 0, 0, 0, 395, 396, 1, 0, 0, 0, 396,
        397, 1, 0, 0, 0, 397, 398, 3, 68, 34, 0, 398, 29, 1, 0, 0, 0, 399, 401, 5, 55, 0, 0, 400,
        402, 5, 126, 0, 0, 401, 400, 1, 0, 0, 0, 401, 402, 1, 0, 0, 0, 402, 403, 1, 0, 0, 0, 403,
        414, 3, 32, 16, 0, 404, 406, 5, 126, 0, 0, 405, 404, 1, 0, 0, 0, 405, 406, 1, 0, 0, 0, 406,
        407, 1, 0, 0, 0, 407, 409, 5, 2, 0, 0, 408, 410, 5, 126, 0, 0, 409, 408, 1, 0, 0, 0, 409,
        410, 1, 0, 0, 0, 410, 411, 1, 0, 0, 0, 411, 413, 3, 32, 16, 0, 412, 405, 1, 0, 0, 0, 413,
        416, 1, 0, 0, 0, 414, 412, 1, 0, 0, 0, 414, 415, 1, 0, 0, 0, 415, 31, 1, 0, 0, 0, 416, 414,
        1, 0, 0, 0, 417, 419, 3, 102, 51, 0, 418, 420, 5, 126, 0, 0, 419, 418, 1, 0, 0, 0, 419,
        420, 1, 0, 0, 0, 420, 421, 1, 0, 0, 0, 421, 423, 5, 3, 0, 0, 422, 424, 5, 126, 0, 0, 423,
        422, 1, 0, 0, 0, 423, 424, 1, 0, 0, 0, 424, 425, 1, 0, 0, 0, 425, 426, 3, 104, 52, 0, 426,
        454, 1, 0, 0, 0, 427, 429, 3, 186, 93, 0, 428, 430, 5, 126, 0, 0, 429, 428, 1, 0, 0, 0,
        429, 430, 1, 0, 0, 0, 430, 431, 1, 0, 0, 0, 431, 433, 5, 3, 0, 0, 432, 434, 5, 126, 0, 0,
        433, 432, 1, 0, 0, 0, 433, 434, 1, 0, 0, 0, 434, 435, 1, 0, 0, 0, 435, 436, 3, 104, 52, 0,
        436, 454, 1, 0, 0, 0, 437, 439, 3, 186, 93, 0, 438, 440, 5, 126, 0, 0, 439, 438, 1, 0, 0,
        0, 439, 440, 1, 0, 0, 0, 440, 441, 1, 0, 0, 0, 441, 443, 5, 4, 0, 0, 442, 444, 5, 126, 0,
        0, 443, 442, 1, 0, 0, 0, 443, 444, 1, 0, 0, 0, 444, 445, 1, 0, 0, 0, 445, 446, 3, 104, 52,
        0, 446, 454, 1, 0, 0, 0, 447, 449, 3, 186, 93, 0, 448, 450, 5, 126, 0, 0, 449, 448, 1, 0,
        0, 0, 449, 450, 1, 0, 0, 0, 450, 451, 1, 0, 0, 0, 451, 452, 3, 92, 46, 0, 452, 454, 1, 0,
        0, 0, 453, 417, 1, 0, 0, 0, 453, 427, 1, 0, 0, 0, 453, 437, 1, 0, 0, 0, 453, 447, 1, 0, 0,
        0, 454, 33, 1, 0, 0, 0, 455, 456, 5, 56, 0, 0, 456, 458, 5, 126, 0, 0, 457, 455, 1, 0, 0,
        0, 457, 458, 1, 0, 0, 0, 458, 459, 1, 0, 0, 0, 459, 461, 5, 57, 0, 0, 460, 462, 5, 126, 0,
        0, 461, 460, 1, 0, 0, 0, 461, 462, 1, 0, 0, 0, 462, 463, 1, 0, 0, 0, 463, 474, 3, 104, 52,
        0, 464, 466, 5, 126, 0, 0, 465, 464, 1, 0, 0, 0, 465, 466, 1, 0, 0, 0, 466, 467, 1, 0, 0,
        0, 467, 469, 5, 2, 0, 0, 468, 470, 5, 126, 0, 0, 469, 468, 1, 0, 0, 0, 469, 470, 1, 0, 0,
        0, 470, 471, 1, 0, 0, 0, 471, 473, 3, 104, 52, 0, 472, 465, 1, 0, 0, 0, 473, 476, 1, 0, 0,
        0, 474, 472, 1, 0, 0, 0, 474, 475, 1, 0, 0, 0, 475, 35, 1, 0, 0, 0, 476, 474, 1, 0, 0, 0,
        477, 478, 5, 58, 0, 0, 478, 479, 5, 126, 0, 0, 479, 490, 3, 38, 19, 0, 480, 482, 5, 126, 0,
        0, 481, 480, 1, 0, 0, 0, 481, 482, 1, 0, 0, 0, 482, 483, 1, 0, 0, 0, 483, 485, 5, 2, 0, 0,
        484, 486, 5, 126, 0, 0, 485, 484, 1, 0, 0, 0, 485, 486, 1, 0, 0, 0, 486, 487, 1, 0, 0, 0,
        487, 489, 3, 38, 19, 0, 488, 481, 1, 0, 0, 0, 489, 492, 1, 0, 0, 0, 490, 488, 1, 0, 0, 0,
        490, 491, 1, 0, 0, 0, 491, 37, 1, 0, 0, 0, 492, 490, 1, 0, 0, 0, 493, 494, 3, 186, 93, 0,
        494, 495, 3, 92, 46, 0, 495, 498, 1, 0, 0, 0, 496, 498, 3, 102, 51, 0, 497, 493, 1, 0, 0,
        0, 497, 496, 1, 0, 0, 0, 498, 39, 1, 0, 0, 0, 499, 500, 5, 59, 0, 0, 500, 501, 5, 126, 0,
        0, 501, 508, 3, 176, 88, 0, 502, 504, 5, 126, 0, 0, 503, 502, 1, 0, 0, 0, 503, 504, 1, 0,
        0, 0, 504, 505, 1, 0, 0, 0, 505, 506, 5, 60, 0, 0, 506, 507, 5, 126, 0, 0, 507, 509, 3, 44,
        22, 0, 508, 503, 1, 0, 0, 0, 508, 509, 1, 0, 0, 0, 509, 41, 1, 0, 0, 0, 510, 511, 5, 59, 0,
        0, 511, 514, 5, 126, 0, 0, 512, 515, 3, 176, 88, 0, 513, 515, 3, 178, 89, 0, 514, 512, 1,
        0, 0, 0, 514, 513, 1, 0, 0, 0, 515, 525, 1, 0, 0, 0, 516, 518, 5, 126, 0, 0, 517, 516, 1,
        0, 0, 0, 517, 518, 1, 0, 0, 0, 518, 519, 1, 0, 0, 0, 519, 520, 5, 60, 0, 0, 520, 523, 5,
        126, 0, 0, 521, 524, 5, 5, 0, 0, 522, 524, 3, 44, 22, 0, 523, 521, 1, 0, 0, 0, 523, 522, 1,
        0, 0, 0, 524, 526, 1, 0, 0, 0, 525, 517, 1, 0, 0, 0, 525, 526, 1, 0, 0, 0, 526, 43, 1, 0,
        0, 0, 527, 538, 3, 46, 23, 0, 528, 530, 5, 126, 0, 0, 529, 528, 1, 0, 0, 0, 529, 530, 1, 0,
        0, 0, 530, 531, 1, 0, 0, 0, 531, 533, 5, 2, 0, 0, 532, 534, 5, 126, 0, 0, 533, 532, 1, 0,
        0, 0, 533, 534, 1, 0, 0, 0, 534, 535, 1, 0, 0, 0, 535, 537, 3, 46, 23, 0, 536, 529, 1, 0,
        0, 0, 537, 540, 1, 0, 0, 0, 538, 536, 1, 0, 0, 0, 538, 539, 1, 0, 0, 0, 539, 545, 1, 0, 0,
        0, 540, 538, 1, 0, 0, 0, 541, 543, 5, 126, 0, 0, 542, 541, 1, 0, 0, 0, 542, 543, 1, 0, 0,
        0, 543, 544, 1, 0, 0, 0, 544, 546, 3, 66, 33, 0, 545, 542, 1, 0, 0, 0, 545, 546, 1, 0, 0,
        0, 546, 45, 1, 0, 0, 0, 547, 548, 3, 180, 90, 0, 548, 549, 5, 126, 0, 0, 549, 550, 5, 51,
        0, 0, 550, 551, 5, 126, 0, 0, 551, 553, 1, 0, 0, 0, 552, 547, 1, 0, 0, 0, 552, 553, 1, 0,
        0, 0, 553, 554, 1, 0, 0, 0, 554, 555, 3, 186, 93, 0, 555, 47, 1, 0, 0, 0, 556, 557, 5, 61,
        0, 0, 557, 562, 3, 52, 26, 0, 558, 560, 5, 126, 0, 0, 559, 558, 1, 0, 0, 0, 559, 560, 1, 0,
        0, 0, 560, 561, 1, 0, 0, 0, 561, 563, 3, 66, 33, 0, 562, 559, 1, 0, 0, 0, 562, 563, 1, 0,
        0, 0, 563, 49, 1, 0, 0, 0, 564, 565, 5, 62, 0, 0, 565, 566, 3, 52, 26, 0, 566, 51, 1, 0, 0,
        0, 567, 569, 5, 126, 0, 0, 568, 567, 1, 0, 0, 0, 568, 569, 1, 0, 0, 0, 569, 570, 1, 0, 0,
        0, 570, 572, 5, 63, 0, 0, 571, 568, 1, 0, 0, 0, 571, 572, 1, 0, 0, 0, 572, 573, 1, 0, 0, 0,
        573, 574, 5, 126, 0, 0, 574, 577, 3, 54, 27, 0, 575, 576, 5, 126, 0, 0, 576, 578, 3, 58,
        29, 0, 577, 575, 1, 0, 0, 0, 577, 578, 1, 0, 0, 0, 578, 581, 1, 0, 0, 0, 579, 580, 5, 126,
        0, 0, 580, 582, 3, 60, 30, 0, 581, 579, 1, 0, 0, 0, 581, 582, 1, 0, 0, 0, 582, 585, 1, 0,
        0, 0, 583, 584, 5, 126, 0, 0, 584, 586, 3, 62, 31, 0, 585, 583, 1, 0, 0, 0, 585, 586, 1, 0,
        0, 0, 586, 53, 1, 0, 0, 0, 587, 598, 5, 5, 0, 0, 588, 590, 5, 126, 0, 0, 589, 588, 1, 0, 0,
        0, 589, 590, 1, 0, 0, 0, 590, 591, 1, 0, 0, 0, 591, 593, 5, 2, 0, 0, 592, 594, 5, 126, 0,
        0, 593, 592, 1, 0, 0, 0, 593, 594, 1, 0, 0, 0, 594, 595, 1, 0, 0, 0, 595, 597, 3, 56, 28,
        0, 596, 589, 1, 0, 0, 0, 597, 600, 1, 0, 0, 0, 598, 596, 1, 0, 0, 0, 598, 599, 1, 0, 0, 0,
        599, 616, 1, 0, 0, 0, 600, 598, 1, 0, 0, 0, 601, 612, 3, 56, 28, 0, 602, 604, 5, 126, 0, 0,
        603, 602, 1, 0, 0, 0, 603, 604, 1, 0, 0, 0, 604, 605, 1, 0, 0, 0, 605, 607, 5, 2, 0, 0,
        606, 608, 5, 126, 0, 0, 607, 606, 1, 0, 0, 0, 607, 608, 1, 0, 0, 0, 608, 609, 1, 0, 0, 0,
        609, 611, 3, 56, 28, 0, 610, 603, 1, 0, 0, 0, 611, 614, 1, 0, 0, 0, 612, 610, 1, 0, 0, 0,
        612, 613, 1, 0, 0, 0, 613, 616, 1, 0, 0, 0, 614, 612, 1, 0, 0, 0, 615, 587, 1, 0, 0, 0,
        615, 601, 1, 0, 0, 0, 616, 55, 1, 0, 0, 0, 617, 618, 3, 104, 52, 0, 618, 619, 5, 126, 0, 0,
        619, 620, 5, 51, 0, 0, 620, 621, 5, 126, 0, 0, 621, 622, 3, 186, 93, 0, 622, 625, 1, 0, 0,
        0, 623, 625, 3, 104, 52, 0, 624, 617, 1, 0, 0, 0, 624, 623, 1, 0, 0, 0, 625, 57, 1, 0, 0,
        0, 626, 627, 5, 64, 0, 0, 627, 628, 5, 126, 0, 0, 628, 629, 5, 65, 0, 0, 629, 630, 5, 126,
        0, 0, 630, 638, 3, 64, 32, 0, 631, 633, 5, 2, 0, 0, 632, 634, 5, 126, 0, 0, 633, 632, 1, 0,
        0, 0, 633, 634, 1, 0, 0, 0, 634, 635, 1, 0, 0, 0, 635, 637, 3, 64, 32, 0, 636, 631, 1, 0,
        0, 0, 637, 640, 1, 0, 0, 0, 638, 636, 1, 0, 0, 0, 638, 639, 1, 0, 0, 0, 639, 59, 1, 0, 0,
        0, 640, 638, 1, 0, 0, 0, 641, 642, 5, 66, 0, 0, 642, 643, 5, 126, 0, 0, 643, 644, 3, 104,
        52, 0, 644, 61, 1, 0, 0, 0, 645, 646, 5, 67, 0, 0, 646, 647, 5, 126, 0, 0, 647, 648, 3,
        104, 52, 0, 648, 63, 1, 0, 0, 0, 649, 654, 3, 104, 52, 0, 650, 652, 5, 126, 0, 0, 651, 650,
        1, 0, 0, 0, 651, 652, 1, 0, 0, 0, 652, 653, 1, 0, 0, 0, 653, 655, 7, 0, 0, 0, 654, 651, 1,
        0, 0, 0, 654, 655, 1, 0, 0, 0, 655, 65, 1, 0, 0, 0, 656, 657, 5, 72, 0, 0, 657, 658, 5,
        126, 0, 0, 658, 659, 3, 104, 52, 0, 659, 67, 1, 0, 0, 0, 660, 671, 3, 70, 35, 0, 661, 663,
        5, 126, 0, 0, 662, 661, 1, 0, 0, 0, 662, 663, 1, 0, 0, 0, 663, 664, 1, 0, 0, 0, 664, 666,
        5, 2, 0, 0, 665, 667, 5, 126, 0, 0, 666, 665, 1, 0, 0, 0, 666, 667, 1, 0, 0, 0, 667, 668,
        1, 0, 0, 0, 668, 670, 3, 70, 35, 0, 669, 662, 1, 0, 0, 0, 670, 673, 1, 0, 0, 0, 671, 669,
        1, 0, 0, 0, 671, 672, 1, 0, 0, 0, 672, 69, 1, 0, 0, 0, 673, 671, 1, 0, 0, 0, 674, 676, 3,
        186, 93, 0, 675, 677, 5, 126, 0, 0, 676, 675, 1, 0, 0, 0, 676, 677, 1, 0, 0, 0, 677, 678,
        1, 0, 0, 0, 678, 680, 5, 3, 0, 0, 679, 681, 5, 126, 0, 0, 680, 679, 1, 0, 0, 0, 680, 681,
        1, 0, 0, 0, 681, 682, 1, 0, 0, 0, 682, 683, 3, 72, 36, 0, 683, 686, 1, 0, 0, 0, 684, 686,
        3, 72, 36, 0, 685, 674, 1, 0, 0, 0, 685, 684, 1, 0, 0, 0, 686, 71, 1, 0, 0, 0, 687, 688, 3,
        74, 37, 0, 688, 73, 1, 0, 0, 0, 689, 696, 3, 78, 39, 0, 690, 692, 5, 126, 0, 0, 691, 690,
        1, 0, 0, 0, 691, 692, 1, 0, 0, 0, 692, 693, 1, 0, 0, 0, 693, 695, 3, 80, 40, 0, 694, 691,
        1, 0, 0, 0, 695, 698, 1, 0, 0, 0, 696, 694, 1, 0, 0, 0, 696, 697, 1, 0, 0, 0, 697, 704, 1,
        0, 0, 0, 698, 696, 1, 0, 0, 0, 699, 700, 5, 6, 0, 0, 700, 701, 3, 74, 37, 0, 701, 702, 5,
        7, 0, 0, 702, 704, 1, 0, 0, 0, 703, 689, 1, 0, 0, 0, 703, 699, 1, 0, 0, 0, 704, 75, 1, 0,
        0, 0, 705, 710, 3, 78, 39, 0, 706, 708, 5, 126, 0, 0, 707, 706, 1, 0, 0, 0, 707, 708, 1, 0,
        0, 0, 708, 709, 1, 0, 0, 0, 709, 711, 3, 80, 40, 0, 710, 707, 1, 0, 0, 0, 711, 712, 1, 0,
        0, 0, 712, 710, 1, 0, 0, 0, 712, 713, 1, 0, 0, 0, 713, 77, 1, 0, 0, 0, 714, 716, 5, 6, 0,
        0, 715, 717, 5, 126, 0, 0, 716, 715, 1, 0, 0, 0, 716, 717, 1, 0, 0, 0, 717, 722, 1, 0, 0,
        0, 718, 720, 3, 186, 93, 0, 719, 721, 5, 126, 0, 0, 720, 719, 1, 0, 0, 0, 720, 721, 1, 0,
        0, 0, 721, 723, 1, 0, 0, 0, 722, 718, 1, 0, 0, 0, 722, 723, 1, 0, 0, 0, 723, 728, 1, 0, 0,
        0, 724, 726, 3, 92, 46, 0, 725, 727, 5, 126, 0, 0, 726, 725, 1, 0, 0, 0, 726, 727, 1, 0, 0,
        0, 727, 729, 1, 0, 0, 0, 728, 724, 1, 0, 0, 0, 728, 729, 1, 0, 0, 0, 729, 734, 1, 0, 0, 0,
        730, 732, 3, 88, 44, 0, 731, 733, 5, 126, 0, 0, 732, 731, 1, 0, 0, 0, 732, 733, 1, 0, 0, 0,
        733, 735, 1, 0, 0, 0, 734, 730, 1, 0, 0, 0, 734, 735, 1, 0, 0, 0, 735, 736, 1, 0, 0, 0,
        736, 737, 5, 7, 0, 0, 737, 79, 1, 0, 0, 0, 738, 740, 3, 82, 41, 0, 739, 741, 5, 126, 0, 0,
        740, 739, 1, 0, 0, 0, 740, 741, 1, 0, 0, 0, 741, 742, 1, 0, 0, 0, 742, 743, 3, 78, 39, 0,
        743, 81, 1, 0, 0, 0, 744, 746, 3, 212, 106, 0, 745, 747, 5, 126, 0, 0, 746, 745, 1, 0, 0,
        0, 746, 747, 1, 0, 0, 0, 747, 748, 1, 0, 0, 0, 748, 750, 3, 216, 108, 0, 749, 751, 5, 126,
        0, 0, 750, 749, 1, 0, 0, 0, 750, 751, 1, 0, 0, 0, 751, 753, 1, 0, 0, 0, 752, 754, 3, 84,
        42, 0, 753, 752, 1, 0, 0, 0, 753, 754, 1, 0, 0, 0, 754, 756, 1, 0, 0, 0, 755, 757, 5, 126,
        0, 0, 756, 755, 1, 0, 0, 0, 756, 757, 1, 0, 0, 0, 757, 758, 1, 0, 0, 0, 758, 760, 3, 216,
        108, 0, 759, 761, 5, 126, 0, 0, 760, 759, 1, 0, 0, 0, 760, 761, 1, 0, 0, 0, 761, 762, 1, 0,
        0, 0, 762, 763, 3, 214, 107, 0, 763, 809, 1, 0, 0, 0, 764, 766, 3, 212, 106, 0, 765, 767,
        5, 126, 0, 0, 766, 765, 1, 0, 0, 0, 766, 767, 1, 0, 0, 0, 767, 768, 1, 0, 0, 0, 768, 770,
        3, 216, 108, 0, 769, 771, 5, 126, 0, 0, 770, 769, 1, 0, 0, 0, 770, 771, 1, 0, 0, 0, 771,
        773, 1, 0, 0, 0, 772, 774, 3, 84, 42, 0, 773, 772, 1, 0, 0, 0, 773, 774, 1, 0, 0, 0, 774,
        776, 1, 0, 0, 0, 775, 777, 5, 126, 0, 0, 776, 775, 1, 0, 0, 0, 776, 777, 1, 0, 0, 0, 777,
        778, 1, 0, 0, 0, 778, 779, 3, 216, 108, 0, 779, 809, 1, 0, 0, 0, 780, 782, 3, 216, 108, 0,
        781, 783, 5, 126, 0, 0, 782, 781, 1, 0, 0, 0, 782, 783, 1, 0, 0, 0, 783, 785, 1, 0, 0, 0,
        784, 786, 3, 84, 42, 0, 785, 784, 1, 0, 0, 0, 785, 786, 1, 0, 0, 0, 786, 788, 1, 0, 0, 0,
        787, 789, 5, 126, 0, 0, 788, 787, 1, 0, 0, 0, 788, 789, 1, 0, 0, 0, 789, 790, 1, 0, 0, 0,
        790, 792, 3, 216, 108, 0, 791, 793, 5, 126, 0, 0, 792, 791, 1, 0, 0, 0, 792, 793, 1, 0, 0,
        0, 793, 794, 1, 0, 0, 0, 794, 795, 3, 214, 107, 0, 795, 809, 1, 0, 0, 0, 796, 798, 3, 216,
        108, 0, 797, 799, 5, 126, 0, 0, 798, 797, 1, 0, 0, 0, 798, 799, 1, 0, 0, 0, 799, 801, 1, 0,
        0, 0, 800, 802, 3, 84, 42, 0, 801, 800, 1, 0, 0, 0, 801, 802, 1, 0, 0, 0, 802, 804, 1, 0,
        0, 0, 803, 805, 5, 126, 0, 0, 804, 803, 1, 0, 0, 0, 804, 805, 1, 0, 0, 0, 805, 806, 1, 0,
        0, 0, 806, 807, 3, 216, 108, 0, 807, 809, 1, 0, 0, 0, 808, 744, 1, 0, 0, 0, 808, 764, 1, 0,
        0, 0, 808, 780, 1, 0, 0, 0, 808, 796, 1, 0, 0, 0, 809, 83, 1, 0, 0, 0, 810, 812, 5, 8, 0,
        0, 811, 813, 5, 126, 0, 0, 812, 811, 1, 0, 0, 0, 812, 813, 1, 0, 0, 0, 813, 818, 1, 0, 0,
        0, 814, 816, 3, 186, 93, 0, 815, 817, 5, 126, 0, 0, 816, 815, 1, 0, 0, 0, 816, 817, 1, 0,
        0, 0, 817, 819, 1, 0, 0, 0, 818, 814, 1, 0, 0, 0, 818, 819, 1, 0, 0, 0, 819, 824, 1, 0, 0,
        0, 820, 822, 3, 90, 45, 0, 821, 823, 5, 126, 0, 0, 822, 821, 1, 0, 0, 0, 822, 823, 1, 0, 0,
        0, 823, 825, 1, 0, 0, 0, 824, 820, 1, 0, 0, 0, 824, 825, 1, 0, 0, 0, 825, 827, 1, 0, 0, 0,
        826, 828, 3, 96, 48, 0, 827, 826, 1, 0, 0, 0, 827, 828, 1, 0, 0, 0, 828, 833, 1, 0, 0, 0,
        829, 831, 5, 126, 0, 0, 830, 829, 1, 0, 0, 0, 830, 831, 1, 0, 0, 0, 831, 832, 1, 0, 0, 0,
        832, 834, 3, 86, 43, 0, 833, 830, 1, 0, 0, 0, 833, 834, 1, 0, 0, 0, 834, 839, 1, 0, 0, 0,
        835, 837, 3, 88, 44, 0, 836, 838, 5, 126, 0, 0, 837, 836, 1, 0, 0, 0, 837, 838, 1, 0, 0, 0,
        838, 840, 1, 0, 0, 0, 839, 835, 1, 0, 0, 0, 839, 840, 1, 0, 0, 0, 840, 841, 1, 0, 0, 0,
        841, 842, 5, 9, 0, 0, 842, 85, 1, 0, 0, 0, 843, 848, 5, 6, 0, 0, 844, 847, 3, 86, 43, 0,
        845, 847, 8, 1, 0, 0, 846, 844, 1, 0, 0, 0, 846, 845, 1, 0, 0, 0, 847, 850, 1, 0, 0, 0,
        848, 846, 1, 0, 0, 0, 848, 849, 1, 0, 0, 0, 849, 851, 1, 0, 0, 0, 850, 848, 1, 0, 0, 0,
        851, 852, 5, 7, 0, 0, 852, 87, 1, 0, 0, 0, 853, 856, 3, 200, 100, 0, 854, 856, 3, 204, 102,
        0, 855, 853, 1, 0, 0, 0, 855, 854, 1, 0, 0, 0, 856, 89, 1, 0, 0, 0, 857, 859, 5, 10, 0, 0,
        858, 860, 5, 126, 0, 0, 859, 858, 1, 0, 0, 0, 859, 860, 1, 0, 0, 0, 860, 861, 1, 0, 0, 0,
        861, 875, 3, 100, 50, 0, 862, 864, 5, 126, 0, 0, 863, 862, 1, 0, 0, 0, 863, 864, 1, 0, 0,
        0, 864, 865, 1, 0, 0, 0, 865, 867, 5, 11, 0, 0, 866, 868, 5, 10, 0, 0, 867, 866, 1, 0, 0,
        0, 867, 868, 1, 0, 0, 0, 868, 870, 1, 0, 0, 0, 869, 871, 5, 126, 0, 0, 870, 869, 1, 0, 0,
        0, 870, 871, 1, 0, 0, 0, 871, 872, 1, 0, 0, 0, 872, 874, 3, 100, 50, 0, 873, 863, 1, 0, 0,
        0, 874, 877, 1, 0, 0, 0, 875, 873, 1, 0, 0, 0, 875, 876, 1, 0, 0, 0, 876, 91, 1, 0, 0, 0,
        877, 875, 1, 0, 0, 0, 878, 885, 3, 94, 47, 0, 879, 881, 5, 126, 0, 0, 880, 879, 1, 0, 0, 0,
        880, 881, 1, 0, 0, 0, 881, 882, 1, 0, 0, 0, 882, 884, 3, 94, 47, 0, 883, 880, 1, 0, 0, 0,
        884, 887, 1, 0, 0, 0, 885, 883, 1, 0, 0, 0, 885, 886, 1, 0, 0, 0, 886, 93, 1, 0, 0, 0, 887,
        885, 1, 0, 0, 0, 888, 890, 5, 10, 0, 0, 889, 891, 5, 126, 0, 0, 890, 889, 1, 0, 0, 0, 890,
        891, 1, 0, 0, 0, 891, 892, 1, 0, 0, 0, 892, 906, 3, 98, 49, 0, 893, 895, 5, 126, 0, 0, 894,
        893, 1, 0, 0, 0, 894, 895, 1, 0, 0, 0, 895, 896, 1, 0, 0, 0, 896, 898, 5, 11, 0, 0, 897,
        899, 5, 10, 0, 0, 898, 897, 1, 0, 0, 0, 898, 899, 1, 0, 0, 0, 899, 901, 1, 0, 0, 0, 900,
        902, 5, 126, 0, 0, 901, 900, 1, 0, 0, 0, 901, 902, 1, 0, 0, 0, 902, 903, 1, 0, 0, 0, 903,
        905, 3, 98, 49, 0, 904, 894, 1, 0, 0, 0, 905, 908, 1, 0, 0, 0, 906, 904, 1, 0, 0, 0, 906,
        907, 1, 0, 0, 0, 907, 95, 1, 0, 0, 0, 908, 906, 1, 0, 0, 0, 909, 911, 5, 5, 0, 0, 910, 912,
        5, 126, 0, 0, 911, 910, 1, 0, 0, 0, 911, 912, 1, 0, 0, 0, 912, 917, 1, 0, 0, 0, 913, 915,
        3, 194, 97, 0, 914, 916, 5, 126, 0, 0, 915, 914, 1, 0, 0, 0, 915, 916, 1, 0, 0, 0, 916,
        918, 1, 0, 0, 0, 917, 913, 1, 0, 0, 0, 917, 918, 1, 0, 0, 0, 918, 929, 1, 0, 0, 0, 919,
        921, 5, 12, 0, 0, 920, 922, 5, 126, 0, 0, 921, 920, 1, 0, 0, 0, 921, 922, 1, 0, 0, 0, 922,
        927, 1, 0, 0, 0, 923, 925, 3, 194, 97, 0, 924, 926, 5, 126, 0, 0, 925, 924, 1, 0, 0, 0,
        925, 926, 1, 0, 0, 0, 926, 928, 1, 0, 0, 0, 927, 923, 1, 0, 0, 0, 927, 928, 1, 0, 0, 0,
        928, 930, 1, 0, 0, 0, 929, 919, 1, 0, 0, 0, 929, 930, 1, 0, 0, 0, 930, 97, 1, 0, 0, 0, 931,
        932, 3, 206, 103, 0, 932, 99, 1, 0, 0, 0, 933, 934, 3, 206, 103, 0, 934, 101, 1, 0, 0, 0,
        935, 940, 3, 140, 70, 0, 936, 938, 5, 126, 0, 0, 937, 936, 1, 0, 0, 0, 937, 938, 1, 0, 0,
        0, 938, 939, 1, 0, 0, 0, 939, 941, 3, 138, 69, 0, 940, 937, 1, 0, 0, 0, 941, 942, 1, 0, 0,
        0, 942, 940, 1, 0, 0, 0, 942, 943, 1, 0, 0, 0, 943, 103, 1, 0, 0, 0, 944, 945, 3, 106, 53,
        0, 945, 105, 1, 0, 0, 0, 946, 953, 3, 108, 54, 0, 947, 948, 5, 126, 0, 0, 948, 949, 5, 73,
        0, 0, 949, 950, 5, 126, 0, 0, 950, 952, 3, 108, 54, 0, 951, 947, 1, 0, 0, 0, 952, 955, 1,
        0, 0, 0, 953, 951, 1, 0, 0, 0, 953, 954, 1, 0, 0, 0, 954, 107, 1, 0, 0, 0, 955, 953, 1, 0,
        0, 0, 956, 963, 3, 110, 55, 0, 957, 958, 5, 126, 0, 0, 958, 959, 5, 74, 0, 0, 959, 960, 5,
        126, 0, 0, 960, 962, 3, 110, 55, 0, 961, 957, 1, 0, 0, 0, 962, 965, 1, 0, 0, 0, 963, 961,
        1, 0, 0, 0, 963, 964, 1, 0, 0, 0, 964, 109, 1, 0, 0, 0, 965, 963, 1, 0, 0, 0, 966, 973, 3,
        112, 56, 0, 967, 968, 5, 126, 0, 0, 968, 969, 5, 75, 0, 0, 969, 970, 5, 126, 0, 0, 970,
        972, 3, 112, 56, 0, 971, 967, 1, 0, 0, 0, 972, 975, 1, 0, 0, 0, 973, 971, 1, 0, 0, 0, 973,
        974, 1, 0, 0, 0, 974, 111, 1, 0, 0, 0, 975, 973, 1, 0, 0, 0, 976, 978, 5, 76, 0, 0, 977,
        979, 5, 126, 0, 0, 978, 977, 1, 0, 0, 0, 978, 979, 1, 0, 0, 0, 979, 981, 1, 0, 0, 0, 980,
        976, 1, 0, 0, 0, 981, 984, 1, 0, 0, 0, 982, 980, 1, 0, 0, 0, 982, 983, 1, 0, 0, 0, 983,
        985, 1, 0, 0, 0, 984, 982, 1, 0, 0, 0, 985, 986, 3, 114, 57, 0, 986, 113, 1, 0, 0, 0, 987,
        994, 3, 118, 59, 0, 988, 990, 5, 126, 0, 0, 989, 988, 1, 0, 0, 0, 989, 990, 1, 0, 0, 0,
        990, 991, 1, 0, 0, 0, 991, 993, 3, 116, 58, 0, 992, 989, 1, 0, 0, 0, 993, 996, 1, 0, 0, 0,
        994, 992, 1, 0, 0, 0, 994, 995, 1, 0, 0, 0, 995, 115, 1, 0, 0, 0, 996, 994, 1, 0, 0, 0,
        997, 999, 5, 3, 0, 0, 998, 1000, 5, 126, 0, 0, 999, 998, 1, 0, 0, 0, 999, 1000, 1, 0, 0, 0,
        1000, 1001, 1, 0, 0, 0, 1001, 1028, 3, 118, 59, 0, 1002, 1004, 5, 13, 0, 0, 1003, 1005, 5,
        126, 0, 0, 1004, 1003, 1, 0, 0, 0, 1004, 1005, 1, 0, 0, 0, 1005, 1006, 1, 0, 0, 0, 1006,
        1028, 3, 118, 59, 0, 1007, 1009, 5, 14, 0, 0, 1008, 1010, 5, 126, 0, 0, 1009, 1008, 1, 0,
        0, 0, 1009, 1010, 1, 0, 0, 0, 1010, 1011, 1, 0, 0, 0, 1011, 1028, 3, 118, 59, 0, 1012,
        1014, 5, 15, 0, 0, 1013, 1015, 5, 126, 0, 0, 1014, 1013, 1, 0, 0, 0, 1014, 1015, 1, 0, 0,
        0, 1015, 1016, 1, 0, 0, 0, 1016, 1028, 3, 118, 59, 0, 1017, 1019, 5, 16, 0, 0, 1018, 1020,
        5, 126, 0, 0, 1019, 1018, 1, 0, 0, 0, 1019, 1020, 1, 0, 0, 0, 1020, 1021, 1, 0, 0, 0, 1021,
        1028, 3, 118, 59, 0, 1022, 1024, 5, 17, 0, 0, 1023, 1025, 5, 126, 0, 0, 1024, 1023, 1, 0,
        0, 0, 1024, 1025, 1, 0, 0, 0, 1025, 1026, 1, 0, 0, 0, 1026, 1028, 3, 118, 59, 0, 1027, 997,
        1, 0, 0, 0, 1027, 1002, 1, 0, 0, 0, 1027, 1007, 1, 0, 0, 0, 1027, 1012, 1, 0, 0, 0, 1027,
        1017, 1, 0, 0, 0, 1027, 1022, 1, 0, 0, 0, 1028, 117, 1, 0, 0, 0, 1029, 1035, 3, 126, 63, 0,
        1030, 1034, 3, 120, 60, 0, 1031, 1034, 3, 122, 61, 0, 1032, 1034, 3, 124, 62, 0, 1033,
        1030, 1, 0, 0, 0, 1033, 1031, 1, 0, 0, 0, 1033, 1032, 1, 0, 0, 0, 1034, 1037, 1, 0, 0, 0,
        1035, 1033, 1, 0, 0, 0, 1035, 1036, 1, 0, 0, 0, 1036, 119, 1, 0, 0, 0, 1037, 1035, 1, 0, 0,
        0, 1038, 1039, 5, 126, 0, 0, 1039, 1040, 5, 77, 0, 0, 1040, 1041, 5, 126, 0, 0, 1041, 1049,
        5, 61, 0, 0, 1042, 1043, 5, 126, 0, 0, 1043, 1044, 5, 78, 0, 0, 1044, 1045, 5, 126, 0, 0,
        1045, 1049, 5, 61, 0, 0, 1046, 1047, 5, 126, 0, 0, 1047, 1049, 5, 79, 0, 0, 1048, 1038, 1,
        0, 0, 0, 1048, 1042, 1, 0, 0, 0, 1048, 1046, 1, 0, 0, 0, 1049, 1051, 1, 0, 0, 0, 1050,
        1052, 5, 126, 0, 0, 1051, 1050, 1, 0, 0, 0, 1051, 1052, 1, 0, 0, 0, 1052, 1053, 1, 0, 0, 0,
        1053, 1054, 3, 126, 63, 0, 1054, 121, 1, 0, 0, 0, 1055, 1056, 5, 126, 0, 0, 1056, 1058, 5,
        80, 0, 0, 1057, 1059, 5, 126, 0, 0, 1058, 1057, 1, 0, 0, 0, 1058, 1059, 1, 0, 0, 0, 1059,
        1060, 1, 0, 0, 0, 1060, 1061, 3, 126, 63, 0, 1061, 123, 1, 0, 0, 0, 1062, 1063, 5, 126, 0,
        0, 1063, 1064, 5, 81, 0, 0, 1064, 1065, 5, 126, 0, 0, 1065, 1073, 5, 82, 0, 0, 1066, 1067,
        5, 126, 0, 0, 1067, 1068, 5, 81, 0, 0, 1068, 1069, 5, 126, 0, 0, 1069, 1070, 5, 76, 0, 0,
        1070, 1071, 5, 126, 0, 0, 1071, 1073, 5, 82, 0, 0, 1072, 1062, 1, 0, 0, 0, 1072, 1066, 1,
        0, 0, 0, 1073, 125, 1, 0, 0, 0, 1074, 1093, 3, 128, 64, 0, 1075, 1077, 5, 126, 0, 0, 1076,
        1075, 1, 0, 0, 0, 1076, 1077, 1, 0, 0, 0, 1077, 1078, 1, 0, 0, 0, 1078, 1080, 5, 18, 0, 0,
        1079, 1081, 5, 126, 0, 0, 1080, 1079, 1, 0, 0, 0, 1080, 1081, 1, 0, 0, 0, 1081, 1082, 1, 0,
        0, 0, 1082, 1092, 3, 128, 64, 0, 1083, 1085, 5, 126, 0, 0, 1084, 1083, 1, 0, 0, 0, 1084,
        1085, 1, 0, 0, 0, 1085, 1086, 1, 0, 0, 0, 1086, 1088, 5, 19, 0, 0, 1087, 1089, 5, 126, 0,
        0, 1088, 1087, 1, 0, 0, 0, 1088, 1089, 1, 0, 0, 0, 1089, 1090, 1, 0, 0, 0, 1090, 1092, 3,
        128, 64, 0, 1091, 1076, 1, 0, 0, 0, 1091, 1084, 1, 0, 0, 0, 1092, 1095, 1, 0, 0, 0, 1093,
        1091, 1, 0, 0, 0, 1093, 1094, 1, 0, 0, 0, 1094, 127, 1, 0, 0, 0, 1095, 1093, 1, 0, 0, 0,
        1096, 1123, 3, 130, 65, 0, 1097, 1099, 5, 126, 0, 0, 1098, 1097, 1, 0, 0, 0, 1098, 1099, 1,
        0, 0, 0, 1099, 1100, 1, 0, 0, 0, 1100, 1102, 5, 5, 0, 0, 1101, 1103, 5, 126, 0, 0, 1102,
        1101, 1, 0, 0, 0, 1102, 1103, 1, 0, 0, 0, 1103, 1104, 1, 0, 0, 0, 1104, 1122, 3, 130, 65,
        0, 1105, 1107, 5, 126, 0, 0, 1106, 1105, 1, 0, 0, 0, 1106, 1107, 1, 0, 0, 0, 1107, 1108, 1,
        0, 0, 0, 1108, 1110, 5, 20, 0, 0, 1109, 1111, 5, 126, 0, 0, 1110, 1109, 1, 0, 0, 0, 1110,
        1111, 1, 0, 0, 0, 1111, 1112, 1, 0, 0, 0, 1112, 1122, 3, 130, 65, 0, 1113, 1115, 5, 126, 0,
        0, 1114, 1113, 1, 0, 0, 0, 1114, 1115, 1, 0, 0, 0, 1115, 1116, 1, 0, 0, 0, 1116, 1118, 5,
        21, 0, 0, 1117, 1119, 5, 126, 0, 0, 1118, 1117, 1, 0, 0, 0, 1118, 1119, 1, 0, 0, 0, 1119,
        1120, 1, 0, 0, 0, 1120, 1122, 3, 130, 65, 0, 1121, 1098, 1, 0, 0, 0, 1121, 1106, 1, 0, 0,
        0, 1121, 1114, 1, 0, 0, 0, 1122, 1125, 1, 0, 0, 0, 1123, 1121, 1, 0, 0, 0, 1123, 1124, 1,
        0, 0, 0, 1124, 129, 1, 0, 0, 0, 1125, 1123, 1, 0, 0, 0, 1126, 1137, 3, 132, 66, 0, 1127,
        1129, 5, 126, 0, 0, 1128, 1127, 1, 0, 0, 0, 1128, 1129, 1, 0, 0, 0, 1129, 1130, 1, 0, 0, 0,
        1130, 1132, 5, 22, 0, 0, 1131, 1133, 5, 126, 0, 0, 1132, 1131, 1, 0, 0, 0, 1132, 1133, 1,
        0, 0, 0, 1133, 1134, 1, 0, 0, 0, 1134, 1136, 3, 132, 66, 0, 1135, 1128, 1, 0, 0, 0, 1136,
        1139, 1, 0, 0, 0, 1137, 1135, 1, 0, 0, 0, 1137, 1138, 1, 0, 0, 0, 1138, 131, 1, 0, 0, 0,
        1139, 1137, 1, 0, 0, 0, 1140, 1147, 3, 134, 67, 0, 1141, 1143, 7, 2, 0, 0, 1142, 1144, 5,
        126, 0, 0, 1143, 1142, 1, 0, 0, 0, 1143, 1144, 1, 0, 0, 0, 1144, 1145, 1, 0, 0, 0, 1145,
        1147, 3, 134, 67, 0, 1146, 1140, 1, 0, 0, 0, 1146, 1141, 1, 0, 0, 0, 1147, 133, 1, 0, 0, 0,
        1148, 1159, 3, 140, 70, 0, 1149, 1151, 5, 126, 0, 0, 1150, 1149, 1, 0, 0, 0, 1150, 1151, 1,
        0, 0, 0, 1151, 1152, 1, 0, 0, 0, 1152, 1158, 3, 136, 68, 0, 1153, 1155, 5, 126, 0, 0, 1154,
        1153, 1, 0, 0, 0, 1154, 1155, 1, 0, 0, 0, 1155, 1156, 1, 0, 0, 0, 1156, 1158, 3, 138, 69,
        0, 1157, 1150, 1, 0, 0, 0, 1157, 1154, 1, 0, 0, 0, 1158, 1161, 1, 0, 0, 0, 1159, 1157, 1,
        0, 0, 0, 1159, 1160, 1, 0, 0, 0, 1160, 1166, 1, 0, 0, 0, 1161, 1159, 1, 0, 0, 0, 1162,
        1164, 5, 126, 0, 0, 1163, 1162, 1, 0, 0, 0, 1163, 1164, 1, 0, 0, 0, 1164, 1165, 1, 0, 0, 0,
        1165, 1167, 3, 92, 46, 0, 1166, 1163, 1, 0, 0, 0, 1166, 1167, 1, 0, 0, 0, 1167, 135, 1, 0,
        0, 0, 1168, 1169, 5, 8, 0, 0, 1169, 1170, 3, 104, 52, 0, 1170, 1171, 5, 9, 0, 0, 1171,
        1182, 1, 0, 0, 0, 1172, 1174, 5, 8, 0, 0, 1173, 1175, 3, 104, 52, 0, 1174, 1173, 1, 0, 0,
        0, 1174, 1175, 1, 0, 0, 0, 1175, 1176, 1, 0, 0, 0, 1176, 1178, 5, 12, 0, 0, 1177, 1179, 3,
        104, 52, 0, 1178, 1177, 1, 0, 0, 0, 1178, 1179, 1, 0, 0, 0, 1179, 1180, 1, 0, 0, 0, 1180,
        1182, 5, 9, 0, 0, 1181, 1168, 1, 0, 0, 0, 1181, 1172, 1, 0, 0, 0, 1182, 137, 1, 0, 0, 0,
        1183, 1185, 5, 23, 0, 0, 1184, 1186, 5, 126, 0, 0, 1185, 1184, 1, 0, 0, 0, 1185, 1186, 1,
        0, 0, 0, 1186, 1189, 1, 0, 0, 0, 1187, 1190, 3, 202, 101, 0, 1188, 1190, 5, 5, 0, 0, 1189,
        1187, 1, 0, 0, 0, 1189, 1188, 1, 0, 0, 0, 1190, 139, 1, 0, 0, 0, 1191, 1217, 3, 188, 94, 0,
        1192, 1217, 3, 204, 102, 0, 1193, 1217, 3, 142, 71, 0, 1194, 1217, 3, 162, 81, 0, 1195,
        1197, 5, 83, 0, 0, 1196, 1198, 5, 126, 0, 0, 1197, 1196, 1, 0, 0, 0, 1197, 1198, 1, 0, 0,
        0, 1198, 1199, 1, 0, 0, 0, 1199, 1201, 5, 6, 0, 0, 1200, 1202, 5, 126, 0, 0, 1201, 1200, 1,
        0, 0, 0, 1201, 1202, 1, 0, 0, 0, 1202, 1203, 1, 0, 0, 0, 1203, 1205, 5, 5, 0, 0, 1204,
        1206, 5, 126, 0, 0, 1205, 1204, 1, 0, 0, 0, 1205, 1206, 1, 0, 0, 0, 1206, 1207, 1, 0, 0, 0,
        1207, 1217, 5, 7, 0, 0, 1208, 1217, 3, 146, 73, 0, 1209, 1217, 3, 148, 74, 0, 1210, 1217,
        3, 150, 75, 0, 1211, 1217, 3, 154, 77, 0, 1212, 1217, 3, 156, 78, 0, 1213, 1217, 3, 160,
        80, 0, 1214, 1217, 3, 174, 87, 0, 1215, 1217, 3, 186, 93, 0, 1216, 1191, 1, 0, 0, 0, 1216,
        1192, 1, 0, 0, 0, 1216, 1193, 1, 0, 0, 0, 1216, 1194, 1, 0, 0, 0, 1216, 1195, 1, 0, 0, 0,
        1216, 1208, 1, 0, 0, 0, 1216, 1209, 1, 0, 0, 0, 1216, 1210, 1, 0, 0, 0, 1216, 1211, 1, 0,
        0, 0, 1216, 1212, 1, 0, 0, 0, 1216, 1213, 1, 0, 0, 0, 1216, 1214, 1, 0, 0, 0, 1216, 1215,
        1, 0, 0, 0, 1217, 141, 1, 0, 0, 0, 1218, 1223, 5, 84, 0, 0, 1219, 1221, 5, 126, 0, 0, 1220,
        1219, 1, 0, 0, 0, 1220, 1221, 1, 0, 0, 0, 1221, 1222, 1, 0, 0, 0, 1222, 1224, 3, 144, 72,
        0, 1223, 1220, 1, 0, 0, 0, 1224, 1225, 1, 0, 0, 0, 1225, 1223, 1, 0, 0, 0, 1225, 1226, 1,
        0, 0, 0, 1226, 1241, 1, 0, 0, 0, 1227, 1229, 5, 84, 0, 0, 1228, 1230, 5, 126, 0, 0, 1229,
        1228, 1, 0, 0, 0, 1229, 1230, 1, 0, 0, 0, 1230, 1231, 1, 0, 0, 0, 1231, 1236, 3, 104, 52,
        0, 1232, 1234, 5, 126, 0, 0, 1233, 1232, 1, 0, 0, 0, 1233, 1234, 1, 0, 0, 0, 1234, 1235, 1,
        0, 0, 0, 1235, 1237, 3, 144, 72, 0, 1236, 1233, 1, 0, 0, 0, 1237, 1238, 1, 0, 0, 0, 1238,
        1236, 1, 0, 0, 0, 1238, 1239, 1, 0, 0, 0, 1239, 1241, 1, 0, 0, 0, 1240, 1218, 1, 0, 0, 0,
        1240, 1227, 1, 0, 0, 0, 1241, 1250, 1, 0, 0, 0, 1242, 1244, 5, 126, 0, 0, 1243, 1242, 1, 0,
        0, 0, 1243, 1244, 1, 0, 0, 0, 1244, 1245, 1, 0, 0, 0, 1245, 1247, 5, 85, 0, 0, 1246, 1248,
        5, 126, 0, 0, 1247, 1246, 1, 0, 0, 0, 1247, 1248, 1, 0, 0, 0, 1248, 1249, 1, 0, 0, 0, 1249,
        1251, 3, 104, 52, 0, 1250, 1243, 1, 0, 0, 0, 1250, 1251, 1, 0, 0, 0, 1251, 1253, 1, 0, 0,
        0, 1252, 1254, 5, 126, 0, 0, 1253, 1252, 1, 0, 0, 0, 1253, 1254, 1, 0, 0, 0, 1254, 1255, 1,
        0, 0, 0, 1255, 1256, 5, 86, 0, 0, 1256, 143, 1, 0, 0, 0, 1257, 1259, 5, 87, 0, 0, 1258,
        1260, 5, 126, 0, 0, 1259, 1258, 1, 0, 0, 0, 1259, 1260, 1, 0, 0, 0, 1260, 1261, 1, 0, 0, 0,
        1261, 1263, 3, 104, 52, 0, 1262, 1264, 5, 126, 0, 0, 1263, 1262, 1, 0, 0, 0, 1263, 1264, 1,
        0, 0, 0, 1264, 1265, 1, 0, 0, 0, 1265, 1267, 5, 88, 0, 0, 1266, 1268, 5, 126, 0, 0, 1267,
        1266, 1, 0, 0, 0, 1267, 1268, 1, 0, 0, 0, 1268, 1269, 1, 0, 0, 0, 1269, 1270, 3, 104, 52,
        0, 1270, 145, 1, 0, 0, 0, 1271, 1273, 5, 8, 0, 0, 1272, 1274, 5, 126, 0, 0, 1273, 1272, 1,
        0, 0, 0, 1273, 1274, 1, 0, 0, 0, 1274, 1275, 1, 0, 0, 0, 1275, 1284, 3, 152, 76, 0, 1276,
        1278, 5, 126, 0, 0, 1277, 1276, 1, 0, 0, 0, 1277, 1278, 1, 0, 0, 0, 1278, 1279, 1, 0, 0, 0,
        1279, 1281, 5, 11, 0, 0, 1280, 1282, 5, 126, 0, 0, 1281, 1280, 1, 0, 0, 0, 1281, 1282, 1,
        0, 0, 0, 1282, 1283, 1, 0, 0, 0, 1283, 1285, 3, 104, 52, 0, 1284, 1277, 1, 0, 0, 0, 1284,
        1285, 1, 0, 0, 0, 1285, 1287, 1, 0, 0, 0, 1286, 1288, 5, 126, 0, 0, 1287, 1286, 1, 0, 0, 0,
        1287, 1288, 1, 0, 0, 0, 1288, 1289, 1, 0, 0, 0, 1289, 1290, 5, 9, 0, 0, 1290, 147, 1, 0, 0,
        0, 1291, 1293, 5, 8, 0, 0, 1292, 1294, 5, 126, 0, 0, 1293, 1292, 1, 0, 0, 0, 1293, 1294, 1,
        0, 0, 0, 1294, 1303, 1, 0, 0, 0, 1295, 1297, 3, 186, 93, 0, 1296, 1298, 5, 126, 0, 0, 1297,
        1296, 1, 0, 0, 0, 1297, 1298, 1, 0, 0, 0, 1298, 1299, 1, 0, 0, 0, 1299, 1301, 5, 3, 0, 0,
        1300, 1302, 5, 126, 0, 0, 1301, 1300, 1, 0, 0, 0, 1301, 1302, 1, 0, 0, 0, 1302, 1304, 1, 0,
        0, 0, 1303, 1295, 1, 0, 0, 0, 1303, 1304, 1, 0, 0, 0, 1304, 1305, 1, 0, 0, 0, 1305, 1307,
        3, 76, 38, 0, 1306, 1308, 5, 126, 0, 0, 1307, 1306, 1, 0, 0, 0, 1307, 1308, 1, 0, 0, 0,
        1308, 1313, 1, 0, 0, 0, 1309, 1311, 3, 66, 33, 0, 1310, 1312, 5, 126, 0, 0, 1311, 1310, 1,
        0, 0, 0, 1311, 1312, 1, 0, 0, 0, 1312, 1314, 1, 0, 0, 0, 1313, 1309, 1, 0, 0, 0, 1313,
        1314, 1, 0, 0, 0, 1314, 1315, 1, 0, 0, 0, 1315, 1317, 5, 11, 0, 0, 1316, 1318, 5, 126, 0,
        0, 1317, 1316, 1, 0, 0, 0, 1317, 1318, 1, 0, 0, 0, 1318, 1319, 1, 0, 0, 0, 1319, 1321, 3,
        104, 52, 0, 1320, 1322, 5, 126, 0, 0, 1321, 1320, 1, 0, 0, 0, 1321, 1322, 1, 0, 0, 0, 1322,
        1323, 1, 0, 0, 0, 1323, 1324, 5, 9, 0, 0, 1324, 149, 1, 0, 0, 0, 1325, 1327, 5, 47, 0, 0,
        1326, 1328, 5, 126, 0, 0, 1327, 1326, 1, 0, 0, 0, 1327, 1328, 1, 0, 0, 0, 1328, 1329, 1, 0,
        0, 0, 1329, 1331, 5, 6, 0, 0, 1330, 1332, 5, 126, 0, 0, 1331, 1330, 1, 0, 0, 0, 1331, 1332,
        1, 0, 0, 0, 1332, 1333, 1, 0, 0, 0, 1333, 1335, 3, 152, 76, 0, 1334, 1336, 5, 126, 0, 0,
        1335, 1334, 1, 0, 0, 0, 1335, 1336, 1, 0, 0, 0, 1336, 1337, 1, 0, 0, 0, 1337, 1338, 5, 7,
        0, 0, 1338, 1382, 1, 0, 0, 0, 1339, 1341, 5, 89, 0, 0, 1340, 1342, 5, 126, 0, 0, 1341,
        1340, 1, 0, 0, 0, 1341, 1342, 1, 0, 0, 0, 1342, 1343, 1, 0, 0, 0, 1343, 1345, 5, 6, 0, 0,
        1344, 1346, 5, 126, 0, 0, 1345, 1344, 1, 0, 0, 0, 1345, 1346, 1, 0, 0, 0, 1346, 1347, 1, 0,
        0, 0, 1347, 1349, 3, 152, 76, 0, 1348, 1350, 5, 126, 0, 0, 1349, 1348, 1, 0, 0, 0, 1349,
        1350, 1, 0, 0, 0, 1350, 1351, 1, 0, 0, 0, 1351, 1352, 5, 7, 0, 0, 1352, 1382, 1, 0, 0, 0,
        1353, 1355, 5, 90, 0, 0, 1354, 1356, 5, 126, 0, 0, 1355, 1354, 1, 0, 0, 0, 1355, 1356, 1,
        0, 0, 0, 1356, 1357, 1, 0, 0, 0, 1357, 1359, 5, 6, 0, 0, 1358, 1360, 5, 126, 0, 0, 1359,
        1358, 1, 0, 0, 0, 1359, 1360, 1, 0, 0, 0, 1360, 1361, 1, 0, 0, 0, 1361, 1363, 3, 152, 76,
        0, 1362, 1364, 5, 126, 0, 0, 1363, 1362, 1, 0, 0, 0, 1363, 1364, 1, 0, 0, 0, 1364, 1365, 1,
        0, 0, 0, 1365, 1366, 5, 7, 0, 0, 1366, 1382, 1, 0, 0, 0, 1367, 1369, 5, 91, 0, 0, 1368,
        1370, 5, 126, 0, 0, 1369, 1368, 1, 0, 0, 0, 1369, 1370, 1, 0, 0, 0, 1370, 1371, 1, 0, 0, 0,
        1371, 1373, 5, 6, 0, 0, 1372, 1374, 5, 126, 0, 0, 1373, 1372, 1, 0, 0, 0, 1373, 1374, 1, 0,
        0, 0, 1374, 1375, 1, 0, 0, 0, 1375, 1377, 3, 152, 76, 0, 1376, 1378, 5, 126, 0, 0, 1377,
        1376, 1, 0, 0, 0, 1377, 1378, 1, 0, 0, 0, 1378, 1379, 1, 0, 0, 0, 1379, 1380, 5, 7, 0, 0,
        1380, 1382, 1, 0, 0, 0, 1381, 1325, 1, 0, 0, 0, 1381, 1339, 1, 0, 0, 0, 1381, 1353, 1, 0,
        0, 0, 1381, 1367, 1, 0, 0, 0, 1382, 151, 1, 0, 0, 0, 1383, 1388, 3, 158, 79, 0, 1384, 1386,
        5, 126, 0, 0, 1385, 1384, 1, 0, 0, 0, 1385, 1386, 1, 0, 0, 0, 1386, 1387, 1, 0, 0, 0, 1387,
        1389, 3, 66, 33, 0, 1388, 1385, 1, 0, 0, 0, 1388, 1389, 1, 0, 0, 0, 1389, 153, 1, 0, 0, 0,
        1390, 1391, 3, 76, 38, 0, 1391, 155, 1, 0, 0, 0, 1392, 1394, 5, 6, 0, 0, 1393, 1395, 5,
        126, 0, 0, 1394, 1393, 1, 0, 0, 0, 1394, 1395, 1, 0, 0, 0, 1395, 1396, 1, 0, 0, 0, 1396,
        1398, 3, 104, 52, 0, 1397, 1399, 5, 126, 0, 0, 1398, 1397, 1, 0, 0, 0, 1398, 1399, 1, 0, 0,
        0, 1399, 1400, 1, 0, 0, 0, 1400, 1401, 5, 7, 0, 0, 1401, 157, 1, 0, 0, 0, 1402, 1403, 3,
        186, 93, 0, 1403, 1404, 5, 126, 0, 0, 1404, 1405, 5, 80, 0, 0, 1405, 1406, 5, 126, 0, 0,
        1406, 1407, 3, 104, 52, 0, 1407, 159, 1, 0, 0, 0, 1408, 1410, 3, 172, 86, 0, 1409, 1411, 5,
        126, 0, 0, 1410, 1409, 1, 0, 0, 0, 1410, 1411, 1, 0, 0, 0, 1411, 1412, 1, 0, 0, 0, 1412,
        1414, 5, 6, 0, 0, 1413, 1415, 5, 126, 0, 0, 1414, 1413, 1, 0, 0, 0, 1414, 1415, 1, 0, 0, 0,
        1415, 1420, 1, 0, 0, 0, 1416, 1418, 5, 63, 0, 0, 1417, 1419, 5, 126, 0, 0, 1418, 1417, 1,
        0, 0, 0, 1418, 1419, 1, 0, 0, 0, 1419, 1421, 1, 0, 0, 0, 1420, 1416, 1, 0, 0, 0, 1420,
        1421, 1, 0, 0, 0, 1421, 1439, 1, 0, 0, 0, 1422, 1424, 3, 104, 52, 0, 1423, 1425, 5, 126, 0,
        0, 1424, 1423, 1, 0, 0, 0, 1424, 1425, 1, 0, 0, 0, 1425, 1436, 1, 0, 0, 0, 1426, 1428, 5,
        2, 0, 0, 1427, 1429, 5, 126, 0, 0, 1428, 1427, 1, 0, 0, 0, 1428, 1429, 1, 0, 0, 0, 1429,
        1430, 1, 0, 0, 0, 1430, 1432, 3, 104, 52, 0, 1431, 1433, 5, 126, 0, 0, 1432, 1431, 1, 0, 0,
        0, 1432, 1433, 1, 0, 0, 0, 1433, 1435, 1, 0, 0, 0, 1434, 1426, 1, 0, 0, 0, 1435, 1438, 1,
        0, 0, 0, 1436, 1434, 1, 0, 0, 0, 1436, 1437, 1, 0, 0, 0, 1437, 1440, 1, 0, 0, 0, 1438,
        1436, 1, 0, 0, 0, 1439, 1422, 1, 0, 0, 0, 1439, 1440, 1, 0, 0, 0, 1440, 1441, 1, 0, 0, 0,
        1441, 1442, 5, 7, 0, 0, 1442, 161, 1, 0, 0, 0, 1443, 1445, 5, 92, 0, 0, 1444, 1446, 5, 126,
        0, 0, 1445, 1444, 1, 0, 0, 0, 1445, 1446, 1, 0, 0, 0, 1446, 1447, 1, 0, 0, 0, 1447, 1449,
        5, 6, 0, 0, 1448, 1450, 5, 126, 0, 0, 1449, 1448, 1, 0, 0, 0, 1449, 1450, 1, 0, 0, 0, 1450,
        1451, 1, 0, 0, 0, 1451, 1453, 3, 104, 52, 0, 1452, 1454, 5, 126, 0, 0, 1453, 1452, 1, 0, 0,
        0, 1453, 1454, 1, 0, 0, 0, 1454, 1463, 1, 0, 0, 0, 1455, 1457, 5, 2, 0, 0, 1456, 1458, 5,
        126, 0, 0, 1457, 1456, 1, 0, 0, 0, 1457, 1458, 1, 0, 0, 0, 1458, 1459, 1, 0, 0, 0, 1459,
        1464, 3, 104, 52, 0, 1460, 1461, 5, 51, 0, 0, 1461, 1462, 5, 126, 0, 0, 1462, 1464, 3, 164,
        82, 0, 1463, 1455, 1, 0, 0, 0, 1463, 1460, 1, 0, 0, 0, 1464, 1466, 1, 0, 0, 0, 1465, 1467,
        5, 126, 0, 0, 1466, 1465, 1, 0, 0, 0, 1466, 1467, 1, 0, 0, 0, 1467, 1468, 1, 0, 0, 0, 1468,
        1469, 5, 7, 0, 0, 1469, 163, 1, 0, 0, 0, 1470, 1497, 3, 170, 85, 0, 1471, 1473, 5, 126, 0,
        0, 1472, 1471, 1, 0, 0, 0, 1472, 1473, 1, 0, 0, 0, 1473, 1474, 1, 0, 0, 0, 1474, 1476, 5,
        6, 0, 0, 1475, 1477, 5, 126, 0, 0, 1476, 1475, 1, 0, 0, 0, 1476, 1477, 1, 0, 0, 0, 1477,
        1478, 1, 0, 0, 0, 1478, 1489, 3, 166, 83, 0, 1479, 1481, 5, 126, 0, 0, 1480, 1479, 1, 0, 0,
        0, 1480, 1481, 1, 0, 0, 0, 1481, 1482, 1, 0, 0, 0, 1482, 1484, 5, 2, 0, 0, 1483, 1485, 5,
        126, 0, 0, 1484, 1483, 1, 0, 0, 0, 1484, 1485, 1, 0, 0, 0, 1485, 1486, 1, 0, 0, 0, 1486,
        1488, 3, 166, 83, 0, 1487, 1480, 1, 0, 0, 0, 1488, 1491, 1, 0, 0, 0, 1489, 1487, 1, 0, 0,
        0, 1489, 1490, 1, 0, 0, 0, 1490, 1493, 1, 0, 0, 0, 1491, 1489, 1, 0, 0, 0, 1492, 1494, 5,
        126, 0, 0, 1493, 1492, 1, 0, 0, 0, 1493, 1494, 1, 0, 0, 0, 1494, 1495, 1, 0, 0, 0, 1495,
        1496, 5, 7, 0, 0, 1496, 1498, 1, 0, 0, 0, 1497, 1472, 1, 0, 0, 0, 1497, 1498, 1, 0, 0, 0,
        1498, 1515, 1, 0, 0, 0, 1499, 1501, 5, 126, 0, 0, 1500, 1499, 1, 0, 0, 0, 1500, 1501, 1, 0,
        0, 0, 1501, 1502, 1, 0, 0, 0, 1502, 1504, 5, 8, 0, 0, 1503, 1505, 5, 126, 0, 0, 1504, 1503,
        1, 0, 0, 0, 1504, 1505, 1, 0, 0, 0, 1505, 1507, 1, 0, 0, 0, 1506, 1508, 5, 97, 0, 0, 1507,
        1506, 1, 0, 0, 0, 1507, 1508, 1, 0, 0, 0, 1508, 1510, 1, 0, 0, 0, 1509, 1511, 5, 126, 0, 0,
        1510, 1509, 1, 0, 0, 0, 1510, 1511, 1, 0, 0, 0, 1511, 1512, 1, 0, 0, 0, 1512, 1514, 5, 9,
        0, 0, 1513, 1500, 1, 0, 0, 0, 1514, 1517, 1, 0, 0, 0, 1515, 1513, 1, 0, 0, 0, 1515, 1516,
        1, 0, 0, 0, 1516, 165, 1, 0, 0, 0, 1517, 1515, 1, 0, 0, 0, 1518, 1522, 5, 97, 0, 0, 1519,
        1522, 3, 168, 84, 0, 1520, 1522, 3, 164, 82, 0, 1521, 1518, 1, 0, 0, 0, 1521, 1519, 1, 0,
        0, 0, 1521, 1520, 1, 0, 0, 0, 1522, 167, 1, 0, 0, 0, 1523, 1524, 3, 170, 85, 0, 1524, 1525,
        5, 126, 0, 0, 1525, 1526, 3, 164, 82, 0, 1526, 169, 1, 0, 0, 0, 1527, 1528, 3, 206, 103, 0,
        1528, 171, 1, 0, 0, 0, 1529, 1530, 3, 184, 92, 0, 1530, 1531, 3, 210, 105, 0, 1531, 173, 1,
        0, 0, 0, 1532, 1534, 5, 93, 0, 0, 1533, 1535, 5, 126, 0, 0, 1534, 1533, 1, 0, 0, 0, 1534,
        1535, 1, 0, 0, 0, 1535, 1536, 1, 0, 0, 0, 1536, 1538, 5, 24, 0, 0, 1537, 1539, 5, 126, 0,
        0, 1538, 1537, 1, 0, 0, 0, 1538, 1539, 1, 0, 0, 0, 1539, 1559, 1, 0, 0, 0, 1540, 1542, 5,
        49, 0, 0, 1541, 1543, 5, 126, 0, 0, 1542, 1541, 1, 0, 0, 0, 1542, 1543, 1, 0, 0, 0, 1543,
        1544, 1, 0, 0, 0, 1544, 1549, 3, 68, 34, 0, 1545, 1547, 5, 126, 0, 0, 1546, 1545, 1, 0, 0,
        0, 1546, 1547, 1, 0, 0, 0, 1547, 1548, 1, 0, 0, 0, 1548, 1550, 3, 66, 33, 0, 1549, 1546, 1,
        0, 0, 0, 1549, 1550, 1, 0, 0, 0, 1550, 1560, 1, 0, 0, 0, 1551, 1560, 3, 6, 3, 0, 1552,
        1557, 3, 68, 34, 0, 1553, 1555, 5, 126, 0, 0, 1554, 1553, 1, 0, 0, 0, 1554, 1555, 1, 0, 0,
        0, 1555, 1556, 1, 0, 0, 0, 1556, 1558, 3, 66, 33, 0, 1557, 1554, 1, 0, 0, 0, 1557, 1558, 1,
        0, 0, 0, 1558, 1560, 1, 0, 0, 0, 1559, 1540, 1, 0, 0, 0, 1559, 1551, 1, 0, 0, 0, 1559,
        1552, 1, 0, 0, 0, 1560, 1562, 1, 0, 0, 0, 1561, 1563, 5, 126, 0, 0, 1562, 1561, 1, 0, 0, 0,
        1562, 1563, 1, 0, 0, 0, 1563, 1564, 1, 0, 0, 0, 1564, 1565, 5, 25, 0, 0, 1565, 175, 1, 0,
        0, 0, 1566, 1568, 3, 182, 91, 0, 1567, 1569, 5, 126, 0, 0, 1568, 1567, 1, 0, 0, 0, 1568,
        1569, 1, 0, 0, 0, 1569, 1570, 1, 0, 0, 0, 1570, 1572, 5, 6, 0, 0, 1571, 1573, 5, 126, 0, 0,
        1572, 1571, 1, 0, 0, 0, 1572, 1573, 1, 0, 0, 0, 1573, 1591, 1, 0, 0, 0, 1574, 1576, 3, 104,
        52, 0, 1575, 1577, 5, 126, 0, 0, 1576, 1575, 1, 0, 0, 0, 1576, 1577, 1, 0, 0, 0, 1577,
        1588, 1, 0, 0, 0, 1578, 1580, 5, 2, 0, 0, 1579, 1581, 5, 126, 0, 0, 1580, 1579, 1, 0, 0, 0,
        1580, 1581, 1, 0, 0, 0, 1581, 1582, 1, 0, 0, 0, 1582, 1584, 3, 104, 52, 0, 1583, 1585, 5,
        126, 0, 0, 1584, 1583, 1, 0, 0, 0, 1584, 1585, 1, 0, 0, 0, 1585, 1587, 1, 0, 0, 0, 1586,
        1578, 1, 0, 0, 0, 1587, 1590, 1, 0, 0, 0, 1588, 1586, 1, 0, 0, 0, 1588, 1589, 1, 0, 0, 0,
        1589, 1592, 1, 0, 0, 0, 1590, 1588, 1, 0, 0, 0, 1591, 1574, 1, 0, 0, 0, 1591, 1592, 1, 0,
        0, 0, 1592, 1593, 1, 0, 0, 0, 1593, 1594, 5, 7, 0, 0, 1594, 177, 1, 0, 0, 0, 1595, 1596, 3,
        182, 91, 0, 1596, 179, 1, 0, 0, 0, 1597, 1598, 3, 210, 105, 0, 1598, 181, 1, 0, 0, 0, 1599,
        1600, 3, 184, 92, 0, 1600, 1601, 3, 210, 105, 0, 1601, 183, 1, 0, 0, 0, 1602, 1603, 3, 210,
        105, 0, 1603, 1604, 5, 23, 0, 0, 1604, 1606, 1, 0, 0, 0, 1605, 1602, 1, 0, 0, 0, 1606,
        1609, 1, 0, 0, 0, 1607, 1605, 1, 0, 0, 0, 1607, 1608, 1, 0, 0, 0, 1608, 185, 1, 0, 0, 0,
        1609, 1607, 1, 0, 0, 0, 1610, 1611, 3, 210, 105, 0, 1611, 187, 1, 0, 0, 0, 1612, 1619, 3,
        190, 95, 0, 1613, 1619, 5, 82, 0, 0, 1614, 1619, 3, 192, 96, 0, 1615, 1619, 5, 108, 0, 0,
        1616, 1619, 3, 198, 99, 0, 1617, 1619, 3, 200, 100, 0, 1618, 1612, 1, 0, 0, 0, 1618, 1613,
        1, 0, 0, 0, 1618, 1614, 1, 0, 0, 0, 1618, 1615, 1, 0, 0, 0, 1618, 1616, 1, 0, 0, 0, 1618,
        1617, 1, 0, 0, 0, 1619, 189, 1, 0, 0, 0, 1620, 1621, 7, 3, 0, 0, 1621, 191, 1, 0, 0, 0,
        1622, 1625, 3, 196, 98, 0, 1623, 1625, 3, 194, 97, 0, 1624, 1622, 1, 0, 0, 0, 1624, 1623,
        1, 0, 0, 0, 1625, 193, 1, 0, 0, 0, 1626, 1627, 7, 4, 0, 0, 1627, 195, 1, 0, 0, 0, 1628,
        1629, 7, 5, 0, 0, 1629, 197, 1, 0, 0, 0, 1630, 1632, 5, 8, 0, 0, 1631, 1633, 5, 126, 0, 0,
        1632, 1631, 1, 0, 0, 0, 1632, 1633, 1, 0, 0, 0, 1633, 1651, 1, 0, 0, 0, 1634, 1636, 3, 104,
        52, 0, 1635, 1637, 5, 126, 0, 0, 1636, 1635, 1, 0, 0, 0, 1636, 1637, 1, 0, 0, 0, 1637,
        1648, 1, 0, 0, 0, 1638, 1640, 5, 2, 0, 0, 1639, 1641, 5, 126, 0, 0, 1640, 1639, 1, 0, 0, 0,
        1640, 1641, 1, 0, 0, 0, 1641, 1642, 1, 0, 0, 0, 1642, 1644, 3, 104, 52, 0, 1643, 1645, 5,
        126, 0, 0, 1644, 1643, 1, 0, 0, 0, 1644, 1645, 1, 0, 0, 0, 1645, 1647, 1, 0, 0, 0, 1646,
        1638, 1, 0, 0, 0, 1647, 1650, 1, 0, 0, 0, 1648, 1646, 1, 0, 0, 0, 1648, 1649, 1, 0, 0, 0,
        1649, 1652, 1, 0, 0, 0, 1650, 1648, 1, 0, 0, 0, 1651, 1634, 1, 0, 0, 0, 1651, 1652, 1, 0,
        0, 0, 1652, 1653, 1, 0, 0, 0, 1653, 1654, 5, 9, 0, 0, 1654, 199, 1, 0, 0, 0, 1655, 1657, 5,
        24, 0, 0, 1656, 1658, 5, 126, 0, 0, 1657, 1656, 1, 0, 0, 0, 1657, 1658, 1, 0, 0, 0, 1658,
        1692, 1, 0, 0, 0, 1659, 1661, 3, 202, 101, 0, 1660, 1662, 5, 126, 0, 0, 1661, 1660, 1, 0,
        0, 0, 1661, 1662, 1, 0, 0, 0, 1662, 1663, 1, 0, 0, 0, 1663, 1665, 5, 10, 0, 0, 1664, 1666,
        5, 126, 0, 0, 1665, 1664, 1, 0, 0, 0, 1665, 1666, 1, 0, 0, 0, 1666, 1667, 1, 0, 0, 0, 1667,
        1669, 3, 104, 52, 0, 1668, 1670, 5, 126, 0, 0, 1669, 1668, 1, 0, 0, 0, 1669, 1670, 1, 0, 0,
        0, 1670, 1689, 1, 0, 0, 0, 1671, 1673, 5, 2, 0, 0, 1672, 1674, 5, 126, 0, 0, 1673, 1672, 1,
        0, 0, 0, 1673, 1674, 1, 0, 0, 0, 1674, 1675, 1, 0, 0, 0, 1675, 1677, 3, 202, 101, 0, 1676,
        1678, 5, 126, 0, 0, 1677, 1676, 1, 0, 0, 0, 1677, 1678, 1, 0, 0, 0, 1678, 1679, 1, 0, 0, 0,
        1679, 1681, 5, 10, 0, 0, 1680, 1682, 5, 126, 0, 0, 1681, 1680, 1, 0, 0, 0, 1681, 1682, 1,
        0, 0, 0, 1682, 1683, 1, 0, 0, 0, 1683, 1685, 3, 104, 52, 0, 1684, 1686, 5, 126, 0, 0, 1685,
        1684, 1, 0, 0, 0, 1685, 1686, 1, 0, 0, 0, 1686, 1688, 1, 0, 0, 0, 1687, 1671, 1, 0, 0, 0,
        1688, 1691, 1, 0, 0, 0, 1689, 1687, 1, 0, 0, 0, 1689, 1690, 1, 0, 0, 0, 1690, 1693, 1, 0,
        0, 0, 1691, 1689, 1, 0, 0, 0, 1692, 1659, 1, 0, 0, 0, 1692, 1693, 1, 0, 0, 0, 1693, 1694,
        1, 0, 0, 0, 1694, 1695, 5, 25, 0, 0, 1695, 201, 1, 0, 0, 0, 1696, 1697, 3, 206, 103, 0,
        1697, 203, 1, 0, 0, 0, 1698, 1701, 5, 26, 0, 0, 1699, 1702, 3, 210, 105, 0, 1700, 1702, 5,
        97, 0, 0, 1701, 1699, 1, 0, 0, 0, 1701, 1700, 1, 0, 0, 0, 1702, 205, 1, 0, 0, 0, 1703,
        1706, 3, 210, 105, 0, 1704, 1706, 3, 208, 104, 0, 1705, 1703, 1, 0, 0, 0, 1705, 1704, 1, 0,
        0, 0, 1706, 207, 1, 0, 0, 0, 1707, 1708, 7, 6, 0, 0, 1708, 209, 1, 0, 0, 0, 1709, 1710, 7,
        7, 0, 0, 1710, 211, 1, 0, 0, 0, 1711, 1712, 7, 8, 0, 0, 1712, 213, 1, 0, 0, 0, 1713, 1714,
        7, 9, 0, 0, 1714, 215, 1, 0, 0, 0, 1715, 1716, 7, 10, 0, 0, 1716, 217, 1, 0, 0, 0, 322,
        219, 223, 226, 229, 237, 241, 246, 253, 258, 261, 265, 269, 273, 279, 283, 288, 293, 297,
        300, 302, 306, 310, 315, 319, 324, 328, 337, 342, 346, 350, 354, 357, 361, 371, 378, 391,
        395, 401, 405, 409, 414, 419, 423, 429, 433, 439, 443, 449, 453, 457, 461, 465, 469, 474,
        481, 485, 490, 497, 503, 508, 514, 517, 523, 525, 529, 533, 538, 542, 545, 552, 559, 562,
        568, 571, 577, 581, 585, 589, 593, 598, 603, 607, 612, 615, 624, 633, 638, 651, 654, 662,
        666, 671, 676, 680, 685, 691, 696, 703, 707, 712, 716, 720, 722, 726, 728, 732, 734, 740,
        746, 750, 753, 756, 760, 766, 770, 773, 776, 782, 785, 788, 792, 798, 801, 804, 808, 812,
        816, 818, 822, 824, 827, 830, 833, 837, 839, 846, 848, 855, 859, 863, 867, 870, 875, 880,
        885, 890, 894, 898, 901, 906, 911, 915, 917, 921, 925, 927, 929, 937, 942, 953, 963, 973,
        978, 982, 989, 994, 999, 1004, 1009, 1014, 1019, 1024, 1027, 1033, 1035, 1048, 1051, 1058,
        1072, 1076, 1080, 1084, 1088, 1091, 1093, 1098, 1102, 1106, 1110, 1114, 1118, 1121, 1123,
        1128, 1132, 1137, 1143, 1146, 1150, 1154, 1157, 1159, 1163, 1166, 1174, 1178, 1181, 1185,
        1189, 1197, 1201, 1205, 1216, 1220, 1225, 1229, 1233, 1238, 1240, 1243, 1247, 1250, 1253,
        1259, 1263, 1267, 1273, 1277, 1281, 1284, 1287, 1293, 1297, 1301, 1303, 1307, 1311, 1313,
        1317, 1321, 1327, 1331, 1335, 1341, 1345, 1349, 1355, 1359, 1363, 1369, 1373, 1377, 1381,
        1385, 1388, 1394, 1398, 1410, 1414, 1418, 1420, 1424, 1428, 1432, 1436, 1439, 1445, 1449,
        1453, 1457, 1463, 1466, 1472, 1476, 1480, 1484, 1489, 1493, 1497, 1500, 1504, 1507, 1510,
        1515, 1521, 1534, 1538, 1542, 1546, 1549, 1554, 1557, 1559, 1562, 1568, 1572, 1576, 1580,
        1584, 1588, 1591, 1607, 1618, 1624, 1632, 1636, 1640, 1644, 1648, 1651, 1657, 1661, 1665,
        1669, 1673, 1677, 1681, 1685, 1689, 1692, 1701, 1705
    ];
}
