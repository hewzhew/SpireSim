//! Bounded numerical combat experiments over one shared execution owner.

use std::sync::atomic::{AtomicU64, Ordering};

use serde::{Deserialize, Serialize};
use sts_core::content::cards::CardId;
use sts_core::content::monsters::{factory::EncounterId, EnemyId};
use sts_core::content::powers::PowerId;
use sts_core::content::relics::RelicId;
use sts_core::sim::combat::CombatPosition;
use sts_core::sim::combat_start::build_natural_combat_start;
use sts_core::state::{map::node::RoomType, run::RunState};

use crate::observation::{combat_observation, model_choices, CardCost, ModelChoice};
use crate::{
    CombatSearchSpace, CombatSearchState, Guidance, GuidanceBudget, NodeRef, SearchBudget,
    SearchConfig, SearchSession, StepStatus,
};

pub const SCHEMA: u32 = 1;
pub const CONTEXT_WIDTH: usize = 48;
pub const CARD_WIDTH: usize = 20;
pub const CARD_COUNT: usize = 11;
static NEXT_FRAME: AtomicU64 = AtomicU64::new(1);

#[derive(Clone, Copy, Debug, Serialize)]
pub enum LocalEnemy {
    JawWorm,
    Cultist,
    TwoLouse,
    ThreeLouse,
}

