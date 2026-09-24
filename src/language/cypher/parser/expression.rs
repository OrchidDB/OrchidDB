//! Legacy expression text parser used by the AST visitor.

use crate::language::cypher::ast::*;

use super::visitor::clean_identifier;
pub(super) fn parse_expr_text(text: &str) -> Expr {
    ExprParser::new(text).parse_expression()
}

#[derive(Debug, Clone, PartialEq)]
pub(super) enum ExprToken {
    Ident(String),
    Int(String),
    Float(f64),
    String(String),
    Param(String),
    Symbol(char),
    Op(&'static str),
    Eof,
}

pub(super) struct ExprParser {
    tokens: Vec<ExprToken>,
    pos: usize,
}

impl ExprParser {
    fn new(input: &str) -> Self {
        Self {
            tokens: lex_expr(input),
            pos: 0,
        }
    }

    fn parse_expression(&mut self) -> Expr {
        self.parse_or()
    }

    fn parse_or(&mut self) -> Expr {
        let mut expr = self.parse_and();
        while self.consume_keyword("OR") {
            let rhs = self.parse_and();
            expr = Expr::Binary {
                op: BinaryOp::Or,
                lhs: Box::new(expr),
                rhs: Box::new(rhs),
            };
        }
        expr
    }

    fn parse_and(&mut self) -> Expr {
        let mut expr = self.parse_not();
        while self.consume_keyword("AND") {
            let rhs = self.parse_not();
            expr = Expr::Binary {
                op: BinaryOp::And,
                lhs: Box::new(expr),
                rhs: Box::new(rhs),
            };
        }
        expr
    }

    fn parse_not(&mut self) -> Expr {
        if self.consume_keyword("NOT") {
            return Expr::Unary {
                op: UnaryOp::Not,
                expr: Box::new(self.parse_not()),
            };
        }
        self.parse_comparison()
    }

    fn parse_comparison(&mut self) -> Expr {
        let mut expr = self.parse_add_sub();
        loop {
            if self.consume_keyword("ISNULL")
                || (self.consume_keyword("IS") && self.consume_keyword("NULL"))
            {
                expr = Expr::IsNull(Box::new(expr));
            } else if self.consume_keyword("ISNOTNULL")
                || (self.consume_keyword("IS")
                    && self.consume_keyword("NOT")
                    && self.consume_keyword("NULL"))
            {
                expr = Expr::IsNotNull(Box::new(expr));
            } else if self.consume_keyword("STARTSWITH") {
                let rhs = self.parse_add_sub();
                expr = Expr::StringPredicate {
                    op: StringPredicateOp::StartsWith,
                    target: Box::new(expr),
                    pattern: Box::new(rhs),
                };
            } else if self.consume_keyword("ENDSWITH") {
                let rhs = self.parse_add_sub();
                expr = Expr::StringPredicate {
                    op: StringPredicateOp::EndsWith,
                    target: Box::new(expr),
                    pattern: Box::new(rhs),
                };
            } else if self.consume_keyword("CONTAINS") {
                let rhs = self.parse_add_sub();
                expr = Expr::StringPredicate {
                    op: StringPredicateOp::Contains,
                    target: Box::new(expr),
                    pattern: Box::new(rhs),
                };
            } else if self.consume_op("=~") {
                let rhs = self.parse_add_sub();
                expr = Expr::StringPredicate {
                    op: StringPredicateOp::Regex,
                    target: Box::new(expr),
                    pattern: Box::new(rhs),
                };
            } else if let Some(op) = self.consume_comparison_op() {
                let rhs = self.parse_add_sub();
                expr = Expr::Binary {
                    op,
                    lhs: Box::new(expr),
                    rhs: Box::new(rhs),
                };
            } else {
                break;
            }
        }
        expr
    }

    fn parse_add_sub(&mut self) -> Expr {
        let mut expr = self.parse_mul_div();
        loop {
            let op = if self.consume_symbol('+') {
                Some(BinaryOp::Add)
            } else if self.consume_symbol('-') {
                Some(BinaryOp::Sub)
            } else {
                None
            };
            let Some(op) = op else { break };
            let rhs = self.parse_mul_div();
            expr = Expr::Binary {
                op,
                lhs: Box::new(expr),
                rhs: Box::new(rhs),
            };
        }
        expr
    }

