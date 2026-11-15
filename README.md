# News Reader

A small, plain-text news reader for the terminal. It reads one source at a
time, prints the article URL, and shows a short excerpt of the article itself.

![news-reader in a terminal](preview.png)

## Why

Headlines alone rarely tell you enough, and opening several news sites to find
out what happened is noisy. This prints a focused digest and exits.

There is one binary, one source per run, no TUI, no GUI, no configuration file,
and no background polling.

## How it works

1. The selected source and topic choose an RSS feed, or GitHub's trending page
   when the source is `github`.
2. A single HTTP client fetches the feed with TLS verification, timeouts,
   redirect limits, and response size caps.
3. RSS is parsed locally. Source-specific metadata is added where it is useful:
   Hacker News points and comments, the Google News publisher, GitHub language
   and star count, and the linked domain for link aggregators.
4. Sources that provide article text in the feed (BBC and Omarchy) use it
   directly. Sources that do not (Hacker News and Lobsters) fetch the linked
   page, extract the readable content, and print a short excerpt.
5. Output is plain text: a headline, source line, URL, and excerpt. It wraps to
   the terminal width and stays unstyled when piped.
6. One source is fetched per run. If it fails, the command fails; there is no
   aggregate mode.

## Usage

```text
Usage: news-reader [OPTIONS] <SOURCE> [TOPIC]

Arguments:
  <SOURCE>  News source [possible values: bbc, google, lobsters, omarchy, github, hackernews]
  [TOPIC]   Topic or time range for the source (e.g. sport, business, monthly)

Options:
  -n, --limit <N>       Number of stories [default: 8]
      --no-links        Do not print article URLs
      --no-color        Disable colors and hyperlinks (NO_COLOR is also respected)
      --timeout <SECS>  HTTP timeout in seconds [default: 15]
  -h, --help            Print help
  -V, --version         Print version
```

### Sources and topics

| Source       | Topics                                                                                                        |
| ------------ | ------------------------------------------------------------------------------------------------------------- |
| `bbc`        | `top`, `world`, `uk`, `business`, `politics`, `health`, `education`, `science`, `technology`, `entertainment`, `sport`, `football` |
| `google`     | `top`, `world`, `nation`, `business`, `technology`, `entertainment`, `sports`, `science`, `health`             |
| `lobsters`   | any tag, e.g. `rust`, `linux`, `security`, `programming`                                                      |
| `omarchy`    | no topics                                                                                                      |
| `github`     | `daily`, `weekly`, `monthly`                                                                                    |
| `hackernews` | `frontpage`, `newest`, `ask`, `show`, `jobs`, `best`                                                            |

The topic defaults to the source's front page or daily range.

### Examples

```bash
news-reader bbc
news-reader bbc sport
news-reader google business
news-reader github monthly
news-reader hackernews ask
news-reader lobsters rust

news-reader bbc -n 5
news-reader bbc --no-links
NO_COLOR=1 news-reader bbc
```

## Installation

**Requirements:** Rust 1.88+, a C toolchain, and CMake for the TLS dependency.

```bash
git clone https://github.com/batusaribay/news-reader.git
cd news-reader
cargo build --release
cargo install --path . --locked
```

The binary is installed as `news-reader`. You can also install directly from
the repository:

```bash
cargo install --git https://github.com/batusaribay/news-reader.git --locked
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

## Files

```text
src/main.rs      CLI, startup, and render orchestration
src/source.rs    source names, topics, feed and trending URLs
src/fetch.rs     HTTP client, RSS, GitHub Trending, article extraction
src/story.rs     story model, dates, text cleanup, excerpts
src/render.rs    terminal output and wrapping
scripts/make-terminal-preview.py   terminal preview renderer
preview.txt                        source transcript for preview.png
```

## Development

```bash
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
cargo build --release
```

The README preview is generated from `preview.txt`:

```bash
python3 scripts/make-terminal-preview.py preview.txt preview.png
```

## Known issues

- Google News' RSS feed does not provide article text, so those entries show
  only the headline, publisher, and URL.
- Article extraction depends on the destination page. Paywalls, JavaScript
  applications, and anti-bot walls can leave a story without an excerpt.
- GitHub Trending is scraped because it has no RSS feed. A layout change can
  break that source until the selectors are updated.
- Google News uses its US English edition.
- There is no cache; every run fetches fresh data.

## Future ideas

- Optional default source and topic in a config file
- JSON output for scripts
- More sources and topic aliases
- Language and regional edition options
- Prebuilt release binaries

## License

This project is licensed under the [MIT License](LICENSE).
