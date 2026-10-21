//! Combat-side encoding for the bridge-owned semantic schema.

use sts_oracle_eval::agent::information::action::{
    PublicCombatIndexedChoiceCandidateV1, PublicCombatIndexedChoiceReasonV1,
};
use sts_oracle_eval::agent::information::combat::{
    HiddenInformationReasonV1, ObservationEvidenceKindV1,
};
use sts_oracle_eval::agent::information::run::PublicCombatRunContextV1;
use sts_oracle_eval::agent::information::state::{
    CombatLearningCardCollectionV1, CombatLearningCardV1, CombatLearningEnemyIdentityV1,
    CombatLearningMonsterPublicCounterV1, CombatLearningPowerV1,
};
use sts_oracle_eval::content::cards::get_card_definition;
use sts_oracle_eval::content::cards::CardType;
use sts_oracle_eval::content::powers::mechanics::typed_profile;
use sts_oracle_eval::runtime::action::CardDestination;
use sts_oracle_eval::runtime::combat::{CombatPhase, Intent, OrbId, StanceId};
use sts_oracle_eval::sim::combat_action_surface::{
    CombatIndexedChoiceInputEncodingV2, CombatSelectionDistinctByV2,
    CombatSelectionInputEncodingV2, CombatSelectionPayloadLanguageV2, CombatSelectionReasonV2,
};
use sts_oracle_eval::state::core::{GridSelectReason, HandSelectReason, PileType};
use sts_oracle_learning::eval::run_control::{
    LearningCombatAtomicActionV1, LearningCombatIndexedChoiceV1, LearningCombatModelObservationV1,
    LearningCombatMonsterV1, LearningCombatSelectionDomainSemanticsV1,
    LearningCombatSelectionFamilyV1, LearningModelCandidateSemanticsV1, LearningModelDecisionV1,
    LearningSelectionCandidateSemanticsV1, LearningSelectionDecisionV1, LearningSelectionDraftV1,
};

use super::{
    bool_value, CardZoneKind, CategoricalField, CombatActionKind, CounterItemKind,
    IndexedChoiceCandidateKind, IndexedChoiceReasonKind, IntentKind, PublicCounterKind,
    RelationKind, ScalarField, SelectionCandidateKind, SelectionDomainKind, SelectionReasonKind,
    SemanticBatchBuilder, SemanticEncodingError, TokenKind,
};

pub(super) struct CombatTokenIndex {
    pub(super) root: u64,
    hand: Vec<u64>,
    potions: Vec<u64>,
    pub(super) monsters: Vec<u64>,
}

impl SemanticBatchBuilder {
    pub(super) fn encode_combat_root(
        &mut self,
        observation: LearningCombatModelObservationV1<'_>,
        decision: &LearningModelDecisionV1<'_>,
    ) -> Result<(), SemanticEncodingError> {
        let tokens = self.encode_combat_observation(observation, true)?;
        for candidate in &decision.candidates {
            let token = self.begin_candidate()?;
            self.edge(tokens.root, RelationKind::ObservationHasCandidate, token);
            match candidate.semantics {
                LearningModelCandidateSemanticsV1::CombatAtomic { action } => {
                    self.encode_atomic_action(token, action, &tokens)?
                }
                LearningModelCandidateSemanticsV1::CombatSelectionFamily { family } => {
                    self.category(
                        token,
                        CategoricalField::CombatActionKind,
                        CombatActionKind::BeginSelection as i64,
                    );
                    self.encode_selection_family(
                        token,
                        RelationKind::CandidateHasSelectionDomain,
                        family,
                    )?;
                }
                LearningModelCandidateSemanticsV1::Strategic { .. } => {
                    return Err(SemanticEncodingError::UnsupportedCombatAtomicInput);
                }
            }
            self.finish_candidate(token)?;
        }
        Ok(())
    }

    pub(super) fn encode_combat_selection(
        &mut self,
        observation: LearningCombatModelObservationV1<'_>,
        draft: &LearningSelectionDraftV1,
        decision: &LearningSelectionDecisionV1,
    ) -> Result<(), SemanticEncodingError> {
        let tokens = self.encode_combat_observation(observation, true)?;
        let selection = self.add_token(TokenKind::SelectionState)?;
        self.edge(
            tokens.root,
            RelationKind::ObservationHasSelectionState,
            selection,
        );
        let domains = self.encode_selection_family(
            selection,
            RelationKind::SelectionHasDomain,
            draft
                .combat_family()
                .ok_or(SemanticEncodingError::SelectionObservationMismatch)?,
        )?;
        for (position, domain_index) in draft.selected_domain_indices().iter().copied().enumerate()
        {
            let target = domains
                .get(domain_index)
                .copied()
                .ok_or(SemanticEncodingError::MissingSelectionDomain(domain_index))?;
            let chosen = self.add_token(TokenKind::SelectionChosen)?;
            self.scalar(chosen, ScalarField::SelectionChosenPosition, position);
            self.edge(selection, RelationKind::SelectionHasChosen, chosen);
            self.edge(chosen, RelationKind::ChosenTargetsDomain, target);
        }

        for candidate in &decision.candidates {
            let token = self.begin_candidate()?;
            self.edge(tokens.root, RelationKind::ObservationHasCandidate, token);
            match candidate.semantics {
                LearningSelectionCandidateSemanticsV1::Submit => {
                    self.category(
                        token,
                        CategoricalField::SelectionCandidateKind,
                        SelectionCandidateKind::Submit as i64,
                    );
                }
                LearningSelectionCandidateSemanticsV1::Append { domain_index } => {
                    self.category(
                        token,
                        CategoricalField::SelectionCandidateKind,
                        SelectionCandidateKind::Append as i64,
                    );
                    let target = domains
                        .get(domain_index)
                        .copied()
                        .ok_or(SemanticEncodingError::MissingSelectionDomain(domain_index))?;
                    self.edge(token, RelationKind::CandidateTargets, target);
                }
            }
            self.finish_candidate(token)?;
        }
        Ok(())
    }

