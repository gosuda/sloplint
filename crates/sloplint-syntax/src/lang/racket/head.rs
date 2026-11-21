//! Head classification for Racket `list` nodes.

/// What a `list` head names. Only `Define`, `Lambda` and `Let` produce
/// function facts; the rest documents the classification whose role
/// the single-role dispatch cannot realize.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Head {
    Define,
    Lambda,
    Let,
    Branch,
    Match,
    Loop,
    Error,
    Type,
    Import,
    Module,
}

/// Head symbol to concept. Data, not a chain of special cases: the
/// hooks match on the looked-up concept, never on the raw text.
const HEAD_TABLE: &[(&str, Head)] = &[
    ("define", Head::Define),
    ("define/contract", Head::Define),
    ("lambda", Head::Lambda),
    ("λ", Head::Lambda),
    ("let", Head::Let),
    ("let*", Head::Let),
    ("letrec", Head::Let),
    ("let-values", Head::Let),
    ("let*-values", Head::Let),
    ("letrec-values", Head::Let),
    ("if", Head::Branch),
    ("cond", Head::Branch),
    ("when", Head::Branch),
    ("unless", Head::Branch),
    ("case", Head::Match),
    ("match", Head::Match),
    ("for", Head::Loop),
    ("for*", Head::Loop),
    ("for/list", Head::Loop),
    ("for*/list", Head::Loop),
    ("for/vector", Head::Loop),
    ("for*/vector", Head::Loop),
    ("for/hash", Head::Loop),
    ("for*/hash", Head::Loop),
    ("for/and", Head::Loop),
    ("for*/and", Head::Loop),
    ("for/or", Head::Loop),
    ("for*/or", Head::Loop),
    ("for/sum", Head::Loop),
    ("for*/sum", Head::Loop),
    ("for/product", Head::Loop),
    ("for*/product", Head::Loop),
    ("for/lists", Head::Loop),
    ("for/fold", Head::Loop),
    ("for*/fold", Head::Loop),
    ("do", Head::Loop),
    ("with-handlers", Head::Error),
    ("struct", Head::Type),
    ("define-struct", Head::Type),
    ("require", Head::Import),
    ("provide", Head::Import),
    ("module", Head::Module),
    ("module+", Head::Module),
    ("module*", Head::Module),
];

pub(super) fn lookup_head(name: &str) -> Option<Head> {
    HEAD_TABLE
        .iter()
        .find(|(head, _)| *head == name)
        .map(|(_, concept)| *concept)
}
