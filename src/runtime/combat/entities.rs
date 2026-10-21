use super::*;
use std::sync::Arc;

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct PlayerEntity {
    pub id: EntityId,
    pub current_hp: i32,
    pub max_hp: i32,
    pub block: i32,
    /// Java `AbstractPlayer.flipHorizontal`.
    ///
    /// This is a mechanical combat field in the Shield/Spear fight: while the
    /// player has Surrounded, Java uses the player's facing and monster drawX
    /// positions to decide which monster receives BackAttack.
    pub facing_left: bool,
    pub gold_delta_this_combat: i32,
    pub gold: i32,
    pub max_orbs: u8,
    pub orbs: Vec<OrbEntity>,
    pub stance: StanceId,
    pub relics: Vec<RelicState>,
    /// Derived dispatch index for Java's ordered `player.relics` hook walks.
    ///
    /// This is an execution cache, not independent game state. Never persist
    /// it: deserialization rebuilds it from `relics`. The strict wire helper
    /// rejects a stored cache so artifacts cannot make hook dispatch disagree
    /// with the authoritative relic list.
    #[serde(skip_serializing)]
    pub relic_buses: Arc<RelicBuses>,
    /// Java: EnergyManager.energyMaster — base energy per turn.
    /// Starts at 3, boss relics with onEquip() { ++energyMaster } increment this.
    /// SlaversCollar conditionally adds +1 at battle start (handled separately).
    pub energy_master: u8,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PlayerEntityDeserialize {
    id: EntityId,
    current_hp: i32,
    max_hp: i32,
    block: i32,
    facing_left: bool,
    gold_delta_this_combat: i32,
    gold: i32,
    max_orbs: u8,
    orbs: Vec<OrbEntity>,
    stance: StanceId,
    relics: Vec<RelicState>,
    energy_master: u8,
}

impl<'de> Deserialize<'de> for PlayerEntity {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let stored = PlayerEntityDeserialize::deserialize(deserializer)?;
        let relic_buses = Arc::new(RelicBuses::from_relics(&stored.relics));
        Ok(Self {
            id: stored.id,
            current_hp: stored.current_hp,
            max_hp: stored.max_hp,
            block: stored.block,
            facing_left: stored.facing_left,
            gold_delta_this_combat: stored.gold_delta_this_combat,
            gold: stored.gold,
            max_orbs: stored.max_orbs,
            orbs: stored.orbs,
            stance: stored.stance,
            relics: stored.relics,
            relic_buses,
            energy_master: stored.energy_master,
        })
    }
}

impl PlayerEntity {
    pub fn has_relic(&self, id: crate::content::relics::RelicId) -> bool {
        self.relics.iter().any(|r| r.id == id)
    }

    pub fn add_relic(&mut self, state: RelicState) {
        let index = self.relics.len();
        let sub = crate::content::relics::get_relic_subscriptions(state.id);
        self.energy_master += crate::content::relics::energy_master_delta(state.id);

        self.relics.push(state);
        Self::register_relic_subscriptions(Arc::make_mut(&mut self.relic_buses), index, sub);
    }

