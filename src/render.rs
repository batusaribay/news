use std::io::{self, Write};

use chrono::{DateTime, Utc};
use textwrap::wrap;

use crate::source::Source;
use crate::story::{Story, relative_time};

/// A plain-text digest. No banners, rules, numbering, or footers.
pub struct Renderer<W: Write> {
    out: W,
    width: usize,
    color: bool,
    hyperlinks: bool,
    links: bool,
    printed: bool,
}

impl<W: Write> Renderer<W> {
    pub fn new(out: W, width: usize, color: bool, hyperlinks: bool, links: bool) -> Self {
        Self {
            out,
            width,
            color,
            hyperlinks,
            links,
            printed: false,
        }
    }

    pub fn print_story(
        &mut self,
        source: Source,
        topic: Option<&str>,
        story: &Story,
        now: DateTime<Utc>,
    ) -> io::Result<()> {
        if self.printed {
            writeln!(self.out)?;
        } else {
            self.printed = true;
        }

        for line in wrap(&story.title, self.width) {
            let title = self.headline(&line, &story.url);
            writeln!(self.out, "{title}")?;
        }

        let mut metadata = vec![source.label().to_string()];
        if let Some(topic) = topic.map(str::trim).filter(|topic| !topic.is_empty()) {
            metadata.push(topic.to_ascii_lowercase());
        }
        if let Some(published) = story.published {
            metadata.push(relative_time(published, now));
        }
        metadata.extend(story.details.iter().cloned());
        if let Some(domain) = &story.domain {
            metadata.push(domain.clone());
        }
        self.print_metadata(&metadata.join(" · "))?;

        if self.links && !story.url.is_empty() {
            let url: String = story.url.chars().filter(|ch| !ch.is_control()).collect();
            let url = self.paint("2", &url);
            writeln!(self.out, "{url}")?;
        }

        if let Some(summary) = &story.summary {
            for line in wrap(summary, self.width) {
                writeln!(self.out, "{line}")?;
            }
        }

        Ok(())
    }

    /// Wrap metadata, keeping separators from dangling at line breaks.
    fn print_metadata(&mut self, text: &str) -> io::Result<()> {
        for line in wrap(text, self.width) {
            let line = line.trim();
            let line = line.strip_prefix("· ").unwrap_or(line);
            let line = line.strip_suffix(" ·").unwrap_or(line);
            let line = self.paint("2", line);
            writeln!(self.out, "{line}")?;
        }
        Ok(())
    }

    pub fn flush(&mut self) -> io::Result<()> {
        self.out.flush()
    }

    fn headline(&self, text: &str, url: &str) -> String {
        let styled = self.paint("1", text);
        if self.hyperlinks && !url.is_empty() {
            let url: String = url.chars().filter(|ch| !ch.is_control()).collect();
            format!("\x1b]8;;{url}\x1b\\{styled}\x1b]8;;\x1b\\")
        } else {
            styled
        }
    }

    fn paint(&self, code: &str, text: &str) -> String {
        if self.color && !text.is_empty() {
            format!("\x1b[{code}m{text}\x1b[0m")
        } else {
            text.to_string()
        }
    }
}

