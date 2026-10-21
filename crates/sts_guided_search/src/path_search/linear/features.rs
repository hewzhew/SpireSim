//! Schema 2: hashed fact templates over the current entity schema plus task/resource rows.
//! No seed/RNG/UUID, answer, route coordinate, future outcome or tactical score.
use super::super::{choice_groups, PathContext};
use crate::entities::{self, EntityPosition, WIDTH};
use crate::{CombatBranch, CombatSearchState};
use serde::{Deserialize, Serialize};
use sts_combat_outcome::combat_witness_materialization::starting_potion_slots_targeted_by_input_v1;
use sts_core::{content::cards::get_card_definition, state::core::ClientInput};

pub const SCHEMA: u32 = 2;
pub const FEATURE_COUNT: usize = 16384;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LinearInput {
    pub schema: u32,
    pub groups: Vec<usize>,
    pub rows: Vec<Vec<(usize, f64)>>,
}

impl LinearInput {
    pub fn validate(&self) -> Result<(), String> {
        if self.schema != SCHEMA
            || self.rows.is_empty()
            || self.rows.len() != self.groups.len()
            || self.groups.iter().any(|g| *g >= self.rows.len())
            || self.rows.iter().any(|r| {
                r.iter().any(|(i, v)| *i >= FEATURE_COUNT || !v.is_finite())
                    || r.windows(2).any(|w| w[0].0 >= w[1].0)
            })
        {
            return Err("invalid linear feature schema, rows or indices".into());
        }
        let mut seen = vec![false; self.groups.iter().max().unwrap() + 1];
        for &g in &self.groups {
            seen[g] = true;
        }
        if seen.contains(&false) {
            return Err("non-dense linear groups".into());
        }
        Ok(())
    }
}

// Fixed FNV-1a, explicitly wrapping: this is feature hashing, not game arithmetic.
fn key(words: &[u64]) -> u64 {
    words
        .iter()
        .flat_map(|w| w.to_le_bytes())
        .fold(14695981039346656037u64, |h, b| {
            (h ^ u64::from(b)).wrapping_mul(1099511628211)
        })
}
#[derive(Default)]
struct Facts(Vec<(u64, f64)>);
impl Facts {
    fn new() -> Self {
        Self::default()
    }

    fn ordered(mut self) -> Vec<(u64, f64)> {
        // Stable sorting preserves insertion order for coincident keys, exactly
        // matching the former tree's additions (including hash collisions).
        self.0.sort_by_key(|(k, _)| *k);
        let mut end = 0;
        for read in 0..self.0.len() {
            let (key, value) = self.0[read];
            if end > 0 && self.0[end - 1].0 == key {
                self.0[end - 1].1 += value;
            } else {
                self.0[end] = (key, 0.0 + value);
                end += 1;
            }
        }
        self.0.truncate(end);
        self.0
    }
}
fn add(facts: &mut Facts, words: &[u64], value: f64) {
    if value != 0.0 {
        facts.0.push((key(words), value));
    }
}
fn squashed(value: f64) -> f64 {
    value.signum() * value.abs().ln_1p()
}

fn hashed_row(local: Facts, hashed: &mut [f64], touched: &mut Vec<usize>) -> Vec<(usize, f64)> {
    // Reduce distinct fact keys in sorted order, after reducing equal keys.
    // Combining these two reductions would change floating-point collisions.
    for (k, v) in local.ordered() {
        let index = (k as usize) % FEATURE_COUNT;
        hashed[index] += v;
        touched.push(index);
    }
    touched.sort_unstable();
    touched.dedup();
    let mut row = Vec::with_capacity(touched.len());
    for index in touched.drain(..) {
        let value = std::mem::replace(&mut hashed[index], 0.0);
        if value != 0.0 {
            row.push((index, value));
        }
    }
    row
}

