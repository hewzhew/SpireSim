//! Shared identity-independent card, potion, power and partial relic mechanics.

use sts_oracle_eval::content::cards::{
    get_card_definition,
    mechanics::{
        card_base_damage_delivery_v1, card_mechanic_profile, card_player_strength_delivery_v1,
        combat_upgrade_scope_for_card_v1, CardAttackAmountV1, CardAttackHitsV1,
        CardAttackPlayConditionV1, CardAttackTargetsV1, CardBaseDamageDeliveryV1,
        CardEnergyDeliveryV1, CardMechanicCoverage, CardMechanicRole, CardPlayerStrengthDeliveryV1,
        CombatUpgradeScopeV1, CARD_MECHANIC_ROLE_COUNT,
    },
    CardId, CardRarity, CardTag, CardTarget, CardType,
};
use sts_oracle_eval::content::potions::{
    get_potion_definition, potion_mechanic_roles, PotionClass, PotionId, PotionMechanicRole,
    PotionRarity, POTION_MECHANIC_ROLE_COUNT,
};

use super::{
    bool_value, CategoricalField, RelationKind, ScalarField, SemanticBatchBuilder,
    SemanticEncodingError, TokenKind,
};
use sts_oracle_eval::content::powers::{
    mechanics::{
        typed_profile, PowerAmountKind, PowerEffect, PowerEffectTarget, PowerLifetime, PowerTrigger,
    },
    PowerId,
};
use sts_oracle_eval::content::relics::{mechanics as relic_mechanics, RelicId};
use sts_oracle_eval::runtime::combat::CombatCard;
use sts_oracle_learning::eval::run_control::LearningStrategicCardV2;

pub(crate) const CARD_COMBAT_UPGRADE_SCOPE_SCHEMA: &[(&str, i64)] = &[
    ("SelectedCardInHand", 0),
    ("WholeHand", 1),
    ("AllCombatZones", 2),
];

pub(crate) const CARD_ENERGY_GAIN_DELIVERY_SCHEMA: &[(&str, i64)] = &[
    ("OnPlay", 0),
    ("EachPlayerTurnStart", 1),
    ("NextPlayerEnergyRecharge", 2),
    ("OnExhaust", 3),
    ("IfTargetVulnerableAtResolution", 4),
];

pub(crate) const CARD_STATUS_CREATION_DESTINATION_SCHEMA: &[(&str, i64)] =
    &[("Hand", 0), ("DrawPileRandom", 1), ("DiscardPile", 2)];
pub(crate) const CARD_STATUS_CREATION_TIMING_SCHEMA: &[(&str, i64)] =
    &[("AfterAttack", 0), ("BeforeBlock", 1)];

pub(crate) const CARD_STRENGTH_GAIN_DELIVERY_SCHEMA: &[(&str, i64)] = &[
    ("OnPlay", 0),
    ("IfTargetIntentBaseDamageNonnegativeAtResolution", 1),
    ("OnPlayWithSeparateLoseStrength", 2),
    ("EachPlayerTurnStartPostDraw", 3),
];
pub(crate) const CARD_STRENGTH_CONVERSION_SCHEMA: &[(&str, i64)] = &[
    ("AmplifyCurrentStrength", 0),
    ("PreventStrengthDownDebuff", 1),
    ("ClearStrengthDownDebuff", 2),
];
pub(crate) const CARD_DAMAGE_DELIVERY_SCHEMA: &[(&str, i64)] =
    &[("Attack", 0), ("StrengthIfChosen", 1)];
pub(crate) const CARD_ATTACK_AMOUNT_SCHEMA: &[(&str, i64)] = &[
    ("Base", 0),
    ("BasePlusStrikeCards", 1),
    ("BasePlusMantraGainedThisCombat", 2),
    ("PlayerBlock", 3),
    ("DrawPileSize", 4),
    ("FrostChanneledThisCombat", 5),
];
pub(crate) const CARD_ATTACK_HITS_SCHEMA: &[(&str, i64)] = &[
    ("Fixed", 0),
    ("OncePlusIfTargetPoisonedAtResolution", 1),
    ("PerEffectiveX", 2),
    ("PerHandCardAtResolutionBeforeExhaust", 3),
    ("PerSkillInHandAtResolution", 4),
    ("PerFilledOrbAtResolution", 5),
    ("PerPriorAttackThisTurnAtResolution", 6),
    ("PerLightningChanneledThisCombatAtUse", 7),
    ("PerLivingEnemyAtUse", 8),
    ("FromInstanceMisc", 9),
];
pub(crate) const CARD_ATTACK_TARGETS_SCHEMA: &[(&str, i64)] = &[
    ("SelectedEnemy", 0),
    ("EachEnemy", 1),
    ("RandomEnemyPerHit", 2),
];
pub(crate) const CARD_ATTACK_PLAY_CONDITION_SCHEMA: &[(&str, i64)] = &[
    ("None", 0),
    ("HandContainsOnlyAttacks", 1),
    ("DrawPileEmpty", 2),
    ("NoOtherAttackInHand", 3),
];

