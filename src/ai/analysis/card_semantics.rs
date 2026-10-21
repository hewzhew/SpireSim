use crate::content::cards::CardId;
use serde::Serialize;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Mechanic {
    Strength,
    TemporaryStrength,
    StrengthMultiplier,
    CardDraw,
    Energy,
    Block,
    Weak,
    Vulnerable,
    EnemyStrengthDown,
    TemporaryEnemyStrengthDown,
    TopdeckControl,
}

impl Mechanic {
    /// Stable token owned by the strategic-semantics vocabulary.
    pub const fn vocabulary_token_v1(self) -> &'static str {
        match self {
            Self::Strength => "strength",
            Self::TemporaryStrength => "temporary_strength",
            Self::StrengthMultiplier => "strength_multiplier",
            Self::CardDraw => "card_draw",
            Self::Energy => "energy",
            Self::Block => "block",
            Self::Weak => "weak",
            Self::Vulnerable => "vulnerable",
            Self::EnemyStrengthDown => "enemy_strength_down",
            Self::TemporaryEnemyStrengthDown => "temporary_enemy_strength_down",
            Self::TopdeckControl => "topdeck_control",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DamageScalingAxis {
    EnergySpent,
    HandSize,
    PerHitStrength,
}

impl DamageScalingAxis {
    pub const fn vocabulary_token_v1(self) -> &'static str {
        match self {
            Self::EnergySpent => "energy_spent",
            Self::HandSize => "hand_size",
            Self::PerHitStrength => "per_hit_strength",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RunRewardKind {
    MaxHpOnFatal,
    GoldOnFatal,
}

impl RunRewardKind {
    pub const fn vocabulary_token_v1(self) -> &'static str {
        match self {
            Self::MaxHpOnFatal => "max_hp_on_fatal",
            Self::GoldOnFatal => "gold_on_fatal",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CombatEvent {
    CardExhausted,
    CardSelfDamage,
    StatusDrawn,
    TurnStart,
    TurnEnd,
}

impl CombatEvent {
    pub const fn vocabulary_token_v1(self) -> &'static str {
        match self {
            Self::CardExhausted => "card_exhausted",
            Self::CardSelfDamage => "card_self_damage",
            Self::StatusDrawn => "status_drawn",
            Self::TurnStart => "turn_start",
            Self::TurnEnd => "turn_end",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PlayEffect {
    Provide(Mechanic),
    FrontloadDamage,
    AreaDamage,
    DamageUses(Mechanic),
    DamageScalesWith(DamageScalingAxis),
    EmitEvent(CombatEvent),
    ExhaustsSelf,
    RunReward(RunRewardKind),
    RecoverCurrentHp,
    CostReducedByHpLossThisCombat,
    AddCombatDeckClutter,
    PlayTopCardAndExhaust,
    CombatUpgradeSingle,
    CombatUpgradeAll,
}

impl PlayEffect {
    pub fn vocabulary_token_v1(self) -> String {
        match self {
            Self::Provide(mechanic) => {
                format!("provide/{}", mechanic.vocabulary_token_v1())
            }
            Self::FrontloadDamage => "frontload_damage".to_string(),
            Self::AreaDamage => "area_damage".to_string(),
            Self::DamageUses(mechanic) => {
                format!("damage_uses/{}", mechanic.vocabulary_token_v1())
            }
            Self::DamageScalesWith(axis) => {
                format!("damage_scales_with/{}", axis.vocabulary_token_v1())
            }
            Self::EmitEvent(event) => {
                format!("emit_event/{}", event.vocabulary_token_v1())
            }
            Self::ExhaustsSelf => "exhausts_self".to_string(),
            Self::RunReward(reward) => {
                format!("run_reward/{}", reward.vocabulary_token_v1())
            }
            Self::RecoverCurrentHp => "recover_current_hp".to_string(),
            Self::CostReducedByHpLossThisCombat => {
                "cost_reduced_by_hp_loss_this_combat".to_string()
            }
            Self::AddCombatDeckClutter => "add_combat_deck_clutter".to_string(),
            Self::PlayTopCardAndExhaust => "play_top_card_and_exhaust".to_string(),
            Self::CombatUpgradeSingle => "combat_upgrade_single".to_string(),
            Self::CombatUpgradeAll => "combat_upgrade_all".to_string(),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InstalledRule {
    SkillCardsCostZeroAndExhaust,
}

impl InstalledRule {
    pub const fn vocabulary_token_v1(self) -> &'static str {
        match self {
            Self::SkillCardsCostZeroAndExhaust => "skill_cards_cost_zero_and_exhaust",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TriggeredEffect {
    Provide(Mechanic),
    LoseHpFromCard,
    DealAllDamage,
}

impl TriggeredEffect {
    pub fn vocabulary_token_v1(self) -> String {
        match self {
            Self::Provide(mechanic) => {
                format!("provide/{}", mechanic.vocabulary_token_v1())
            }
            Self::LoseHpFromCard => "lose_hp_from_card".to_string(),
            Self::DealAllDamage => "deal_all_damage".to_string(),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct EventHandler {
    pub on: CombatEvent,
    pub effect: TriggeredEffect,
}

impl EventHandler {
    pub const fn new(on: CombatEvent, effect: TriggeredEffect) -> Self {
        Self { on, effect }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PayoffRequirement {
    WantsMechanic(Mechanic),
    WantsEventStream(CombatEvent),
}

impl PayoffRequirement {
    pub fn vocabulary_token_v1(self) -> String {
        match self {
            Self::WantsMechanic(mechanic) => {
                format!("wants_mechanic/{}", mechanic.vocabulary_token_v1())
            }
            Self::WantsEventStream(event) => {
                format!("wants_event_stream/{}", event.vocabulary_token_v1())
            }
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CardBurden {
    PowerSetup,
    HpCost,
    DrawLockout,
    AddsCombatDeckClutter,
    RandomExhaust,
    ExhaustsHand,
    RequiresEnemyAttackIntent,
    CardBlockLockoutUntilNextTurn,
}

impl CardBurden {
    pub const fn vocabulary_token_v1(self) -> &'static str {
        match self {
            Self::PowerSetup => "power_setup",
            Self::HpCost => "hp_cost",
            Self::DrawLockout => "draw_lockout",
            Self::AddsCombatDeckClutter => "adds_combat_deck_clutter",
            Self::RandomExhaust => "random_exhaust",
            Self::ExhaustsHand => "exhausts_hand",
            Self::RequiresEnemyAttackIntent => "requires_enemy_attack_intent",
            Self::CardBlockLockoutUntilNextTurn => "card_block_lockout_until_next_turn",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DuplicateBehavior {
    Normal,
    StackingHandler,
    StackingOutput,
    RedundantAfterInstalled,
    DiminishingReturn,
    AccessCopyUseful,
}

impl DuplicateBehavior {
    pub const fn vocabulary_token_v1(self) -> &'static str {
        match self {
            Self::Normal => "normal",
            Self::StackingHandler => "stacking_handler",
            Self::StackingOutput => "stacking_output",
            Self::RedundantAfterInstalled => "redundant_after_installed",
            Self::DiminishingReturn => "diminishing_return",
            Self::AccessCopyUseful => "access_copy_useful",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CardDefinition {
    pub card: CardId,
    pub play_effects: Vec<PlayEffect>,
    pub installed_rules: Vec<InstalledRule>,
    pub event_handlers: Vec<EventHandler>,
    pub payoff_requirements: Vec<PayoffRequirement>,
    pub burdens: Vec<CardBurden>,
    pub duplicate_behaviors: Vec<DuplicateBehavior>,
}

impl CardDefinition {
    pub fn new(card: CardId) -> Self {
        Self {
            card,
            play_effects: Vec::new(),
            installed_rules: Vec::new(),
            event_handlers: Vec::new(),
            payoff_requirements: Vec::new(),
            burdens: Vec::new(),
            duplicate_behaviors: Vec::new(),
        }
    }

    pub fn effect(mut self, effect: PlayEffect) -> Self {
        push_unique(&mut self.play_effects, effect);
        self
    }

    pub fn provides(self, mechanic: Mechanic) -> Self {
        self.effect(PlayEffect::Provide(mechanic))
    }

    pub fn installs(mut self, rule: InstalledRule) -> Self {
        push_unique(&mut self.installed_rules, rule);
        self
    }

    pub fn handles(mut self, handler: EventHandler) -> Self {
        push_unique(&mut self.event_handlers, handler);
        self
    }

    pub fn wants(mut self, requirement: PayoffRequirement) -> Self {
        push_unique(&mut self.payoff_requirements, requirement);
        self
    }

    pub fn burden(mut self, burden: CardBurden) -> Self {
        push_unique(&mut self.burdens, burden);
        self
    }

    pub fn duplicate(mut self, behavior: DuplicateBehavior) -> Self {
        if behavior != DuplicateBehavior::Normal {
            push_unique(&mut self.duplicate_behaviors, behavior);
        }
        self
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct DeckMechanicContext {
    pub mechanics: Vec<Mechanic>,
    pub event_streams: Vec<CombatEvent>,
    pub repeatable_event_streams: Vec<CombatEvent>,
    pub installed_rules: Vec<InstalledRule>,
    pub event_handlers: Vec<EventHandler>,
    pub burdens: Vec<CardBurden>,
    pub duplicate_behaviors: Vec<DuplicateBehavior>,
    pub payoff_requirements: Vec<PayoffRequirement>,
    pub open_payoff_requirements: Vec<PayoffRequirement>,
}

impl DeckMechanicContext {
    pub fn from_definitions(definitions: &[CardDefinition]) -> Self {
        let mut context = Self::default();
        for definition in definitions {
            context.add_direct_definition_facts(definition);
        }
        context.derive_triggered_facts();
        for definition in definitions {
            for requirement in &definition.payoff_requirements {
                push_unique(&mut context.payoff_requirements, *requirement);
            }
            for requirement in &definition.payoff_requirements {
                if !context.satisfies(*requirement) {
                    push_unique(&mut context.open_payoff_requirements, *requirement);
                }
            }
        }
        context
    }

    pub fn satisfies(&self, requirement: PayoffRequirement) -> bool {
        match requirement {
            PayoffRequirement::WantsMechanic(mechanic) => self.mechanics.contains(&mechanic),
            PayoffRequirement::WantsEventStream(event) => self.event_streams.contains(&event),
        }
    }

    fn add_direct_definition_facts(&mut self, definition: &CardDefinition) {
        let exhausts_self = definition.play_effects.contains(&PlayEffect::ExhaustsSelf);
        for effect in &definition.play_effects {
            match effect {
                PlayEffect::Provide(mechanic) => push_unique(&mut self.mechanics, *mechanic),
                PlayEffect::EmitEvent(event) => {
                    push_unique(&mut self.event_streams, *event);
                    if !exhausts_self {
                        push_unique(&mut self.repeatable_event_streams, *event);
                    }
                }
                PlayEffect::PlayTopCardAndExhaust => {
                    push_unique(&mut self.event_streams, CombatEvent::CardExhausted);
                }
                PlayEffect::AddCombatDeckClutter => {
                    push_unique(&mut self.event_streams, CombatEvent::StatusDrawn);
                }
                PlayEffect::FrontloadDamage
                | PlayEffect::AreaDamage
                | PlayEffect::DamageUses(_)
                | PlayEffect::DamageScalesWith(_)
                | PlayEffect::ExhaustsSelf
                | PlayEffect::RunReward(_)
                | PlayEffect::RecoverCurrentHp
                | PlayEffect::CostReducedByHpLossThisCombat
                | PlayEffect::CombatUpgradeSingle
                | PlayEffect::CombatUpgradeAll => {}
            }
        }
        for rule in &definition.installed_rules {
            push_unique(&mut self.installed_rules, *rule);
        }
        for handler in &definition.event_handlers {
            push_unique(&mut self.event_handlers, *handler);
        }
        for burden in &definition.burdens {
            push_unique(&mut self.burdens, *burden);
        }
        for behavior in &definition.duplicate_behaviors {
            push_unique(&mut self.duplicate_behaviors, *behavior);
        }
    }

    fn derive_triggered_facts(&mut self) {
        let handlers = self.event_handlers.clone();
        for handler in handlers {
            if !self.event_is_available(handler.on) {
                continue;
            }
            match handler.effect {
                TriggeredEffect::Provide(mechanic) => push_unique(&mut self.mechanics, mechanic),
                TriggeredEffect::LoseHpFromCard => {
                    push_unique(&mut self.event_streams, CombatEvent::CardSelfDamage);
                    push_unique(
                        &mut self.repeatable_event_streams,
                        CombatEvent::CardSelfDamage,
                    );
                }
                TriggeredEffect::DealAllDamage => {}
            }
        }
    }

    fn event_is_available(&self, event: CombatEvent) -> bool {
        matches!(event, CombatEvent::TurnStart | CombatEvent::TurnEnd)
            || self.event_streams.contains(&event)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CandidateMechanicFinding {
    PlayEffect(PlayEffect),
    InstallsRule(InstalledRule),
    EventHandler(EventHandler),
    SupportedPayoff(PayoffRequirement),
    OpenPayoff(PayoffRequirement),
    Burden(CardBurden),
    DuplicateBehavior(DuplicateBehavior),
}

pub fn card_definition(card: CardId) -> CardDefinition {
    card_definition_with_upgrades(card, 0)
}

pub fn card_definition_with_upgrades(card: CardId, upgrades: u8) -> CardDefinition {
    use CardBurden::*;
    use CardId::*;
    use CombatEvent::*;
    use DamageScalingAxis::*;
    use DuplicateBehavior::*;
    use Mechanic::*;
    use PlayEffect::*;
    use RunRewardKind::*;

    let definition = match card {
        Inflame => CardDefinition::new(card)
            .provides(Strength)
            .duplicate(StackingOutput),
        SpotWeakness => CardDefinition::new(card)
            .provides(Strength)
            .burden(RequiresEnemyAttackIntent)
            .duplicate(StackingOutput),
        DemonForm => CardDefinition::new(card)
            .handles(EventHandler::new(
                TurnStart,
                TriggeredEffect::Provide(Strength),
            ))
            .burden(PowerSetup)
            .duplicate(StackingOutput),
        Flex => CardDefinition::new(card).provides(TemporaryStrength),
        LimitBreak => CardDefinition::new(card)
            .wants(PayoffRequirement::WantsMechanic(Strength))
            .provides(StrengthMultiplier),
        HeavyBlade | SwordBoomerang => CardDefinition::new(card)
            .wants(PayoffRequirement::WantsMechanic(Strength))
            .effect(DamageUses(Strength)),
        Reaper => CardDefinition::new(card)
            .wants(PayoffRequirement::WantsMechanic(Strength))
            .effect(DamageUses(Strength))
            .effect(AreaDamage)
            .effect(RecoverCurrentHp),
        Pummel => CardDefinition::new(card)
            .wants(PayoffRequirement::WantsMechanic(Strength))
            .effect(DamageUses(Strength))
            .effect(FrontloadDamage),

        Offering => CardDefinition::new(card)
            .provides(CardDraw)
            .provides(Energy)
            .effect(EmitEvent(CardSelfDamage))
            .effect(ExhaustsSelf)
            .burden(HpCost),
        SeeingRed => CardDefinition::new(card).provides(Energy),
        Bloodletting => CardDefinition::new(card)
            .provides(Energy)
            .effect(EmitEvent(CardSelfDamage))
            .burden(HpCost),
        Rupture => CardDefinition::new(card)
            .wants(PayoffRequirement::WantsEventStream(CardSelfDamage))
            .handles(EventHandler::new(
                CardSelfDamage,
                TriggeredEffect::Provide(Strength),
            ))
            .burden(PowerSetup)
            .duplicate(StackingHandler),

        Corruption => CardDefinition::new(card)
            .installs(InstalledRule::SkillCardsCostZeroAndExhaust)
            .burden(PowerSetup)
            .duplicate(RedundantAfterInstalled)
            .duplicate(AccessCopyUseful),
        FeelNoPain => CardDefinition::new(card)
            .wants(PayoffRequirement::WantsEventStream(CardExhausted))
            .handles(EventHandler::new(
                CardExhausted,
                TriggeredEffect::Provide(Block),
            ))
            .burden(PowerSetup)
            .duplicate(StackingHandler),
        DarkEmbrace => CardDefinition::new(card)
            .wants(PayoffRequirement::WantsEventStream(CardExhausted))
            .handles(EventHandler::new(
                CardExhausted,
                TriggeredEffect::Provide(CardDraw),
            ))
            .burden(PowerSetup)
            .duplicate(StackingHandler),
        TrueGrit => CardDefinition::new(card)
            .provides(Block)
            .effect(EmitEvent(CardExhausted))
            .burden(RandomExhaust),
        SecondWind => CardDefinition::new(card)
            .provides(Block)
            .effect(EmitEvent(CardExhausted)),
        SeverSoul => CardDefinition::new(card)
            .effect(FrontloadDamage)
            .effect(EmitEvent(CardExhausted)),
        BurningPact => CardDefinition::new(card)
            .provides(CardDraw)
            .effect(EmitEvent(CardExhausted)),
        Havoc => CardDefinition::new(card)
            .effect(PlayTopCardAndExhaust)
            .effect(EmitEvent(CardExhausted))
            .burden(RandomExhaust),

        Armaments => {
            let upgrade_effect = if upgrades > 0 {
                CombatUpgradeAll
            } else {
                CombatUpgradeSingle
            };
            CardDefinition::new(card)
                .provides(Block)
                .effect(upgrade_effect)
        }
        BattleTrance => CardDefinition::new(card)
            .provides(CardDraw)
            .burden(DrawLockout)
            .duplicate(DiminishingReturn)
            .duplicate(AccessCopyUseful),
        Finesse => CardDefinition::new(card).provides(Block).provides(CardDraw),
        FlashOfSteel => CardDefinition::new(card)
            .effect(FrontloadDamage)
            .provides(CardDraw),
        PommelStrike => CardDefinition::new(card)
            .effect(FrontloadDamage)
            .provides(CardDraw),
        ShrugItOff => CardDefinition::new(card).provides(Block).provides(CardDraw),
        Trip => CardDefinition::new(card).provides(Vulnerable),
        Bash | ThunderClap => CardDefinition::new(card)
            .effect(FrontloadDamage)
            .provides(Vulnerable),
        Shockwave => CardDefinition::new(card)
            .provides(Vulnerable)
            .provides(Weak)
            .effect(ExhaustsSelf),
        Uppercut => CardDefinition::new(card)
            .effect(FrontloadDamage)
            .provides(Vulnerable)
            .provides(Weak),
        Clothesline => CardDefinition::new(card)
            .effect(FrontloadDamage)
            .provides(Weak),
        FlameBarrier | Impervious => CardDefinition::new(card).provides(Block),
        IronWave => CardDefinition::new(card)
            .effect(FrontloadDamage)
            .provides(Block),
        Carnage | Rampage | SwiftStrike | TwinStrike => {
            CardDefinition::new(card).effect(FrontloadDamage)
        }
        Feed => CardDefinition::new(card)
            .effect(FrontloadDamage)
            .effect(ExhaustsSelf)
            .effect(RunReward(MaxHpOnFatal)),
        FiendFire => CardDefinition::new(card)
            .effect(FrontloadDamage)
            .effect(EmitEvent(CardExhausted))
            .effect(DamageScalesWith(HandSize))
            .effect(DamageScalesWith(PerHitStrength))
            .burden(ExhaustsHand),
        Whirlwind => CardDefinition::new(card)
            .effect(AreaDamage)
            .effect(DamageScalesWith(EnergySpent))
            .effect(DamageScalesWith(PerHitStrength)),
        Cleave => CardDefinition::new(card).effect(AreaDamage),
        Immolate => CardDefinition::new(card)
            .effect(AreaDamage)
            .effect(AddCombatDeckClutter)
            .burden(AddsCombatDeckClutter),
        Hemokinesis => CardDefinition::new(card)
            .effect(FrontloadDamage)
            .effect(EmitEvent(CardSelfDamage))
            .burden(HpCost),
        BodySlam => CardDefinition::new(card)
            .wants(PayoffRequirement::WantsMechanic(Block))
            .effect(DamageUses(Block)),
        Entrench => CardDefinition::new(card).wants(PayoffRequirement::WantsMechanic(Block)),
        PowerThrough => CardDefinition::new(card)
            .provides(Block)
            .effect(AddCombatDeckClutter)
            .burden(AddsCombatDeckClutter),
        WildStrike | RecklessCharge => CardDefinition::new(card)
            .effect(FrontloadDamage)
            .effect(AddCombatDeckClutter)
            .burden(AddsCombatDeckClutter),
        Headbutt => CardDefinition::new(card)
            .effect(FrontloadDamage)
            .provides(TopdeckControl),
        Warcry => CardDefinition::new(card).provides(TopdeckControl),
        Evolve => CardDefinition::new(card)
            .wants(PayoffRequirement::WantsEventStream(StatusDrawn))
            .handles(EventHandler::new(
                StatusDrawn,
                TriggeredEffect::Provide(CardDraw),
            ))
            .burden(PowerSetup)
            .duplicate(StackingHandler),
        FireBreathing => CardDefinition::new(card)
            .wants(PayoffRequirement::WantsEventStream(StatusDrawn))
            .handles(EventHandler::new(
                StatusDrawn,
                TriggeredEffect::DealAllDamage,
            ))
            .burden(PowerSetup)
            .duplicate(StackingOutput),
        Metallicize => CardDefinition::new(card)
            .handles(EventHandler::new(TurnEnd, TriggeredEffect::Provide(Block)))
            .burden(PowerSetup)
            .duplicate(StackingOutput),
        Combust => CardDefinition::new(card)
            .handles(EventHandler::new(TurnEnd, TriggeredEffect::LoseHpFromCard))
            .handles(EventHandler::new(TurnEnd, TriggeredEffect::DealAllDamage))
            .burden(PowerSetup)
            .duplicate(StackingOutput),
        Brutality => CardDefinition::new(card)
            .handles(EventHandler::new(
                TurnStart,
                TriggeredEffect::LoseHpFromCard,
            ))
            .handles(EventHandler::new(
                TurnStart,
                TriggeredEffect::Provide(CardDraw),
            ))
            .burden(PowerSetup)
            .duplicate(StackingOutput),
        MasterOfStrategy => CardDefinition::new(card)
            .provides(CardDraw)
            .effect(ExhaustsSelf),
        GoodInstincts => CardDefinition::new(card).provides(Block),
        Intimidate => CardDefinition::new(card)
            .provides(Weak)
            .effect(ExhaustsSelf),
        DarkShackles | PiercingWail => CardDefinition::new(card).effect(ExhaustsSelf),
        BandageUp => CardDefinition::new(card)
            .effect(RecoverCurrentHp)
            .effect(ExhaustsSelf),
        PanicButton => CardDefinition::new(card)
            .provides(Block)
            .effect(ExhaustsSelf)
            .burden(CardBlockLockoutUntilNextTurn),
        HandOfGreed => CardDefinition::new(card)
            .effect(FrontloadDamage)
            .effect(RunReward(GoldOnFatal)),
        BloodForBlood => CardDefinition::new(card)
            .effect(FrontloadDamage)
            .effect(CostReducedByHpLossThisCombat),
        // Juggernaut needs a future BlockGained reactive-power model; do not
        // flatten it into ordinary frontload damage.
        Juggernaut => CardDefinition::new(card),
        _ => CardDefinition::new(card),
    };
    use crate::content::cards::mechanics::{
        card_enemy_strength_restoration_v1, EnemyStrengthRestorationV1,
    };
    match card_enemy_strength_restoration_v1(card) {
        Some(EnemyStrengthRestorationV1::NoScheduledRestoration) => {
            definition.provides(EnemyStrengthDown)
        }
        Some(EnemyStrengthRestorationV1::ShackledAtOwnerTurnEnd) => {
            definition.provides(TemporaryEnemyStrengthDown)
        }
        None => definition,
    }
}

pub fn evaluate_candidate_definition(
    candidate: &CardDefinition,
    deck: &DeckMechanicContext,
) -> Vec<CandidateMechanicFinding> {
    let mut findings = Vec::new();
    for effect in &candidate.play_effects {
        findings.push(CandidateMechanicFinding::PlayEffect(*effect));
    }
    for rule in &candidate.installed_rules {
        findings.push(CandidateMechanicFinding::InstallsRule(*rule));
    }
    for handler in &candidate.event_handlers {
        findings.push(CandidateMechanicFinding::EventHandler(*handler));
    }
    for requirement in &candidate.payoff_requirements {
        if deck.satisfies(*requirement) {
            findings.push(CandidateMechanicFinding::SupportedPayoff(*requirement));
        } else {
            findings.push(CandidateMechanicFinding::OpenPayoff(*requirement));
        }
    }
    for burden in &candidate.burdens {
        findings.push(CandidateMechanicFinding::Burden(*burden));
    }
    for behavior in &candidate.duplicate_behaviors {
        findings.push(CandidateMechanicFinding::DuplicateBehavior(*behavior));
    }
    findings
}

fn push_unique<T: Eq>(items: &mut Vec<T>, value: T) {
    if !items.contains(&value) {
        items.push(value);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reaper_semantics_include_aoe_sustain_and_strength_scaling() {
        let definition = card_definition(CardId::Reaper);

        assert!(definition
            .play_effects
            .contains(&PlayEffect::DamageUses(Mechanic::Strength)));
        assert!(definition.play_effects.contains(&PlayEffect::AreaDamage));
        assert!(definition
            .play_effects
            .contains(&PlayEffect::RecoverCurrentHp));
    }

    #[test]
    fn dark_shackles_semantics_distinguish_temporary_enemy_strength_down() {
        let definition = card_definition(CardId::DarkShackles);

        assert!(definition
            .play_effects
            .contains(&PlayEffect::Provide(Mechanic::TemporaryEnemyStrengthDown)));
        assert!(!definition
            .play_effects
            .contains(&PlayEffect::Provide(Mechanic::EnemyStrengthDown)));
        assert!(definition.play_effects.contains(&PlayEffect::ExhaustsSelf));
    }

    #[test]
    fn shockwave_semantics_do_not_claim_direct_strength_reduction() {
        let definition = card_definition(CardId::Shockwave);

        assert!(definition
            .play_effects
            .contains(&PlayEffect::Provide(Mechanic::Weak)));
        assert!(definition
            .play_effects
            .contains(&PlayEffect::Provide(Mechanic::Vulnerable)));
        assert!(definition.play_effects.contains(&PlayEffect::ExhaustsSelf));
        assert!(!definition
            .play_effects
            .contains(&PlayEffect::Provide(Mechanic::EnemyStrengthDown)));
    }

    #[test]
    fn offering_self_damage_is_available_but_not_repeatable() {
        let context = DeckMechanicContext::from_definitions(&[card_definition(CardId::Offering)]);

        assert!(context.event_streams.contains(&CombatEvent::CardSelfDamage));
        assert!(!context
            .repeatable_event_streams
            .contains(&CombatEvent::CardSelfDamage));
    }

    #[test]
    fn non_exhausting_direct_self_damage_is_repeatable() {
        for card in [CardId::Bloodletting, CardId::Hemokinesis] {
            let context = DeckMechanicContext::from_definitions(&[card_definition(card)]);

            assert!(context
                .repeatable_event_streams
                .contains(&CombatEvent::CardSelfDamage));
        }
    }

    #[test]
    fn recurring_power_self_damage_is_repeatable() {
        for card in [CardId::Combust, CardId::Brutality] {
            let context = DeckMechanicContext::from_definitions(&[card_definition(card)]);

            assert!(context
                .repeatable_event_streams
                .contains(&CombatEvent::CardSelfDamage));
        }
    }

    #[test]
    fn fire_breathing_semantics_expose_conditional_status_damage() {
        let definition = card_definition(CardId::FireBreathing);

        assert!(definition
            .payoff_requirements
            .contains(&PayoffRequirement::WantsEventStream(
                CombatEvent::StatusDrawn
            )));
        assert!(definition.event_handlers.contains(&EventHandler::new(
            CombatEvent::StatusDrawn,
            TriggeredEffect::DealAllDamage,
        )));
        assert!(definition.burdens.contains(&CardBurden::PowerSetup));
    }
}
