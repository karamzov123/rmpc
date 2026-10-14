use std::{borrow::Cow, cmp::Ordering, collections::VecDeque, time::Duration};

use itertools::Itertools;
use ratatui::{
    style::Style,
    text::{Line, Span, Text},
    widgets::ListItem,
};
use rmpc_mpd::commands::Song;
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

use crate::{
    config::{
        sort_mode::{SortMode, SortOptions},
        theme::{
            SymbolsConfig,
            TagResolutionStrategy,
            properties::{Property, PropertyKindOrText, SongProperty, Transform},
        },
    },
    ctx::Ctx,
    shared::{
        ext::{duration::DurationExt as _, span::SpanExt},
        mpd_query::PreviewGroup,
    },
    ui::dir_or_song::CmpByProp,
};

#[derive(Debug, PartialEq, Eq)]
pub(crate) struct SongCustomSort<'a, 'opts> {
    song: &'a Song,
    opts: &'opts SortOptions,
}

impl Ord for SongCustomSort<'_, '_> {
    fn cmp(&self, other: &Self) -> Ordering {
        match &self.opts.mode {
            SortMode::Format(items) => {
                let ignore_the = self.opts.ignore_leading_the;
                let mut a_is_leading = true;
                let mut b_is_leading = true;

                for prop in items {
                    let result = CmpByProp::song_cmp(
                        self.song,
                        other.song,
                        prop,
                        self.opts.fold_case,
                        a_is_leading && ignore_the,
                        b_is_leading && ignore_the,
                    );

                    // The property was not empty so we should no longer ignore
                    // leading "the"
                    if !result.first_empty {
                        a_is_leading = false;
                    }
                    if !result.second_empty {
                        b_is_leading = false;
                    }

                    if result.ordering != Ordering::Equal {
                        return if self.opts.reverse {
                            result.ordering.reverse()
                        } else {
                            result.ordering
                        };
                    }
                }

                Ordering::Equal
            }
            SortMode::ModifiedTime => {
                let result = self.song.last_modified.cmp(&other.song.last_modified);
                if self.opts.reverse { result.reverse() } else { result }
            }
        }
    }
}

impl PartialOrd for SongCustomSort<'_, '_> {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

pub trait SongExt {
    fn with_custom_sort<'song, 'opts>(
        &'song self,
        opts: &'opts SortOptions,
    ) -> SongCustomSort<'song, 'opts>;

    fn to_preview(
        &self,
        key_style: Style,
        group_style: Style,
        max_width: Option<usize>,
        ctx: &Ctx,
    ) -> Vec<PreviewGroup>;

    fn file_name(&self) -> Option<Cow<'_, str>>;

    fn file_ext(&self) -> Option<Cow<'_, str>>;

    fn format<'song>(
        &'song self,
        property: &SongProperty,
        tag_separator: &str,
        strategy: TagResolutionStrategy,
    ) -> Option<Cow<'song, str>>;

    fn matches_formats<'a>(
        &self,
        formats: impl IntoIterator<Item = &'a Property<SongProperty>>,
        filter: &str,
        ctx: &Ctx,
    ) -> bool;

    fn default_as_line<'song, 'stickers: 'song>(
        &'song self,
        format: &Property<SongProperty>,
        tag_separator: &str,
        strategy: TagResolutionStrategy,
        ctx: &'stickers Ctx,
    ) -> Option<Line<'song>>;

    fn as_line<'song, 'stickers: 'song>(
        &'song self,
        format: &Property<SongProperty>,
        tag_separator: &str,
        strategy: TagResolutionStrategy,
        ctx: &'stickers Ctx,
    ) -> Option<Line<'song>>;

    #[allow(dead_code)]
    fn as_line_scrolling<'song, 'stickers: 'song>(
        &'song self,
        format: &Property<SongProperty>,
        max_len: usize,
        tag_separator: &str,
        strategy: TagResolutionStrategy,
        scroll_speed: u64,
        ctx: &'stickers Ctx,
    ) -> Option<Line<'song>>;

    fn as_line_bouncing<'song, 'stickers: 'song>(
        &'song self,
        format: &Property<SongProperty>,
        max_len: usize,
        tag_separator: &str,
        strategy: TagResolutionStrategy,
        scroll_speed: u64,
        elapsed_on_cursor: Duration,
        ctx: &'stickers Ctx,
    ) -> Option<Line<'song>>;

    fn as_line_ellipsized<'song, 'stickers: 'song>(
        &'song self,
        format: &Property<SongProperty>,
        max_len: usize,
        symbols: &SymbolsConfig,
        tag_separator: &str,
        strategy: TagResolutionStrategy,
        ctx: &'stickers Ctx,
    ) -> Option<Line<'song>>;
}

