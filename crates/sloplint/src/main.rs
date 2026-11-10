use std::io::{self, BufWriter, Write};
use std::ops::Range;
use std::path::Path;
use std::process::ExitCode;

use annotate_snippets::renderer::DecorStyle;
use annotate_snippets::{AnnotationKind, Group, Level, Renderer, Snippet};
use clap::Parser;
use rayon::prelude::*;
use sloplint::cli::{BaselineArgs, CheckArgs, Cli, Command, Format, RulesArgs};
use sloplint::rules_cmd::{self, RulesRequest};
use sloplint::{
    Execution, Report, RunError, RunOptions, Summary, TerminalText, init, next_control,
};
use sloplint_core::finding::line_starts;
use sloplint_core::{Finding, RuleFilter};

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

/// Exits in-scope so the run's heap stays reachable for LeakSanitizer; output is flushed first, so bytes match a return.
fn check(cwd: &Path, args: &CheckArgs) -> ExitCode {
    let root = args.run.select.root.as_deref();
    let config = args.run.select.config.as_deref();
    let paths = if args.paths.is_empty() {
        match sloplint::implicit_scope(cwd, root, config) {
            Ok(scope) => scope,
            Err(error) => return report_error(&error),
        }
    } else {
        args.paths.clone()
    };
    let options = RunOptions {
        cwd: cwd.to_path_buf(),
        root: args.run.select.root.clone(),
        config: args.run.select.config.clone(),
        paths,
        strictness: args.run.select.strictness.map(Into::into),
        mode: args.mode.map(Into::into),
        fail_on: args.fail_on.map(Into::into),
        min_confidence: args.min_confidence.map(Into::into),
        fail_on_skipped: args.fail_on_skipped,
        rule_filter: RuleFilter(args.rules.clone()),
        cache_root: sloplint::settings::cache_base(),
        use_cache: !args.run.no_cache,
        jobs: args.run.jobs.unwrap_or(0),
        write_baseline: false,
    };
    match sloplint::execute(&options) {
        Ok(execution) => {
            let success = if execution.failed { 1 } else { 0 };
            let status = sink_status(emit(&execution, args.format), success);
            std::process::exit(i32::from(status));
        }
        Err(error) => report_error(&error),
    }
}
fn baseline(cwd: &Path, args: &BaselineArgs) -> ExitCode {
    let options = RunOptions {
        cwd: cwd.to_path_buf(),
        root: args.run.select.root.clone(),
        config: args.run.select.config.clone(),
        paths: Vec::new(),
        strictness: args.run.select.strictness.map(Into::into),
        mode: None,
        fail_on: None,
        min_confidence: None,
        fail_on_skipped: false,
        rule_filter: RuleFilter::default(),
        cache_root: sloplint::settings::cache_base(),
        use_cache: !args.run.no_cache,
        jobs: args.run.jobs.unwrap_or(0),
        write_baseline: true,
    };
    match sloplint::execute(&options) {
        Ok(execution) => {
            let status: u8 = match execution.baseline_written.as_deref() {
                Some(path) => {
                    let text = format!(
                        "baseline: wrote {} entries to {}\n",
                        execution.report.summary.findings,
                        TerminalText::line(path)
                    );
                    print_text_status(&text, TextKind::Plain)
                }
                None => 0,
            };
            std::process::exit(i32::from(status));
        }
        Err(error) => report_error(&error),
    }
}

#[derive(Clone, Copy)]
enum TextKind {
    Plain,
    Json,
}
fn rules(cwd: &Path, args: &RulesArgs) -> ExitCode {
    let request = RulesRequest {
        cwd: cwd.to_path_buf(),
        root: args.select.root.clone(),
        config: args.select.config.clone(),
        strictness: args.select.strictness.map(Into::into),
        id: args.id.clone(),
        format: args.format,
    };
    let kind = if matches!(args.format, Format::Json) {
        TextKind::Json
    } else {
        TextKind::Plain
    };
    match rules_cmd::render(&request) {
        Ok(text) => ExitCode::from(print_text_status(&text, kind)),
        Err(error) => report_error(&error),
    }
}

const OUTPUT_BUFFER_BYTES: usize = 64 * 1024;
const RECORD_BUFFER_BYTES: usize = 1024;

