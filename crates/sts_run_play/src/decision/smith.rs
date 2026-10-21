//! Upgrade differences and grouping of mechanically identical public copies.
use super::*;
use sts_oracle_learning::content::cards::{self, mechanics, CardId};
use sts_oracle_learning::runtime::combat::CardBlueprint;

pub(super) struct SmithComparison {
    before: CardBlueprint,
    after: CardBlueprint,
    bottled_by: Option<sts_oracle_learning::content::relics::RelicId>,
    pub ordinals: Vec<usize>,
    pub changes: String,
}

pub(super) fn comparisons(
    observation: &TaskStrategicObservationV1,
    options: &[DecisionOptionV1],
) -> Result<Vec<SmithComparison>, String> {
    let mut groups: Vec<SmithComparison> = Vec::new();
    for (ordinal, option) in options.iter().enumerate() {
        let TaskPublicCandidateV1::Strategic {
            action: TaskStrategicActionV1::Smith { card_ordinal, .. },
        } = &option.candidate
        else {
            continue;
        };
        let card = project_deck_cards_v1(&observation.cards, &[*card_ordinal])?[0];
        let before = blueprint(card);
        if let Some(group) = groups
            .iter_mut()
            .find(|g| g.before == before && g.bottled_by == card.bottled_by)
        {
            group.ordinals.push(ordinal);
            continue;
        }
        let mut after = before.clone();
        if !cards::upgrade_card_blueprint_once_java(&mut after) {
            return Err("Legal upgrade candidate could not be upgraded".into());
        }
        let changes = changes(&before, &after).join("; ");
        groups.push(SmithComparison {
            before,
            after,
            bottled_by: card.bottled_by,
            ordinals: vec![ordinal],
            changes,
        });
    }
    Ok(groups)
}

fn blueprint(card: TaskStrategicCardV1) -> CardBlueprint {
    CardBlueprint {
        upgrades: card.upgrades,
        misc_value: card.misc_value,
        base_damage_override: card.base_damage_override,
        base_block_override: card.base_block_override,
        cost_modifier: card.cost_modifier,
        ..CardBlueprint::fresh(card.card)
    }
}

pub(super) fn changes_for(card: TaskStrategicCardV1) -> Result<String, String> {
    let before = blueprint(card);
    let mut after = before.clone();
    if !cards::upgrade_card_blueprint_once_java(&mut after) {
        return Err("Legal upgrade candidate could not be upgraded".into());
    }
    Ok(changes(&before, &after).join("; "))
}