    pub(super) fn encode_combat_observation(
        &mut self,
        observation: LearningCombatModelObservationV1<'_>,
        include_turn_count: bool,
    ) -> Result<CombatTokenIndex, SemanticEncodingError> {
        let root = self.add_token(TokenKind::Observation)?;

        let encounter = self.add_token(TokenKind::CombatEncounter)?;
        self.edge(root, RelationKind::ObservationHasEncounter, encounter);
        self.category(
            encounter,
            CategoricalField::CombatIsBoss,
            bool_value(observation.encounter.is_boss_fight),
        );
        self.category(
            encounter,
            CategoricalField::CombatIsElite,
            bool_value(observation.encounter.is_elite_fight),
        );
        self.encode_combat_public_run_context(root, encounter, observation.public_run_context)?;

        for reason in observation.hidden_reasons {
            let token = self.add_token(TokenKind::CombatHiddenReason)?;
            self.edge(root, RelationKind::ObservationHasHiddenReason, token);
            self.category(token, CategoricalField::HiddenReason, *reason as i64);
        }

        let turn = self.add_token(TokenKind::CombatTurn)?;
        self.edge(root, RelationKind::ObservationHasTurn, turn);
        self.category(
            turn,
            CategoricalField::CombatPhase,
            observation.turn.phase as i64,
        );
        if include_turn_count {
            self.scalar(
                turn,
                ScalarField::CombatTurnCount,
                observation.turn.turn_count,
            );
        }
        self.scalar(turn, ScalarField::CombatEnergy, observation.turn.energy);
        self.scalar(
            turn,
            ScalarField::TurnStartDrawModifier,
            observation.turn.turn_start_draw_modifier,
        );
        self.encode_turn_counters(turn, &observation.turn.counters)?;

        let player = self.add_token(TokenKind::CombatPlayer)?;
        self.edge(root, RelationKind::ObservationHasPlayer, player);
        if let Some(player_class) = observation.player.player_class {
            self.category(player, CategoricalField::PlayerClass, player_class as i64);
        }
        self.category(
            player,
            CategoricalField::PlayerFacingLeft,
            bool_value(observation.player.facing_left),
        );
        self.category(
            player,
            CategoricalField::StanceId,
            observation.player.stance.stable_ordinal() as i64,
        );
        self.scalar(
            player,
            ScalarField::AscensionLevel,
            observation.player.ascension_level,
        );
        self.scalar(player, ScalarField::CombatPlayerHp, observation.player.hp);
        self.scalar(
            player,
            ScalarField::CombatPlayerMaxHp,
            observation.player.max_hp,
        );
        self.scalar(
            player,
            ScalarField::CombatPlayerBlock,
            observation.player.block,
        );
        self.scalar(
            player,
            ScalarField::CombatPlayerGold,
            observation.player.gold,
        );
        self.scalar(
            player,
            ScalarField::GoldDeltaThisCombat,
            observation.player.gold_delta_this_combat,
        );
        self.scalar(
            player,
            ScalarField::EnergyMaster,
            observation.player.energy_master,
        );
        self.scalar(player, ScalarField::MaxOrbs, observation.player.max_orbs);

        for (position, relic) in observation.player.relics.iter().enumerate() {
            let token = self.add_token(TokenKind::Relic)?;
            self.edge(player, RelationKind::PlayerHasRelic, token);
            self.relic_identity_with_mechanics(token, CategoricalField::RelicId, relic.id);
            self.category(
                token,
                CategoricalField::RelicUsedUp,
                bool_value(relic.used_up),
            );
            self.scalar(token, ScalarField::CollectionPosition, position);
            self.scalar(token, ScalarField::RelicCounter, relic.counter);
            self.scalar(token, ScalarField::RelicAmount, relic.amount);
        }

        for (position, orb) in observation.player.orbs.iter().enumerate() {
            let token = self.add_token(TokenKind::CombatOrb)?;
            self.edge(player, RelationKind::PlayerHasOrb, token);
            self.category(
                token,
                CategoricalField::OrbId,
                orb.orb.stable_ordinal() as i64,
            );
            self.scalar(token, ScalarField::CollectionPosition, position);
            self.scalar(token, ScalarField::OrbBasePassive, orb.base_passive_amount);
            self.scalar(token, ScalarField::OrbBaseEvoke, orb.base_evoke_amount);
            self.scalar(token, ScalarField::OrbPassive, orb.passive_amount);
            self.scalar(token, ScalarField::OrbEvoke, orb.evoke_amount);
        }

        let mut monster_tokens = Vec::with_capacity(observation.monsters.len());
        for monster_index in 0..observation.monsters.len() {
            let monster = observation
                .monsters
                .get(monster_index)
                .expect("monster index came from the same observation length");
            let token = self.add_token(TokenKind::CombatMonster)?;
            monster_tokens.push(token);
            self.edge(root, RelationKind::ObservationHasMonster, token);
            self.encode_monster_basics(token, monster);
        }

        let mut on_play_powers = Vec::new();
        for (position, power) in observation.player.powers.iter().enumerate() {
            let token = self.encode_power(player, position, power, &monster_tokens)?;
            if let Some(profile) = typed_profile(power.power) {
                on_play_powers.push((token, profile));
            }
        }
        for (monster_index, token) in monster_tokens.iter().copied().enumerate() {
            let monster = observation
                .monsters
                .get(monster_index)
                .expect("monster token index came from the same observation length");
            self.encode_monster_details(token, monster)?;
            for (position, power) in monster.powers().iter().enumerate() {
                let power_token = self.encode_power(token, position, power, &monster_tokens)?;
                if monster.alive() {
                    if let Some(profile) = typed_profile(power.power) {
                        on_play_powers.push((power_token, profile));
                    }
                }
            }
        }

        let mut potions = Vec::with_capacity(observation.potions.len());
        for (slot, potion) in observation.potions.iter().enumerate() {
            let token = self.add_token(TokenKind::PotionSlot)?;
            potions.push(token);
            self.edge(root, RelationKind::ObservationHasPotionSlot, token);
            self.scalar(token, ScalarField::PotionSlot, slot);
            self.category(
                token,
                CategoricalField::PotionOccupied,
                bool_value(potion.is_some()),
            );
            if let Some(potion) = potion {
                self.potion_identity_with_mechanics(
                    token,
                    CategoricalField::PotionId,
                    potion.potion_id,
                );
                self.category(
                    token,
                    CategoricalField::PotionCanUse,
                    bool_value(potion.can_use),
                );
                self.category(
                    token,
                    CategoricalField::PotionCanDiscard,
                    bool_value(potion.can_discard),
                );
                self.category(
                    token,
                    CategoricalField::PotionRequiresTarget,
                    bool_value(potion.requires_target),
                );
            }
        }

        self.encode_card_zone(
            root,
            CardZoneKind::MasterDeck,
            &observation.cards.master_deck,
            &monster_tokens,
        )?;
        let hand = self.encode_card_zone(
            root,
            CardZoneKind::Hand,
            &observation.cards.hand,
            &monster_tokens,
        )?;
        for (card, token) in observation.cards.hand.cards.iter().zip(&hand) {
            let card_type = get_card_definition(card.card_id).card_type;
            for (power, profile) in &on_play_powers {
                if profile.triggered_by_card_type(card_type) {
                    self.edge(*token, RelationKind::CardTriggersPower, *power);
                }
            }
        }
        self.encode_card_zone(
            root,
            CardZoneKind::Draw,
            &observation.cards.draw,
            &monster_tokens,
        )?;
        self.encode_card_zone(
            root,
            CardZoneKind::Discard,
            &observation.cards.discard,
            &monster_tokens,
        )?;
        self.encode_card_zone(
            root,
            CardZoneKind::Exhaust,
            &observation.cards.exhaust,
            &monster_tokens,
        )?;
        self.encode_card_zone(
            root,
            CardZoneKind::Limbo,
            &observation.cards.limbo,
            &monster_tokens,
        )?;

        Ok(CombatTokenIndex {
            root,
            hand,
            potions,
            monsters: monster_tokens,
        })
    }

