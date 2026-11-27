mod config;
mod fetch;
mod render;
mod source;
mod story;
mod theme;

use std::env;
use std::ffi::{OsStr, OsString};
use std::io::{self, BufWriter, ErrorKind, Write};
use std::process::ExitCode;
use std::time::Duration;

use anyhow::{Context, Result};
use chrono::Utc;
use clap::{CommandFactory, Parser};

use crate::render::{Renderer, terminal_width};
use crate::source::Source;
use crate::theme::{ColorMode, Theme};

/// A Rust CLI to read news articles
#[derive(Debug, Parser)]
#[command(
    name = "news",
    version,
    about = "A Rust CLI to read news articles",
    styles = clap::builder::Styles::plain(),
    color = clap::ColorChoice::Never
)]
struct Cli {
    /// News source
    #[arg(value_enum, value_name = "SOURCE", required_unless_present = "sources")]
    source: Option<Source>,

    /// Topic or time range for the source (e.g. sport, ask, monthly)
    #[arg(value_name = "TOPIC")]
    topic: Option<String>,

    /// Number of stories (default: 8)
    #[arg(short = 'n', long, value_name = "N", value_parser = parse_limit)]
    limit: Option<usize>,

    /// Show a short summary instead of the full article
    #[arg(long)]
    summary: bool,

    /// Reading speed in characters per minute
    #[arg(long, value_name = "N", value_parser = parse_cpm)]
    cpm: Option<u32>,

    /// List sources and exit
    #[arg(long)]
    sources: bool,

    /// Do not print article URLs
    #[arg(long)]
    no_links: bool,

    /// Print JSON instead of formatted text
    #[arg(long)]
    json: bool,

    /// HTTP timeout in seconds (default: 15)
    #[arg(long, value_name = "SECS", value_parser = parse_timeout)]
    timeout: Option<u64>,

    /// Color output: auto, always, or never
    #[arg(long, value_enum, default_value = "auto", value_name = "WHEN")]
    color: ColorMode,
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

fn parse_cpm(value: &str) -> std::result::Result<u32, String> {
    let cpm = value
        .parse::<u32>()
        .map_err(|_| "must be a whole number".to_string())?;
    if cpm == 0 {
        return Err("must be greater than 0".to_string());
    }
    Ok(cpm)
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
    let mut args: Vec<_> = env::args_os().skip(1).collect();

    if args.is_empty() {
        return print_help();
    }
    if args[0] == OsStr::new("sources") {
        args[0] = OsString::from("--sources");
    }

    let cli = Cli::parse_from(std::iter::once(OsString::from("news")).chain(args));
    if cli.sources {
        return print_sources(cli.json);
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

fn print_sources(json: bool) -> ExitCode {
    let stdout = io::stdout();
    let mut out = BufWriter::new(stdout.lock());
    let result = (|| -> io::Result<()> {
        if json {
            let sources: Vec<_> = Source::ALL
                .iter()
                .map(|source| {
                    serde_json::json!({
                        "name": source.name(),
                        "description": source.description(),
                    })
                })
                .collect();
            serde_json::to_writer_pretty(&mut out, &sources).map_err(io::Error::other)?;
            writeln!(out)?;
        } else {
            for source in Source::ALL {
                writeln!(out, "{:<11} {}", source.name(), source.description())?;
            }
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
    let config = config::load()?;
    let limit = cli.limit.or(config.limit).unwrap_or(config::DEFAULT_LIMIT);
    let timeout = cli
        .timeout
        .or(config.timeout)
        .unwrap_or(config::DEFAULT_TIMEOUT);
    let client = fetch::client(Duration::from_secs(timeout))?;

    let topic = cli
        .topic
        .as_deref()
        .map(str::trim)
        .filter(|topic| !topic.is_empty());
    let source = cli
        .source
        .expect("clap requires a source when --sources is absent");
    let stories = fetch::fetch(source, topic, &client, limit, cli.summary)?;

    let now = Utc::now();
    let width = terminal_width();
    let stdout = io::stdout();
    let mut out = BufWriter::new(stdout.lock());

    if cli.json {
        let stories: Vec<_> = stories
            .iter()
            .map(|story| {
                serde_json::json!({
                    "title": story.title,
                    "url": story.url,
                    "published": story.published.map(|date| date.to_rfc3339()),
                    "summary": story.summary,
                    "details": story.details,
                    "domain": story.domain,
                    "full": story.full,
                })
            })
            .collect();
        let value = serde_json::json!({
            "source": source.name(),
            "topic": topic,
            "stories": stories,
        });
        let printed = serde_json::to_writer_pretty(&mut out, &value)
            .map_err(io::Error::other)
            .and_then(|()| writeln!(out))
            .and_then(|()| out.flush());
        return match printed {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == ErrorKind::BrokenPipe => Ok(()),
            Err(error) => Err(error).context("could not write output"),
        };
    }

    let printed = (|| -> io::Result<()> {
        let theme = Theme::from_env(cli.color);
        let mut renderer = Renderer::new(&mut out, width, !cli.no_links, theme, cli.cpm);
        for story in &stories {
            renderer.print_story(source, topic, story, now, cli.summary)?;
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
        let cli = Cli::try_parse_from(["news", "hackernews", "ask"]).unwrap();
        assert_eq!(cli.source, Some(Source::Hn));
        assert_eq!(cli.topic.as_deref(), Some("ask"));
        assert!(!cli.no_links);
        assert!(!cli.sources);
        assert!(!cli.summary);
        assert_eq!(cli.cpm, None);
    }

    #[test]
    fn parses_cpm_and_rejects_zero() {
        let cli = Cli::try_parse_from(["news", "--cpm", "600", "bbc"]).unwrap();
        assert_eq!(cli.cpm, Some(600));
        assert!(Cli::try_parse_from(["news", "--cpm", "0", "bbc"]).is_err());
        assert!(Cli::try_parse_from(["news", "--cpm", "fast", "bbc"]).is_err());
    }

    #[test]
    fn source_is_required() {
        assert!(Cli::try_parse_from(["news"]).is_err());
    }

    #[test]
    fn source_listing_still_validates_all_options() {
        let cli = Cli::try_parse_from(["news", "--sources", "--json"]).unwrap();
        assert!(cli.sources);
        assert!(cli.json);
        assert!(Cli::try_parse_from(["news", "--sources", "--timeout", "0"]).is_err());
        assert!(Cli::try_parse_from(["news", "--sources", "--unknown"]).is_err());
    }

    #[test]
    fn help_has_no_terminal_styling() {
        let help = Cli::command().render_help().to_string();
        assert!(!help.contains('\x1b'));
    }
}