fn with_buffered_stdout(write: impl FnOnce(&mut dyn Write) -> io::Result<()>) -> io::Result<()> {
    let raw = io::stdout();
    match anstream::AutoStream::choice(&raw) {
        anstream::ColorChoice::Never => {
            let mut buffer = BufWriter::with_capacity(OUTPUT_BUFFER_BYTES, raw.lock());
            let mut stripped = anstream::StripStream::new(&mut buffer as &mut dyn Write);
            write(&mut stripped)?;
            stripped.flush()
        }
        choice => {
            let mut out = BufWriter::with_capacity(
                OUTPUT_BUFFER_BYTES,
                anstream::AutoStream::new(raw, choice).lock(),
            );
            write(&mut out)?;
            out.flush()
        }
    }
}

fn emit(execution: &Execution, format: Format) -> io::Result<()> {
    match format {
        Format::Agent => with_plain_stdout(|out| write_agent(execution, out)),
        Format::Human => with_buffered_stdout(|out| write_human(execution, out)),
        Format::Json => with_plain_stdout(|out| write_json(&execution.report, out)),
    }
}

fn with_plain_stdout(write: impl FnOnce(&mut dyn Write) -> io::Result<()>) -> io::Result<()> {
    let mut out = BufWriter::with_capacity(OUTPUT_BUFFER_BYTES, io::stdout().lock());
    write(&mut out)?;
    out.flush()
}

fn write_agent(execution: &Execution, out: &mut dyn Write) -> io::Result<()> {
    const RENDER_CHUNK: usize = 2048;
    let report = &execution.report;
    let window = RENDER_CHUNK * execution.install(rayon::current_num_threads);
    for chunk in report.findings.chunks(window) {
        let buffers = execution.install(|| {
            chunk
                .par_chunks(RENDER_CHUNK)
                .map(render_findings)
                .collect::<io::Result<Vec<Vec<u8>>>>()
        })?;
        for buffer in &buffers {
            out.write_all(buffer)?;
        }
    }
    if !report.findings.is_empty() {
        writeln!(out)?;
        writeln!(out, "rules:")?;
        for (rule, why) in &report.rules_fired {
            writeln!(
                out,
                "  {}: {}",
                TerminalText::line(rule),
                TerminalText::line(why)
            )?;
        }
    }
    writeln!(out, "{}", summary_line(&report.summary))?;
    write_skipped_gate(&report.skipped_gate, out)
}

fn write_skipped_gate(
    gate: &[sloplint_core::baseline::SkippedEntry],
    out: &mut dyn Write,
) -> io::Result<()> {
    for entry in gate {
        writeln!(
            out,
            "skipped: {} ({})",
            TerminalText::line(&entry.path),
            TerminalText::line(&entry.reason),
        )?;
    }
    Ok(())
}

fn render_findings(findings: &[Finding]) -> io::Result<Vec<u8>> {
    let mut buffer = Vec::new();
    let mut digits = itoa::Buffer::new();
    for finding in findings {
        TerminalText::line(&finding.path).write_escaped(&mut buffer)?;
        buffer.write_all(b":")?;
        buffer.write_all(digits.format(finding.line).as_bytes())?;
        buffer.write_all(b":")?;
        buffer.write_all(digits.format(finding.column).as_bytes())?;
        buffer.write_all(b": ")?;
        buffer.write_all(finding.severity.as_str().as_bytes())?;
        buffer.write_all(b"/")?;
        buffer.write_all(finding.confidence.as_str().as_bytes())?;
        buffer.write_all(b": ")?;
        TerminalText::line(&finding.message).write_escaped(&mut buffer)?;
        buffer.write_all(b" [")?;
        TerminalText::line(&finding.rule).write_escaped(&mut buffer)?;
        buffer.write_all(b"]\n  context: ")?;
        TerminalText::line(&finding.context).write_escaped(&mut buffer)?;
        buffer.write_all(b"\n  fix: ")?;
        TerminalText::line(&finding.fix).write_escaped(&mut buffer)?;
        buffer.write_all(b"\n")?;
    }
    Ok(buffer)
}

fn write_json(report: &Report, out: &mut dyn Write) -> io::Result<()> {
    let mut record = Vec::with_capacity(RECORD_BUFFER_BYTES);
    out.write_all(b"{\"version\":1,\n \"findings\":[")?;
    for (index, finding) in report.findings.iter().enumerate() {
        if index > 0 {
            out.write_all(b",")?;
        }
        write_record(out, &mut record, finding)?;
    }
    out.write_all(b"],\n \"rules\":")?;
    write_record(out, &mut record, &report.rules_fired)?;
    out.write_all(b",\n \"summary\":")?;
    write_record(out, &mut record, &report.summary)?;
    if !report.skipped_gate.is_empty() {
        out.write_all(b",\n \"skipped_gate\":")?;
        write_record(out, &mut record, &report.skipped_gate)?;
    }
    out.write_all(b"}\n")
}

