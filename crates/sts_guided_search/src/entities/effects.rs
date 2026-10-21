//! Typed planned effects. Never conflate these rows with already active powers.
use super::{EntityPosition, WIDTH};
use sts_core::runtime::monster_move::*;

pub(super) fn encode(p: &mut EntityPosition, steps: &[MoveStep], owner: i64) {
    for (order, step) in steps.iter().enumerate() {
        let mut v = [0.; WIDTH];
        v[0] = order as f32 / 8.;
        let mut target = None;
        let (kind, category) = match step {
            MoveStep::Attack(s) => {
                target = Some(s.target);
                v[4] = s.attack.base_damage as f32 / 30.;
                v[5] = s.attack.hits as f32 / 5.;
                v[6 + s.attack.damage_kind as usize] = 1.;
                (18, 0)
            }
            MoveStep::ApplyPower(s) => {
                target = Some(s.target);
                v[4] = s.amount as f32 / 10.;
                v[5] = 1.; // apply, versus remove
                v[6 + s.effect as usize] = 1.;
                v[8 + s.visible_strength as usize] = 1.;
                (19, s.power_id.stable_ordinal() as i64)
            }
            MoveStep::RemovePower(s) => {
                target = Some(s.target);
                v[10] = 1.;
                (19, s.power_id.stable_ordinal() as i64)
            }
            MoveStep::Heal(s) => {
                target = Some(s.target);
                v[4] = s.amount as f32 / 30.;
                (18, 1)
            }
            MoveStep::GainBlock(s) => {
                target = Some(s.target);
                v[4] = s.amount as f32 / 30.;
                (18, 2)
            }
            MoveStep::GainBlockRandomMonster(s) => {
                v[4] = s.amount as f32 / 30.;
                (18, 3)
            }
            MoveStep::AddCard(s) => {
                target = Some(MoveTarget::Player);
                v[4] = s.amount as f32 / 5.;
                v[5] = f32::from(s.upgraded);
                v[6 + s.destination as usize] = 1.;
                v[10] = 1.; // add versus upgrade existing cards
                (20, s.card_id.stable_ordinal() as i64)
            }
            MoveStep::UpgradeCards(s) => {
                target = Some(MoveTarget::Player);
                v[11] = 1.;
                (20, s.card_id.stable_ordinal() as i64)
            }
            MoveStep::StealGold(s) => {
                target = Some(MoveTarget::Player);
                v[4] = s.amount as f32 / 100.;
                (18, 4)
            }
            MoveStep::Utility(UtilityStep::RemoveAllDebuffs { target: t }) => {
                target = Some(*t);
                (18, 5)
            }
            MoveStep::SpawnMonster(s) => {
                v[4] = s.logical_position_offset as f32 / 5.;
                v[5] = f32::from(s.is_minion);
                for (offset, hp) in [(6, s.hp.current), (11, s.hp.max)] {
                    match hp {
                        SpawnHpValue::Rolled => v[offset] = 1.,
                        SpawnHpValue::Fixed(n) => {
                            v[offset + 1] = 1.;
                            v[offset + 4] = n as f32 / 100.;
                        }
                        SpawnHpValue::SourceCurrentHp => v[offset + 2] = 1.,
                        SpawnHpValue::SourceMaxHp => v[offset + 3] = 1.,
                    }
                }
                (21, s.enemy_id.stable_ordinal() as i64)
            }
            MoveStep::Suicide => (18, 6),
            MoveStep::Charge => (18, 7),
            MoveStep::Escape => (18, 8),
            MoveStep::Magic => (18, 9),
            MoveStep::Sleep => (18, 10),
            MoveStep::Stun => (18, 11),
            MoveStep::Debug => (18, 12),
            MoveStep::None => (18, 13),
        };
        let target_ref = match target {
            Some(MoveTarget::Player) => {
                v[1] = 1.;
                0
            }
            Some(MoveTarget::SelfTarget) => {
                v[2] = 1.;
                owner
            }
            Some(MoveTarget::AllMonsters) => {
                v[3] = 1.;
                -1
            }
            None => -1,
        };
        let row = p.push(kind, category, v, [owner, target_ref, -1]);
        match step {
            MoveStep::ApplyPower(s) => super::powers::encode(p, s.power_id, row, target_ref),
            MoveStep::RemovePower(s) => super::powers::encode(p, s.power_id, row, target_ref),
            _ => {}
        }
    }
}
