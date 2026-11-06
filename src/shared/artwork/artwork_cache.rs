use super::decode_album_art;
use crate::shared::repaint::Repaint;
use gpui::RenderImage;
use std::{
    collections::{HashMap, HashSet, VecDeque},
    sync::{
        mpsc::{self, Receiver, Sender},
        Arc, Condvar, Mutex, MutexGuard, PoisonError,
    },
    time::{Duration, Instant},
};

/// Covers kept on the GPU; at most 320 px each, so the cache stays around 64 MB.
const MAX_CACHED_ALBUM_TEXTURES: usize = 160;
const COVER_FADE_DURATION: Duration = Duration::from_millis(250);
/// Time between redraws while a cover fades in.
const FADE_FRAME: Duration = Duration::from_millis(16);
/// Most threads decoding covers at once; one core is left for drawing.
const MAX_DECODE_WORKERS: usize = 4;

/// Where an album's cover stands; an album without a slot has not been asked for yet, or was evicted.
enum Slot {
    /// The cover is queued for a worker or being decoded.
    Loading,
    Ready(Arc<RenderImage>),
    /// The cover could not be decoded, so it is not tried again until the cache is cleared.
    Failed,
}

/// A cover waiting to be decoded.
struct Job {
    epoch: usize,
    album_index: usize,
    bytes: Arc<[u8]>,
}

/// A decoded cover: the epoch it was asked for in, its album, and its image.
type Decoded = (usize, usize, Option<Arc<RenderImage>>);

#[derive(Default)]
struct QueueState {
    /// Covers waiting for a worker, in the order they were asked for.
    jobs: VecDeque<Job>,
    /// Set when the cache is dropped, so the workers stop.
    closed: bool,
}

/// Work shared with the decode workers.
#[derive(Default)]
struct Queue {
    state: Mutex<QueueState>,
    wake: Condvar,
}

impl Queue {
    fn lock(&self) -> MutexGuard<'_, QueueState> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

pub(crate) struct ArtworkCache {
    slots: HashMap<usize, Slot>,
    /// Albums with a ready texture, least recently drawn first.
    texture_lru: VecDeque<usize>,
    visible_texture_indices: HashSet<usize>,
    texture_fade_started_at: HashMap<usize, Instant>,
    /// Covers no longer cached, to be freed from the GPU by [`Self::take_evicted`]'s caller.
    evicted: Vec<Arc<RenderImage>>,
    /// Bumped by `clear`, so covers decoded before it are dropped.
    texture_epoch: usize,
    queue: Arc<Queue>,
    /// Decode workers started; they start with the first cover asked for.
    workers: usize,
    texture_receiver: Receiver<Decoded>,
    texture_sender: Sender<Decoded>,
}

impl ArtworkCache {
    pub(crate) fn new() -> Self {
        let (texture_sender, texture_receiver) = mpsc::channel();
        Self {
            slots: HashMap::new(),
            texture_lru: VecDeque::new(),
            visible_texture_indices: HashSet::new(),
            texture_fade_started_at: HashMap::new(),
            evicted: Vec::new(),
            texture_epoch: 0,
            queue: Arc::default(),
            workers: 0,
            texture_receiver,
            texture_sender,
        }
    }

    pub(crate) fn clear(&mut self) {
        self.texture_epoch = self.texture_epoch.wrapping_add(1);
        self.queue.lock().jobs.clear();
        self.evicted
            .extend(self.slots.drain().filter_map(|(_, slot)| match slot {
                Slot::Ready(image) => Some(image),
                Slot::Loading | Slot::Failed => None,
            }));
        self.texture_lru.clear();
        self.visible_texture_indices.clear();
        self.texture_fade_started_at.clear();
    }

    /// Take the covers that left the cache, so their GPU textures can be freed.
    pub(crate) fn take_evicted(&mut self) -> Vec<Arc<RenderImage>> {
        std::mem::take(&mut self.evicted)
    }

    /// Start a frame. Queued covers the last frame did not draw are dropped, so after a fast
    /// scroll the workers go straight to the covers in view.
    pub(crate) fn begin_frame(&mut self) {
        let mut drawn = std::mem::take(&mut self.visible_texture_indices);
        let slots = &mut self.slots;
        self.queue.lock().jobs.retain(|job| {
            let wanted = drawn.contains(&job.album_index);
            if !wanted {
                slots.remove(&job.album_index);
            }
            wanted
        });
        drawn.clear();
        self.visible_texture_indices = drawn;
    }

