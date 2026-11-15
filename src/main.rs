mod fetch;
mod render;
mod source;
mod story;

use std::env;
use std::io::{self, BufWriter, ErrorKind, IsTerminal};
use std::process::ExitCode;
use std::time::Duration;

use anyhow::{Context, Result};
use chrono::Utc;
use clap::Parser;

use crate::render::{Renderer, terminal_width};
use crate::source::Source;

/// Print news as plain text in the terminal.
#[derive(Debug, Parser)]
#[command(
    name = "news-reader",
    version,
    about = "Print news as plain text in the terminal.",
    styles = clap::builder::Styles::plain()
)]
struct Cli {
    /// News source
    #[arg(value_enum, value_name = "SOURCE")]
    source: Source,

    /// Topic or time range for the source (e.g. sport, business, monthly)
    #[arg(value_name = "TOPIC")]
    topic: Option<String>,

    /// Number of stories
    #[arg(short = 'n', long, default_value_t = 8, value_name = "N", value_parser = parse_limit)]
    limit: usize,

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
    let cli = Cli::parse();

    match run(cli) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("error: {error:#}");
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
    use clap::CommandFactory;

    #[test]
    fn parses_one_source_and_topic() {
        let cli = Cli::try_parse_from(["news-reader", "hackernews", "ask"]).unwrap();
        assert_eq!(cli.source, Source::Hn);
        assert_eq!(cli.topic.as_deref(), Some("ask"));
        assert!(!cli.no_links);
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
