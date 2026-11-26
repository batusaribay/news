use std::io::{self, Read};
use std::net::{IpAddr, SocketAddr, ToSocketAddrs};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use anyhow::{Context, Result, bail};
use dom_smoothie::{Config, Readability};
use flate2::read::GzDecoder;
use reqwest::blocking::Client;
use reqwest::header::CONTENT_TYPE;
use reqwest::redirect::Policy;
use rss::{Channel, Item};
use scraper::{ElementRef, Html, Selector};
use serde_json::Value;

use crate::source::{Source, github_trending_url};
use crate::story::{Story, clean_text, excerpt, hn_details, host_of, parse_date};

const MAX_FEED_BYTES: usize = 4 * 1024 * 1024;
const MAX_PAGE_BYTES: usize = 4 * 1024 * 1024;
const MAX_DECODED_BYTES: usize = 8 * 1024 * 1024;

/// Shared HTTP client: bounded redirects, no local destinations.
pub fn client(timeout: Duration) -> Result<Client> {
    Client::builder()
        .user_agent(format!(
            "news/{} (terminal news)",
            env!("CARGO_PKG_VERSION")
        ))
        .timeout(timeout)
        .connect_timeout(Duration::from_secs(5))
        // Checking the URL host is not enough: an attacker-controlled feed can
        // point a public-looking name at a private address (DNS rebinding).
        // Resolve once here and drop every non-public address before reqwest
        // connects.
        .dns_resolver(Arc::new(PublicResolver))
        .redirect(Policy::custom(|attempt| {
            if attempt.previous().len() >= 8 || !is_fetchable(attempt.url().as_str()) {
                attempt.stop()
            } else {
                attempt.follow()
            }
        }))
        .build()
        .context("could not initialise the HTTP client")
}

/// A resolver that only returns publicly routable addresses.
struct PublicResolver;

impl reqwest::dns::Resolve for PublicResolver {
    fn resolve(&self, name: reqwest::dns::Name) -> reqwest::dns::Resolving {
        let host = name.as_str().to_string();
        Box::pin(async move {
            let addrs: Vec<SocketAddr> = (host.as_str(), 0)
                .to_socket_addrs()?
                .filter(|addr| is_public_ip(addr.ip()))
                .collect();
            if addrs.is_empty() {
                return Err(format!("{host} resolves only to non-public addresses").into());
            }
            Ok(Box::new(addrs.into_iter()) as reqwest::dns::Addrs)
        })
    }
}

/// Fetch the selected source and topic.
pub fn fetch(
    source: Source,
    topic: Option<&str>,
    client: &Client,
    limit: usize,
    summary_only: bool,
) -> Result<Vec<Story>> {
    if source == Source::Github {
        return fetch_github_trending(client, limit, topic);
    }
    if source == Source::OmarchyReleases {
        if topic.is_some() {
            bail!("Omarchy Releases has no topics");
        }
        return fetch_omarchy_releases(client, limit, summary_only);
    }

    let url = source.feed_url(topic)?;
    let body = get_text(client, &url)?;
    let mut stories = parse_feed(source, &body, limit)?;

    enrich_articles(&mut stories, client, summary_only);

    Ok(stories)
}

fn parse_feed(source: Source, body: &str, limit: usize) -> Result<Vec<Story>> {
    let channel = Channel::read_from(body.as_bytes())
        .with_context(|| format!("could not parse the {} feed", source.label()))?;

    let stories: Vec<Story> = channel
        .items()
        .iter()
        .filter_map(|item| story_from_item(source, item))
        .take(limit)
        .collect();

    if stories.is_empty() {
        bail!("the feed contained no readable stories");
    }

    Ok(stories)
}