/// Reconstruct only public persistent numbers for a private mechanics preview.
/// The temporary scenario identity is never emitted or bound to a run card.
pub(super) fn observed_owned_card(card: LearningStrategicCardV2<'_>) -> CombatCard {
    let mut owned = CombatCard::for_scenario(card.card(), 0);
    owned.upgrades = card.upgrades();
    owned.misc_value = card.misc_value();
    owned.base_damage_override = card.base_damage_override();
    owned.base_block_override = card.base_block_override();
    owned.cost_modifier = card.cost_modifier();
    owned
}

/// Inventory of reviewed deployment rules, not active combat resources. Missing
/// core profiles do not contribute; zero is a count of these reviewed cards.
#[derive(Default)]
pub(super) struct OwnedDeckMechanicCounts {
    strength: [usize; 4],
    exhaust_draw: usize,
    exhaust_block: usize,
}

impl OwnedDeckMechanicCounts {
    pub(super) fn include(&mut self, card: CardId) {
        use sts_oracle_eval::content::cards::mechanics::{
            card_exhaust_block_power_v1, card_exhaust_draw_power_v1,
        };
        if let Some(delivery) = card_player_strength_delivery_v1(card) {
            let index = match delivery {
                CardPlayerStrengthDeliveryV1::OnPlay => 0,
                CardPlayerStrengthDeliveryV1::IfTargetIntentBaseDamageNonnegativeAtResolution => 1,
                CardPlayerStrengthDeliveryV1::OnPlayWithSeparateLoseStrength => 2,
                CardPlayerStrengthDeliveryV1::EachPlayerTurnStartPostDraw => 3,
            };
            self.strength[index] += 1;
        }
        let magic = get_card_definition(card).base_magic;
        self.exhaust_draw += usize::from(card_exhaust_draw_power_v1(card, magic).is_some());
        self.exhaust_block += usize::from(card_exhaust_block_power_v1(card, magic).is_some());
    }

    pub(super) fn encode(self, builder: &mut SemanticBatchBuilder, run: u64) {
        for (field, count) in [
            ScalarField::DeckStrengthOnPlayCardCount,
            ScalarField::DeckStrengthConditionalCardCount,
            ScalarField::DeckTemporaryStrengthCardCount,
            ScalarField::DeckStrengthPerTurnCardCount,
            ScalarField::DeckExhaustDrawPowerCardCount,
            ScalarField::DeckExhaustBlockPowerCardCount,
        ]
        .into_iter()
        .zip(
            self.strength
                .into_iter()
                .chain([self.exhaust_draw, self.exhaust_block]),
        ) {
            builder.scalar(run, field, count);
        }
    }
}

/// Identity embeddings in these fields are residuals over shared mechanics.
/// The model initializes their full vocabulary slices to zero, so an identity
/// absent from training falls back to typed mechanical attributes instead of
/// contributing a random vector.
pub(crate) const IDENTITY_RESIDUAL_CATEGORICAL_FIELDS: &[u16] = &[
    CategoricalField::CardId as u16,
    CategoricalField::PotionId as u16,
    CategoricalField::ActionCardId as u16,
    CategoricalField::CardStatusCreationCardId as u16,
    CategoricalField::ActionPotionId as u16,
    CategoricalField::PowerId as u16,
];

pub(crate) const POWER_TRIGGER_SCHEMA: &[(&str, i64)] = &[
    ("AttackDamage", PowerTrigger::AttackDamage as i64),
    ("CardBlock", PowerTrigger::CardBlock as i64),
    ("PlayerCardPlayed", PowerTrigger::PlayerCardPlayed as i64),
];
pub(crate) const POWER_EFFECT_SCHEMA: &[(&str, i64)] = &[
    ("AddDamageDealt", PowerEffect::AddDamageDealt as i64),
    ("AddBlock", PowerEffect::AddBlock as i64),
    ("ReduceDamageDealt", PowerEffect::ReduceDamageDealt as i64),
    (
        "IncreaseDamageReceived",
        PowerEffect::IncreaseDamageReceived as i64,
    ),
    ("ReduceBlock", PowerEffect::ReduceBlock as i64),
    ("ApplyPower", PowerEffect::ApplyPower as i64),
    ("ThornsDamage", PowerEffect::ThornsDamage as i64),
];
pub(crate) const POWER_EFFECT_TARGET_SCHEMA: &[(&str, i64)] = &[
    ("Owner", PowerEffectTarget::Owner as i64),
    ("Player", PowerEffectTarget::Player as i64),
];
pub(crate) const POWER_AMOUNT_KIND_SCHEMA: &[(&str, i64)] = &[
    ("AdditiveModifier", PowerAmountKind::AdditiveModifier as i64),
    ("PerTrigger", PowerAmountKind::PerTrigger as i64),
    ("RemainingRounds", PowerAmountKind::RemainingRounds as i64),
];
pub(crate) const POWER_LIFETIME_SCHEMA: &[(&str, i64)] = &[
    ("UntilChanged", PowerLifetime::UntilChanged as i64),
    (
        "RoundDecayExceptFresh",
        PowerLifetime::RoundDecayExceptFresh as i64,
    ),
    ("EndPlayerTurn", PowerLifetime::EndPlayerTurn as i64),
];

