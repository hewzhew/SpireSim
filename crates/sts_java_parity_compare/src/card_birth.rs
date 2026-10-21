use super::*;
use sts_core::content::potions::PotionId;
use sts_core::runtime::combat::{
    CombatCardBirthCause, CombatCardBirthReceipt, CombatCardBirthZone,
};
#[cfg(test)]
use sts_java_parity_protocol::CardBirthProduct as JavaCardBirthProductProjection;
use sts_java_parity_protocol::{
    CardBirthCause as JavaCardBirthCauseProjection,
    CardBirthWitness as JavaCardBirthWitnessProjection,
    CardBirthZone as JavaCardBirthZoneProjection,
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct JavaCardBirthObservedCardV1 {
    pub java_uuid: String,
    pub card_id: String,
    pub upgrades: u8,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CardBirthTransitionContextV1 {
    pub from_frame_id: u64,
    pub to_frame_id: u64,
    pub request_id: String,
    pub action_id: String,
    pub cause: CardBirthTransitionCauseV1,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CardBirthTransitionCauseV1 {
    CardPlay {
        producer_java_uuid: String,
        producer_rust_lineage: CardLineageId,
    },
    EndTurnResolution,
    PotionUse {
        java_potion_id: String,
        rust_potion_id: PotionId,
        slot: usize,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReconciledCardBirthProductV1 {
    pub generation_index: u32,
    pub java_uuid: String,
    pub rust_card: sts_core::state::DomainCardSnapshot,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) enum CardBindingBirthCauseEvidence {
    CardPlay { producer: JavaCardUuid },
    EndTurnResolution,
}

pub fn reconcile_card_birth_witness_v1(
    context: &CardBirthTransitionContextV1,
    witness: Option<&sts_java_parity_protocol::CardBirthWitness>,
    rust_receipt: &CombatCardBirthReceipt,
    before_cards: &[JavaCardBirthObservedCardV1],
    after_cards: &[JavaCardBirthObservedCardV1],
) -> Result<Vec<ReconciledCardBirthProductV1>, String> {
    let witness = witness.ok_or_else(|| {
        "generated-card transition has no typed Java card-birth witness; identity is Unknown"
            .to_owned()
    })?;
    if witness.kind != "card_birth_witness_v1"
        || witness.from_frame_id != context.from_frame_id
        || witness.to_frame_id != context.to_frame_id
        || witness.request_id != context.request_id
        || witness.action_id != context.action_id
    {
        return Err(
            "Java card-birth witness does not bind the compared action/frame transition".to_owned(),
        );
    }
    if !witness.complete || witness.unsupported_reason.is_some() {
        return Err(format!(
            "Java card-birth witness is incomplete; identity is Unknown{}",
            witness
                .unsupported_reason
                .as_deref()
                .map(|reason| format!(": {reason}"))
                .unwrap_or_default()
        ));
    }
    if !rust_receipt.complete || rust_receipt.unsupported_reason.is_some() {
        return Err(format!(
            "Rust card-birth receipt is incomplete; identity is Unknown{}",
            rust_receipt
                .unsupported_reason
                .as_deref()
                .map(|reason| format!(": {reason}"))
                .unwrap_or_default()
        ));
    }
    match (&context.cause, &witness.cause, &rust_receipt.cause) {
        (
            CardBirthTransitionCauseV1::CardPlay {
                producer_java_uuid: expected_java_uuid,
                producer_rust_lineage: expected_rust_lineage,
            },
            JavaCardBirthCauseProjection::CardPlay { producer_java_uuid },
            CombatCardBirthCause::CardPlay { producer_lineage },
        ) if producer_java_uuid == expected_java_uuid
            && producer_lineage == expected_rust_lineage => {}
        (
            CardBirthTransitionCauseV1::EndTurnResolution,
            JavaCardBirthCauseProjection::EndTurnResolution,
            CombatCardBirthCause::EndTurnResolution,
        ) => {}
        (
            CardBirthTransitionCauseV1::PotionUse {
                java_potion_id,
                rust_potion_id,
                slot,
            },
            JavaCardBirthCauseProjection::PotionUse {
                potion_id: witnessed_java_potion_id,
                slot: witnessed_java_slot,
            },
            CombatCardBirthCause::PotionUse {
                potion_id: witnessed_rust_potion_id,
                slot: witnessed_rust_slot,
            },
        ) if witnessed_java_potion_id == java_potion_id
            && witnessed_rust_potion_id == rust_potion_id
            && witnessed_java_slot == slot
            && witnessed_rust_slot == slot => {}
        (_, JavaCardBirthCauseProjection::RelicTrigger { .. }, _)
        | (_, JavaCardBirthCauseProjection::PowerTrigger { .. }, _)
        | (_, JavaCardBirthCauseProjection::EncounterRule { .. }, _) => {
            return Err(
                "this reconciler does not yet own the witnessed non-card-play birth cause; identity is Unknown"
                    .to_owned(),
            );
        }
        _ => {
            return Err(
                "Java and Rust card-birth witnesses disagree about their typed card-play producer; identity is Unknown"
                    .to_owned(),
            );
        }
    }
    validate_sequential_indices(
        witness
            .products
            .iter()
            .map(|product| product.generation_index),
        "Java",
    )?;
    validate_sequential_indices(
        rust_receipt
            .products
            .iter()
            .map(|product| product.generation_index),
        "Rust",
    )?;
    if witness.products.len() != rust_receipt.products.len()
        || usize::try_from(rust_receipt.allocated_count) != Ok(rust_receipt.products.len())
    {
        return Err(
            "Java and Rust card-birth receipts contain different product counts; identity is Unknown"
                .to_owned(),
        );
    }

    let before_java = unique_observed_java_cards(before_cards)?;
    let after_java = unique_observed_java_cards(after_cards)?;
    let fresh_after_java = after_java
        .iter()
        .filter(|(uuid, _)| !before_java.contains_key(*uuid))
        .map(|(uuid, card)| ((*uuid).to_owned(), *card))
        .collect::<BTreeMap<_, _>>();
    if fresh_after_java.len() != witness.products.len()
        || witness.products.iter().any(|product| {
            fresh_after_java
                .get(product.java_uuid.as_str())
                .is_none_or(|card| {
                    card.card_id != product.card_id || card.upgrades != product.upgrades
                })
        })
    {
        return Err(
            "Java card-birth products are not a complete semantic cover of fresh successor UUIDs"
                .to_owned(),
        );
    }

    witness
        .products
        .iter()
        .zip(&rust_receipt.products)
        .map(|(java, rust)| {
            if java.generation_index != rust.generation_index
                || java.card_id != java_id(rust.card.id)
                || java.upgrades != rust.card.upgrades
                || rust.first_zone.is_none()
                || java.autoplay_copy.is_some()
                || rust.autoplay_copy.is_some()
                || java.first_zone != rust.first_zone.map(java_zone)
                || rust.card.lineage.origin() != CardLineageOrigin::CombatGenerated
            {
                return Err(
                    "Java and Rust card-birth products disagree in ordered lineage semantics"
                        .to_owned(),
                );
            }
            Ok(ReconciledCardBirthProductV1 {
                generation_index: java.generation_index,
                java_uuid: java.java_uuid.clone(),
                rust_card: rust.card,
            })
        })
        .collect()
}

fn unique_observed_java_cards(
    cards: &[JavaCardBirthObservedCardV1],
) -> Result<BTreeMap<&str, &JavaCardBirthObservedCardV1>, String> {
    let mut unique = BTreeMap::new();
    for card in cards {
        if let Some(existing) = unique.insert(card.java_uuid.as_str(), card) {
            if existing.card_id != card.card_id || existing.upgrades != card.upgrades {
                return Err(
                    "one Java UUID has conflicting stable semantics across successor zones"
                        .to_owned(),
                );
            }
        }
    }
    Ok(unique)
}

pub(super) fn bind_card_birth_witness(
    bindings: &mut CrossRuntimeCardBindings,
    before: &StateFrameProjection,
    after: &StateFrameProjection,
    action_receipt: &ActionReceipt,
    rust_receipt: &CombatCardBirthReceipt,
) -> Result<(), String> {
    bind_card_birth_witness_parts(
        bindings,
        &before.parity_internal.zones,
        &after.parity_internal.zones,
        before.frame_id,
        after.frame_id,
        &action_receipt.request_id,
        &action_receipt.action_id,
        after.card_birth_witness.as_ref(),
        rust_receipt,
    )
}

#[allow(clippy::too_many_arguments)]
fn bind_card_birth_witness_parts(
    bindings: &mut CrossRuntimeCardBindings,
    before_zones: &ParityZonesProjection,
    after_zones: &ParityZonesProjection,
    from_frame_id: u64,
    to_frame_id: u64,
    request_id: &str,
    action_id: &str,
    witness: Option<&JavaCardBirthWitnessProjection>,
    rust_receipt: &CombatCardBirthReceipt,
) -> Result<(), String> {
    let witness = witness.ok_or_else(|| {
        "generated-card transition has no typed Java card-birth witness; identity is Unknown"
            .to_owned()
    })?;
    if witness.kind != "card_birth_witness_v1"
        || witness.from_frame_id != from_frame_id
        || witness.to_frame_id != to_frame_id
        || witness.request_id != request_id
        || witness.action_id != action_id
    {
        return Err(
            "Java card-birth witness does not bind the compared action/frame transition".to_owned(),
        );
    }
    if !witness.complete || witness.unsupported_reason.is_some() {
        return Err(format!(
            "Java card-birth witness is incomplete; identity is Unknown{}",
            witness
                .unsupported_reason
                .as_deref()
                .map(|reason| format!(": {reason}"))
                .unwrap_or_default()
        ));
    }
    if !rust_receipt.complete || rust_receipt.unsupported_reason.is_some() {
        return Err(format!(
            "Rust card-birth receipt is incomplete; identity is Unknown{}",
            rust_receipt
                .unsupported_reason
                .as_deref()
                .map(|reason| format!(": {reason}"))
                .unwrap_or_default()
        ));
    }

    let binding_cause = match (&witness.cause, &rust_receipt.cause) {
        (
            JavaCardBirthCauseProjection::CardPlay { producer_java_uuid },
            CombatCardBirthCause::CardPlay { producer_lineage },
        ) => {
            let bound = bindings
                .java_uuid(*producer_lineage)
                .ok_or_else(|| "Rust card-birth producer lineage is not bound to Java".to_owned())?;
            if bound.as_str() != producer_java_uuid {
                return Err(
                    "Java and Rust card-birth witnesses name different card-play producers"
                        .to_owned(),
                );
            }
            CardBindingBirthCauseEvidence::CardPlay {
                producer: JavaCardUuid::new(producer_java_uuid.clone())?,
            }
        }
        (
            JavaCardBirthCauseProjection::EndTurnResolution,
            CombatCardBirthCause::EndTurnResolution,
        ) => CardBindingBirthCauseEvidence::EndTurnResolution,
        (JavaCardBirthCauseProjection::PotionUse { potion_id, slot }, _) => {
            let _ = (potion_id, slot);
            return Err(
                "this comparator does not yet reconcile the witnessed potion birth cause; identity is Unknown"
                    .to_owned(),
            );
        }
        (JavaCardBirthCauseProjection::RelicTrigger { relic_id }, _) => {
            let _ = relic_id;
            return Err(
                "this comparator does not yet reconcile the witnessed relic birth cause; identity is Unknown"
                    .to_owned(),
            );
        }
        (JavaCardBirthCauseProjection::EncounterRule { encounter_id }, _) => {
            let _ = encounter_id;
            return Err(
                "this comparator does not yet reconcile the witnessed encounter birth cause; identity is Unknown"
                    .to_owned(),
            );
        }
        (
            JavaCardBirthCauseProjection::PowerTrigger {
                power_id,
                owner_instance_id,
            },
            _,
        ) => {
            let _ = (power_id, owner_instance_id);
            return Err(
                "this comparator does not yet reconcile the witnessed power birth cause; identity is Unknown"
                    .to_owned(),
            );
        }
        _ => {
            return Err(
                "Java and Rust card-birth witnesses disagree about their typed cause; identity is Unknown"
                    .to_owned(),
            )
        }
    };

    validate_sequential_indices(
        witness
            .products
            .iter()
            .map(|product| product.generation_index),
        "Java",
    )?;
    validate_sequential_indices(
        rust_receipt
            .products
            .iter()
            .map(|product| product.generation_index),
        "Rust",
    )?;
    if witness.products.len() != rust_receipt.products.len()
        || usize::try_from(rust_receipt.allocated_count) != Ok(rust_receipt.products.len())
    {
        return Err(
            "Java and Rust card-birth receipts contain different product counts; identity is Unknown"
                .to_owned(),
        );
    }

    let before_java = unique_java_cards(before_zones)?;
    let after_java = unique_java_cards(after_zones)?;
    let fresh_after_java = after_java
        .iter()
        .filter(|(uuid, _)| !before_java.contains_key(*uuid))
        .map(|(uuid, card)| ((*uuid).to_owned(), (*card).clone()))
        .collect::<BTreeMap<_, _>>();
    if fresh_after_java.len() != witness.products.len()
        || witness.products.iter().any(|product| {
            fresh_after_java
                .get(product.java_uuid.as_str())
                .is_none_or(|card| {
                    card.card_id != product.card_id || card.upgrades != product.upgrades
                })
        })
    {
        return Err(
            "Java card-birth products are not a complete semantic cover of fresh successor UUIDs"
                .to_owned(),
        );
    }

    for (java, rust) in witness.products.iter().zip(&rust_receipt.products) {
        if java.generation_index != rust.generation_index
            || java.card_id != java_id(rust.card.id)
            || java.upgrades != rust.card.upgrades
            || rust.first_zone.is_none()
            || java.autoplay_copy.is_some()
            || rust.autoplay_copy.is_some()
            || java.first_zone != rust.first_zone.map(java_zone)
            || rust.card.lineage.origin() != CardLineageOrigin::CombatGenerated
            || bindings.contains_rust(rust.card.lineage)
        {
            return Err(
                "Java and Rust card-birth products disagree in ordered lineage semantics"
                    .to_owned(),
            );
        }
        bindings.bind(
            JavaCardUuid::new(java.java_uuid.clone())?,
            rust.card.lineage,
            CardBindingEvidence::CapturedBirth {
                cause: binding_cause.clone(),
                from_frame_id,
                to_frame_id,
                generation_index: java.generation_index,
            },
        )?;
    }
    Ok(())
}

fn validate_sequential_indices(
    indices: impl IntoIterator<Item = u32>,
    owner: &str,
) -> Result<(), String> {
    for (index, generation_index) in indices.into_iter().enumerate() {
        if u32::try_from(index) != Ok(generation_index) {
            return Err(format!(
                "{owner} card-birth generation indices are not contiguous from zero"
            ));
        }
    }
    Ok(())
}

fn unique_java_cards(
    zones: &ParityZonesProjection,
) -> Result<BTreeMap<&str, &CardProjection>, String> {
    let mut cards = BTreeMap::new();
    for card in zones
        .draw
        .cards
        .iter()
        .chain(&zones.discard.cards)
        .chain(&zones.exhaust.cards)
        .chain(&zones.hand.cards)
        .chain(&zones.limbo.cards)
    {
        if let Some(existing) = cards.insert(card.instance_uuid.as_str(), card) {
            if existing.card_id != card.card_id || existing.upgrades != card.upgrades {
                return Err(
                    "one Java UUID has conflicting stable semantics across successor zones"
                        .to_owned(),
                );
            }
        }
    }
    Ok(cards)
}

fn java_zone(zone: CombatCardBirthZone) -> JavaCardBirthZoneProjection {
    match zone {
        CombatCardBirthZone::Draw => JavaCardBirthZoneProjection::Draw,
        CombatCardBirthZone::Hand => JavaCardBirthZoneProjection::Hand,
        CombatCardBirthZone::Discard => JavaCardBirthZoneProjection::Discard,
        CombatCardBirthZone::Exhaust => JavaCardBirthZoneProjection::Exhaust,
        CombatCardBirthZone::Limbo => JavaCardBirthZoneProjection::Limbo,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sts_core::content::cards::CardId;
    use sts_core::runtime::combat::{
        CardLineageId, CombatCardBirthProduct, CombatCardBirthReceipt,
    };
    use sts_core::state::DomainCardSnapshot;

    fn card(uuid: &str, card_id: &str, cost: i32) -> CardProjection {
        CardProjection {
            instance_uuid: uuid.to_owned(),
            card_id: card_id.to_owned(),
            upgrades: 0,
            cost_for_turn: cost,
        }
    }

    fn zone(cards: Vec<CardProjection>) -> OrderedZoneProjection {
        OrderedZoneProjection {
            ordering: "java_group_index_ascending".to_owned(),
            top: "last".to_owned(),
            cards,
        }
    }

    fn zones(hand: Vec<CardProjection>, discard: Vec<CardProjection>) -> ParityZonesProjection {
        ParityZonesProjection {
            draw: zone(Vec::new()),
            discard: zone(discard),
            exhaust: zone(Vec::new()),
            hand: zone(hand),
            limbo: zone(Vec::new()),
        }
    }

    fn witness(products: Vec<JavaCardBirthProductProjection>) -> JavaCardBirthWitnessProjection {
        JavaCardBirthWitnessProjection {
            kind: "card_birth_witness_v1".to_owned(),
            from_frame_id: 7,
            to_frame_id: 8,
            request_id: "request-1".to_owned(),
            action_id: "play_card:power:none".to_owned(),
            cause: JavaCardBirthCauseProjection::CardPlay {
                producer_java_uuid: "power".to_owned(),
            },
            complete: true,
            products,
            unsupported_reason: None,
        }
    }

    fn java_product(
        generation_index: u32,
        uuid: &str,
        zone: JavaCardBirthZoneProjection,
    ) -> JavaCardBirthProductProjection {
        JavaCardBirthProductProjection {
            generation_index,
            java_uuid: uuid.to_owned(),
            card_id: "Wound".to_owned(),
            upgrades: 0,
            first_zone: Some(zone),
            autoplay_copy: None,
        }
    }

    fn rust_receipt(products: Vec<CombatCardBirthProduct>) -> CombatCardBirthReceipt {
        CombatCardBirthReceipt {
            cause: CombatCardBirthCause::CardPlay {
                producer_lineage: CardLineageId::constructed_scenario(1),
            },
            allocated_count: u32::try_from(products.len()).unwrap(),
            complete: true,
            products,
            unsupported_reason: None,
        }
    }

    fn rust_product(
        generation_index: u32,
        ordinal: u32,
        zone: CombatCardBirthZone,
    ) -> CombatCardBirthProduct {
        CombatCardBirthProduct {
            generation_index,
            card: DomainCardSnapshot {
                id: CardId::Wound,
                upgrades: 0,
                lineage: CardLineageId::combat_generated(ordinal),
            },
            first_zone: Some(zone),
            autoplay_copy: None,
        }
    }

    fn bind(
        bindings: &mut CrossRuntimeCardBindings,
        before: &ParityZonesProjection,
        after: &ParityZonesProjection,
        witness: Option<&JavaCardBirthWitnessProjection>,
        receipt: &CombatCardBirthReceipt,
    ) -> Result<(), String> {
        bind_card_birth_witness_parts(
            bindings,
            before,
            after,
            7,
            8,
            "request-1",
            "play_card:power:none",
            witness,
            receipt,
        )
    }

    #[test]
    fn ordinary_card_birth_does_not_bind_products_without_a_concrete_first_zone() {
        let before = zones(vec![card("power", POWER_THROUGH_JAVA_ID, 1)], vec![]);
        let after = zones(
            vec![card("born", "Wound", -2)],
            vec![card("power", POWER_THROUGH_JAVA_ID, 1)],
        );
        let mut java = java_product(0, "born", JavaCardBirthZoneProjection::Hand);
        let mut rust = rust_product(0, 1, CombatCardBirthZone::Hand);
        bind(
            &mut compile_captured_scenario_bindings(&before).unwrap(),
            &before,
            &after,
            Some(&witness(vec![java.clone()])),
            &rust_receipt(vec![rust.clone()]),
        )
        .unwrap();
        java.first_zone = None;
        rust.first_zone = None;
        let mut bindings = compile_captured_scenario_bindings(&before).unwrap();
        let error = bind(
            &mut bindings,
            &before,
            &after,
            Some(&witness(vec![java])),
            &rust_receipt(vec![rust]),
        )
        .unwrap_err();
        assert!(error.contains("ordered lineage semantics"));
        assert!(!bindings.contains_rust(CardLineageId::combat_generated(1)));
    }

    #[test]
    fn typed_birth_witness_binds_two_ordered_hand_products() {
        let before = zones(
            vec![
                card("power", POWER_THROUGH_JAVA_ID, 1),
                card("strike", BASE_STRIKE_JAVA_ID, 1),
            ],
            Vec::new(),
        );
        let after = zones(
            vec![
                card("strike", BASE_STRIKE_JAVA_ID, 1),
                card("wound-a", "Wound", -2),
                card("wound-b", "Wound", -2),
            ],
            vec![card("power", POWER_THROUGH_JAVA_ID, 1)],
        );
        let witness = witness(vec![
            java_product(0, "wound-a", JavaCardBirthZoneProjection::Hand),
            java_product(1, "wound-b", JavaCardBirthZoneProjection::Hand),
        ]);
        let receipt = rust_receipt(vec![
            rust_product(0, 1, CombatCardBirthZone::Hand),
            rust_product(1, 2, CombatCardBirthZone::Hand),
        ]);
        let mut bindings = compile_captured_scenario_bindings(&before).unwrap();

        bind(&mut bindings, &before, &after, Some(&witness), &receipt).unwrap();
        assert_eq!(
            bindings.rust_lineage("wound-a"),
            Some(CardLineageId::combat_generated(1))
        );
        assert_eq!(
            bindings.rust_lineage("wound-b"),
            Some(CardLineageId::combat_generated(2))
        );
    }

    #[test]
    fn missing_or_malformed_birth_evidence_is_unknown() {
        let before = zones(vec![card("power", POWER_THROUGH_JAVA_ID, 1)], Vec::new());
        let after = zones(
            vec![card("wound-a", "Wound", -2)],
            vec![card("power", POWER_THROUGH_JAVA_ID, 1)],
        );
        let receipt = rust_receipt(vec![rust_product(0, 1, CombatCardBirthZone::Hand)]);
        let mut bindings = compile_captured_scenario_bindings(&before).unwrap();
        assert!(bind(&mut bindings, &before, &after, None, &receipt)
            .unwrap_err()
            .contains("Unknown"));

        let bad_index = witness(vec![java_product(
            1,
            "wound-a",
            JavaCardBirthZoneProjection::Hand,
        )]);
        let mut bindings = compile_captured_scenario_bindings(&before).unwrap();
        assert!(bind(&mut bindings, &before, &after, Some(&bad_index), &receipt).is_err());

        let bad_zone = witness(vec![java_product(
            0,
            "wound-a",
            JavaCardBirthZoneProjection::Discard,
        )]);
        let mut bindings = compile_captured_scenario_bindings(&before).unwrap();
        assert!(bind(&mut bindings, &before, &after, Some(&bad_zone), &receipt).is_err());
    }

    #[test]
    fn same_java_uuid_replay_allocates_no_new_binding() {
        let before = zones(
            vec![
                card("power", POWER_THROUGH_JAVA_ID, 1),
                card("same", "Wound", -2),
            ],
            Vec::new(),
        );
        let after = zones(
            vec![card("same", "Wound", -2)],
            vec![card("power", POWER_THROUGH_JAVA_ID, 1)],
        );
        let witness = witness(Vec::new());
        let receipt = rust_receipt(Vec::new());
        let mut bindings = compile_captured_scenario_bindings(&before).unwrap();
        let original_count = bindings.java_to_rust.len();

        bind(&mut bindings, &before, &after, Some(&witness), &receipt).unwrap();
        assert_eq!(bindings.java_to_rust.len(), original_count);
    }

    #[test]
    fn end_turn_resolution_reconciles_one_ordered_status_birth() {
        let witness = JavaCardBirthWitnessProjection {
            kind: "card_birth_witness_v1".to_owned(),
            from_frame_id: 7,
            to_frame_id: 8,
            request_id: "request-1".to_owned(),
            action_id: "end_turn".to_owned(),
            cause: JavaCardBirthCauseProjection::EndTurnResolution,
            complete: true,
            products: vec![JavaCardBirthProductProjection {
                generation_index: 0,
                java_uuid: "slimed".to_owned(),
                card_id: "Slimed".to_owned(),
                upgrades: 0,
                first_zone: Some(JavaCardBirthZoneProjection::Discard),
                autoplay_copy: None,
            }],
            unsupported_reason: None,
        };
        let receipt = CombatCardBirthReceipt {
            cause: CombatCardBirthCause::EndTurnResolution,
            allocated_count: 1,
            complete: true,
            products: vec![CombatCardBirthProduct {
                generation_index: 0,
                card: DomainCardSnapshot {
                    id: CardId::Slimed,
                    upgrades: 0,
                    lineage: CardLineageId::combat_generated(1),
                },
                first_zone: Some(CombatCardBirthZone::Discard),
                autoplay_copy: None,
            }],
            unsupported_reason: None,
        };
        let context = CardBirthTransitionContextV1 {
            from_frame_id: 7,
            to_frame_id: 8,
            request_id: "request-1".to_owned(),
            action_id: "end_turn".to_owned(),
            cause: CardBirthTransitionCauseV1::EndTurnResolution,
        };
        let before = vec![JavaCardBirthObservedCardV1 {
            java_uuid: "strike".to_owned(),
            card_id: BASE_STRIKE_JAVA_ID.to_owned(),
            upgrades: 0,
        }];
        let after = vec![
            before[0].clone(),
            JavaCardBirthObservedCardV1 {
                java_uuid: "slimed".to_owned(),
                card_id: "Slimed".to_owned(),
                upgrades: 0,
            },
        ];

        let reconciled =
            reconcile_card_birth_witness_v1(&context, Some(&witness), &receipt, &before, &after)
                .unwrap();
        assert_eq!(reconciled.len(), 1);
        assert_eq!(reconciled[0].java_uuid, "slimed");
        assert_eq!(reconciled[0].rust_card.id, CardId::Slimed);
    }

    #[test]
    fn potion_use_reconciles_one_ordered_birth_with_both_runtime_identities() {
        let witness = JavaCardBirthWitnessProjection {
            kind: "card_birth_witness_v1".to_owned(),
            from_frame_id: 11,
            to_frame_id: 12,
            request_id: "request-potion".to_owned(),
            action_id: "use_potion:0:Attack Potion:none".to_owned(),
            cause: JavaCardBirthCauseProjection::PotionUse {
                potion_id: "Attack Potion".to_owned(),
                slot: 0,
            },
            complete: true,
            products: vec![JavaCardBirthProductProjection {
                generation_index: 0,
                java_uuid: "generated".to_owned(),
                card_id: "Slimed".to_owned(),
                upgrades: 0,
                first_zone: Some(JavaCardBirthZoneProjection::Hand),
                autoplay_copy: None,
            }],
            unsupported_reason: None,
        };
        let receipt = CombatCardBirthReceipt {
            cause: CombatCardBirthCause::PotionUse {
                potion_id: PotionId::AttackPotion,
                slot: 0,
            },
            allocated_count: 1,
            complete: true,
            products: vec![CombatCardBirthProduct {
                generation_index: 0,
                card: DomainCardSnapshot {
                    id: CardId::Slimed,
                    upgrades: 0,
                    lineage: CardLineageId::combat_generated(4),
                },
                first_zone: Some(CombatCardBirthZone::Hand),
                autoplay_copy: None,
            }],
            unsupported_reason: None,
        };
        let context = CardBirthTransitionContextV1 {
            from_frame_id: 11,
            to_frame_id: 12,
            request_id: "request-potion".to_owned(),
            action_id: "use_potion:0:Attack Potion:none".to_owned(),
            cause: CardBirthTransitionCauseV1::PotionUse {
                java_potion_id: "Attack Potion".to_owned(),
                rust_potion_id: PotionId::AttackPotion,
                slot: 0,
            },
        };
        let before = vec![JavaCardBirthObservedCardV1 {
            java_uuid: "strike".to_owned(),
            card_id: BASE_STRIKE_JAVA_ID.to_owned(),
            upgrades: 0,
        }];
        let after = vec![
            before[0].clone(),
            JavaCardBirthObservedCardV1 {
                java_uuid: "generated".to_owned(),
                card_id: "Slimed".to_owned(),
                upgrades: 0,
            },
        ];

        let reconciled =
            reconcile_card_birth_witness_v1(&context, Some(&witness), &receipt, &before, &after)
                .unwrap();
        assert_eq!(reconciled.len(), 1);
        assert_eq!(reconciled[0].java_uuid, "generated");
        assert_eq!(
            reconciled[0].rust_card.lineage,
            CardLineageId::combat_generated(4)
        );
    }
}
