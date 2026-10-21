//! Shared successor filtering and rendering for root and continuation trials.
use super::*;

impl TurnResearch {
    pub(super) fn page(
        &mut self,
        request: &TurnResearchRequestV1,
        scope: &str,
    ) -> Result<TurnResearchPageV1, String> {
        let power_filters = resolve_power_filters(&request.player_powers)?;
        let prefix_actions = self.prefix.as_ref().map_or(0, |p| p.exact_actions.len());
        let root_powers =
            &public_combat_state_v1(&self.generator.current_root().unwrap().position().combat)
                .player
                .powers;
        // Exactly one explicit allowance, with a short responsiveness ceiling.
        // Zero work reads retained results without entering the generator.
        if request.work != 0 && request.engine_steps != 0 {
            self.generator
                .advance_options(
                    &EngineCombatStepper,
                    CombatPlanningQuantum {
                        additional_generation_work: request.work,
                        additional_engine_steps: request.engine_steps,
                        deadline: Some(
                            std::time::Instant::now() + std::time::Duration::from_millis(500),
                        ),
                    },
                )
                .map_err(|e| format!("turn generation: {e:?}"))?;
        }
        let handles = self.generator.option_handles();
        // Slots describe the current group order, not a monster's lifetime.
        // Resolve once at this trial root, then follow that exact Rust entity.
        let enemy_filter = request
            .enemy_hp_at_most
            .as_ref()
            .map(|filter| {
                let monster = self
                    .generator
                    .current_root()
                    .unwrap()
                    .position()
                    .combat
                    .entities
                    .monsters
                    .iter()
                    .find(|m| m.slot == filter.slot)
                    .ok_or("enemy HP filter must name a monster slot in this root")?;
                Ok::<_, String>((
                    monster.id,
                    filter.hp,
                    enemy_label(monster.enemy_id, monster.slot),
                ))
            })
            .transpose()?;
        let mut outcomes = BTreeMap::new();
        for handle in &handles {
            let outcome = TurnOutcomeV1::from(self.generator.option(*handle).unwrap().boundary());
            *outcomes.entry(outcome).or_insert(0usize) += 1;
        }
        let matching = handles
            .iter()
            .filter(|h| {
                let option = self.generator.option(**h).unwrap();
                request
                    .outcome
                    .is_none_or(|outcome| outcome == TurnOutcomeV1::from(option.boundary()))
                    && request.minimum_hp.is_none_or(|hp| {
                        option.exact_successor().combat.entities.player.current_hp >= hp
                    })
                    && request.minimum_block.is_none_or(|block| {
                        option.exact_successor().combat.entities.player.block >= block
                    })
                    && enemy_filter.as_ref().is_none_or(|(entity, hp, _)| {
                        option
                            .exact_successor()
                            .combat
                            .entities
                            .monsters
                            .iter()
                            .any(|m| m.id == *entity && m.current_hp <= *hp)
                    })
                    && power_filters.iter().all(|(power, filter)| {
                        let powers = store::powers_for(
                            &option.exact_successor().combat,
                            option.exact_successor().combat.entities.player.id,
                        )
                        .unwrap_or_default();
                        let found = powers.iter().find(|p| p.power_type == *power);
                        found.is_some() == filter.present
                            && filter
                                .minimum_amount
                                .is_none_or(|minimum| found.is_some_and(|p| p.amount >= minimum))
                    })
            })
            .copied()
            .collect::<Vec<_>>();
        let options = matching
            .iter()
            .skip(request.offset)
            .take(request.limit)
            .map(|h| -> Result<_, String> {
                let option = self.generator.option(*h).unwrap();
                let successor = public_combat_state_v1(&option.exact_successor().combat);
                let detail = project_combat_draft_option_v1(&self.generator, *h, TRANSITION_STEPS)?;
                let line = trial_line_label(&detail.actions);
                let player_power_changes = power_changes(root_powers, &successor.player.powers);
                let mut label =
                    format!("{line}\n  {}", outcome_label(option.boundary(), &successor));
                if !player_power_changes.is_empty() {
                    label.push_str(&format!(
                        "\n  Powers: {}",
                        player_power_changes
                            .iter()
                            .map(|change| {
                                change.after.map_or_else(
                                    || {
                                        format!(
                                            "{} removed",
                                            crate::presentation_names::name::<PowerId>(
                                                change.power.stable_id()
                                            )
                                        )
                                    },
                                    |amount| {
                                        crate::presentation_names::power(
                                            change.power.stable_id(),
                                            amount.into(),
                                        )
                                    },
                                )
                            })
                            .collect::<Vec<_>>()
                            .join(", ")
                    ));
                }
                Ok(TurnResearchOptionV1 {
                    reference: format!("{scope}:turn:{}:{}", self.id, h.ordinal()),
                    label,
                    boundary: TurnOutcomeV1::from(option.boundary()).id().into(),
                    action_count: prefix_actions + option.actions().len(),
                    prefix_action_count: prefix_actions,
                    player_power_changes,
                })
            })
            .collect::<Result<Vec<_>, _>>()?;
        let coverage = match self.generator.current_option_coverage().unwrap() {
            sts_combat_planner::CombatDraftOptionCoverage::Complete => "complete",
            sts_combat_planner::CombatDraftOptionCoverage::Open {
                mechanics_gap_count: 0,
            } => "open",
            _ => "mechanics_gap",
        };
        let next = request.offset.saturating_add(options.len());
        let counters = self.generator.current_generation_counters().unwrap();
        // Resource bounds describe the same filtered set the analyst can select.
        // They remain independent of the displayed page and generation order.
        let hp_range = resource_range(matching.iter().map(|h| {
            self.generator
                .option(*h)
                .unwrap()
                .exact_successor()
                .combat
                .entities
                .player
                .current_hp
        }));
        let block_range = resource_range(matching.iter().map(|h| {
            self.generator
                .option(*h)
                .unwrap()
                .exact_successor()
                .combat
                .entities
                .player
                .block
        }));
        let counts = if outcomes.is_empty() {
            "0 outcomes".into()
        } else {
            outcomes
                .iter()
                .map(|(outcome, count)| format!("{} {count}", outcome.title()))
                .collect::<Vec<_>>()
                .join(", ")
        };
        let mut summary = if let Some(prefix) = &self.prefix {
            format!(
                "Rust: after {}\nResults: {counts}; search {coverage}",
                prefix.label
            )
        } else {
            format!("Rust: {counts}; search {coverage}")
        };
        if let Some(range) = &hp_range {
            summary.push_str(&format!(
                "\nCombat HP: {}..{}",
                range.minimum, range.maximum
            ));
        }
        if let Some(range) = &block_range {
            if range.maximum > 0 || request.minimum_block.is_some() {
                summary.push_str(&format!("\nBlock: {}..{}", range.minimum, range.maximum));
            }
        }
        let slots = request
            .potion_slots
            .iter()
            .map(|s| format!("P{s}"))
            .collect::<Vec<_>>();
        summary.push_str(&format!(
            "\nPotion plays: {}",
            if slots.is_empty() {
                "none".into()
            } else {
                slots.join(", ")
            }
        ));
        if let Some(outcome) = request.outcome {
            summary.push_str(&format!("\nFilter: {}", outcome.title()));
        }
        if let Some(hp) = request.minimum_hp {
            summary.push_str(&format!("\nFilter: HP >= {hp}"));
        }
        if let Some(block) = request.minimum_block {
            summary.push_str(&format!("\nFilter: Block >= {block}"));
        }
        if let Some((_, hp, label)) = &enemy_filter {
            summary.push_str(&format!("\nFilter: {label} HP <= {hp}"));
        }
        for (power, filter) in &power_filters {
            let title = crate::presentation_names::name::<PowerId>(power.stable_id());
            let condition = filter.minimum_amount.map_or_else(
                || {
                    if filter.present {
                        "present".into()
                    } else {
                        "absent".into()
                    }
                },
                |amount| format!(">= {amount}"),
            );
            summary.push_str(&format!("\nFilter: {title} {condition}"));
        }
        if request.limit != 0 {
            summary.push_str(&format!(
                "\n{} shown / {} matching",
                options.len(),
                matching.len()
            ));
        } else if request.outcome.is_some()
            || request.minimum_hp.is_some()
            || request.minimum_block.is_some()
            || request.enemy_hp_at_most.is_some()
            || !request.player_powers.is_empty()
        {
            summary.push_str(&format!("\nMatching: {}", matching.len()));
        }
        for option in &options {
            summary.push_str(&format!("\n{}", option.label));
        }
        Ok(TurnResearchPageV1 {
            after: self.prefix.as_ref().map(|p| p.label.clone()),
            hp_range,
            block_range,
            options,
            total: handles.len(),
            outcomes,
            outcome: request.outcome,
            matching: matching.len(),
            next_offset: (request.limit != 0 && next < matching.len()).then_some(next),
            coverage: coverage.into(),
            generation_work: counters.generation_work,
            engine_steps: counters.engine_steps,
            potion_slots: request.potion_slots.clone(),
            minimum_hp: request.minimum_hp,
            minimum_block: request.minimum_block,
            enemy_hp_at_most: request.enemy_hp_at_most.clone(),
            player_powers: request.player_powers.clone(),
            summary,
        })
    }
}