    fn register_relic_subscriptions(
        relic_buses: &mut RelicBuses,
        index: usize,
        sub: crate::content::relics::RelicSubscriptions,
    ) {
        if sub.at_pre_battle {
            relic_buses.at_pre_battle.push(index);
        }
        if sub.at_battle_start_pre_draw {
            relic_buses.at_battle_start_pre_draw.push(index);
        }
        if sub.at_battle_start {
            relic_buses.at_battle_start.push(index);
        }
        if sub.at_turn_start {
            relic_buses.at_turn_start.push(index);
        }
        if sub.at_turn_start_post_draw {
            relic_buses.at_turn_start_post_draw.push(index);
        }
        if sub.on_use_card {
            relic_buses.on_use_card.push(index);
        }
        if sub.on_shuffle {
            relic_buses.on_shuffle.push(index);
        }
        if sub.on_exhaust {
            relic_buses.on_exhaust.push(index);
        }
        if sub.on_lose_hp {
            relic_buses.on_lose_hp.push(index);
        }
        if sub.on_victory {
            relic_buses.on_victory.push(index);
        }
        if sub.on_apply_power {
            relic_buses.on_apply_power.push(index);
        }
        if sub.on_monster_death {
            relic_buses.on_monster_death.push(index);
        }
        if sub.on_spawn_monster {
            relic_buses.on_spawn_monster.push(index);
        }
        if sub.at_end_of_turn {
            relic_buses.at_end_of_turn.push(index);
        }
        if sub.on_use_potion {
            relic_buses.on_use_potion.push(index);
        }
        if sub.on_discard {
            relic_buses.on_discard.push(index);
        }
        if sub.on_change_stance {
            relic_buses.on_change_stance.push(index);
        }
        if sub.on_attacked_to_change_damage {
            relic_buses.on_attacked_to_change_damage.push(index);
        }
        if sub.on_lose_hp_last {
            relic_buses.on_lose_hp_last.push(index);
        }

        if sub.on_calculate_heal {
            relic_buses.on_calculate_heal.push(index);
        }
        if sub.on_calculate_x_cost {
            relic_buses.on_calculate_x_cost.push(index);
        }
        if sub.on_calculate_block_retained {
            relic_buses.on_calculate_block_retained.push(index);
        }
        if sub.on_calculate_energy_retained {
            relic_buses.on_calculate_energy_retained.push(index);
        }
        if sub.on_scry {
            relic_buses.on_scry.push(index);
        }
        if sub.on_receive_power_modify {
            relic_buses.on_receive_power_modify.push(index);
        }
        if sub.on_calculate_vulnerable_multiplier {
            relic_buses.on_calculate_vulnerable_multiplier.push(index);
        }
    }
}

impl RelicBuses {
    pub(crate) fn from_relics(relics: &[RelicState]) -> Self {
        Self::from_relic_ids(relics.iter().map(|relic| relic.id))
    }