fn format_preview_item(
    key: &str,
    value: &str,
    key_style: Style,
    max_width: Option<usize>,
) -> ListItem<'static> {
    let start_spacer = " ";
    let sep = ": ";
    let prefix = format!("{start_spacer}{key}{sep}");
    let prefix_width = UnicodeWidthStr::width(prefix.as_str());

    let Some(width) = max_width.filter(|&w| w > prefix_width + 4) else {
        return ListItem::new(Line::from(vec![
            Span::from(start_spacer),
            Span::styled(key.to_string(), key_style),
            Span::from(sep),
            Span::from(value.to_string()),
        ]));
    };

    let first_line_limit = width.saturating_sub(prefix_width);
    let indent_width = if prefix_width <= width.saturating_sub(8) {
        prefix_width
    } else {
        4
    };
    let indent_str = " ".repeat(indent_width);
    let subsequent_limit = width.saturating_sub(indent_width);

    // Split value into tokens that can break on spaces or after slashes
    let mut tokens: Vec<(String, bool)> = Vec::new();
    let words = value.split(' ').collect::<Vec<_>>();
    let num_words = words.len();
    for (i, &word) in words.iter().enumerate() {
        let is_last_word = i + 1 == num_words;
        if word.contains('/') {
            let mut rem = word;
            while let Some(slash_pos) = rem.find('/') {
                let part = &rem[..=slash_pos];
                rem = &rem[slash_pos + 1..];
                let is_end_of_word = rem.is_empty();
                tokens.push((part.to_string(), is_end_of_word && !is_last_word));
            }
            if !rem.is_empty() {
                tokens.push((rem.to_string(), !is_last_word));
            }
        } else {
            tokens.push((word.to_string(), !is_last_word));
        }
    }

    let mut lines: Vec<Line<'static>> = Vec::new();
    let mut current_line = String::new();
    let mut is_first_line = true;
    let mut current_limit = first_line_limit;

    for (token, has_space_after) in tokens {
        let token_width = UnicodeWidthStr::width(token.as_str());
        let current_width = UnicodeWidthStr::width(current_line.as_str());

        if !current_line.is_empty() && current_width + token_width > current_limit {
            if is_first_line {
                lines.push(Line::from(vec![
                    Span::from(start_spacer),
                    Span::styled(key.to_string(), key_style),
                    Span::from(sep),
                    Span::from(std::mem::take(&mut current_line)),
                ]));
                is_first_line = false;
                current_limit = subsequent_limit;
            } else {
                lines.push(Line::from(vec![
                    Span::from(indent_str.clone()),
                    Span::from(std::mem::take(&mut current_line)),
                ]));
            }
        }

        let mut rem_token = token.as_str();
        while UnicodeWidthStr::width(rem_token) > current_limit && current_limit > 0 {
            let mut fit_len = 0;
            for (idx, ch) in rem_token.char_indices() {
                let w = UnicodeWidthStr::width(&rem_token[..idx + ch.len_utf8()]);
                if w > current_limit {
                    break;
                }
                fit_len = idx + ch.len_utf8();
            }
            if fit_len == 0 {
                if let Some(ch) = rem_token.chars().next() {
                    fit_len = ch.len_utf8();
                }
            }
            let chunk = &rem_token[..fit_len];
            rem_token = &rem_token[fit_len..];

            if is_first_line {
                lines.push(Line::from(vec![
                    Span::from(start_spacer),
                    Span::styled(key.to_string(), key_style),
                    Span::from(sep),
                    Span::from(chunk.to_string()),
                ]));
                is_first_line = false;
                current_limit = subsequent_limit;
            } else {
                lines.push(Line::from(vec![
                    Span::from(indent_str.clone()),
                    Span::from(chunk.to_string()),
                ]));
            }
        }

        if !rem_token.is_empty() {
            current_line.push_str(rem_token);
        }
        if has_space_after {
            current_line.push(' ');
        }
    }

    if !current_line.is_empty() || is_first_line {
        let final_text = current_line.trim_end().to_string();
        if is_first_line {
            lines.push(Line::from(vec![
                Span::from(start_spacer),
                Span::styled(key.to_string(), key_style),
                Span::from(sep),
                Span::from(final_text),
            ]));
        } else {
            lines.push(Line::from(vec![
                Span::from(indent_str),
                Span::from(final_text),
            ]));
        }
    }

    ListItem::new(Text::from(lines))
}

