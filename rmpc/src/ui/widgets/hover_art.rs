use std::{collections::HashMap, sync::Arc};

use anyhow::Result;
use ratatui::prelude::Rect;

use crate::{
    config::tabs::PaneType,
    ctx::Ctx,
    shared::{mpd_client_ext::MpdClientExt, mpd_query::MpdQueryResult},
    ui::{
        UiEvent,
        image::facade::{AlbumArtFacade, EncodeData},
    },
};

pub const HOVER_ALBUM_ART: &str = "hover_album_art";

#[derive(Debug)]
pub struct HoverAlbumArt {
    album_art: AlbumArtFacade,
    target_pane: PaneType,
    last_file: Option<String>,
    current_area: Rect,
    is_modal_open: bool,
    cache: HashMap<String, Option<Arc<Vec<u8>>>>,
}

impl HoverAlbumArt {
    pub fn new(target_pane: PaneType, ctx: &Ctx) -> Self {
        Self {
            album_art: AlbumArtFacade::new(ctx),
            target_pane,
            last_file: None,
            current_area: Rect::default(),
            is_modal_open: false,
            cache: HashMap::new(),
        }
    }

    pub fn set_area(&mut self, area: Rect) {
        if self.current_area != area {
            self.current_area = area;
            self.album_art.set_size(area);
        }
    }

    pub fn update_hover(
        &mut self,
        file: Option<String>,
        area: Rect,
        is_visible: bool,
        ctx: &Ctx,
    ) -> Result<()> {
        self.set_area(area);
        if !is_visible || self.is_modal_open || area.width == 0 || area.height == 0 {
            return Ok(());
        }

        if file != self.last_file {
            self.last_file = file.clone();
            if let Some(f) = file {
                let cache_key = if let Some(parent) =
                    std::path::Path::new(&f).parent().and_then(|p| p.to_str())
                {
                    if !parent.is_empty() { parent.to_string() } else { f.clone() }
                } else {
                    f.clone()
                };

                if let Some(cached) = self.cache.get(&cache_key) {
                    if let Some(data) = cached {
                        self.album_art.show(Arc::clone(data), ctx)?;
                    } else {
                        self.album_art.show_default(ctx)?;
                    }
                    return Ok(());
                }

                let order = ctx.config.album_art.order;
                let target = self.target_pane.clone();
                ctx.query().id(HOVER_ALBUM_ART).replace_id(HOVER_ALBUM_ART).target(target).query(
                    move |client| {
                        let result = client.find_album_art(&f, order)?;
                        Ok(MpdQueryResult::AlbumArt(result))
                    },
                );
            } else {
                self.album_art.show_default(ctx)?;
            }
        }
        Ok(())
    }

    pub fn on_query_finished(
        &mut self,
        id: &'static str,
        data: &MpdQueryResult,
        is_visible: bool,
        ctx: &Ctx,
    ) -> Result<()> {
        if !is_visible || self.is_modal_open {
            return Ok(());
        }
        if id == HOVER_ALBUM_ART {
            match data {
                MpdQueryResult::AlbumArt(Some(data)) => {
                    let arc_data = Arc::new(data.clone());
                    if let Some(f) = &self.last_file {
                        let cache_key = if let Some(parent) =
                            std::path::Path::new(f).parent().and_then(|p| p.to_str())
                        {
                            if !parent.is_empty() { parent.to_string() } else { f.clone() }
                        } else {
                            f.clone()
                        };
                        if self.cache.len() > 100 {
                            self.cache.clear();
                        }
                        self.cache.insert(cache_key, Some(Arc::clone(&arc_data)));
                    }
                    self.album_art.show(arc_data, ctx)?;
                }
                MpdQueryResult::AlbumArt(None) => {
                    if let Some(f) = &self.last_file {
                        let cache_key = if let Some(parent) =
                            std::path::Path::new(f).parent().and_then(|p| p.to_str())
                        {
                            if !parent.is_empty() { parent.to_string() } else { f.clone() }
                        } else {
                            f.clone()
                        };
                        self.cache.insert(cache_key, None);
                    }
                    self.album_art.show_default(ctx)?;
                }
                _ => {}
            }
        }
        Ok(())
    }

    pub fn on_event(&mut self, event: &mut UiEvent, is_visible: bool, ctx: &Ctx) -> Result<()> {
        match event {
            UiEvent::ImageEncoded { data } if is_visible && !self.is_modal_open => {
                if !matches!(data, EncodeData::Empty) {
                    self.album_art.display(std::mem::take(data), ctx)?;
                }
            }
            UiEvent::ImageEncodeFailed { err } if is_visible => {
                self.album_art.image_processing_failed(err, ctx)?;
            }
            UiEvent::ModalOpened if is_visible => {
                if !self.is_modal_open {
                    self.album_art.hide(ctx)?;
                }
                self.is_modal_open = true;
            }
            UiEvent::ModalClosed if is_visible => {
                self.is_modal_open = false;
                self.album_art.show_current(ctx)?;
            }
            UiEvent::Exit => {
                self.album_art.cleanup()?;
            }
            _ => {}
        }
        Ok(())
    }

    pub fn on_hide(&mut self, ctx: &Ctx) -> Result<()> {
        self.album_art.hide(ctx)
    }

    pub fn resize(&mut self, area: Rect, ctx: &Ctx) -> Result<()> {
        if self.is_modal_open {
            return Ok(());
        }
        self.set_area(area);
        self.album_art.show_current(ctx)
    }
}