fn story_from_item(source: Source, item: &Item) -> Option<Story> {
    let title = clean_text(item.title()?);
    if title.is_empty() {
        return None;
    }

    let url = item
        .link()
        .map(str::trim)
        .filter(|url| !url.is_empty())
        .map(str::to_string)
        .or_else(|| {
            item.guid()
                .map(|guid| guid.value.trim())
                .filter(|guid| guid.starts_with("http"))
                .map(str::to_string)
        })
        .unwrap_or_default();

    let mut details = Vec::new();
    let mut summary = None;
    let mut full = None;

    match source {
        Source::Bbc | Source::Omarchy => {
            summary = item
                .description()
                .and_then(|description| excerpt(description, 900))
                .or_else(|| item.content().and_then(|content| excerpt(content, 900)));

            if source == Source::Omarchy {
                full = item.content().and_then(|content| {
                    let paragraphs = html_to_paragraphs(content);
                    (!paragraphs.is_empty()).then_some(paragraphs)
                });
            }
        }
        Source::Hn => details = hn_details(item.description().unwrap_or_default()),
        Source::Lobsters | Source::Github | Source::OmarchyReleases => {}
    }

    let domain = match source {
        Source::Bbc | Source::Omarchy | Source::Github => None,
        _ => host_of(&url),
    };

    Some(Story {
        title,
        url,
        published: parse_date(item.pub_date()),
        summary,
        full,
        details,
        domain,
    })
}

fn fetch_omarchy_releases(client: &Client, limit: usize, summary_only: bool) -> Result<Vec<Story>> {
    const URL: &str = "https://api.github.com/repos/omacom/omarchy/releases?per_page=100";
    let body = get_text(client, URL)?;
    let releases: Value =
        serde_json::from_str(&body).context("could not parse the Omarchy releases")?;
    let mut stories = parse_omarchy_releases(&releases, limit, summary_only)?;
    enrich_articles(&mut stories, client, summary_only);
    Ok(stories)
}

fn parse_omarchy_releases(value: &Value, limit: usize, summary_only: bool) -> Result<Vec<Story>> {
    let releases = value
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("GitHub returned an unexpected releases response"))?;
    let stories: Vec<Story> = releases
        .iter()
        .filter(|release| release["draft"].as_bool() != Some(true))
        .filter_map(|release| {
            let tag = release["tag_name"].as_str()?.trim();
            if tag.is_empty() {
                return None;
            }
            let name = release["name"]
                .as_str()
                .map(str::trim)
                .filter(|name| !name.is_empty())
                .unwrap_or(tag);
            let url = release["html_url"].as_str()?.trim();
            if !is_fetchable(url) {
                return None;
            }
            let body = release["body"].as_str().unwrap_or_default();
            let text = markdown_text(body);
            let summary = excerpt(&text, 900);
            let full = (!summary_only)
                .then(|| markdown_paragraphs(body))
                .filter(|paragraphs| !paragraphs.is_empty());
            let mut details = vec![format!("tag {tag}")];
            if release["prerelease"].as_bool() == Some(true) {
                details.push("prerelease".to_string());
            }
            Some(Story {
                title: clean_text(name),
                url: url.to_string(),
                published: crate::story::parse_date(release["published_at"].as_str()),
                summary,
                full,
                details,
                domain: None,
            })
        })
        .take(limit)
        .collect();

    if stories.is_empty() {
        bail!("GitHub returned no readable Omarchy releases");
    }
    Ok(stories)
}

fn markdown_paragraphs(body: &str) -> Vec<String> {
    body.split("\n\n")
        .map(markdown_text)
        .filter(|paragraph| !paragraph.is_empty())
        .collect()
}

fn markdown_text(input: &str) -> String {
    let mut output = String::with_capacity(input.len());
    let chars: Vec<char> = input.chars().collect();
    let mut index = 0;
    while index < chars.len() {
        match chars[index] {
            '!' if chars.get(index + 1) == Some(&'[') => {
                // Images add nothing to terminal release notes.
                index += 2;
                if let Some(end) = chars[index..].iter().position(|ch| *ch == ']') {
                    index += end + 1;
                    if chars.get(index) == Some(&'(')
                        && let Some(url_end) = chars[index + 1..].iter().position(|ch| *ch == ')')
                    {
                        index += url_end + 2;
                    }
                }
            }
            '[' => {
                if let Some(label_end) = chars[index + 1..].iter().position(|ch| *ch == ']') {
                    let label_end = index + 1 + label_end;
                    if chars.get(label_end + 1) == Some(&'(')
                        && let Some(url_end) =
                            chars[label_end + 2..].iter().position(|ch| *ch == ')')
                    {
                        let url_end = label_end + 2 + url_end;
                        let label: String = chars[index + 1..label_end].iter().collect();
                        let url: String = chars[label_end + 2..url_end].iter().collect();
                        output.push_str(&label);
                        if url.starts_with("http") && url != label {
                            output.push_str(&format!(" ({url})"));
                        }
                        index = url_end + 1;
                        continue;
                    }
                }
                output.push(chars[index]);
            }
            '`' | '*' => {}
            _ => output.push(chars[index]),
        }
        index += 1;
    }

    let lines: Vec<&str> = output
        .lines()
        .map(|line| line.trim().trim_start_matches(['#', '-', '+']).trim())
        .filter(|line| !line.is_empty())
        .collect();
    clean_text(&lines.join(" "))
}

