use std::io::Read;
use std::net::IpAddr;
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

use crate::source::{Source, github_trending_url};
use crate::story::{Story, clean_text, excerpt, hn_details, host_of, parse_date};

const MAX_FEED_BYTES: usize = 4 * 1024 * 1024;
const MAX_PAGE_BYTES: usize = 4 * 1024 * 1024;
const MAX_DECODED_BYTES: usize = 8 * 1024 * 1024;

/// Shared HTTP client: bounded redirects, no local destinations.
pub fn client(timeout: Duration) -> Result<Client> {
    Client::builder()
        .user_agent(format!(
            "newspaper/{} (terminal newspaper)",
            env!("CARGO_PKG_VERSION")
        ))
        .timeout(timeout)
        .connect_timeout(Duration::from_secs(5))
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

/// Fetch the selected source and topic.
pub fn fetch(
    source: Source,
    topic: Option<&str>,
    client: &Client,
    limit: usize,
) -> Result<Vec<Story>> {
    if source == Source::Github {
        return fetch_github_trending(client, limit, topic);
    }

    let url = source.feed_url(topic)?;
    let body = get_text(client, &url)?;
    let mut stories = parse_feed(source, &body, limit)?;

    if source != Source::Github {
        enrich_summaries(&mut stories, client);
    }

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

    match source {
        Source::Bbc | Source::Omarchy => {
            summary = item
                .description()
                .and_then(|description| excerpt(description, 900))
                .or_else(|| item.content().and_then(|content| excerpt(content, 900)));
        }
        Source::Hn => details = hn_details(item.description().unwrap_or_default()),
        _ => {}
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
        details,
        domain,
    })
}

/// Fetch article text for stories whose feed did not include a summary.
/// Bounded to eight concurrent requests per source.
fn enrich_summaries(stories: &mut [Story], client: &Client) {
    for chunk in stories.chunks_mut(8) {
        thread::scope(|scope| {
            let mut handles = Vec::new();
            for story in chunk.iter_mut() {
                if story.summary.is_some() {
                    continue;
                }
                handles.push(scope.spawn(move || {
                    story.summary = fetch_article_summary(client, &story.url);
                }));
            }
            // A parser panic should lose one summary, not the whole source.
            for handle in handles {
                let _ = handle.join();
            }
        });
    }
}

fn fetch_article_summary(client: &Client, url: &str) -> Option<String> {
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
    let content = html_to_text(&article.content);
    let summary = excerpt(&content, 700)?;

    looks_like_article(&summary).then_some(summary)
}

/// Some servers serve gzip bytes without saying so in the headers.
fn decode_body(bytes: &[u8]) -> Option<String> {
    let mut data = Vec::new();

    if bytes.starts_with(&[0x1f, 0x8b]) {
        let mut decoder = GzDecoder::new(bytes).take(MAX_DECODED_BYTES as u64);
        if decoder.read_to_end(&mut data).is_err() && data.is_empty() {
            return None;
        }
    } else {
        data.extend_from_slice(bytes);
    }

    Some(String::from_utf8_lossy(&data).into_owned())
}

fn read_limited(reader: impl Read, limit: usize) -> std::io::Result<Vec<u8>> {
    let mut limited = reader.take(limit as u64);
    let mut bytes = Vec::new();
    limited.read_to_end(&mut bytes)?;
    Ok(bytes)
}

/// Prefer paragraph text from readability's HTML so words are not glued together.
fn html_to_text(html: &str) -> String {
    let document = Html::parse_fragment(html);
    let paragraphs = selector("p");
    let text: Vec<String> = document
        .select(&paragraphs)
        .map(element_text)
        .filter(|text| !text.is_empty())
        .collect();

    if text.is_empty() {
        clean_text(html)
    } else {
        text.join(" ")
    }
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
    let lower = url.to_ascii_lowercase();
    if !(lower.starts_with("https://") || lower.starts_with("http://")) {
        return false;
    }

    let Some(host) = host_of(url) else {
        return false;
    };
    let host = host.trim_end_matches('.').to_ascii_lowercase();
    if host == "localhost"
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
            !(ip.is_private()
                || ip.is_loopback()
                || ip.is_link_local()
                || ip.is_broadcast()
                || ip.is_documentation()
                || ip.is_unspecified()
                || ip.is_multicast())
        }
        IpAddr::V6(ip) => {
            if let Some(ip) = ip.to_ipv4_mapped() {
                return is_public_ip(IpAddr::V4(ip));
            }
            !(ip.is_loopback()
                || ip.is_unspecified()
                || ip.is_multicast()
                || ip.is_unique_local()
                || ip.is_unicast_link_local())
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
            .trim_matches('/');
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
    let response = client
        .get(url)
        .send()
        .with_context(|| format!("request to {url} failed"))?;

    if !response.status().is_success() {
        bail!("{url} returned HTTP {}", response.status());
    }

    let bytes = read_limited(response, MAX_FEED_BYTES)
        .with_context(|| format!("could not read the response from {url}"))?;
    Ok(String::from_utf8_lossy(&bytes).into_owned())
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
    fn bounds_response_reads() {
        let bytes = read_limited(&b"abcdefghij"[..], 4).unwrap();
        assert_eq!(bytes, b"abcd");
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
    }

    #[test]
    fn uses_paragraph_text_when_available() {
        assert_eq!(
            html_to_text("<div><p>One.</p><p>Two.</p></div>"),
            "One. Two."
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
