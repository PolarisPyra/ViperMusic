use super::decode_album_art;
use eframe::egui;
use std::{
    collections::{HashMap, HashSet, VecDeque},
    sync::{
        atomic::{AtomicUsize, Ordering},
        mpsc::{self, Receiver},
        Arc,
    },
};

/// Covers kept on the GPU; at most 320 px each, so the cache stays around 64 MB.
const MAX_CACHED_ALBUM_TEXTURES: usize = 160;
const COVER_FADE_DURATION: f64 = 0.42;

pub(crate) struct ArtworkCache {
    textures: HashMap<usize, egui::TextureHandle>,
    texture_lru: VecDeque<usize>,
    visible_texture_indices: HashSet<usize>,
    texture_fade_started_at: HashMap<usize, f64>,
    texture_requests: HashSet<(usize, usize)>,
    texture_epoch: usize,
    texture_workers: Arc<AtomicUsize>,
    texture_receiver: Receiver<(usize, usize, Option<egui::ColorImage>)>,
    texture_sender: mpsc::Sender<(usize, usize, Option<egui::ColorImage>)>,
}

impl ArtworkCache {
    pub(crate) fn new() -> Self {
        let (texture_sender, texture_receiver) = mpsc::channel();
        Self {
            textures: HashMap::new(),
            texture_lru: VecDeque::new(),
            visible_texture_indices: HashSet::new(),
            texture_fade_started_at: HashMap::new(),
            texture_requests: HashSet::new(),
            texture_epoch: 0,
            texture_workers: Arc::new(AtomicUsize::new(0)),
            texture_receiver,
            texture_sender,
        }
    }

    pub(crate) fn clear(&mut self) {
        self.texture_epoch = self.texture_epoch.wrapping_add(1);
        self.texture_requests.clear();
        self.textures.clear();
        self.texture_lru.clear();
        self.visible_texture_indices.clear();
        self.texture_fade_started_at.clear();
    }

    pub(crate) fn begin_frame(&mut self) {
        self.visible_texture_indices.clear();
    }

    pub(crate) fn texture(
        &mut self,
        ctx: &egui::Context,
        album_index: usize,
        bytes: &Arc<[u8]>,
    ) -> Option<egui::TextureHandle> {
        self.visible_texture_indices.insert(album_index);
        if let Some(texture) = self.textures.get(&album_index).cloned() {
            self.texture_lru.retain(|&index| index != album_index);
            self.texture_lru.push_back(album_index);
            return Some(texture);
        }

        let request = (self.texture_epoch, album_index);
        if !self.texture_requests.contains(&request) {
            let mut active = self.texture_workers.load(Ordering::Relaxed);
            while active < 4 {
                match self.texture_workers.compare_exchange_weak(
                    active,
                    active + 1,
                    Ordering::Relaxed,
                    Ordering::Relaxed,
                ) {
                    Ok(_) => break,
                    Err(observed) => active = observed,
                }
            }
            if active < 4 {
                let bytes = Arc::clone(bytes);
                let sender = self.texture_sender.clone();
                let repaint = ctx.clone();
                let workers = Arc::clone(&self.texture_workers);
                let epoch = self.texture_epoch;
                if std::thread::Builder::new()
                    .name("album-art-decode".into())
                    .spawn(move || {
                        let decoded = decode_album_art(&bytes);
                        let _ = sender.send((epoch, album_index, decoded));
                        workers.fetch_sub(1, Ordering::Relaxed);
                        repaint.request_repaint();
                    })
                    .is_ok()
                {
                    self.texture_requests.insert(request);
                } else {
                    self.texture_workers.fetch_sub(1, Ordering::Relaxed);
                }
            }
        }
        None
    }

    pub(crate) fn poll(&mut self, ctx: &egui::Context) {
        while let Ok((epoch, album_index, image)) = self.texture_receiver.try_recv() {
            self.texture_requests.remove(&(epoch, album_index));
            if epoch != self.texture_epoch {
                continue;
            }
            let Some(image) = image else {
                continue;
            };
            let texture = ctx.load_texture(
                format!("album-art-{epoch}-{album_index}"),
                image,
                egui::TextureOptions::LINEAR,
            );
            while self.textures.len() >= MAX_CACHED_ALBUM_TEXTURES {
                let Some(oldest_position) = self
                    .texture_lru
                    .iter()
                    .position(|index| !self.visible_texture_indices.contains(index))
                else {
                    break;
                };
                // Keep the fade record so a cover reloaded after eviction appears without fading.
                if let Some(oldest) = self.texture_lru.remove(oldest_position) {
                    self.textures.remove(&oldest);
                }
            }
            self.textures.insert(album_index, texture);
            self.texture_lru.push_back(album_index);
        }
    }

    pub(crate) fn finish_frame(&mut self, ctx: &egui::Context) {
        let now = ctx.input(|input| input.time);
        let mut fade_in_progress = false;
        for &album_index in &self.visible_texture_indices {
            if !self.textures.contains_key(&album_index) {
                continue;
            }
            let fade_started_at = self
                .texture_fade_started_at
                .entry(album_index)
                .or_insert(now);
            fade_in_progress |= now - *fade_started_at < COVER_FADE_DURATION;
        }
        if fade_in_progress {
            ctx.request_repaint_after(std::time::Duration::from_millis(16));
        }
    }

    pub(crate) fn texture_opacity(&self, ctx: &egui::Context, album_index: usize) -> f32 {
        let Some(fade_started_at) = self.texture_fade_started_at.get(&album_index) else {
            return 0.0;
        };

        let elapsed = (ctx.input(|input| input.time) - *fade_started_at).max(0.0);
        if elapsed < COVER_FADE_DURATION {
            ctx.request_repaint_after(std::time::Duration::from_millis(16));
        }
        let progress = (elapsed / COVER_FADE_DURATION).clamp(0.0, 1.0) as f32;
        progress * progress * (3.0 - 2.0 * progress)
    }
}
