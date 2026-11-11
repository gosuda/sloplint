//! Contract types only: no tree-sitter dependency and no I/O; `sloplint-syntax` fills the facts from parsed trees, and the CLI renders them.

pub mod baseline;
pub mod callee;
pub mod config;
pub mod facts;
pub mod finding;
pub mod lang;
pub mod module_keys;
pub mod role;
pub mod rules;
pub mod suppress;
pub mod text;

#[cfg(test)]
mod facts_tests;

pub use baseline::{Baseline, BaselineEntry, SkippedEntry};
pub use callee::CalleePattern;
pub use config::{Config, ConfigError, LanguageConfig, Mode, ProjectConfig, Strictness};
pub use facts::{
    AssertShape, FileFacts, ImportScope, Pos, PrimitiveKind, ScrutineeKind, Visibility, WorldFacts,
};
pub use finding::{Confidence, FailOn, Finding, Level, Severity};
pub use role::{CodeRole, RolePolicy, RoleRegion, Shift, role_at};
pub use rules::{
    Build, CATALOG, CustomHit, CustomRuleSpec, Family, FileRule, Preset, ResolvedRule, RuleFilter,
    RuleInfo, RuleSet, Settings, Thresholds, WorldFile, WorldRule,
};
pub use suppress::{CommentText, RulePattern, Suppression};
