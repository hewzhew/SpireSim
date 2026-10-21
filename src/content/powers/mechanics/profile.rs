//! Public, identity-independent facts. These describe mechanics, not action
//! preferences. Profiles are partial: they do not replace the execution hooks,
//! card-specific multipliers, relic interactions, or trigger ordering.
use crate::content::cards::CardType;
use crate::content::powers::PowerId;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PowerTrigger {
    AttackDamage,
    CardBlock,
    PlayerCardPlayed,
    OwnerTurnEnd,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PowerEffect {
    AddDamageDealt,
    AddBlock,
    ReduceDamageDealt,
    IncreaseDamageReceived,
    ReduceBlock,
    ApplyPower,
    ThornsDamage,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PowerEffectTarget {
    Owner,
    Player,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PowerAmountKind {
    AdditiveModifier,
    PerTrigger,
    RemainingRounds,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PowerLifetime {
    UntilChanged,
    RoundDecayExceptFresh,
    EndPlayerTurn,
    OwnerTurnEnd,
}

#[derive(Clone, Copy, Debug)]
pub struct PowerMechanicProfile {
    pub trigger: PowerTrigger,
    pub card_type: Option<CardType>,
    pub effect: PowerEffect,
    pub target: PowerEffectTarget,
    pub amount_kind: PowerAmountKind,
    pub lifetime: PowerLifetime,
    pub applied_power: Option<PowerId>,
    /// The scheduled application is skipped for non-positive amounts. The
    /// lifetime still removes the source; this is not a duration counter.
    pub positive_amount_only: bool,
}

impl PowerMechanicProfile {
    /// An on-play relationship, not a prediction that a card will resolve or
    /// survive other hooks. It applies only while the source power is present.
    pub fn triggered_by_card_type(self, card_type: CardType) -> bool {
        self.trigger == PowerTrigger::PlayerCardPlayed && self.card_type == Some(card_type)
    }
}

pub fn typed_profile(power: PowerId) -> Option<PowerMechanicProfile> {
    use PowerAmountKind::*;
    use PowerEffect::*;
    use PowerLifetime::*;
    use PowerTrigger::*;
    let (trigger, card_type, effect, target, amount_kind, lifetime, applied_power) = match power {
        PowerId::Strength => (
            AttackDamage,
            None,
            AddDamageDealt,
            PowerEffectTarget::Owner,
            AdditiveModifier,
            UntilChanged,
            None,
        ),
        PowerId::Dexterity => (
            CardBlock,
            None,
            AddBlock,
            PowerEffectTarget::Owner,
            AdditiveModifier,
            UntilChanged,
            None,
        ),
        PowerId::Weak => (
            AttackDamage,
            None,
            ReduceDamageDealt,
            PowerEffectTarget::Owner,
            RemainingRounds,
            RoundDecayExceptFresh,
            None,
        ),
        PowerId::Vulnerable => (
            AttackDamage,
            None,
            IncreaseDamageReceived,
            PowerEffectTarget::Owner,
            RemainingRounds,
            RoundDecayExceptFresh,
            None,
        ),
        PowerId::Frail => (
            CardBlock,
            None,
            ReduceBlock,
            PowerEffectTarget::Owner,
            RemainingRounds,
            RoundDecayExceptFresh,
            None,
        ),
        PowerId::Anger => (
            PlayerCardPlayed,
            Some(CardType::Skill),
            ApplyPower,
            PowerEffectTarget::Owner,
            PerTrigger,
            UntilChanged,
            Some(PowerId::Strength),
        ),
        PowerId::Rage => (
            PlayerCardPlayed,
            Some(CardType::Attack),
            AddBlock,
            PowerEffectTarget::Owner,
            PerTrigger,
            EndPlayerTurn,
            None,
        ),
        PowerId::SharpHide => (
            PlayerCardPlayed,
            Some(CardType::Attack),
            ThornsDamage,
            PowerEffectTarget::Player,
            PerTrigger,
            UntilChanged,
            None,
        ),
        PowerId::Shackled => (
            PowerTrigger::OwnerTurnEnd,
            None,
            ApplyPower,
            PowerEffectTarget::Owner,
            PerTrigger,
            PowerLifetime::OwnerTurnEnd,
            Some(PowerId::Strength),
        ),
        _ => return None,
    };
    Some(PowerMechanicProfile {
        trigger,
        card_type,
        effect,
        target,
        amount_kind,
        lifetime,
        applied_power,
        positive_amount_only: power == PowerId::Shackled,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::powers::core;
    use crate::runtime::action::Action;

    #[test]
    fn on_play_profiles_follow_execution_hooks_without_a_monster_or_turn_gate() {
        let state = crate::test_support::blank_test_combat();
        for id in [
            crate::content::cards::CardId::Defend,
            crate::content::cards::CardId::Strike,
            crate::content::cards::CardId::Inflame,
        ] {
            let card = crate::runtime::combat::CombatCard::for_scenario(id, 0);
            let card_type = crate::content::cards::get_card_definition(id).card_type;
            for power in [PowerId::Anger, PowerId::Rage, PowerId::SharpHide] {
                let profile = typed_profile(power).unwrap();
                let actions = match power {
                    PowerId::Anger => core::anger::on_card_played(&state, 7, &card, 3),
                    PowerId::Rage => core::rage::on_card_played(7, &card, 3),
                    PowerId::SharpHide => core::sharp_hide::on_card_played(&state, 7, &card, 3),
                    _ => unreachable!(),
                };
                assert_eq!(
                    profile.triggered_by_card_type(card_type),
                    !actions.is_empty()
                );
                if power == PowerId::Anger && !actions.is_empty() {
                    assert!(matches!(
                        actions[0],
                        Action::ApplyPower {
                            target: 7,
                            power_id: PowerId::Strength,
                            amount: 3,
                            ..
                        }
                    ));
                    assert_eq!(profile.applied_power, Some(PowerId::Strength));
                    assert_eq!(
                        typed_profile(PowerId::Strength).unwrap().effect,
                        PowerEffect::AddDamageDealt
                    );
                }
            }
        }
    }

    #[test]
    fn shackled_restores_only_positive_amount_then_expires_at_owner_turn_end() {
        let p = typed_profile(PowerId::Shackled).unwrap();
        assert_eq!(p.trigger, PowerTrigger::OwnerTurnEnd);
        assert_eq!(p.lifetime, PowerLifetime::OwnerTurnEnd);
        assert_eq!(p.applied_power, Some(PowerId::Strength));
        assert!(p.positive_amount_only);
        for amount in [-3, 0, 3] {
            let actions = core::shackled::at_end_of_turn(7, amount);
            assert_eq!(actions.len(), 1 + usize::from(amount > 0));
            if amount > 0 {
                assert!(matches!(
                    actions[0],
                    Action::ApplyPower {
                        target: 7,
                        power_id: PowerId::Strength,
                        amount: 3,
                        ..
                    }
                ));
            }
            assert!(matches!(
                actions.last().unwrap(),
                Action::RemovePower {
                    target: 7,
                    power_id: PowerId::Shackled
                }
            ));
        }
    }
}