    fn parse_mul_div(&mut self) -> Expr {
        let mut expr = self.parse_unary();
        loop {
            let op = if self.consume_symbol('*') {
                Some(BinaryOp::Mul)
            } else if self.consume_symbol('/') {
                Some(BinaryOp::Div)
            } else {
                None
            };
            let Some(op) = op else { break };
            let rhs = self.parse_unary();
            expr = Expr::Binary {
                op,
                lhs: Box::new(expr),
                rhs: Box::new(rhs),
            };
        }
        expr
    }

    fn parse_unary(&mut self) -> Expr {
        if self.consume_symbol('-') {
            return Expr::Unary {
                op: UnaryOp::Neg,
                expr: Box::new(self.parse_unary()),
            };
        }
        self.parse_postfix()
    }

    fn parse_postfix(&mut self) -> Expr {
        let mut expr = self.parse_primary();
        while self.consume_symbol('.') {
            let key = match self.next() {
                ExprToken::Ident(name) => name,
                _ => "_".to_string(),
            };
            expr = Expr::Property {
                target: Box::new(expr),
                key,
            };
        }
        expr
    }

    fn parse_primary(&mut self) -> Expr {
        match self.next() {
            ExprToken::Ident(name) => {
                let upper = name.to_ascii_uppercase();
                match upper.as_str() {
                    "NULL" => Expr::Literal(Literal::Null),
                    "TRUE" => Expr::Literal(Literal::Bool(true)),
                    "FALSE" => Expr::Literal(Literal::Bool(false)),
                    "COUNT" if self.consume_symbol('(') && self.consume_symbol('*') => {
                        self.consume_symbol(')');
                        Expr::CountStar
                    }
                    _ if self.consume_symbol('(') => {
                        let distinct = self.consume_keyword("DISTINCT");
                        let mut args = Vec::new();
                        if !self.check_symbol(')') {
                            loop {
                                args.push(self.parse_expression());
                                if !self.consume_symbol(',') {
                                    break;
                                }
                            }
                        }
                        self.consume_symbol(')');
                        Expr::Function {
                            name,
                            distinct,
                            args,
                        }
                    }
                    _ => Expr::Variable(clean_identifier(&name)),
                }
            }
            ExprToken::Int(value) => Expr::Literal(Literal::Integer(value)),
            ExprToken::Float(value) => Expr::Literal(Literal::Float(value)),
            ExprToken::String(value) => Expr::Literal(Literal::String(value)),
            ExprToken::Param(name) => Expr::Parameter(clean_identifier(&name)),
            ExprToken::Symbol('*') => Expr::Star,
            ExprToken::Symbol('(') => {
                let expr = self.parse_expression();
                self.consume_symbol(')');
                expr
            }
            ExprToken::Symbol('[') => {
                let mut items = Vec::new();
                if !self.check_symbol(']') {
                    loop {
                        items.push(self.parse_expression());
                        if !self.consume_symbol(',') {
                            break;
                        }
                    }
                }
                self.consume_symbol(']');
                Expr::List(items)
            }
            ExprToken::Symbol('{') => {
                let mut items = Vec::new();
                if !self.check_symbol('}') {
                    loop {
                        let key = match self.next() {
                            ExprToken::Ident(name) | ExprToken::String(name) => {
                                clean_identifier(&name)
                            }
                            _ => "_".to_string(),
                        };
                        self.consume_symbol(':');
                        let value = self.parse_expression();
                        items.push((key, value));
                        if !self.consume_symbol(',') {
                            break;
                        }
                    }
                }
                self.consume_symbol('}');
                Expr::Map(items)
            }
            _ => Expr::Literal(Literal::Null),
        }
    }

    fn consume_comparison_op(&mut self) -> Option<BinaryOp> {
        let op = match self.peek() {
            ExprToken::Op("=") => BinaryOp::Eq,
            ExprToken::Op("<>") => BinaryOp::Neq,
            ExprToken::Op("<") => BinaryOp::Lt,
            ExprToken::Op("<=") => BinaryOp::Lte,
            ExprToken::Op(">") => BinaryOp::Gt,
            ExprToken::Op(">=") => BinaryOp::Gte,
            _ => return None,
        };
        self.pos += 1;
        Some(op)
    }

    fn consume_op(&mut self, expected: &str) -> bool {
        if matches!(self.peek(), ExprToken::Op(op) if *op == expected) {
            self.pos += 1;
            true
        } else {
            false
        }
    }

