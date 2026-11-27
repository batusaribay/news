use std::io::{self, Write};
use std::thread::sleep;
use std::time::Duration;

use chrono::{DateTime, Utc};
use textwrap::wrap;

use crate::source::Source;
use crate::story::{Story, relative_time};
use crate::theme::{Role, Theme};

/// A plain-text digest. No banners, rules, numbering, or footers.
pub struct Renderer<W: Write> {
    out: W,
    width: usize,
    links: bool,
    theme: Theme,
    printed: bool,
    delay: Option<Duration>,
}

impl<W: Write> Renderer<W> {
    pub fn new(out: W, width: usize, links: bool, theme: Theme, cpm: Option<u32>) -> Self {
        let delay = cpm.map(|cpm| Duration::from_millis((60_000 / u64::from(cpm)).max(1)));
        Self {
            out,
            width,
            links,
            theme,
            printed: false,
            delay,
        }
    }

    pub fn print_story(
        &mut self,
        source: Source,
        topic: Option<&str>,
        story: &Story,
        now: DateTime<Utc>,
        summary_only: bool,
    ) -> io::Result<()> {
        if self.printed {
            writeln!(self.out)?;
        } else {
            self.printed = true;
        }

        for line in wrap(&story.title, self.width) {
            writeln!(self.out, "{}", self.theme.paint(Role::Heading, &line))?;
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
            writeln!(self.out, "{}", self.theme.paint(Role::Muted, &url))?;
        }

        let full = if summary_only {
            None
        } else {
            story.full.as_deref()
        };

        if let Some(paragraphs) = full.filter(|paragraphs| !paragraphs.is_empty()) {
            for (index, paragraph) in paragraphs.iter().enumerate() {
                if index > 0 {
                    writeln!(self.out)?;
                }
                for line in wrap(paragraph, self.width) {
                    self.print_reading(&line)?;
                    writeln!(self.out)?;
                }
            }
        } else if let Some(summary) = &story.summary {
            for line in wrap(summary, self.width) {
                self.print_reading(&line)?;
                writeln!(self.out)?;
            }
        }

        Ok(())
    }

    fn print_reading(&mut self, line: &str) -> io::Result<()> {
        let Some(delay) = self.delay else {
            return write!(self.out, "{line}");
        };
        for character in line.chars() {
            write!(self.out, "{character}")?;
            self.out.flush()?;
            sleep(delay);
        }
        Ok(())
    }

    /// Wrap metadata, keeping separators from dangling at line breaks.
    fn print_metadata(&mut self, text: &str) -> io::Result<()> {
        for line in wrap(text, self.width) {
            let line = line.trim();
            let line = line.strip_prefix("· ").unwrap_or(line);
            let line = line.strip_suffix(" ·").unwrap_or(line);
            writeln!(self.out, "{}", self.theme.paint(Role::Muted, line))?;
        }
        Ok(())
    }

    pub fn flush(&mut self) -> io::Result<()> {
        self.out.flush()
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
            full: None,
            details: vec!["42 points".to_string()],
            domain: Some("example.com".to_string()),
        }
    }

    fn render(stories: &[Story], links: bool) -> String {
        let now = Utc.with_ymd_and_hms(2026, 11, 15, 12, 0, 0).unwrap();
        let mut buffer = Vec::new();
        {
            let mut renderer = Renderer::new(&mut buffer, 60, links, Theme::plain(), None);
            for story in stories {
                renderer
                    .print_story(Source::Hn, None, story, now, false)
                    .unwrap();
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
        let output = render(&stories, true);

        assert!(output.starts_with("First headline\n"));
        assert!(output.contains("Hacker News · 2h ago · 42 points · example.com"));
        assert!(output.contains("https://example.com/story"));
        assert!(output.contains("First article text."));
        assert!(output.contains("\nSecond headline\n"));
        assert!(!output.contains("──"));
        assert!(!output.contains("news"));
        assert!(!output.contains("stories"));
        assert!(!output.contains("1."));
        assert!(!output.contains("\x1b["));
    }

    #[test]
    fn links_can_be_hidden() {
        let story = sample_story("Headline", "Text.");

        let plain = render(std::slice::from_ref(&story), false);
        assert!(!plain.contains("https://example.com/story"));

        let linked = render(&[story], true);
        assert!(linked.contains("https://example.com/story"));
    }

    #[test]
    fn prints_full_article_paragraphs_by_default() {
        let mut story = sample_story("Headline", "Short summary.");
        story.full = Some(vec![
            "First paragraph.".to_string(),
            "Second paragraph.".to_string(),
        ]);

        let output = render(&[story], true);
        assert!(output.contains("First paragraph."));
        assert!(output.contains("\n\nSecond paragraph."));
        assert!(!output.contains("Short summary."));
    }

    #[test]
    fn summary_mode_uses_the_short_summary() {
        let now = Utc.with_ymd_and_hms(2026, 11, 15, 12, 0, 0).unwrap();
        let mut story = sample_story("Headline", "Short summary.");
        story.full = Some(vec!["Full paragraph.".to_string()]);

        let mut buffer = Vec::new();
        {
            let mut renderer = Renderer::new(&mut buffer, 60, true, Theme::plain(), None);
            renderer
                .print_story(Source::Hn, None, &story, now, true)
                .unwrap();
        }
        let output = String::from_utf8(buffer).unwrap();

        assert!(output.contains("Short summary."));
        assert!(!output.contains("Full paragraph."));
    }

    #[test]
    fn slow_mode_keeps_the_text() {
        let story = sample_story("Headline", "Hi.");
        let render = |cpm| {
            let mut buffer = Vec::new();
            {
                let mut renderer = Renderer::new(&mut buffer, 60, true, Theme::plain(), cpm);
                renderer
                    .print_story(Source::Hn, None, &story, Utc::now(), false)
                    .unwrap();
                renderer.flush().unwrap();
            }
            String::from_utf8(buffer).unwrap()
        };
        assert_eq!(render(None), render(Some(100_000)));
    }

    #[test]
    fn prints_the_selected_topic_in_the_source_line() {
        let now = Utc.with_ymd_and_hms(2026, 11, 15, 12, 0, 0).unwrap();
        let mut buffer = Vec::new();
        {
            let mut renderer = Renderer::new(&mut buffer, 60, true, Theme::plain(), None);
            renderer
                .print_story(
                    Source::Bbc,
                    Some("Sport"),
                    &sample_story("Headline", "Text."),
                    now,
                    false,
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
            full: None,
            details: vec!["5 points".to_string(), "1 comment".to_string()],
            domain: Some("apps.apple.com".to_string()),
        };

        let mut buffer = Vec::new();
        {
            let mut renderer = Renderer::new(&mut buffer, 34, false, Theme::plain(), None);
            renderer
                .print_story(Source::Hn, None, &story, Utc::now(), false)
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
}
