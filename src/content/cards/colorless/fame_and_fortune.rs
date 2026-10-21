use crate::content::cards::{CardDefinition, CardId, CardRarity, CardTarget, CardType};

pub fn definition() -> CardDefinition {
    CardDefinition {
        id: CardId::FameAndFortune,
        name: "Fame and Fortune",
        card_type: CardType::Skill,
        rarity: CardRarity::Special,
        cost: -2,
        base_damage: 0,
        base_block: 0,
        base_magic: 25,
        target: CardTarget::None,
        is_multi_damage: false,
        exhaust: false,
        ethereal: false,
        innate: false,
        tags: &[],
        upgrade_damage: 0,
        upgrade_block: 0,
        upgrade_magic: 5,
    }
}