    pub(crate) fn from_relic_ids(
        relic_ids: impl IntoIterator<Item = crate::content::relics::RelicId>,
    ) -> Self {
        let mut buses = Self::default();
        for (index, relic_id) in relic_ids.into_iter().enumerate() {
            let subscriptions = crate::content::relics::get_relic_subscriptions(relic_id);
            PlayerEntity::register_relic_subscriptions(&mut buses, index, subscriptions);
        }
        buses
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub enum Intent {
    Attack { damage: i32, hits: u8 },
    AttackBuff { damage: i32, hits: u8 },
    AttackDebuff { damage: i32, hits: u8 },
    AttackDefend { damage: i32, hits: u8 },
    Buff,
    Heal { amount: Option<i32> },
    Debuff,
    StrongDebuff,
    Debug,
    Defend,
    DefendDebuff,
    DefendBuff,
    Escape,
    Magic,
    None,
    Sleep,
    Stun,
    Unknown,
}

impl Intent {
    /// Payload-free mechanics token for feature vocabularies.
    pub const fn kind_v1(&self) -> &'static str {
        match self {
            Self::Attack { .. } => "attack",
            Self::AttackBuff { .. } => "attack_buff",
            Self::AttackDebuff { .. } => "attack_debuff",
            Self::AttackDefend { .. } => "attack_defend",
            Self::Buff => "buff",
            Self::Heal { .. } => "heal",
            Self::Debuff => "debuff",
            Self::StrongDebuff => "strong_debuff",
            Self::Debug => "debug",
            Self::Defend => "defend",
            Self::DefendDebuff => "defend_debuff",
            Self::DefendBuff => "defend_buff",
            Self::Escape => "escape",
            Self::Magic => "magic",
            Self::None => "none",
            Self::Sleep => "sleep",
            Self::Stun => "stun",
            Self::Unknown => "unknown",
        }
    }

    /// Stable public protocol token including visible attack payload.
    pub fn protocol_token_v1(&self) -> String {
        match self {
            Self::Attack { damage, hits }
            | Self::AttackBuff { damage, hits }
            | Self::AttackDebuff { damage, hits }
            | Self::AttackDefend { damage, hits } => {
                format!("{}/damage:{damage}/hits:{hits}", self.kind_v1())
            }
            Self::Heal {
                amount: Some(amount),
            } => format!("heal/amount:{amount}"),
            _ => self.kind_v1().to_string(),
        }
    }

    /// Project one explicitly visible semantic move spec into the public
    /// protocol intent shape. This is observation data only; combat execution
    /// continues to use `MonsterTurnPlan`.
    pub fn from_visible_move_spec(spec: &MonsterMoveSpec) -> Self {
        match spec {
            MonsterMoveSpec::Attack(attack) => Intent::Attack {
                damage: attack.base_damage,
                hits: attack.hits,
            },
            MonsterMoveSpec::AttackAddCard(attack, _)
            | MonsterMoveSpec::AttackUpgradeCards(attack, _)
            | MonsterMoveSpec::AttackDebuff(attack, _) => Intent::AttackDebuff {
                damage: attack.base_damage,
                hits: attack.hits,
            },
            MonsterMoveSpec::AttackBuff(attack, _) | MonsterMoveSpec::AttackBuffIntent(attack) => {
                Intent::AttackBuff {
                    damage: attack.base_damage,
                    hits: attack.hits,
                }
            }
            MonsterMoveSpec::AttackDefend(attack, _) => Intent::AttackDefend {
                damage: attack.base_damage,
                hits: attack.hits,
            },
            MonsterMoveSpec::AddCard(add_card) => match add_card.visible_strength {
                crate::runtime::monster_move::EffectStrength::Strong => Intent::StrongDebuff,
                crate::runtime::monster_move::EffectStrength::Normal => Intent::Debuff,
            },
            MonsterMoveSpec::Buff(_) | MonsterMoveSpec::BuffIntent => Intent::Buff,
            MonsterMoveSpec::Heal(heal) => Intent::Heal {
                amount: (heal.amount > 0).then_some(heal.amount),
            },
            MonsterMoveSpec::Debuff(_) => Intent::Debuff,
            MonsterMoveSpec::StrongDebuff(_) | MonsterMoveSpec::StrongDebuffIntent => {
                Intent::StrongDebuff
            }
            MonsterMoveSpec::Defend(_) => Intent::Defend,
            MonsterMoveSpec::DefendDebuff(_, _) => Intent::DefendDebuff,
            MonsterMoveSpec::DefendBuff(_, _) => Intent::DefendBuff,
            MonsterMoveSpec::Escape => Intent::Escape,
            MonsterMoveSpec::Magic => Intent::Magic,
            MonsterMoveSpec::Sleep => Intent::Sleep,
            MonsterMoveSpec::Stun => Intent::Stun,
            MonsterMoveSpec::Debug => Intent::Debug,
            MonsterMoveSpec::None => Intent::None,
            MonsterMoveSpec::Unknown => Intent::Unknown,
        }
    }

    pub fn is_java_attack_intent(&self) -> bool {
        matches!(
            self,
            Intent::Attack { .. }
                | Intent::AttackBuff { .. }
                | Intent::AttackDebuff { .. }
                | Intent::AttackDefend { .. }
        )
    }

    pub fn base_damage(&self) -> Option<i32> {
        match self {
            Intent::Attack { damage, .. }
            | Intent::AttackBuff { damage, .. }
            | Intent::AttackDebuff { damage, .. }
            | Intent::AttackDefend { damage, .. } => Some(*damage),
            _ => None,
        }
    }

    pub fn hits(&self) -> i32 {
        match self {
            Intent::Attack { hits, .. }
            | Intent::AttackBuff { hits, .. }
            | Intent::AttackDebuff { hits, .. }
            | Intent::AttackDefend { hits, .. } => (*hits as i32).max(1),
            _ => 0,
        }
    }

    /// Legacy protocol/old-monster bridge only.
    ///
    /// Semantic main paths must not derive move truth from `Intent`.
    pub fn to_legacy_move_spec(&self) -> MonsterMoveSpec {
        match self {
            Intent::Attack { damage, hits } => MonsterMoveSpec::Attack(AttackSpec {
                base_damage: *damage,
                hits: *hits,
                damage_kind: DamageKind::Normal,
            }),
            Intent::AttackBuff { .. }
            | Intent::AttackDebuff { .. }
            | Intent::AttackDefend { .. }
            | Intent::Buff
            | Intent::Heal { .. }
            | Intent::Debuff
            | Intent::StrongDebuff
            | Intent::Defend
            | Intent::DefendDebuff
            | Intent::DefendBuff => MonsterMoveSpec::Unknown,
            Intent::Debug => MonsterMoveSpec::Debug,
            Intent::Escape => MonsterMoveSpec::Escape,
            Intent::Magic => MonsterMoveSpec::Magic,
            Intent::None => MonsterMoveSpec::None,
            Intent::Sleep => MonsterMoveSpec::Sleep,
            Intent::Stun => MonsterMoveSpec::Stun,
            Intent::Unknown => MonsterMoveSpec::Unknown,
        }
    }
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct MonsterRuntimeBundle {
    pub hexaghost: HexaghostRuntimeState,
    pub louse: LouseRuntimeState,
    pub jaw_worm: JawWormRuntimeState,
    pub thief: ThiefRuntimeState,
    pub byrd: ByrdRuntimeState,
    pub chosen: ChosenRuntimeState,
    pub snecko: SneckoRuntimeState,
    pub shelled_parasite: ShelledParasiteRuntimeState,
    pub bronze_automaton: BronzeAutomatonRuntimeState,
    pub bronze_orb: BronzeOrbRuntimeState,
    pub book_of_stabbing: BookOfStabbingRuntimeState,
    pub collector: CollectorRuntimeState,
    pub champ: ChampRuntimeState,
    pub awakened_one: AwakenedOneRuntimeState,
    pub corrupt_heart: CorruptHeartRuntimeState,
    pub writhing_mass: WrithingMassRuntimeState,
    pub spiker: SpikerRuntimeState,
    pub spire_shield: SpireShieldRuntimeState,
    pub spire_spear: SpireSpearRuntimeState,
    pub slaver_red: SlaverRedRuntimeState,
    pub gremlin_leader: GremlinLeaderRuntimeState,
    pub gremlin_nob: GremlinNobRuntimeState,
    pub gremlin_wizard: GremlinWizardRuntimeState,
    pub cultist: CultistRuntimeState,
    pub sentry: SentryRuntimeState,
    pub slime_boss: SlimeBossRuntimeState,
    pub large_slime: LargeSlimeRuntimeState,
    pub spheric_guardian: SphericGuardianRuntimeState,
    pub reptomancer: ReptomancerRuntimeState,
    pub darkling: DarklingRuntimeState,
    pub nemesis: NemesisRuntimeState,
    pub giant_head: GiantHeadRuntimeState,
    pub time_eater: TimeEaterRuntimeState,
    pub donu: DonuRuntimeState,
    pub deca: DecaRuntimeState,
    pub transient: TransientRuntimeState,
    pub exploder: ExploderRuntimeState,
    pub maw: MawRuntimeState,
    pub snake_dagger: SnakeDaggerRuntimeState,
    pub lagavulin: LagavulinRuntimeState,
    pub guardian: GuardianRuntimeState,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct MonsterEntity {
    pub id: EntityId,
    pub enemy_id: crate::content::monsters::EnemyId,
    pub current_hp: i32,
    pub max_hp: i32,
    pub block: i32,
    pub slot: u8,
    pub is_dying: bool,
    pub is_escaped: bool,
    pub half_dead: bool,
    pub move_state: MonsterMoveState,
    pub logical_position: i32,
    #[serde(flatten)]
    pub runtime: Arc<MonsterRuntimeBundle>,
}

impl Deref for MonsterEntity {
    type Target = MonsterRuntimeBundle;

    fn deref(&self) -> &Self::Target {
        &self.runtime
    }
}

impl DerefMut for MonsterEntity {
    fn deref_mut(&mut self) -> &mut Self::Target {
        Arc::make_mut(&mut self.runtime)
    }
}

impl MonsterEntity {
    pub fn is_dead_or_escaped(&self) -> bool {
        self.is_dying || self.half_dead || self.is_escaped
    }

    pub fn is_alive_for_action(&self) -> bool {
        self.current_hp > 0 && !self.is_dead_or_escaped()
    }

    /// Java `MonsterGroup.getRandomMonster(..., aliveOnly=true, cardRandomRng)`
    /// filters out half-dead, dying, and escaping monsters. It does not check
    /// `currentHealth`, because subsequent actions own their own cancellation.
    pub fn is_random_target_candidate(&self) -> bool {
        !self.half_dead && !self.is_dying && !self.is_escaped
    }

    pub fn turn_plan(&self) -> MonsterTurnPlan {
        let move_id = self.planned_move_id();
        if self.is_dying || self.half_dead {
            return MonsterTurnPlan::unknown(move_id);
        }

        MonsterTurnPlan {
            move_id,
            steps: self.move_state.planned_steps.clone().unwrap_or_default(),
            visible_spec: self.move_state.planned_visible_spec.clone(),
        }
    }

    pub fn planned_move_id(&self) -> u8 {
        self.move_state.planned_move_id
    }

    pub fn set_planned_move_id(&mut self, move_id: u8) {
        self.move_state.planned_move_id = move_id;
    }

    pub fn set_planned_steps(&mut self, steps: crate::runtime::monster_move::MonsterTurnSteps) {
        self.move_state.planned_steps = Some(steps);
    }

    pub fn set_planned_visible_spec(
        &mut self,
        visible_spec: Option<crate::runtime::monster_move::MonsterMoveSpec>,
    ) {
        self.move_state.planned_visible_spec = visible_spec;
    }

    pub fn record_planned_move(&mut self, move_id: u8) {
        self.move_state.planned_move_id = move_id;
        self.move_state.history.push_back(move_id);
    }

    pub fn move_history(&self) -> &VecDeque<u8> {
        &self.move_state.history
    }

    pub fn move_history_mut(&mut self) -> &mut VecDeque<u8> {
        &mut self.move_state.history
    }

    pub fn louse_bite_damage(&self) -> Option<i32> {
        self.louse.bite_damage
    }
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq, Hash, Serialize)]
#[serde(deny_unknown_fields)]
pub struct MonsterMoveState {
    pub planned_move_id: u8,
    pub history: VecDeque<u8>,
    pub planned_steps: Option<crate::runtime::monster_move::MonsterTurnSteps>,
    pub planned_visible_spec: Option<crate::runtime::monster_move::MonsterMoveSpec>,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct MonsterProtocolIdentity {
    pub instance_id: Option<u64>,
    pub spawn_order: Option<u64>,
    pub draw_x: Option<i32>,
    pub group_index: Option<usize>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct MonsterProtocolObservationState {
    pub visible_intent: Intent,
    /// UI / protocol preview damage after monster damage modifiers are applied.
    /// This is not an executable damage base and must not be fed back into
    /// combat resolution.
    pub preview_damage_per_hit: i32,
    /// Append-only public history recorded only when the monster actually
    /// begins a turn. `MonsterMoveState::history` is mechanics state and
    /// includes the current unexecuted roll, so it must never substitute for
    /// this observation field.
    pub executed_move_history: Vec<u8>,
}

impl Default for MonsterProtocolObservationState {
    fn default() -> Self {
        Self {
            visible_intent: Intent::Unknown,
            preview_damage_per_hit: 0,
            executed_move_history: Vec::new(),
        }
    }
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct MonsterProtocolState {
    pub observation: MonsterProtocolObservationState,
    pub identity: MonsterProtocolIdentity,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::relics::RelicId;

    #[test]
    fn player_json_rebuilds_relic_dispatch_cache_and_rejects_stored_cache() {
        let mut player = crate::test_support::blank_test_combat().entities.player;
        player.add_relic(RelicState::new(RelicId::PenNib));
        player.add_relic(RelicState::new(RelicId::Calipers));
        assert_eq!(player.relic_buses.on_use_card.as_slice(), &[0]);
        assert_eq!(
            player.relic_buses.on_calculate_block_retained.as_slice(),
            &[1]
        );

        let current_json = serde_json::to_value(&player).expect("serialize player");
        assert!(
            current_json.get("relic_buses").is_none(),
            "derived dispatch cache must not be persisted"
        );
        let rebuilt: PlayerEntity =
            serde_json::from_value(current_json.clone()).expect("read current player wire");
        assert_eq!(rebuilt, player);
        assert_eq!(rebuilt.relic_buses.on_use_card.as_slice(), &[0]);
        assert_eq!(
            rebuilt.relic_buses.on_calculate_block_retained.as_slice(),
            &[1]
        );

        let mut obsolete_json = current_json;
        obsolete_json
            .as_object_mut()
            .expect("player serializes as an object")
            .insert(
                "relic_buses".to_owned(),
                serde_json::json!({
                    "on_use_card": [999],
                    "on_calculate_block_retained": []
                }),
            );

        let error = serde_json::from_value::<PlayerEntity>(obsolete_json)
            .expect_err("pre-cutover stored relic cache must be rejected");
        assert!(error.to_string().contains("unknown field `relic_buses`"));
    }

    #[test]
    fn monster_protocol_rejects_missing_executed_history() {
        let protocol = MonsterProtocolState::default();
        let mut value = serde_json::to_value(protocol).expect("serialize monster protocol");
        value["observation"]
            .as_object_mut()
            .unwrap()
            .remove("executed_move_history");

        let error = serde_json::from_value::<MonsterProtocolState>(value)
            .expect_err("current monster protocol history must be explicit");
        assert!(error.to_string().contains("executed_move_history"));
    }
}
