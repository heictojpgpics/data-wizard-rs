//! Carve plan and parameters.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize, Deserialize)]
pub enum CarveStrategy {
    HeaderFooter,
    Bifragment,
    Structured,
    FragmentMap,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CarveParams {
    pub strategy: CarveStrategy,
    pub max_fragment_gap: u64,
    pub max_results: u64,
    pub chunk_size: u32,
}

impl Default for CarveParams {
    fn default() -> Self {
        Self {
            strategy: CarveStrategy::HeaderFooter,
            max_fragment_gap: 1 << 20, // 1 MiB
            max_results: 1_000_000,
            chunk_size: 1 << 20, // 1 MiB
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CarvePlan {
    pub params: CarveParams,
    pub regions: Vec<(u64, u64)>, // (offset, len)
}
