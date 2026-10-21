use std::collections::{BTreeMap, BTreeSet};
use sts_java_parity_protocol::canonical_game_id;
use sts_oracle_run_control::{
    content::cards::CardId,
    eval::run_control::{RunControlConfig, RunControlSession},
    runtime::combat::{CardLineageId, CardLineageOrigin},
};
use sts_parity_lockstep::{
    NaturalRunBootstrapCardV1, NaturalRunBootstrapV1, NaturalRunCardSourceV1,
    NaturalRunContentProfileV1, NaturalRunNeowEligibilityV1,
};

/// Builds the shared source recipe before either runtime is observed.
///
/// Repeated starter cards remain distinct because their source slots are
/// distinct. Current Java deck order and Java UUID values are deliberately
/// absent from this contract.
pub fn natural_run_bootstrap_v1(
    config: &RunControlConfig,
) -> Result<NaturalRunBootstrapV1, String> {
    let starter_cards = starter_cards(config.player_class)?;
    let player_class = player_class_id(config.player_class)
        .ok_or_else(|| format!("unsupported player class {:?}", config.player_class))?;
    let mut cards = starter_cards
        .iter()
        .enumerate()
        .map(|(source_slot, card)| NaturalRunBootstrapCardV1 {
            binding: format!("natural_run/starter/{source_slot}"),
            source: NaturalRunCardSourceV1::StarterDeck {
                player_class: player_class.to_owned(),
                source_slot: source_slot as u32,
            },
            card_id: canonical_game_id(card.stable_id()),
            upgrades: 0,
        })
        .collect::<Vec<_>>();
    if config.ascension_level >= 10 {
        cards.push(NaturalRunBootstrapCardV1 {
            binding: "natural_run/ascension_10/ascenders_bane".to_owned(),
            source: NaturalRunCardSourceV1::AscendersBane {
                rule: "ascension_10".to_owned(),
            },
            card_id: canonical_game_id(CardId::AscendersBane.stable_id()),
            upgrades: 0,
        });
    }
    NaturalRunBootstrapV1::new(
        config.seed.to_string(),
        config.ascension_level,
        config.final_act,
        NaturalRunContentProfileV1::AllUnlocked,
        match config.neow_eligibility {
            sts_oracle_run_control::content::events::neow::NeowEligibilityV1::Mini => {
                NaturalRunNeowEligibilityV1::Mini
            }
            sts_oracle_run_control::content::events::neow::NeowEligibilityV1::Rich => {
                NaturalRunNeowEligibilityV1::Rich
            }
        },
        player_class,
        cards,
    )
}

pub fn run_control_config_from_bootstrap_v1(
    bootstrap: &NaturalRunBootstrapV1,
) -> Result<RunControlConfig, String> {
    bootstrap.validate()?;
    if bootstrap.content_profile != NaturalRunContentProfileV1::AllUnlocked {
        return Err("unsupported Rust natural-run content profile".to_owned());
    }
    let seed = bootstrap
        .seed
        .parse::<u64>()
        .map_err(|_| "natural-run bootstrap seed is not an unsigned 64-bit integer".to_owned())?;
    let player_class = match bootstrap.player_class.as_str() {
        "IRONCLAD" => "Ironclad",
        "THE_SILENT" => "Silent",
        "DEFECT" => "Defect",
        "WATCHER" => "Watcher",
        other => return Err(format!("unsupported player class {other:?}")),
    };
    let neow_eligibility = match bootstrap.neow_eligibility {
        NaturalRunNeowEligibilityV1::Mini => {
            sts_oracle_run_control::content::events::neow::NeowEligibilityV1::Mini
        }
        NaturalRunNeowEligibilityV1::Rich => {
            sts_oracle_run_control::content::events::neow::NeowEligibilityV1::Rich
        }
    };
    let config = RunControlConfig {
        seed,
        ascension_level: bootstrap.ascension,
        final_act: bootstrap.final_act,
        player_class,
        neow_eligibility,
        ..RunControlConfig::default()
    };
    if natural_run_bootstrap_v1(&config)? != *bootstrap {
        return Err("natural-run bootstrap cannot be reconstructed by the Rust worker".to_owned());
    }
    Ok(config)
}