impl SemanticBatchBuilder {
    /// Rule requests on the same owner/candidate token. Absence is unreviewed
    /// coverage, not a claim that this relic has no effects.
    pub(super) fn relic_identity_with_mechanics(
        &mut self,
        token: u64,
        identity_field: CategoricalField,
        relic: RelicId,
    ) {
        self.category(token, identity_field, relic.stable_ordinal() as i64);
        if let Some(power) = relic_mechanics::relic_prevent_player_debuff_before_artifact_v1(relic)
        {
            self.category(
                token,
                CategoricalField::RelicPreventPlayerDebuffBeforeArtifactPowerId,
                power.stable_ordinal() as i64,
            );
        }
        if let Some(rule) = relic_mechanics::relic_combat_potion_drop_override_v1(relic) {
            self.scalar(
                token,
                ScalarField::RelicCombatPotionDropChanceOverridePercent,
                rule.chance_percent,
            );
            self.scalar(
                token,
                ScalarField::RelicCombatPotionDropBlockedAtRewardEntryCount,
                rule.blocked_at_reward_entry_count,
            );
        }
        if let Some(amount) = relic_mechanics::relic_victory_healing_request_v1(relic) {
            self.scalar(token, ScalarField::RelicVictoryHealingRequest, amount);
        }
    }

    pub(super) fn owned_card_output(
        &mut self,
        token: u64,
        card: &CombatCard,
    ) -> sts_oracle_eval::ai::card_semantics_v1::CardOwnedBaseOutputV1 {
        let output = sts_oracle_eval::ai::card_semantics_v1::card_owned_base_output_v1(card);
        for (field, value) in [
            (ScalarField::CardPersistentEnergyCost, output.energy_cost),
            (
                ScalarField::CardPersistentDamageParameter,
                output.damage.damage_per_hit,
            ),
            (ScalarField::CardPersistentBlockParameter, output.base_block),
            (ScalarField::CardPersistentMagicParameter, output.base_magic),
        ] {
            self.scalar(token, field, value);
        }
        if let Some(value) = output.damage.unconditional_base_total() {
            self.scalar(token, ScalarField::CardUnconditionalBaseDamage, value);
        }
        if let Some(value) = output.unconditional_block {
            self.scalar(token, ScalarField::CardUnconditionalBaseBlock, value);
        }
        output
    }

    pub(super) fn power_identity_with_mechanics(
        &mut self,
        token: u64,
        power: PowerId,
    ) -> Result<(), SemanticEncodingError> {
        self.category(
            token,
            CategoricalField::PowerId,
            power.stable_ordinal() as i64,
        );
        let profile = typed_profile(power);
        self.category(
            token,
            CategoricalField::PowerMechanicKnown,
            bool_value(profile.is_some()),
        );
        if let Some(profile) = profile {
            self.category(
                token,
                CategoricalField::PowerTrigger,
                profile.trigger as i64,
            );
            self.category(token, CategoricalField::PowerEffect, profile.effect as i64);
            self.category(
                token,
                CategoricalField::PowerEffectTarget,
                profile.target as i64,
            );
            self.category(
                token,
                CategoricalField::PowerAmountKind,
                profile.amount_kind as i64,
            );
            self.category(
                token,
                CategoricalField::PowerLifetime,
                profile.lifetime as i64,
            );
            if let Some(card_type) = profile.card_type {
                self.category(
                    token,
                    CategoricalField::PowerTriggerCardType,
                    card_type as i64,
                );
            }
            if let Some(applied) = profile.applied_power {
                // A description of the resulting power, not a currently active
                // power instance. No EntityHasPower edge or active amount.
                let effect = self.add_token(TokenKind::PowerEffect)?;
                self.edge(token, RelationKind::PowerAppliesPower, effect);
                self.power_identity_with_mechanics(effect, applied)?;
            }
        }
        Ok(())
    }
}

