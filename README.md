# News Reader

A terminal news reader with article text.

![Preview](preview.png)

## What it is

A single Rust binary that prints one news source at a time: headline, source
line, URL, and a short excerpt of the article. It covers BBC, GitHub Trending,
Hacker News, Lobsters, and Omarchy News.

## Why this exists

A headline says what happened; the first few lines say whether it matters.
This prints both, so you can check one source and get back to work.

It is intentionally small: one source per run, no accounts, no cache, no TUI,
no browser.

## How it works

1. The selected source and topic choose an RSS feed, or GitHub's trending page
   when the source is `github`.
2. A single HTTP client fetches the feed with TLS verification, timeouts,
   redirect limits, and response size caps.
3. RSS is parsed locally. Source-specific metadata is added where it is useful:
   Hacker News points and comments, GitHub language and star count, and the
   linked domain for link aggregators.
4. Sources that provide article text in the feed (BBC and Omarchy) use it
   directly. Sources that do not (Hacker News and Lobsters) fetch the linked
   page, extract the readable content, and print a short excerpt.
5. Output is plain text: a headline, source line, URL, and excerpt. It wraps to
   the terminal width. Headlines are bold; metadata and URLs are dimmed.
6. One source is fetched per run. If it fails, the command fails; there is no
   aggregate mode.

## Usage

### Commands

| Command | Action |
| --- | --- |
| `news-reader` | Print help. |
| `news-reader <source>` | Latest stories from one source. |
| `news-reader <source> <topic>` | A topic or time range. |
| `news-reader sources` | List sources and descriptions. |
| `news-reader --version` | Show the version. |

### Sources and topics

`news-reader sources` (or `news-reader --sources`) prints the source names and
a short description of each.

| Source       | Topics                                                                                                        |
| ------------ | ------------------------------------------------------------------------------------------------------------- |
| `bbc`        | `top`, `world`, `uk`, `business`, `politics`, `health`, `education`, `science`, `technology`, `entertainment`, `sport`, `football` |
| `github`     | `daily`, `weekly`, `monthly`                                                                                    |
| `hackernews` | `frontpage`, `newest`, `ask`, `show`, `jobs`, `best`                                                            |
| `lobsters`   | any tag, e.g. `rust`, `linux`, `security`, `programming`                                                      |
| `omarchy`    | no topics                                                                                                      |

The topic defaults to the source's front page or daily range.

### Examples

```bash
news-reader bbc
news-reader bbc sport
news-reader github monthly
news-reader hackernews ask
news-reader lobsters rust

news-reader sources
news-reader bbc -n 5
news-reader bbc --no-links
NO_COLOR=1 news-reader bbc
```

## Installation

**Requirements:** Rust 1.88+, Git, a C toolchain, and CMake for the TLS dependency.

```bash
cargo install --git https://github.com/batusaribay/news-reader.git --locked
```

## Uninstall

```bash
cargo uninstall news-reader
```

## Security

- TLS certificates are verified by `rustls`.
- Only public HTTP(S) pages are fetched. Localhost, private, loopback,
  link-local, and cloud metadata addresses are skipped, and redirects are
  checked against the same rules.
- Feed responses are capped at 4 MiB and article pages at 4 MiB; decompressed
  pages are capped at 8 MiB.
- Connect, request, and article timeouts are bounded, and redirects are limited.
- Terminal control characters are stripped from feed text, scraped text, hosts,
  and URLs before anything is printed.
- Feeds and article pages are fetched over the network, but no credentials are
  read, no configuration is written, and no telemetry is sent.
- Unsafe code is forbidden by the package lint configuration.

## License

This project is licensed under the [MIT License](LICENSE).
