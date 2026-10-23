use std::fmt::Write;

use super::{replay_and_project_actions, selected_action, CompleteTurnSelectedActionV1};
use sts_agent::agent::information::action::{
    project_public_combat_actions_v1, public_choice_for_exact_combat_input_v1,
    PublicCombatAtomicActionV1, PublicCombatSelectionDomainCandidateV1,
};
use sts_agent::agent::information::state::{
    public_combat_state_v1, CombatLearningCardV1, CombatLearningIntentV1, CombatLearningPowerV1,
    PublicCombatStateV1,
};
use sts_combat_planner::{
    CombatDraftContinuation, CombatDraftFrameHandle, CombatDraftOptionCoverage,
    CombatDraftOptionHandle, CombatDraftSession, CompleteTurnOptionBoundary,
};

/// Cheap current-turn index. It exposes enough meaning to choose which `Tn`
/// deserves inspection without replaying and printing every full successor.
#[derive(Clone, Debug, PartialEq)]
pub struct CombatDraftTurnViewV1 {
    pub frame: CombatDraftFrameHandle,
    pub selected_turn_count: usize,
    pub coverage: CombatDraftOptionCoverage,
    pub root: PublicCombatStateV1,
    pub options: Vec<CombatDraftOptionIndexEntryV1>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct CombatDraftOptionIndexEntryV1 {
    pub handle: CombatDraftOptionHandle,
    pub first_action: Option<CompleteTurnSelectedActionV1>,
    pub action_count: usize,
    pub boundary: CompleteTurnOptionBoundary,
    pub continuation: Option<CombatDraftContinuation>,
    pub successor: crate::CombatDraftSuccessorSummaryV1,
}

struct CombatDraftOptionSection<'a> {
    first_action: Option<&'a CompleteTurnSelectedActionV1>,
    buckets: Vec<CombatDraftOptionBucket>,
}

struct CombatDraftOptionBucket {
    action_count: usize,
    boundary: CompleteTurnOptionBoundary,
    handles: Vec<CombatDraftOptionHandle>,
}

impl<'a> CombatDraftOptionSection<'a> {
    fn new(option: &'a CombatDraftOptionIndexEntryV1) -> Self {
        Self {
            first_action: option.first_action.as_ref(),
            buckets: vec![CombatDraftOptionBucket::new(option)],
        }
    }

    fn push(&mut self, option: &CombatDraftOptionIndexEntryV1) {
        if let Some(bucket) = self
            .buckets
            .iter_mut()
            .find(|bucket| bucket.matches(option))
        {
            bucket.handles.push(option.handle);
        } else {
            self.buckets.push(CombatDraftOptionBucket::new(option));
        }
    }
}

impl CombatDraftOptionBucket {
    fn new(option: &CombatDraftOptionIndexEntryV1) -> Self {
        Self {
            action_count: option.action_count,
            boundary: option.boundary,
            handles: vec![option.handle],
        }
    }

    fn matches(&self, option: &CombatDraftOptionIndexEntryV1) -> bool {
        self.action_count == option.action_count && self.boundary == option.boundary
    }
}

/// On-demand public detail for one complete-turn candidate.
///
/// Exact identities remain inside replay. This value contains only the short
/// frame-local handle, public action semantics, and public successor state.
#[derive(Clone, Debug, PartialEq)]
pub struct CombatDraftOptionViewV1 {
    pub handle: CombatDraftOptionHandle,
    pub actions: Vec<CompleteTurnSelectedActionV1>,
    pub boundary: CompleteTurnOptionBoundary,
    pub successor: PublicCombatStateV1,
}

