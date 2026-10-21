use super::PotionId;
use crate::content::cards::make_constructed_temp_card_in_hand_action;
use crate::content::powers::PowerId;
use crate::runtime::action::{Action, ActionInfo, AddTo, DamageInfo, DamageType, NO_SOURCE};
use crate::runtime::combat::CombatState;
use crate::EntityId;
use smallvec::SmallVec;

/// Player entity ID constant
const PLAYER: EntityId = 0;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PotionCardDiscoverySpec {
    pub colorless: bool,
    pub card_type: Option<crate::content::cards::CardType>,
    pub cost_for_turn: Option<u8>,
    pub can_skip: bool,
}

pub fn card_discovery_spec(potion: PotionId) -> Option<PotionCardDiscoverySpec> {
    use crate::content::cards::CardType;

    match potion {
        PotionId::AttackPotion => Some(PotionCardDiscoverySpec {
            colorless: false,
            card_type: Some(CardType::Attack),
            cost_for_turn: Some(0),
            can_skip: true,
        }),
        PotionId::SkillPotion => Some(PotionCardDiscoverySpec {
            colorless: false,
            card_type: Some(CardType::Skill),
            cost_for_turn: Some(0),
            can_skip: true,
        }),
        PotionId::PowerPotion => Some(PotionCardDiscoverySpec {
            colorless: false,
            card_type: Some(CardType::Power),
            cost_for_turn: Some(0),
            can_skip: true,
        }),
        PotionId::ColorlessPotion => Some(PotionCardDiscoverySpec {
            colorless: true,
            card_type: None,
            cost_for_turn: Some(0),
            can_skip: false,
        }),
        _ => None,
    }
}

/// Helper: push a single action to the bottom
fn bottom(actions: &mut SmallVec<[ActionInfo; 4]>, action: Action) {
    actions.push(ActionInfo {
        action,
        insertion_mode: AddTo::Bottom,
    });
}

