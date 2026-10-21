//! Progress reporting for long-running engines.
//!
//! Engines are generic over a `Progress` sink, so the job runner's database-backed reporter
//! and a unit test's `Silent` compile to direct calls with no boxing or dynamic dispatch.

/// Receives progress in `[0, 1]`; returning `false` asks the engine to stop early.
pub trait Progress: Sync {
    fn report(&self, fraction: f64) -> bool;
}

/// Any `Fn(f64) -> bool` closure is a progress sink.
impl<F> Progress for F
where
    F: Fn(f64) -> bool + Sync,
{
    fn report(&self, fraction: f64) -> bool {
        self(fraction)
    }
}

/// Discards progress and never cancels.
#[derive(Debug, Clone, Copy, Default)]
pub struct Silent;

impl Progress for Silent {
    fn report(&self, _: f64) -> bool {
        true
    }
}

/// Shared bookkeeping for chunked parallel runs: counts finished chunks and latches a
/// cancellation flag once the sink asks to stop.
pub struct ChunkTracker<'a, P: Progress + ?Sized> {
    sink: &'a P,
    total: u64,
    done: std::sync::atomic::AtomicU64,
    cancelled: std::sync::atomic::AtomicBool,
}

impl<'a, P: Progress + ?Sized> ChunkTracker<'a, P> {
    pub fn new(sink: &'a P, total_chunks: u64) -> Self {
        Self {
            sink,
            total: total_chunks.max(1),
            done: std::sync::atomic::AtomicU64::new(0),
            cancelled: std::sync::atomic::AtomicBool::new(false),
        }
    }
    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(std::sync::atomic::Ordering::Relaxed)
    }
    pub fn chunk_done(&self) {
        let d = self.done.fetch_add(1, std::sync::atomic::Ordering::Relaxed) + 1;
        if !self.sink.report(d as f64 / self.total as f64) {
            self.cancelled
                .store(true, std::sync::atomic::Ordering::Relaxed);
        }
    }
}
