use crate::content::monsters::EnemyId;
use crate::runtime::combat::{
    AwakenedOneRuntimeState, BookOfStabbingRuntimeState, BronzeAutomatonRuntimeState,
    BronzeOrbRuntimeState, ByrdRuntimeState, ChampRuntimeState, ChosenRuntimeState,
    CollectorRuntimeState, CorruptHeartRuntimeState, CultistRuntimeState, DarklingRuntimeState,
    DecaRuntimeState, DonuRuntimeState, ExploderRuntimeState, GiantHeadRuntimeState,
    GremlinLeaderRuntimeState, GremlinNobRuntimeState, GremlinWizardRuntimeState,
    GuardianRuntimeState, HexaghostRuntimeState, JawWormRuntimeState, LagavulinRuntimeState,
    LargeSlimeRuntimeState, LouseRuntimeState, MawRuntimeState, MonsterMoveState,
    NemesisRuntimeState, ReptomancerRuntimeState, SentryRuntimeState, ShelledParasiteRuntimeState,
    SlaverRedRuntimeState, SlimeBossRuntimeState, SnakeDaggerRuntimeState, SneckoRuntimeState,
    SphericGuardianRuntimeState, SpikerRuntimeState, SpireShieldRuntimeState,
    SpireSpearRuntimeState, ThiefRuntimeState, TimeEaterRuntimeState, TransientRuntimeState,
    WrithingMassRuntimeState,
};

/// Exact monster identity without duplicating its derived turn plan.
///
/// `MonsterEntity::turn_plan()` is wholly determined by `move_state`,
/// `is_dying`, and `half_dead`, all of which are already part of this key.
/// Keeping a second owned plan cloned its steps and visible specification for
/// every transposition. Durable identity now encodes `move_state` directly,
/// so the historical derived turn-plan compatibility view is unnecessary.
#[derive(Clone, Debug, PartialEq, Eq, Hash, serde::Serialize)]
pub(crate) struct CombatMonsterKey {
    pub(crate) entity_id: usize,
    pub(crate) enemy_id: EnemyId,
    pub(crate) current_hp: i32,
    pub(crate) max_hp: i32,
    pub(crate) block: i32,
    pub(crate) slot: u8,
    pub(crate) logical_position: i32,
    pub(crate) is_dying: bool,
    pub(crate) is_escaped: bool,
    pub(crate) half_dead: bool,
    pub(crate) move_state: MonsterMoveState,
    pub(crate) runtime: CombatMonsterRuntimeKey,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, serde::Serialize)]
pub(crate) enum CombatMonsterRuntimeKey {
    None,
    Hexaghost(HexaghostRuntimeState),
    Louse(LouseRuntimeState),
    JawWorm(JawWormRuntimeState),
    Thief(ThiefRuntimeState),
    Byrd(ByrdRuntimeState),
    Chosen(ChosenRuntimeState),
    Snecko(SneckoRuntimeState),
    ShelledParasite(ShelledParasiteRuntimeState),
    BronzeAutomaton(BronzeAutomatonRuntimeState),
    BronzeOrb(BronzeOrbRuntimeState),
    BookOfStabbing(BookOfStabbingRuntimeState),
    Collector(CollectorRuntimeState),
    Champ(ChampRuntimeState),
    AwakenedOne(AwakenedOneRuntimeState),
    CorruptHeart(CorruptHeartRuntimeState),
    WrithingMass(WrithingMassRuntimeState),
    Spiker(SpikerRuntimeState),
    SpireShield(SpireShieldRuntimeState),
    SpireSpear(SpireSpearRuntimeState),
    SlaverRed(SlaverRedRuntimeState),
    GremlinLeader(GremlinLeaderRuntimeState),
    GremlinNob(GremlinNobRuntimeState),
    GremlinWizard(GremlinWizardRuntimeState),
    Cultist(CultistRuntimeState),
    Sentry(SentryRuntimeState),
    SlimeBoss(SlimeBossRuntimeState),
    LargeSlime(LargeSlimeRuntimeState),
    SphericGuardian(SphericGuardianRuntimeState),
    Reptomancer(ReptomancerRuntimeState),
    Darkling(DarklingRuntimeState),
    Nemesis(NemesisRuntimeState),
    GiantHead(GiantHeadRuntimeState),
    TimeEater(TimeEaterRuntimeState),
    Donu(DonuRuntimeState),
    Deca(DecaRuntimeState),
    Transient(TransientRuntimeState),
    Exploder(ExploderRuntimeState),
    Maw(MawRuntimeState),
    SnakeDagger(SnakeDaggerRuntimeState),
    Lagavulin(LagavulinRuntimeState),
    Guardian(GuardianRuntimeState),
}