pub(crate) const CARD_TYPE_SCHEMA: &[(&str, i64)] = &[
    ("Attack", CardType::Attack as i64),
    ("Skill", CardType::Skill as i64),
    ("Power", CardType::Power as i64),
    ("Status", CardType::Status as i64),
    ("Curse", CardType::Curse as i64),
];
pub(crate) const CARD_RARITY_SCHEMA: &[(&str, i64)] = &[
    ("Basic", CardRarity::Basic as i64),
    ("Common", CardRarity::Common as i64),
    ("Uncommon", CardRarity::Uncommon as i64),
    ("Rare", CardRarity::Rare as i64),
    ("Special", CardRarity::Special as i64),
    ("Curse", CardRarity::Curse as i64),
];
pub(crate) const CARD_TARGET_SCHEMA: &[(&str, i64)] = &[
    ("Enemy", CardTarget::Enemy as i64),
    ("AllEnemy", CardTarget::AllEnemy as i64),
    ("All", CardTarget::All as i64),
    ("SelfAndEnemy", CardTarget::SelfAndEnemy as i64),
    ("SelfTarget", CardTarget::SelfTarget as i64),
    ("None", CardTarget::None as i64),
];
pub(crate) const CARD_TAG_SCHEMA: &[(&str, i64)] = &[
    ("Strike", CardTag::Strike as i64),
    ("StarterStrike", CardTag::StarterStrike as i64),
    ("StarterDefend", CardTag::StarterDefend as i64),
    ("Healing", CardTag::Healing as i64),
    ("Empty", CardTag::Empty as i64),
];
pub(crate) const CARD_MECHANIC_COVERAGE_SCHEMA: &[(&str, i64)] = &[
    (
        "DefinitionOnly",
        CardMechanicCoverage::DefinitionOnly as i64,
    ),
    ("Partial", CardMechanicCoverage::Partial as i64),
    ("Complete", CardMechanicCoverage::Complete as i64),
];
pub(crate) const CARD_MECHANIC_ROLE_SCHEMA: &[(&str, i64)] = &[
    ("DrawCards", CardMechanicRole::DrawCards as i64),
    ("GainEnergy", CardMechanicRole::GainEnergy as i64),
    ("ApplyWeak", CardMechanicRole::ApplyWeak as i64),
    ("ApplyVulnerable", CardMechanicRole::ApplyVulnerable as i64),
    (
        "ExhaustOtherCards",
        CardMechanicRole::ExhaustOtherCards as i64,
    ),
    ("SelectFromHand", CardMechanicRole::SelectFromHand as i64),
    ("DiscardFromHand", CardMechanicRole::DiscardFromHand as i64),
    ("DrawPileControl", CardMechanicRole::DrawPileControl as i64),
    ("MultiHit", CardMechanicRole::MultiHit as i64),
    ("RandomTarget", CardMechanicRole::RandomTarget as i64),
    ("GenerateCard", CardMechanicRole::GenerateCard as i64),
    ("SelfHpLoss", CardMechanicRole::SelfHpLoss as i64),
    (
        "EndTurnHandTrigger",
        CardMechanicRole::EndTurnHandTrigger as i64,
    ),
    ("DrawOnStatus", CardMechanicRole::DrawOnStatus as i64),
    ("HandSizeScaling", CardMechanicRole::HandSizeScaling as i64),
    ("RandomOutcome", CardMechanicRole::RandomOutcome as i64),
    ("ExhaustPayoff", CardMechanicRole::ExhaustPayoff as i64),
    (
        "PreventFurtherDrawThisTurn",
        CardMechanicRole::PreventFurtherDrawThisTurn as i64,
    ),
    (
        "PersistentEnemyAttackReduction",
        CardMechanicRole::PersistentEnemyAttackReduction as i64,
    ),
    (
        "GenerateStatusCard",
        CardMechanicRole::GenerateStatusCard as i64,
    ),
    (
        "StatusDrawPayoff",
        CardMechanicRole::StatusDrawPayoff as i64,
    ),
];
pub(crate) const POTION_RARITY_SCHEMA: &[(&str, i64)] = &[
    ("Common", PotionRarity::Common as i64),
    ("Uncommon", PotionRarity::Uncommon as i64),
    ("Rare", PotionRarity::Rare as i64),
];
pub(crate) const POTION_CLASS_SCHEMA: &[(&str, i64)] = &[
    ("Any", PotionClass::Any as i64),
    ("Ironclad", PotionClass::Ironclad as i64),
    ("Silent", PotionClass::Silent as i64),
    ("Defect", PotionClass::Defect as i64),
    ("Watcher", PotionClass::Watcher as i64),
];
pub(crate) const POTION_MECHANIC_ROLE_SCHEMA: &[(&str, i64)] = &[
    ("DirectDamage", PotionMechanicRole::DirectDamage as i64),
    ("MultiTarget", PotionMechanicRole::MultiTarget as i64),
    ("ApplyPoison", PotionMechanicRole::ApplyPoison as i64),
    ("ApplyWeak", PotionMechanicRole::ApplyWeak as i64),
    (
        "ApplyVulnerable",
        PotionMechanicRole::ApplyVulnerable as i64,
    ),
    ("GainBlock", PotionMechanicRole::GainBlock as i64),
    ("Heal", PotionMechanicRole::Heal as i64),
    ("GainEnergy", PotionMechanicRole::GainEnergy as i64),
    ("GainStrength", PotionMechanicRole::GainStrength as i64),
    ("GainDexterity", PotionMechanicRole::GainDexterity as i64),
    ("Temporary", PotionMechanicRole::Temporary as i64),
    ("DrawCards", PotionMechanicRole::DrawCards as i64),
    ("GainFocus", PotionMechanicRole::GainFocus as i64),
    ("DiscoverAttack", PotionMechanicRole::DiscoverAttack as i64),
    ("DiscoverSkill", PotionMechanicRole::DiscoverSkill as i64),
    ("DiscoverPower", PotionMechanicRole::DiscoverPower as i64),
    (
        "DiscoverColorless",
        PotionMechanicRole::DiscoverColorless as i64,
    ),
    ("AddCardToHand", PotionMechanicRole::AddCardToHand as i64),
    ("UpgradeHand", PotionMechanicRole::UpgradeHand as i64),
    ("GainArtifact", PotionMechanicRole::GainArtifact as i64),
    (
        "GainRegeneration",
        PotionMechanicRole::GainRegeneration as i64,
    ),
    (
        "GainPlatedArmor",
        PotionMechanicRole::GainPlatedArmor as i64,
    ),
    ("GainThorns", PotionMechanicRole::GainThorns as i64),
    ("PlayTopCards", PotionMechanicRole::PlayTopCards as i64),
    ("DuplicateCards", PotionMechanicRole::DuplicateCards as i64),
    ("GainOrbSlots", PotionMechanicRole::GainOrbSlots as i64),
    (
        "RetrieveFromDiscard",
        PotionMechanicRole::RetrieveFromDiscard as i64,
    ),
    ("DiscardAndDraw", PotionMechanicRole::DiscardAndDraw as i64),
    ("ExhaustCards", PotionMechanicRole::ExhaustCards as i64),
    ("ChooseStance", PotionMechanicRole::ChooseStance as i64),
    ("PreventDeath", PotionMechanicRole::PreventDeath as i64),
    ("EscapeCombat", PotionMechanicRole::EscapeCombat as i64),
    ("GainMaxHp", PotionMechanicRole::GainMaxHp as i64),
    (
        "GeneratePotions",
        PotionMechanicRole::GeneratePotions as i64,
    ),
    (
        "RandomizeHandCosts",
        PotionMechanicRole::RandomizeHandCosts as i64,
    ),
    ("GainIntangible", PotionMechanicRole::GainIntangible as i64),
    (
        "GainMetallicize",
        PotionMechanicRole::GainMetallicize as i64,
    ),
    ("GainRitual", PotionMechanicRole::GainRitual as i64),
    ("EnterDivinity", PotionMechanicRole::EnterDivinity as i64),
    ("ChannelDark", PotionMechanicRole::ChannelDark as i64),
];