    pub(crate) fn texture(
        &mut self,
        repaint: &Repaint,
        album_index: usize,
        bytes: &Arc<[u8]>,
    ) -> Option<Arc<RenderImage>> {
        self.visible_texture_indices.insert(album_index);
        match self.slots.get(&album_index) {
            Some(Slot::Ready(texture)) => {
                let texture = Arc::clone(texture);
                self.texture_lru.retain(|&index| index != album_index);
                self.texture_lru.push_back(album_index);
                // A cover starts fading in the first time it is drawn.
                self.texture_fade_started_at
                    .entry(album_index)
                    .or_insert_with(Instant::now);
                return Some(texture);
            }
            Some(Slot::Loading | Slot::Failed) => return None,
            None => {}
        }
        if !self.start_workers(repaint) {
            self.slots.insert(album_index, Slot::Failed);
            return None;
        }
        self.slots.insert(album_index, Slot::Loading);
        self.queue.lock().jobs.push_back(Job {
            epoch: self.texture_epoch,
            album_index,
            bytes: Arc::clone(bytes),
        });
        self.queue.wake.notify_one();
        None
    }

    /// Whether an album's cover could not be decoded, so it should be drawn without one.
    pub(crate) fn failed(&self, album_index: usize) -> bool {
        matches!(self.slots.get(&album_index), Some(Slot::Failed))
    }

    /// Start the decode workers if they are not running yet.
    ///
    /// # Returns
    /// Whether at least one worker is running.
    fn start_workers(&mut self, repaint: &Repaint) -> bool {
        if self.workers > 0 {
            return true;
        }
        let count = std::thread::available_parallelism()
            .map_or(2, |cores| cores.get().saturating_sub(1))
            .clamp(1, MAX_DECODE_WORKERS);
        for _ in 0..count {
            let queue = Arc::clone(&self.queue);
            let sender = self.texture_sender.clone();
            let repaint = repaint.clone();
            if std::thread::Builder::new()
                .name("album-art-decode".into())
                .spawn(move || decode_jobs(&queue, &sender, &repaint))
                .is_ok()
            {
                self.workers += 1;
            }
        }
        self.workers > 0
    }

    pub(crate) fn poll(&mut self) {
        while let Ok((epoch, album_index, image)) = self.texture_receiver.try_recv() {
            // A cover dropped from the queue may still finish; only one asked for counts.
            if epoch != self.texture_epoch
                || !matches!(self.slots.get(&album_index), Some(Slot::Loading))
            {
                continue;
            }
            let Some(image) = image else {
                self.slots.insert(album_index, Slot::Failed);
                continue;
            };
            while self.texture_lru.len() >= MAX_CACHED_ALBUM_TEXTURES {
                let Some(oldest_position) = self
                    .texture_lru
                    .iter()
                    .position(|index| !self.visible_texture_indices.contains(index))
                else {
                    break;
                };
                // Keep the fade record so a cover reloaded after eviction appears without fading.
                if let Some(oldest) = self.texture_lru.remove(oldest_position) {
                    if let Some(Slot::Ready(image)) = self.slots.remove(&oldest) {
                        self.evicted.push(image);
                    }
                }
            }
            self.slots.insert(album_index, Slot::Ready(image));
            self.texture_lru.push_back(album_index);
        }
    }

    pub(crate) fn texture_opacity(&self, repaint: &Repaint, album_index: usize) -> f32 {
        let Some(fade_started_at) = self.texture_fade_started_at.get(&album_index) else {
            return 0.0;
        };

        let elapsed = fade_started_at.elapsed();
        if elapsed < COVER_FADE_DURATION {
            repaint.request_repaint_after(FADE_FRAME);
        }
        let progress = (elapsed.as_secs_f32() / COVER_FADE_DURATION.as_secs_f32()).clamp(0.0, 1.0);
        progress * progress * (3.0 - 2.0 * progress)
    }
}

impl Drop for ArtworkCache {
    fn drop(&mut self) {
        self.queue.lock().closed = true;
        self.queue.wake.notify_all();
    }
}

/// Decode queued covers until the cache is dropped.
fn decode_jobs(queue: &Queue, sender: &Sender<Decoded>, repaint: &Repaint) {
    loop {
        let job = {
            let mut state = queue.lock();
            loop {
                if state.closed {
                    return;
                }
                if let Some(job) = state.jobs.pop_front() {
                    break job;
                }
                state = queue
                    .wake
                    .wait(state)
                    .unwrap_or_else(PoisonError::into_inner);
            }
        };
        // A panic would otherwise end the worker and leave the cover loading.
        let decoded = std::panic::catch_unwind(|| decode_album_art(&job.bytes)).unwrap_or(None);
        if sender.send((job.epoch, job.album_index, decoded)).is_err() {
            return;
        }
        repaint.request_repaint();
    }
}
