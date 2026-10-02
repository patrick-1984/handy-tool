//! Agglomerative clustering of speaker embeddings.
//!
//! Complete linkage on cosine distance (1 - cos of L2-normalised vectors),
//! cut at a distance threshold — the same scheme as sherpa-onnx's
//! `FastClustering` (fastcluster `HCLUST_METHOD_COMPLETE` + `cutree_cdist`).
//! Built with the nearest-neighbour-chain algorithm, so it is O(n²) in time
//! and needs one condensed distance matrix (n·(n-1)/2 f32) of memory; n is the
//! number of (window, local speaker) embeddings, a few thousand at most.

/// Index into a condensed upper-triangular matrix for `i != j`.
fn condensed_index(n: usize, i: usize, j: usize) -> usize {
    let (a, b) = if i < j { (i, j) } else { (j, i) };
    // Rows 0..a hold (n-1) + (n-2) + ... + (n-a) entries.
    a * n - a * (a + 1) / 2 + (b - a - 1)
}

fn find(parent: &mut [usize], mut x: usize) -> usize {
    while parent[x] != x {
        parent[x] = parent[parent[x]];
        x = parent[x];
    }
    x
}

/// Cluster `embeddings` (all the same length) and return one label per row,
/// numbered 0.. in order of each cluster's first row. Two rows end up in the
/// same cluster exactly when complete linkage merges them at a cosine
/// distance `<= threshold`.
pub fn cluster(embeddings: &[Vec<f32>], threshold: f32) -> Vec<usize> {
    let n = embeddings.len();
    if n <= 1 {
        return vec![0; n];
    }

    let normalized: Vec<Vec<f32>> = embeddings
        .iter()
        .map(|e| {
            let norm = e.iter().map(|v| v * v).sum::<f32>().sqrt();
            if norm > 0.0 {
                e.iter().map(|v| v / norm).collect()
            } else {
                e.clone()
            }
        })
        .collect();

    let mut dist = vec![0f32; n * (n - 1) / 2];
    for i in 0..n {
        for j in (i + 1)..n {
            let cos: f32 = normalized[i]
                .iter()
                .zip(&normalized[j])
                .map(|(a, b)| a * b)
                .sum();
            dist[condensed_index(n, i, j)] = (1.0 - cos).max(0.0);
        }
    }

    // Nearest-neighbour chain. A cluster lives in the slot of one of its
    // members; merging a into b keeps slot b and retires slot a.
    let mut active = vec![true; n];
    let mut remaining = n;
    let mut chain: Vec<usize> = Vec::with_capacity(n);
    let mut parent: Vec<usize> = (0..n).collect();

    while remaining > 1 {
        if chain.is_empty()
            && let Some(first) = active.iter().position(|&a| a)
        {
            chain.push(first);
        }
        let (a, b, height) = loop {
            let a = *chain.last().expect("chain is non-empty");
            let prev = if chain.len() >= 2 {
                Some(chain[chain.len() - 2])
            } else {
                None
            };
            // Prefer the previous chain element on ties so the chain always
            // terminates at a reciprocal nearest-neighbour pair.
            let mut best = prev;
            let mut best_d = prev.map_or(f32::INFINITY, |p| dist[condensed_index(n, a, p)]);
            for k in 0..n {
                if k == a || !active[k] || Some(k) == prev {
                    continue;
                }
                let d = dist[condensed_index(n, a, k)];
                if d < best_d {
                    best_d = d;
                    best = Some(k);
                }
            }
            let b = best.expect("at least two active clusters remain");
            if Some(b) == prev {
                break (a, b, best_d);
            }
            chain.push(b);
        };
        chain.pop();
        chain.pop();

        // Complete linkage (Lance–Williams): d(k, a∪b) = max(d(k, a), d(k, b)).
        for k in 0..n {
            if k == a || k == b || !active[k] {
                continue;
            }
            let ka = dist[condensed_index(n, k, a)];
            let kb = condensed_index(n, k, b);
            if ka > dist[kb] {
                dist[kb] = ka;
            }
        }
        active[a] = false;
        remaining -= 1;

        // Complete-linkage heights are monotone, so every merge at or under
        // the threshold belongs to the cut regardless of the order the chain
        // discovers them in.
        if height <= threshold {
            let ra = find(&mut parent, a);
            let rb = find(&mut parent, b);
            if ra != rb {
                parent[ra] = rb;
            }
        }
    }

    let mut labels = vec![usize::MAX; n];
    let mut root_label: Vec<Option<usize>> = vec![None; n];
    let mut next = 0;
    for (i, label) in labels.iter_mut().enumerate() {
        let root = find(&mut parent, i);
        *label = *root_label[root].get_or_insert_with(|| {
            next += 1;
            next - 1
        });
    }
    labels
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Deterministic pseudo-random generator (LCG) so tests need no crate.
    struct Lcg(u64);
    impl Lcg {
        fn next(&mut self) -> f32 {
            self.0 = self
                .0
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            ((self.0 >> 33) as f32 / (1u64 << 31) as f32) * 2.0 - 1.0
        }
    }

    fn noisy(center: &[f32], rng: &mut Lcg, noise: f32) -> Vec<f32> {
        center.iter().map(|c| c + noise * rng.next()).collect()
    }

    /// Reference: textbook O(n³) complete linkage, merging the closest pair
    /// while its distance is within the threshold.
    fn naive(embeddings: &[Vec<f32>], threshold: f32) -> Vec<usize> {
        let n = embeddings.len();
        let cosd = |a: &Vec<f32>, b: &Vec<f32>| {
            let na = a.iter().map(|v| v * v).sum::<f32>().sqrt();
            let nb = b.iter().map(|v| v * v).sum::<f32>().sqrt();
            let dot: f32 = a.iter().zip(b).map(|(x, y)| x * y).sum();
            (1.0 - dot / (na * nb)).max(0.0)
        };
        let mut clusters: Vec<Vec<usize>> = (0..n).map(|i| vec![i]).collect();
        loop {
            let mut best: Option<(usize, usize, f32)> = None;
            for i in 0..clusters.len() {
                for j in (i + 1)..clusters.len() {
                    let d = clusters[i]
                        .iter()
                        .flat_map(|&x| clusters[j].iter().map(move |&y| (x, y)))
                        .map(|(x, y)| cosd(&embeddings[x], &embeddings[y]))
                        .fold(0.0f32, f32::max);
                    if best.is_none_or(|(_, _, bd)| d < bd) {
                        best = Some((i, j, d));
                    }
                }
            }
            match best {
                Some((i, j, d)) if d <= threshold => {
                    let moved = clusters.remove(j);
                    clusters[i].extend(moved);
                }
                _ => break,
            }
        }
        let mut labels = vec![0; n];
        let mut order: Vec<_> = clusters.iter().map(|c| *c.iter().min().unwrap()).collect();
        order.sort();
        for c in &clusters {
            let min = *c.iter().min().unwrap();
            let label = order.iter().position(|&m| m == min).unwrap();
            for &x in c {
                labels[x] = label;
            }
        }
        labels
    }

    #[test]
    fn empty_and_single() {
        assert!(cluster(&[], 0.5).is_empty());
        assert_eq!(cluster(&[vec![1.0, 2.0]], 0.5), vec![0]);
    }

    #[test]
    fn separates_three_synthetic_speakers() {
        let mut rng = Lcg(7);
        let dim = 64;
        let centers: Vec<Vec<f32>> = (0..3)
            .map(|_| (0..dim).map(|_| rng.next()).collect())
            .collect();
        // Interleave the speakers the way windows of a conversation would.
        let order = [0, 1, 0, 2, 1, 1, 0, 2, 2, 0, 1, 2, 0, 0, 1];
        let embeddings: Vec<Vec<f32>> = order
            .iter()
            .map(|&s| noisy(&centers[s], &mut rng, 0.15))
            .collect();
        let labels = cluster(&embeddings, 0.5);
        // Labels are numbered by first appearance: 0, 1, 0, 2, ...
        let expected: Vec<usize> = order.to_vec();
        assert_eq!(labels, expected);
    }

    #[test]
    fn threshold_controls_granularity() {
        let mut rng = Lcg(11);
        let base: Vec<f32> = (0..32).map(|_| rng.next()).collect();
        let embeddings: Vec<Vec<f32>> = (0..10).map(|_| noisy(&base, &mut rng, 0.2)).collect();
        // A loose threshold collapses everything into one speaker…
        assert!(cluster(&embeddings, 0.9).iter().all(|&l| l == 0));
        // …and a zero threshold keeps every (distinct) embedding apart.
        let strict = cluster(&embeddings, 0.0);
        assert_eq!(strict, (0..10).collect::<Vec<_>>());
    }

    #[test]
    fn matches_naive_complete_linkage() {
        let mut rng = Lcg(42);
        for trial in 0..20 {
            let n = 5 + trial * 2;
            let embeddings: Vec<Vec<f32>> = (0..n)
                .map(|_| (0..8).map(|_| rng.next()).collect())
                .collect();
            for threshold in [0.3, 0.6, 0.9, 1.2] {
                assert_eq!(
                    cluster(&embeddings, threshold),
                    naive(&embeddings, threshold),
                    "trial {trial} threshold {threshold}"
                );
            }
        }
    }
}