pub(crate) fn bind_session(
    bootstrap: &NaturalRunBootstrapV1,
    session: &RunControlSession,
) -> Result<BTreeMap<CardLineageId, BoundCardSourceV1>, String> {
    bootstrap.validate()?;
    let run = &session.run_state;
    if bootstrap.seed != run.seed.to_string()
        || bootstrap.ascension != run.ascension_level
        || bootstrap.final_act != run.is_final_act_available
        || bootstrap.player_class
            != player_class_id(run.player_class)
                .ok_or_else(|| format!("unsupported player class {:?}", run.player_class))?
    {
        return Err("natural run bootstrap and Rust session configuration differ".to_owned());
    }

    let starter_count = bootstrap
        .cards
        .iter()
        .filter(|card| matches!(card.source, NaturalRunCardSourceV1::StarterDeck { .. }))
        .count() as u32;
    let mut by_lineage = BTreeMap::new();
    for card in &bootstrap.cards {
        let rust_lineage_ordinal = rust_lineage_ordinal(card, starter_count)?;
        let lineage = CardLineageId::master_deck(rust_lineage_ordinal);
        if by_lineage
            .insert(
                lineage,
                BoundCardSourceV1 {
                    binding: card.binding.clone(),
                    source: card.source.clone(),
                    expected_card_id: card.card_id.clone(),
                    expected_upgrades: card.upgrades,
                },
            )
            .is_some()
        {
            return Err("natural run bootstrap repeats a Rust lineage".to_owned());
        }
    }

    if session.run_state.master_deck.len() != by_lineage.len() {
        return Err(format!(
            "natural run bootstrap has {} cards but Rust starts with {}",
            by_lineage.len(),
            session.run_state.master_deck.len()
        ));
    }
    let mut seen_session_lineages = BTreeSet::new();
    for card in &session.run_state.master_deck {
        if card.lineage.origin() != CardLineageOrigin::MasterDeck {
            return Err("natural Rust starter card has non-master-deck lineage".to_owned());
        }
        if !seen_session_lineages.insert(card.lineage) {
            return Err("natural Rust starter deck repeats one lineage".to_owned());
        }
        let expected = by_lineage.get(&card.lineage).ok_or_else(|| {
            "natural Rust starter lineage is absent from the bootstrap recipe".to_owned()
        })?;
        if expected.expected_card_id != canonical_game_id(card.id.stable_id())
            || expected.expected_upgrades != card.upgrades
        {
            return Err(format!(
                "natural Rust starter lineage {} disagrees with its source recipe",
                card.lineage
            ));
        }
    }
    Ok(by_lineage)
}

fn rust_lineage_ordinal(
    card: &NaturalRunBootstrapCardV1,
    starter_count: u32,
) -> Result<u32, String> {
    match &card.source {
        NaturalRunCardSourceV1::StarterDeck { source_slot, .. } => source_slot
            .checked_add(1)
            .ok_or_else(|| "natural run starter source slot overflows lineage".to_owned()),
        NaturalRunCardSourceV1::AscendersBane { .. } => starter_count
            .checked_add(1)
            .ok_or_else(|| "natural run starter count overflows lineage".to_owned()),
        NaturalRunCardSourceV1::NeowDrawback { .. }
        | NaturalRunCardSourceV1::NeowCardReward { .. }
        | NaturalRunCardSourceV1::CardReward { .. }
        | NaturalRunCardSourceV1::ShopPurchase { .. }
        | NaturalRunCardSourceV1::EventCard { .. }
        | NaturalRunCardSourceV1::EventGrant { .. }
        | NaturalRunCardSourceV1::RelicGrant { .. }
        | NaturalRunCardSourceV1::MonsterGrant { .. }
        | NaturalRunCardSourceV1::CombatBirth { .. } => {
            Err("dynamic card source cannot appear in the natural bootstrap".to_owned())
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct BoundCardSourceV1 {
    pub binding: String,
    pub source: NaturalRunCardSourceV1,
    pub expected_card_id: String,
    pub expected_upgrades: u8,
}

fn player_class_id(player_class: &str) -> Option<&'static str> {
    match player_class {
        "Ironclad" => Some("IRONCLAD"),
        "Silent" => Some("THE_SILENT"),
        "Defect" => Some("DEFECT"),
        "Watcher" => Some("WATCHER"),
        _ => None,
    }
}

fn starter_cards(player_class: &str) -> Result<&'static [CardId], String> {
    match player_class {
        "Ironclad" => Ok(&[
            CardId::Strike,
            CardId::Strike,
            CardId::Strike,
            CardId::Strike,
            CardId::Strike,
            CardId::Defend,
            CardId::Defend,
            CardId::Defend,
            CardId::Defend,
            CardId::Bash,
        ]),
        "Silent" => Ok(&[
            CardId::StrikeG,
            CardId::StrikeG,
            CardId::StrikeG,
            CardId::StrikeG,
            CardId::StrikeG,
            CardId::DefendG,
            CardId::DefendG,
            CardId::DefendG,
            CardId::DefendG,
            CardId::DefendG,
            CardId::Survivor,
            CardId::Neutralize,
        ]),
        "Defect" => Ok(&[
            CardId::StrikeB,
            CardId::StrikeB,
            CardId::StrikeB,
            CardId::StrikeB,
            CardId::DefendB,
            CardId::DefendB,
            CardId::DefendB,
            CardId::DefendB,
            CardId::Zap,
            CardId::Dualcast,
        ]),
        "Watcher" => Ok(&[
            CardId::StrikeP,
            CardId::StrikeP,
            CardId::StrikeP,
            CardId::StrikeP,
            CardId::DefendP,
            CardId::DefendP,
            CardId::DefendP,
            CardId::DefendP,
            CardId::Eruption,
            CardId::Vigilance,
        ]),
        other => Err(format!("unsupported player class {other:?}")),
    }
}
