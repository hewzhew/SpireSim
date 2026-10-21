//! Retained local-delta reconstructions for the pre-PairedRoot comparator.
//!
//! These helpers deliberately use a neutral Jaw Worm-shaped monster container.
//! They prove only the explicitly checked card/potion delta and must never be
//! used to construct, preflight, or certify a complete PairedRoot combat state.

use super::*;

pub(super) fn reconstruct_potion_combat(
    before: &StateFrameProjection,
    slot: usize,
    potion_id: PotionId,
) -> Result<
    (
        sts_core::runtime::combat::CombatState,
        CrossRuntimeCardBindings,
    ),
    String,
> {
    let zones = &before.parity_internal.zones;
    let identities = compile_captured_scenario_bindings(zones)?;
    let mut combat = sts_core::test_support::blank_test_combat();
    combat.meta.ascension_level = before.public_base.run.ascension;
    combat.turn.turn_count = before.public_base.combat.turn;
    combat.turn.set_energy(
        u8::try_from(before.public_base.combat.player.energy)
            .map_err(|_| "captured energy does not fit simulator u8".to_owned())?,
    );
    combat.entities.player.current_hp = before.public_base.combat.player.current_hp;
    combat.entities.player.max_hp = before.public_base.combat.player.max_hp;
    combat.entities.player.block = before.public_base.combat.player.block;
    combat.zones.hand = build_cards(&zones.hand.cards, &identities)?;
    combat.zones.discard_pile = build_cards(&zones.discard.cards, &identities)?.into();
    combat.zones.exhaust_pile = build_cards(&zones.exhaust.cards, &identities)?.into();
    combat.zones.limbo = build_cards(&zones.limbo.cards, &identities)?;
    let mut rust_draw = build_cards(&zones.draw.cards, &identities)?;
    rust_draw.reverse();
    combat.zones.draw_pile = rust_draw.into();
    combat.entities.monsters = before
        .public_base
        .combat
        .monsters
        .iter()
        .enumerate()
        .map(|(index, captured)| {
            let mut monster = sts_core::test_support::test_monster(EnemyId::JawWorm);
            monster.id = index + 1;
            monster.slot = u8::try_from(index).expect("small captured monster index");
            monster.current_hp = captured.current_hp;
            monster.max_hp = captured.max_hp;
            monster.block = captured.block;
            monster
        })
        .collect();
    combat.entities.potions = vec![None; before.public_base.combat.potions.len()];
    let selected = combat
        .entities
        .potions
        .get_mut(slot)
        .ok_or_else(|| "captured potion slot does not fit reconstructed slots".to_owned())?;
    *selected = Some(Potion::new(potion_id, 1));
    inject_rng_streams(&mut combat.rng.pool, &before.rng_internal.streams)?;
    combat.update_hand_cards();
    Ok((combat, identities))
}

pub(super) fn reconstruct_card_combat_base(
    before: &StateFrameProjection,
) -> Result<
    (
        sts_core::runtime::combat::CombatState,
        CrossRuntimeCardBindings,
    ),
    String,
> {
    let zones = &before.parity_internal.zones;
    let identities = compile_captured_scenario_bindings(zones)?;
    let mut combat = sts_core::test_support::blank_test_combat();
    combat.meta.ascension_level = before.public_base.run.ascension;
    combat.turn.turn_count = before.public_base.combat.turn;
    combat.turn.set_energy(
        u8::try_from(before.public_base.combat.player.energy)
            .map_err(|_| "captured energy does not fit simulator u8".to_owned())?,
    );
    combat.entities.player.current_hp = before.public_base.combat.player.current_hp;
    combat.entities.player.max_hp = before.public_base.combat.player.max_hp;
    combat.entities.player.block = before.public_base.combat.player.block;
    combat.zones.hand = build_cards(&zones.hand.cards, &identities)?;
    combat.zones.discard_pile = build_cards(&zones.discard.cards, &identities)?.into();
    combat.zones.exhaust_pile = build_cards(&zones.exhaust.cards, &identities)?.into();
    combat.zones.limbo = build_cards(&zones.limbo.cards, &identities)?;
    let mut rust_draw = build_cards(&zones.draw.cards, &identities)?;
    rust_draw.reverse();
    combat.zones.draw_pile = rust_draw.into();
    combat.entities.monsters = before
        .public_base
        .combat
        .monsters
        .iter()
        .enumerate()
        .map(|(index, captured)| {
            if captured.index != index || captured.dead_or_escaped {
                return Err("plain card reconstruction requires ordered living monsters".to_owned());
            }
            let mut monster = sts_core::test_support::test_monster(EnemyId::JawWorm);
            monster.id = index + 1;
            monster.slot = u8::try_from(index)
                .map_err(|_| "captured monster index does not fit simulator slot".to_owned())?;
            monster.current_hp = captured.current_hp;
            monster.max_hp = captured.max_hp;
            monster.block = captured.block;
            Ok(monster)
        })
        .collect::<Result<_, String>>()?;
    if combat.entities.monsters.is_empty() {
        return Err("plain card reconstruction requires a nonempty monster set".to_owned());
    }
    inject_rng_streams(&mut combat.rng.pool, &before.rng_internal.streams)?;
    combat.update_hand_cards();
    Ok((combat, identities))
}