const _: () = {
    assert!(CardType::Attack as i64 == 0);
    assert!(CardType::Curse as usize + 1 == CARD_TYPE_SCHEMA.len());
    assert!(CardRarity::Basic as i64 == 0);
    assert!(CardRarity::Curse as usize + 1 == CARD_RARITY_SCHEMA.len());
    assert!(CardTarget::Enemy as i64 == 0);
    assert!(CardTarget::None as usize + 1 == CARD_TARGET_SCHEMA.len());
    assert!(CardTag::Strike as i64 == 0);
    assert!(CardTag::Empty as usize + 1 == CARD_TAG_SCHEMA.len());
    assert!(CardMechanicCoverage::DefinitionOnly as i64 == 0);
    assert!(CardMechanicCoverage::Complete as usize + 1 == CARD_MECHANIC_COVERAGE_SCHEMA.len());
    assert!(CardMechanicRole::DrawCards as i64 == 0);
    assert!(CardMechanicRole::StatusDrawPayoff as u64 + 1 == CARD_MECHANIC_ROLE_COUNT);
    assert!(CARD_MECHANIC_ROLE_SCHEMA.len() as u64 == CARD_MECHANIC_ROLE_COUNT);
    assert!(PotionRarity::Common as i64 == 0);
    assert!(PotionRarity::Rare as usize + 1 == POTION_RARITY_SCHEMA.len());
    assert!(PotionClass::Any as i64 == 0);
    assert!(PotionClass::Watcher as usize + 1 == POTION_CLASS_SCHEMA.len());
    assert!(PotionMechanicRole::DirectDamage as i64 == 0);
    assert!(PotionMechanicRole::ChannelDark as u64 + 1 == POTION_MECHANIC_ROLE_COUNT);
    assert!(POTION_MECHANIC_ROLE_SCHEMA.len() as u64 == POTION_MECHANIC_ROLE_COUNT);
};