/// Fetch full article text, or a short excerpt in summary mode.
/// Bounded to eight concurrent requests per source.
fn enrich_articles(stories: &mut [Story], client: &Client, summary_only: bool) {
    for chunk in stories.chunks_mut(8) {
        thread::scope(|scope| {
            let mut handles = Vec::new();
            for story in chunk.iter_mut() {
                if summary_only {
                    if story.summary.is_some() {
                        continue;
                    }
                } else if story.full.is_some() {
                    continue;
                }

                handles.push(scope.spawn(move || {
                    let Some(paragraphs) = fetch_article_text(client, &story.url) else {
                        return;
                    };
                    if summary_only {
                        story.summary = excerpt(&paragraphs.join("\n\n"), 700);
                    } else {
                        story.full = Some(paragraphs);
                    }
                }));
            }
            // A parser panic should lose one article, not the whole source.
            for handle in handles {
                let _ = handle.join();
            }
        });
    }
}

fn fetch_article_text(client: &Client, url: &str) -> Option<Vec<String>> {
    if !is_fetchable(url) || is_link_only_host(url) {
        return None;
    }

    let response = client
        .get(url)
        .timeout(Duration::from_secs(8))
        .send()
        .ok()?;
    if !response.status().is_success() {
        return None;
    }
    if let Some(content_type) = response
        .headers()
        .get(CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        && !content_type.contains("html")
    {
        return None;
    }

    let bytes = read_limited(response, MAX_PAGE_BYTES).ok()?;
    let html = decode_body(&bytes)?;
    let config = Config {
        max_elements_to_parse: 9000,
        ..Config::default()
    };
    let mut readability = Readability::new(html, Some(url), Some(config)).ok()?;
    let article = readability.parse().ok()?;
    let mut paragraphs = html_to_paragraphs(&article.content);
    if paragraphs.is_empty() {
        if let Some(excerpt_text) = article.excerpt.filter(|text| !text.is_empty()) {
            paragraphs.push(clean_text(&excerpt_text));
        } else {
            let text = clean_text(&article.text_content);
            if !text.is_empty() {
                paragraphs.push(text);
            }
        }
    }
    if paragraphs.is_empty() {
        return None;
    }

    looks_like_article(&paragraphs.join(" ")).then_some(paragraphs)
}

/// Some servers serve gzip bytes without saying so in the headers.
fn decode_body(bytes: &[u8]) -> Option<String> {
    let mut data = Vec::new();

    if bytes.starts_with(&[0x1f, 0x8b]) {
        let mut decoder = GzDecoder::new(bytes).take(MAX_DECODED_BYTES as u64 + 1);
        if decoder.read_to_end(&mut data).is_err() {
            return None;
        }
        if data.len() > MAX_DECODED_BYTES {
            return None;
        }
    } else {
        data.extend_from_slice(bytes);
    }

    Some(String::from_utf8_lossy(&data).into_owned())
}

fn read_limited(reader: impl Read, limit: usize) -> io::Result<Vec<u8>> {
    let mut limited = reader.take(limit as u64 + 1);
    let mut bytes = Vec::new();
    limited.read_to_end(&mut bytes)?;
    if bytes.len() > limit {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("response exceeds the {limit}-byte limit"),
        ));
    }
    Ok(bytes)
}

/// Extract article blocks in document order so full text keeps its paragraphs.
fn html_to_paragraphs(html: &str) -> Vec<String> {
    let document = Html::parse_fragment(html);
    let blocks = selector("p, h2, h3, li, blockquote");
    let mut paragraphs = Vec::new();

    for block in document.select(&blocks) {
        let text = element_text(block);
        if !text.is_empty() && paragraphs.last() != Some(&text) {
            paragraphs.push(text);
        }
    }

    if paragraphs.is_empty() {
        let text = clean_text(html);
        if !text.is_empty() {
            paragraphs.push(text);
        }
    }

    paragraphs
}

