use std::{
    collections::HashMap,
    sync::{LazyLock, Mutex},
    time::{Duration, Instant},
};

use anyhow::Result;
use either::Either;
use ratatui::{
    Frame,
    layout::{Alignment, Rect},
    text::Line,
};

use super::Pane;
use crate::{
    config::theme::properties::{Property, PropertyKind},
    ctx::Ctx,
    shared::{id::{self, Id}, keys::ActionEvent},
    ui::widgets::scrolling_line::ScrollingLine,
};

struct PropertyScrollState {
    last_song_id: Option<u32>,
    last_elapsed: Duration,
    song_started_at: Instant,
    schedule_id: Id,
}

static SCROLL_STATES: LazyLock<Mutex<HashMap<usize, PropertyScrollState>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

#[derive(Debug)]
pub struct PropertyPane<'content> {
    content: &'content Vec<Property<PropertyKind>>,
    align: Alignment,
    scroll_speed: u64,
}

impl<'content> PropertyPane<'content> {
    pub fn new(
        content: &'content Vec<Property<PropertyKind>>,
        align: Alignment,
        scroll_speed: u64,
        _ctx: &Ctx,
    ) -> Self {
        Self { content, align, scroll_speed }
    }
}

impl Pane for PropertyPane<'_> {
    fn render(&mut self, frame: &mut Frame, area: Rect, ctx: &Ctx) -> Result<()> {
        let song = ctx.current_song();
        let current_song_id = song.map(|s| s.id);

        let content_key = self.content as *const _ as usize;
        let mut states = SCROLL_STATES.lock().unwrap();
        let state = states.entry(content_key).or_insert_with(|| PropertyScrollState {
            last_song_id: current_song_id,
            last_elapsed: ctx.status.elapsed,
            song_started_at: Instant::now(),
            schedule_id: id::new(),
        });

        if state.last_song_id != current_song_id
            || (ctx.status.elapsed < Duration::from_secs(1) && state.last_elapsed > Duration::from_secs(2))
        {
            state.last_song_id = current_song_id;
            state.song_started_at = Instant::now();
        }
        state.last_elapsed = ctx.status.elapsed;

        let elapsed = state.song_started_at.elapsed();
        let schedule_id = state.schedule_id;
        drop(states);

        let line = Line::from(self.content.iter().fold(Vec::new(), |mut acc, val| {
            match val.as_span(
                song,
                ctx,
                &ctx.config.theme.format_tag_separator,
                ctx.config.theme.multiple_tag_resolution_strategy,
            ) {
                Some(Either::Left(span)) => acc.push(span),
                Some(Either::Right(ref mut spans)) => acc.append(spans),
                None => {}
            }
            acc
        }));

        let line_len = line.width() as u16;
        let scrolling_line = ScrollingLine::builder()
            .scroll_speed(self.scroll_speed)
            .align(self.align)
            .line(line)
            .progress(elapsed)
            .build();
        frame.render_widget(scrolling_line, area);

        if self.scroll_speed > 0 && line_len > area.width {
            ctx.scheduler.schedule_replace(schedule_id, Duration::from_millis(90), |(tx, _)| {
                tx.send(crate::shared::events::AppEvent::RequestRender)?;
                Ok(())
            });
        }

        Ok(())
    }

    fn handle_action(&mut self, _event: &mut ActionEvent, _ctx: &mut Ctx) -> Result<()> {
        Ok(())
    }
}
