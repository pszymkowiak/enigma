use crate::types::ProviderInfo;
use std::sync::atomic::{AtomicUsize, Ordering};

/// Distributes chunks across storage providers.
pub struct Distributor {
    providers: Vec<ProviderInfo>,
    strategy: Strategy,
    rr_counter: AtomicUsize,
}

enum Strategy {
    RoundRobin,
    Weighted { cumulative_weights: Vec<u64> },
}

impl Distributor {
    /// Create a round-robin distributor.
    pub fn round_robin(providers: Vec<ProviderInfo>) -> Self {
        assert!(!providers.is_empty(), "At least one provider required");
        Self {
            providers,
            strategy: Strategy::RoundRobin,
            rr_counter: AtomicUsize::new(0),
        }
    }

    /// Create a weighted distributor. Chunks are assigned proportionally to provider weights.
    pub fn weighted(providers: Vec<ProviderInfo>) -> Self {
        assert!(!providers.is_empty(), "At least one provider required");
        let mut cumulative = Vec::with_capacity(providers.len());
        let mut total = 0u64;
        for p in &providers {
            total += p.weight as u64;
            cumulative.push(total);
        }
        Self {
            providers,
            strategy: Strategy::Weighted {
                cumulative_weights: cumulative,
            },
            rr_counter: AtomicUsize::new(0),
        }
    }

    /// Select the next provider for a chunk.
    pub fn next_provider(&self) -> &ProviderInfo {
        match &self.strategy {
            Strategy::RoundRobin => {
                let idx = self.rr_counter.fetch_add(1, Ordering::Relaxed) % self.providers.len();
                &self.providers[idx]
            }
            Strategy::Weighted { cumulative_weights } => {
                let total = *cumulative_weights.last().unwrap();
                let counter = self.rr_counter.fetch_add(1, Ordering::Relaxed) as u64;
                let point = counter % total;
                let idx = cumulative_weights.iter().position(|&w| point < w).unwrap();
                &self.providers[idx]
            }
        }
    }

    /// Get provider by ID.
    pub fn provider_by_id(&self, id: i64) -> Option<&ProviderInfo> {
        self.providers.iter().find(|p| p.id == id)
    }

    /// All providers.
    pub fn providers(&self) -> &[ProviderInfo] {
        &self.providers
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::ProviderType;

    fn make_providers(n: usize) -> Vec<ProviderInfo> {
        (0..n)
            .map(|i| ProviderInfo {
                id: i as i64,
                name: format!("provider-{i}"),
                provider_type: ProviderType::Local,
                bucket: format!("bucket-{i}"),
                region: None,
                weight: 1,
            })
            .collect()
    }

    #[test]
    fn round_robin_cycles() {
        let providers = make_providers(3);
        let dist = Distributor::round_robin(providers);

        let ids: Vec<i64> = (0..9).map(|_| dist.next_provider().id).collect();
        assert_eq!(ids, vec![0, 1, 2, 0, 1, 2, 0, 1, 2]);
    }

    #[test]
    fn weighted_distribution() {
        let providers = vec![
            ProviderInfo {
                id: 0,
                name: "heavy".into(),
                provider_type: ProviderType::S3,
                bucket: "b1".into(),
                region: None,
                weight: 3,
            },
            ProviderInfo {
                id: 1,
                name: "light".into(),
                provider_type: ProviderType::Azure,
                bucket: "b2".into(),
                region: None,
                weight: 1,
            },
        ];

        let dist = Distributor::weighted(providers);

        // Over 4 chunks, expect ~3:1 ratio
        let mut counts = [0u32; 2];
        for _ in 0..400 {
            let p = dist.next_provider();
            counts[p.id as usize] += 1;
        }

        // Heavy should have ~3x the chunks of light
        assert!(
            counts[0] > counts[1] * 2,
            "Expected heavy > 2*light, got {counts:?}"
        );
    }

    #[test]
    fn provider_by_id_found() {
        let providers = make_providers(3);
        let dist = Distributor::round_robin(providers);
        assert_eq!(dist.provider_by_id(1).unwrap().name, "provider-1");
    }

    #[test]
    fn provider_by_id_not_found() {
        let providers = make_providers(3);
        let dist = Distributor::round_robin(providers);
        assert!(dist.provider_by_id(99).is_none());
    }
}