    fn encode_combat_public_run_context(
        &mut self,
        root: u64,
        encounter: u64,
        context: &PublicCombatRunContextV1,
    ) -> Result<(), SemanticEncodingError> {
        let PublicCombatRunContextV1::Available {
            run_goal,
            act,
            floor,
            keys,
            public_map,
            encounter_id,
            ..
        } = context
        else {
            return Ok(());
        };

        let run = self.add_token(TokenKind::Run)?;
        self.edge(root, RelationKind::ObservationHasRun, run);
        self.category(run, CategoricalField::RunGoal, *run_goal as i64);
        self.category(run, CategoricalField::RubyKey, bool_value(keys[0]));
        self.category(run, CategoricalField::EmeraldKey, bool_value(keys[1]));
        self.category(run, CategoricalField::SapphireKey, bool_value(keys[2]));
        self.scalar(run, ScalarField::Act, *act);
        self.scalar(run, ScalarField::Floor, *floor);
        if let Some(encounter_id) = encounter_id {
            self.category(
                encounter,
                CategoricalField::CombatEncounterId,
                encounter_id.stable_ordinal() as i64,
            );
        }
        self.encode_public_map(root, public_map)?;
        Ok(())
    }

    fn encode_turn_counters(
        &mut self,
        turn: u64,
        counters: &sts_oracle_eval::agent::information::state::CombatLearningTurnCountersV1,
    ) -> Result<(), SemanticEncodingError> {
        let token = self.add_token(TokenKind::CombatTurnCounters)?;
        self.edge(turn, RelationKind::TurnHasCounters, token);
        self.scalar(
            token,
            ScalarField::CardsPlayedThisTurn,
            counters.cards_played_this_turn,
        );
        self.scalar(
            token,
            ScalarField::AttacksPlayedThisTurn,
            counters.attacks_played_this_turn,
        );
        self.scalar(
            token,
            ScalarField::CardsDiscardedThisTurn,
            counters.cards_discarded_this_turn,
        );
        self.scalar(
            token,
            ScalarField::MantraGainedThisCombat,
            counters.mantra_gained_this_combat,
        );
        self.scalar(
            token,
            ScalarField::TimesDamagedThisCombat,
            counters.times_damaged_this_combat,
        );
        if let Some(cost) = counters.discovery_cost_for_turn {
            self.scalar(token, ScalarField::DiscoveryCostForTurn, cost);
        }
        self.encode_counter_cards(
            token,
            CounterItemKind::CardPlayedThisTurn,
            &counters.card_ids_played_this_turn,
        )?;
        self.encode_counter_cards(
            token,
            CounterItemKind::CardPlayedThisCombat,
            &counters.card_ids_played_this_combat,
        )?;
        self.encode_counter_orbs(
            token,
            CounterItemKind::OrbChanneledThisTurn,
            &counters.orbs_channeled_this_turn,
        )?;
        self.encode_counter_orbs(
            token,
            CounterItemKind::OrbChanneledThisCombat,
            &counters.orbs_channeled_this_combat,
        )?;
        Ok(())
    }