impl SemanticBatchBuilder {
    pub(super) fn card_identity_with_mechanics(
        &mut self,
        token: u64,
        identity_field: CategoricalField,
        card: CardId,
        upgrades: Option<u8>,
    ) {
        self.category(token, identity_field, card.stable_ordinal() as i64);
        let definition = get_card_definition(card);
        let profile = upgrades.map(|upgrades| card_mechanic_profile(card, upgrades));
        self.category(
            token,
            CategoricalField::CardType,
            definition.card_type as i64,
        );
        self.category(
            token,
            CategoricalField::CardRarity,
            definition.rarity as i64,
        );
        self.category(
            token,
            CategoricalField::CardTarget,
            profile
                .map(|profile| profile.effective_target)
                .unwrap_or(definition.target) as i64,
        );
        self.category(
            token,
            CategoricalField::CardIsMultiDamage,
            bool_value(definition.is_multi_damage),
        );
        self.category(
            token,
            CategoricalField::CardExhaust,
            bool_value(
                profile
                    .map(|profile| profile.exhausts_when_played)
                    .unwrap_or(definition.exhaust),
            ),
        );
        self.category(
            token,
            CategoricalField::CardEthereal,
            bool_value(
                profile
                    .map(|profile| profile.ethereal)
                    .unwrap_or(definition.ethereal),
            ),
        );
        self.category(
            token,
            CategoricalField::CardInnate,
            bool_value(sts_oracle_eval::content::cards::is_innate_card_for(
                card,
                upgrades.unwrap_or(0),
            )),
        );
        if let Some(upgrades) = upgrades {
            self.category(
                token,
                CategoricalField::CardCanUpgrade,
                bool_value(sts_oracle_eval::content::cards::can_upgrade_card_once_for(
                    card, upgrades,
                )),
            );
            self.card_resource_requests(token, card, upgrades);
        }
        if let Some(scope) =
            upgrades.and_then(|upgrades| combat_upgrade_scope_for_card_v1(card, upgrades))
        {
            self.category(
                token,
                CategoricalField::CardCombatUpgradeScope,
                match scope {
                    CombatUpgradeScopeV1::SelectedCardInHand => 0,
                    CombatUpgradeScopeV1::WholeHand => 1,
                    CombatUpgradeScopeV1::AllCombatZones => 2,
                },
            );
        }
        for tag in definition.tags {
            self.category(token, CategoricalField::CardTag, *tag as i64);
        }
        self.category(
            token,
            CategoricalField::CardMechanicCoverage,
            profile
                .map(|profile| profile.coverage)
                .unwrap_or(CardMechanicCoverage::DefinitionOnly) as i64,
        );
        if let Some(profile) = profile {
            for role in profile.roles.iter() {
                self.category(token, CategoricalField::CardMechanicRole, role as i64);
            }
        }
        self.card_strength_and_attack_mechanics(token, card, upgrades);
        self.scalar(token, ScalarField::CardBaseCost, definition.cost);
        self.scalar(
            token,
            ScalarField::CardDefinitionDamage,
            definition.base_damage,
        );
        self.scalar(
            token,
            ScalarField::CardDefinitionBlock,
            definition.base_block,
        );
        self.scalar(
            token,
            ScalarField::CardDefinitionMagic,
            definition.base_magic,
        );
        self.scalar(
            token,
            ScalarField::CardUpgradeDamage,
            definition.upgrade_damage,
        );
        self.scalar(
            token,
            ScalarField::CardUpgradeBlock,
            definition.upgrade_block,
        );
        self.scalar(
            token,
            ScalarField::CardUpgradeMagic,
            definition.upgrade_magic,
        );
    }