fn looks_like_article(text: &str) -> bool {
    let total = text.chars().count();
    if total < 60 {
        return false;
    }

    let lower = text.to_ascii_lowercase();
    let blocked = [
        "log in or sign up",
        "see what’s happening and join the conversation",
        "see what's happening and join the conversation",
        "continue with phone or log in",
        "log in with username or email",
        "enable javascript",
        "javascript is required",
        "javascript is disabled",
        "access denied",
        "403 forbidden",
        "verify you are human",
    ];
    if blocked.iter().any(|phrase| lower.contains(phrase)) {
        return false;
    }

    let broken = text.chars().filter(|ch| *ch == '\u{FFFD}').count();
    broken * 20 <= total
}

/// Pages that are apps or video rather than readable articles.
fn is_link_only_host(url: &str) -> bool {
    let Some(host) = host_of(url) else {
        return false;
    };
    let host = host.to_ascii_lowercase();
    let blocked = [
        "x.com",
        "twitter.com",
        "youtube.com",
        "youtu.be",
        "instagram.com",
        "facebook.com",
        "tiktok.com",
    ];

    blocked
        .iter()
        .any(|domain| host == *domain || host.ends_with(&format!(".{domain}")))
}

/// Only fetch public HTTP(S) URLs; never local services or local files.
fn is_fetchable(url: &str) -> bool {
    let Ok(parsed) = reqwest::Url::parse(url) else {
        return false;
    };
    if !matches!(parsed.scheme(), "https" | "http") {
        return false;
    }

    let Some(host) = parsed.host_str() else {
        return false;
    };
    let host = host
        .trim_start_matches('[')
        .trim_end_matches(']')
        .trim_end_matches('.')
        .to_ascii_lowercase();
    if host.is_empty()
        || host == "localhost"
        || host.ends_with(".localhost")
        || host.ends_with(".local")
        || host.ends_with(".internal")
    {
        return false;
    }

    match host.parse::<IpAddr>() {
        Ok(ip) => is_public_ip(ip),
        Err(_) => true,
    }
}

fn is_public_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(ip) => {
            let octets = ip.octets();
            !(ip.is_private()
                || ip.is_loopback()
                || ip.is_link_local()
                || ip.is_broadcast()
                || ip.is_documentation()
                || ip.is_unspecified()
                || ip.is_multicast()
                // Current and former special-purpose, non-public ranges.
                || octets[0] == 0
                || (octets[0] == 100 && (64..=127).contains(&octets[1]))
                || (octets[0] == 192 && octets[1] == 0 && octets[2] == 0)
                || (octets[0] == 192 && octets[1] == 88 && octets[2] == 99)
                || (octets[0] == 198 && matches!(octets[1], 18 | 19))
                || octets[0] >= 240)
        }
        IpAddr::V6(ip) => {
            if let Some(ip) = ip.to_ipv4() {
                return is_public_ip(IpAddr::V4(ip));
            }
            let segments = ip.segments();
            // Globally routable unicast currently lives in 2000::/3. Exclude
            // documentation, benchmarking, and transition mechanisms within it.
            (segments[0] & 0xe000) == 0x2000
                && !(ip.is_loopback()
                    || ip.is_unspecified()
                    || ip.is_multicast()
                    || ip.is_unique_local()
                    || ip.is_unicast_link_local()
                    || (segments[0] == 0x2001 && (segments[1] <= 0x01ff || segments[1] == 0x0db8))
                    || segments[0] == 0x2002
                    || (segments[0] == 0x3fff && (segments[1] & 0xf000) == 0))
        }
    }
}

