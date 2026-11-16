# Newspaper

A newspaper in the terminal

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
| `newspaper` | Print help. |
| `newspaper <source>` | Latest stories from one source. |
| `newspaper <source> <topic>` | A topic or time range. |
| `newspaper sources` | List sources and descriptions. |
| `newspaper --version` | Show the version. |

### Sources and topics

`newspaper sources` (or `newspaper --sources`) prints the source names and
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
newspaper bbc
newspaper bbc sport
newspaper github monthly
newspaper hackernews ask
newspaper lobsters rust

newspaper sources
newspaper bbc -n 5
newspaper bbc --no-links
NO_COLOR=1 newspaper bbc
```

## Installation

**Requirements:** Rust 1.88+, Git, a C toolchain, and CMake for the TLS dependency.

```bash
cargo install --git https://github.com/batusaribay/newspaper.git --locked
```

## Uninstall

```bash
cargo uninstall newspaper
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