    fn encode_counter_cards(
        &mut self,
        counters: u64,
        kind: CounterItemKind,
        cards: &[sts_oracle_eval::content::cards::CardId],
    ) -> Result<(), SemanticEncodingError> {
        for (position, card) in cards.iter().copied().enumerate() {
            let token = self.add_token(TokenKind::CombatCounterItem)?;
            self.edge(counters, RelationKind::CountersHasItem, token);
            self.category(token, CategoricalField::CounterItemKind, kind as i64);
            self.card_identity_with_mechanics(token, CategoricalField::CardId, card, None);
            self.scalar(token, ScalarField::CollectionPosition, position);
        }
        Ok(())
    }

    fn encode_counter_orbs(
        &mut self,
        counters: u64,
        kind: CounterItemKind,
        orbs: &[sts_oracle_eval::runtime::combat::OrbId],
    ) -> Result<(), SemanticEncodingError> {
        for (position, orb) in orbs.iter().copied().enumerate() {
            let token = self.add_token(TokenKind::CombatCounterItem)?;
            self.edge(counters, RelationKind::CountersHasItem, token);
            self.category(token, CategoricalField::CounterItemKind, kind as i64);
            self.category(token, CategoricalField::OrbId, orb.stable_ordinal() as i64);
            self.scalar(token, ScalarField::CollectionPosition, position);
        }
        Ok(())
    }

    fn encode_monster_basics(&mut self, token: u64, monster: LearningCombatMonsterV1<'_>) {
        match monster.enemy() {
            CombatLearningEnemyIdentityV1::Known { enemy_id } => {
                self.category(
                    token,
                    CategoricalField::EnemyId,
                    enemy_id.stable_ordinal() as i64,
                );
            }
        }
        self.scalar(token, ScalarField::MonsterSlot, monster.slot());
        self.scalar(token, ScalarField::MonsterHp, monster.hp());
        self.scalar(token, ScalarField::MonsterMaxHp, monster.max_hp());
        self.scalar(token, ScalarField::MonsterBlock, monster.block());
        self.category(
            token,
            CategoricalField::MonsterAlive,
            bool_value(monster.alive()),
        );
        self.category(
            token,
            CategoricalField::MonsterEscaped,
            bool_value(monster.escaped()),
        );
        self.category(
            token,
            CategoricalField::MonsterDying,
            bool_value(monster.dying()),
        );
        self.category(
            token,
            CategoricalField::MonsterHalfDead,
            bool_value(monster.half_dead()),
        );
    }

    fn encode_monster_details(
        &mut self,
        token: u64,
        monster: LearningCombatMonsterV1<'_>,
    ) -> Result<(), SemanticEncodingError> {
        let intent = self.add_token(TokenKind::CombatIntent)?;
        self.edge(token, RelationKind::MonsterHasIntent, intent);
        self.category(
            intent,
            CategoricalField::EvidenceKind,
            monster.intent().evidence as i64,
        );
        if let Some(hidden_reason) = monster.intent().hidden_reason {
            self.category(intent, CategoricalField::HiddenReason, hidden_reason as i64);
        }
        if let Some(preview) = monster.intent().preview_damage_per_hit {
            self.scalar(intent, ScalarField::IntentPreviewDamagePerHit, preview);
        }
        if let Some(value) = &monster.intent().intent {
            self.encode_intent(intent, value);
        }

        let history = self.add_token(TokenKind::CombatMoveHistory)?;
        self.edge(token, RelationKind::MonsterHasMoveHistory, history);
        self.category(
            history,
            CategoricalField::EvidenceKind,
            monster.executed_moves().evidence as i64,
        );
        for (position, move_id) in monster
            .executed_moves()
            .move_ids
            .iter()
            .copied()
            .enumerate()
        {
            let item = self.add_token(TokenKind::CombatMove)?;
            self.edge(history, RelationKind::HistoryHasMove, item);
            self.scalar(item, ScalarField::CollectionPosition, position);
            self.scalar(item, ScalarField::MoveId, move_id);
        }

        for counter in monster.public_counters() {
            let item = self.add_token(TokenKind::CombatPublicCounter)?;
            self.edge(token, RelationKind::MonsterHasPublicCounter, item);
            match counter {
                CombatLearningMonsterPublicCounterV1::HexaghostActiveOrbs { count } => {
                    self.category(
                        item,
                        CategoricalField::PublicCounterKind,
                        PublicCounterKind::HexaghostActiveOrbs as i64,
                    );
                    self.scalar(item, ScalarField::PublicCounterValue, *count);
                }
                CombatLearningMonsterPublicCounterV1::StolenGold { amount } => {
                    self.category(
                        item,
                        CategoricalField::PublicCounterKind,
                        PublicCounterKind::StolenGold as i64,
                    );
                    self.scalar(item, ScalarField::PublicCounterValue, *amount);
                }
            }
        }
        Ok(())
    }