    /// Reviewed card-owned requests, before prevention, death, capacity and
    /// conditional resolution. Missing entries stay absent, never zero-filled.
    fn card_resource_requests(&mut self, token: u64, card: CardId, upgrades: u8) {
        use sts_oracle_eval::content::cards::mechanics::{
            card_energy_gain_v1, card_exhaust_block_power_v1, card_exhaust_draw_power_v1,
            card_hand_topdeck_after_draw_v1, card_is_ethereal, card_self_hp_loss_on_play_v1,
            card_status_creation_on_play_v1, unconditional_card_draw_request_v1,
        };
        let definition = get_card_definition(card);
        let magic = definition.base_magic + i32::from(upgrades > 0) * definition.upgrade_magic;
        // These requests belong to the power installed by a deployment. They
        // are per later Exhaust event, separate from immediate on-play draw.
        if let Some(amount) = card_exhaust_draw_power_v1(card, magic) {
            self.scalar(
                token,
                ScalarField::CardDeployedExhaustDrawPerEventRequest,
                amount,
            );
        }
        if let Some(amount) = card_exhaust_block_power_v1(card, magic) {
            self.scalar(
                token,
                ScalarField::CardDeployedExhaustBlockPerEventRequest,
                amount,
            );
        }
        if let Some(loss) = card_self_hp_loss_on_play_v1(card, upgrades) {
            self.scalar(token, ScalarField::CardSelfHpLossOnPlay, loss.base_hp_loss);
            for (field, value) in [
                (
                    CategoricalField::CardSelfHpLossBypassesBlock,
                    loss.bypasses_block,
                ),
                (
                    CategoricalField::CardSelfHpLossBypassesIntangible,
                    loss.bypasses_intangible,
                ),
                (
                    CategoricalField::CardSelfHpLossTriggersRupture,
                    loss.triggers_rupture,
                ),
            ] {
                self.category(token, field, bool_value(value));
            }
        }
        if let Some(gain) = card_energy_gain_v1(card, upgrades, magic) {
            self.scalar(token, ScalarField::CardEnergyGainRequest, gain.amount);
            self.category(
                token,
                CategoricalField::CardEnergyGainDelivery,
                match gain.delivery {
                    CardEnergyDeliveryV1::OnPlay => 0,
                    CardEnergyDeliveryV1::EachPlayerTurnStart => 1,
                    CardEnergyDeliveryV1::NextPlayerEnergyRecharge => 2,
                    CardEnergyDeliveryV1::OnExhaust => 3,
                    CardEnergyDeliveryV1::IfTargetVulnerableAtResolution => 4,
                },
            );
        }
        if let Some(amount) = unconditional_card_draw_request_v1(card, magic) {
            self.scalar(token, ScalarField::CardDrawOnPlayRequest, amount);
        }
        if let Some(amount) = card_hand_topdeck_after_draw_v1(card) {
            self.scalar(token, ScalarField::CardHandTopdeckAfterDrawRequest, amount);
        }
        if let Some(request) = card_status_creation_on_play_v1(card) {
            self.scalar(
                token,
                ScalarField::CardStatusCreationRequest,
                request.amount,
            );
            self.category(
                token,
                CategoricalField::CardStatusCreationCardId,
                request.card.stable_ordinal() as i64,
            );
            self.category(
                token,
                CategoricalField::CardStatusCreationDestination,
                request.destination as i64,
            );
            self.category(
                token,
                CategoricalField::CardStatusCreationTiming,
                request.timing as i64,
            );
            self.category(
                token,
                CategoricalField::CardStatusCreationUpgraded,
                bool_value(request.upgraded),
            );
            self.category(
                token,
                CategoricalField::CardStatusCreationEthereal,
                bool_value(card_is_ethereal(request.card, u8::from(request.upgraded))),
            );
        }
    }

