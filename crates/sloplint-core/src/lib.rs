//! Shared contracts for sloplint: language tables, file facts, findings,
//! configuration, suppressions, baselines, and the rule catalog.
//!
//! This crate has no tree-sitter dependency and performs no I/O; the syntax
//! crate turns parsed trees into these facts, and the CLI crate renders them.

pub mod baseline;
pub mod callee;
pub mod config;
pub mod facts;
pub mod finding;
pub mod lang;
pub mod rules;
pub mod suppress;
pub mod text;

pub use baseline::{Baseline, BaselineEntry};
pub use callee::CalleePattern;
pub use config::{Config, ConfigError, LanguageConfig, Mode, ProjectConfig, Strictness};
pub use facts::{FileFacts, Pos, WorldFacts};
pub use finding::{FailOn, Finding, Level, Severity};
pub use rules::{
    Build, CATALOG, CustomHit, CustomRuleSpec, Family, FileRule, Preset, ResolvedRule, RuleFilter,
    RuleInfo, RuleSet, Settings, Thresholds, WorldFile, WorldRule,
};
pub use suppress::{CommentText, RulePattern, Suppression};
