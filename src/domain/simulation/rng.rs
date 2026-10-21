//! Deterministic, parallel-safe random number generation.
//!
//! Every Monte Carlo run is split into chunks; chunk `i` of a run seeded with `seed` always
//! uses the stream `ChaCha8Rng::seed_from_u64(seed) ^ stream(i)`. Chunk results are combined
//! by addition, so the final result is identical regardless of thread scheduling.

use rand::SeedableRng;
use rand_chacha::ChaCha8Rng;

pub fn chunk_rng(seed: u64, chunk: u64) -> ChaCha8Rng {
    let mut rng = ChaCha8Rng::seed_from_u64(seed);
    rng.set_stream(chunk);
    rng
}

/// Split `total` iterations into chunks of roughly `chunk_size`; returns (chunk index, count).
pub fn chunks(total: u64, chunk_size: u64) -> Vec<(u64, u64)> {
    let mut out = Vec::new();
    let mut done = 0;
    let mut idx = 0;
    while done < total {
        let n = (total - done).min(chunk_size);
        out.push((idx, n));
        done += n;
        idx += 1;
    }
    out
}
