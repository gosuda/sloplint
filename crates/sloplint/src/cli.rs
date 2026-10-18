//! Command-line grammar: `check` (the default), `baseline`, `rules`, and `init`.

use std::path::PathBuf;

use clap::{Args, Parser, Subcommand, ValueEnum};
use sloplint_core::{FailOn, Mode, RulePattern, Strictness};

/// Agent-first code quality linter for twenty-three languages.
#[derive(Debug, Parser)]
#[command(name = "sloplint", version, args_conflicts_with_subcommands = true)]
pub struct Cli {
    /// Subcommand to run; without one, `check` runs with these arguments.
    #[command(subcommand)]
    pub command: Option<Command>,
    /// Arguments for the default `check` command.
    #[command(flatten)]
    pub check: CheckArgs,
}

/// Subcommands of the `sloplint` binary.
#[derive(Debug, Subcommand)]
pub enum Command {
    /// Lint the project and report findings (the default).
    Check(CheckArgs),
    /// Record the current findings as the ratchet baseline.
    Baseline(BaselineArgs),
    /// Show the effective rule settings.
    Rules(RulesArgs),
    /// Write a default sloplint.toml in the current directory.
    Init,
}

/// Arguments of `check`.
#[derive(Debug, Args)]
pub struct CheckArgs {
    /// Paths whose findings are reported; every run analyzes the whole project root.
    #[arg(value_name = "PATHS")]
    pub paths: Vec<PathBuf>,
    /// Output format.
    #[arg(short = 'f', long, value_enum, default_value = "agent")]
    pub format: Format,
    /// Gate mode, overriding `project.mode`.
    #[arg(long, value_enum)]
    pub mode: Option<ModeArg>,
    /// Lowest severity that fails the run, overriding `project.fail_on`.
    #[arg(long = "fail-on", value_enum)]
    pub fail_on: Option<FailOnArg>,
    /// Report only findings of this rule id or family (`family/*`); repeatable.
    #[arg(long = "rule", value_name = "ID", value_parser = parse_rule)]
    pub rules: Vec<RulePattern>,
    /// Shared selection and cache options.
    #[command(flatten)]
    pub run: RunArgs,
}

/// Arguments of `baseline`.
#[derive(Debug, Args)]
pub struct BaselineArgs {
    /// Shared selection and cache options.
    #[command(flatten)]
    pub run: RunArgs,
}

/// Arguments of `rules`.
#[derive(Debug, Args)]
pub struct RulesArgs {
    /// Show one rule with its rationale and every strictness preset.
    #[arg(value_name = "ID")]
    pub id: Option<String>,
    /// Output format; `human` prints the agent text.
    #[arg(short = 'f', long, value_enum, default_value = "agent")]
    pub format: Format,
    /// Configuration selection.
    #[command(flatten)]
    pub select: SelectArgs,
}

/// Configuration selection shared by every command.
#[derive(Debug, Args)]
pub struct SelectArgs {
    /// Configuration file; its directory becomes the project root.
    #[arg(short = 'c', long = "config", value_name = "FILE")]
    pub config: Option<PathBuf>,
    /// Strictness profile, overriding `project.strictness`.
    #[arg(long, value_enum)]
    pub strictness: Option<StrictnessArg>,
}

/// Configuration selection plus cache and parallelism controls.
#[derive(Debug, Args)]
pub struct RunArgs {
    /// Configuration selection.
    #[command(flatten)]
    pub select: SelectArgs,
    /// Analyze every file without reading or writing the cache.
    #[arg(long = "no-cache")]
    pub no_cache: bool,
    /// Worker threads, from 1 to 512; defaults to the available parallelism.
    #[arg(short = 'j', long, value_name = "N", value_parser = parse_jobs)]
    pub jobs: Option<usize>,
}

/// Output rendering selected by `--format`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, ValueEnum)]
pub enum Format {
    /// Compiler-style lines for agents and editors.
    #[default]
    Agent,
    /// Annotated snippets for people.
    Human,
    /// One JSON document.
    Json,
}

/// Strictness profile names accepted on the command line.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum StrictnessArg {
    /// Gates only egregious complexity, giant files, and swallowed errors.
    Minimal,
    /// Adds precise greppability and agent rules at warn.
    Basic,
    /// Enables every heuristic rule with moderate thresholds.
    Standard,
    /// The full posture.
    Strict,
}

/// Gate mode names accepted on the command line.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum ModeArg {
    /// Fail on findings at or above `fail_on`.
    Threshold,
    /// Fail only on findings absent from the baseline.
    Ratchet,
}

/// Fail-on severities accepted on the command line.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum FailOnArg {
    /// Any finding fails the run.
    Warn,
    /// Only deny findings fail the run.
    Deny,
    /// Findings never fail the run.
    Never,
}

impl From<StrictnessArg> for Strictness {
    fn from(arg: StrictnessArg) -> Self {
        match arg {
            StrictnessArg::Minimal => Strictness::Minimal,
            StrictnessArg::Basic => Strictness::Basic,
            StrictnessArg::Standard => Strictness::Standard,
            StrictnessArg::Strict => Strictness::Strict,
        }
    }
}

impl From<ModeArg> for Mode {
    fn from(arg: ModeArg) -> Self {
        match arg {
            ModeArg::Threshold => Mode::Threshold,
            ModeArg::Ratchet => Mode::Ratchet,
        }
    }
}

impl From<FailOnArg> for FailOn {
    fn from(arg: FailOnArg) -> Self {
        match arg {
            FailOnArg::Warn => FailOn::Warn,
            FailOnArg::Deny => FailOn::Deny,
            FailOnArg::Never => FailOn::Never,
        }
    }
}

fn parse_jobs(text: &str) -> Result<usize, String> {
    match text.parse::<usize>() {
        Ok(count) if (1..=512).contains(&count) => Ok(count),
        _ => Err(format!("`{text}` is not a job count from 1 to 512")),
    }
}

fn parse_rule(text: &str) -> Result<RulePattern, String> {
    RulePattern::parse(text)
}
