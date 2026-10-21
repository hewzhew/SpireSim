use std::collections::BTreeMap;

use crate::runtime::combat::EphemeralCounters;

use super::types::*;

impl OracleAnalysisCombatScratchNavigationV1 {
    pub fn apply_to_cached(
        &self,
        source: &OracleAnalysisCombatScratchDecisionViewV1,
        cached: &OracleAnalysisCombatScratchDecisionViewV1,
    ) -> Result<OracleAnalysisCombatScratchDecisionViewV1, String> {
        if self.kind != ORACLE_ANALYSIS_COMBAT_SCRATCH_NAVIGATION_KIND {
            return Err("unsupported combat scratch navigation receipt".to_string());
        }
        if source.run_node_id != self.run_node_id
            || source.cursor_scratch_node_id != self.source_scratch_node_id
        {
            return Err(format!(
                "combat scratch navigation source mismatch: expected run/node {}/{}, got {}/{}",
                self.run_node_id,
                self.source_scratch_node_id,
                source.run_node_id,
                source.cursor_scratch_node_id
            ));
        }
        if cached.run_node_id != self.run_node_id
            || cached.cursor_scratch_node_id != self.cursor_scratch_node_id
        {
            return Err(format!(
                "combat scratch navigation cache mismatch: expected run/node {}/{}, got {}/{}",
                self.run_node_id,
                self.cursor_scratch_node_id,
                cached.run_node_id,
                cached.cursor_scratch_node_id
            ));
        }

        let mut result = cached.clone();
        result.scratch_node_count = self.scratch_node_count;
        result.parent_scratch_node_id = self.parent_scratch_node_id;
        Ok(result)
    }
}

impl OracleAnalysisCombatScratchDecisionDeltaV1 {
    pub fn between(
        base: &OracleAnalysisCombatScratchDecisionViewV1,
        result: &OracleAnalysisCombatScratchDecisionViewV1,
    ) -> Result<Self, String> {
        if base.run_node_id != result.run_node_id {
            return Err("combat scratch delta cannot cross run nodes".to_string());
        }
        if result.parent_scratch_node_id != Some(base.cursor_scratch_node_id) {
            return Err(format!(
                "combat scratch delta result node {} is not a child of base node {}",
                result.cursor_scratch_node_id, base.cursor_scratch_node_id
            ));
        }

        let (removed_potion_slots, potion_upserts) = potion_delta(&base.potions, &result.potions);
        let (monsters, monster_updates) = if base.monsters.len() == result.monsters.len()
            && base
                .monsters
                .iter()
                .zip(&result.monsters)
                .all(|(left, right)| left.monster_index == right.monster_index)
        {
            (
                None,
                base.monsters
                    .iter()
                    .zip(&result.monsters)
                    .filter(|(left, right)| left != right)
                    .map(|(left, right)| {
                        OracleAnalysisCombatScratchMonsterDeltaV1::between(left, right)
                    })
                    .collect(),
            )
        } else {
            (Some(result.monsters.clone()), Vec::new())
        };

        Ok(Self {
            kind: ORACLE_ANALYSIS_COMBAT_SCRATCH_DECISION_DELTA_KIND.to_string(),
            run_node_id: result.run_node_id,
            base_scratch_node_id: base.cursor_scratch_node_id,
            cursor_scratch_node_id: result.cursor_scratch_node_id,
            scratch_node_count: result.scratch_node_count,
            parent_scratch_node_id: result.parent_scratch_node_id,
            context: changed(&base.context, &result.context),
            terminal: changed(&base.terminal, &result.terminal),
            turn: changed(&base.turn, &result.turn),
            phase: changed(&base.phase, &result.phase),
            counters: (base.counters != result.counters).then(|| {
                OracleAnalysisCombatScratchCountersDeltaV1::between(
                    &base.counters,
                    &result.counters,
                )
            }),
            player: (base.player != result.player).then(|| {
                OracleAnalysisCombatScratchPlayerDeltaV1::between(&base.player, &result.player)
            }),
            hand: changed(&base.hand, &result.hand),
            draw_pile_top_first: OracleAnalysisCombatScratchSequenceDeltaV1::between(
                &base.draw_pile_top_first,
                &result.draw_pile_top_first,
            ),
            discard_pile: OracleAnalysisCombatScratchSequenceDeltaV1::between(
                &base.discard_pile,
                &result.discard_pile,
            ),
            exhaust_pile: OracleAnalysisCombatScratchSequenceDeltaV1::between(
                &base.exhaust_pile,
                &result.exhaust_pile,
            ),
            removed_potion_slots,
            potion_upserts,
            monsters,
            monster_updates,
            atomic_actions: changed(&base.atomic_actions, &result.atomic_actions),
            selection_families: changed(&base.selection_families, &result.selection_families),
        })
    }