impl LocalEnemy {
    fn encounter(self) -> EncounterId {
        match self {
            Self::JawWorm => EncounterId::JawWorm,
            Self::Cultist => EncounterId::Cultist,
            Self::TwoLouse => EncounterId::TwoLouse,
            Self::ThreeLouse => EncounterId::ThreeLouse,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum LocalTask {
    Starter,
    DrawSelect,
    Entities,
}

impl LocalTask {
    pub fn schema(self) -> u32 {
        match self {
            Self::Starter => SCHEMA,
            Self::DrawSelect => crate::local_tactics::SCHEMA,
            Self::Entities => crate::entities::SCHEMA,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct NumericPosition {
    pub context: [f32; CONTEXT_WIDTH],
    /// Hand, draw (top first), discard, exhaust, limbo. Each instance is a row.
    pub cards: Vec<[f32; CARD_WIDTH]>,
    /// Request-local card row, or -1 for EndTurn. Every legal edge is retained.
    pub action_cards: Vec<i64>,
}

/// The encoder can also be used on a matching source in the independent tree.
/// It contains numerical facts, no tactical scoring, candidate merging or seed.
pub fn encode_starter(
    session: &SearchSession<CombatSearchSpace>,
    source: NodeRef,
) -> Result<NumericPosition, String> {
    let view = session.view(source).map_err(|e| format!("{e:?}"))?;
    let obs = combat_observation(view.state)?;
    if obs.player_class != "Ironclad"
        || obs.ascension != 20
        || obs.monsters.len() != 1
        || obs.relics.len() != 1
        || obs.relics[0].relic != RelicId::BurningBlood
        || obs.potions.iter().any(Option::is_some)
        || !obs.powers.is_empty()
        || !obs.orbs.is_empty()
        || !obs.selections.is_empty()
        || obs.assembling.is_some()
        || obs.indexed_choice_reason.is_some()
    {
        return Err("position is outside the numerical starter task".into());
    }
    let m = &obs.monsters[0];
    if !matches!(m.enemy, EnemyId::JawWorm | EnemyId::Cultist) {
        return Err("enemy is outside the numerical starter task".into());
    }
    let mut context = [0.0; CONTEXT_WIDTH];
    context[..11].copy_from_slice(&[
        obs.hp[0] as f32 / 75.0,
        obs.hp[1] as f32 / 75.0,
        obs.block as f32 / 50.0,
        obs.energy as f32 / 3.0,
        obs.energy_per_turn as f32 / 3.0,
        obs.turn as f32 / 20.0,
        m.hp[0] as f32 / 60.0,
        m.hp[1] as f32 / 60.0,
        m.block as f32 / 50.0,
        f32::from(m.enemy == EnemyId::JawWorm),
        f32::from(m.enemy == EnemyId::Cultist),
    ]);
    if let Some(intent) = &m.intent {
        context[11] = intent.damage_per_hit.unwrap_or(0) as f32 / 30.0;
        context[12] = intent.hits as f32 / 5.0;
        context[13] = intent.total_damage.unwrap_or(0) as f32 / 30.0;
        context[14 + intent.visible_intent as usize] = 1.0;
    }
    for p in &m.powers {
        let index = match p.power {
            PowerId::Strength => 32,
            PowerId::Ritual => 35,
            PowerId::Vulnerable => 38,
            _ => return Err("power is outside the numerical starter task".into()),
        };
        context[index] = p.amount.unwrap_or(0) as f32 / 10.0;
        context[index + 1] = f32::from(p.just_applied);
        context[index + 2] = p.extra as f32 / 10.0;
    }
    context[41..].copy_from_slice(&[
        obs.counters.played as f32 / 10.0,
        obs.counters.attacks as f32 / 10.0,
        obs.counters.discarded as f32 / 10.0,
        obs.counters.draw_modifier_next_turn as f32 / 5.0,
        obs.counters.damaged_this_combat as f32 / 10.0,
        obs.gold as f32 / 200.0,
        f32::from(obs.facing_left),
    ]);
    let mut cards = Vec::new();
    for (zone, pile) in [
        &obs.hand,
        &obs.draw_top_first,
        &obs.discard,
        &obs.exhaust,
        &obs.limbo,
    ]
    .into_iter()
    .enumerate()
    {
        for (position, c) in pile.iter().enumerate() {
            let mut row = [0.0; CARD_WIDTH];
            row[0] = 1.0;
            row[match c.card {
                CardId::Strike => 1,
                CardId::Defend => 2,
                CardId::Bash => 3,
                CardId::AscendersBane => 4,
                _ => return Err("card is outside the numerical starter task".into()),
            }] = 1.0;
            row[5 + zone] = 1.0;
            row[10] = position as f32 / CARD_COUNT as f32;
            row[11] = c.upgrades as f32;
            let cost = match c.cost {
                CardCost::Energy(n) => n,
                CardCost::Special("unplayable") => -2,
                _ => return Err("cost is outside the numerical starter task".into()),
            };
            row[12] = cost as f32 / 3.0;
            row[13] = c.combat_cost.unwrap_or(cost) as f32 / 3.0;
            row[14] = c.damage as f32 / 20.0;
            row[15] = c.block as f32 / 20.0;
            row[16] = c.magic as f32 / 5.0;
            row[17] = f32::from(c.free_once);
            row[18] = c.misc as f32 / 20.0;
            row[19] = c.damage_by_target.first().copied().unwrap_or(0) as f32 / 20.0;
            cards.push(row);
        }
    }
    if cards.len() != CARD_COUNT {
        return Err("starter task must preserve all eleven card instances".into());
    }
    let action_cards = model_choices(view.state, &view.actions)?
        .into_iter()
        .map(|a| match a {
            ModelChoice::Play { hand, target } if target.is_none_or(|t| t == 0) => Ok(hand as i64),
            ModelChoice::EndTurn {} => Ok(-1),
            _ => Err("action is outside the numerical starter task".into()),
        })
        .collect::<Result<Vec<_>, String>>()?;
    Ok(NumericPosition {
        context,
        cards,
        action_cards,
    })
}

pub struct NumericBatch<P = NumericPosition> {
    pub revision: u64,
    pub slots: Vec<usize>,
    pub positions: Vec<P>,
}

/// Chronological decisions from one replayed victory. Each ordinal belongs only
/// to the adjacent encoded position. This is not an executable decision frame.
pub struct NumericDemonstration<P = NumericPosition> {
    pub positions: Vec<P>,
    pub choices: Vec<usize>,
}

#[derive(Clone, Debug, Serialize)]
pub struct LocalOutcome {
    pub slot: usize,
    pub seed: u64,
    pub enemy: EncounterId,
    pub starting_hp: i32,
    pub kind: String,
    pub hp: i32,
    /// Explicit UsePotion inputs; automatic Fairy consumption is reflected in inventory.
    pub potion_uses: usize,
    pub potions: Vec<Option<sts_core::content::potions::PotionId>>,
    pub turn: u32,
    pub decisions: usize,
    pub engine_steps: usize,
    pub replay_engine_steps: usize,
    pub win_replayed: bool,
    pub services: usize,
    pub guidance_rows: usize,
    pub terminal_observations: usize,
    pub coverage_gaps: usize,
    pub policy_rollout_services: usize,
}

struct Episode {
    seed: u64,
    enemy: EncounterId,
    starting_hp: i32,
    root: CombatPosition,
    session: SearchSession<CombatSearchSpace>,
    source: NodeRef,
    outcome: Option<LocalOutcome>,
}

/// An ephemeral experimental population. It has no run owner, recovery, file IO,
/// policy, optimizer, external-model transport or Java submission authority.
pub struct LocalCombatBatch {
    task: LocalTask,
    deck: Option<Vec<(CardId, u8)>>,
    episodes: Vec<Episode>,
    revision: u64,
    max_decisions: usize,
    search: bool,
    max_services: usize,
    max_policy_rollout_steps: usize,
    poisoned: bool,
    specifications: Option<Vec<ConstructedCombat>>,
}

/// An explicit construction recipe. It carries no executable historical state.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ConstructedCombat {
    pub seed: u64,
    pub encounter: EncounterId,
    pub act: u8,
    pub room: RoomType,
    pub hp: i32,
    pub deck: Vec<(CardId, u8)>,
    pub relics: Vec<RelicId>,
    pub potions: Vec<sts_core::content::potions::PotionId>,
    pub max_potion_uses: Option<usize>,
}

impl ConstructedCombat {
    pub fn build(&self) -> Result<CombatPosition, String> {
        let mut run = self.build_run()?;
        let (engine, combat) = build_natural_combat_start(&mut run, self.encounter, self.room)?;
        Ok(CombatPosition::new(engine, combat))
    }

    /// Fresh explicit loadout, before any combat. Continuous experiments keep
    /// this owner instead of rebuilding it at each encounter.
    pub(crate) fn build_run(&self) -> Result<RunState, String> {
        if !(1..=4).contains(&self.act)
            || !(1..=75).contains(&self.hp)
            || self.deck.is_empty()
            || !matches!(
                self.room,
                RoomType::MonsterRoom | RoomType::MonsterRoomElite | RoomType::MonsterRoomBoss
            )
        {
            return Err("invalid constructed combat specification".into());
        }
        let mut run = RunState::new(self.seed, 20, true, "Ironclad");
        run.act_num = self.act;
        // Equip first, then set the explicit deck/HP intervention. On-equip
        // upgrades or removals must not silently change the declared recipe.
        for relic in &self.relics {
            if matches!(
                relic,
                RelicId::BottledFlame | RelicId::BottledLightning | RelicId::BottledTornado
            ) {
                return Err("bottled relic requires an explicit target contract".into());
            }
            if run.relics.iter().any(|r| r.id == *relic)
                || run
                    .obtain_relic(*relic, sts_core::state::core::EngineState::CombatPlayerTurn)
                    .is_some()
            {
                return Err("duplicate relic or pending on-equip choice in construction".into());
            }
        }
        // This is an explicit initial-state intervention, not a sequence of
        // card acquisitions. Obtain/remove hooks would change the recipe,
        // relic counters and resources (Eggs, Omamori, Ceramic Fish, Parasite).
        let mut deck = Vec::with_capacity(self.deck.len());
        for &(card, upgrades) in &self.deck {
            if upgrades > 0
                && !sts_core::content::cards::can_upgrade_card_once_for(card, upgrades - 1)
            {
                return Err("invalid constructed card or upgrade".into());
            }
            let mut instance = sts_core::runtime::combat::CombatCard::with_lineage(
                card,
                run.allocate_master_deck_lineage(),
            );
            instance.upgrades = upgrades;
            deck.push(instance);
        }
        run.master_deck = deck;
        run.dispatch_on_master_deck_change();
        for (i, potion) in self.potions.iter().enumerate() {
            if !run.obtain_potion(sts_core::content::potions::Potion::new(*potion, i as u32)) {
                return Err("constructed potion slots full".into());
            }
        }
        run.current_hp = self.hp;
        Ok(run)
    }
}

impl LocalCombatBatch {
    pub fn from_specifications(
        specifications: &[ConstructedCombat],
        max_decisions: usize,
        search_services: Option<usize>,
        max_policy_rollout_steps: usize,
    ) -> Result<Self, String> {
        if specifications.is_empty()
            || specifications.len() > 128
            || !(1..=512).contains(&max_decisions)
            || search_services.is_some_and(|n| !(1..=16_384).contains(&n))
            || max_policy_rollout_steps > 512
            || (search_services.is_none() && max_policy_rollout_steps != 0)
        {
            return Err("invalid constructed population budget".into());
        }
        let max_services = search_services.unwrap_or(max_decisions);
        let mut episodes = vec![];
        for spec in specifications {
            let root = spec.build()?;
            let space = spec.max_potion_uses.map_or_else(
                CombatSearchSpace::default,
                CombatSearchSpace::with_potion_limit,
            );
            let session = SearchSession::new(
                space,
                CombatSearchState::new(root.clone())?,
                SearchConfig {
                    max_policy_rollout_steps,
                    ..Default::default()
                },
                SearchBudget {
                    max_services,
                    max_engine_steps: max_services * 256,
                    max_nodes: max_services + 1,
                    max_depth: max_decisions,
                    deadline: None,
                    guidance: GuidanceBudget::default(),
                },
            )
            .map_err(|e| format!("{e:?}"))?;
            let source = session.root();
            crate::entities::encode(&session, source)?;
            episodes.push(Episode {
                seed: spec.seed,
                enemy: spec.encounter,
                starting_hp: spec.hp,
                root,
                session,
                source,
                outcome: None,
            });
        }
        Ok(Self {
            task: LocalTask::Entities,
            deck: None,
            episodes,
            revision: next_frame()?,
            max_decisions,
            search: search_services.is_some(),
            max_services,
            max_policy_rollout_steps,
            poisoned: false,
            specifications: Some(specifications.to_vec()),
        })
    }

    pub fn entity_frame(&self) -> Result<NumericBatch<crate::entities::EntityPosition>, String> {
        if self.task != LocalTask::Entities {
            return Err("task requires entity numerical schema".into());
        }
        self.frame_with(crate::entities::encode)
    }

    pub fn new(cases: &[(u64, LocalEnemy)], max_decisions: usize) -> Result<Self, String> {
        Self::with_starting_hp(cases, max_decisions, &vec![68; cases.len()])
    }

    /// Fresh, explicitly constructed HP intervention before combat initialization.
    pub fn with_starting_hp(
        cases: &[(u64, LocalEnemy)],
        max_decisions: usize,
        starting_hp: &[i32],
    ) -> Result<Self, String> {
        Self::configured(
            cases,
            max_decisions,
            starting_hp,
            LocalTask::Starter,
            None,
            0,
            None,
        )
    }

    /// Independent fixed-root search. Stops at the first replayed win or its
    /// declared work bound. Failure to find a win remains Unknown, never loss.
    pub fn search(
        cases: &[(u64, LocalEnemy)],
        max_decisions: usize,
        starting_hp: &[i32],
        max_services: usize,
        max_policy_rollout_steps: usize,
    ) -> Result<Self, String> {
        Self::configured(
            cases,
            max_decisions,
            starting_hp,
            LocalTask::Starter,
            Some(max_services),
            max_policy_rollout_steps,
            None,
        )
    }

    pub fn configured(
        cases: &[(u64, LocalEnemy)],
        max_decisions: usize,
        starting_hp: &[i32],
        task: LocalTask,
        search_services: Option<usize>,
        max_policy_rollout_steps: usize,
        deck: Option<&[(CardId, u8)]>,
    ) -> Result<Self, String> {
        if task == LocalTask::Entities {
            return Err("entity task requires explicit specifications".into());
        }
        if let Some(deck) = deck {
            if task != LocalTask::DrawSelect {
                return Err("custom deck requires the draw/select task".into());
            }
            crate::local_tactics::validate_deck(deck)?;
        }
        if search_services.is_some_and(|s| !(1..=16_384).contains(&s))
            || max_policy_rollout_steps > 512
            || (search_services.is_none() && max_policy_rollout_steps != 0)
        {
            return Err("invalid search service or policy continuation limit".into());
        }
        if cases.is_empty()
            || cases.len() > 128
            || !(1..=512).contains(&max_decisions)
            || starting_hp.len() != cases.len()
            || starting_hp.iter().any(|hp| !(1..=75).contains(hp))
        {
            return Err("invalid combat population or decision limit".into());
        }
        let max_services = search_services.unwrap_or(max_decisions);
        let mut episodes = Vec::new();
        for (&(seed, enemy), &hp) in cases.iter().zip(starting_hp) {
            let mut run = RunState::new(seed, 20, true, "Ironclad");
            run.current_hp = hp;
            if let Some(deck) = deck {
                // A declared fresh construction, never an imported root. Use typed
                // deck mutation so every duplicate has its own allocated lineage.
                let original: Vec<_> = run.master_deck.iter().map(|c| c.lineage).collect();
                for lineage in original {
                    run.remove_card_from_deck(lineage);
                }
                if !run.master_deck.is_empty() {
                    return Err("constructed deck removal failed".into());
                }
                for &(card, upgrades) in deck {
                    if !run.add_card_to_deck_with_upgrades(card, upgrades) {
                        return Err("constructed deck addition failed".into());
                    }
                }
            } else if task == LocalTask::DrawSelect {
                // Explicit deck intervention before normal combat initialization.
                for card in [
                    CardId::BurningPact,
                    CardId::Headbutt,
                    CardId::PommelStrike,
                    CardId::ShrugItOff,
                ] {
                    if !run.add_card_to_deck_with_upgrades(card, 0) {
                        return Err("constructed deck addition failed".into());
                    }
                }
            }
            let (engine, combat) =
                build_natural_combat_start(&mut run, enemy.encounter(), RoomType::MonsterRoom)?;
            let root = CombatPosition::new(engine, combat);
            let session = SearchSession::new(
                CombatSearchSpace::with_potion_limit(0),
                CombatSearchState::new(root.clone())?,
                SearchConfig {
                    max_policy_rollout_steps,
                    ..SearchConfig::default()
                },
                SearchBudget {
                    max_services,
                    max_engine_steps: max_services * 256,
                    max_nodes: max_services + 1,
                    max_depth: max_decisions,
                    deadline: None,
                    guidance: GuidanceBudget::default(),
                },
            )
            .map_err(|e| format!("{e:?}"))?;
            let source = session.root();
            match task {
                LocalTask::Starter => {
                    encode_starter(&session, source)?;
                }
                LocalTask::DrawSelect => {
                    crate::local_tactics::encode(&session, source)?;
                }
                LocalTask::Entities => unreachable!(),
            }
            episodes.push(Episode {
                seed,
                enemy: enemy.encounter(),
                starting_hp: hp,
                root,
                session,
                source,
                outcome: None,
            });
        }
        Ok(Self {
            task,
            deck: deck.map(<[_]>::to_vec),
            episodes,
            revision: next_frame()?,
            max_decisions,
            search: search_services.is_some(),
            max_services,
            max_policy_rollout_steps,
            poisoned: false,
            specifications: None,
        })
    }

    pub fn frame(&self) -> Result<NumericBatch, String> {
        if self.task != LocalTask::Starter {
            return Err("task requires numerical schema 2".into());
        }
        self.frame_with(encode_starter)
    }

    pub fn task(&self) -> LocalTask {
        self.task
    }

    /// Source-bound assistant reading of the same decisions as entity_frame.
    /// Observation and choice semantics remain owned by the shared projection.
    pub fn readings_json(&self) -> Result<String, String> {
        if self.task != LocalTask::Entities {
            return Err("structured readings require the entity task".into());
        }
        let frame = self.frame_with(|session, source| {
            let view = session.view(source).map_err(|e| format!("{e:?}"))?;
            Ok(serde_json::json!({
                "combat": combat_observation(view.state)?,
                "choices": model_choices(view.state, &view.actions)?
            }))
        })?;
        serde_json::to_string(&serde_json::json!({
            "revision": frame.revision, "slots": frame.slots, "readings": frame.positions
        }))
        .map_err(|e| e.to_string())
    }

    pub fn tactical_frame(
        &self,
    ) -> Result<NumericBatch<crate::local_tactics::TacticalPosition>, String> {
        if self.task != LocalTask::DrawSelect {
            return Err("task requires numerical schema 1".into());
        }
        self.frame_with(crate::local_tactics::encode)
    }

    fn frame_with<P>(
        &self,
        encode: impl Fn(&SearchSession<CombatSearchSpace>, NodeRef) -> Result<P, String>,
    ) -> Result<NumericBatch<P>, String> {
        if self.poisoned {
            return Err("population stopped after a core error".into());
        }
        let mut slots = Vec::new();
        let mut positions = Vec::new();
        for (slot, e) in self.episodes.iter().enumerate() {
            if e.outcome.is_none() {
                let position = e
                    .session
                    .view(e.source)
                    .map_err(|err| format!("{err:?}"))?
                    .state
                    .position();
                if self.task != LocalTask::Entities && card_count(position) != card_count(&e.root) {
                    return Err("constructed task changed its card instance count".into());
                }
                slots.push(slot);
                positions.push(encode(&e.session, e.source)?);
            }
        }
        Ok(NumericBatch {
            revision: self.revision,
            slots,
            positions,
        })
    }

    fn action_frame(&self) -> Result<NumericBatch<usize>, String> {
        if self.task == LocalTask::Entities {
            let frame = self.entity_frame()?;
            return Ok(NumericBatch {
                revision: frame.revision,
                slots: frame.slots,
                positions: frame.positions.iter().map(|p| p.actions.len()).collect(),
            });
        }
        self.frame_with(|session, source| {
            session
                .view(source)
                .map(|v| v.actions.len())
                .map_err(|e| format!("{e:?}"))
        })
    }

    /// Validate the complete source/choice batch before applying any guidance.
    /// A genuine core error seals the population; callers must retain evidence
    /// and abort collection rather than train on a partial failed batch.
    pub fn advance(&mut self, revision: u64, ordinals: &[usize]) -> Result<(), String> {
        if self.search {
            return Err("search population requires numerical guidance".into());
        }
        let frame = self.action_frame()?;
        if revision != self.revision || ordinals.len() != frame.slots.len() || ordinals.is_empty() {
            return Err("stale or incomplete numerical choice batch".into());
        }
        if ordinals
            .iter()
            .zip(&frame.positions)
            .any(|(&a, &count)| a >= count)
        {
            return Err("choice is outside its source".into());
        }
        self.revision = next_frame()?;
        for ((slot, p), &a) in frame.slots.into_iter().zip(frame.positions).zip(ordinals) {
            let result = self.advance_one(slot, a, p);
            if let Err(error) = result {
                self.poisoned = true;
                return Err(error);
            }
        }
        Ok(())
    }

    fn advance_one(&mut self, slot: usize, ordinal: usize, count: usize) -> Result<(), String> {
        let e = &mut self.episodes[slot];
        let mut weights = vec![0.0; count];
        weights[ordinal] = 1.0;
        e.session
            .submit_guidance(
                e.source,
                Guidance {
                    weights: Some(weights),
                    value: None,
                },
            )
            .map_err(|err| format!("{err:?}"))?;
        let status = e
            .session
            .follow_guidance(e.source)
            .map_err(|err| format!("{err:?}"))?;
        let mut replay_steps = 0;
        let mut replayed = false;
        let kind = match status {
            StepStatus::NeedsGuidance(source) | StepStatus::Guided(source) => {
                e.source = source;
                if e.session.stats().services < self.max_decisions {
                    return Ok(());
                }
                "unknown_decision_limit".into()
            }
            StepStatus::Terminal { node, won } => {
                e.source = node;
                if won {
                    let replay = e
                        .session
                        .replay_win(node, self.max_decisions * 256, 256, None)
                        .map_err(|err| format!("{err:?}"))?;
                    replay_steps = replay.engine_steps();
                    replayed = true;
                    "win".into()
                } else {
                    "loss".into()
                }
            }
            StepStatus::Stopped(reason) => format!("unknown_{reason:?}"),
            StepStatus::Progress => "unknown_coverage_gap".into(),
        };
        self.finish(slot, kind, replay_steps, replayed)
    }

    /// Every row belongs to the current frame. Validate the whole population
    /// before installing anything; the independent tree owns branch selection.
    pub fn guide(
        &mut self,
        revision: u64,
        weights: &[Vec<f64>],
        values: &[Option<f64>],
    ) -> Result<(), String> {
        let frame = self.action_frame()?;
        if !self.search
            || revision != frame.revision
            || weights.is_empty()
            || weights.len() != frame.slots.len()
            || values.len() != frame.slots.len()
        {
            return Err("stale or incomplete numerical guidance batch".into());
        }
        for ((w, v), p) in weights.iter().zip(values).zip(&frame.positions) {
            if w.len() != *p
                || w.iter().any(|x| !x.is_finite() || *x < 0.0)
                || !w.iter().any(|x| *x > 0.0)
                || v.is_some_and(|x| !x.is_finite() || !(0.0..=1.0).contains(&x))
            {
                return Err("invalid numerical guidance".into());
            }
        }
        self.revision = next_frame()?;
        for ((slot, w), &v) in frame.slots.into_iter().zip(weights).zip(values) {
            if let Err(error) = self.search_one(slot, w.clone(), v) {
                self.poisoned = true;
                return Err(error);
            }
        }
        Ok(())
    }

    fn search_one(
        &mut self,
        slot: usize,
        weights: Vec<f64>,
        value: Option<f64>,
    ) -> Result<(), String> {
        let e = &mut self.episodes[slot];
        e.session
            .submit_guidance(
                e.source,
                Guidance {
                    weights: Some(weights),
                    value,
                },
            )
            .map_err(|err| format!("{err:?}"))?;
        loop {
            match e.session.step().map_err(|err| format!("{err:?}"))? {
                StepStatus::NeedsGuidance(source) | StepStatus::Guided(source) => {
                    e.source = source;
                    if e.session.stats().services >= self.max_services {
                        return self.finish(slot, "unknown_ServiceBudget".into(), 0, false);
                    }
                    return Ok(());
                }
                StepStatus::Terminal { node, won: true } => {
                    e.source = node;
                    let replay = e
                        .session
                        .replay_win(node, self.max_decisions * 256, 256, None)
                        .map_err(|err| format!("{err:?}"))?;
                    return self.finish(slot, "win".into(), replay.engine_steps(), true);
                }
                StepStatus::Stopped(reason) => {
                    return self.finish(slot, format!("unknown_{reason:?}"), 0, false)
                }
                StepStatus::Progress | StepStatus::Terminal { won: false, .. } => {}
            }
        }
    }

    fn finish(
        &mut self,
        slot: usize,
        kind: String,
        replay_steps: usize,
        replayed: bool,
    ) -> Result<(), String> {
        let e = &mut self.episodes[slot];
        let state = &e
            .session
            .view(e.source)
            .map_err(|err| format!("{err:?}"))?
            .state;
        let combat = &state.position().combat;
        e.outcome = Some(LocalOutcome {
            slot,
            seed: e.seed,
            enemy: e.enemy,
            starting_hp: e.starting_hp,
            kind,
            hp: combat.entities.player.current_hp,
            potion_uses: state.potion_uses(),
            potions: combat
                .entities
                .potions
                .iter()
                .map(|p| p.as_ref().map(|p| p.id))
                .collect(),
            turn: combat.turn.turn_count.saturating_add(1),
            decisions: e
                .session
                .observed_trace(e.source)
                .map_err(|err| format!("{err:?}"))?
                .len(),
            engine_steps: e.session.stats().engine_steps,
            replay_engine_steps: replay_steps,
            win_replayed: replayed,
            services: e.session.stats().services,
            guidance_rows: e.session.stats().guidance_replies,
            terminal_observations: e.session.stats().terminal_observations,
            coverage_gaps: e.session.stats().coverage_gaps,
            policy_rollout_services: e.session.stats().policy_rollout_services,
        });
        Ok(())
    }

    pub fn outcomes(&self) -> Vec<LocalOutcome> {
        self.episodes
            .iter()
            .filter_map(|e| e.outcome.clone())
            .collect()
    }

    pub fn demonstration(&self, slot: usize) -> Result<NumericDemonstration, String> {
        if self.task != LocalTask::Starter {
            return Err("task requires numerical schema 2".into());
        }
        self.demonstration_with(slot, encode_starter)
    }

    pub fn tactical_demonstration(
        &self,
        slot: usize,
    ) -> Result<NumericDemonstration<crate::local_tactics::TacticalPosition>, String> {
        if self.task != LocalTask::DrawSelect {
            return Err("task requires numerical schema 1".into());
        }
        self.demonstration_with(slot, crate::local_tactics::encode)
    }

    fn demonstration_with<P>(
        &self,
        slot: usize,
        encode: impl Fn(&SearchSession<CombatSearchSpace>, NodeRef) -> Result<P, String>,
    ) -> Result<NumericDemonstration<P>, String> {
        if self.poisoned {
            return Err("population stopped after a core error".into());
        }
        let e = self.episodes.get(slot).ok_or("unknown episode slot")?;
        if !e
            .outcome
            .as_ref()
            .is_some_and(|o| o.kind == "win" && o.win_replayed)
        {
            return Err("demonstration requires a replayed victory".into());
        }
        let mut positions = Vec::new();
        let mut choices = Vec::new();
        for (source, ordinal, _) in e
            .session
            .decision_path(e.source)
            .map_err(|err| format!("{err:?}"))?
        {
            let view = e.session.view(source).map_err(|err| format!("{err:?}"))?;
            if card_count(view.state.position()) != card_count(&e.root) {
                return Err("demonstration changed its card instance count".into());
            }
            positions.push(encode(&e.session, source)?);
            choices.push(ordinal);
        }
        Ok(NumericDemonstration { positions, choices })
    }

    /// Low-frequency evidence only; never sent to the numerical model.
    pub fn evidence_json(&self) -> Result<String, String> {
        let mut episodes = Vec::new();
        for (slot, e) in self.episodes.iter().enumerate() {
            let trace = e
                .session
                .observed_trace(e.source)
                .map_err(|err| format!("{err:?}"))?;
            let mut evidence = serde_json::json!({
                "source": if self.deck.is_some() { "prescribed_custom_deck_encounter" } else { match self.task { LocalTask::Starter => "prescribed_starter_encounter", LocalTask::DrawSelect => "prescribed_draw_select_encounter", LocalTask::Entities => "prescribed_entity_combat" } },
                "deck_recipe": self.deck,
                "task":self.task, "seed":e.seed, "enemy":e.enemy,
                "starting_hp": e.starting_hp,
                "decision_ordinals": e.session.decision_path(e.source).map_err(|err| format!("{err:?}"))?.iter().map(|(_, a, _)| *a).collect::<Vec<_>>(),
                "root":e.root, "inputs":trace.iter().map(|s| &s.input).collect::<Vec<_>>(),
                "successor":e.session.view(e.source).map_err(|err| format!("{err:?}"))?.state.position(),
                "outcome":e.outcome,
            });
            if let Some(specs) = &self.specifications {
                evidence["construction"] =
                    serde_json::to_value(&specs[slot]).map_err(|e| e.to_string())?;
            }
            episodes.push(evidence);
        }
        serde_json::to_string(
            &serde_json::json!({"schema":self.task.schema(),"poisoned":self.poisoned,"episodes":episodes,
                "mode":if self.search { "fixed_root_search" } else { "direct_policy" },
                "max_decisions":self.max_decisions,"max_services":self.max_services,
                "max_policy_rollout_steps":self.max_policy_rollout_steps}),
        )
        .map_err(|e| e.to_string())
    }
}

fn card_count(position: &CombatPosition) -> usize {
    let zones = &position.combat.zones;
    zones.hand.len()
        + zones.draw_pile.len()
        + zones.discard_pile.len()
        + zones.exhaust_pile.len()
        + zones.limbo.len()
}

fn next_frame() -> Result<u64, String> {
    NEXT_FRAME
        .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |v| v.checked_add(1))
        .map_err(|_| "numerical source counter exhausted".into())
}