    fn consume_keyword(&mut self, expected: &str) -> bool {
        match self.peek() {
            ExprToken::Ident(name) if name.eq_ignore_ascii_case(expected) => {
                self.pos += 1;
                true
            }
            _ => false,
        }
    }

    fn consume_symbol(&mut self, expected: char) -> bool {
        if self.check_symbol(expected) {
            self.pos += 1;
            true
        } else {
            false
        }
    }

    fn check_symbol(&self, expected: char) -> bool {
        matches!(self.peek(), ExprToken::Symbol(actual) if *actual == expected)
    }

    fn next(&mut self) -> ExprToken {
        let token = self.peek().clone();
        if !matches!(token, ExprToken::Eof) {
            self.pos += 1;
        }
        token
    }

    fn peek(&self) -> &ExprToken {
        self.tokens.get(self.pos).unwrap_or(&ExprToken::Eof)
    }
}

pub(super) fn lex_expr(input: &str) -> Vec<ExprToken> {
    let mut chars = input.chars().peekable();
    let mut tokens = Vec::new();
    while let Some(ch) = chars.peek().copied() {
        match ch {
            c if c.is_whitespace() => {
                chars.next();
            }
            '\'' | '"' => {
                let quote = chars.next().unwrap();
                let mut value = String::new();
                while let Some(c) = chars.next() {
                    if c == quote {
                        break;
                    }
                    if c == '\\' {
                        if let Some(next) = chars.next() {
                            value.push(next);
                        }
                    } else {
                        value.push(c);
                    }
                }
                tokens.push(ExprToken::String(value));
            }
            '`' => {
                chars.next();
                let mut value = String::new();
                while let Some(c) = chars.next() {
                    if c == '`' {
                        break;
                    }
                    value.push(c);
                }
                tokens.push(ExprToken::Ident(value));
            }
            '$' => {
                chars.next();
                tokens.push(ExprToken::Param(read_identifier(&mut chars)));
            }
            '0'..='9' => {
                let mut value = String::new();
                let mut is_float = false;
                while let Some(c) = chars.peek().copied() {
                    if c.is_ascii_digit() {
                        value.push(c);
                        chars.next();
                    } else if c == '.' && !is_float {
                        is_float = true;
                        value.push(c);
                        chars.next();
                    } else {
                        break;
                    }
                }
                if is_float {
                    tokens.push(ExprToken::Float(value.parse().unwrap_or(0.0)));
                } else {
                    tokens.push(ExprToken::Int(value));
                }
            }
            '=' => {
                chars.next();
                if chars.peek() == Some(&'~') {
                    chars.next();
                    tokens.push(ExprToken::Op("=~"));
                } else {
                    tokens.push(ExprToken::Op("="));
                }
            }
            '<' => {
                chars.next();
                if chars.peek() == Some(&'=') {
                    chars.next();
                    tokens.push(ExprToken::Op("<="));
                } else if chars.peek() == Some(&'>') {
                    chars.next();
                    tokens.push(ExprToken::Op("<>"));
                } else {
                    tokens.push(ExprToken::Op("<"));
                }
            }
            '>' => {
                chars.next();
                if chars.peek() == Some(&'=') {
                    chars.next();
                    tokens.push(ExprToken::Op(">="));
                } else {
                    tokens.push(ExprToken::Op(">"));
                }
            }
            '.' => {
                chars.next();
                tokens.push(ExprToken::Symbol('.'));
            }
            ',' | ':' | '(' | ')' | '[' | ']' | '{' | '}' | '+' | '-' | '*' | '/' => {
                tokens.push(ExprToken::Symbol(ch));
                chars.next();
            }
            c if is_ident_start(c) => {
                let ident = read_identifier(&mut chars);
                tokens.push(ExprToken::Ident(ident));
            }
            _ => {
                chars.next();
            }
        }
    }
    tokens.push(ExprToken::Eof);
    tokens
}

pub(super) fn read_identifier<I>(chars: &mut std::iter::Peekable<I>) -> String
where
    I: Iterator<Item = char>,
{
    let mut value = String::new();
    while let Some(c) = chars.peek().copied() {
        if is_ident_continue(c) {
            value.push(c);
            chars.next();
        } else {
            break;
        }
    }
    value
}

pub(super) fn is_ident_start(ch: char) -> bool {
    ch == '_' || ch.is_ascii_alphabetic()
}

pub(super) fn is_ident_continue(ch: char) -> bool {
    ch == '_' || ch.is_ascii_alphanumeric()
}
