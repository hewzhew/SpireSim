use crate::content::cards::{get_card_definition, CardId};
use crate::content::relics::RelicId;
use crate::runtime::combat::{CombatCard, CombatState};
#[derive(Default)]
pub(super) struct AwakenedOneDeckSignals {
    pub deck: Vec<CombatCard>,
    pub energy: u8,
    pub has_runic_dome: bool,
    pub powers: Vec<CombatCard>,
    pub damage_scaling: Vec<CombatCard>,
    pub slow_damage_scaling: Vec<CombatCard>,
    pub scaling_multiplier: Vec<CombatCard>,
    pub defensive_scaling_or_mitigation: Vec<CombatCard>,
    pub mitigation_or_strength_down: Vec<CombatCard>,
    pub defensive_engine_or_repeatable_block: Vec<CombatCard>,
    pub big_block: Vec<CombatCard>,
    pub reliable_burst_block: Vec<CombatCard>,
    pub generic_block: Vec<CombatCard>,
    pub aoe: Vec<CombatCard>,
    pub premium_aoe: Vec<CombatCard>,
    pub access: Vec<CombatCard>,
    pub premium_access: Vec<CombatCard>,
    pub self_damage: Vec<CombatCard>,
    pub curses: Vec<CombatCard>,
}

pub(super) fn card_labels(cards: &[CombatCard]) -> Vec<String> {
    cards.iter().map(card_label).collect()
}

pub(super) fn card_label(card: &CombatCard) -> String {
    format!("{}+{}", get_card_definition(card.id).name, card.upgrades)
}

impl AwakenedOneDeckSignals {
    pub(super) fn from_combat(combat: &CombatState) -> Self {
        let deck = if combat.meta.master_deck_snapshot.is_empty() {
            combat
                .zones
                .hand
                .iter()
                .chain(combat.zones.draw_pile.iter())
                .chain(combat.zones.discard_pile.iter())
                .chain(combat.zones.exhaust_pile.iter())
                .cloned()
                .collect()
        } else {
            combat.meta.master_deck_snapshot.to_vec()
        };
        let has_runic_dome = combat
            .entities
            .player
            .relics
            .iter()
            .any(|relic| relic.id == RelicId::RunicDome);
        Self::from_deck(deck, combat.entities.player.energy_master, has_runic_dome)
    }

    pub(super) fn from_deck(deck: Vec<CombatCard>, energy: u8, has_runic_dome: bool) -> Self {
        let mut signals = Self {
            deck,
            energy,
            has_runic_dome,
            ..Default::default()
        };
        signals.collect();
        signals
    }

    fn collect(&mut self) {
        for card in &self.deck {
            if is_power(card.id) {
                self.powers.push(card.clone());
            }
            if is_damage_scaling(card.id) {
                self.damage_scaling.push(card.clone());
            }
            if card.id == CardId::DemonForm {
                self.slow_damage_scaling.push(card.clone());
            }
            if card.id == CardId::LimitBreak {
                self.scaling_multiplier.push(card.clone());
            }
            if is_defensive_scaling_or_mitigation(card.id) {
                self.defensive_scaling_or_mitigation.push(card.clone());
            }
            if is_mitigation_or_strength_down(card) {
                self.mitigation_or_strength_down.push(card.clone());
            }
            if is_defensive_engine_or_repeatable_block(card.id) {
                self.defensive_engine_or_repeatable_block.push(card.clone());
            }
            if is_big_block(card.id) {
                self.big_block.push(card.clone());
            }
            if matches!(card.id, CardId::Impervious | CardId::PowerThrough) {
                self.reliable_burst_block.push(card.clone());
            }
            if is_generic_block(card.id) {
                self.generic_block.push(card.clone());
            }
            if is_aoe(card.id) {
                self.aoe.push(card.clone());
            }
            if card.id == CardId::Immolate {
                self.premium_aoe.push(card.clone());
            }
            if is_access(card.id) {
                self.access.push(card.clone());
            }
            if matches!(card.id, CardId::Offering | CardId::BattleTrance) {
                self.premium_access.push(card.clone());
            }
            if is_self_damage(card.id) {
                self.self_damage.push(card.clone());
            }
            if is_curse(card.id) {
                self.curses.push(card.clone());
            }
        }
    }
}

fn is_power(card: CardId) -> bool {
    matches!(
        card,
        CardId::DemonForm
            | CardId::Rupture
            | CardId::Barricade
            | CardId::Corruption
            | CardId::FeelNoPain
            | CardId::DarkEmbrace
            | CardId::Inflame
            | CardId::Metallicize
            | CardId::Combust
            | CardId::Brutality
            | CardId::FireBreathing
            | CardId::Evolve
            | CardId::Juggernaut
            | CardId::Berserk
    )
}

fn is_damage_scaling(card: CardId) -> bool {
    matches!(
        card,
        CardId::DemonForm | CardId::Inflame | CardId::SpotWeakness
    )
}

fn is_defensive_scaling_or_mitigation(card: CardId) -> bool {
    matches!(
        card,
        CardId::Disarm
            | CardId::Shockwave
            | CardId::Impervious
            | CardId::PowerThrough
            | CardId::FeelNoPain
            | CardId::SecondWind
            | CardId::Barricade
            | CardId::Entrench
            | CardId::Corruption
            | CardId::TrueGrit
            | CardId::Metallicize
    )
}

fn is_mitigation_or_strength_down(card: &CombatCard) -> bool {
    matches!(
        card.id,
        CardId::Disarm | CardId::Shockwave | CardId::Intimidate
    ) || (card.id == CardId::Uppercut && card.upgrades > 0)
}

fn is_defensive_engine_or_repeatable_block(card: CardId) -> bool {
    matches!(
        card,
        CardId::FeelNoPain | CardId::SecondWind | CardId::Corruption | CardId::Barricade
    )
}

fn is_big_block(card: CardId) -> bool {
    matches!(
        card,
        CardId::Impervious | CardId::PowerThrough | CardId::FlameBarrier
    )
}

fn is_generic_block(card: CardId) -> bool {
    matches!(
        card,
        CardId::Defend
            | CardId::ShrugItOff
            | CardId::Armaments
            | CardId::FlameBarrier
            | CardId::GhostlyArmor
    )
}

fn is_aoe(card: CardId) -> bool {
    matches!(
        card,
        CardId::Whirlwind | CardId::Cleave | CardId::Immolate | CardId::Combust
    )
}

fn is_access(card: CardId) -> bool {
    matches!(
        card,
        CardId::BurningPact
            | CardId::Offering
            | CardId::BattleTrance
            | CardId::ShrugItOff
            | CardId::PommelStrike
    )
}

fn is_self_damage(card: CardId) -> bool {
    matches!(
        card,
        CardId::Offering
            | CardId::Bloodletting
            | CardId::Hemokinesis
            | CardId::Combust
            | CardId::Brutality
    )
}

fn is_curse(card: CardId) -> bool {
    matches!(
        card,
        CardId::Writhe
            | CardId::Normality
            | CardId::Regret
            | CardId::Pain
            | CardId::Parasite
            | CardId::Decay
            | CardId::Doubt
            | CardId::Shame
            | CardId::Injury
            | CardId::Clumsy
            | CardId::CurseOfTheBell
            | CardId::Necronomicurse
    )
}
