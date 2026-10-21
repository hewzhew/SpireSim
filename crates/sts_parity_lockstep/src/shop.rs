use crate::{SemanticChoiceV1, SemanticScalarV1};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct NaturalShopCardV1 {
    pub card_id: String,
    pub upgrades: u8,
    pub price: i32,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct NaturalShopItemV1 {
    pub id: String,
    pub price: i32,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct NaturalShopStockV1 {
    pub cards: Vec<NaturalShopCardV1>,
    pub relics: Vec<NaturalShopItemV1>,
    pub potions: Vec<NaturalShopItemV1>,
    pub purge_cost: i32,
    pub purge_available: bool,
}

impl NaturalShopStockV1 {
    pub fn validate(&self) -> Result<(), String> {
        if self.purge_cost < 0
            || self
                .cards
                .iter()
                .any(|item| item.card_id.is_empty() || item.price < 0)
            || self
                .relics
                .iter()
                .chain(&self.potions)
                .any(|item| item.id.is_empty() || item.price < 0)
        {
            return Err("shop inventory has an invalid identity or price".to_owned());
        }
        Ok(())
    }

    pub fn card_choices(&self) -> Vec<SemanticChoiceV1> {
        choices(self.cards.iter().map(|item| {
            (
                "shop_card",
                item.card_id.as_str(),
                item.price,
                Some(item.upgrades),
            )
        }))
    }

    pub fn relic_choices(&self) -> Vec<SemanticChoiceV1> {
        choices(
            self.relics
                .iter()
                .map(|item| ("shop_relic", item.id.as_str(), item.price, None)),
        )
    }

    pub fn potion_choices(&self) -> Vec<SemanticChoiceV1> {
        choices(
            self.potions
                .iter()
                .map(|item| ("shop_potion", item.id.as_str(), item.price, None)),
        )
    }
}

fn choices<'a>(
    items: impl Iterator<Item = (&'a str, &'a str, i32, Option<u8>)>,
) -> Vec<SemanticChoiceV1> {
    let mut occurrences = BTreeMap::new();
    items
        .map(|(kind, id, price, upgrades)| {
            let occurrence = occurrences.entry((id, price, upgrades)).or_insert(0);
            let mut choice = SemanticChoiceV1::new(kind, id, *occurrence);
            *occurrence += 1;
            choice.attributes.insert(
                "price".to_owned(),
                SemanticScalarV1::Integer(i64::from(price)),
            );
            if let Some(upgrades) = upgrades {
                choice.attributes.insert(
                    "upgrades".to_owned(),
                    SemanticScalarV1::Integer(i64::from(upgrades)),
                );
            }
            choice
        })
        .collect()
}
