use crate::runtime::combat::CardLineageId;
use crate::state::{
    GridSelectFilter, GridSelectReason, HandSelectFilter, HandSelectReason, PileType,
};
use crate::EntityId;
use serde::{Deserialize, Serialize};

pub const NO_SOURCE: EntityId = EntityId::MAX;
/// Java `DiscoveryAction` presents three distinct cards when the choice opens.
/// Keep this beside the typed suspend action so execution and public mechanics
/// projections cannot drift.
pub const DISCOVERY_CARD_CHOICE_COUNT: u8 = 3;

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct DamageInfo {
    pub source: EntityId,
    pub target: EntityId,
    pub base: i32,
    pub output: i32,
    pub damage_type: DamageType,
    pub is_modified: bool,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub enum MonsterRuntimePatch {
    Hexaghost {
        activated: Option<bool>,
        orb_active_count: Option<u8>,
        burn_upgraded: Option<bool>,
        divider_damage: Option<i32>,
        clear_divider_damage: bool,
    },
    Lagavulin {
        idle_count: Option<u8>,
        debuff_turn_count: Option<u8>,
        is_out: Option<bool>,
        is_out_triggered: Option<bool>,
    },
    JawWorm {
        first_move: Option<bool>,
        hard_mode: Option<bool>,
        protocol_seeded: Option<bool>,
    },
    Guardian {
        damage_threshold: Option<i32>,
        damage_taken: Option<i32>,
        is_open: Option<bool>,
        close_up_triggered: Option<bool>,
    },
    Byrd {
        first_move: Option<bool>,
        is_flying: Option<bool>,
        protocol_seeded: Option<bool>,
    },
    Chosen {
        first_turn: Option<bool>,
        used_hex: Option<bool>,
        protocol_seeded: Option<bool>,
    },
    Snecko {
        first_turn: Option<bool>,
        protocol_seeded: Option<bool>,
    },
    ShelledParasite {
        first_move: Option<bool>,
        protocol_seeded: Option<bool>,
    },
    BronzeAutomaton {
        first_turn: Option<bool>,
        num_turns: Option<u8>,
        protocol_seeded: Option<bool>,
    },
    BronzeOrb {
        used_stasis: Option<bool>,
        protocol_seeded: Option<bool>,
    },
    BookOfStabbing {
        stab_count: Option<u8>,
        protocol_seeded: Option<bool>,
    },
    Collector {
        initial_spawn: Option<bool>,
        ult_used: Option<bool>,
        turns_taken: Option<u8>,
        enemy_slots: Option<[Option<EntityId>; 2]>,
        protocol_seeded: Option<bool>,
    },
    Champ {
        first_turn: Option<bool>,
        num_turns: Option<u8>,
        forge_times: Option<u8>,
        threshold_reached: Option<bool>,
        protocol_seeded: Option<bool>,
    },
    AwakenedOne {
        form1: Option<bool>,
        first_turn: Option<bool>,
        protocol_seeded: Option<bool>,
    },
    Darkling {
        first_move: Option<bool>,
        nip_dmg: Option<i32>,
        protocol_seeded: Option<bool>,
    },
    CorruptHeart {
        first_move: Option<bool>,
        move_count: Option<u8>,
        buff_count: Option<u8>,
        blood_hit_count: Option<u8>,
        protocol_seeded: Option<bool>,
    },
    WrithingMass {
        first_move: Option<bool>,
        used_mega_debuff: Option<bool>,
        protocol_seeded: Option<bool>,
    },
    Spiker {
        thorns_count: Option<u8>,
        protocol_seeded: Option<bool>,
    },
    SpireShield {
        move_count: Option<u8>,
        protocol_seeded: Option<bool>,
    },
    SpireSpear {
        move_count: Option<u8>,
        skewer_count: Option<u8>,
        protocol_seeded: Option<bool>,
    },
    SlaverRed {
        first_turn: Option<bool>,
        used_entangle: Option<bool>,
        protocol_seeded: Option<bool>,
    },
    GremlinNob {
        used_bellow: Option<bool>,
        protocol_seeded: Option<bool>,
    },
    GremlinLeader {
        gremlin_slots: Option<[Option<EntityId>; 3]>,
        protocol_seeded: Option<bool>,
    },
    Reptomancer {
        first_move: Option<bool>,
        dagger_slots: Option<[Option<EntityId>; 4]>,
        protocol_seeded: Option<bool>,
    },
    Nemesis {
        first_move: Option<bool>,
        scythe_cooldown: Option<i32>,
        protocol_seeded: Option<bool>,
    },
    GiantHead {
        count: Option<i32>,
        protocol_seeded: Option<bool>,
    },
    TimeEater {
        used_haste: Option<bool>,
        protocol_seeded: Option<bool>,
    },
    Donu {
        is_attacking: Option<bool>,
        protocol_seeded: Option<bool>,
    },
    Deca {
        is_attacking: Option<bool>,
        protocol_seeded: Option<bool>,
    },
    Transient {
        count: Option<i32>,
        protocol_seeded: Option<bool>,
    },
    Exploder {
        turn_count: Option<i32>,
        protocol_seeded: Option<bool>,
    },
    Maw {
        roared: Option<bool>,
        turn_count: Option<i32>,
        protocol_seeded: Option<bool>,
    },
    SnakeDagger {
        first_move: Option<bool>,
        protocol_seeded: Option<bool>,
    },
    GremlinWizard {
        current_charge: Option<u8>,
        protocol_seeded: Option<bool>,
    },
    Cultist {
        first_move: Option<bool>,
        protocol_seeded: Option<bool>,
    },
    Sentry {
        first_move: Option<bool>,
        protocol_seeded: Option<bool>,
    },
    SlimeBoss {
        first_turn: Option<bool>,
        protocol_seeded: Option<bool>,
    },
    LargeSlime {
        split_triggered: Option<bool>,
        protocol_seeded: Option<bool>,
    },
    SphericGuardian {
        first_move: Option<bool>,
        second_move: Option<bool>,
        protocol_seeded: Option<bool>,
    },
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub enum Action {
    Damage(DamageInfo),
    /// Java `PummelDamageAction`.
    ///
    /// Unlike ordinary `DamageAction`, execution re-checks that the target's
    /// current HP is still above zero before applying damage. Pummel queues
    /// light hits through this action and finishes with one ordinary
    /// `DamageAction`.
    PummelDamage(DamageInfo),
    /// Java `BaneAction`: execute a second attack only if the target is alive
    /// and still has Poison when this queued action resolves.
    BaneDamage(DamageInfo),
    /// Java `WallopAction`: after damage resolves, gain block equal to the
    /// target's actual HP lost from this hit.
    WallopDamage(DamageInfo),
    /// Java Watcher `JudgementAction`: if the target monster is at or below
    /// the cutoff when this action resolves, queue an `InstantKillAction`.
    Judgement {
        target: EntityId,
        cutoff: i32,
    },
    /// Java `InstantKillAction`: set the target HP to zero and run the normal
    /// monster death path.
    InstantKill {
        target: EntityId,
    },
    /// Java Watcher `TriggerMarksAction`: trigger every monster MarkPower for
    /// the calling card.
    TriggerMarks {
        card_id: crate::content::cards::CardId,
    },
    /// Java `DamagePerAttackPlayedAction` used by Finisher.
    ///
    /// Execution reads the current attack count from the action manager state,
    /// subtracts the Finisher card itself, and queues that many ordinary
    /// `DamageAction`s to the top.
    DamagePerAttackPlayed(DamageInfo),
    /// Java `HeelHookAction`: queue damage, and if the target has Weak at
    /// execution time queue the energy/draw follow-up behind that damage.
    HeelHook(DamageInfo),
    /// Java `FlechetteAction`: count Skills in hand at execution time and
    /// queue one ordinary `DamageAction` per Skill.
    Flechettes(DamageInfo),
    /// Canonical monster attack action.
    ///
    /// Contract:
    /// - `base_damage` is the monster plan truth input.
    /// - Monster content code should emit this action instead of constructing `DamageInfo`.
    /// - For `Normal` monster attacks, engine execution re-resolves final damage from
    ///   `base_damage`; content code must not guess a modified `output` value.
    /// - Non-normal kinds such as `HpLoss` and `Thorns` use the provided numeric value
    ///   directly via the damage pipeline.
    MonsterAttack {
        source: EntityId,
        target: EntityId,
        base_damage: i32,
        damage_kind: crate::runtime::monster_move::DamageKind,
    },
    /// Canonical thief strike side effect.
    ///
    /// Contract:
    /// - `amount` is the locked per-hit steal cap from monster plan truth.
    /// - Execution steals `min(amount, player.gold)` immediately from the player.
    /// - Execution also records the actual stolen amount on the thief runtime state.
    StealPlayerGold {
        thief_id: EntityId,
        amount: i32,
    },
    DamageAllEnemies {
        source: EntityId,
        damages: smallvec::SmallVec<[i32; 5]>,
        damage_type: DamageType,
        is_modified: bool,
    },
    /// Defect orb damage path. Java applies Lock-On only through
    /// AbstractOrb.applyLockOn / orb damage matrices, not to every THORNS hit.
    OrbDamage {
        source: EntityId,
        target: EntityId,
        base_damage: i32,
    },
    OrbDamageRandomEnemy {
        source: EntityId,
        base_damage: i32,
    },
    OrbDamageAllEnemies {
        source: EntityId,
        base_damage: i32,
    },
    Whirlwind {
        damages: smallvec::SmallVec<[i32; 5]>,
        damage_type: DamageType,
        free_to_play_once: bool,
        energy_on_use: i32,
    },
    Skewer {
        target: EntityId,
        damage_info: DamageInfo,
        free_to_play_once: bool,
        energy_on_use: i32,
    },
    Sunder {
        target: EntityId,
        damage_info: DamageInfo,
        energy_gain: i32,
    },
    Doppelganger {
        upgraded: bool,
        free_to_play_once: bool,
        energy_on_use: i32,
    },
    Malaise {
        target: EntityId,
        upgraded: bool,
        free_to_play_once: bool,
        energy_on_use: i32,
    },
    Collect {
        upgraded: bool,
        free_to_play_once: bool,
        energy_on_use: i32,
    },
    ConjureBlade {
        free_to_play_once: bool,
        energy_on_use: i32,
    },
    Meditate {
        amount: u8,
    },
    GainBlock {
        target: EntityId,
        amount: i32,
    },
    DoubleBlock {
        target: EntityId,
    },
    GainBlockRandomMonster {
        source: EntityId,
        amount: i32,
    },
    LoseBlock {
        target: EntityId,
        amount: i32,
    },
    RemoveAllBlock {
        target: EntityId,
    },
    /// Rust migration shim for Java `LoseHPAction` provenance.
    ///
    /// In Java, whether HP loss triggers `RupturePower.wasHPLost(...)` is determined
    /// by the concrete action path and `DamageInfo.owner`, not by a standalone flag.
    /// Our unified action model loses that provenance, so `triggers_rupture` must be
    /// set only for player-authored self HP loss sources that Java routes through a
    /// `LoseHPAction(player, player, ...)`-equivalent path.
    LoseHp {
        target: EntityId,
        amount: i32,
        triggers_rupture: bool,
    },
    /// Java PoisonLoseHpAction: HP_LOSS damage and Poison countdown happen
    /// inside one action before clearPostCombatActions.
    PoisonLoseHp {
        source: EntityId,
        target: EntityId,
        amount: i32,
    },
    SetCurrentHp {
        target: EntityId,
        hp: i32,
    },
    Heal {
        target: EntityId,
        amount: i32,
    },
    GainGold {
        amount: i32,
    },
    AddCombatReward {
        item: crate::state::rewards::RewardItem,
    },
    GainEnergy {
        amount: i32,
    },
    /// Java Defect `DoubleEnergyAction`: gain current energy at execution time.
    DoubleEnergy,
    /// Java `GainEnergyIfDiscardAction`: checked at action execution time.
    GainEnergyIfDiscardedThisTurn {
        amount: i32,
    },
    /// Java Watcher `FollowUpAction`: at execution time, gain 1 energy if the
    /// previous card in `cardsPlayedThisCombat` was an Attack.
    FollowUp,
    /// Java Watcher `SanctityAction`: at execution time, draw if the previous
    /// card in `cardsPlayedThisCombat` was a Skill.
    Sanctity {
        draw_amount: u32,
    },
    /// Java Watcher `CrushJointsAction`: at execution time, apply Vulnerable
    /// to the target if the previous card in combat was a Skill.
    CrushJoints {
        target: EntityId,
        amount: i32,
    },
    /// Java Watcher `HeadStompAction` used by Sash Whip: at execution time,
    /// apply Weak to the target if the previous card in combat was an Attack.
    SashWhip {
        target: EntityId,
        amount: i32,
    },
    /// Java `ExpertiseAction`: draw until the current hand size reaches this
    /// amount, evaluated when the queued action resolves.
    ExpertiseDraw {
        target_hand_size: i32,
    },
    GainMaxHp {
        amount: i32,
    },
    /// Java `IncreaseMaxHpAction` for monsters. The percentage is evaluated
    /// against the target's max HP when the queued action resolves, then both
    /// max HP and current HP increase by the rounded amount.
    IncreaseMonsterMaxHpPercent {
        target: EntityId,
        percent: u8,
    },
    LoseMaxHp {
        target: EntityId,
        amount: i32,
    },
    /// Java `DamageRandomEnemyAction`: select a random living monster at
    /// execution time, then queue/resolve a normal `DamageAction`.
    ///
    /// This is a DAMAGE action in Java and is therefore retained by
    /// `GameActionManager.clearPostCombatActions`.
    DamageRandomEnemy {
        source: EntityId,
        base_damage: i32,
        damage_type: DamageType,
    },
    /// Java `AttackDamageRandomEnemyAction`: select a random living monster at
    /// execution time, recalculate the referenced card against that target,
    /// then queue/resolve a normal `DamageAction`.
    ///
    /// This action does not set `actionType = DAMAGE` in Java; only the
    /// generated `DamageAction` is retained after post-combat cleanup.
    AttackDamageRandomEnemyCard {
        card: Box<crate::runtime::combat::CombatCard>,
    },
    BouncingFlask {
        target: Option<EntityId>,
        amount: i32,
        num_times: u8,
    },
    DropkickDamageAndEffect {
        target: EntityId,
        damage_info: DamageInfo,
    },
    SpotWeakness {
        target: EntityId,
        amount: i32,
    },
    /// Java Defect `ForTheEyesAction`: apply Weak to the target only if its
    /// current `getIntentBaseDmg()` is non-negative when the action resolves.
    ApplyWeakIfTargetAttacking {
        target: EntityId,
        amount: i32,
    },
    /// Java Watcher `FearNoEvilAction`: at execution time, queue damage and
    /// then Calm if the target's current Java intent is attack-class.
    FearNoEvil {
        target: EntityId,
        damage_info: DamageInfo,
    },
    Ftl {
        target: EntityId,
        damage_info: DamageInfo,
        card_play_count: i32,
    },
    FiendFire {
        target: EntityId,
        damage_info: DamageInfo,
    },
    ExhaustAllNonAttack,
    BlockPerNonAttack {
        block_per_card: i32,
    },
    Feed {
        target: EntityId,
        damage_info: DamageInfo,
        max_hp_amount: i32,
    },
    /// Java Watcher `LessonLearnedAction`: damage, then if the target is a
    /// qualifying kill, randomly upgrade an upgradeable master-deck card.
    LessonLearned {
        target: EntityId,
        damage_info: DamageInfo,
    },
    HandOfGreed {
        target: EntityId,
        damage_info: DamageInfo,
        gold_amount: i32,
    },
    RitualDagger {
        target: EntityId,
        damage_info: DamageInfo,
        misc_amount: i32,
        card_lineage: CardLineageId,
    },
    VampireDamage(DamageInfo),
    VampireDamageAllEnemies {
        source: EntityId,
        damages: smallvec::SmallVec<[i32; 5]>,
        damage_type: DamageType,
    },
    /// Java `BarrageAction`: at action execution time, queue one damage action
    /// for each current non-empty orb slot.
    Barrage {
        damage: DamageInfo,
    },
    LimitBreak,
    DrawCards(u32),
    /// Java Watcher `InnerPeaceAction`: at execution time, draw if currently
    /// in Calm; otherwise queue a Calm stance change.
    InnerPeace {
        draw_amount: u32,
    },
    /// Java Watcher `IndignationAction`: at execution time, enter Wrath unless
    /// already in Wrath; in Wrath, apply Vulnerable to all current monsters.
    Indignation {
        amount: i32,
    },
    /// Java `CompileDriverAction`: at action execution time, draw
    /// `amount_per_orb_type` for each unique non-empty orb type.
    DrawForUniqueOrbTypes {
        amount_per_orb_type: u32,
    },
    /// Java `DrawCardAction` path that exposes its static `drawnCards`
    /// history to a queued follow-up action. Generic draw actions intentionally
    /// do not mutate this history; actions that need it must opt in.
    DrawCardsWithHistory {
        amount: u32,
        clear_history: bool,
    },
    /// Java `EscapePlanAction`: after the preceding DrawCardAction finishes,
    /// gain block if that DrawCardAction drew at least one Skill.
    EscapePlanBlockIfSkill {
        block: i32,
    },
    /// Java Defect `ScrapeFollowUpAction`: discard every card drawn by the
    /// preceding DrawCardAction unless it currently costs 0 or is free once.
    ScrapeFollowUp,
    /// Java `CalculatedGambleAction`: at execution time, discard the current
    /// hand, then draw the same count, optionally plus one.
    CalculatedGamble {
        draw_extra: bool,
    },
    /// Java `BladeFuryAction` from Storm of Steel.
    BladeFury {
        upgraded: bool,
    },
    /// Java `ApplyBulletTimeAction`: set every current hand card free for the turn.
    ApplyBulletTime,
    /// Java `UnloadAction`: queues DiscardSpecificCardAction for every
    /// non-Attack in hand at execution time.
    UnloadNonAttack,
    /// Java `EmptyDeckShuffleAction`: shuffle discard and move it above any
    /// existing draw cards. Relics are notified at construction, not settlement.
    EmptyDeckShuffle,
    /// Java Defect `ShuffleAllAction`: shuffle discard pile, move it onto
    /// draw pile, then queue random PutOnDeck for the current hand.
    ShuffleAllIntoDraw,
    /// Java `ShuffleAction(drawPile, triggerRelics)`: shuffle the current draw
    /// pile without moving discard cards.
    ShuffleDrawPile {
        trigger_relics: bool,
    },
    PlayCard {
        card_index: usize,
        target: Option<EntityId>,
    },
    PlayCardDirect {
        card: Box<crate::runtime::combat::CombatCard>,
        target: Option<EntityId>,
        purge: bool,
    },
    EnqueueCardPlay {
        item: Box<crate::runtime::combat::QueuedCardPlay>,
        in_front: bool,
    },
    FlushNextQueuedCard,
    UsePotion {
        slot: usize,
        target: Option<EntityId>,
    },
    DiscardPotion {
        slot: usize,
    },
    UseCard {
        card_id: crate::content::cards::CardId,
        lineage: CardLineageId,
        exhaust: bool,
        rebound: bool,
        shuffle_back: bool,
        return_to_hand: bool,
        purge: bool,
    },
    ExhaustCard {
        card_lineage: CardLineageId,
        source_pile: PileType,
    },
    ExhaustRandomCard {
        amount: usize,
    },
    ExhaustFromHand {
        amount: usize,
        random: bool,
        any_number: bool,
        can_pick_zero: bool,
    },
    /// Java Defect `RecycleAction`: choose/exhaust a hand card and gain
    /// energy from its current costForTurn.
    Recycle,
    DiscardCard {
        card_lineage: CardLineageId,
    },
    DiscardFromHand {
        amount: i32,
        random: bool,
        end_turn: bool,
    },
    /// Internal continuation of Java DiscardAtEndOfTurnAction. Ethereal
    /// exhaust actions run before this marker; bottom-queued exhaust hooks run
    /// after it and therefore survive into the next turn's hand.
    DiscardHandForTurnTransition,
    /// Java `DiscardToHandAction`: move a specific discard-pile card to hand
    /// only if hand has room; otherwise leave it in discard.
    DiscardToHand {
        card_lineage: CardLineageId,
        cost_for_turn: Option<u8>,
    },
    /// Java Defect `AllCostToHandAction`: queues DiscardToHandAction for all
    /// discard-pile cards whose combat cost matches `cost_target`, or that are
    /// currently freeToPlayOnce.
    AllCostToHand {
        cost_target: i32,
    },
    MoveCard {
        card_lineage: CardLineageId,
        from: PileType,
        to: PileType,
    },
    PutOnDeck {
        amount: usize,
        random: bool,
    },
    Forethought {
        upgraded: bool,
    },
    DiscardPileToTopOfDeck,
    RemoveCardFromPile {
        card_lineage: CardLineageId,
        from: PileType,
    },
    SuspendForHandSelect {
        min: u8,
        max: u8,
        can_cancel: bool,
        filter: HandSelectFilter,
        reason: HandSelectReason,
    },
    SuspendForGridSelect {
        source_pile: PileType,
        min: u8,
        max: u8,
        can_cancel: bool,
        filter: GridSelectFilter,
        reason: GridSelectReason,
    },
    SuspendForDiscovery {
        colorless: bool,
        card_type: Option<crate::content::cards::CardType>,
        amount: u8,
        cost_for_turn: Option<u8>,
        can_skip: bool,
    },
    /// Java Watcher `ForeignInfluenceAction`: generate three unique any-color
    /// attacks, then add the selected stat-equivalent copy to hand or discard.
    SuspendForForeignInfluence {
        upgraded: bool,
    },
    /// StancePotion: Java ChooseOneAction(ChooseWrath, ChooseCalm)
    SuspendForStanceChoice,
    /// Java Watcher `ChooseOneAction`: open an option-card choice and run the
    /// chosen card's `onChoseThisOption` callback.
    SuspendForChooseOne {
        choices: Vec<crate::state::ChooseOneCardChoice>,
    },
    ApplyPower {
        source: EntityId,
        target: EntityId,
        power_id: crate::content::powers::PowerId,
        amount: i32,
    },
    ApplyPowerDetailed {
        source: EntityId,
        target: EntityId,
        power_id: crate::content::powers::PowerId,
        amount: i32,
        instance_id: Option<u32>,
        extra_data: Option<i32>,
    },
    ApplyPowerWithPayload {
        source: EntityId,
        target: EntityId,
        power_id: crate::content::powers::PowerId,
        amount: i32,
        instance_id: Option<u32>,
        extra_data: Option<i32>,
        payload: crate::runtime::combat::PowerPayload,
    },
    ReducePower {
        target: EntityId,
        power_id: crate::content::powers::PowerId,
        amount: i32,
    },
    ReducePowerInstance {
        target: EntityId,
        power_id: crate::content::powers::PowerId,
        instance_id: u32,
        amount: i32,
    },
    RemovePower {
        target: EntityId,
        power_id: crate::content::powers::PowerId,
    },
    RemovePowerInstance {
        target: EntityId,
        power_id: crate::content::powers::PowerId,
        instance_id: u32,
    },
    RemoveAllDebuffs {
        target: EntityId,
    },
    UpdatePowerExtraData {
        target: EntityId,
        power_id: crate::runtime::combat::PowerId,
        value: i32,
    },
    UpdatePowerExtraDataInstance {
        target: EntityId,
        power_id: crate::runtime::combat::PowerId,
        instance_id: u32,
        value: i32,
    },
    MakeTempCardInHand {
        card_id: crate::content::cards::CardId,
        amount: u8,
        upgraded: bool,
    },
    MakeTempCardInDiscard {
        card_id: crate::content::cards::CardId,
        amount: u8,
        upgraded: bool,
    },
    MakeCopyInHand {
        original: Box<crate::runtime::combat::CombatCard>,
        amount: u8,
    },
    /// Java `MakeTempCardInHandAction` applies its constructor-time mechanics
    /// before the action is queued. The card stored here has already passed
    /// that constructor boundary; execution still applies the hand/discard
    /// effect boundary.
    MakeConstructedCopyInHand {
        original: Box<crate::runtime::combat::CardBlueprint>,
        amount: u8,
    },
    MakeCopyInDrawPile {
        original: Box<crate::runtime::combat::CardBlueprint>,
        amount: u8,
        random_spot: bool,
        to_bottom: bool,
    },
    MakeCopyInDiscard {
        original: Box<crate::runtime::combat::CombatCard>,
        amount: u8,
    },
    ReturnStasisCard {
        card_lineage: CardLineageId,
        to_hand: bool,
    },
    MakeTempCardInDiscardAndDeck {
        card_id: crate::content::cards::CardId,
        amount: u8,
    },
    MakeRandomCardInHand {
        card_type: Option<crate::content::cards::CardType>,
        cost_for_turn: Option<u8>,
    },
    /// Java `NightmareAction`: at execution time choose/capture a hand card
    /// and apply one independent Nightmare power carrying a stat-equivalent
    /// card snapshot.
    Nightmare {
        amount: u8,
    },
    MakeRandomCardInDrawPile {
        card_type: Option<crate::content::cards::CardType>,
        cost_for_turn: Option<u8>,
        random_spot: bool,
    },
    DrawPileToHandByType {
        amount: u8,
        card_type: crate::content::cards::CardType,
    },
    MakeRandomColorlessCardInHand {
        cost_for_turn: Option<u8>,
        upgraded: bool,
    },
    Transmutation {
        upgraded: bool,
        free_to_play_once: bool,
        energy_on_use: i32,
    },
    AggregateEnergy {
        divide_amount: i32,
    },
    Tempest {
        upgraded: bool,
        free_to_play_once: bool,
        energy_on_use: i32,
    },
    MultiCast {
        upgraded: bool,
        free_to_play_once: bool,
        energy_on_use: i32,
    },
    ReinforcedBody {
        block_amount: i32,
        free_to_play_once: bool,
        energy_on_use: i32,
    },
    ReduceAllHandCosts {
        amount: u8,
    },
    /// Java `EstablishmentPowerAction`: reduce combat cost of hand cards that
    /// are retained by selfRetain or the one-turn retain flag.
    ReduceRetainedHandCosts {
        amount: i32,
    },
    Enlightenment {
        permanent: bool,
    },
    Halt {
        block: i32,
        additional: i32,
    },
    Madness,
    RandomizeHandCosts,
    UpgradeAllInHand,
    UpgradeAllCardsInCombat,
    /// Hexaghost's BurnIncreaseAction: upgrades all Burn cards in draw pile and discard pile.
    UpgradeAllBurns,
    MakeTempCardInDrawPile {
        card_id: crate::content::cards::CardId,
        amount: u8,
        random_spot: bool,
        to_bottom: bool,
        upgraded: bool,
    },
    /// Java PlayTopCardAction with random target selection via cardRandomRng.
    /// Used by DistilledChaosPotion and similar.
    PlayTopCard {
        target: Option<EntityId>,
        exhaust: bool,
    },
    QueuePlayTopCardToBottom {
        target: Option<EntityId>,
        exhaust: bool,
    },
    ModifyCardMisc {
        card_lineage: CardLineageId,
        amount: i32,
    },
    ModifyCardDamage {
        card_lineage: CardLineageId,
        amount: i32,
    },
    /// Java Defect `GashAction`: increase the used Claw and all Claw cards in
    /// hand/draw/discard by the current magic amount.
    Gash {
        card_lineage: CardLineageId,
        amount: i32,
    },
    ModifyCardBlock {
        card_lineage: CardLineageId,
        amount: i32,
    },
    ReduceCardCostForCombat {
        card_lineage: CardLineageId,
        amount: i32,
    },
    UpgradeCard {
        card_lineage: CardLineageId,
    },
    /// Java: UpgradeRandomCardAction — filters upgradeable non-STATUS cards, shuffles, upgrades first.
    UpgradeRandomCard,
    ExecuteMonsterTurn(EntityId),
    RollMonsterMove {
        monster_id: EntityId,
    },
    SetMonsterMove {
        monster_id: EntityId,
        next_move_byte: u8,
        planned_steps: crate::runtime::monster_move::MonsterTurnSteps,
        planned_visible_spec: Option<crate::runtime::monster_move::MonsterMoveSpec>,
    },
    UpdateMonsterRuntime {
        monster_id: EntityId,
        patch: MonsterRuntimePatch,
    },
    /// Java `TheGuardian.damage`: when Mode Shift reaches its threshold,
    /// damage accounting and `closeUpTriggered` update immediately, while the
    /// actual Defensive Mode state change is still a queued ChangeStateAction.
    GuardianModeShiftThresholdTriggered {
        monster_id: EntityId,
        hp_lost: i32,
    },
    /// Java `ChangeStateAction(this, "Defensive Mode")` for The Guardian.
    /// This is intentionally a single queued action because its body queues
    /// RemovePower/GainBlock at execution time, not when damage crossed the
    /// threshold.
    GuardianEnterDefensiveMode {
        monster_id: EntityId,
        next_threshold: i32,
    },
    ReviveMonster {
        target: EntityId,
    },
    AbortDeath {
        target: EntityId,
    },
    Suicide {
        target: EntityId,
        trigger_relics: bool,
    },
    IncreaseMaxOrb(u8),
    DecreaseMaxOrb(u8),
    SpawnMonster {
        monster_id: crate::content::monsters::EnemyId,
        slot: u8,
        current_hp: i32,
        max_hp: i32,
        logical_position: i32,
        protocol_draw_x: Option<i32>,
        is_minion: bool,
    },
    SpawnMonsterSmart {
        monster_id: crate::content::monsters::EnemyId,
        logical_position: i32,
        hp: crate::runtime::monster_move::SpawnHpSpec,
        protocol_draw_x: Option<i32>,
        is_minion: bool,
    },
    SpawnCollectorTorch {
        collector_id: EntityId,
        slot: u8,
        logical_position: i32,
        hp: crate::runtime::monster_move::SpawnHpSpec,
        protocol_draw_x: Option<i32>,
    },
    SpawnGremlinLeaderMinion {
        leader_id: EntityId,
        slot: u8,
        monster_id: crate::content::monsters::EnemyId,
        logical_position: i32,
        hp: crate::runtime::monster_move::SpawnHpSpec,
        protocol_draw_x: Option<i32>,
    },
    SpawnReptomancerDagger {
        reptomancer_id: EntityId,
        slot: u8,
        logical_position: i32,
        hp: crate::runtime::monster_move::SpawnHpSpec,
        protocol_draw_x: Option<i32>,
    },
    SpawnEncounter {
        encounter: crate::content::monsters::factory::EncounterId,
    },
    Escape {
        target: EntityId,
    },
    FleeCombat,
    /// Deferred card-to-pile placement (matches Java UseCardAction.update() ordering).
    /// Card is held in limbo until this action fires, then moved to discard/exhaust.
    /// Java early-end cleanup routes queued autoplay cards through UseCardAction with
    /// dontTriggerOnUseCard=true, so those cleanup actions must skip after-use hooks.
    UseCardDone {
        should_exhaust: bool,
        trigger_after_use_hooks: bool,
    },
    /// Java `UseCardAction.update()` after-use power hook path for cards that
    /// do not enter the normal non-Power discard/exhaust cleanup.
    UseCardAfterUseHooks {
        card: Box<crate::runtime::combat::CombatCard>,
    },
    QueueEarlyEndTurn,
    SkipEnemiesTurn,
    TriggerTimeWarpEndTurn {
        owner: EntityId,
    },
    StartTurnTrigger,
    PostDrawTrigger,
    EndTurnTrigger,
    PreBattleTrigger,
    BattleStartPreDrawTrigger,
    BattleStartTrigger,
    /// Java `RedSkull.atBattleStart()` queues a custom action that checks
    /// bloodied state when it executes, after earlier top-inserted battle-start
    /// effects such as Blood Vial have resolved.
    RedSkullBattleStartCheck,
    /// Java deprecated `Dodecahedron.atTurnStart()` queues an anonymous action
    /// that checks full HP when that action executes, then queues the energy
    /// gain behind any currently pending actions.
    DodecahedronTurnStartCheck,
    ClearCardQueue,
    AddCardToMasterDeck {
        card_id: crate::content::cards::CardId,
        source: EntityId,
    },
    ApplyStasis {
        target_id: EntityId,
    },
    UpdateRelicCounter {
        relic_id: crate::content::relics::RelicId,
        counter: i32,
    },
    UpdateRelicAmount {
        relic_id: crate::content::relics::RelicId,
        amount: i32,
    },
    UpdateRelicUsedUp {
        relic_id: crate::content::relics::RelicId,
        used_up: bool,
    },
    ChannelOrb(crate::runtime::combat::OrbId),
    /// Java `AbstractOrb.getRandomOrb(true)` from Chaos.
    ///
    /// The handler materializes all requested orb ids before channeling any of
    /// them, because upgraded Chaos calls getRandomOrb twice during card use
    /// before either queued ChannelAction resolves.
    ChannelRandomOrbs {
        amount: u8,
    },
    /// Java `ChannelAction(existingOrb, false)`: channel the concrete orb
    /// instance into an empty slot without auto-evoking.
    ChannelOrbEntity {
        orb: crate::runtime::combat::OrbEntity,
    },
    EvokeOrb,
    EvokeOrbWithoutRemoving,
    /// Java Defect `FissionAction`: reads filled orb count at execution time,
    /// then queues RemoveAll/EvokeAll, GainEnergy, and DrawCards to the top.
    Fission {
        upgraded: bool,
    },
    /// Java Defect `RemoveAllOrbsAction`: removes filled orbs without evoking.
    RemoveAllOrbs,
    /// Java Defect `EvokeAllOrbsAction`: queues one EvokeOrb per orb slot.
    EvokeAllOrbs,
    /// Java Defect `RedoAction`: snapshot the front orb instance, evoke it,
    /// then channel the same orb instance back into the empty slot.
    RedoOrb,
    TriggerStartOfTurnOrbs,
    TriggerEndOfTurnOrbs,
    TriggerImpulseOrbs,
    /// Java Defect `DarkImpulseAction`: trigger start/end turn callbacks only
    /// for Dark orbs, with Gold Plated Cables repeating only if the first orb
    /// is a non-empty Dark orb.
    TriggerDarkImpulseOrbs,
    Scry(usize),
    EnterStance(String), // Watcher stance: "Wrath", "Calm", "Divinity", "Neutral"
    ObtainPotion,
    ObtainSpecificPotion(crate::content::potions::PotionId),
    /// Unified action for NilrysCodex (Codex) / Toolbox (ChooseOneColorless) / similar relic reward screens.
    /// Generates 3 unique random cards from `pool`, player picks 1.
    /// Card goes to `destination`. If `can_skip`, player can cancel.
    SuspendForCardReward {
        pool: CardRewardPool,
        destination: CardDestination,
        can_skip: bool,
        skip_if_monsters_basically_dead: bool,
    },
}

impl Action {
    pub fn retained_by_java_clear_post_combat_actions(&self) -> bool {
        matches!(
            self,
            Action::Damage(_)
                | Action::PummelDamage(_)
                | Action::WallopDamage(_)
                | Action::MonsterAttack { .. }
                | Action::DamagePerAttackPlayed(_)
                | Action::DamageAllEnemies { .. }
                | Action::OrbDamage { .. }
                | Action::OrbDamageRandomEnemy { .. }
                | Action::OrbDamageAllEnemies { .. }
                | Action::DamageRandomEnemy { .. }
                | Action::Feed { .. }
                | Action::LessonLearned { .. }
                | Action::HandOfGreed { .. }
                | Action::RitualDagger { .. }
                | Action::VampireDamage(_)
                | Action::VampireDamageAllEnemies { .. }
                | Action::LoseHp { .. }
                | Action::PoisonLoseHp { .. }
                | Action::GainBlock { .. }
                | Action::Heal { .. }
                | Action::UseCardDone { .. }
                | Action::UseCardAfterUseHooks { .. }
        )
    }
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
pub enum CardRewardPool {
    /// All class cards (Common + Uncommon + Rare), any type
    /// Java: returnTrulyRandomCardInCombat()
    ClassAll,
    /// Colorless cards (Uncommon + Rare)
    /// Java: returnTrulyRandomColorlessCardInCombat()
    Colorless,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
pub enum CardDestination {
    /// Add to hand (overflow to discard if hand >= 10)
    Hand,
    /// Add to draw pile at a random position
    DrawPileRandom,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
pub enum DamageType {
    Normal,
    Thorns,
    HpLoss,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct ActionInfo {
    pub action: Action,
    pub insertion_mode: AddTo,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
pub enum AddTo {
    Top,
    Bottom,
}

pub fn repeated_damage_matrix(enemy_count: usize, amount: i32) -> smallvec::SmallVec<[i32; 5]> {
    std::iter::repeat_n(amount, enemy_count).collect()
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub enum EventPayload {
    CardPlayed {
        card_lineage: CardLineageId,
        card_id: crate::content::cards::CardId,
        target_id: Option<EntityId>,
    },
    CardDrawn {
        card_lineage: CardLineageId,
    },
    CardExhausted {
        card_lineage: CardLineageId,
        card_id: crate::content::cards::CardId,
    },
    CardDiscarded {
        card_lineage: CardLineageId,
    },
    HpLost {
        amount: i32,
    },
    DamageTaken {
        amount: i32,
        source: EntityId,
    },
    Attacked {
        target: EntityId,
        source: EntityId,
        amount: i32,
    },
    BlockGained {
        amount: i32,
        target: EntityId,
    },
    MonsterDied {
        target_id: EntityId,
    },
    TurnStarted,
    PostDraw,
    PlayerTurnEnded,
    MonsterTurnEnded,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct ModifierContext {
    pub source: EntityId,
    pub target: EntityId,
    pub original_damage: i32,
    pub damage_type: DamageType,
}

pub type HookId = usize;
pub type DamageModifierId = usize;
pub type CardHookId = usize;

#[derive(Clone, Default, Deserialize, Serialize)]
pub struct ModifierBus {
    pub on_attack_to_change_damage: Vec<DamageModifierId>, // Attacker changes damage
    pub on_attacked_to_change_damage: Vec<DamageModifierId>, // Defender changes damage
    pub on_attack_hooks: Vec<HookId>,                      // Execute after damage is calculated
    pub on_attacked_hooks: Vec<HookId>,                    // Execute after being attacked
    pub on_lose_hp_last: Vec<DamageModifierId>,            // Final modifier (Tungsten Rod)
    pub on_lose_hp: Vec<HookId>,
    pub was_hp_lost_hooks: Vec<HookId>,
    pub on_death_hooks: Vec<HookId>,
    pub on_monster_death_hooks: Vec<HookId>,

    // Card and Combat flow hooks
    pub on_calculate_damage: Vec<DamageModifierId>,
    pub on_play_card: Vec<CardHookId>,
    pub on_combat_start: Vec<HookId>,
    pub on_turn_start: Vec<HookId>,
}
