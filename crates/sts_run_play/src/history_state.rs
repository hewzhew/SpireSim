use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;

use sts_oracle_learning::agent::information::state::{CombatLearningPowerV1, PublicCombatStateV1};
use sts_oracle_learning::agent::task::TaskStrategicObservationV1;
use sts_oracle_learning::content::powers::uses_sentinel_amount;
use sts_presentation::Locale;

pub(crate) fn message(locale: Locale, key: &str, parameters: &[(&str, &str)]) -> String {
    locale
        .text("history", key, parameters)
        .expect("history message")
}

pub(crate) fn render_deck_v1(observation: &TaskStrategicObservationV1, locale: Locale) -> String {
    let groups = crate::compact::inventory_card_groups_v1(&observation.cards);
    let mut identities = BTreeMap::<_, BTreeSet<_>>::new();
    for (_, card) in &groups {
        identities
            .entry(locale.entity("card", card.card.stable_id()))
            .or_default()
            .insert(card.card.stable_id());
    }
    groups
        .into_iter()
        .map(|(count, card)| {
            let mut label = crate::compact::permanent_card_label_v1(&card, locale);
            if identities[locale.entity("card", card.card.stable_id())].len() > 1 {
                write!(label, " [{}]", card.card.stable_id()).expect("string write");
            }
            if count == 1 {
                label
            } else {
                format!("{label}x{count}")
            }
        })
        .collect::<Vec<_>>()
        .join(", ")
}

pub(crate) fn render_relics_v1(observation: &TaskStrategicObservationV1, locale: Locale) -> String {
    observation
        .relics
        .iter()
        .map(|relic| {
            let mut label = locale.entity("relic", relic.relic.stable_id()).to_owned();
            if relic.counter != -1 {
                write!(label, "{{counter={}}}", relic.counter).expect("string write");
            }
            if relic.amount != 0 {
                write!(label, "{{amount={}}}", relic.amount).expect("string write");
            }
            if let Some(status) = sts_oracle_learning::content::relics::mechanics::trigger_state_v1(
                relic.relic,
                relic.used_up,
            ) {
                write!(label, "{{{}}}", status.description(locale)).expect("string write");
            }
            label
        })
        .collect::<Vec<_>>()
        .join(", ")
}

pub(crate) fn render_potions_v1(
    observation: &TaskStrategicObservationV1,
    locale: Locale,
) -> String {
    observation
        .potions
        .iter()
        .map(|slot| {
            slot.potion.as_ref().map_or_else(
                || format!("{}:{}", slot.slot, message(locale, "empty", &[])),
                |potion| {
                    format!(
                        "{}:{}",
                        slot.slot,
                        locale.entity("potion", potion.potion.stable_id())
                    )
                },
            )
        })
        .collect::<Vec<_>>()
        .join(", ")
}

pub(crate) fn push_scalar_delta_v1(deltas: &mut Vec<String>, label: &str, before: i32, after: i32) {
    if before != after {
        deltas.push(format!("{label}={before}->{after}({:+})", after - before));
    }
}

/// Compare typed recorded values before rendering them in either language.
/// Slots do not establish persistent monster identity across a summon.
pub(crate) fn combat_boundary_deltas_v1(
    before: &PublicCombatStateV1,
    after: &PublicCombatStateV1,
    locale: Locale,
) -> Vec<String> {
    let mut changes = Vec::new();
    push_scalar_delta_v1(
        &mut changes,
        &message(locale, "state.playerHp", &[]),
        before.player.hp,
        after.player.hp,
    );
    push_scalar_delta_v1(
        &mut changes,
        &message(locale, "state.playerBlock", &[]),
        before.player.block,
        after.player.block,
    );
    power_deltas(
        &mut changes,
        &message(locale, "state.player", &[]),
        &before.player.powers,
        &after.player.powers,
        locale,
    );
    let roster = |state: &PublicCombatStateV1| {
        state
            .monsters
            .iter()
            .map(|monster| {
                (
                    monster.slot,
                    monster.enemy.clone(),
                    monster.hp,
                    monster.block,
                    monster.alive,
                    monster.half_dead,
                    monster.escaped,
                    power_values(&monster.powers),
                )
            })
            .collect::<Vec<_>>()
    };
    if roster(before) != roster(after) {
        changes.push(message(
            locale,
            "state.enemiesBefore",
            &[("roster", &render_monster_roster_v1(before, locale))],
        ));
        changes.push(message(
            locale,
            "state.enemiesAfter",
            &[("roster", &render_monster_roster_v1(after, locale))],
        ));
    }
    changes
}

