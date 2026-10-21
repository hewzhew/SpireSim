use super::{NaturalAction, NaturalEpisode};
use crate::observation::{CardAddress, CardCost, CombatObservation, ModelChoice, PowerObservation};
use sts_core::content::identity::gameplay_display_label_v1 as name;
use sts_core::state::core::{ClientInput, EngineState};

pub(super) fn choice_label(o: &CombatObservation, choice: &ModelChoice) -> String {
    let target = |t: Option<usize>| {
        t.map(|i| format!(" -> M{i} {}", o.monsters[i].name))
            .unwrap_or_default()
    };
    match choice {
        ModelChoice::Play { hand, target: t } => {
            format!("Play H{hand} {}{}", o.hand[*hand].name, target(*t))
        }
        ModelChoice::UsePotion { slot, target: t } => format!(
            "Use P{slot} {}{}",
            o.potions[*slot].as_ref().map_or("", |p| &p.name),
            target(*t)
        ),
        ModelChoice::DiscardPotion { slot } => format!("Discard P{slot}"),
        ModelChoice::EndTurn {} => "End turn".into(),
        ModelChoice::Choose { name, .. } => format!("Choose {name}"),
        ModelChoice::BeginSelection { family } => {
            let s = &o.selections[*family];
            format!("Choose {}..{} cards", s.min, s.max)
        }
        ModelChoice::SelectMember { member } => {
            let s = &o.selections[o.assembling.as_ref().unwrap().family];
            let card = match s.members[*member] {
                CardAddress::Hand(i) => &o.hand[i],
                CardAddress::Draw(i) => &o.draw_top_first[i],
                CardAddress::Discard(i) => &o.discard[i],
                CardAddress::Exhaust(i) => &o.exhaust[i],
                CardAddress::Limbo(i) => &o.limbo[i],
            };
            format!("Choose {}", card.name)
        }
        ModelChoice::FinishSelection {} => "Confirm selection".into(),
        ModelChoice::Cancel {} => "Cancel".into(),
        ModelChoice::Proceed {} => "Proceed".into(),
    }
}
fn powers(p: &[PowerObservation]) -> String {
    p.iter()
        .map(|p| match p.amount {
            Some(n) => format!("{} {n}", p.name),
            None => p.name.clone(),
        })
        .collect::<Vec<_>>()
        .join(", ")
}

