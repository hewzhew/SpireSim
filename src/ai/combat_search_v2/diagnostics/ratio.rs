use std::collections::HashMap;

use super::super::ResourceVector;

pub(super) fn resource_vector_count<K>(table: &HashMap<K, Vec<ResourceVector>>) -> usize {
    table.values().map(Vec::len).sum()
}

pub(super) fn rounded_ratio(numerator: u64, denominator: u64) -> f64 {
    if denominator == 0 {
        return 0.0;
    }
    let value = numerator as f64 / denominator as f64;
    (value * 100.0).round() / 100.0
}
