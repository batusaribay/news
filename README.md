# News Reader

A terminal news reader with article text.

![news-reader showing a Lobsters story](preview.png)

## Why

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
   the terminal width and stays unstyled when piped.
6. One source is fetched per run. If it fails, the command fails; there is no
   aggregate mode.

## Usage

Running `news-reader` with no arguments prints help.

```text
Usage: news-reader [OPTIONS] <SOURCE> [TOPIC]

Arguments:
  <SOURCE>  News source [possible values: bbc, github, hackernews, lobsters, omarchy]
  [TOPIC]   Topic or time range for the source (e.g. sport, ask, monthly)

Options:
  -n, --limit <N>       Number of stories [default: 8]
      --sources         List sources and exit
      --no-links        Do not print article URLs
      --no-color        Disable colors and hyperlinks (NO_COLOR is also respected)
      --timeout <SECS>  HTTP timeout in seconds [default: 15]
  -h, --help            Print help
  -V, --version         Print version
```

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
src/main.rs      CLI, startup, source list, and render orchestration
src/source.rs    source names, descriptions, topics, feed URLs
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

- Article extraction depends on the destination page. Paywalls, JavaScript
  applications, and anti-bot walls can leave a story without an excerpt.
- GitHub Trending is scraped because it has no RSS feed. A layout change can
  break that source until the selectors are updated.
- There is no cache; every run fetches fresh data.

## Future ideas

- JSON output for scripts
- More sources and topic aliases
- Optional local cache
- Prebuilt release binaries

## License

This project is licensed under the [MIT License](LICENSE).
