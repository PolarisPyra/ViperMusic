//! Wakes the application from background threads so it polls their results and redraws.
use std::{
    sync::{Arc, Mutex, PoisonError},
    time::{Duration, Instant},
};

struct Shared {
    wake: async_channel::Sender<()>,
    /// Earliest time a delayed wake is due, if any.
    deadline: Mutex<Option<Instant>>,
}

/// Cloneable, thread-safe handle that asks the application to poll and redraw.
#[derive(Clone)]
pub(crate) struct Repaint(Arc<Shared>);

/// Receiving side of [`Repaint`], awaited by the application's poll loop.
pub(crate) struct RepaintReceiver {
    shared: Arc<Shared>,
    wakes: async_channel::Receiver<()>,
}

impl Repaint {
    /// Create a handle and the receiver the application's poll loop waits on.
    ///
    /// # Returns
    /// The handle and its receiver.
    pub(crate) fn new() -> (Self, RepaintReceiver) {
        let (wake, wakes) = async_channel::unbounded();
        let shared = Arc::new(Shared {
            wake,
            deadline: Mutex::new(None),
        });
        (Self(Arc::clone(&shared)), RepaintReceiver { shared, wakes })
    }

    /// Wake the application now.
    pub(crate) fn request_repaint(&self) {
        // Coalesce wakes: one queued wake already makes the loop poll everything.
        if self.0.wake.is_empty() {
            let _ = self.0.wake.try_send(());
        }
    }

    /// Wake the application once `delay` has passed, unless an earlier wake is already due.
    ///
    /// # Arguments
    /// * `delay` - Time to wait before waking.
    pub(crate) fn request_repaint_after(&self, delay: Duration) {
        let due = Instant::now() + delay;
        let mut deadline = self
            .0
            .deadline
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        if deadline.is_none_or(|current| due < current) {
            *deadline = Some(due);
            drop(deadline);
            // The poll loop recomputes its timer whenever it is woken.
            self.request_repaint();
        }
    }
}

impl RepaintReceiver {
    /// Wait until a wake is requested or a delayed wake falls due.
    ///
    /// # Arguments
    /// * `timer` - Builds a future that completes after the given duration.
    pub(crate) async fn wait<F: std::future::Future<Output = ()>>(
        &self,
        timer: impl FnOnce(Duration) -> F,
    ) {
        let deadline = *self
            .shared
            .deadline
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        // The receiver shares the sender, so the channel never closes while it waits.
        match deadline {
            Some(deadline) => {
                let delay = deadline.saturating_duration_since(Instant::now());
                futures_lite::future::or(
                    async {
                        let _ = self.wakes.recv().await;
                    },
                    timer(delay),
                )
                .await;
            }
            None => {
                let _ = self.wakes.recv().await;
            }
        }
        while self.wakes.try_recv().is_ok() {}
        let mut deadline = self
            .shared
            .deadline
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        if deadline.is_some_and(|deadline| deadline <= Instant::now()) {
            *deadline = None;
        }
    }
}

#[cfg(test)]
impl RepaintReceiver {
    /// Whether a wake arrives within `timeout`.
    pub(crate) fn woke_within(&self, timeout: Duration) -> bool {
        let end = Instant::now() + timeout;
        while Instant::now() < end {
            if self.wakes.try_recv().is_ok() {
                return true;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        false
    }
}
