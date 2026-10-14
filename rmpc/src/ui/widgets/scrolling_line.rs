use std::time::Duration;

use bon::Builder;
use ratatui::{
    buffer::Buffer,
    layout::{Alignment, Rect},
    style::Style,
    text::{Line, StyledGrapheme},
    widgets::{Paragraph, Widget},
};
use unicode_width::UnicodeWidthStr;

#[derive(Builder)]
pub struct ScrollingLine<'a> {
    scroll_speed: u64,
    align: Alignment,
    line: Line<'a>,
    progress: Duration,
}

impl Widget for ScrollingLine<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.width == 0 || area.height == 0 {
            return;
        }

        let line_len = self.line.width() as u64;
        let max_len = area.width as u64;

        if line_len <= max_len || self.scroll_speed == 0 {
            Paragraph::new(self.line).alignment(self.align).render(area, buf);
            return;
        }

        let overflow = line_len.saturating_sub(max_len);
        let speed = (self.scroll_speed as f64) * (2.0 / 3.0);
        let scroll_secs = overflow as f64 / speed;
        let pause_secs = scroll_secs / 2.0;
        let cycle_secs = scroll_secs * 2.0 + pause_secs * 2.0;

        let t = self.progress.as_secs_f64() % cycle_secs;
        let cols_to_offset = if t < scroll_secs {
            let progress = (t / scroll_secs).min(1.0);
            ((progress * overflow as f64).floor() as usize).min(overflow as usize)
        } else if t < scroll_secs + pause_secs {
            overflow as usize
        } else if t < scroll_secs * 2.0 + pause_secs {
            let t_back = t - (scroll_secs + pause_secs);
            let progress = (t_back / scroll_secs).min(1.0);
            let scrolled = (progress * overflow as f64).floor() as usize;
            (overflow as usize).saturating_sub(scrolled)
        } else {
            0
        };

        let mut x = 0u64;
        let mut acc = 0;

        for StyledGrapheme { symbol, style } in self
            .line
            .styled_graphemes(Style::default())
            .skip_while(|StyledGrapheme { symbol, .. }| {
                let result = acc < cols_to_offset;
                acc += symbol.width();
                result
            })
        {
            let width = symbol.width() as u64;
            if width == 0 {
                continue;
            }
            if x + width > max_len {
                break;
            }

            buf[(area.left() + x as u16, area.top())].set_symbol(symbol).set_style(style);
            x += width;
        }
    }
}
