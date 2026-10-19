//! Entry point for the sloplint binary: parses arguments, runs the pipeline, renders, and maps exit codes.

use std::io::{self, Write};
use std::ops::Range;
use std::path::Path;
use std::process::ExitCode;

use annotate_snippets::renderer::DecorStyle;
use annotate_snippets::{AnnotationKind, Group, Level, Renderer, Snippet};
use clap::Parser;
use sloplint::cli::{BaselineArgs, CheckArgs, Cli, Command, Format, RulesArgs};
use sloplint::rules_cmd::{self, RulesRequest};
use sloplint::{Execution, Report, RunError, RunOptions, Summary, init};
use sloplint_core::{Finding, RuleFilter, Severity};

fn main() -> ExitCode {
    let cli = Cli::parse();
    let cwd = match std::env::current_dir() {
        Ok(cwd) => cwd,
        Err(error) => return fail(&error.to_string(), 3),
    };
    match cli.command {
        None => check(&cwd, &cli.check),
        Some(Command::Check(args)) => check(&cwd, &args),
        Some(Command::Baseline(args)) => baseline(&cwd, &args),
        Some(Command::Rules(args)) => rules(&cwd, &args),
        Some(Command::Init) => match init::write_default(&cwd) {
            Ok(()) => ExitCode::SUCCESS,
            Err(error) => report_error(&error),
        },
    }
}

fn check(cwd: &Path, args: &CheckArgs) -> ExitCode {
    let options = RunOptions {
        cwd: cwd.to_path_buf(),
        root: None,
        config: args.run.select.config.clone(),
        paths: if args.paths.is_empty() {
            vec![cwd.to_path_buf()]
        } else {
            args.paths.clone()
        },
        strictness: args.run.select.strictness.map(Into::into),
        mode: args.mode.map(Into::into),
        fail_on: args.fail_on.map(Into::into),
        rule_filter: RuleFilter(args.rules.clone()),
        use_cache: !args.run.no_cache,
        jobs: args.run.jobs.unwrap_or(0),
        write_baseline: false,
    };
    match sloplint::execute(&options) {
        Ok(execution) => match emit(&execution, args.format) {
            Ok(()) if execution.failed => ExitCode::from(1),
            Ok(()) => ExitCode::SUCCESS,
            Err(error) => fail(&error.to_string(), 3),
        },
        Err(error) => report_error(&error),
    }
}

fn baseline(cwd: &Path, args: &BaselineArgs) -> ExitCode {
    let options = RunOptions {
        cwd: cwd.to_path_buf(),
        root: None,
        config: args.run.select.config.clone(),
        paths: Vec::new(),
        strictness: args.run.select.strictness.map(Into::into),
        mode: None,
        fail_on: None,
        rule_filter: RuleFilter::default(),
        use_cache: !args.run.no_cache,
        jobs: args.run.jobs.unwrap_or(0),
        write_baseline: true,
    };
    match sloplint::execute(&options) {
        Ok(execution) => match execution.baseline_written.as_deref() {
            Some(path) => print_text(&format!(
                "baseline: wrote {} entries to {path}\n",
                execution.report.summary.findings
            )),
            None => ExitCode::SUCCESS,
        },
        Err(error) => report_error(&error),
    }
}

fn rules(cwd: &Path, args: &RulesArgs) -> ExitCode {
    let request = RulesRequest {
        cwd: cwd.to_path_buf(),
        config: args.select.config.clone(),
        strictness: args.select.strictness.map(Into::into),
        id: args.id.clone(),
        format: args.format,
    };
    match rules_cmd::render(&request) {
        Ok(text) => print_text(&text),
        Err(error) => report_error(&error),
    }
}

fn emit(execution: &Execution, format: Format) -> io::Result<()> {
    let mut out = anstream::stdout();
    match format {
        Format::Agent => write_agent(&execution.report, &mut out)?,
        Format::Human => write_human(execution, &mut out)?,
        Format::Json => write_json(&execution.report, &mut out)?,
    }
    out.flush()
}

fn write_agent(report: &Report, out: &mut dyn Write) -> io::Result<()> {
    for finding in &report.findings {
        writeln!(
            out,
            "{}:{}:{}: {}: {} [{}]",
            finding.path,
            finding.line,
            finding.column,
            severity_name(finding.severity),
            finding.message,
            finding.rule
        )?;
        writeln!(out, "  context: {}", finding.context)?;
        writeln!(out, "  fix: {}", finding.fix)?;
    }
    if !report.findings.is_empty() {
        writeln!(out)?;
        writeln!(out, "rules:")?;
        for (rule, why) in &report.rules_fired {
            writeln!(out, "  {rule}: {why}")?;
        }
    }
    writeln!(out, "{}", summary_line(&report.summary))
}

