use std::collections::BTreeMap;
use std::fmt::Write as _;

use sts_oracle_learning::agent::information::state::{CombatLearningPowerV1, PublicCombatStateV1};
use sts_oracle_learning::agent::task::TaskStrategicObservationV1;
use sts_oracle_learning::content::identity::gameplay_display_label_v1;

use crate::render::format_card;

pub(crate) fn card_counts_v1(observation: &TaskStrategicObservationV1) -> BTreeMap<String, usize> {
    let mut cards = BTreeMap::new();
    for card in &observation.cards {
        let mut label = format_card(card.card, card.upgrades);
        if card.misc_value != 0 {
            write!(label, "{{misc={}}}", card.misc_value).expect("string write");
        }
        *cards.entry(label).or_default() += 1;
    }
    cards
}

pub(crate) fn render_deck_v1(observation: &TaskStrategicObservationV1) -> String {
    card_counts_v1(observation)
        .into_iter()
        .map(|(card, count)| {
            if count == 1 {
                card
            } else {
                format!("{card}x{count}")
            }
        })
        .collect::<Vec<_>>()
        .join(", ")
}

pub(crate) fn render_relics_v1(observation: &TaskStrategicObservationV1) -> String {
    observation
        .relics
        .iter()
        .map(|relic| {
            let mut label = gameplay_display_label_v1(relic.relic).to_string();
            if relic.counter != -1 {
                write!(label, "{{counter={}}}", relic.counter).expect("string write");
            }
            if relic.amount != 0 {
                write!(label, "{{amount={}}}", relic.amount).expect("string write");
            }
            if let Some(status) =
                sts_oracle_learning::content::relics::mechanics::trigger_state_description(
                    relic.relic,
                    relic.used_up,
                )
            {
                write!(label, "{{{status}}}").expect("string write");
            }
            label
        })
        .collect::<Vec<_>>()
        .join(", ")
}

pub(crate) fn render_potions_v1(observation: &TaskStrategicObservationV1) -> String {
    observation
        .potions
        .iter()
        .map(|slot| {
            slot.potion.as_ref().map_or_else(
                || format!("{}:empty", slot.slot),
                |potion| format!("{}:{}", slot.slot, gameplay_display_label_v1(potion.potion)),
            )
        })
        .collect::<Vec<_>>()
        .join(", ")
}

pub(crate) fn multiset_delta_v1(
    before: &BTreeMap<String, usize>,
    after: &BTreeMap<String, usize>,
) -> (Vec<String>, Vec<String>) {
    let mut added = Vec::new();
    let mut removed = Vec::new();
    for (item, after_count) in after {
        let delta = after_count.saturating_sub(before.get(item).copied().unwrap_or(0));
        if delta > 0 {
            added.push(if delta == 1 {
                item.clone()
            } else {
                format!("{item}x{delta}")
            });
        }
    }
    for (item, before_count) in before {
        let delta = before_count.saturating_sub(after.get(item).copied().unwrap_or(0));
        if delta > 0 {
            removed.push(if delta == 1 {
                item.clone()
            } else {
                format!("{item}x{delta}")
            });
        }
    }
    (added, removed)
}

pub(crate) fn push_scalar_delta_v1(deltas: &mut Vec<String>, label: &str, before: i32, after: i32) {
    if before != after {
        deltas.push(format!("{label}={before}->{after}({:+})", after - before));
    }
}

/// Differences at recorded public boundaries, not a reconstruction of effect triggers.
pub(crate) fn combat_boundary_deltas_v1(
    before: &PublicCombatStateV1,
    after: &PublicCombatStateV1,
) -> Vec<String> {
    let mut changes = Vec::new();
    push_scalar_delta_v1(&mut changes, "Player HP", before.player.hp, after.player.hp);
    push_scalar_delta_v1(
        &mut changes,
        "Player Block",
        before.player.block,
        after.player.block,
    );
    power_deltas(
        &mut changes,
        "Player",
        &before.player.powers,
        &after.player.powers,
    );
    // Archived public snapshots carry slots, not persistent monster identities.
    // Show the two observed rosters instead of attributing changes across slots.
    let old_roster = render_monster_roster_v1(before);
    let new_roster = render_monster_roster_v1(after);
    if old_roster != new_roster {
        changes.push(format!("Enemies before: {old_roster}"));
        changes.push(format!("Enemies after: {new_roster}"));
    }
    changes
}