fn resource_rows(p: &mut EntityPosition, state: &CombatSearchState, context: PathContext<'_>) {
    let mut values = [0.0; WIDTH];
    values[0] = context.goal.min_final_hp as f32 / 75.0;
    values[1] = context.spent_originals.count_ones() as f32;
    values[2] = context.goal.preserve_starting_potion_slots.count_ones() as f32;
    values[3] = context.root.combat.entities.player.current_hp as f32 / 75.0;
    p.ids.push([16, 0]);
    p.values.push(values);
    p.refs.push([0, -1, -1]);
    let slots: Vec<_> = state
        .position()
        .combat
        .entities
        .potions
        .iter()
        .enumerate()
        .map(|(s, _)| {
            starting_potion_slots_targeted_by_input_v1(
                context.root,
                state.position(),
                &ClientInput::DiscardPotion(s),
            )
        })
        .collect();
    for (slot, potion) in context.root.combat.entities.potions.iter().enumerate() {
        let Some(potion) = potion else {
            continue;
        };
        let bit = 1u64.checked_shl(slot as u32).unwrap_or(0);
        let current = slots.iter().position(|mask| mask & bit != 0);
        let row = current.and_then(|slot| {
            p.ids
                .iter()
                .enumerate()
                .find(|(i, id)| id[0] == 6 && p.values[*i][0] == slot as f32)
                .map(|(i, _)| i as i64)
        });
        let mut values = [0.0; WIDTH];
        values[0] = slot as f32;
        values[1] = f32::from(context.goal.preserve_starting_potion_slots & bit != 0);
        values[2] = f32::from(context.spent_originals & bit != 0);
        values[3] = f32::from(current.is_some());
        p.ids.push([17, i64::from(potion.id.stable_ordinal()) + 1]);
        p.values.push(values);
        p.refs.push([row.unwrap_or(-1), -1, -1]);
    }
}

fn object(facts: &mut Facts, p: &EntityPosition, values: &[[f64; WIDTH]], row: usize, role: u64) {
    let [kind, category] = p.ids[row];
    add(facts, &[10, role, kind as u64, category as u64], 1.0);
    for (field, v) in values[row].iter().enumerate() {
        add(facts, &[11, role, kind as u64, field as u64], *v);
    }
}

pub(crate) fn encode_entities(
    state: &CombatSearchState,
    context: PathContext<'_>,
    branches: &[CombatBranch],
) -> Result<EntityPosition, String> {
    let mut p = entities::encode_choices(state, &branches.iter().collect::<Vec<_>>(), None)?;
    resource_rows(&mut p, state, context);
    Ok(p)
}