fn write_json(report: &Report, out: &mut dyn Write) -> io::Result<()> {
    let findings = report
        .findings
        .iter()
        .map(sonic_rs::to_string)
        .collect::<Result<Vec<_>, _>>()
        .map_err(io::Error::other)?;
    let rules = sonic_rs::to_string(&report.rules_fired).map_err(io::Error::other)?;
    let summary = sonic_rs::to_string(&report.summary).map_err(io::Error::other)?;
    writeln!(out, "{{\"version\":1,")?;
    writeln!(out, " \"findings\":[{}],", findings.join(","))?;
    writeln!(out, " \"rules\":{rules},")?;
    writeln!(out, " \"summary\":{summary}}}")
}

fn write_human(execution: &Execution, out: &mut dyn Write) -> io::Result<()> {
    let renderer = Renderer::styled().decor_style(DecorStyle::Unicode);
    let mut sources = Sources {
        root: execution.root.as_path(),
        max_bytes: execution.max_file_bytes,
        current: None,
    };
    for finding in &execution.report.findings {
        let rendered = render_finding(&renderer, &mut sources, &execution.report, finding);
        writeln!(out, "{}", rendered.trim_end())?;
        writeln!(out)?;
    }
    writeln!(out, "{}", summary_line(&execution.report.summary))
}

struct Source {
    text: String,
    starts: Vec<usize>,
}

impl Source {
    fn new(text: String) -> Self {
        let mut starts = Vec::new();
        if !text.is_empty() {
            starts.push(0);
            starts.extend(
                text.match_indices('\n')
                    .map(|(at, _)| at + 1)
                    .filter(|&start| start < text.len()),
            );
        }
        Source { text, starts }
    }

    fn line(&self, line: u32) -> Option<&str> {
        let index = usize::try_from(line.saturating_sub(1)).ok()?;
        let rest = self.text.get(*self.starts.get(index)?..)?;
        let row = rest.split('\n').next()?;
        Some(row.strip_suffix('\r').unwrap_or(row))
    }
}

struct Sources<'a> {
    root: &'a Path,
    max_bytes: u64,
    current: Option<(String, Option<Source>)>,
}

impl Sources<'_> {
    fn line(&mut self, path: &str, line: u32) -> Option<&str> {
        let cached = matches!(&self.current, Some((name, _)) if name == path);
        if !cached {
            let source = sloplint::read_source(&self.root.join(path), self.max_bytes)
                .ok()
                .map(Source::new);
            self.current = Some((path.to_string(), source));
        }
        self.current.as_ref()?.1.as_ref()?.line(line)
    }
}

fn render_finding(
    renderer: &Renderer,
    sources: &mut Sources<'_>,
    report: &Report,
    finding: &Finding,
) -> String {
    let text = sources
        .line(&finding.path, finding.line)
        .map_or(&*finding.context, str::trim_end);
    let span = span_for(text, finding.column);
    let level = match finding.severity {
        Severity::Deny => Level::ERROR,
        Severity::Warn => Level::WARNING,
    };
    let why = report
        .rules_fired
        .get(&*finding.rule)
        .map_or("", String::as_str);
    let title = level
        .primary_title(finding.message.clone())
        .id(finding.rule.to_string());
    let snippet = Snippet::source(text)
        .line_start(line_number(finding.line))
        .path(finding.path.to_string())
        .annotation(AnnotationKind::Primary.span(span));
    let groups = [
        Group::with_title(title).element(snippet),
        Group::with_title(Level::HELP.secondary_title(finding.fix.clone())),
        Group::with_title(Level::NOTE.secondary_title(why.to_string())),
    ];
    renderer.render(&groups)
}

fn span_for(text: &str, column: u32) -> Range<usize> {
    let wanted = usize::try_from(column.saturating_sub(1)).unwrap_or(usize::MAX);
    let start = text
        .char_indices()
        .nth(wanted)
        .map_or(text.len(), |(index, _)| index);
    if start < text.len() {
        return start..text.len();
    }
    let last = text.char_indices().last().map_or(0, |(index, _)| index);
    last..text.len()
}

fn line_number(line: u32) -> usize {
    usize::try_from(line).unwrap_or(usize::MAX)
}

fn summary_line(summary: &Summary) -> String {
    format!(
        "summary: strictness={} files={} findings={} deny={} warn={} baselined={} stale={} skipped={}",
        summary.strictness.as_str(),
        summary.files,
        summary.findings,
        summary.deny,
        summary.warn,
        summary.baselined,
        summary.stale,
        summary.skipped
    )
}

fn severity_name(severity: Severity) -> &'static str {
    match severity {
        Severity::Warn => "warn",
        Severity::Deny => "deny",
    }
}

fn print_text(text: &str) -> ExitCode {
    let mut out = anstream::stdout();
    match out.write_all(text.as_bytes()).and_then(|()| out.flush()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => fail(&error.to_string(), 3),
    }
}

fn fail(message: &str, code: u8) -> ExitCode {
    let _ = writeln!(io::stderr(), "sloplint: error: {message}");
    ExitCode::from(code)
}

fn report_error(error: &RunError) -> ExitCode {
    fail(&error.to_string(), error.exit_code())
}