    fn encode_intent(&mut self, token: u64, intent: &Intent) {
        let (kind, attack) = match intent {
            Intent::Attack { damage, hits } => (IntentKind::Attack, Some((*damage, *hits))),
            Intent::AttackBuff { damage, hits } => (IntentKind::AttackBuff, Some((*damage, *hits))),
            Intent::AttackDebuff { damage, hits } => {
                (IntentKind::AttackDebuff, Some((*damage, *hits)))
            }
            Intent::AttackDefend { damage, hits } => {
                (IntentKind::AttackDefend, Some((*damage, *hits)))
            }
            Intent::Buff => (IntentKind::Buff, None),
            Intent::Heal { amount } => {
                if let Some(amount) = amount {
                    self.scalar(token, ScalarField::IntentHealAmount, *amount);
                }
                (IntentKind::Heal, None)
            }
            Intent::Debuff => (IntentKind::Debuff, None),
            Intent::StrongDebuff => (IntentKind::StrongDebuff, None),
            Intent::Debug => (IntentKind::Debug, None),
            Intent::Defend => (IntentKind::Defend, None),
            Intent::DefendDebuff => (IntentKind::DefendDebuff, None),
            Intent::DefendBuff => (IntentKind::DefendBuff, None),
            Intent::Escape => (IntentKind::Escape, None),
            Intent::Magic => (IntentKind::Magic, None),
            Intent::None => (IntentKind::None, None),
            Intent::Sleep => (IntentKind::Sleep, None),
            Intent::Stun => (IntentKind::Stun, None),
            Intent::Unknown => (IntentKind::Unknown, None),
        };
        self.category(token, CategoricalField::IntentKind, kind as i64);
        if let Some((damage, hits)) = attack {
            self.scalar(token, ScalarField::IntentDamage, damage);
            self.scalar(token, ScalarField::IntentHits, hits);
        }
    }

    fn encode_power(
        &mut self,
        owner: u64,
        position: usize,
        power: &CombatLearningPowerV1,
        monsters: &[u64],
    ) -> Result<u64, SemanticEncodingError> {
        let token = self.add_token(TokenKind::CombatPower)?;
        self.edge(owner, RelationKind::EntityHasPower, token);
        self.power_identity_with_mechanics(token, power.power)?;
        self.category(
            token,
            CategoricalField::PowerJustApplied,
            bool_value(power.just_applied),
        );
        self.scalar(token, ScalarField::CollectionPosition, position);
        self.scalar(token, ScalarField::PowerAmount, power.amount);
        self.scalar(token, ScalarField::PowerExtraData, power.extra_data);
        if let Some(card) = &power.payload_card {
            let payload = self.encode_combat_card(card, None, monsters)?;
            self.edge(token, RelationKind::PowerHasPayloadCard, payload);
        }
        Ok(token)
    }

    fn encode_card_zone(
        &mut self,
        root: u64,
        kind: CardZoneKind,
        collection: &CombatLearningCardCollectionV1,
        monsters: &[u64],
    ) -> Result<Vec<u64>, SemanticEncodingError> {
        let zone = self.add_token(TokenKind::CombatCardZone)?;
        self.edge(root, RelationKind::ObservationHasCardZone, zone);
        self.category(zone, CategoricalField::CardZoneKind, kind as i64);
        self.category(
            zone,
            CategoricalField::EvidenceKind,
            collection.evidence as i64,
        );
        let position_is_semantic = !matches!(kind, CardZoneKind::Hand)
            && matches!(
                collection.evidence,
                ObservationEvidenceKindV1::VisibleExact
                    | ObservationEvidenceKindV1::PublicOrderedCollection
            );
        let mut tokens = Vec::with_capacity(collection.cards.len());
        for (position, card) in collection.cards.iter().enumerate() {
            let card =
                self.encode_combat_card(card, position_is_semantic.then_some(position), monsters)?;
            self.edge(zone, RelationKind::ZoneHasCard, card);
            tokens.push(card);
        }
        Ok(tokens)
    }

