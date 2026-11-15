use chrono::{DateTime, Datelike, Utc};

/// A single story ready for rendering.
#[derive(Clone, Debug)]
pub struct Story {
    pub title: String,
    pub url: String,
    pub published: Option<DateTime<Utc>>,
    /// Article text or a short summary, when one is available.
    pub summary: Option<String>,
    /// Extra facts about a story, such as "95 points" or "412 stars today".
    pub details: Vec<String>,
    /// Site the story links to, when it is worth showing.
    pub domain: Option<String>,
}

/// Strip tags, decode entities, and collapse whitespace in feed text.
pub fn clean_text(input: &str) -> String {
    let without_tags = strip_tags(input);
    let decoded = decode_entities(&without_tags);
    let printable: String = decoded
        .chars()
        .map(|ch| if ch.is_control() { ' ' } else { ch })
        .collect();
    printable.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn strip_tags(input: &str) -> String {
    let mut output = String::with_capacity(input.len());
    let mut inside_tag = false;

    for ch in input.chars() {
        match ch {
            '<' => {
                inside_tag = true;
                output.push(' ');
            }
            '>' if inside_tag => inside_tag = false,
            _ if !inside_tag => output.push(ch),
            _ => {}
        }
    }

    output
}

fn decode_entities(input: &str) -> String {
    let mut output = String::with_capacity(input.len());
    let mut rest = input;

    while let Some(start) = rest.find('&') {
        output.push_str(&rest[..start]);
        let tail = &rest[start + 1..];

        let decoded = tail
            .find(';')
            .filter(|end| *end <= 10)
            .and_then(|end| decode_entity(&tail[..end]).map(|ch| (ch, end)));

        if let Some((ch, end)) = decoded {
            output.push(ch);
            rest = &tail[end + 1..];
        } else {
            output.push('&');
            rest = tail;
        }
    }

    output.push_str(rest);
    output
}

fn decode_entity(entity: &str) -> Option<char> {
    let named = match entity {
        "amp" => Some('&'),
        "lt" => Some('<'),
        "gt" => Some('>'),
        "quot" => Some('"'),
        "apos" => Some('\''),
        "nbsp" => Some(' '),
        _ => None,
    };
    if named.is_some() {
        return named;
    }

    let numeric = entity
        .strip_prefix("#x")
        .or_else(|| entity.strip_prefix("#X"))
        .and_then(|hex| u32::from_str_radix(hex, 16).ok())
        .or_else(|| {
            entity
                .strip_prefix('#')
                .and_then(|dec| dec.parse::<u32>().ok())
        })?;

    char::from_u32(numeric)
}

/// Turn article HTML/text into a compact, readable excerpt.
pub fn excerpt(input: &str, max_chars: usize) -> Option<String> {
    if max_chars == 0 {
        return None;
    }

    let text = clean_text(input);
    if text.chars().count() <= max_chars {
        return (!text.is_empty()).then_some(text);
    }

    let cut: String = text.chars().take(max_chars).collect();
    let sentence_end = cut.rfind(['.', '!', '?']);

    let body = match sentence_end {
        // Only use a sentence boundary if it is not too close to the start.
        Some(position) if position * 10 >= max_chars * 6 => cut[..=position].trim_end(),
        _ => {
            let boundary = cut.rfind(char::is_whitespace).unwrap_or(cut.len());
            cut[..boundary].trim_end()
        }
    };

    (!body.is_empty()).then(|| format!("{body}…"))
}

/// Parse the timestamp formats found in the supported feeds.
pub fn parse_date(value: Option<&str>) -> Option<DateTime<Utc>> {
    let value = value?.trim();
    DateTime::parse_from_rfc2822(value)
        .or_else(|_| DateTime::parse_from_rfc3339(value))
        .ok()
        .map(|date| date.with_timezone(&Utc))
}

/// Format a publication time relative to now, falling back to a short date.
pub fn relative_time(published: DateTime<Utc>, now: DateTime<Utc>) -> String {
    let seconds = now.signed_duration_since(published).num_seconds();

    if seconds < 60 {
        return "just now".to_string();
    }
    if seconds < 3_600 {
        return format!("{}m ago", seconds / 60);
    }
    if seconds < 86_400 {
        return format!("{}h ago", seconds / 3_600);
    }

    let days = seconds / 86_400;
    if days < 7 {
        return format!("{days}d ago");
    }

    let short = format!("{} {}", published.format("%b"), published.day());
    if published.year() == now.year() {
        short
    } else {
        format!("{short}, {}", published.year())
    }
}

/// Extract the score and comment count hnrss puts in the description.
pub fn hn_details(description: &str) -> Vec<String> {
    let text = clean_text(description);
    let mut details = Vec::new();

    if let Some(points) = number_after(&text, "Points: ") {
        details.push(format!(
            "{points} {}",
            if points == 1 { "point" } else { "points" }
        ));
    }
    if let Some(comments) = number_after(&text, "# Comments: ") {
        details.push(format!(
            "{comments} {}",
            if comments == 1 { "comment" } else { "comments" }
        ));
    }

    details
}

fn number_after(text: &str, key: &str) -> Option<u64> {
    let rest = text.split_once(key)?.1.trim_start();
    let digits: String = rest
        .chars()
        .take_while(|ch| ch.is_ascii_digit() || *ch == ',')
        .filter(|ch| *ch != ',')
        .collect();
    digits.parse().ok()
}

/// Turn a URL into a compact host name for display.
pub fn host_of(url: &str) -> Option<String> {
    let rest = url.split_once("://").map(|(_, rest)| rest).unwrap_or(url);
    let authority = rest.split(['/', '?', '#']).next()?.rsplit('@').next()?;
    let host = if authority.starts_with('[') {
        authority.split(']').next()?.trim_start_matches('[')
    } else {
        authority.split(':').next()?
    };
    let host = host.strip_prefix("www.").unwrap_or(host);

    (!host.is_empty()
        && !host.chars().any(char::is_whitespace)
        && !host.chars().any(char::is_control))
    .then(|| host.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    #[test]
    fn cleans_html_and_entities() {
        assert_eq!(
            clean_text("<p>Hello &amp; welcome</p>\n  to <b>Lobsters</b>"),
            "Hello & welcome to Lobsters"
        );
        assert_eq!(clean_text("caf&#233; &#x2014; open"), "café — open");
        assert_eq!(clean_text("safe\u{1b}[31mred"), "safe [31mred");
    }

    #[test]
    fn reads_hacker_news_details() {
        let description = "<p>Points: 95</p><p># Comments: 1</p>";
        assert_eq!(hn_details(description), vec!["95 points", "1 comment"]);
    }

    #[test]
    fn excerpts_stop_at_a_sentence_or_word() {
        let short = "A short summary.";
        assert_eq!(excerpt(short, 100).as_deref(), Some(short));

        let long = "First sentence is here. Second sentence goes on and on and on.";
        assert_eq!(
            excerpt(long, 30).as_deref(),
            Some("First sentence is here.…")
        );

        let unpunctuated = "one two three four five six seven eight nine ten";
        assert_eq!(
            excerpt(unpunctuated, 20).as_deref(),
            Some("one two three four…")
        );
        assert_eq!(excerpt("   ", 20), None);
        assert_eq!(excerpt("something", 0), None);
    }

    #[test]
    fn extracts_hosts() {
        assert_eq!(
            host_of("https://www.bbc.co.uk/news/articles/1").as_deref(),
            Some("bbc.co.uk")
        );
        assert_eq!(
            host_of("https://news.ycombinator.com/item?id=1").as_deref(),
            Some("news.ycombinator.com")
        );
        assert_eq!(host_of("https://exa\u{1b}mple.com/x"), None);
        assert_eq!(host_of("not a url"), None);
    }

    #[test]
    fn formats_relative_times() {
        let now = Utc.with_ymd_and_hms(2026, 11, 15, 12, 0, 0).unwrap();
        assert_eq!(
            relative_time(now - chrono::Duration::seconds(30), now),
            "just now"
        );
        assert_eq!(
            relative_time(now - chrono::Duration::minutes(5), now),
            "5m ago"
        );
        assert_eq!(
            relative_time(now - chrono::Duration::hours(3), now),
            "3h ago"
        );
        assert_eq!(
            relative_time(now - chrono::Duration::days(2), now),
            "2d ago"
        );
        assert_eq!(
            relative_time(Utc.with_ymd_and_hms(2026, 9, 3, 12, 0, 0).unwrap(), now),
            "Sep 3"
        );
        assert_eq!(
            relative_time(Utc.with_ymd_and_hms(2025, 9, 3, 12, 0, 0).unwrap(), now),
            "Sep 3, 2025"
        );
    }
}
