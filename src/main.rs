mod fetch;
mod render;
mod source;
mod story;

use std::env;
use std::ffi::OsStr;
use std::io::{self, BufWriter, ErrorKind, IsTerminal, Write};
use std::process::ExitCode;
use std::time::Duration;

use anyhow::{Context, Result};
use chrono::Utc;
use clap::{CommandFactory, Parser};

use crate::render::{Renderer, terminal_width};
use crate::source::Source;

/// A news reader with article text in the terminal
#[derive(Debug, Parser)]
#[command(
    name = "news-reader",
    version,
    about = "A news reader with article text in the terminal",
    styles = clap::builder::Styles::plain()
)]
struct Cli {
    /// News source
    #[arg(value_enum, value_name = "SOURCE")]
    source: Source,

    /// Topic or time range for the source (e.g. sport, ask, monthly)
    #[arg(value_name = "TOPIC")]
    topic: Option<String>,

    /// Number of stories
    #[arg(short = 'n', long, default_value_t = 8, value_name = "N", value_parser = parse_limit)]
    limit: usize,

    /// List sources and exit
    #[arg(long)]
    sources: bool,

    /// Do not print article URLs
    #[arg(long)]
    no_links: bool,

    /// Disable colors and hyperlinks (NO_COLOR is also respected)
    #[arg(long)]
    no_color: bool,

    /// HTTP timeout in seconds
    #[arg(long, default_value_t = 15, value_name = "SECS", value_parser = parse_timeout)]
    timeout: u64,
}

fn parse_limit(value: &str) -> std::result::Result<usize, String> {
    let limit = value
        .parse::<usize>()
        .map_err(|_| "must be a whole number".to_string())?;
    if (1..=100).contains(&limit) {
        Ok(limit)
    } else {
        Err("must be between 1 and 100".to_string())
    }
}

fn parse_timeout(value: &str) -> std::result::Result<u64, String> {
    let timeout = value
        .parse::<u64>()
        .map_err(|_| "must be a whole number".to_string())?;
    if (1..=120).contains(&timeout) {
        Ok(timeout)
    } else {
        Err("must be between 1 and 120 seconds".to_string())
    }
}

fn main() -> ExitCode {
    let args: Vec<_> = env::args_os().skip(1).collect();

    if args.is_empty() {
        return print_help();
    }
    let wants_sources = args
        .iter()
        .any(|arg| arg.as_os_str() == OsStr::new("--sources"));
    if args[0].as_os_str() == OsStr::new("sources") || wants_sources {
        return print_sources();
    }

    let cli = Cli::parse();
    if cli.sources {
        return print_sources();
    }

    match run(cli) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("error: {error:#}");
            ExitCode::FAILURE
        }
    }
}

fn print_help() -> ExitCode {
    let mut command = Cli::command();
    let result = command.print_help().and_then(|_| writeln!(io::stdout()));
    finish_output(result)
}

fn print_sources() -> ExitCode {
    let stdout = io::stdout();
    let mut out = BufWriter::new(stdout.lock());
    let result = (|| -> io::Result<()> {
        for source in Source::ALL {
            writeln!(out, "{:<11} {}", source.name(), source.description())?;
        }
        out.flush()
    })();

    finish_output(result)
}

fn finish_output(result: io::Result<()>) -> ExitCode {
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) if error.kind() == ErrorKind::BrokenPipe => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("error: {error}");
            ExitCode::FAILURE
        }
    }
}

fn run(cli: Cli) -> Result<()> {
    let stdout_terminal = io::stdout().is_terminal();
    let dumb_terminal = env::var("TERM").is_ok_and(|term| term == "dumb");
    let color_allowed = !cli.no_color && env::var_os("NO_COLOR").is_none() && !dumb_terminal;
    let stdout_color = color_allowed && stdout_terminal;
    let hyperlinks = color_allowed && stdout_terminal && !dumb_terminal;

    let client = fetch::client(Duration::from_secs(cli.timeout))?;

    let topic = cli
        .topic
        .as_deref()
        .map(str::trim)
        .filter(|topic| !topic.is_empty());
    let stories = fetch::fetch(cli.source, topic, &client, cli.limit)?;

    let now = Utc::now();
    let width = terminal_width();
    let stdout = io::stdout();
    let mut out = BufWriter::new(stdout.lock());

    let printed = (|| -> io::Result<()> {
        let mut renderer = Renderer::new(&mut out, width, stdout_color, hyperlinks, !cli.no_links);
        for story in &stories {
            renderer.print_story(cli.source, topic, story, now)?;
        }
        renderer.flush()
    })();

    match printed {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == ErrorKind::BrokenPipe => Ok(()),
        Err(error) => Err(error).context("could not write output"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_one_source_and_topic() {
        let cli = Cli::try_parse_from(["news-reader", "hackernews", "ask"]).unwrap();
        assert_eq!(cli.source, Source::Hn);
        assert_eq!(cli.topic.as_deref(), Some("ask"));
        assert!(!cli.no_links);
        assert!(!cli.sources);
    }

    #[test]
    fn source_is_required() {
        assert!(Cli::try_parse_from(["news-reader"]).is_err());
    }

    #[test]
    fn help_has_no_terminal_styling() {
        let help = Cli::command().render_help().to_string();
        assert!(!help.contains('\x1b'));
    }
}
