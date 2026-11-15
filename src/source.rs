use anyhow::{Result, bail};
use clap::ValueEnum;

/// A news source supported by the reader.
#[derive(Clone, Copy, Debug, PartialEq, Eq, ValueEnum)]
pub enum Source {
    #[value(name = "bbc")]
    Bbc,
    #[value(name = "google")]
    Google,
    #[value(name = "lobsters")]
    Lobsters,
    #[value(name = "omarchy")]
    Omarchy,
    #[value(name = "github")]
    Github,
    #[value(name = "hackernews")]
    Hn,
}

impl Source {
    pub fn label(self) -> &'static str {
        match self {
            Source::Bbc => "BBC News",
            Source::Google => "Google News",
            Source::Lobsters => "Lobsters",
            Source::Omarchy => "Omarchy News",
            Source::Github => "GitHub Trending",
            Source::Hn => "Hacker News",
        }
    }

    /// RSS/Atom feed for the source and topic.
    pub fn feed_url(self, topic: Option<&str>) -> Result<String> {
        match self {
            Source::Bbc => bbc_feed(topic),
            Source::Google => google_feed(topic),
            Source::Lobsters => lobsters_feed(topic),
            Source::Omarchy => {
                if topic.is_some() {
                    bail!("Omarchy News has no topics");
                }
                Ok("https://omarchy.org/news/rss.xml".to_string())
            }
            Source::Hn => hn_feed(topic),
            Source::Github => bail!("GitHub Trending is not an RSS source"),
        }
    }
}

/// GitHub's trending page range.
pub fn github_trending_url(topic: Option<&str>) -> Result<String> {
    let topic = normalize(topic);
    let since = match topic.as_deref() {
        None | Some("daily") | Some("day") | Some("today") => "daily",
        Some("weekly") | Some("week") => "weekly",
        Some("monthly") | Some("month") => "monthly",
        Some(other) => {
            bail!("unknown GitHub range '{other}'; use: daily, weekly, monthly")
        }
    };

    Ok(format!("https://github.com/trending?since={since}"))
}

fn bbc_feed(topic: Option<&str>) -> Result<String> {
    let topic = normalize(topic);
    let url = match topic.as_deref() {
        None | Some("top") | Some("news") | Some("headlines") => {
            "https://feeds.bbci.co.uk/news/rss.xml"
        }
        Some("world") => "https://feeds.bbci.co.uk/news/world/rss.xml",
        Some("uk") => "https://feeds.bbci.co.uk/news/uk/rss.xml",
        Some("business") => "https://feeds.bbci.co.uk/news/business/rss.xml",
        Some("politics") => "https://feeds.bbci.co.uk/news/politics/rss.xml",
        Some("health") => "https://feeds.bbci.co.uk/news/health/rss.xml",
        Some("education") => "https://feeds.bbci.co.uk/news/education/rss.xml",
        Some("science") | Some("environment") | Some("science-and-environment") => {
            "https://feeds.bbci.co.uk/news/science_and_environment/rss.xml"
        }
        Some("technology") | Some("tech") => "https://feeds.bbci.co.uk/news/technology/rss.xml",
        Some("entertainment") | Some("arts") => {
            "https://feeds.bbci.co.uk/news/entertainment_and_arts/rss.xml"
        }
        Some("sport") | Some("sports") => "https://feeds.bbci.co.uk/sport/rss.xml",
        Some("football") => "https://feeds.bbci.co.uk/sport/football/rss.xml",
        Some(other) => bail!(
            "unknown BBC topic '{other}'; use: top, world, uk, business, politics, health, education, science, technology, entertainment, sport, football"
        ),
    };

    Ok(url.to_string())
}

