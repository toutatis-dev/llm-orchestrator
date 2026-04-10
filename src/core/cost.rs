use crate::core::WorkerTier;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Arc, RwLock};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CostEstimate {
    pub input_tokens: usize,
    pub output_tokens: usize,
    pub cost_usd: Decimal,
}

impl CostEstimate {
    pub fn total_tokens(&self) -> usize {
        self.input_tokens + self.output_tokens
    }
}

#[derive(Debug, Clone)]
pub struct CostTracker {
    costs: Arc<RwLock<Vec<WorkerCost>>>,
}

impl Default for CostTracker {
    fn default() -> Self {
        Self {
            costs: Arc::new(RwLock::new(Vec::new())),
        }
    }
}

#[derive(Debug, Clone)]
pub struct WorkerCost {
    pub tier: WorkerTier,
    pub model: String,
    pub input_tokens: usize,
    pub output_tokens: usize,
    pub cost_usd: Decimal,
}

impl CostTracker {
    pub fn record(&self, cost: WorkerCost) {
        if let Ok(mut costs) = self.costs.write() {
            costs.push(cost);
        }
    }

    pub fn total(&self) -> Decimal {
        self.costs
            .read()
            .map(|c| c.iter().map(|cost| cost.cost_usd).sum())
            .unwrap_or_default()
    }

    pub fn by_tier(&self) -> HashMap<WorkerTier, Decimal> {
        let mut by_tier: HashMap<WorkerTier, Decimal> = HashMap::new();

        if let Ok(costs) = self.costs.read() {
            for cost in costs.iter() {
                *by_tier.entry(cost.tier).or_insert(Decimal::ZERO) += cost.cost_usd;
            }
        }

        by_tier
    }

    pub fn summary(&self) -> CostSummary {
        let costs = self.costs.read().unwrap();
        CostSummary {
            total: costs.iter().map(|c| c.cost_usd).sum(),
            total_input_tokens: costs.iter().map(|c| c.input_tokens).sum(),
            total_output_tokens: costs.iter().map(|c| c.output_tokens).sum(),
            call_count: costs.len(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct CostSummary {
    pub total: Decimal,
    pub total_input_tokens: usize,
    pub total_output_tokens: usize,
    pub call_count: usize,
}

impl std::fmt::Display for CostSummary {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Cost: ${:.4} | Tokens: {} in / {} out | Calls: {}",
            self.total, self.total_input_tokens, self.total_output_tokens, self.call_count
        )
    }
}