impl SongExt for Song {
    fn with_custom_sort<'song, 'opts>(
        &'song self,
        opts: &'opts SortOptions,
    ) -> SongCustomSort<'song, 'opts> {
        SongCustomSort { song: self, opts }
    }

    fn to_preview(
        &self,
        key_style: Style,
        group_style: Style,
        max_width: Option<usize>,
        ctx: &Ctx,
    ) -> Vec<PreviewGroup> {
        let mut info_group = PreviewGroup::new(Some(" --- [Info]"), Some(group_style));

        info_group.push(format_preview_item("File", &self.file, key_style, max_width));

        if let Some(file_name) = self.file_name() {
            info_group.push(format_preview_item("Filename", &file_name, key_style, max_width));
        }

        if let Some(title) = self.metadata.get("title") {
            title.for_each(|item| {
                info_group.push(format_preview_item("Title", item, key_style, max_width));
            });
        }
        if let Some(artist) = self.metadata.get("artist") {
            artist.for_each(|item| {
                info_group.push(format_preview_item("Artist", item, key_style, max_width));
            });
        }

        if let Some(album) = self.metadata.get("album") {
            album.for_each(|item| {
                info_group.push(format_preview_item("Album", item, key_style, max_width));
            });
        }

        if let Some(duration) = &self.duration {
            info_group.push(format_preview_item(
                "Duration",
                &ctx.config.duration_format.format(duration),
                key_style,
                max_width,
            ));
        }

        info_group.push(format_preview_item(
            "Last Modified",
            &self.last_modified.to_string(),
            key_style,
            max_width,
        ));

        if let Some(added) = &self.added {
            info_group.push(format_preview_item(
                "Added",
                &added.to_string(),
                key_style,
                max_width,
            ));
        }

        let mut tags_group = PreviewGroup::new(Some(" --- [Tags]"), Some(group_style));
        for (k, v) in self
            .metadata
            .iter()
            .filter(|(key, _)| !["title", "album", "artist", "duration"].contains(&(*key).as_str()))
            .sorted_by_key(|(key, _)| *key)
        {
            v.for_each(|item| {
                tags_group.push(format_preview_item(k, item, key_style, max_width));
            });
        }

        let mut result = vec![info_group, tags_group];

        let stickers = ctx.song_stickers_if_supported(&self.file);
        if let Some(stickers) = stickers
            && !stickers.is_empty()
        {
            let mut stickers_group = PreviewGroup::new(Some(" --- [Stickers]"), Some(group_style));

            for (k, v) in stickers.iter().sorted_by_key(|(key, _)| *key) {
                stickers_group.push(format_preview_item(k, v, key_style, max_width));
            }

            result.push(stickers_group);
        }

        result
    }