fn render_monster_roster_v1(state: &PublicCombatStateV1) -> String {
    state.monsters.iter().map(|monster| {
        let sts_oracle_learning::agent::information::state::CombatLearningEnemyIdentityV1::Known { enemy_id } = monster.enemy;
        let mut label = format!("M{} {} HP {} Block {}", monster.slot,
            gameplay_display_label_v1(enemy_id), monster.hp, monster.block);
        if !monster.alive { write!(label, " alive=false").expect("string write"); }
        if monster.half_dead { label.push_str(" reviving"); }
        if monster.escaped { label.push_str(" escaped"); }
        for power in &monster.powers {
            let amount = if sts_oracle_learning::content::powers::uses_sentinel_amount(power.power) {
                "present".to_string()
            } else {
                power.amount.to_string()
            };
            write!(label, " {}({})", gameplay_display_label_v1(power.power), amount).expect("string write");
        }
        label
    }).collect::<Vec<_>>().join("; ")
}

fn power_deltas(
    changes: &mut Vec<String>,
    owner: &str,
    before: &[CombatLearningPowerV1],
    after: &[CombatLearningPowerV1],
) {
    let values = |powers: &[CombatLearningPowerV1]| {
        let mut map = BTreeMap::<&'static str, Vec<String>>::new();
        for power in powers {
            map.entry(gameplay_display_label_v1(power.power))
                .or_default()
                .push(
                    if sts_oracle_learning::content::powers::uses_sentinel_amount(power.power) {
                        "present".to_string()
                    } else {
                        power.amount.to_string()
                    },
                );
        }
        for entries in map.values_mut() {
            entries.sort();
        }
        map
    };
    let old = values(before);
    let new = values(after);
    let keys: std::collections::BTreeSet<_> = old.keys().chain(new.keys()).copied().collect();
    for key in keys {
        if old.get(key) != new.get(key) {
            let describe = |value: Option<&Vec<String>>| {
                value.map_or_else(|| "absent".into(), |values| values.join(","))
            };
            changes.push(format!(
                "{owner} {key}: {} -> {}",
                describe(old.get(key)),
                describe(new.get(key))
            ));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn public_history_shows_separate_rosters_after_a_summon() {
        use sts_oracle_learning::agent::information::state::public_combat_state_v1;
        use sts_oracle_learning::content::monsters::EnemyId;
        use sts_oracle_learning::test_support::{blank_test_combat, test_monster};
        let mut state = blank_test_combat();
        let mut boss = test_monster(EnemyId::TheCollector);
        boss.slot = 0;
        boss.current_hp = 300;
        state.entities.monsters = vec![boss.clone()];
        let before = public_combat_state_v1(&state);
        boss.slot = 1;
        let mut torch = test_monster(EnemyId::TorchHead);
        torch.slot = 0;
        torch.current_hp = 43;
        state.entities.monsters = vec![torch, boss];
        let changes = combat_boundary_deltas_v1(&before, &public_combat_state_v1(&state));
        assert_eq!(changes.len(), 2);
        assert!(changes[0].starts_with("Enemies before: M0 The Collector HP 300"));
        assert!(changes[1].starts_with("Enemies after: M0 Torch Head HP 43"));
        assert!(changes[1].contains("M1 The Collector HP 300"));
    }

    #[test]
    fn power_removal_is_explicit_and_does_not_reuse_another_monsters_state() {
        use sts_oracle_learning::content::powers::PowerId;
        let artifact = CombatLearningPowerV1 {
            power: PowerId::Artifact,
            amount: 1,
            extra_data: 0,
            just_applied: false,
            payload_card: None,
            source_creature: None,
        };
        let mut changes = Vec::new();
        power_deltas(&mut changes, "M0", &[artifact.clone()], &[]);
        power_deltas(&mut changes, "M2", &[artifact.clone()], &[artifact]);
        assert_eq!(changes, ["M0 Artifact: 1 -> absent"]);
    }

    #[test]
    fn multiset_delta_reports_only_real_deck_changes() {
        let before = BTreeMap::from([("Strike".to_string(), 5), ("Defend".to_string(), 4)]);
        let after = BTreeMap::from([
            ("Strike".to_string(), 4),
            ("Defend".to_string(), 4),
            ("Impervious".to_string(), 1),
        ]);

        let (added, removed) = multiset_delta_v1(&before, &after);

        assert_eq!(added, vec!["Impervious"]);
        assert_eq!(removed, vec!["Strike"]);
    }
}