fn fetch_github_trending(client: &Client, limit: usize, topic: Option<&str>) -> Result<Vec<Story>> {
    let url = github_trending_url(topic)?;
    let body = get_text(client, &url)?;
    let document = Html::parse_document(&body);

    let articles = selector("article.Box-row");
    let repo_link = selector("h2 a");
    let description = selector("p");
    let language = selector(r#"span[itemprop="programmingLanguage"]"#);
    let spans = selector("span");

    let mut stories = Vec::new();

    for article in document.select(&articles) {
        let Some(link) = article.select(&repo_link).next() else {
            continue;
        };
        let Some(href) = link.value().attr("href") else {
            continue;
        };

        let repo = href
            .split('?')
            .next()
            .unwrap_or(href)
            .trim()
            .trim_matches('/')
            .chars()
            .filter(|character| !character.is_control())
            .collect::<String>();
        if repo.is_empty() {
            continue;
        }

        let mut details = Vec::new();
        if let Some(language) = article
            .select(&language)
            .next()
            .map(element_text)
            .filter(|text| !text.is_empty())
        {
            details.push(language);
        }
        if let Some(stars) = article.select(&spans).map(element_text).find(|text| {
            text.contains("stars today")
                || text.contains("stars this week")
                || text.contains("stars this month")
        }) {
            details.push(stars);
        }

        stories.push(Story {
            title: repo.to_string(),
            url: format!("https://github.com/{repo}"),
            published: None,
            summary: article
                .select(&description)
                .next()
                .map(element_text)
                .filter(|text| !text.is_empty()),
            full: None,
            details,
            domain: None,
        });

        if stories.len() == limit {
            break;
        }
    }

    if stories.is_empty() {
        bail!("no repositories found (GitHub may have changed its page)");
    }

    Ok(stories)
}

fn selector(css: &str) -> Selector {
    Selector::parse(css).expect("valid hard-coded selector")
}

fn element_text(element: ElementRef<'_>) -> String {
    let text: String = element
        .text()
        .collect::<String>()
        .chars()
        .map(|ch| if ch.is_control() { ' ' } else { ch })
        .collect();

    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn get_text(client: &Client, url: &str) -> Result<String> {
    if !is_fetchable(url) {
        bail!("refusing to fetch a non-public URL: {url}");
    }
    let response = client
        .get(url)
        .send()
        .with_context(|| format!("request to {url} failed"))?;

    if !response.status().is_success() {
        bail!("{url} returned HTTP {}", response.status());
    }

    let bytes = read_limited(response, MAX_FEED_BYTES)
        .with_context(|| format!("could not read the response from {url}"))?;
    decode_body(&bytes).with_context(|| format!("could not decode the response from {url}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    const BBC_FEED: &str = r#"<?xml version="1.0"?>
<rss version="2.0"><channel><title>BBC News</title>
<item>
  <title>A BBC headline</title>
  <link>https://www.bbc.co.uk/news/articles/abc</link>
  <description>The US has not confirmed the Ukrainian president's comments.</description>
  <pubDate>Sat, 14 Nov 2026 20:52:00 GMT</pubDate>
</item>
</channel></rss>"#;

    #[test]
    fn feed_descriptions_become_article_text() {
        let stories = parse_feed(Source::Bbc, BBC_FEED, 8).unwrap();
        assert_eq!(
            stories[0].summary.as_deref(),
            Some("The US has not confirmed the Ukrainian president's comments.")
        );
        assert_eq!(stories[0].domain, None);
    }

    #[test]
    fn limits_are_applied() {
        let stories = parse_feed(Source::Bbc, BBC_FEED, 1).unwrap();
        assert_eq!(stories.len(), 1);
    }

    #[test]
    fn malformed_feeds_are_errors() {
        assert!(parse_feed(Source::Bbc, "definitely not xml", 5).is_err());
    }

    #[test]
    fn decodes_gzipped_pages() {
        use flate2::Compression;
        use flate2::write::GzEncoder;
        use std::io::Write as _;

        let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
        encoder.write_all(b"<html>hello</html>").unwrap();
        let compressed = encoder.finish().unwrap();

        assert_eq!(
            decode_body(&compressed).as_deref(),
            Some("<html>hello</html>")
        );
        assert_eq!(decode_body(&compressed[..compressed.len() - 2]), None);
    }

    #[test]
    fn rejects_login_walls_and_binary_text() {
        assert!(!looks_like_article(
            "See what’s happening and join the conversation Continue with phone or Log in with username or email"
        ));
        assert!(!looks_like_article(
            "Please enable JavaScript to view this page. Please enable JavaScript to view this page."
        ));
        assert!(looks_like_article(
            "This is a real article paragraph with enough words to be considered readable text."
        ));
        assert!(is_link_only_host("https://x.com/someone/status/1"));
        assert!(!is_link_only_host("https://example.com/story"));
    }

    #[test]
    fn parses_omarchy_releases_and_skips_drafts() {
        let value = serde_json::json!([
            {
                "tag_name": "v4.5.0",
                "name": "Omarchy 4.5",
                "html_url": "https://github.com/omacom/omarchy/releases/tag/v4.5.0",
                "published_at": "2026-11-05T05:36:53Z",
                "body": "First **paragraph** with [a link](https://example.com).\n\n## Second\n\n- Last",
                "draft": false,
                "prerelease": false
            },
            {
                "tag_name": "draft",
                "name": "Draft",
                "html_url": "https://github.com/omacom/omarchy/releases/tag/draft",
                "draft": true
            }
        ]);
        let stories = parse_omarchy_releases(&value, 8, false).unwrap();
        assert_eq!(stories.len(), 1);
        assert_eq!(stories[0].title, "Omarchy 4.5");
        assert_eq!(stories[0].details, vec!["tag v4.5.0"]);
        assert_eq!(
            stories[0].full.as_ref().unwrap(),
            &[
                "First paragraph with a link (https://example.com).",
                "Second",
                "Last"
            ]
        );
        assert!(parse_omarchy_releases(&serde_json::json!([]), 8, false).is_err());
    }

    #[test]
    fn bounds_response_reads() {
        let bytes = read_limited(&b"abcd"[..], 4).unwrap();
        assert_eq!(bytes, b"abcd");
        assert!(read_limited(&b"abcde"[..], 4).is_err());
    }

    #[test]
    fn only_fetches_public_http_urls() {
        assert!(is_fetchable("https://example.com/story"));
        assert!(is_fetchable("http://example.com/story"));
        assert!(!is_fetchable("file:///etc/passwd"));
        assert!(!is_fetchable("ftp://example.com/file"));
        assert!(!is_fetchable("http://localhost:8080/admin"));
        assert!(!is_fetchable("http://127.0.0.1/admin"));
        assert!(!is_fetchable("http://10.0.0.1/admin"));
        assert!(!is_fetchable("http://[::1]/admin"));
        assert!(!is_fetchable("http://[::ffff:127.0.0.1]/admin"));
        assert!(!is_fetchable("http://169.254.169.254/latest/meta-data/"));
        assert!(!is_fetchable("http://printer.local/admin"));
        assert!(!is_fetchable("http://100.64.0.1/"));
        assert!(!is_fetchable("http://198.18.0.1/"));
        assert!(!is_fetchable("http://240.0.0.1/"));
        assert!(!is_fetchable("http://[2001:db8::1]/"));
        assert!(!is_fetchable("http://0.0.0.1/"));
        assert!(!is_fetchable("http://192.88.99.1/"));
        assert!(!is_fetchable("http://2130706433/"));
        assert!(!is_fetchable("http://0x7f000001/"));
        assert!(!is_fetchable("http://127.0.0.1\\@example.com/"));
        assert!(!is_fetchable("http://[::127.0.0.1]/"));
        assert!(!is_fetchable("http://[64:ff9b::127.0.0.1]/"));
        assert!(!is_fetchable("http://[fec0::1]/"));
        assert!(!is_fetchable("http://[2001:2::1]/"));
        assert!(!is_fetchable("http://[2002:7f00:1::]/"));
        assert!(is_fetchable("https://8.8.8.8/"));
        assert!(is_fetchable("https://[2606:4700:4700::1111]/"));
        assert!(is_fetchable("http://example.com\\@127.0.0.1/"));
    }

    #[test]
    fn uses_paragraph_text_when_available() {
        assert_eq!(
            html_to_paragraphs("<div><p>One.</p><p>Two.</p></div>"),
            vec!["One.", "Two."]
        );
    }

    #[test]
    fn removes_terminal_controls_from_scraped_text() {
        let document = Html::parse_fragment("<p>safe&#27;[31mtext</p>");
        let paragraph = selector("p");
        let text = element_text(document.select(&paragraph).next().unwrap());
        assert_eq!(text, "safe [31mtext");
    }
}