    pub fn apply_to(
        &self,
        base: &OracleAnalysisCombatScratchDecisionViewV1,
    ) -> Result<OracleAnalysisCombatScratchDecisionViewV1, String> {
        if self.kind != ORACLE_ANALYSIS_COMBAT_SCRATCH_DECISION_DELTA_KIND {
            return Err("unsupported combat scratch decision delta".to_string());
        }
        if base.run_node_id != self.run_node_id
            || base.cursor_scratch_node_id != self.base_scratch_node_id
        {
            return Err(format!(
                "combat scratch delta base mismatch: expected run/node {}/{}, got {}/{}",
                self.run_node_id,
                self.base_scratch_node_id,
                base.run_node_id,
                base.cursor_scratch_node_id
            ));
        }

        let mut result = base.clone();
        result.cursor_scratch_node_id = self.cursor_scratch_node_id;
        result.scratch_node_count = self.scratch_node_count;
        result.parent_scratch_node_id = self.parent_scratch_node_id;
        apply_changed(&mut result.context, &self.context);
        apply_changed(&mut result.terminal, &self.terminal);
        apply_changed(&mut result.turn, &self.turn);
        apply_changed(&mut result.phase, &self.phase);
        if let Some(delta) = &self.counters {
            delta.apply_to(&mut result.counters);
        }
        if let Some(delta) = &self.player {
            delta.apply_to(&mut result.player);
        }
        apply_changed(&mut result.hand, &self.hand);
        if let Some(delta) = &self.draw_pile_top_first {
            result.draw_pile_top_first = delta.apply_to(&result.draw_pile_top_first)?;
        }
        if let Some(delta) = &self.discard_pile {
            result.discard_pile = delta.apply_to(&result.discard_pile)?;
        }
        if let Some(delta) = &self.exhaust_pile {
            result.exhaust_pile = delta.apply_to(&result.exhaust_pile)?;
        }
        apply_potion_delta(
            &mut result.potions,
            &self.removed_potion_slots,
            &self.potion_upserts,
        );
        if let Some(monsters) = &self.monsters {
            result.monsters.clone_from(monsters);
        } else {
            for update in &self.monster_updates {
                let monster = result
                    .monsters
                    .iter_mut()
                    .find(|monster| monster.monster_index == update.monster_index)
                    .ok_or_else(|| {
                        format!(
                            "combat scratch delta has no base monster {}",
                            update.monster_index
                        )
                    })?;
                update.apply_to(monster);
            }
        }
        apply_changed(&mut result.atomic_actions, &self.atomic_actions);
        apply_changed(&mut result.selection_families, &self.selection_families);
        Ok(result)
    }
}

impl<T: Clone + PartialEq> OracleAnalysisCombatScratchSequenceDeltaV1<T> {
    fn between(base: &[T], result: &[T]) -> Option<Self> {
        if base == result {
            return None;
        }
        let retain_prefix = base
            .iter()
            .zip(result)
            .take_while(|(left, right)| left == right)
            .count();
        let maximum_suffix = base
            .len()
            .saturating_sub(retain_prefix)
            .min(result.len().saturating_sub(retain_prefix));
        let retain_suffix = base
            .iter()
            .rev()
            .zip(result.iter().rev())
            .take(maximum_suffix)
            .take_while(|(left, right)| left == right)
            .count();
        let remove_count = base.len() - retain_prefix - retain_suffix;
        let insert_end = result.len() - retain_suffix;
        Some(Self {
            base_len: base.len(),
            retain_prefix,
            remove_count,
            insert: result[retain_prefix..insert_end].to_vec(),
            result_len: result.len(),
        })
    }

    fn apply_to(&self, base: &[T]) -> Result<Vec<T>, String> {
        if base.len() != self.base_len
            || self.retain_prefix > base.len()
            || self.remove_count > base.len().saturating_sub(self.retain_prefix)
        {
            return Err("combat scratch sequence delta base mismatch".to_string());
        }
        let mut result = Vec::with_capacity(self.result_len);
        result.extend_from_slice(&base[..self.retain_prefix]);
        result.extend(self.insert.iter().cloned());
        result.extend_from_slice(&base[self.retain_prefix + self.remove_count..]);
        if result.len() != self.result_len {
            return Err("combat scratch sequence delta result length mismatch".to_string());
        }
        Ok(result)
    }
}