/// Pick a readable width, capped so very wide terminals do not produce long lines.
pub fn terminal_width() -> usize {
    terminal_size::terminal_size()
        .map(|(width, _)| width.0 as usize)
        .or_else(|| {
            std::env::var("COLUMNS")
                .ok()
                .and_then(|columns| columns.parse().ok())
        })
        .filter(|width| *width > 0)
        .unwrap_or(100)
        .clamp(40, 100)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn sample_story(title: &str, summary: &str) -> Story {
        Story {
            title: title.to_string(),
            url: "https://example.com/story".to_string(),
            published: Some(Utc.with_ymd_and_hms(2026, 11, 15, 10, 0, 0).unwrap()),
            summary: Some(summary.to_string()),
            details: vec!["42 points".to_string()],
            domain: Some("example.com".to_string()),
        }
    }

    fn render(stories: &[Story], color: bool, hyperlinks: bool, links: bool) -> String {
        let now = Utc.with_ymd_and_hms(2026, 11, 15, 12, 0, 0).unwrap();
        let mut buffer = Vec::new();
        {
            let mut renderer = Renderer::new(&mut buffer, 60, color, hyperlinks, links);
            for story in stories {
                renderer.print_story(Source::Hn, None, story, now).unwrap();
            }
            renderer.flush().unwrap();
        }
        String::from_utf8(buffer).unwrap()
    }

    #[test]
    fn prints_text_without_decoration() {
        let stories = vec![
            sample_story("First headline", "First article text."),
            sample_story("Second headline", "Second article text."),
        ];
        let output = render(&stories, false, false, true);

        assert!(output.starts_with("First headline\n"));
        assert!(output.contains("Hacker News · 2h ago · 42 points · example.com"));
        assert!(output.contains("https://example.com/story"));
        assert!(output.contains("First article text."));
        assert!(output.contains("\nSecond headline\n"));
        assert!(!output.contains("──"));
        assert!(!output.contains("news-scraper"));
        assert!(!output.contains("stories"));
        assert!(!output.contains("1."));
        assert!(!output.contains("\x1b["));
    }

    #[test]
    fn links_can_be_hidden() {
        let story = sample_story("Headline", "Text.");

        let plain = render(std::slice::from_ref(&story), false, false, false);
        assert!(!plain.contains("https://example.com/story"));

        let linked = render(&[story], false, false, true);
        assert!(linked.contains("https://example.com/story"));
    }

    #[test]
    fn prints_the_selected_topic_in_the_source_line() {
        let now = Utc.with_ymd_and_hms(2026, 11, 15, 12, 0, 0).unwrap();
        let mut buffer = Vec::new();
        {
            let mut renderer = Renderer::new(&mut buffer, 60, false, false, true);
            renderer
                .print_story(
                    Source::Bbc,
                    Some("Sport"),
                    &sample_story("Headline", "Text."),
                    now,
                )
                .unwrap();
        }
        let output = String::from_utf8(buffer).unwrap();
        assert!(output.contains("BBC News · sport · 2h ago"));
    }

    #[test]
    fn metadata_breaks_without_dangling_separators() {
        let story = Story {
            title: "Short headline".to_string(),
            url: "https://apps.apple.com/app".to_string(),
            published: Some(Utc.with_ymd_and_hms(2026, 11, 15, 11, 0, 0).unwrap()),
            summary: None,
            details: vec!["5 points".to_string(), "1 comment".to_string()],
            domain: Some("apps.apple.com".to_string()),
        };

        let mut buffer = Vec::new();
        {
            let mut renderer = Renderer::new(&mut buffer, 34, false, false, false);
            renderer
                .print_story(Source::Hn, None, &story, Utc::now())
                .unwrap();
        }
        let output = String::from_utf8(buffer).unwrap();

        assert!(output.contains("apps.apple.com"));
        for line in output.lines() {
            assert!(
                !line.trim_end().ends_with('·'),
                "dangling separator: {line:?}"
            );
        }
    }

    #[test]
    fn colour_and_hyperlinks_are_opt_in_escapes() {
        let output = render(&[sample_story("Headline", "Text.")], true, true, true);

        assert!(output.contains("\x1b[1mHeadline\x1b[0m"));
        assert!(output.contains("\x1b[2mHacker News · 2h ago · 42 points · example.com\x1b[0m"));
        assert!(output.contains("\x1b[2mhttps://example.com/story\x1b[0m"));
        assert!(output.contains("\x1b]8;;https://example.com/story\x1b\\"));
        assert!(!output.contains("\x1b[34m"));
        assert!(!output.contains("\x1b[1;33m"));
    }
}