fn resource_range(values: impl Iterator<Item = i32>) -> Option<TurnResourceRangeV1> {
    values.fold(None, |range, value| {
        Some(range.map_or(
            TurnResourceRangeV1 {
                minimum: value,
                maximum: value,
            },
            |range: TurnResourceRangeV1| TurnResourceRangeV1 {
                minimum: range.minimum.min(value),
                maximum: range.maximum.max(value),
            },
        ))
    })
}

fn power_changes(
    before: &[sts_oracle_run_control::agent::information::state::CombatLearningPowerV1],
    after: &[sts_oracle_run_control::agent::information::state::CombatLearningPowerV1],
) -> Vec<TurnPlayerPowerChangeV1> {
    let mut ids = before
        .iter()
        .chain(after)
        .map(|p| p.power)
        .collect::<Vec<_>>();
    ids.sort_by_key(|p| p.stable_id());
    ids.dedup();
    ids.into_iter()
        .filter_map(|power| {
            let before = before.iter().find(|p| p.power == power).map(|p| p.amount);
            let after = after.iter().find(|p| p.power == power).map(|p| p.amount);
            (before != after).then_some(TurnPlayerPowerChangeV1 {
                power,
                before,
                after,
            })
        })
        .collect()
}

pub(super) fn outcome_label(
    boundary: CompleteTurnOptionBoundary,
    state: &PublicCombatStateV1,
) -> String {
    let end = match boundary {
        CompleteTurnOptionBoundary::NextPlayerTurn => format!("Turn {}", state.turn.turn_count + 1),
        CompleteTurnOptionBoundary::TerminalWin => "Victory, before rewards".into(),
        CompleteTurnOptionBoundary::TerminalLoss => "Defeat".into(),
        CompleteTurnOptionBoundary::Escape => "Escape".into(),
    };
    let enemies = state
        .monsters
        .iter()
        .filter(|m| m.alive)
        .map(|m| format!("{} HP {}", monster_label(m), m.hp))
        .collect::<Vec<_>>();
    let potions = state
        .potions
        .iter()
        .enumerate()
        .filter_map(|(slot, p)| {
            p.as_ref().map(|p| {
                format!(
                    "P{slot} {}",
                    crate::presentation_names::name::<
                        sts_oracle_run_control::content::potions::PotionId,
                    >(p.potion_id.stable_id())
                )
            })
        })
        .collect::<Vec<_>>();
    format!(
        "{end}, HP {}/{}{}{}; Potions: {}",
        state.player.hp,
        state.player.max_hp,
        if state.player.block > 0 {
            format!(", Block {}", state.player.block)
        } else {
            String::new()
        },
        if enemies.is_empty() {
            String::new()
        } else {
            format!(", {}", enemies.join(", "))
        },
        if potions.is_empty() {
            "empty".into()
        } else {
            potions.join(", ")
        }
    )
}