    pub(super) fn encode_combat_card(
        &mut self,
        card: &CombatLearningCardV1,
        position: Option<usize>,
        monsters: &[u64],
    ) -> Result<u64, SemanticEncodingError> {
        let token = self.add_token(TokenKind::CombatCard)?;
        self.card_identity_with_mechanics(
            token,
            CategoricalField::CardId,
            card.card_id,
            Some(card.upgrades),
        );
        self.scalar(token, ScalarField::CardUpgrades, card.upgrades);
        self.scalar(token, ScalarField::CardMiscValue, card.misc_value);
        if let Some(value) = card.base_damage_override {
            self.scalar(token, ScalarField::CardBaseDamageOverride, value);
        }
        if let Some(value) = card.base_block_override {
            self.scalar(token, ScalarField::CardBaseBlockOverride, value);
        }
        self.scalar(token, ScalarField::CardCostModifier, card.cost_modifier);
        if let Some(value) = card.cost_for_turn {
            self.scalar(token, ScalarField::CardCostForTurn, value);
        }
        self.scalar(token, ScalarField::CardEffectiveCost, card.effective_cost);
        self.scalar(token, ScalarField::CardCurrentDamage, card.current_damage);
        self.scalar(token, ScalarField::CardCurrentBlock, card.current_block);
        self.scalar(
            token,
            ScalarField::CardCurrentMagicNumber,
            card.current_magic_number,
        );
        if let Some(value) = card.exhaust_override {
            self.category(
                token,
                CategoricalField::CardExhaustOverride,
                bool_value(value),
            );
        }
        if let Some(value) = card.retain_override {
            self.category(
                token,
                CategoricalField::CardRetainOverride,
                bool_value(value),
            );
        }
        self.category(
            token,
            CategoricalField::CardFreeToPlay,
            bool_value(card.free_to_play_once),
        );
        self.scalar(token, ScalarField::CardEnergyOnUse, card.energy_on_use);
        if let Some(position) = position {
            self.scalar(token, ScalarField::CollectionPosition, position);
        }
        for (monster_order, damage) in card.damage_by_monster_order.iter().copied().enumerate() {
            let target = monsters.get(monster_order).copied().ok_or(
                SemanticEncodingError::MissingDamageProjectionMonster(monster_order),
            )?;
            let projection = self.add_token(TokenKind::CombatDamageProjection)?;
            self.edge(token, RelationKind::CardHasDamageProjection, projection);
            self.edge(projection, RelationKind::DamageTargetsMonster, target);
            self.scalar(projection, ScalarField::CollectionPosition, monster_order);
            self.scalar(projection, ScalarField::DamageProjectionValue, damage);
        }
        Ok(token)
    }

    pub(super) fn encode_atomic_action(
        &mut self,
        token: u64,
        action: LearningCombatAtomicActionV1<'_>,
        tokens: &CombatTokenIndex,
    ) -> Result<(), SemanticEncodingError> {
        match action {
            LearningCombatAtomicActionV1::PlayCard {
                hand_index,
                target_monster_index,
            } => {
                self.combat_action_kind(token, CombatActionKind::PlayCard);
                let card = tokens
                    .hand
                    .get(hand_index)
                    .copied()
                    .ok_or(SemanticEncodingError::MissingCombatHandCard(hand_index))?;
                self.edge(token, RelationKind::CandidateTargets, card);
                self.encode_monster_target(token, target_monster_index, &tokens.monsters)?;
            }
            LearningCombatAtomicActionV1::UsePotion {
                potion_index,
                target_monster_index,
            } => {
                self.combat_action_kind(token, CombatActionKind::UsePotion);
                self.scalar(token, ScalarField::ActionIndex, potion_index);
                let potion = tokens
                    .potions
                    .get(potion_index)
                    .copied()
                    .ok_or(SemanticEncodingError::MissingCombatPotionSlot(potion_index))?;
                self.edge(token, RelationKind::CandidateTargets, potion);
                self.encode_monster_target(token, target_monster_index, &tokens.monsters)?;
            }
            LearningCombatAtomicActionV1::DiscardPotion { potion_index } => {
                self.combat_action_kind(token, CombatActionKind::DiscardPotion);
                self.scalar(token, ScalarField::ActionIndex, potion_index);
                let potion = tokens
                    .potions
                    .get(potion_index)
                    .copied()
                    .ok_or(SemanticEncodingError::MissingCombatPotionSlot(potion_index))?;
                self.edge(token, RelationKind::CandidateTargets, potion);
            }
            LearningCombatAtomicActionV1::EndTurn => {
                self.combat_action_kind(token, CombatActionKind::EndTurn);
            }
            LearningCombatAtomicActionV1::SubmitIndexedChoice {
                choice_index,
                indexed,
            } => {
                self.combat_action_kind(token, CombatActionKind::SubmitIndexedChoice);
                self.scalar(token, ScalarField::ActionIndex, choice_index);
                self.encode_indexed_choice(token, indexed);
            }
            LearningCombatAtomicActionV1::Proceed => {
                self.combat_action_kind(token, CombatActionKind::Proceed);
            }
            LearningCombatAtomicActionV1::Cancel => {
                self.combat_action_kind(token, CombatActionKind::Cancel);
            }
        }
        Ok(())
    }

    pub(super) fn encode_monster_target(
        &mut self,
        candidate: u64,
        target: Option<usize>,
        monsters: &[u64],
    ) -> Result<(), SemanticEncodingError> {
        if let Some(target) = target {
            let token = monsters
                .get(target)
                .copied()
                .ok_or(SemanticEncodingError::MissingCombatMonsterTarget(target))?;
            self.edge(candidate, RelationKind::CandidateTargets, token);
        }
        Ok(())
    }

