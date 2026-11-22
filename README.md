# news

A Rust CLI to read full news articles

![news in a terminal](preview.png)

## Why this exists

Feeds give headlines; readers give pages full of scripts and layout. This tool
takes one source, follows each story to its article, and prints the text in the
terminal with nothing else around it.

## How it works

The selected source and topic choose an RSS feed, or GitHub's trending page
when the source is `github`. One HTTP client fetches the feed with TLS
verification, timeouts, redirect limits, and response size caps. RSS is parsed
locally, and source-specific metadata is added where it matters: Hacker News
points and comments, GitHub language and star count, and the linked domain for
aggregators.

By default it prints the full article text. Omarchy supplies the full article
in its feed; BBC, Hacker News, and Lobsters pages are fetched and extracted when
needed. If extraction fails, the short feed summary is used. `--summary` forces
the short version, and `--json` prints the same data as structured output.
Output is plain text with a headline, source line, URL, and article text,
wrapped to the terminal width. One source is fetched per run; there is no
aggregate mode.

## Usage

```text
news [OPTIONS] <SOURCE> [TOPIC]

Arguments:
  <SOURCE>  News source [bbc, github, hackernews, lobsters, omarchy]
  [TOPIC]   Topic or time range for the source

Options:
  -n, --limit <N>       Number of stories (default: 8)
      --summary         Show a short summary instead of the full article
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
news --json bbc -n 5
news sources
```

## Notes

BBC topics are `top`, `world`, `uk`, `business`, `politics`, `health`,
`education`, `science`, `technology`, `entertainment`, `sport`, and `football`.
GitHub takes `daily`, `weekly`, or `monthly`. Hacker News takes `frontpage`,
`newest`, `ask`, `show`, `jobs`, or `best`. Lobsters takes any tag, such as
`rust`, `linux`, `security`, or `programming`. Omarchy has no topics. The
default topic is the source's front page or daily range.

TLS is verified by `rustls`. Only public HTTP(S) pages are fetched; localhost,
private, loopback, link-local, and cloud metadata addresses are skipped, and
redirects follow the same rules. Responses are capped at 4 MiB, decompressed
pages at 8 MiB, and timeouts and redirects are bounded. Terminal control
characters are stripped from feed text, scraped text, hosts, and URLs.

No credentials are read, no configuration is written, and no telemetry is sent.
Unsafe code is forbidden by the package lint configuration.

Color uses the terminal palette and ANSI attributes only. Article text inherits
the terminal foreground, while headlines and metadata use semantic roles.
`--color never`, `NO_COLOR`, `CLICOLOR=0`, `TERM=dumb`, and non-terminal output
all disable it. JSON is never colored.

## Install

**Requirements:** Rust 1.88+, `git`, a C toolchain, and CMake for the TLS
dependency.

```bash
cargo install --git https://github.com/batusaribay/news.git --locked
```

## License

This project is licensed under the [MIT License](LICENSE).