pub(super) fn trial_action_label(selected: &CompleteTurnSelectedActionV1) -> String {
    use sts_oracle_run_control::agent::information::action::{
        PublicCombatAtomicActionV1 as A, PublicCombatSelectionDomainCandidateV1 as S,
    };
    use sts_oracle_run_control::content::{cards::CardId, potions::PotionId};
    match selected {
        CompleteTurnSelectedActionV1::Atomic {
            action,
            card,
            potion,
            target,
        } => match action {
            A::PlayCard {
                target_monster_index,
                ..
            } => {
                let title = card
                    .as_ref()
                    .map(|c| {
                        crate::manual::format_card(c.card_id.stable_id(), c.upgrades.into(), None)
                    })
                    .unwrap_or_else(|| "Play card".into());
                let destination = target
                    .as_ref()
                    .map(monster_label)
                    .or_else(|| target_monster_index.map(|index| format!("enemy index {index}")));
                destination.map_or(title.clone(), |target| format!("{title} @{target}"))
            }
            A::UsePotion {
                potion_index,
                target_monster_index,
            } => {
                let title = potion
                    .as_ref()
                    .map(|p| crate::presentation_names::name::<PotionId>(p.potion_id.stable_id()))
                    .unwrap_or_else(|| "Potion".into());
                let destination = target
                    .as_ref()
                    .map(|m| format!(" @{}", monster_label(m)))
                    .or_else(|| target_monster_index.map(|index| format!(" @enemy index {index}")))
                    .unwrap_or_default();
                format!("P{potion_index} {title}{destination}")
            }
            A::DiscardPotion { potion_index } => format!("Discard P{potion_index}"),
            A::EndTurn => "End turn".into(),
            A::SubmitIndexedChoice { choice_index } => {
                format!("Choose option {}", choice_index + 1)
            }
            A::Proceed => "Proceed".into(),
            A::Cancel => "Cancel".into(),
        },
        CompleteTurnSelectedActionV1::Selection {
            family,
            selected_domain_ordinals,
        } => {
            let cards = selected_domain_ordinals
                .iter()
                .map(|&i| match family.domain.get(i) {
                    Some(S::Card {
                        card_id: Some(id),
                        upgrades,
                        ..
                    }) => crate::manual::format_card(
                        id.stable_id(),
                        upgrades.unwrap_or_default().into(),
                        None,
                    ),
                    Some(S::Scry {
                        card_id: Some(id), ..
                    }) => crate::presentation_names::name::<CardId>(id.stable_id()),
                    _ => "Unknown card".into(),
                })
                .collect::<Vec<_>>();
            format!("Select {}", cards.join(", "))
        }
    }
}

pub(super) fn trial_line_label(actions: &[CompleteTurnSelectedActionV1]) -> String {
    let mut label = actions
        .iter()
        .take(6)
        .map(trial_action_label)
        .collect::<Vec<_>>()
        .join(" -> ");
    if actions.len() > 6 {
        label.push_str(&format!(" -> ... ({} actions)", actions.len()));
    }
    label
}

fn enemy_label(enemy: sts_oracle_run_control::content::monsters::EnemyId, slot: u8) -> String {
    format!(
        "{} (M{slot})",
        crate::presentation_names::name::<sts_oracle_run_control::content::monsters::EnemyId>(
            enemy.stable_id()
        )
    )
}

fn monster_label(
    monster: &sts_oracle_run_control::agent::information::state::CombatLearningMonsterStateV1,
) -> String {
    let sts_oracle_run_control::agent::information::state::CombatLearningEnemyIdentityV1::Known {
        enemy_id,
    } = monster.enemy;
    enemy_label(enemy_id, monster.slot)
}