fn render_monster_roster_v1(state: &PublicCombatStateV1, locale: Locale) -> String {
    state.monsters.iter().map(|monster| {
        let sts_oracle_learning::agent::information::state::CombatLearningEnemyIdentityV1::Known { enemy_id } = monster.enemy;
        let mut label = message(locale, "state.monster", &[
            ("slot", &monster.slot.to_string()),
            ("enemy", locale.entity("enemy", enemy_id.stable_id())),
            ("hp", &monster.hp.to_string()),
            ("block", &monster.block.to_string()),
        ]);
        for (flag, key) in [
            (!monster.alive, "state.dead"),
            (monster.half_dead, "state.reviving"),
            (monster.escaped, "state.escaped"),
        ] {
            if flag {
                label.push_str(&message(locale, key, &[]));
            }
        }
        for power in &monster.powers {
            let amount = if uses_sentinel_amount(power.power) {
                message(locale, "present", &[])
            } else {
                power.amount.to_string()
            };
            write!(label, " {}({amount})", locale.entity("power", power.power.stable_id())).expect("string write");
        }
        label
    }).collect::<Vec<_>>().join("; ")
}

fn power_values(powers: &[CombatLearningPowerV1]) -> BTreeMap<&'static str, Vec<Option<i32>>> {
    let mut map = BTreeMap::<_, Vec<_>>::new();
    for power in powers {
        map.entry(power.power.stable_id())
            .or_default()
            .push((!uses_sentinel_amount(power.power)).then_some(power.amount));
    }
    for entries in map.values_mut() {
        entries.sort();
    }
    map
}