pub fn encode(
    state: &CombatSearchState,
    context: PathContext<'_>,
    branches: &[CombatBranch],
) -> Result<LinearInput, String> {
    let p = encode_entities(state, context, branches)?;
    // An entity participates in many action/neighbor/product rows. Squash its
    // individual fields once; pooled global sums still get squashed separately.
    let values: Vec<_> = p
        .values
        .iter()
        .map(|row| row.map(|v| squashed(f64::from(v))))
        .collect();
    let mut global = Facts::new();
    for (i, [kind, category]) in p.ids.iter().enumerate() {
        add(&mut global, &[1, *kind as u64, *category as u64], 1.0);
        let owner = p.refs[i][0];
        let owner_id = if owner >= 0 {
            p.ids[owner as usize]
        } else {
            [-1, -1]
        };
        for (field, v) in p.values[i].iter().enumerate() {
            add(&mut global, &[2, *kind as u64, field as u64], f64::from(*v));
            add(
                &mut global,
                &[
                    3,
                    *kind as u64,
                    *category as u64,
                    owner_id[0] as u64,
                    owner_id[1] as u64,
                    field as u64,
                ],
                f64::from(*v),
            );
        }
    }
    let global: Vec<_> = global
        .ordered()
        .into_iter()
        .map(|(k, v)| (k, squashed(v)))
        .collect();
    let mut rows = Vec::new();
    // Reuse the bounded feature domain instead of allocating a second tree for
    // every action. Traverse local facts in their original sorted order so hash
    // collisions retain exactly the same floating-point accumulation order.
    let mut hashed = vec![0.0; FEATURE_COUNT];
    let mut touched = Vec::new();
    for (a, branch) in branches.iter().enumerate() {
        let kind = p.action_ids[a][0] as u64;
        let mut local = Facts::new();
        let mut selectors = vec![key(&[20, kind])];
        add(&mut local, &[0, kind], 1.0);
        for (i, v) in p.actions[a].iter().enumerate() {
            add(&mut local, &[4, kind, i as u64], squashed(f64::from(*v)));
        }
        if let CombatBranch::Input(ClientInput::PlayCard { card_index, .. }) = branch {
            let card = &state.position().combat.zones.hand[*card_index];
            let card_type = get_card_definition(card.id).card_type as u64;
            selectors.push(key(&[21, kind, card_type]));
            add(&mut local, &[5, kind, card_type], 1.0);
        }
        for (role, &r) in p.action_refs[a].iter().enumerate() {
            if r < 0 {
                continue;
            }
            let row = r as usize;
            object(&mut local, &p, &values, row, role as u64);
            if role == 0 {
                selectors.push(key(&[22, kind, p.ids[row][0] as u64, p.ids[row][1] as u64]));
            }
            for (edge, &child) in p.refs[row].iter().enumerate() {
                if child >= 0 {
                    object(
                        &mut local,
                        &p,
                        &values,
                        child as usize,
                        10 + role as u64 * 3 + edge as u64,
                    );
                }
            }
            for (child, refs) in p.refs.iter().enumerate() {
                if refs.contains(&r) {
                    object(&mut local, &p, &values, child, 30 + role as u64);
                }
            }
        }
        // Generic numerical interactions: source object x player/target facts.
        // No sign or coefficient here expresses a tactical preference.
        if p.action_refs[a][0] >= 0 {
            let obj = p.action_refs[a][0] as usize;
            for (side, owner) in [0, p.action_refs[a][1]].iter().enumerate() {
                if *owner < 0 {
                    continue;
                }
                for (i, x) in values[obj].iter().enumerate() {
                    for (j, y) in values[*owner as usize].iter().take(14).enumerate() {
                        add(
                            &mut local,
                            &[6, kind, side as u64, i as u64, j as u64],
                            *x * *y,
                        );
                    }
                }
            }
        }
        for selector in selectors {
            for &(k, v) in &global {
                add(&mut local, &[7, selector, k], v);
            }
        }
        rows.push(hashed_row(local, &mut hashed, &mut touched));
    }
    let input = LinearInput {
        schema: SCHEMA,
        groups: choice_groups(state, branches)?,
        rows,
    };
    input.validate()?;
    Ok(input)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn equal_fact_keys_keep_insertion_order_and_zero_entries() {
        let facts = Facts(vec![(9, 1e16), (3, -2.0), (9, 1.0), (3, 0.5), (9, -1e16)]);
        assert_eq!(facts.ordered(), vec![(3, -1.5), (9, 0.0)]);
    }

    #[test]
    fn hash_collisions_use_key_order_and_do_not_leak_between_rows() {
        let mut hashed = vec![0.0; FEATURE_COUNT];
        let mut touched = Vec::new();
        let stride = FEATURE_COUNT as u64;
        // Source order gives 1, sorted-key order gives 0 at feature index 7.
        let facts = Facts(vec![
            (7 + 2 * stride, -1e16),
            (7, 1e16),
            (7 + stride, 1.0),
            (3, 2.0),
        ]);
        assert_eq!(hashed_row(facts, &mut hashed, &mut touched), vec![(3, 2.0)]);
        assert!(touched.is_empty());
        let next = Facts(vec![(7, 0.5), (7 + stride, 0.25)]);
        assert_eq!(hashed_row(next, &mut hashed, &mut touched), vec![(7, 0.75)]);
        assert!(hashed.iter().all(|v| v.to_bits() == 0));
    }
}