fn changes(before: &CardBlueprint, after: &CardBlueprint) -> Vec<String> {
    let mut changes = Vec::new();
    let old_cost = before.combat_cost_without_turn_override_java();
    let new_cost = after.combat_cost_without_turn_override_java();
    if old_cost != new_cost {
        let label = |cost| match cost {
            -2 => "unplayable".into(),
            -1 => "X".into(),
            n => format!("{n}c"),
        };
        changes.push(format!("cost {} -> {}", label(old_cost), label(new_cost)));
    }
    let mut number = |name: &str, a: i32, b: i32| {
        if a != b {
            changes.push(format!("{name} {a} -> {b}"));
        }
    };
    let old = mechanics::card_blueprint_persistent_base_numbers_v1(before);
    let new = mechanics::card_blueprint_persistent_base_numbers_v1(after);
    number("base damage/hit", old.damage, new.damage);
    number("Block", old.block, new.block);
    let definition = cards::get_card_definition(before.id);
    let magic =
        |upgrades| definition.base_magic + i32::from(upgrades > 0) * definition.upgrade_magic;
    let a = magic(before.upgrades);
    let b = magic(after.upgrades);
    let mut magic_named = false;
    if let (Some(a), Some(b)) = (
        mechanics::unconditional_card_draw_request_v1(before.id, a),
        mechanics::unconditional_card_draw_request_v1(after.id, b),
    ) {
        number("draw", a as i32, b as i32);
        magic_named = true;
        if before.id == CardId::Prepared {
            number("discard", a as i32, b as i32);
        }
    }
    if let Some(delivery) = mechanics::card_player_strength_delivery_v1(before.id) {
        use mechanics::CardPlayerStrengthDeliveryV1::*;
        number(
            match delivery {
                OnPlay => "Strength",
                IfTargetIntentBaseDamageNonnegativeAtResolution => "Strength if target attacks",
                OnPlayWithSeparateLoseStrength => "temporary Strength",
                EachPlayerTurnStartPostDraw => "Strength/turn",
            },
            a,
            b,
        );
        magic_named = true;
    }
    let magic_name = match before.id {
        CardId::Bash | CardId::ThunderClap => Some("Vulnerable"),
        CardId::Clothesline | CardId::Intimidate => Some("Weak"),
        CardId::Uppercut | CardId::Shockwave => Some("Weak/Vulnerable"),
        CardId::Disarm => Some("Strength reduction"),
        CardId::FlameBarrier => Some("retaliation/hit"),
        CardId::PerfectedStrike => Some("damage/Strike card"),
        _ => None,
    };
    if let Some(name) = magic_name {
        number(name, a, b);
        magic_named = true;
    }
    if let (Some(old), Some(new)) = (
        mechanics::card_base_damage_delivery_v1(before.id, before.upgrades),
        mechanics::card_base_damage_delivery_v1(after.id, after.upgrades),
    ) {
        use mechanics::{CardAttackHitsV1::Fixed, CardBaseDamageDeliveryV1::Attack};
        if let (
            Attack {
                hits: Fixed { count: a },
                ..
            },
            Attack {
                hits: Fixed { count: b },
                ..
            },
        ) = (old, new)
        {
            number("hits", a, b);
            if a != b {
                magic_named = true;
            }
        }
    }
    if let (Some(a), Some(b)) = (
        mechanics::card_energy_gain_v1(before.id, before.upgrades, a),
        mechanics::card_energy_gain_v1(after.id, after.upgrades, b),
    ) {
        use mechanics::CardEnergyDeliveryV1::*;
        number(
            match a.delivery {
                OnPlay => "Energy on play",
                EachPlayerTurnStart => "Energy/turn",
                NextPlayerEnergyRecharge => "Energy next turn",
                OnExhaust => "Energy on Exhaust",
                IfTargetVulnerableAtResolution => "Energy if target Vulnerable",
            },
            a.amount,
            b.amount,
        );
        if a.amount != b.amount {
            magic_named = true;
        }
    }
    for (name, a, b) in [
        (
            "Exhaust on play",
            mechanics::card_exhausts_when_played(before.id, before.upgrades),
            mechanics::card_exhausts_when_played(after.id, after.upgrades),
        ),
        (
            "Ethereal",
            mechanics::card_is_ethereal(before.id, before.upgrades),
            mechanics::card_is_ethereal(after.id, after.upgrades),
        ),
        (
            "Innate",
            cards::is_innate_card_for(before.id, before.upgrades),
            cards::is_innate_card_for(after.id, after.upgrades),
        ),
    ] {
        if a != b {
            changes.push(format!("{name} {a} -> {b}"));
        }
    }
    if mechanics::combat_upgrade_scope_for_card_v1(before.id, before.upgrades)
        != mechanics::combat_upgrade_scope_for_card_v1(after.id, after.upgrades)
    {
        changes.push("combat upgrade: one card in hand -> whole hand".into());
    }
    if before.id == CardId::TrueGrit && before.upgrades == 0 {
        changes.push("Exhaust target: random -> chosen".into());
    }
    // Keep the actual rule where a secondary upgrade effect is not yet named.
    // A shorter numeric list must not claim to be a complete upgrade summary.
    if (a != b && !magic_named) || changes.is_empty() {
        changes.push(format!(
            "Rule: {}",
            mechanics::explanation(before.id).unwrap_or("Upgrade effect unknown; query info card")
        ));
    }
    changes
}

pub(super) fn write_full(out: &mut String, groups: &[SmithComparison]) {
    if groups.is_empty() {
        return;
    }
    writeln!(out, "Smith upgrades:").unwrap();
    for group in groups {
        let binding = group.bottled_by.map_or_else(String::new, |relic| {
            format!(" [{}]", gameplay_display_label_v1(relic))
        });
        writeln!(
            out,
            "  {} -> {}{binding} | {}",
            format_card(group.before.id, group.before.upgrades),
            format_card(group.after.id, group.after.upgrades),
            group.changes
        )
        .unwrap();
        if !group.changes.contains("Rule:") {
            writeln!(
                out,
                "    Printed rule (base/upgraded): {}",
                mechanics::explanation(group.before.id)
                    .unwrap_or("Effect unknown; query info card")
            )
            .unwrap();
        }
    }
}