    fn card_strength_and_attack_mechanics(
        &mut self,
        token: u64,
        card: CardId,
        upgrades: Option<u8>,
    ) {
        use sts_oracle_eval::ai::card_semantics_v1::{
            card_mechanics_profile_v1, StrengthConversionMechanicV1,
        };
        if let Some(delivery) = card_player_strength_delivery_v1(card) {
            self.category(
                token,
                CategoricalField::CardStrengthGainDelivery,
                match delivery {
                    CardPlayerStrengthDeliveryV1::OnPlay => 0,
                    CardPlayerStrengthDeliveryV1::IfTargetIntentBaseDamageNonnegativeAtResolution => 1,
                    CardPlayerStrengthDeliveryV1::OnPlayWithSeparateLoseStrength => 2,
                    CardPlayerStrengthDeliveryV1::EachPlayerTurnStartPostDraw => 3,
                },
            );
            if let Some(upgrades) = upgrades {
                let definition = get_card_definition(card);
                self.scalar(
                    token,
                    ScalarField::CardStrengthGainParameter,
                    definition.base_magic + i32::from(upgrades > 0) * definition.upgrade_magic,
                );
            }
        }
        if let Some(conversion) = card_mechanics_profile_v1(card).strength_converter {
            self.category(
                token,
                CategoricalField::CardStrengthConversion,
                match conversion {
                    StrengthConversionMechanicV1::AmplifyCurrentStrength => 0,
                    StrengthConversionMechanicV1::PreventStrengthDownDebuff => 1,
                    StrengthConversionMechanicV1::ClearStrengthDownDebuff => 2,
                },
            );
        }
        let Some(upgrades) = upgrades else { return };
        let Some(delivery) = card_base_damage_delivery_v1(card, upgrades) else {
            return;
        };
        let CardBaseDamageDeliveryV1::Attack {
            amount,
            hits,
            targets,
            play_condition,
        } = delivery
        else {
            self.category(token, CategoricalField::CardDamageDeliveryKind, 1);
            return;
        };
        self.category(token, CategoricalField::CardDamageDeliveryKind, 0);
        let amount_kind = match amount {
            CardAttackAmountV1::Base => 0,
            CardAttackAmountV1::BasePlusStrikeCards { per_card } => {
                self.scalar(token, ScalarField::CardAttackAmountPerUnit, per_card);
                1
            }
            CardAttackAmountV1::BasePlusMantraGainedThisCombat => 2,
            CardAttackAmountV1::PlayerBlock => 3,
            CardAttackAmountV1::DrawPileSize => 4,
            CardAttackAmountV1::FrostChanneledThisCombat { per_orb } => {
                self.scalar(token, ScalarField::CardAttackAmountPerUnit, per_orb);
                5
            }
        };
        self.category(token, CategoricalField::CardAttackAmountKind, amount_kind);
        let hits_kind = match hits {
            CardAttackHitsV1::Fixed { count } => {
                self.scalar(token, ScalarField::CardAttackFixedHits, count);
                0
            }
            CardAttackHitsV1::OncePlusIfTargetPoisonedAtResolution => 1,
            CardAttackHitsV1::PerEffectiveX => 2,
            CardAttackHitsV1::PerHandCardAtResolutionBeforeExhaust => 3,
            CardAttackHitsV1::PerSkillInHandAtResolution => 4,
            CardAttackHitsV1::PerFilledOrbAtResolution => 5,
            CardAttackHitsV1::PerPriorAttackThisTurnAtResolution => 6,
            CardAttackHitsV1::PerLightningChanneledThisCombatAtUse => 7,
            CardAttackHitsV1::PerLivingEnemyAtUse => 8,
            CardAttackHitsV1::FromInstanceMisc => 9,
        };
        self.category(token, CategoricalField::CardAttackHitsKind, hits_kind);
        self.category(
            token,
            CategoricalField::CardAttackTargetsKind,
            match targets {
                CardAttackTargetsV1::SelectedEnemy => 0,
                CardAttackTargetsV1::EachEnemy => 1,
                CardAttackTargetsV1::RandomEnemyPerHit => 2,
            },
        );
        self.category(
            token,
            CategoricalField::CardAttackPlayCondition,
            match play_condition {
                None => 0,
                Some(CardAttackPlayConditionV1::HandContainsOnlyAttacks) => 1,
                Some(CardAttackPlayConditionV1::DrawPileEmpty) => 2,
                Some(CardAttackPlayConditionV1::NoOtherAttackInHand) => 3,
            },
        );
        let definition = get_card_definition(card);
        let magic = definition.base_magic + i32::from(upgrades > 0) * definition.upgrade_magic;
        self.scalar(
            token,
            ScalarField::CardAttackStrengthMultiplier,
            sts_oracle_eval::content::powers::core::strength::card_strength_multiplier_v1(
                card, magic,
            ),
        );
    }

    pub(super) fn potion_identity_with_mechanics(
        &mut self,
        token: u64,
        identity_field: CategoricalField,
        potion: PotionId,
    ) {
        self.category(token, identity_field, potion.stable_ordinal() as i64);
        let definition = get_potion_definition(potion);
        self.category(
            token,
            CategoricalField::PotionRarity,
            definition.rarity as i64,
        );
        self.category(
            token,
            CategoricalField::PotionClass,
            definition.class as i64,
        );
        self.category(
            token,
            CategoricalField::PotionIsThrown,
            bool_value(definition.is_thrown),
        );
        for role in potion_mechanic_roles(potion) {
            self.category(token, CategoricalField::PotionMechanicRole, *role as i64);
        }
        self.scalar(
            token,
            ScalarField::PotionBasePotency,
            definition.base_potency,
        );
    }

    pub(super) fn action_card(&mut self, token: u64, card: CardId, upgrades: u8) {
        self.card_identity_with_mechanics(
            token,
            CategoricalField::ActionCardId,
            card,
            Some(upgrades),
        );
        self.scalar(token, ScalarField::ActionUpgrades, upgrades);
    }

    pub(super) fn action_potion(&mut self, token: u64, potion: PotionId) {
        self.potion_identity_with_mechanics(token, CategoricalField::ActionPotionId, potion);
    }
}
