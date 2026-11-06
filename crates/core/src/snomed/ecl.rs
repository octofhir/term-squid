//! Bounded brief ECL over active scalar concepts, not post-coordinated expressions.
use crate::{Error, Result};
use std::collections::BTreeSet;

pub const MAX_BYTES: usize = 4096;
pub const MAX_DEPTH: usize = 32;
pub const MAX_NODES: usize = 128;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HierarchyOperator {
    Descendant,
    DescendantOrSelf,
    Ancestor,
    AncestorOrSelf,
    Child,
    ChildOrSelf,
    Parent,
    ParentOrSelf,
}
impl HierarchyOperator {
    pub fn ancestors(self) -> bool {
        matches!(
            self,
            Self::Ancestor | Self::AncestorOrSelf | Self::Parent | Self::ParentOrSelf
        )
    }
    pub fn include_self(self) -> bool {
        matches!(
            self,
            Self::DescendantOrSelf | Self::AncestorOrSelf | Self::ChildOrSelf | Self::ParentOrSelf
        )
    }
    pub fn direct(self) -> bool {
        matches!(
            self,
            Self::Child | Self::ChildOrSelf | Self::Parent | Self::ParentOrSelf
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Expression {
    All,
    Concept(String),
    MemberOf(String),
    Hierarchy {
        operator: HierarchyOperator,
        code: String,
    },
    And(Vec<Expression>),
    Or(Vec<Expression>),
    Minus(Box<Expression>, Box<Expression>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ecl {
    source: String,
    root: Expression,
    nodes: usize,
}
impl Ecl {
    pub fn parse(source: &str) -> Result<Self> {
        if source.is_empty() || source.len() > MAX_BYTES {
            return Err(error("input must contain 1..4096 bytes"));
        }
        let mut parser = Parser {
            source,
            position: 0,
            nodes: 0,
        };
        let root = parser.expression(0)?;
        parser.whitespace();
        if parser.position != source.len() {
            return Err(error("unsupported or trailing syntax"));
        }
        Ok(Self {
            source: source.to_owned(),
            root,
            nodes: parser.nodes,
        })
    }
    pub fn root(&self) -> &Expression {
        &self.root
    }
    pub fn source(&self) -> &str {
        &self.source
    }
    pub fn nodes(&self) -> usize {
        self.nodes
    }
    pub fn anchors(&self) -> BTreeSet<&str> {
        fn collect<'a>(expression: &'a Expression, result: &mut BTreeSet<&'a str>) {
            match expression {
                Expression::All => {}
                Expression::Concept(code)
                | Expression::MemberOf(code)
                | Expression::Hierarchy { code, .. } => {
                    result.insert(code);
                }
                Expression::And(children) | Expression::Or(children) => {
                    for child in children {
                        collect(child, result);
                    }
                }
                Expression::Minus(left, right) => {
                    collect(left, result);
                    collect(right, result);
                }
            }
        }
        let mut result = BTreeSet::new();
        collect(&self.root, &mut result);
        result
    }
    /// Every explicitly named refset requires a declared complete concept source.
    pub fn refsets(&self) -> BTreeSet<&str> {
        fn collect<'a>(expression: &'a Expression, result: &mut BTreeSet<&'a str>) {
            match expression {
                Expression::MemberOf(code) => {
                    result.insert(code);
                }
                Expression::And(children) | Expression::Or(children) => {
                    for child in children {
                        collect(child, result);
                    }
                }
                Expression::Minus(left, right) => {
                    collect(left, result);
                    collect(right, result);
                }
                _ => {}
            }
        }
        let mut result = BTreeSet::new();
        collect(&self.root, &mut result);
        result
    }
    pub fn hierarchies(&self) -> Vec<(HierarchyOperator, &str)> {
        fn collect<'a>(expression: &'a Expression, result: &mut Vec<(HierarchyOperator, &'a str)>) {
            match expression {
                Expression::Hierarchy { operator, code } => result.push((*operator, code)),
                Expression::And(children) | Expression::Or(children) => {
                    for child in children {
                        collect(child, result);
                    }
                }
                Expression::Minus(left, right) => {
                    collect(left, result);
                    collect(right, result);
                }
                _ => {}
            }
        }
        let mut result = Vec::new();
        collect(&self.root, &mut result);
        result
    }
}

fn error(reason: &str) -> Error {
    Error(format!(
        "Unsupported or invalid bounded SNOMED ECL: {reason}"
    ))
}
fn whitespace(byte: u8) -> bool {
    matches!(byte, b' ' | b'\t' | b'\r' | b'\n')
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Binary {
    And,
    Or,
    Minus,
}
struct Parser<'a> {
    source: &'a str,
    position: usize,
    nodes: usize,
}
impl Parser<'_> {
    fn peek(&self) -> Option<u8> {
        self.source.as_bytes().get(self.position).copied()
    }
    fn whitespace(&mut self) {
        while self.peek().is_some_and(whitespace) {
            self.position += 1;
        }
    }
    fn node(&mut self, expression: Expression) -> Result<Expression> {
        self.nodes += 1;
        if self.nodes > MAX_NODES {
            return Err(error("AST node limit exceeded"));
        }
        Ok(expression)
    }
    fn expression(&mut self, depth: usize) -> Result<Expression> {
        if depth > MAX_DEPTH {
            return Err(error("parenthesis depth limit exceeded"));
        }
        let first = self.subexpression(depth)?;
        let mut children = vec![first];
        let mut operator = None;
        loop {
            self.whitespace();
            if self.peek().is_none_or(|byte| byte == b')') {
                break;
            }
            let next = if self.peek() == Some(b',') {
                self.position += 1;
                Binary::And
            } else {
                let start = self.position;
                while self.peek().is_some_and(|byte| byte.is_ascii_alphabetic()) {
                    self.position += 1;
                }
                let keyword = &self.source[start..self.position];
                let binary = if keyword.eq_ignore_ascii_case("AND") {
                    Binary::And
                } else if keyword.eq_ignore_ascii_case("OR") {
                    Binary::Or
                } else if keyword.eq_ignore_ascii_case("MINUS") {
                    Binary::Minus
                } else {
                    return Err(error("refset selectors, refinements, nested hierarchy operands and other syntax are unavailable"));
                };
                if !self.peek().is_some_and(whitespace) {
                    return Err(error("binary keywords require following whitespace"));
                }
                binary
            };
            if operator.is_some_and(|previous| previous != next || previous == Binary::Minus) {
                return Err(error(
                    "mixed binary operators and repeated MINUS require parentheses",
                ));
            }
            operator = Some(next);
            children.push(self.subexpression(depth)?);
        }
        match operator {
            None => Ok(children.pop().expect("First operand exists")),
            Some(Binary::And) => self.node(Expression::And(children)),
            Some(Binary::Or) => self.node(Expression::Or(children)),
            Some(Binary::Minus) => {
                let right = children.pop().expect("Right operand exists");
                self.node(Expression::Minus(
                    Box::new(children.pop().expect("Left operand exists")),
                    Box::new(right),
                ))
            }
        }
    }
    fn subexpression(&mut self, depth: usize) -> Result<Expression> {
        self.whitespace();
        if self.peek() == Some(b'(') {
            self.position += 1;
            let expression = self.expression(depth + 1)?;
            self.whitespace();
            if self.peek() != Some(b')') {
                return Err(error("unclosed parenthesis"));
            }
            self.position += 1;
            return Ok(expression);
        }
        if self.peek() == Some(b'*') {
            self.position += 1;
            return self.node(Expression::All);
        }
        let member_of = self.peek() == Some(b'^');
        if member_of {
            self.position += 1;
            self.whitespace();
        }
        let operator = if let Some(direction @ (b'<' | b'>')) = self.peek() {
            self.position += 1;
            let include_self = self.peek() == Some(direction);
            if include_self {
                self.position += 1;
            }
            let direct = self.peek() == Some(b'!');
            if direct {
                self.position += 1;
            }
            Some(match (direction, include_self, direct) {
                (b'<', false, false) => HierarchyOperator::Descendant,
                (b'<', true, false) => HierarchyOperator::DescendantOrSelf,
                (b'<', false, true) => HierarchyOperator::Child,
                (b'<', true, true) => HierarchyOperator::ChildOrSelf,
                (b'>', false, false) => HierarchyOperator::Ancestor,
                (b'>', true, false) => HierarchyOperator::AncestorOrSelf,
                (b'>', false, true) => HierarchyOperator::Parent,
                _ => HierarchyOperator::ParentOrSelf,
            })
        } else {
            None
        };
        self.whitespace();
        let start = self.position;
        while self.peek().is_some_and(|byte| byte.is_ascii_digit()) {
            self.position += 1;
        }
        let code = &self.source[start..self.position];
        if !(6..=18).contains(&code.len()) || code.starts_with('0') {
            return Err(error("focus requires a nonzero 6..18 digit scalar SCTID"));
        }
        self.whitespace();
        if self.peek() == Some(b'|') {
            self.position += 1;
            let label_start = self.position;
            let Some(end) = self.source[self.position..].find('|') else {
                return Err(error("unclosed concept label"));
            };
            self.position += end;
            let label = &self.source[label_start..self.position];
            if label.trim().is_empty() || label.chars().any(char::is_control) {
                return Err(error("invalid concept label"));
            }
            self.position += 1;
        }
        if member_of {
            if operator.is_some() {
                return Err(error(
                    "member-of accepts a scalar refset, not a hierarchy operand",
                ));
            }
            return self.node(Expression::MemberOf(code.to_owned()));
        }
        self.node(match operator {
            Some(operator) => Expression::Hierarchy {
                operator,
                code: code.to_owned(),
            },
            None => Expression::Concept(code.to_owned()),
        })
    }
}

/// Decode only the ECL URI path segment, once; '+' is never form-decoded to space.
pub fn decode_implicit(source: &str) -> Result<Ecl> {
    if source.len() > MAX_BYTES * 3
        || source
            .bytes()
            .any(|byte| matches!(byte, b'&' | b'#' | b'?'))
    {
        return Err(error("invalid ECL URI segment or trailing query/fragment"));
    }
    let mut decoded = Vec::with_capacity(source.len().min(MAX_BYTES));
    let bytes = source.as_bytes();
    let mut position = 0;
    while position < bytes.len() {
        let byte = if bytes[position] == b'%' {
            let hex = bytes
                .get(position + 1..position + 3)
                .ok_or_else(|| error("truncated percent escape"))?;
            let digit = |byte: u8| match byte {
                b'0'..=b'9' => Some(byte - b'0'),
                b'a'..=b'f' => Some(byte - b'a' + 10),
                b'A'..=b'F' => Some(byte - b'A' + 10),
                _ => None,
            };
            let high = digit(hex[0]).ok_or_else(|| error("invalid percent escape"))?;
            let low = digit(hex[1]).ok_or_else(|| error("invalid percent escape"))?;
            position += 3;
            high * 16 + low
        } else {
            let byte = bytes[position];
            position += 1;
            byte
        };
        decoded.push(byte);
        if decoded.len() > MAX_BYTES {
            return Err(error("decoded ECL input limit exceeded"));
        }
    }
    let decoded =
        std::str::from_utf8(&decoded).map_err(|_| error("ECL URI must decode to UTF-8"))?;
    Ecl::parse(decoded)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn scalar_member_of_leaves_preserve_declared_source_dependencies() {
        let parsed = Ecl::parse("(^ 100001 |annotation μ| OR ^100002) MINUS <<100003").unwrap();
        assert_eq!(parsed.refsets(), BTreeSet::from(["100001", "100002"]));
        assert_eq!(
            parsed.anchors(),
            BTreeSet::from(["100001", "100002", "100003"])
        );
        assert_eq!(parsed.hierarchies().len(), 1);
        assert_eq!(parsed.nodes(), 5);
        assert_eq!(
            Ecl::parse("^100001").unwrap().root(),
            &Expression::MemberOf("100001".into())
        );
        assert_eq!(
            decode_implicit("%5E%20100001").unwrap().refsets(),
            BTreeSet::from(["100001"])
        );
    }
    #[test]
    fn member_of_does_not_silently_accept_unloaded_selectors_or_field_projections() {
        for source in [
            "^",
            "^ *",
            "^ (100001 OR 100002)",
            "^ [referencedComponentId] 100001",
            "^ <<100001",
            "<< ^100001",
            "^^100001",
            "^100001 {{ M active=true }}",
            "^100001 +HISTORY",
            "^100001:100002=100003",
            "^0100001",
            "^100001 OR ^",
        ] {
            assert!(Ecl::parse(source).is_err(), "{source}");
        }
        assert!(Ecl::parse(&vec!["^100001"; 128].join(" OR ")).is_err());
    }
    #[test]
    fn scalar_wildcard_labels_all_hierarchy_directions_and_boolean_groups() {
        for source in [
            "100000",
            "100000 |Labels AND OR MINUS μ|",
            "*",
            "<100000",
            "<<100000",
            ">100000",
            ">>100000",
            "<!100000",
            "<<!100000",
            ">!100000",
            ">>!100000",
            "100000 AND 110000,120000",
            "100000 or (110000 MINUS 120000)",
            "(100000 OR 110000) MINUS (120000 AND 130000)",
        ] {
            assert!(Ecl::parse(source).is_ok(), "{source}");
        }
        let parsed = Ecl::parse("<<100000 AND (>!110000 OR 120000)").unwrap();
        assert_eq!(
            parsed.anchors(),
            BTreeSet::from(["100000", "110000", "120000"])
        );
        assert_eq!(parsed.hierarchies().len(), 2);
        assert_eq!(parsed.nodes(), 5);
        assert!(HierarchyOperator::Parent.direct());
        assert!(HierarchyOperator::Parent.ancestors());
        assert!(!HierarchyOperator::Parent.include_self());
    }
    #[test]
    fn invalid_ambiguous_unsupported_and_bounded_input_is_not_a_partial_ast() {
        for source in [
            "",
            " ",
            "1",
            "0100000",
            "1234567890123456789",
            "100000 OR 110000 AND 120000",
            "100000 MINUS 110000 MINUS 120000",
            "100000 AND(110000)",
            "100000 110000",
            "100000 OR ",
            "100000 |unclosed",
            "100000 ||",
            "(100000",
            "100000)",
            "^ *",
            "<<100000:110000=120000",
            "100000 {{+HISTORY}}",
            "<*",
            "<<(100000 OR 110000)",
            "100000 /* comment */",
            "100000;DROP TABLE concepts",
        ] {
            assert!(Ecl::parse(source).is_err(), "{source}");
        }
        assert!(Ecl::parse(&format!("{}100000{}", "(".repeat(32), ")".repeat(32))).is_ok());
        assert!(Ecl::parse(&format!("{}100000{}", "(".repeat(33), ")".repeat(33))).is_err());
        assert!(Ecl::parse(&vec!["100000"; 127].join(" OR ")).is_ok());
        assert!(Ecl::parse(&vec!["100000"; 128].join(" OR ")).is_err());
        assert!(Ecl::parse(&" ".repeat(MAX_BYTES + 1)).is_err());
    }
    #[test]
    fn implicit_percent_decode_is_strict_utf8_once_and_not_form_encoding() {
        let parsed = decode_implicit("%3C%3C%20100000%20%7CA%2BB%26C%23D%7C").unwrap();
        assert_eq!(parsed.source(), "<< 100000 |A+B&C#D|");
        for source in [
            "%253C%253C100000",
            "%FF",
            "%",
            "%0",
            "%GG",
            "100000+OR+110000",
            "100000&other=1",
            "100000#fragment",
            "100000?other=1",
        ] {
            assert!(decode_implicit(source).is_err(), "{source}");
        }
    }
}