fn write_record<T>(out: &mut dyn Write, record: &mut Vec<u8>, value: &T) -> io::Result<()>
where
    T: sonic_rs::Serialize + ?Sized,
{
    record.clear();
    sonic_rs::to_writer(&mut *record, value).map_err(io::Error::other)?;
    write_guarded(out, record)
}

fn write_guarded(out: &mut dyn Write, json: &[u8]) -> io::Result<()> {
    let mut run = 0;
    let mut at = 0;
    while let Some((off, width, code)) = next_control(json, at) {
        if code != 0x0a {
            out.write_all(&json[run..off])?;
            write!(out, "\\u{code:04x}")?;
            run = off + width;
        }
        at = off + width;
    }
    out.write_all(&json[run..])
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
    writeln!(out, "{}", summary_line(&execution.report.summary))?;
    write_skipped_gate(&execution.report.skipped_gate, out)
}

struct Source {
    text: String,
    starts: Vec<u32>,
}

impl Source {
    fn new(text: String) -> Self {
        let starts = line_starts(&text);
        Source { text, starts }
    }

    fn line(&self, line: u32) -> Option<&str> {
        let index = usize::try_from(line.saturating_sub(1)).ok()?;
        let start = usize::try_from(*self.starts.get(index)?).ok()?;
        let rest = self.text.get(start..)?;
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
    let raw = sources
        .line(&finding.path, finding.line)
        .map_or(&*finding.context, str::trim_end);
    let text = TerminalText::line(raw).into_text();
    let start = span_for(raw, finding.column).start;
    let span = TerminalText::line(&raw[..start]).into_text().len()..text.len();
    let level = match finding.severity {
        sloplint_core::finding::Severity::Blocker => Level::ERROR,
        sloplint_core::finding::Severity::Critical => Level::WARNING,
        sloplint_core::finding::Severity::Major => Level::INFO,
        sloplint_core::finding::Severity::Minor => Level::NOTE,
        sloplint_core::finding::Severity::Info => Level::HELP,
    };
    let why = report
        .rules_fired
        .get(&*finding.rule)
        .map_or("", String::as_str);
    let head = format!(
        "{}/{}: {}",
        finding.severity.as_str(),
        finding.confidence.as_str(),
        finding.message
    );
    let title = level
        .primary_title(TerminalText::line(&head).into_text())
        .id(TerminalText::line(&finding.rule).into_text());
    let snippet = Snippet::source(text)
        .line_start(usize::try_from(finding.line).unwrap_or(usize::MAX))
        .path(TerminalText::line(&finding.path).into_text())
        .annotation(AnnotationKind::Primary.span(span));
    let groups = [
        Group::with_title(title).element(snippet),
        Group::with_title(
            Level::HELP.secondary_title(TerminalText::line(&finding.fix).into_text()),
        ),
        Group::with_title(Level::NOTE.secondary_title(TerminalText::line(why).into_text())),
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

fn summary_line(summary: &Summary) -> String {
    format!(
        "summary: strictness={} files={} findings={} blocker={} critical={} major={} minor={} info={} baselined={} stale={} skipped={}",
        summary.strictness.as_str(),
        summary.files,
        summary.findings,
        summary.blocker,
        summary.critical,
        summary.major,
        summary.minor,
        summary.info,
        summary.baselined,
        summary.stale,
        summary.skipped
    )
}

fn print_text_status(text: &str, kind: TextKind) -> u8 {
    let written = with_buffered_stdout(|out| match kind {
        TextKind::Plain => write!(out, "{}", TerminalText::block(text)),
        TextKind::Json => write_guarded(out, text.as_bytes()),
    });
    sink_status(written, 0)
}

fn sink_status(result: io::Result<()>, success: u8) -> u8 {
    match result {
        Ok(()) => success,
        Err(error) if error.kind() == io::ErrorKind::BrokenPipe => success,
        Err(error) => {
            emit_error(&error.to_string());
            3
        }
    }
}

fn fail(message: &str, code: u8) -> ExitCode {
    emit_error(message);
    ExitCode::from(code)
}

fn emit_error(message: &str) {
    let _ = writeln!(
        io::stderr(),
        "sloplint: error: {}",
        TerminalText::line(message)
    );
}

fn report_error(error: &RunError) -> ExitCode {
    fail(&error.to_string(), error.exit_code())
}