fn power_deltas(
    changes: &mut Vec<String>,
    owner: &str,
    before: &[CombatLearningPowerV1],
    after: &[CombatLearningPowerV1],
    locale: Locale,
) {
    let old = power_values(before);
    let new = power_values(after);
    let keys: BTreeSet<_> = old.keys().chain(new.keys()).copied().collect();
    for key in keys {
        if old.get(key) != new.get(key) {
            let describe = |value: Option<&Vec<Option<i32>>>| {
                value.map_or_else(
                    || message(locale, "absent", &[]),
                    |values| {
                        values
                            .iter()
                            .map(|amount| {
                                amount.map_or_else(
                                    || message(locale, "present", &[]),
                                    |amount| amount.to_string(),
                                )
                            })
                            .collect::<Vec<_>>()
                            .join(",")
                    },
                )
            };
            changes.push(format!(
                "{owner} {}: {} -> {}",
                locale.entity("power", key),
                describe(old.get(key)),
                describe(new.get(key))
            ));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn observed_deck() -> TaskStrategicObservationV1 {
        let directory = crate::decision_tests::TestDirectory::new();
        let controller = crate::decision_tests::controller_with_session(
            &directory.0,
            "history-deck",
            &crate::decision_tests::fixture("reward"),
        );
        let public = controller
            .head()
            .current_decision
            .as_ref()
            .unwrap()
            .public_decision_v1()
            .unwrap();
        let sts_oracle_learning::agent::task::TaskPublicObservationV1::Strategic { observation } =
            public.observation
        else {
            panic!()
        };
        observation
    }

    #[test]
    fn history_deck_keeps_same_name_identities_and_all_permanent_modifiers() {
        use sts_oracle_learning::content::{cards::CardId, relics::RelicId};
        let mut observation = observed_deck();
        let mut red = observation.cards[0];
        red.card = CardId::Strike;
        red.misc_value = 42;
        red.base_damage_override = Some(17);
        red.base_block_override = Some(9);
        red.cost_modifier = -1;
        red.bottled_by = Some(RelicId::BottledFlame);
        let mut green = red;
        green.card = CardId::StrikeG;
        green.ordinal += 1;
        observation.cards = vec![red, green];
        let rendered = render_deck_v1(&observation, Locale::En);
        assert!(
            rendered.contains("[Strike]") && rendered.contains("[StrikeG]"),
            "{rendered}"
        );
        for expected in ["42", "17", "9", "-1", "Bottled Flame"] {
            assert!(rendered.contains(expected), "{rendered}");
        }
        for locale in [Locale::En, Locale::ZhCn] {
            let changes = crate::compact::permanent_card_changes_v1(&[red], &[green], locale);
            assert_eq!(changes.len(), 2);
            assert_eq!(
                changes.iter().map(|(_, count)| *count).collect::<Vec<_>>(),
                [-1, 1]
            );
            assert!(changes[0].0.contains("[Strike]") && changes[1].0.contains("[StrikeG]"));
            let mut reordered = red;
            reordered.ordinal = 99;
            assert!(
                crate::compact::permanent_card_changes_v1(&[red], &[reordered], locale).is_empty()
            );
        }
    }

    #[test]
    fn history_permanent_change_exposes_cost_changes_without_a_card_name_change() {
        let observation = observed_deck();
        let before = observation.cards[0];
        let mut after = before;
        after.cost_modifier += 1;
        let changes = crate::compact::permanent_card_changes_v1(&[before], &[after], Locale::En);
        assert_eq!(changes.len(), 2);
        assert_eq!(changes[0].1, -1);
        assert_eq!(changes[1].1, 1);
        assert!(changes[1].0.contains("cost +1"), "{changes:?}");
    }

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
        let changes =
            combat_boundary_deltas_v1(&before, &public_combat_state_v1(&state), Locale::En);
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
        power_deltas(&mut changes, "M0", &[artifact.clone()], &[], Locale::En);
        power_deltas(
            &mut changes,
            "M2",
            &[artifact.clone()],
            &[artifact],
            Locale::En,
        );
        assert_eq!(changes, ["M0 Artifact: 1 -> absent"]);
    }

    #[test]
    fn tracked_power_changes_ignore_sentinel_amount_and_order_but_keep_removal() {
        use sts_oracle_learning::agent::information::state::public_combat_state_v1;
        use sts_oracle_learning::content::{monsters::EnemyId, powers::PowerId};
        use sts_oracle_learning::test_support::{blank_test_combat, test_monster};
        let mut state = blank_test_combat();
        state.entities.monsters = vec![test_monster(EnemyId::TheCollector)];
        let mut before = public_combat_state_v1(&state);
        let sentinel = CombatLearningPowerV1 {
            power: PowerId::Barricade,
            amount: -1,
            extra_data: 0,
            just_applied: false,
            payload_card: None,
            source_creature: None,
        };
        let mut strength = sentinel.clone();
        strength.power = PowerId::Strength;
        strength.amount = 3;
        before.player.powers = vec![sentinel.clone(), strength.clone()];
        before.monsters[0].powers = vec![sentinel, strength];
        let mut after = before.clone();
        after.player.powers[0].amount = -99;
        after.monsters[0].powers[0].amount = -99;
        after.monsters[0].powers.reverse();
        for locale in [Locale::En, Locale::ZhCn] {
            assert!(combat_boundary_deltas_v1(&before, &after, locale).is_empty());
        }
        after
            .player
            .powers
            .retain(|power| power.power != PowerId::Barricade);
        let english = combat_boundary_deltas_v1(&before, &after, Locale::En);
        let chinese = combat_boundary_deltas_v1(&before, &after, Locale::ZhCn);
        assert_eq!(english, ["Player Barricade: present -> absent"]);
        assert_eq!(chinese.len(), 1);
        assert!(chinese[0].contains("壁垒") && !chinese[0].contains("-99"));
    }
}