    fn encode_indexed_choice(&mut self, token: u64, indexed: LearningCombatIndexedChoiceV1<'_>) {
        self.category(
            token,
            CategoricalField::IndexedChoiceInputEncoding,
            indexed.input_encoding as i64,
        );
        match indexed.reason {
            PublicCombatIndexedChoiceReasonV1::Discovery {
                colorless,
                card_type,
                amount,
            } => {
                self.indexed_reason(token, IndexedChoiceReasonKind::Discovery);
                self.category(
                    token,
                    CategoricalField::IndexedChoiceColorless,
                    bool_value(*colorless),
                );
                if let Some(card_type) = card_type {
                    self.category(
                        token,
                        CategoricalField::IndexedChoiceCardType,
                        *card_type as i64,
                    );
                }
                self.scalar(token, ScalarField::IndexedChoiceAmount, *amount);
            }
            PublicCombatIndexedChoiceReasonV1::CardReward { destination } => {
                self.indexed_reason(token, IndexedChoiceReasonKind::CardReward);
                self.category(
                    token,
                    CategoricalField::IndexedChoiceDestination,
                    *destination as i64,
                );
            }
            PublicCombatIndexedChoiceReasonV1::ForeignInfluence { upgraded } => {
                self.indexed_reason(token, IndexedChoiceReasonKind::ForeignInfluence);
                self.category(
                    token,
                    CategoricalField::IndexedChoiceUpgraded,
                    bool_value(*upgraded),
                );
            }
            PublicCombatIndexedChoiceReasonV1::ChooseOne => {
                self.indexed_reason(token, IndexedChoiceReasonKind::ChooseOne);
            }
            PublicCombatIndexedChoiceReasonV1::Stance => {
                self.indexed_reason(token, IndexedChoiceReasonKind::Stance);
            }
        }
        match indexed.candidate {
            PublicCombatIndexedChoiceCandidateV1::Card { card_id, upgrades } => {
                self.category(
                    token,
                    CategoricalField::IndexedChoiceCandidateKind,
                    IndexedChoiceCandidateKind::Card as i64,
                );
                self.action_card(token, *card_id, *upgrades);
            }
            PublicCombatIndexedChoiceCandidateV1::Stance { stance } => {
                self.category(
                    token,
                    CategoricalField::IndexedChoiceCandidateKind,
                    IndexedChoiceCandidateKind::Stance as i64,
                );
                self.category(
                    token,
                    CategoricalField::StanceId,
                    stance.stable_ordinal() as i64,
                );
            }
        }
    }

    pub(super) fn encode_selection_family(
        &mut self,
        owner: u64,
        domain_relation: RelationKind,
        family: LearningCombatSelectionFamilyV1<'_>,
    ) -> Result<Vec<u64>, SemanticEncodingError> {
        self.category(
            owner,
            CategoricalField::SelectionInputEncoding,
            family.input_encoding() as i64,
        );
        if let Some(source_pile) = family.source_pile() {
            self.category(
                owner,
                CategoricalField::SelectionSourcePile,
                source_pile as i64,
            );
        }
        self.encode_selection_reason(owner, family.reason());
        let CombatSelectionPayloadLanguageV2::OrderedDistinctSequence(distinct_by) =
            family.payload_language();
        self.category(
            owner,
            CategoricalField::SelectionPayloadDistinctBy,
            distinct_by as i64,
        );
        self.scalar(
            owner,
            ScalarField::SelectionRawDomainCount,
            family.raw_domain_count(),
        );
        self.scalar(
            owner,
            ScalarField::SelectionEligibleDomainCount,
            family.eligible_domain_count(),
        );
        self.scalar(
            owner,
            ScalarField::SelectionMaxDistinctCount,
            family.max_distinct_selection_count(),
        );
        self.scalar(
            owner,
            ScalarField::SelectionDeclaredMin,
            family.declared_min(),
        );
        self.scalar(
            owner,
            ScalarField::SelectionDeclaredMax,
            family.declared_max(),
        );
        self.scalar(
            owner,
            ScalarField::SelectionEffectiveMax,
            family.effective_max(),
        );

        let mut domains = Vec::with_capacity(family.domain_count());
        for domain_index in 0..family.domain_count() {
            let domain = family
                .domain(domain_index)
                .ok_or(SemanticEncodingError::MissingSelectionDomain(domain_index))?;
            let token = self.add_token(TokenKind::SelectionDomain)?;
            self.edge(owner, domain_relation, token);
            match domain.semantics() {
                LearningCombatSelectionDomainSemanticsV1::Card {
                    ordinal,
                    card_id,
                    upgrades,
                    eligible,
                } => {
                    self.category(
                        token,
                        CategoricalField::SelectionDomainKind,
                        SelectionDomainKind::Card as i64,
                    );
                    self.category(
                        token,
                        CategoricalField::SelectionDomainEligible,
                        bool_value(eligible),
                    );
                    self.scalar(token, ScalarField::SelectionDomainAddress, ordinal);
                    if let Some(card_id) = card_id {
                        self.card_identity_with_mechanics(
                            token,
                            CategoricalField::CardId,
                            card_id,
                            upgrades,
                        );
                    }
                    if let Some(upgrades) = upgrades {
                        self.scalar(token, ScalarField::CardUpgrades, upgrades);
                    }
                }
                LearningCombatSelectionDomainSemanticsV1::Scry {
                    index,
                    card_id,
                    currently_present,
                } => {
                    self.category(
                        token,
                        CategoricalField::SelectionDomainKind,
                        SelectionDomainKind::Scry as i64,
                    );
                    self.category(
                        token,
                        CategoricalField::SelectionDomainEligible,
                        bool_value(currently_present),
                    );
                    self.scalar(token, ScalarField::SelectionDomainAddress, index);
                    if let Some(card_id) = card_id {
                        self.card_identity_with_mechanics(
                            token,
                            CategoricalField::CardId,
                            card_id,
                            None,
                        );
                    }
                }
            }
            domains.push(token);
        }
        Ok(domains)
    }

