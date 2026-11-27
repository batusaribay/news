# news

A Rust CLI to read news articles

![news in a terminal](preview.png)

## Why this exists

Feeds give headlines; readers give pages full of scripts and layout. This tool
takes one source, follows each story to its article, and prints the text in the
terminal with nothing else around it.

## How it works

The selected source and topic choose an RSS feed, GitHub's trending page when
the source is `github`, or the GitHub releases API when it is
`omarchy-releases`. One HTTP client fetches the feed with TLS verification,
timeouts, redirect limits, and response size caps. Feeds and release notes are
parsed locally, and source-specific metadata is added where it matters: Hacker
News points and comments, GitHub language and star count, release tags, and the
linked domain for aggregators.

By default it prints the full article text. Omarchy supplies the full article
in its feed; BBC, Hacker News, and Lobsters pages are fetched and extracted when
needed. If extraction fails, the short feed summary is used. `--summary` forces
the short version, and `--json` prints the same data as structured output.
Output is plain text with a headline, source line, URL, and article text,
wrapped to the terminal width. One source is fetched per run; there is no
aggregate mode.

## Usage

```text
news [OPTIONS] [SOURCE] [TOPIC]

Arguments:
  [SOURCE]  News source [bbc, github, hackernews, lobsters, omarchy,
            omarchy-releases]
  [TOPIC]   Topic or time range for the source

Options:
  -n, --limit <N>       Number of stories (default: 8)
      --summary         Show a short summary instead of the full article
      --slow            Print article text character by character
      --json            Print JSON instead of formatted text
      --sources         List sources and exit
      --no-links        Do not print article URLs
      --color <WHEN>    Color output: auto, always, or never [default: auto]
      --timeout <SECS>  HTTP timeout in seconds (default: 15)
  -h, --help            Print help
  -V, --version         Print version
```

```bash
news bbc
news bbc sport
news github monthly
news hackernews ask
news --summary bbc
news --slow bbc
news --json bbc -n 5
news omarchy-releases
news sources
```

## Notes

The default topic is each source's front page or daily range. BBC, GitHub,
Hacker News, and Lobsters accept their usual topic or time-range names;
`omarchy` and `omarchy-releases` have no topics.

## Config

Reads `~/.config/news/config.toml` if present. Flags override the file;
defaults apply otherwise.

## Install

**Requirements:** Rust 1.88+, `git`, a C toolchain, and CMake for the TLS
dependency.

```bash
cargo install --git https://github.com/batusaribay/news.git --locked
```

## License

This project is licensed under the [MIT License](LICENSE).