fn google_feed(topic: Option<&str>) -> Result<String> {
    let topic = normalize(topic);
    let section = match topic.as_deref() {
        None | Some("top") | Some("news") | Some("headlines") => {
            return Ok("https://news.google.com/rss?hl=en-US&gl=US&ceid=US:en".to_string());
        }
        Some("world") => "WORLD",
        Some("nation") | Some("national") => "NATION",
        Some("business") => "BUSINESS",
        Some("technology") | Some("tech") => "TECHNOLOGY",
        Some("entertainment") => "ENTERTAINMENT",
        Some("sports") | Some("sport") => "SPORTS",
        Some("science") => "SCIENCE",
        Some("health") => "HEALTH",
        Some(other) => bail!(
            "unknown Google News topic '{other}'; use: top, world, nation, business, technology, entertainment, sports, science, health"
        ),
    };

    Ok(format!(
        "https://news.google.com/rss/headlines/section/topic/{section}?hl=en-US&gl=US&ceid=US:en"
    ))
}

fn hn_feed(topic: Option<&str>) -> Result<String> {
    let topic = normalize(topic);
    let path = match topic.as_deref() {
        None | Some("frontpage") | Some("top") | Some("front") => "frontpage",
        Some("newest") | Some("new") => "newest",
        Some("ask") => "ask",
        Some("show") => "show",
        Some("jobs") => "jobs",
        Some("best") => "best",
        Some(other) => bail!(
            "unknown Hacker News topic '{other}'; use: frontpage, newest, ask, show, jobs, best"
        ),
    };

    Ok(format!("https://hnrss.org/{path}"))
}

fn lobsters_feed(topic: Option<&str>) -> Result<String> {
    let Some(tag) = normalize(topic) else {
        return Ok("https://lobste.rs/rss".to_string());
    };

    if !tag
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || ch == '-' || ch == '_')
    {
        bail!("invalid Lobsters tag '{tag}'; use letters, numbers, '-' or '_'");
    }

    Ok(format!("https://lobste.rs/t/{tag}.rss"))
}

fn normalize(topic: Option<&str>) -> Option<String> {
    topic
        .map(str::trim)
        .filter(|topic| !topic.is_empty())
        .map(str::to_ascii_lowercase)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hackernews_uses_its_full_name() {
        assert_eq!(
            Source::Hn.to_possible_value().unwrap().get_name(),
            "hackernews"
        );
    }

    #[test]
    fn builds_default_feeds() {
        assert_eq!(
            Source::Bbc.feed_url(None).unwrap(),
            "https://feeds.bbci.co.uk/news/rss.xml"
        );
        assert_eq!(
            Source::Google.feed_url(None).unwrap(),
            "https://news.google.com/rss?hl=en-US&gl=US&ceid=US:en"
        );
        assert_eq!(
            Source::Lobsters.feed_url(None).unwrap(),
            "https://lobste.rs/rss"
        );
        assert_eq!(
            Source::Hn.feed_url(None).unwrap(),
            "https://hnrss.org/frontpage"
        );
    }

    #[test]
    fn builds_topic_feeds() {
        assert_eq!(
            Source::Bbc.feed_url(Some("Sport")).unwrap(),
            "https://feeds.bbci.co.uk/sport/rss.xml"
        );
        assert_eq!(
            Source::Google.feed_url(Some("business")).unwrap(),
            "https://news.google.com/rss/headlines/section/topic/BUSINESS?hl=en-US&gl=US&ceid=US:en"
        );
        assert_eq!(
            Source::Hn.feed_url(Some("ask")).unwrap(),
            "https://hnrss.org/ask"
        );
        assert_eq!(
            Source::Lobsters.feed_url(Some("rust")).unwrap(),
            "https://lobste.rs/t/rust.rss"
        );
        assert_eq!(
            github_trending_url(Some("monthly")).unwrap(),
            "https://github.com/trending?since=monthly"
        );
    }

    #[test]
    fn rejects_unknown_topics() {
        assert!(Source::Bbc.feed_url(Some("gossip")).is_err());
        assert!(Source::Google.feed_url(Some("gossip")).is_err());
        assert!(Source::Hn.feed_url(Some("gossip")).is_err());
        assert!(Source::Omarchy.feed_url(Some("sport")).is_err());
        assert!(Source::Lobsters.feed_url(Some("rust lang")).is_err());
        assert!(github_trending_url(Some("yearly")).is_err());
    }
}