/// Generates the actions applied when a potion is used in combat.
/// `target_idx`: Some(enemy_entity_id) for targeted/thrown potions, None for self.
/// `potency`: the effective potency (base * SacredBark multiplier).
pub fn get_potion_actions(
    state: &CombatState,
    potion: PotionId,
    target_idx: Option<usize>,
    potency: i32,
) -> SmallVec<[ActionInfo; 4]> {
    let mut actions = SmallVec::new();
    let target = target_idx.unwrap_or(0) as EntityId;
    let enemy_count = state.entities.monsters.len();

    match potion {
        // ────────────── Common (20) ──────────────
        PotionId::FirePotion => {
            // Deal 20 damage to target enemy (thrown). Java: DamageType.THORNS
            bottom(
                &mut actions,
                Action::Damage(DamageInfo {
                    source: PLAYER,
                    target,
                    base: potency,
                    output: potency,
                    damage_type: DamageType::Thorns,
                    is_modified: false,
                }),
            );
        }
        PotionId::ExplosivePotion => {
            // Deal 10 damage to ALL enemies (thrown)
            bottom(
                &mut actions,
                Action::DamageAllEnemies {
                    // Java uses DamageAllEnemiesAction(null, ..., NORMAL), so this
                    // damage should not look like a player-owned normal attack for
                    // hooks such as Curl Up.
                    source: NO_SOURCE,
                    damages: crate::runtime::action::repeated_damage_matrix(enemy_count, potency),
                    damage_type: DamageType::Normal,
                    is_modified: false,
                },
            );
        }
        PotionId::PoisonPotion => {
            // Apply 6 Poison to target enemy (thrown)
            bottom(
                &mut actions,
                Action::ApplyPower {
                    source: PLAYER,
                    target,
                    power_id: PowerId::Poison,
                    amount: potency,
                },
            );
        }
        PotionId::WeakenPotion
        | PotionId::BloodPotion
        | PotionId::LiquidBronze
        | PotionId::Elixir => {
            return super::mechanics::reviewed_potion_use_v1(potion, potency)
                .expect("reviewed potion use rule")
                .combat_actions(state, target);
        }
        PotionId::FearPotion => {
            // Apply 3 Vulnerable to target enemy (thrown)
            bottom(
                &mut actions,
                Action::ApplyPower {
                    source: PLAYER,
                    target,
                    power_id: PowerId::Vulnerable,
                    amount: potency,
                },
            );
        }
        PotionId::BlockPotion => {
            // Gain 12 Block
            bottom(
                &mut actions,
                Action::GainBlock {
                    target: PLAYER,
                    amount: potency,
                },
            );
        }
        PotionId::EnergyPotion => {
            // Gain 2 Energy
            bottom(&mut actions, Action::GainEnergy { amount: potency });
        }
        PotionId::StrengthPotion => {
            // Gain 2 Strength (permanent, unlike Java's Flex which wears off)
            bottom(
                &mut actions,
                Action::ApplyPower {
                    source: PLAYER,
                    target: PLAYER,
                    power_id: PowerId::Strength,
                    amount: potency,
                },
            );
        }
        PotionId::DexterityPotion => {
            // Gain 2 Dexterity (permanent)
            bottom(
                &mut actions,
                Action::ApplyPower {
                    source: PLAYER,
                    target: PLAYER,
                    power_id: PowerId::Dexterity,
                    amount: potency,
                },
            );
        }
        PotionId::SpeedPotion => {
            // Gain 5 temporary Dexterity (lost at end of turn via DexterityDown)
            bottom(
                &mut actions,
                Action::ApplyPower {
                    source: PLAYER,
                    target: PLAYER,
                    power_id: PowerId::Dexterity,
                    amount: potency,
                },
            );
            bottom(
                &mut actions,
                Action::ApplyPower {
                    source: PLAYER,
                    target: PLAYER,
                    power_id: PowerId::DexterityDown,
                    amount: potency,
                },
            );
        }
        PotionId::SteroidPotion => {
            // Gain 5 temporary Strength (lost at end of turn via LoseStrength)
            bottom(
                &mut actions,
                Action::ApplyPower {
                    source: PLAYER,
                    target: PLAYER,
                    power_id: PowerId::Strength,
                    amount: potency,
                },
            );
            bottom(
                &mut actions,
                Action::ApplyPower {
                    source: PLAYER,
                    target: PLAYER,
                    power_id: PowerId::LoseStrength,
                    amount: potency,
                },
            );
        }
        PotionId::SwiftPotion => {
            // Draw 3 cards
            bottom(&mut actions, Action::DrawCards(potency as u32));
        }
        PotionId::FocusPotion => {
            // Gain 2 Focus (Defect)
            bottom(
                &mut actions,
                Action::ApplyPower {
                    source: PLAYER,
                    target: PLAYER,
                    power_id: PowerId::Focus,
                    amount: potency,
                },
            );
        }
        PotionId::AttackPotion
        | PotionId::SkillPotion
        | PotionId::PowerPotion
        | PotionId::ColorlessPotion => {
            let spec = card_discovery_spec(potion)
                .expect("discovery potion match arm must have a discovery spec");
            bottom(
                &mut actions,
                Action::SuspendForDiscovery {
                    colorless: spec.colorless,
                    card_type: spec.card_type,
                    amount: potency.max(0) as u8,
                    cost_for_turn: spec.cost_for_turn,
                    can_skip: spec.can_skip,
                },
            );
        }
        PotionId::BottledMiracle => {
            // Add 2 Miracles to hand (Watcher)
            bottom(
                &mut actions,
                make_constructed_temp_card_in_hand_action(
                    crate::content::cards::CardId::Miracle,
                    potency as u8,
                    false,
                    state,
                ),
            );
        }
        PotionId::BlessingOfTheForge => {
            // Upgrade ALL cards in hand
            bottom(&mut actions, Action::UpgradeAllInHand);
        }

        // ────────────── Uncommon (12) ──────────────
        PotionId::AncientPotion => {
            // Gain 1 Artifact
            bottom(
                &mut actions,
                Action::ApplyPower {
                    source: PLAYER,
                    target: PLAYER,
                    power_id: PowerId::Artifact,
                    amount: potency,
                },
            );
        }
        PotionId::RegenPotion => {
            // Gain 5 Regen
            bottom(
                &mut actions,
                Action::ApplyPower {
                    source: PLAYER,
                    target: PLAYER,
                    power_id: PowerId::Regeneration,
                    amount: potency,
                },
            );
        }
        PotionId::EssenceOfSteel => {
            // Gain 4 Plated Armor
            bottom(
                &mut actions,
                Action::ApplyPower {
                    source: PLAYER,
                    target: PLAYER,
                    power_id: PowerId::PlatedArmor,
                    amount: potency,
                },
            );
        }
        PotionId::DistilledChaosPotion => {
            // Action::UsePotion handles this statefully so Java's
            // getRandomMonster(cardRandomRng) target rolls happen when the
            // potion is used, before any queued PlayTopCardAction executes.
            // Java adds one PlayTopCardAction per potency. Each action takes the
            // current top card when it executes, then appends that card to the
            // normal card queue.
            for _ in 0..potency.max(0) {
                bottom(
                    &mut actions,
                    Action::PlayTopCard {
                        target: None,
                        exhaust: false,
                    },
                );
            }
        }
        PotionId::DuplicationPotion => {
            // This turn, your next card is played twice
            // Java applies DuplicationPower (any card type), NOT DoubleTap (Attack-only)
            bottom(
                &mut actions,
                Action::ApplyPower {
                    source: PLAYER,
                    target: PLAYER,
                    power_id: PowerId::DuplicationPower,
                    amount: potency,
                },
            );
        }
        PotionId::CunningPotion => {
            // Java: creates UPGRADED Shiv, then MakeTempCardInHandAction(shiv, potency)
            bottom(
                &mut actions,
                make_constructed_temp_card_in_hand_action(
                    crate::content::cards::CardId::Shiv,
                    potency as u8,
                    true,
                    state,
                ),
            );
        }
        PotionId::PotionOfCapacity => {
            // Gain 2 Orb Slots (Defect)
            bottom(&mut actions, Action::IncreaseMaxOrb(potency as u8));
        }
        PotionId::LiquidMemories => {
            // Choose potency card(s) from discard pile to return to hand (cost 0 this turn).
            // Java: BetterDiscardPileToHandAction(potency, 0)
            bottom(
                &mut actions,
                Action::SuspendForGridSelect {
                    source_pile: crate::state::PileType::Discard,
                    min: potency as u8,
                    max: potency as u8,
                    can_cancel: false,
                    filter: crate::state::GridSelectFilter::Any,
                    reason: crate::state::GridSelectReason::DiscardToHand,
                },
            );
        }
        PotionId::GamblersBrew => {
            // Discard any number of cards from hand, then draw that many.
            // Java: GamblingChipAction — same as Gambling Chip relic.
            bottom(
                &mut actions,
                Action::SuspendForHandSelect {
                    min: 0,
                    max: 99,
                    can_cancel: true,
                    filter: crate::state::HandSelectFilter::Any,
                    reason: crate::state::HandSelectReason::GamblingChip,
                },
            );
        }
        PotionId::StancePotion => {
            // Java: ChooseOneAction(ChooseWrath, ChooseCalm) — player picks Wrath or Calm.
            bottom(&mut actions, Action::SuspendForStanceChoice);
        }

        // ────────────── Rare (10) ──────────────
        PotionId::FairyPotion => {
            // Passive death-prevention: heal potency% of Max HP.
            // Negative sentinel triggers maxHP% calculation in handler.
            bottom(
                &mut actions,
                Action::Heal {
                    target: PLAYER,
                    amount: -(potency),
                },
            );
        }
        PotionId::SmokeBomb => {
            // Escape from a non-boss combat (thrown).
            bottom(&mut actions, Action::FleeCombat);
        }
        PotionId::FruitJuice => {
            // Gain 5 Max HP (non-combat potion, but can be used in combat too).
            bottom(&mut actions, Action::GainMaxHp { amount: potency });
        }
        PotionId::EntropicBrew => {
            // Handled natively in Action::UsePotion (action_handlers.rs) to maintain accurate RNG parity.
        }
        PotionId::SneckoOil => {
            // Draw 5 cards, then randomize cost of all cards in hand to 0-3.
            // Java: DrawCardAction(potency) + RandomizeHandCostAction()
            bottom(&mut actions, Action::DrawCards(potency as u32));
            bottom(&mut actions, Action::RandomizeHandCosts);
        }
        PotionId::GhostInAJar => {
            // Gain 1 Intangible
            bottom(
                &mut actions,
                Action::ApplyPower {
                    source: PLAYER,
                    target: PLAYER,
                    power_id: PowerId::IntangiblePlayer,
                    amount: potency,
                },
            );
        }
        PotionId::HeartOfIron => {
            // Gain 6 Metallicize
            bottom(
                &mut actions,
                Action::ApplyPower {
                    source: PLAYER,
                    target: PLAYER,
                    power_id: PowerId::Metallicize,
                    amount: potency,
                },
            );
        }
        PotionId::CultistPotion => {
            // Gain 1 Ritual
            bottom(
                &mut actions,
                Action::ApplyPowerDetailed {
                    source: PLAYER,
                    target: PLAYER,
                    power_id: PowerId::Ritual,
                    amount: potency,
                    instance_id: None,
                    extra_data: Some(crate::content::powers::core::ritual::extra_data(true, true)),
                },
            );
        }
        PotionId::Ambrosia => {
            // Enter Divinity stance (Watcher).
            // Java: AbstractDungeon.actionManager.addToBottom(new ChangeStanceAction("Divinity"))
            bottom(&mut actions, Action::EnterStance("Divinity".to_string()));
        }
        PotionId::EssenceOfDarkness => {
            // Action::UsePotion handles this statefully because Java
            // EssenceOfDarknessAction channels `potency` Dark orbs once for
            // each current orb slot, not just `potency` total.
            for _ in 0..potency.max(0) {
                bottom(
                    &mut actions,
                    Action::ChannelOrb(crate::runtime::combat::OrbId::Dark),
                );
            }
        }
    }

    actions
}

pub(crate) fn elixir_has_pending_use_effect(state: &CombatState) -> bool {
    let player = &state.entities.player;
    player.current_hp < player.max_hp
        && player.has_relic(crate::content::relics::RelicId::ToyOrnithopter)
        && !player.has_relic(crate::content::relics::RelicId::MarkOfTheBloom)
}