    fn file_name(&self) -> Option<Cow<'_, str>> {
        std::path::Path::new(&self.file).file_stem().map(|file_name| file_name.to_string_lossy())
    }

    fn file_ext(&self) -> Option<Cow<'_, str>> {
        std::path::Path::new(&self.file).extension().map(|ext| ext.to_string_lossy()).or_else(
            || {
                self.metadata.get("realuri").and_then(|uri| {
                    std::path::Path::new(uri.first()).extension().map(|ext| ext.to_string_lossy())
                })
            },
        )
    }

    fn format<'song>(
        &'song self,
        property: &SongProperty,
        tag_separator: &str,
        strategy: TagResolutionStrategy,
    ) -> Option<Cow<'song, str>> {
        match property {
            SongProperty::Filename => self.file_name(),
            SongProperty::FileExtension => self.file_ext(),
            SongProperty::File => Some(Cow::Borrowed(self.file.as_str())),
            SongProperty::Title => {
                self.metadata.get("title").map(|v| strategy.resolve(v, tag_separator))
            }
            SongProperty::Artist => {
                self.metadata.get("artist").map(|v| strategy.resolve(v, tag_separator))
            }
            SongProperty::Album => {
                self.metadata.get("album").map(|v| strategy.resolve(v, tag_separator))
            }
            SongProperty::AlbumArtist => {
                self.metadata.get("albumartist").map(|v| strategy.resolve(v, tag_separator))
            }
            SongProperty::Duration => self.duration.map(|d| Cow::Owned(d.to_string())),
            SongProperty::Other(name) => {
                self.metadata.get(name).map(|v| strategy.resolve(v, tag_separator))
            }
            SongProperty::Disc => self.metadata.get("disc").map(|v| Cow::Borrowed(v.last())),
            SongProperty::Position => self.metadata.get("pos").map(|v| {
                v.last()
                    .parse::<usize>()
                    .map(|v| Cow::Owned((v + 1).to_string()))
                    .unwrap_or_default()
            }),
            SongProperty::Track => self.metadata.get("track").map(|v| {
                Cow::Owned(
                    v.last()
                        .parse::<u32>()
                        .map_or_else(|_| v.last().to_owned(), |v| format!("{v:0>2}")),
                )
            }),
            SongProperty::SampleRate() => self.samplerate().map(|v| Cow::Owned(v.to_string())),
            SongProperty::Bits() => self.bits().map(|v| Cow::Owned(v.to_string())),
            SongProperty::Channels() => self.channels().map(|v| Cow::Owned(v.to_string())),
            SongProperty::Added() => self.added.map(|d| Cow::Owned(d.to_string())),
            SongProperty::LastModified() => Some(Cow::Owned(self.last_modified.to_string())),
        }
    }

    fn matches_formats<'a>(
        &self,
        formats: impl IntoIterator<Item = &'a Property<SongProperty>>,
        filter: &str,
        ctx: &Ctx,
    ) -> bool {
        for format in formats {
            let match_found = match &format.kind {
                PropertyKindOrText::Text(value) => {
                    Some(value.to_lowercase().contains(&filter.to_lowercase()))
                }
                PropertyKindOrText::Sticker(key) => {
                    ctx.song_stickers(&self.file)
                        .and_then(|s| s.get(key))
                        .map(|value| value.to_lowercase().contains(&filter.to_lowercase()))
                        .or_else(|| {
                            format.default.as_ref().map(|f| {
                                self.matches_formats(std::iter::once(f.as_ref()), filter, ctx)
                            })
                        })
                }
                PropertyKindOrText::Property(property) => {
                    self.format(property, "", TagResolutionStrategy::All).map_or_else(
                        || {
                            format.default.as_ref().map(|f| {
                                self.matches_formats(std::iter::once(f.as_ref()), filter, ctx)
                            })
                        },
                        |p| Some(p.to_lowercase().contains(&filter.to_lowercase())),
                    )
                }
                PropertyKindOrText::Group(_) => format
                    .as_string(Some(self), "", TagResolutionStrategy::All, ctx)
                    .map(|v| v.to_lowercase().contains(&filter.to_lowercase())),
                PropertyKindOrText::Transform(Transform::Truncate { .. }) => format
                    .as_string(Some(self), "", TagResolutionStrategy::All, ctx)
                    .map(|v| v.to_lowercase().contains(&filter.to_lowercase())),
                PropertyKindOrText::Transform(Transform::Replace { .. }) => format
                    .as_string(Some(self), "", TagResolutionStrategy::All, ctx)
                    .map(|v| v.to_lowercase().contains(&filter.to_lowercase())),
            };
            if match_found.is_some_and(|v| v) {
                return true;
            }
        }

        false
    }

    fn default_as_line<'song, 'stickers: 'song>(
        &'song self,
        format: &Property<SongProperty>,
        tag_separator: &str,
        strategy: TagResolutionStrategy,
        ctx: &'stickers Ctx,
    ) -> Option<Line<'song>> {
        format.default.as_ref().and_then(|f| self.as_line(f.as_ref(), tag_separator, strategy, ctx))
    }

    fn as_line<'song, 'stickers: 'song>(
        &'song self,
        format: &Property<SongProperty>,
        tag_separator: &str,
        strategy: TagResolutionStrategy,
        ctx: &'stickers Ctx,
    ) -> Option<Line<'song>> {
        let style = ctx.config.as_text_style().patch(format.style.unwrap_or_default());
        match &format.kind {
            PropertyKindOrText::Text(value) => Some(Line::styled(value.clone(), style)),
            PropertyKindOrText::Sticker(key) => ctx
                .song_stickers(&self.file)
                .and_then(|s| s.get(key))
                .map(|sticker| Line::styled(sticker, style))
                .or_else(|| {
                    format.default.as_ref().and_then(|format| {
                        self.as_line(format.as_ref(), tag_separator, strategy, ctx)
                    })
                }),
            PropertyKindOrText::Property(property) => {
                self.format(property, tag_separator, strategy).map_or_else(
                    || self.default_as_line(format, tag_separator, strategy, ctx),
                    |v| Some(Line::styled(v, style)),
                )
            }
            PropertyKindOrText::Group(group) => {
                let mut buf = Line::default().style(style);
                for grformat in group {
                    if let Some(res) = self.as_line(grformat, tag_separator, strategy, ctx) {
                        for span in res.spans {
                            let span_style = span.style;
                            buf.push_span(span.style(res.style).patch_style(span_style));
                        }
                    } else {
                        return format
                            .default
                            .as_ref()
                            .and_then(|format| self.as_line(format, tag_separator, strategy, ctx));
                    }
                }

                Some(buf)
            }
            PropertyKindOrText::Transform(Transform::Replace { content, replacements }) => self
                .as_line(content, tag_separator, strategy, ctx)
                .and_then(|line| {
                    let mut content = String::new();
                    for span in &line.spans {
                        content.push_str(span.content.as_ref());
                    }

                    if let Some(replacement) = replacements.get(&content) {
                        return self.as_line(replacement, tag_separator, strategy, ctx).or_else(
                            || {
                                replacement.default.as_ref().and_then(|format| {
                                    self.as_line(format, tag_separator, strategy, ctx)
                                })
                            },
                        );
                    }

                    Some(line)
                })
                .or_else(|| {
                    format
                        .default
                        .as_ref()
                        .and_then(|format| self.as_line(format, tag_separator, strategy, ctx))
                }),
            PropertyKindOrText::Transform(Transform::Truncate { content, length, from_start }) => {
                self.as_line(content, tag_separator, strategy, ctx)
                    .map(|mut line| {
                        let mut buf = VecDeque::new();
                        let mut remaining_len = *length;
                        let push_fn =
                            if *from_start { VecDeque::push_front } else { VecDeque::push_back };
                        let truncate_fn =
                            if *from_start { Span::truncate_start } else { Span::truncate_end };
                        let spans_len = line.spans.len();

                        for i in 0..spans_len {
                            if remaining_len == 0 {
                                break;
                            }
                            let i = if *from_start { spans_len - 1 - i } else { i };
                            let mut span = std::mem::take(&mut line.spans[i]);

                            let remaining = truncate_fn(&mut span, remaining_len);
                            push_fn(&mut buf, span);
                            remaining_len = remaining_len.saturating_sub(remaining);
                        }

                        line.spans = Vec::from(buf);
                        line
                    })
                    .or_else(|| {
                        format
                            .default
                            .as_ref()
                            .and_then(|format| self.as_line(format, tag_separator, strategy, ctx))
                    })
            }
        }
    }

    fn as_line_scrolling<'song, 'stickers: 'song>(
        &'song self,
        format: &Property<SongProperty>,
        max_len: usize,
        tag_separator: &str,
        strategy: TagResolutionStrategy,
        scroll_speed: u64,
        ctx: &'stickers Ctx,
    ) -> Option<Line<'song>> {
        let mut line = self.as_line(format, tag_separator, strategy, ctx)?;
        let line_len = line.width();

        if line_len <= max_len || scroll_speed == 0 {
            return Some(line);
        }

        let line_len = (line_len + 3) as u64;
        let elapsed_ms = ctx.status.elapsed.as_millis() as u64;
        let cols_to_offset = ((elapsed_ms * scroll_speed) / 1000) % line_len;

        if cols_to_offset == 0 {
            return Some(line);
        }

        line.spans.push(Span::from(" | "));

        let mut already_offset = 0;
        while already_offset < cols_to_offset as usize {
            let Some(span) = line.spans.get_mut(0) else {
                break;
            };

            let sw = span.width();

            if sw == 0 {
                break;
            }

            // Span is smaller than the required offset, simply move it to the
            // end of the line
            if already_offset + sw <= cols_to_offset as usize {
                already_offset += sw;
                let span = line.spans.remove(0);
                line.spans.push(span);
                continue;
            }

            // Need to cut part of this span and move the cut part to the end of
            // the line
            let target = (cols_to_offset as usize).saturating_sub(already_offset);

            let mut owned = std::mem::take(&mut span.content).into_owned();
            let span_style = span.style;
            let mut new_span_content = String::new();

            let mut acc = 0;
            let mut cut_at_byte = 0;

            for (i, g) in owned.grapheme_indices(true) {
                let gw = g.width();
                if acc + gw > target {
                    cut_at_byte = i;
                    break;
                }
                acc += gw;
                cut_at_byte = i + g.len();
                new_span_content.push_str(g);
            }

            owned.drain(0..cut_at_byte);
            span.content = Cow::Owned(owned);
            line.spans.push(Span::styled(new_span_content, span_style));
            break;
        }

        Some(line)
    }

    fn as_line_bouncing<'song, 'stickers: 'song>(
        &'song self,
        format: &Property<SongProperty>,
        max_len: usize,
        tag_separator: &str,
        strategy: TagResolutionStrategy,
        scroll_speed: u64,
        elapsed_on_cursor: Duration,
        ctx: &'stickers Ctx,
    ) -> Option<Line<'song>> {
        let line = self.as_line(format, tag_separator, strategy, ctx)?;
        let line_len = line.width();

        if line_len <= max_len || scroll_speed == 0 {
            return Some(line);
        }

        let overflow = line_len.saturating_sub(max_len);
        let speed = (scroll_speed as f64) * (2.0 / 3.0);
        let scroll_secs = overflow as f64 / speed;
        let pause_secs = scroll_secs / 2.0;
        let cycle_secs = scroll_secs * 2.0 + pause_secs * 2.0;

        let t = elapsed_on_cursor.as_secs_f64() % cycle_secs;
        let offset = if t < scroll_secs {
            let progress = (t / scroll_secs).min(1.0);
            ((progress * overflow as f64).floor() as usize).min(overflow)
        } else if t < scroll_secs + pause_secs {
            overflow
        } else if t < scroll_secs * 2.0 + pause_secs {
            let t_back = t - (scroll_secs + pause_secs);
            let progress = (t_back / scroll_secs).min(1.0);
            let scrolled = (progress * overflow as f64).floor() as usize;
            overflow.saturating_sub(scrolled)
        } else {
            0
        };

        let mut new_spans = Vec::new();
        let mut col = 0;
        let window_end = offset + max_len;

        for span in line.spans {
            let mut sub_content = String::new();
            for g in span.content.graphemes(true) {
                let gw = UnicodeWidthStr::width(g);
                if col >= offset && col + gw <= window_end {
                    sub_content.push_str(g);
                }
                col += gw;
                if col >= window_end {
                    break;
                }
            }
            if !sub_content.is_empty() {
                new_spans.push(Span::styled(sub_content, span.style));
            }
            if col >= window_end {
                break;
            }
        }

        Some(Line::from(new_spans))
    }

    fn as_line_ellipsized<'song, 'stickers: 'song>(
        &'song self,
        format: &Property<SongProperty>,
        max_len: usize,
        symbols: &SymbolsConfig,
        tag_separator: &str,
        strategy: TagResolutionStrategy,
        ctx: &'stickers Ctx,
    ) -> Option<Line<'song>> {
        let mut line = self.as_line(format, tag_separator, strategy, ctx)?;

        let mut remaining = max_len;
        let mut idx = 0;

        let ellipsis_width = symbols.ellipsis.width();
        while remaining > 0 {
            let Some(span) = line.spans.get_mut(idx) else {
                break;
            };

            let sw = span.width();

            if sw < remaining {
                remaining -= sw;
                idx += 1;
                continue;
            }

            if sw == remaining {
                line.spans.truncate(idx + 1);
                break;
            }

            if remaining < ellipsis_width {
                // No space even for the configured ellipsis, just default the
                // whole line to "…"
                span.content = Cow::Borrowed("…");
                line.spans.truncate(idx + 1);
                break;
            }

            let target = remaining - ellipsis_width;

            let mut owned = std::mem::take(&mut span.content).into_owned();

            let mut acc = 0;
            let mut cut_at_byte = 0;

            for (i, g) in owned.grapheme_indices(true) {
                let gw = g.width();
                if acc + gw > target {
                    cut_at_byte = i;
                    break;
                }
                acc += gw;
                cut_at_byte = i + g.len();
            }

            owned.truncate(cut_at_byte);
            owned.push_str(&symbols.ellipsis);
            span.content = Cow::Owned(owned);
            line.spans.truncate(idx + 1);
            break;
        }

        Some(line)
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use rmpc_mpd::commands::Song;
    use rstest::rstest;

    use super::SongExt;

    fn song(file: &str, realuri: Option<&str>) -> Song {
        let mut metadata = HashMap::new();
        if let Some(realuri) = realuri {
            metadata.insert("realuri".to_string(), realuri.into());
        }
        Song {
            id: 1,
            file: file.to_string(),
            duration: None,
            metadata,
            last_modified: chrono::Utc::now(),
            added: None,
        }
    }

    #[rstest]
    #[case("album/track.flac", None, Some("flac"))]
    // relative RealUri, the form lsinfo and search report
    #[case("album/album.cue/track0007", Some("../01 - Title.flac"), Some("flac"))]
    // absolute RealUri, the form currentsong and playlistinfo report
    #[case("album/album.cue/track0007", Some("/mnt/music/album/01 - Title.flac"), Some("flac"))]
    #[case("album/track.mp3", Some("../01 - Title.flac"), Some("mp3"))]
    #[case("album/album.cue/track0007", None, None)]
    #[case("album/album.cue/track0007", Some("../CDImage"), None)]
    fn file_ext(#[case] file: &str, #[case] realuri: Option<&str>, #[case] expected: Option<&str>) {
        let song = song(file, realuri);

        assert_eq!(song.file_ext().as_deref(), expected);
    }
}