    fn encode_selection_reason(&mut self, token: u64, reason: &CombatSelectionReasonV2) {
        let kind = match reason {
            CombatSelectionReasonV2::Hand(reason) => match reason {
                HandSelectReason::Exhaust => SelectionReasonKind::HandExhaust,
                HandSelectReason::PotionExhaust {
                    has_pending_use_effect,
                } => {
                    self.category(
                        token,
                        CategoricalField::SelectionReasonFlag,
                        bool_value(*has_pending_use_effect),
                    );
                    SelectionReasonKind::HandPotionExhaust
                }
                HandSelectReason::Discard => SelectionReasonKind::HandDiscard,
                HandSelectReason::Retain => SelectionReasonKind::HandRetain,
                HandSelectReason::PutOnDrawPile => SelectionReasonKind::HandPutOnDrawPile,
                HandSelectReason::PutToBottomOfDraw => SelectionReasonKind::HandPutToBottomOfDraw,
                HandSelectReason::Setup => SelectionReasonKind::HandSetup,
                HandSelectReason::Copy { amount } => {
                    self.scalar(token, ScalarField::SelectionReasonAmount, *amount);
                    SelectionReasonKind::HandCopy
                }
                HandSelectReason::Nightmare { amount } => {
                    self.scalar(token, ScalarField::SelectionReasonAmount, *amount);
                    SelectionReasonKind::HandNightmare
                }
                HandSelectReason::Upgrade => SelectionReasonKind::HandUpgrade,
                HandSelectReason::GamblingChip => SelectionReasonKind::HandGamblingChip,
                HandSelectReason::Recycle => SelectionReasonKind::HandRecycle,
            },
            CombatSelectionReasonV2::Grid(reason) => match reason {
                GridSelectReason::MoveToDrawPile => SelectionReasonKind::GridMoveToDrawPile,
                GridSelectReason::Exhume { upgrade } => {
                    self.category(
                        token,
                        CategoricalField::SelectionReasonFlag,
                        bool_value(*upgrade),
                    );
                    SelectionReasonKind::GridExhume
                }
                GridSelectReason::DrawPileToHand => SelectionReasonKind::GridDrawPileToHand,
                GridSelectReason::SkillFromDeckToHand => {
                    SelectionReasonKind::GridSkillFromDeckToHand
                }
                GridSelectReason::AttackFromDeckToHand => {
                    SelectionReasonKind::GridAttackFromDeckToHand
                }
                GridSelectReason::DiscardToHand => SelectionReasonKind::GridDiscardToHand,
                GridSelectReason::DiscardToHandNoCostChange => {
                    SelectionReasonKind::GridDiscardToHandNoCostChange
                }
                GridSelectReason::DiscardToHandRetain => {
                    SelectionReasonKind::GridDiscardToHandRetain
                }
                GridSelectReason::Omniscience { play_amount } => {
                    self.scalar(token, ScalarField::SelectionReasonAmount, *play_amount);
                    SelectionReasonKind::GridOmniscience
                }
            },
            CombatSelectionReasonV2::ScryDiscard => SelectionReasonKind::ScryDiscard,
        };
        self.category(token, CategoricalField::SelectionReasonKind, kind as i64);
    }

    fn combat_action_kind(&mut self, token: u64, kind: CombatActionKind) {
        self.category(token, CategoricalField::CombatActionKind, kind as i64);
    }

    fn indexed_reason(&mut self, token: u64, kind: IndexedChoiceReasonKind) {
        self.category(
            token,
            CategoricalField::IndexedChoiceReasonKind,
            kind as i64,
        );
    }
}

const _: () = {
    assert!(CombatPhase::PlayerTurn as i64 == 0);
    assert!(CombatPhase::TurnTransition as i64 == 2);
    assert!(HiddenInformationReasonV1::RunicDome as i64 == 0);
    assert!(HiddenInformationReasonV1::DrawPileOrderHidden as i64 == 2);
    assert!(ObservationEvidenceKindV1::VisibleExact as i64 == 0);
    assert!(ObservationEvidenceKindV1::Hidden as i64 == 3);
    assert!(CardDestination::Hand as i64 == 0);
    assert!(CardDestination::DrawPileRandom as i64 == 1);
    assert!(CardType::Attack as i64 == 0);
    assert!(CardType::Curse as i64 == 4);
    assert!(OrbId::Empty.stable_ordinal() == 0);
    assert!(OrbId::Plasma.stable_ordinal() == 4);
    assert!(StanceId::Neutral.stable_ordinal() == 0);
    assert!(StanceId::Divinity.stable_ordinal() == 3);
    assert!(PileType::Draw as i64 == 0);
    assert!(PileType::MasterDeck as i64 == 5);
    assert!(CombatIndexedChoiceInputEncodingV2::SubmitDiscoverChoiceIndex as i64 == 0);
    assert!(CombatSelectionInputEncodingV2::SubmitSelectionHandCardLineages as i64 == 0);
    assert!(CombatSelectionInputEncodingV2::SubmitScryDiscardIndices as i64 == 2);
    assert!(CombatSelectionDistinctByV2::CardLineage as i64 == 0);
    assert!(CombatSelectionDistinctByV2::ScryIndexAndCardLineage as i64 == 1);
};