impl OracleAnalysisCombatScratchCountersDeltaV1 {
    fn between(from: &EphemeralCounters, to: &EphemeralCounters) -> Self {
        Self {
            cards_played_this_turn: changed(
                &from.cards_played_this_turn,
                &to.cards_played_this_turn,
            ),
            attacks_played_this_turn: changed(
                &from.attacks_played_this_turn,
                &to.attacks_played_this_turn,
            ),
            cards_discarded_this_turn: changed(
                &from.cards_discarded_this_turn,
                &to.cards_discarded_this_turn,
            ),
            card_ids_played_this_turn: changed(
                &from.card_ids_played_this_turn,
                &to.card_ids_played_this_turn,
            ),
            card_ids_played_this_combat: changed(
                &from.card_ids_played_this_combat,
                &to.card_ids_played_this_combat,
            ),
            orbs_channeled_this_turn: changed(
                &from.orbs_channeled_this_turn,
                &to.orbs_channeled_this_turn,
            ),
            orbs_channeled_this_combat: changed(
                &from.orbs_channeled_this_combat,
                &to.orbs_channeled_this_combat,
            ),
            mantra_gained_this_combat: changed(
                &from.mantra_gained_this_combat,
                &to.mantra_gained_this_combat,
            ),
            times_damaged_this_combat: changed(
                &from.times_damaged_this_combat,
                &to.times_damaged_this_combat,
            ),
            victory_triggered: changed(&from.victory_triggered, &to.victory_triggered),
            discovery_cost_for_turn: changed(
                &from.discovery_cost_for_turn,
                &to.discovery_cost_for_turn,
            ),
            early_end_turn_pending: changed(
                &from.early_end_turn_pending,
                &to.early_end_turn_pending,
            ),
            skip_monster_turn_pending: changed(
                &from.skip_monster_turn_pending,
                &to.skip_monster_turn_pending,
            ),
            player_escaping: changed(&from.player_escaping, &to.player_escaping),
            escape_pending_reward: changed(&from.escape_pending_reward, &to.escape_pending_reward),
        }
    }

    fn apply_to(&self, target: &mut EphemeralCounters) {
        apply_changed(
            &mut target.cards_played_this_turn,
            &self.cards_played_this_turn,
        );
        apply_changed(
            &mut target.attacks_played_this_turn,
            &self.attacks_played_this_turn,
        );
        apply_changed(
            &mut target.cards_discarded_this_turn,
            &self.cards_discarded_this_turn,
        );
        apply_changed(
            &mut target.card_ids_played_this_turn,
            &self.card_ids_played_this_turn,
        );
        apply_changed(
            &mut target.card_ids_played_this_combat,
            &self.card_ids_played_this_combat,
        );
        apply_changed(
            &mut target.orbs_channeled_this_turn,
            &self.orbs_channeled_this_turn,
        );
        apply_changed(
            &mut target.orbs_channeled_this_combat,
            &self.orbs_channeled_this_combat,
        );
        apply_changed(
            &mut target.mantra_gained_this_combat,
            &self.mantra_gained_this_combat,
        );
        apply_changed(
            &mut target.times_damaged_this_combat,
            &self.times_damaged_this_combat,
        );
        apply_changed(&mut target.victory_triggered, &self.victory_triggered);
        apply_changed(
            &mut target.discovery_cost_for_turn,
            &self.discovery_cost_for_turn,
        );
        apply_changed(
            &mut target.early_end_turn_pending,
            &self.early_end_turn_pending,
        );
        apply_changed(
            &mut target.skip_monster_turn_pending,
            &self.skip_monster_turn_pending,
        );
        apply_changed(&mut target.player_escaping, &self.player_escaping);
        apply_changed(
            &mut target.escape_pending_reward,
            &self.escape_pending_reward,
        );
    }
}

impl OracleAnalysisCombatScratchPlayerDeltaV1 {
    fn between(
        from: &OracleAnalysisCombatScratchPlayerV1,
        to: &OracleAnalysisCombatScratchPlayerV1,
    ) -> Self {
        Self {
            current_hp: changed(&from.current_hp, &to.current_hp),
            max_hp: changed(&from.max_hp, &to.max_hp),
            block: changed(&from.block, &to.block),
            energy: changed(&from.energy, &to.energy),
            stance: changed(&from.stance, &to.stance),
            orbs: changed(&from.orbs, &to.orbs),
            relics: changed(&from.relics, &to.relics),
            powers: changed(&from.powers, &to.powers),
        }
    }

