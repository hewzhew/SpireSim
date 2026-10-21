use crate::content::cards::CardId;
use crate::runtime::combat::CardLineageId;

#[derive(Clone, Debug, PartialEq, Eq, Hash, serde::Serialize)]
pub(crate) struct CombatRuntimeHintsKey {
    pub(crate) using_card: bool,
    pub(crate) colorless_combat_pool: Vec<CardId>,
    pub(crate) pending_rewards: Vec<String>,
    pub(crate) power_instance_counter: u32,
    pub(crate) last_drawn_cards: Vec<CombatDrawnCardKey>,
    pub(crate) monster_protocol: Vec<CombatMonsterProtocolKey>,
    pub(crate) combat_mugged: bool,
    pub(crate) combat_smoked: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, serde::Serialize)]
pub(crate) struct CombatDrawnCardKey {
    pub(crate) card_lineage: CardLineageId,
    pub(crate) card_id: CardId,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, serde::Serialize)]
pub(crate) struct CombatMonsterProtocolKey {
    pub(crate) entity_id: usize,
    pub(crate) observation: CombatMonsterProtocolObservationKey,
    pub(crate) identity: CombatMonsterProtocolIdentityKey,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, serde::Serialize)]
pub(crate) struct CombatMonsterProtocolObservationKey {
    pub(crate) visible_intent: CombatIntentKey,
    pub(crate) preview_damage_per_hit: i32,
    pub(crate) executed_move_history: Vec<u8>,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, serde::Serialize)]
pub(crate) struct CombatMonsterProtocolIdentityKey {
    pub(crate) instance_id: Option<u64>,
    pub(crate) spawn_order: Option<u64>,
    pub(crate) draw_x: Option<i32>,
    pub(crate) group_index: Option<usize>,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, serde::Serialize)]
pub(crate) enum CombatIntentKey {
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