impl NaturalEpisode {
    /// Short decision view. Opaque sources and numerical records stay in the
    /// structured frame; blocked inventory remains visible alongside options.
    pub fn view(&self) -> Result<String, String> {
        let r = self.run();
        let o = self.combat_observation()?;
        let resources = self.resources();
        let mut lines = vec![format!(
            "Act {} F{} | HP {}/{} | Gold {}",
            r.act_num, r.floor_num, resources.hp, resources.max_hp, resources.gold
        )];
        lines.push(format!(
            "Deck: {}",
            r.master_deck
                .iter()
                .map(|c| {
                    let suffix = match c.upgrades {
                        0 => String::new(),
                        n if c.id == sts_core::content::cards::CardId::SearingBlow || n > 1 => {
                            format!("+{n}")
                        }
                        _ => "+".into(),
                    };
                    format!("{}{suffix}", name(c.id))
                })
                .collect::<Vec<_>>()
                .join(", ")
        ));
        let potions = if let Some(o) = &o {
            o.potions
                .iter()
                .enumerate()
                .map(|(i, p)| format!("P{i} {}", p.as_ref().map_or("empty", |p| &p.name)))
                .collect::<Vec<_>>()
        } else {
            r.potions
                .iter()
                .enumerate()
                .map(|(i, p)| format!("P{i} {}", p.as_ref().map_or("empty", |p| name(p.id))))
                .collect()
        };
        lines.push(format!("Potions: {}", potions.join(", ")));
        if let Some(o) = &o {
            lines.push(format!(
                "Relics: {}",
                o.relics
                    .iter()
                    .map(|r| match r.counter {
                        Some(n) => format!("{} {n}", r.name),
                        None => r.name.clone(),
                    })
                    .collect::<Vec<_>>()
                    .join(", ")
            ));
            lines.push(format!(
                "Turn {} | Energy {} | Block {}",
                o.turn, o.energy, o.block
            ));
            lines.push(format!(
                "Hand: {}",
                o.hand
                    .iter()
                    .enumerate()
                    .map(|(i, c)| {
                        let cost = match c.cost {
                            CardCost::Energy(n) => format!("{n}c"),
                            CardCost::Special(s) => s.into(),
                        };
                        format!("H{i} {}[{cost}]", c.name)
                    })
                    .collect::<Vec<_>>()
                    .join(", ")
            ));
            if !o.powers.is_empty() {
                lines.push(format!("Powers: {}", powers(&o.powers)));
            }
            for (i, m) in o
                .monsters
                .iter()
                .enumerate()
                .filter(|(_, m)| m.alive || m.half_dead)
            {
                let intent = m
                    .intent
                    .as_ref()
                    .map(|p| match p.total_damage {
                        Some(d) => format!("{:?} {d}", p.visible_intent),
                        None => format!("{:?}", p.visible_intent),
                    })
                    .unwrap_or_else(|| "Hidden intent".into());
                lines.push(format!(
                    "M{i} {} {}/{}{} | {}{}",
                    m.name,
                    m.hp[0],
                    m.hp[1],
                    if m.block > 0 {
                        format!(" Block {}", m.block)
                    } else {
                        String::new()
                    },
                    intent,
                    if m.powers.is_empty() {
                        String::new()
                    } else {
                        format!(" | {}", powers(&m.powers))
                    }
                ));
            }
        } else {
            lines.push(format!(
                "Relics: {}",
                r.relics
                    .iter()
                    .map(|r| name(r.id))
                    .collect::<Vec<_>>()
                    .join(", ")
            ));
        }
        if let Some(stop) = self.stop() {
            lines.push(format!("Stopped: {stop:?}"));
            return Ok(lines.join("\n"));
        }
        let options = self.options()?;
        for (i, o) in options.iter().enumerate() {
            if matches!(
                o.action,
                NaturalAction::Run(ClientInput::DiscardPotion(_))
                    | NaturalAction::Combat(crate::CombatBranch::Input(
                        ClientInput::DiscardPotion(_)
                    ))
            ) {
                continue;
            }
            lines.push(format!("{i} {}", o.label));
        }
        match self.engine() {
            EngineState::RewardScreen(reward)
            | EngineState::RewardOverlay {
                reward_state: reward,
                ..
            } if reward.pending_card_choice.is_none() => {
                for (i, item) in reward.items.iter().enumerate() {
                    if !options
                        .iter()
                        .any(|o| o.action == NaturalAction::Run(ClientInput::ClaimReward(i)))
                    {
                        if let sts_core::state::rewards::RewardItem::Potion { potion_id } = item {
                            lines.push(format!("{}: potion slots full", name(*potion_id)));
                        }
                    }
                }
            }
            EngineState::Shop(s) => {
                for (i, c) in s.cards.iter().enumerate() {
                    if !options
                        .iter()
                        .any(|o| o.action == NaturalAction::Run(ClientInput::BuyCard(i)))
                    {
                        lines.push(format!(
                            "{}: {} gold (unavailable)",
                            name(c.card_id),
                            c.price
                        ));
                    }
                }
                for (i, r) in s.relics.iter().enumerate() {
                    if !options
                        .iter()
                        .any(|o| o.action == NaturalAction::Run(ClientInput::BuyRelic(i)))
                    {
                        lines.push(format!(
                            "{}: {} gold (unavailable)",
                            name(r.relic_id),
                            r.price
                        ));
                    }
                }
                for (i, p) in s.potions.iter().enumerate() {
                    if !options
                        .iter()
                        .any(|o| o.action == NaturalAction::Run(ClientInput::BuyPotion(i)))
                    {
                        lines.push(format!(
                            "{}: {} gold ({})",
                            name(p.potion_id),
                            p.price,
                            if r.find_empty_potion_slot().is_none() {
                                "potion slots full"
                            } else {
                                "unavailable"
                            }
                        ));
                    }
                }
            }
            _ => {}
        }
        Ok(lines.join("\n"))
    }
}