    fn apply_to(&self, target: &mut OracleAnalysisCombatScratchPlayerV1) {
        apply_changed(&mut target.current_hp, &self.current_hp);
        apply_changed(&mut target.max_hp, &self.max_hp);
        apply_changed(&mut target.block, &self.block);
        apply_changed(&mut target.energy, &self.energy);
        apply_changed(&mut target.stance, &self.stance);
        apply_changed(&mut target.orbs, &self.orbs);
        apply_changed(&mut target.relics, &self.relics);
        apply_changed(&mut target.powers, &self.powers);
    }
}

impl OracleAnalysisCombatScratchMonsterDeltaV1 {
    fn between(
        from: &OracleAnalysisCombatScratchDecisionMonsterV1,
        to: &OracleAnalysisCombatScratchDecisionMonsterV1,
    ) -> Self {
        Self {
            monster_index: to.monster_index,
            label: changed(&from.label, &to.label),
            current_hp: changed(&from.current_hp, &to.current_hp),
            max_hp: changed(&from.max_hp, &to.max_hp),
            block: changed(&from.block, &to.block),
            is_dying: changed(&from.is_dying, &to.is_dying),
            is_escaped: changed(&from.is_escaped, &to.is_escaped),
            half_dead: changed(&from.half_dead, &to.half_dead),
            planned_move_id: changed(&from.planned_move_id, &to.planned_move_id),
            planned_steps: changed(&from.planned_steps, &to.planned_steps),
            intent: changed(&from.intent, &to.intent),
            move_preview: changed(&from.move_preview, &to.move_preview),
            thief: changed(&from.thief, &to.thief),
            powers: changed(&from.powers, &to.powers),
        }
    }

    fn apply_to(&self, target: &mut OracleAnalysisCombatScratchDecisionMonsterV1) {
        apply_changed(&mut target.label, &self.label);
        apply_changed(&mut target.current_hp, &self.current_hp);
        apply_changed(&mut target.max_hp, &self.max_hp);
        apply_changed(&mut target.block, &self.block);
        apply_changed(&mut target.is_dying, &self.is_dying);
        apply_changed(&mut target.is_escaped, &self.is_escaped);
        apply_changed(&mut target.half_dead, &self.half_dead);
        apply_changed(&mut target.planned_move_id, &self.planned_move_id);
        apply_changed(&mut target.planned_steps, &self.planned_steps);
        apply_changed(&mut target.intent, &self.intent);
        apply_changed(&mut target.move_preview, &self.move_preview);
        apply_changed(&mut target.thief, &self.thief);
        apply_changed(&mut target.powers, &self.powers);
    }
}

fn potion_delta(
    from: &[OracleAnalysisCombatScratchDecisionPotionV1],
    to: &[OracleAnalysisCombatScratchDecisionPotionV1],
) -> (Vec<usize>, Vec<OracleAnalysisCombatScratchDecisionPotionV1>) {
    let from = from
        .iter()
        .map(|potion| (potion.potion_slot, potion))
        .collect::<BTreeMap<_, _>>();
    let to = to
        .iter()
        .map(|potion| (potion.potion_slot, potion))
        .collect::<BTreeMap<_, _>>();
    let removed = from
        .keys()
        .filter(|slot| !to.contains_key(slot))
        .copied()
        .collect();
    let upserts = to
        .into_iter()
        .filter(|(slot, potion)| from.get(slot).copied() != Some(*potion))
        .map(|(_, potion)| potion.clone())
        .collect();
    (removed, upserts)
}

fn apply_potion_delta(
    target: &mut Vec<OracleAnalysisCombatScratchDecisionPotionV1>,
    removed: &[usize],
    upserts: &[OracleAnalysisCombatScratchDecisionPotionV1],
) {
    let mut by_slot = target
        .drain(..)
        .map(|potion| (potion.potion_slot, potion))
        .collect::<BTreeMap<_, _>>();
    for slot in removed {
        by_slot.remove(slot);
    }
    for potion in upserts {
        by_slot.insert(potion.potion_slot, potion.clone());
    }
    target.extend(by_slot.into_values());
}

fn changed<T: Clone + PartialEq>(from: &T, to: &T) -> Option<T> {
    (from != to).then(|| to.clone())
}

fn apply_changed<T: Clone>(target: &mut T, changed: &Option<T>) {
    if let Some(value) = changed {
        target.clone_from(value);
    }
}
