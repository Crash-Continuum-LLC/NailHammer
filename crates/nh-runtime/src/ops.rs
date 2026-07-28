//! The expression tree and its builder.
//!
//! DESIGN.md §5.2 explains why this exists at all. Pest ships a `PrattParser`,
//! but its fold calls `map_infix(lhs_result, op, rhs_result)` — both operands
//! are *already evaluated* when the callback runs. That is eager by
//! construction and cannot express `&&`, which must not evaluate its right
//! operand when the left is false.
//!
//! To defer an operand you must know its **extent** before deciding whether to
//! evaluate it. So the driver works in two phases: fold the flat pair stream
//! into an [`OpTree`] of borrowed pairs (no values allocated), then evaluate
//! that tree, where an unevaluated child simply *is* the deferred operand.
//!
//! This is the deliberate, bounded exception to "no AST layer": it exists only
//! for expressions, lives entirely in this crate, and is never visible to
//! handlers.

use pest::iterators::Pair;
use pest::RuleType;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Fixity {
    Infix,
    Prefix,
    Postfix,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Assoc {
    Left,
    Right,
}

/// What the generated table knows about one operator.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct OpInfo {
    /// Higher binds tighter. Derived from the tier's position in the resolved
    /// table, so `nh explain` and this agree by construction.
    pub precedence: u16,
    pub fixity: Fixity,
    pub assoc: Assoc,
}

/// A folded expression.
#[derive(Debug)]
pub enum OpTree<'i, R: RuleType> {
    Atom(Pair<'i, R>),
    Prefix {
        op: Pair<'i, R>,
        operand: Box<OpTree<'i, R>>,
    },
    Postfix {
        op: Pair<'i, R>,
        operand: Box<OpTree<'i, R>>,
    },
    Infix {
        op: Pair<'i, R>,
        lhs: Box<OpTree<'i, R>>,
        rhs: Box<OpTree<'i, R>>,
    },
}

impl<'i, R: RuleType> OpTree<'i, R> {
    /// The pair this subtree spans, for diagnostics.
    pub fn pair(&self) -> &Pair<'i, R> {
        match self {
            OpTree::Atom(p) => p,
            OpTree::Prefix { op, .. } | OpTree::Postfix { op, .. } | OpTree::Infix { op, .. } => op,
        }
    }
}

/// Why a flat operator stream could not be folded.
#[derive(Debug, PartialEq, Eq)]
pub enum BuildError {
    /// The stream ended where an operand was expected.
    MissingOperand,
    /// An operator appeared where an operand was expected, or vice versa.
    UnexpectedOperator(String),
}

impl std::fmt::Display for BuildError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BuildError::MissingOperand => write!(f, "expected an operand"),
            BuildError::UnexpectedOperator(op) => {
                write!(f, "unexpected operator `{op}`")
            }
        }
    }
}

/// Folds the direct children of an `expr` pair into an [`OpTree`].
///
/// Two lookups rather than one, because a spelling can have two readings: `-`
/// is `sub` between operands and `neg` before one, and the lowerer emits a
/// single rule for it. `prefix` is consulted only where an operand is expected.
pub fn build<'i, R: RuleType>(
    parts: Vec<Pair<'i, R>>,
    info: impl Fn(R) -> Option<OpInfo>,
    prefix: impl Fn(R) -> Option<OpInfo>,
) -> Result<OpTree<'i, R>, BuildError> {
    let mut p = Parser {
        parts,
        pos: 0,
        info,
        prefix,
    };
    let tree = p.parse(0)?;
    match p.peek() {
        None => Ok(tree),
        Some(pair) => Err(BuildError::UnexpectedOperator(
            pair.as_str().trim().to_string(),
        )),
    }
}

struct Parser<'i, R: RuleType, F, G> {
    parts: Vec<Pair<'i, R>>,
    pos: usize,
    info: F,
    prefix: G,
}

impl<'i, R: RuleType, F, G> Parser<'i, R, F, G>
where
    F: Fn(R) -> Option<OpInfo>,
    G: Fn(R) -> Option<OpInfo>,
{
    fn peek(&self) -> Option<&Pair<'i, R>> {
        self.parts.get(self.pos)
    }

    fn peek_op(&self, fixity: Fixity) -> Option<OpInfo> {
        let pair = self.peek()?;
        let info = if fixity == Fixity::Prefix {
            (self.prefix)(pair.as_rule())?
        } else {
            (self.info)(pair.as_rule())?
        };
        (info.fixity == fixity).then_some(info)
    }

    fn next(&mut self) -> Option<Pair<'i, R>> {
        let out = self.parts.get(self.pos).cloned();
        if out.is_some() {
            self.pos += 1;
        }
        out
    }

    /// Precedence climbing.
    ///
    /// A left-associative operator recurses at `precedence + 1` so an operator
    /// of equal precedence terminates the inner call and groups leftward; a
    /// right-associative one recurses at its own precedence so equal operators
    /// nest rightward.
    fn parse(&mut self, min_precedence: u16) -> Result<OpTree<'i, R>, BuildError> {
        let mut lhs = self.parse_unary()?;

        while let Some(info) = self.peek_op(Fixity::Infix) {
            if info.precedence < min_precedence {
                break;
            }
            let op = self.next().expect("peeked");
            let next_min = match info.assoc {
                Assoc::Left => info.precedence + 1,
                Assoc::Right => info.precedence,
            };
            let rhs = self.parse(next_min)?;
            lhs = OpTree::Infix {
                op,
                lhs: Box::new(lhs),
                rhs: Box::new(rhs),
            };
        }

        Ok(lhs)
    }

    fn parse_unary(&mut self) -> Result<OpTree<'i, R>, BuildError> {
        // A prefix operator parses its operand at its *own* precedence, which
        // is what makes BASIC's `NOT A = B` mean `NOT (A = B)`: `NOT` sits
        // below comparison, so `=` binds tighter and is absorbed. In C the
        // prefix tier is tightest, so `-a * b` is `(-a) * b`.
        if let Some(info) = self.peek_op(Fixity::Prefix) {
            let op = self.next().expect("peeked");
            let operand = self.parse(info.precedence)?;
            return Ok(OpTree::Prefix {
                op,
                operand: Box::new(operand),
            });
        }

        let atom = self.next().ok_or(BuildError::MissingOperand)?;
        if let Some(info) = (self.info)(atom.as_rule()) {
            if info.fixity != Fixity::Postfix {
                return Err(BuildError::UnexpectedOperator(
                    atom.as_str().trim().to_string(),
                ));
            }
        }

        let mut node = OpTree::Atom(atom);
        while self.peek_op(Fixity::Postfix).is_some() {
            let op = self.next().expect("peeked");
            node = OpTree::Postfix {
                op,
                operand: Box::new(node),
            };
        }
        Ok(node)
    }
}