pub fn project_combat_draft_turn_v1(
    draft: &CombatDraftSession,
) -> Result<CombatDraftTurnViewV1, String> {
    let root = draft
        .current_root()
        .ok_or_else(|| "combat draft is terminal; no current turn remains".to_owned())?;
    let observation = public_combat_state_v1(&root.position().combat);
    let surface =
        project_public_combat_actions_v1(&root.position().engine, &root.position().combat)
            .map_err(|error| format!("combat draft root action projection failed: {error}"))?;
    let options = draft
        .option_handles()
        .into_iter()
        .map(|handle| {
            let option = draft
                .option(handle)
                .map_err(|error| format!("combat draft option {handle} is unavailable: {error:?}"))?;
            let first_action = option
                .actions()
                .first()
                .map(|action| {
                    let choice = public_choice_for_exact_combat_input_v1(&surface, &action.input)
                        .map_err(|error| {
                            format!(
                                "combat draft option {handle} first action has no public choice: {error}"
                            )
                        })?;
                    selected_action(&observation, &surface.public, &choice)
                })
                .transpose()?;
            Ok(CombatDraftOptionIndexEntryV1 {
                handle,
                first_action,
                action_count: option.actions().len(),
                boundary: option.boundary(),
                successor: crate::draft_successor::project_draft_successor(&option.exact_successor().combat),
                continuation: draft.option_continuation(handle)
                    .map_err(|error| format!("draft continuation unavailable: {error:?}"))?,
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    Ok(CombatDraftTurnViewV1 {
        frame: draft
            .current_frame_handle()
            .expect("an active draft has a frame"),
        selected_turn_count: draft.selected_turn_count(),
        coverage: draft
            .current_option_coverage()
            .expect("a non-terminal draft has current option coverage"),
        root: observation,
        options,
    })
}

pub fn project_combat_draft_option_v1(
    draft: &CombatDraftSession,
    handle: CombatDraftOptionHandle,
    max_engine_steps_per_action: usize,
) -> Result<CombatDraftOptionViewV1, String> {
    let root = draft
        .current_root()
        .ok_or_else(|| "combat draft is terminal; no current option remains".to_owned())?;
    let option = draft
        .option(handle)
        .map_err(|error| format!("combat draft option {handle} is unavailable: {error:?}"))?;
    let (actions, replayed) = replay_and_project_actions(
        root.position(),
        option
            .actions()
            .iter()
            .map(|action| (&action.input, Some(action.expected_successor_hash.as_str()))),
        max_engine_steps_per_action,
        "combat draft option",
    )?;
    if replayed != *option.exact_successor() {
        return Err(format!(
            "combat draft option {handle} final exact successor drifted"
        ));
    }
    Ok(CombatDraftOptionViewV1 {
        handle,
        actions,
        boundary: option.boundary(),
        successor: public_combat_state_v1(&replayed.combat),
    })
}

pub fn render_combat_draft_turn_v1(view: &CombatDraftTurnViewV1) -> Result<String, String> {
    let mut text = String::from("combat-draft-v1\n");
    writeln!(
        text,
        "frame={} selected_turns={} available_options={} coverage={}",
        view.frame,
        view.selected_turn_count,
        view.options.len(),
        coverage_token(view.coverage),
    )
    .unwrap();
    append_public_state(&mut text, "turn", &view.root);
    text.push_str("\n[options]\n");
    if view.options.is_empty() {
        text.push_str("none\n");
    }
    for section in group_combat_draft_options(&view.options) {
        let first = section
            .first_action
            .map(render_action_summary)
            .transpose()?
            .unwrap_or_else(|| "none".to_owned());
        writeln!(text, "FIRST {first}").unwrap();
        for bucket in section.buckets {
            write!(
                text,
                "  actions={} boundary={} options=",
                bucket.action_count,
                boundary_token(bucket.boundary),
            )
            .unwrap();
            for (ordinal, handle) in bucket.handles.iter().enumerate() {
                if ordinal > 0 {
                    text.push(',');
                }
                write!(text, "{handle}").unwrap();
            }
            text.push('\n');
        }
    }
    let mut explored_heading = false;
    for option in &view.options {
        if let Some(child) = option.continuation {
            if !explored_heading {
                text.push_str("\n[explored]\n");
                explored_heading = true;
            }
            writeln!(
                text,
                "{} -> {} | {} options | work {} | {}",
                option.handle,
                child.frame,
                child.available_option_count,
                child.counters.generation_work,
                coverage_token(child.coverage)
            )
            .unwrap();
        }
    }
    Ok(text)
}

fn group_combat_draft_options(
    options: &[CombatDraftOptionIndexEntryV1],
) -> Vec<CombatDraftOptionSection<'_>> {
    let mut sections: Vec<CombatDraftOptionSection<'_>> = Vec::new();
    for option in options {
        let first_action = option.first_action.as_ref();
        if let Some(section) = sections
            .iter_mut()
            .find(|section| section.first_action == first_action)
        {
            section.push(option);
        } else {
            sections.push(CombatDraftOptionSection::new(option));
        }
    }
    sections
}

pub fn render_combat_draft_option_v1(view: &CombatDraftOptionViewV1) -> Result<String, String> {
    let mut text = format!(
        "combat-draft-option-v1\nOPTION {} boundary={} actions={}\n\n[actions]\n",
        view.handle,
        boundary_token(view.boundary),
        view.actions.len(),
    );
    if view.actions.is_empty() {
        text.push_str("none\n");
    }
    for (ordinal, action) in view.actions.iter().enumerate() {
        writeln!(text, "ACTION A{ordinal} {}", render_action_summary(action)?).unwrap();
    }
    text.push('\n');
    append_public_state(&mut text, "next", &view.successor);
    Ok(text)
}

pub(crate) fn append_public_state(text: &mut String, section: &str, state: &PublicCombatStateV1) {
    writeln!(text, "[{section}]").unwrap();
    writeln!(
        text,
        "TURN {} player_hp={}/{} block={} energy={}",
        state.turn.turn_count,
        state.player.hp,
        state.player.max_hp,
        state.player.block,
        state.turn.energy,
    )
    .unwrap();
    append_powers(text, "PLAYER_POWER", &state.player.powers);

    if state.cards.hand.cards.is_empty() {
        text.push_str("HAND none\n");
    } else {
        for (ordinal, card) in state.cards.hand.cards.iter().enumerate() {
            writeln!(text, "HAND C{ordinal} {}", card_summary(card)).unwrap();
        }
    }
    writeln!(
        text,
        "PILES draw={} discard={} exhaust={}",
        state.cards.draw.cards.len(),
        state.cards.discard.cards.len(),
        state.cards.exhaust.cards.len(),
    )
    .unwrap();

    if state.potions.is_empty() {
        text.push_str("POTIONS none\n");
    } else {
        for (slot, potion) in state.potions.iter().enumerate() {
            match potion {
                Some(potion) => {
                    writeln!(text, "POTION P{slot} {}", potion.potion_id.stable_id()).unwrap()
                }
                None => writeln!(text, "POTION P{slot} empty").unwrap(),
            }
        }
    }

    if state.monsters.is_empty() {
        text.push_str("ENEMIES none\n");
    }
    for (ordinal, monster) in state.monsters.iter().enumerate() {
        let enemy = match monster.enemy {
            sts_agent::agent::information::state::CombatLearningEnemyIdentityV1::Known {
                enemy_id,
            } => enemy_id.stable_id(),
        };
        let status = if monster.alive { "active" } else { "inactive" };
        writeln!(
            text,
            "ENEMY E{ordinal} {} hp={}/{} block={} status={} intent={}",
            enemy,
            monster.hp,
            monster.max_hp,
            monster.block,
            status,
            intent_summary(&monster.intent),
        )
        .unwrap();
        append_powers(text, &format!("E{ordinal}_POWER"), &monster.powers);
    }
}

fn append_powers(text: &mut String, prefix: &str, powers: &[CombatLearningPowerV1]) {
    for power in powers {
        writeln!(
            text,
            "{prefix} {} amount={}",
            power.power.stable_id(),
            power.amount
        )
        .unwrap();
    }
}

fn card_summary(card: &CombatLearningCardV1) -> String {
    format!(
        "{} upgrades={} cost={} damage={} block={} magic={}",
        card.card_id.stable_id(),
        card.upgrades,
        card.effective_cost,
        card.current_damage,
        card.current_block,
        card.current_magic_number,
    )
}

fn intent_summary(intent: &CombatLearningIntentV1) -> String {
    let preview_damage_per_hit = intent.preview_damage_per_hit;
    let Some(intent) = intent.intent.as_ref() else {
        return "hidden_or_unknown".to_owned();
    };
    let hits = match intent {
        sts_core::runtime::combat::Intent::Attack { hits, .. }
        | sts_core::runtime::combat::Intent::AttackBuff { hits, .. }
        | sts_core::runtime::combat::Intent::AttackDebuff { hits, .. }
        | sts_core::runtime::combat::Intent::AttackDefend { hits, .. } => Some(*hits),
        _ => None,
    };
    match (preview_damage_per_hit, hits) {
        (Some(damage), Some(hits)) => format!("{}:{damage}x{hits}", intent.kind_v1()),
        (Some(damage), None) => format!("{}:{damage}", intent.kind_v1()),
        (None, _) => intent.kind_v1().to_owned(),
    }
}

fn render_action_summary(action: &CompleteTurnSelectedActionV1) -> Result<String, String> {
    render_action(action, false, sts_presentation::Locale::En)
}

pub fn render_action_brief(
    action: &CompleteTurnSelectedActionV1,
    locale: sts_presentation::Locale,
) -> Result<String, String> {
    render_action(action, true, locale)
}

fn render_action(
    action: &CompleteTurnSelectedActionV1,
    brief: bool,
    locale: sts_presentation::Locale,
) -> Result<String, String> {
    match action {
        CompleteTurnSelectedActionV1::Atomic {
            action,
            card,
            potion,
            ..
        } => match action {
            PublicCombatAtomicActionV1::PlayCard {
                hand_index,
                target_monster_index,
            } => {
                let card = card
                    .as_ref()
                    .ok_or_else(|| "draft play action is missing its public card".to_owned())?;
                let summary = if brief {
                    let cost = match card.effective_cost {
                        -1 => crate::draft_message(locale, "xCost", &[]),
                        -2 => locale
                            .text("mechanics", "card.unplayable", &[])
                            .expect("unplayable card"),
                        cost => {
                            crate::draft_message(locale, "cost", &[("cost", &cost.to_string())])
                        }
                    };
                    format!(
                        "{}[{cost}]",
                        crate::selection_intent::card_name(
                            Some(card.card_id),
                            Some(card.upgrades),
                            locale
                        ),
                    )
                } else {
                    card_summary(card)
                };
                if brief {
                    return Ok(target_monster_index.map_or_else(
                        || {
                            crate::draft_message(
                                locale,
                                "play",
                                &[
                                    ("hand", &(hand_index.to_string())),
                                    ("card", &(summary.clone())),
                                ],
                            )
                        },
                        |target| {
                            crate::draft_message(
                                locale,
                                "playTarget",
                                &[
                                    ("hand", &(hand_index.to_string())),
                                    ("card", &(summary.clone())),
                                    ("target", &(target.to_string())),
                                ],
                            )
                        },
                    ));
                }
                Ok(target_monster_index.map_or_else(
                    || format!("play hand[{hand_index}] {summary}"),
                    |target| format!("play hand[{hand_index}] {summary} target=E{target}"),
                ))
            }
            PublicCombatAtomicActionV1::UsePotion {
                potion_index,
                target_monster_index,
            } => {
                let potion = potion
                    .as_ref()
                    .ok_or_else(|| "draft potion action is missing its public potion".to_owned())?;
                let name = if brief {
                    locale.entity("potion", potion.potion_id.stable_id())
                } else {
                    potion.potion_id.stable_id()
                };
                if brief {
                    return Ok(target_monster_index.map_or_else(
                        || {
                            crate::draft_message(
                                locale,
                                "use",
                                &[
                                    ("slot", &(potion_index.to_string())),
                                    ("potion", &(name.to_owned())),
                                ],
                            )
                        },
                        |target| {
                            crate::draft_message(
                                locale,
                                "useTarget",
                                &[
                                    ("slot", &(potion_index.to_string())),
                                    ("potion", &(name.to_owned())),
                                    ("target", &(target.to_string())),
                                ],
                            )
                        },
                    ));
                }
                Ok(target_monster_index.map_or_else(
                    || format!("use P{potion_index} {name}"),
                    |target| format!("use P{potion_index} {name} target=E{target}"),
                ))
            }
            PublicCombatAtomicActionV1::DiscardPotion { potion_index } => {
                let potion = potion.as_ref().ok_or_else(|| {
                    "draft potion discard is missing its public potion".to_owned()
                })?;
                let name = if brief {
                    locale.entity("potion", potion.potion_id.stable_id())
                } else {
                    potion.potion_id.stable_id()
                };
                Ok(if brief {
                    crate::draft_message(
                        locale,
                        "discard",
                        &[("slot", &potion_index.to_string()), ("potion", name)],
                    )
                } else {
                    format!("discard P{potion_index} {name}")
                })
            }
            PublicCombatAtomicActionV1::EndTurn => Ok(if brief {
                crate::draft_message(locale, "end", &[])
            } else {
                "end_turn".to_owned()
            }),
            PublicCombatAtomicActionV1::SubmitIndexedChoice { choice_index } => Ok(if brief {
                crate::draft_message(locale, "choose", &[("index", &choice_index.to_string())])
            } else {
                format!("choose option[{choice_index}]")
            }),
            PublicCombatAtomicActionV1::Proceed => Ok(if brief {
                crate::draft_message(locale, "proceed", &[])
            } else {
                "proceed".to_owned()
            }),
            PublicCombatAtomicActionV1::Cancel => Ok(if brief {
                crate::draft_message(locale, "cancel", &[])
            } else {
                "cancel".to_owned()
            }),
        },
        CompleteTurnSelectedActionV1::Selection {
            family,
            selected_domain_ordinals,
        } => {
            let selected = selected_domain_ordinals
                .iter()
                .map(|ordinal| {
                    let candidate = family.domain.get(*ordinal).ok_or_else(|| {
                        format!("draft selection ordinal {ordinal} is out of range")
                    })?;
                    Ok(if brief {
                        let (id, upgrades) = match candidate {
                            PublicCombatSelectionDomainCandidateV1::Card {
                                card_id,
                                upgrades,
                                ..
                            } => (*card_id, *upgrades),
                            PublicCombatSelectionDomainCandidateV1::Scry { card_id, .. } => {
                                (*card_id, None)
                            }
                        };
                        format!(
                            "{ordinal} {}",
                            crate::selection_intent::card_name(id, upgrades, locale)
                        )
                    } else {
                        selection_candidate_summary(*ordinal, candidate)
                    })
                })
                .collect::<Result<Vec<_>, String>>()?;
            if brief {
                return Ok(format!(
                    "{} [{}]",
                    crate::selection_intent::reason_label(&family.reason, locale),
                    selected.join(", ")
                ));
            }
            Ok(format!(
                "select reason={:?} [{}]",
                family.reason,
                selected.join(", ")
            ))
        }
    }
}

fn selection_candidate_summary(
    ordinal: usize,
    candidate: &PublicCombatSelectionDomainCandidateV1,
) -> String {
    match candidate {
        PublicCombatSelectionDomainCandidateV1::Card {
            card_id, upgrades, ..
        } => card_id.map_or_else(
            || format!("choice[{ordinal}] card_unknown"),
            |card_id| {
                format!(
                    "choice[{ordinal}] {} upgrades={}",
                    card_id.stable_id(),
                    upgrades.unwrap_or(0)
                )
            },
        ),
        PublicCombatSelectionDomainCandidateV1::Scry { index, card_id, .. } => card_id.map_or_else(
            || format!("choice[{ordinal}] scry[{index}] card_unknown"),
            |card_id| format!("choice[{ordinal}] scry[{index}] {}", card_id.stable_id()),
        ),
    }
}

fn boundary_token(boundary: CompleteTurnOptionBoundary) -> &'static str {
    match boundary {
        CompleteTurnOptionBoundary::NextPlayerTurn => "next_turn",
        CompleteTurnOptionBoundary::TerminalWin => "win",
        CompleteTurnOptionBoundary::TerminalLoss => "loss",
        CompleteTurnOptionBoundary::Escape => "escape",
    }
}

fn coverage_token(coverage: CombatDraftOptionCoverage) -> String {
    match coverage {
        CombatDraftOptionCoverage::Open {
            mechanics_gap_count: 0,
        } => "open".to_owned(),
        CombatDraftOptionCoverage::Open {
            mechanics_gap_count,
        } => format!("open_with_gaps:{mechanics_gap_count}"),
        CombatDraftOptionCoverage::Complete => "complete".to_owned(),
        CombatDraftOptionCoverage::MechanicsGap { gap_count } => {
            format!("mechanics_gap:{gap_count}")
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use sts_combat_planner::{
        CombatDecisionRoot, CombatDraftSession, CombatPlanningQuantum, TurnOptionGeneratorConfig,
    };
    use sts_core::content::{cards::CardId, monsters::EnemyId};
    use sts_core::runtime::combat::CombatCard;
    use sts_core::sim::combat::{CombatPosition, EngineCombatStepper};
    use sts_core::state::core::EngineState;
    use sts_core::test_support::{blank_test_combat, planned_monster};

    use super::*;

    fn draft_with_generated_options() -> CombatDraftSession {
        let mut combat = blank_test_combat();
        combat.entities.monsters = vec![planned_monster(EnemyId::JawWorm, 1)];
        combat.zones.hand = vec![
            CombatCard::for_scenario(CardId::Defend, 11),
            CombatCard::for_scenario(CardId::Defend, 12),
            CombatCard::for_scenario(CardId::Defend, 13),
            CombatCard::for_scenario(CardId::Strike, 14),
            CombatCard::for_scenario(CardId::Brutality, 15),
        ];
        combat.turn.energy = 3;
        let root =
            CombatDecisionRoot::new(CombatPosition::new(EngineState::CombatPlayerTurn, combat))
                .unwrap();
        let mut draft = CombatDraftSession::new(
            root,
            TurnOptionGeneratorConfig {
                max_engine_steps_per_transition: 250,
                ..TurnOptionGeneratorConfig::default()
            },
        );
        let report = draft
            .advance_options(
                &EngineCombatStepper,
                CombatPlanningQuantum::deterministic(1_000, 16_384),
            )
            .unwrap();
        assert!(!draft.option_handles().is_empty(), "{report:?}");
        draft
    }

    #[test]
    fn turn_index_is_short_and_keeps_actions_nested_under_turn_handles() {
        let draft = draft_with_generated_options();
        let view = project_combat_draft_turn_v1(&draft).unwrap();
        let rendered = render_combat_draft_turn_v1(&view).unwrap();

        assert!(rendered.contains("[turn]"));
        assert!(rendered.contains("[options]"));
        assert!(rendered.contains("coverage=complete"));
        assert!(rendered.contains("FIRST play hand[0] Defend"));
        assert!(rendered.contains("options=T"));
        assert!(view.options.len() > rendered.matches("FIRST ").count());
        let rendered_handles: Vec<&str> = rendered
            .lines()
            .filter_map(|line| line.split_once(" options=").map(|(_, handles)| handles))
            .flat_map(|handles| handles.split(','))
            .collect();
        let expected_handles: Vec<String> = view
            .options
            .iter()
            .map(|option| option.handle.to_string())
            .collect();
        assert_eq!(rendered_handles.len(), expected_handles.len());
        assert_eq!(
            rendered_handles.into_iter().collect::<BTreeSet<_>>(),
            expected_handles
                .iter()
                .map(String::as_str)
                .collect::<BTreeSet<_>>()
        );
        assert!(rendered.lines().count() < view.options.len());
        assert!(!rendered.contains("exact_state"));
        assert!(!rendered.contains("sha"));
        assert!(!rendered.contains(['{', '}']));
    }

    #[test]
    fn option_detail_shows_semantic_actions_and_public_successor_only() {
        let draft = draft_with_generated_options();
        let handle = draft
            .option_handles()
            .into_iter()
            .find(|handle| {
                draft
                    .option(*handle)
                    .unwrap()
                    .actions()
                    .iter()
                    .any(|action| {
                        matches!(
                            action.input,
                            sts_core::state::core::ClientInput::PlayCard { .. }
                        )
                    })
            })
            .unwrap();
        let view = project_combat_draft_option_v1(&draft, handle, 250).unwrap();
        let rendered = render_combat_draft_option_v1(&view).unwrap();

        assert!(rendered.contains(&format!("OPTION {handle}")));
        assert!(rendered.contains("ACTION A0 play hand[0] Defend"));
        assert!(rendered.contains("ACTION A1 end_turn"));
        assert!(rendered.contains("[next]"));
        assert!(rendered.contains("ENEMY E0 JawWorm"));
        assert!(!rendered.contains("expected_successor_hash"));
        assert!(!rendered.contains(['{', '}']));
    }

    #[test]
    fn retained_resource_index_matches_replayed_public_successors() {
        use sts_core::content::potions::{Potion, PotionId};

        let mut combat = blank_test_combat();
        combat.entities.monsters = vec![planned_monster(EnemyId::JawWorm, 1)];
        combat.entities.potions = vec![None, Some(Potion::new(PotionId::FirePotion, 1))];
        combat.zones.hand = vec![CombatCard::for_scenario(CardId::Strike, 1)];
        combat.turn.energy = 1;
        let root =
            CombatDecisionRoot::new(CombatPosition::new(EngineState::CombatPlayerTurn, combat))
                .unwrap();
        let mut draft = CombatDraftSession::new(root, TurnOptionGeneratorConfig::default());
        draft
            .advance_options(
                &EngineCombatStepper,
                CombatPlanningQuantum::deterministic(1000, 65_536),
            )
            .unwrap();
        let index = project_combat_draft_turn_v1(&draft).unwrap();
        assert!(index
            .options
            .iter()
            .any(|o| o.successor.potions[1].is_some()));
        assert!(index
            .options
            .iter()
            .any(|o| o.successor.potions[1].is_none()));
        for option in &index.options {
            let review = project_combat_draft_option_v1(&draft, option.handle, 512).unwrap();
            let summary = &option.successor;
            let public = review.successor;
            assert_eq!(
                (summary.hp, summary.max_hp, summary.block),
                (public.player.hp, public.player.max_hp, public.player.block)
            );
            assert_eq!(
                summary.potions,
                public
                    .potions
                    .iter()
                    .map(|p| p.as_ref().map(|p| p.potion_id))
                    .collect::<Vec<_>>()
            );
            assert_eq!(summary.monsters.len(), public.monsters.len());
            for (small, full) in summary.monsters.iter().zip(&public.monsters) {
                assert_eq!(
                    (small.slot, small.hp, small.max_hp, small.block),
                    (full.slot, full.hp, full.max_hp, full.block)
                );
                assert_eq!(
                    (small.alive, small.escaped, small.half_dead),
                    (full.alive, full.escaped, full.half_dead)
                );
            }
        }
        assert_eq!(project_combat_draft_turn_v1(&draft).unwrap(), index);
    }
}
