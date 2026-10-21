//! Bounded, constructed mechanism contrasts. All edges use the shared executor.
//! This diagnostic exports evidence, not a gameplay solver or training labels.
use std::path::Path;
use std::time::{Duration, Instant};

use serde::Serialize;
use serde_json::json;
use sts_core::content::cards::CardId;
use sts_core::content::monsters::factory::EncounterId;
use sts_core::content::powers::{store, PowerId};
use sts_core::runtime::combat::{Power, PowerPayload};
use sts_core::sim::combat::{CombatPosition, CombatStepLimits, CombatStepper, EngineCombatStepper};
use sts_core::sim::combat_projection::project_monster_move_preview_in_combat;
use sts_core::state::core::ClientInput;
use sts_core::state::map::node::RoomType;
use sts_guided_search::entities::{self, EntityPosition};
use sts_guided_search::local::ConstructedCombat;
use sts_guided_search::observation::{
    combat_observation, model_choices, CombatObservation, ModelChoice,
};
use sts_guided_search::{
    CombatBranch, CombatSearchSpace, CombatSearchState, GuidanceBudget, SearchBudget, SearchConfig,
    SearchSession, SearchSpace, Transition,
};

#[derive(Clone, Copy, Debug, Serialize)]
enum Mechanism {
    NobBlock,
    NobDraw,
    TimeWarp,
}

#[derive(Clone, Debug, Serialize)]
struct Recipe {
    mechanism: Mechanism,
    condition: bool,
    seed: u64,
    distractor: bool,
    fragile: bool,
}

fn limits(deadline: Option<Instant>) -> CombatStepLimits {
    CombatStepLimits {
        max_engine_steps: 256,
        deadline,
    }
}

fn end_turn(root: &CombatPosition) -> Result<CombatPosition, String> {
    let next = EngineCombatStepper.apply_to_stable(root, ClientInput::EndTurn, limits(None));
    if next.truncated || next.timed_out {
        return Err("preparation did not settle".into());
    }
    Ok(next.position)
}

fn set_power(root: &mut CombatPosition, owner: usize, id: PowerId, amount: i32) {
    let mut powers = store::powers_snapshot_for(&root.combat, owner);
    powers.retain(|p| p.power_type != id);
    powers.push(Power {
        power_type: id,
        amount,
        extra_data: 0,
        payload: PowerPayload::None,
        just_applied: false,
        instance_id: None,
    });
    store::set_powers_for(&mut root.combat, owner, powers);
}

fn construct(recipe: &Recipe) -> Result<CombatPosition, String> {
    let (encounter, act, room, probe) = match recipe.mechanism {
        Mechanism::NobBlock => (
            EncounterId::GremlinNob,
            1,
            RoomType::MonsterRoomElite,
            CardId::Entrench,
        ),
        Mechanism::NobDraw => (
            EncounterId::GremlinNob,
            1,
            RoomType::MonsterRoomElite,
            CardId::BattleTrance,
        ),
        Mechanism::TimeWarp => (
            EncounterId::TimeEater,
            3,
            RoomType::MonsterRoomBoss,
            CardId::Strike,
        ),
    };
    let mut deck = vec![(probe, 0), (CardId::HeavyBlade, 0)];
    if recipe.distractor {
        deck.push((CardId::Defend, 0));
    }
    let mut root = ConstructedCombat {
        seed: recipe.seed,
        encounter,
        act,
        room,
        hp: 75,
        deck,
        relics: vec![],
        potions: vec![],
        max_potion_uses: Some(0),
    }
    .build()?;
    match recipe.mechanism {
        Mechanism::NobBlock | Mechanism::NobDraw => {
            root = end_turn(&root)?;
        }
        Mechanism::TimeWarp => {
            // Retain the engine's first actual attack plan; do not invent intent
            // damage or reject seeds whose initial move was not an attack.
            for _ in 0..3 {
                let m = &root.combat.entities.monsters[0];
                if project_monster_move_preview_in_combat(&root.combat, m)
                    .total_damage
                    .is_some()
                {
                    break;
                }
                root = end_turn(&root)?;
            }
        }
    }
    let damage =
        project_monster_move_preview_in_combat(&root.combat, &root.combat.entities.monsters[0])
            .total_damage
            .ok_or("preparation lacks attack")?;
    // Explicit state interventions after preparation. Existing typed card
    // instances are redistributed, with Heavy Blade next to draw; no IDs are guessed.
    let mut cards: Vec<_> = root.combat.zones.hand.drain(..).collect();
    cards.extend(std::mem::take(&mut root.combat.zones.draw_pile).into_vec());
    cards.extend(std::mem::take(&mut root.combat.zones.discard_pile).into_vec());
    if !root.combat.zones.exhaust_pile.is_empty() || !root.combat.zones.limbo.is_empty() {
        return Err("unexpected preparation card zones".into());
    }
    let index = cards
        .iter()
        .position(|c| c.id == probe)
        .ok_or("missing probe card")?;
    root.combat.zones.hand.push(cards.remove(index));
    cards.sort_by_key(|c| usize::from(c.id != CardId::HeavyBlade));
    root.combat.zones.draw_pile = cards.into();
    root.combat.turn.energy = 3;
    root.combat.entities.player.current_hp = if recipe.fragile { damage } else { damage + 1 };
    root.combat.entities.player.block = 0;
    match recipe.mechanism {
        Mechanism::NobBlock => {
            root.combat.entities.monsters[0].current_hp = 10;
            root.combat.entities.player.block = if recipe.condition { 5 } else { 0 };
            if recipe.fragile {
                root.combat.entities.player.current_hp = 2;
            }
        }
        Mechanism::NobDraw => {
            root.combat.entities.monsters[0].current_hp = 10;
            if recipe.condition {
                set_power(&mut root, 0, PowerId::NoDraw, -1);
            }
        }
        Mechanism::TimeWarp => {
            let owner = root.combat.entities.monsters[0].id;
            // Ripple may have left block. Explicitly remove it for this lethal
            // versus nonlethal damage contrast; retain actual player debuffs.
            root.combat.entities.monsters[0].block = 0;
            set_power(&mut root, owner, PowerId::TimeWarp, 11);
            let preview = observe(CombatSearchState::new(root.clone())?)?.1.state;
            let strike_damage = preview.hand[0].damage_by_target[0];
            root.combat.entities.monsters[0].current_hp = if recipe.condition {
                strike_damage
            } else {
                strike_damage + 1
            };
        }
    }
    CombatSearchState::new(root.clone())?;
    Ok(root)
}

struct ObservedChoices {
    state: CombatObservation,
    choices: Vec<ModelChoice>,
}

fn observe(state: CombatSearchState) -> Result<(EntityPosition, ObservedChoices), String> {
    let session = SearchSession::new(
        CombatSearchSpace::with_potion_limit(0),
        state,
        SearchConfig::default(),
        SearchBudget {
            max_services: 1,
            max_engine_steps: 256,
            max_nodes: 2,
            max_depth: 1,
            deadline: None,
            guidance: GuidanceBudget::default(),
        },
    )
    .map_err(|e| format!("{e:?}"))?;
    let numeric = entities::encode(&session, session.root())?;
    let view = session.view(session.root()).map_err(|e| format!("{e:?}"))?;
    Ok((
        numeric,
        ObservedChoices {
            state: combat_observation(view.state)?,
            choices: model_choices(view.state, &view.actions)?,
        },
    ))
}

#[derive(Serialize)]
struct Node {
    kind: &'static str,
    hp: i32,
    #[serde(skip_serializing_if = "Option::is_none")]
    input: Option<EntityPosition>,
    choices: Vec<ModelChoice>,
    inputs: Vec<ClientInput>,
    edges: Vec<usize>,
}

impl Node {
    fn leaf(kind: &'static str, hp: i32) -> Self {
        Self {
            kind,
            hp,
            input: None,
            choices: vec![],
            inputs: vec![],
            edges: vec![],
        }
    }
}

struct Expansion {
    root: CombatPosition,
    nodes: Vec<Node>,
    max_nodes: usize,
    max_depth: usize,
    cutoff: Option<usize>,
    deadline: Instant,
    engine_steps: usize,
    replay_steps: usize,
    terminals_replayed: usize,
}

impl Expansion {
    fn visit(
        &mut self,
        state: CombatSearchState,
        path: &mut Vec<ClientInput>,
    ) -> Result<usize, String> {
        if self.nodes.len() >= self.max_nodes || Instant::now() >= self.deadline {
            if let Some(id) = self.cutoff {
                return Ok(id);
            }
            let id = self.nodes.len();
            self.nodes.push(Node::leaf("unknown_budget", 0));
            self.cutoff = Some(id);
            return Ok(id);
        }
        let id = self.nodes.len();
        self.nodes.push(Node::leaf("pending", 0));
        let space = CombatSearchSpace::with_potion_limit(0);
        if let Some(won) = space.terminal(&state) {
            let mut replay = self.root.clone();
            for input in path.iter() {
                let next = EngineCombatStepper.apply_to_stable(
                    &replay,
                    input.clone(),
                    limits(Some(self.deadline)),
                );
                if next.truncated || next.timed_out {
                    return Err("terminal replay interrupted".into());
                }
                self.replay_steps += next.engine_steps;
                replay = next.position;
            }
            if &replay != state.position() {
                return Err("terminal replay differs".into());
            }
            self.terminals_replayed += 1;
            self.nodes[id] = Node::leaf(
                if won { "win" } else { "loss" },
                state.position().combat.entities.player.current_hp,
            );
            return Ok(id);
        }
        if path.len() >= self.max_depth {
            self.nodes[id] = Node::leaf("unknown_depth", 0);
            return Ok(id);
        }
        let (input, observation) = observe(state.clone())?;
        let choices = observation.choices;
        let actions = space.choices(&state)?;
        if choices.is_empty()
            || choices.len() != actions.len()
            || input.actions.len() != choices.len()
        {
            return Err("incomplete diagnostic action domain".into());
        }
        let mut edges = vec![];
        let mut inputs = vec![];
        for action in actions {
            let CombatBranch::Input(actual) = &action.action else {
                return Err("mechanism diagnostic requires atomic choices".into());
            };
            inputs.push(actual.clone());
            match space.transition(&state, &action.action, limits(Some(self.deadline)))? {
                Transition::Settled {
                    state: next,
                    engine_steps,
                } => {
                    self.engine_steps += engine_steps;
                    path.push(actual.clone());
                    edges.push(self.visit(next, path)?);
                    path.pop();
                }
                Transition::Interrupted { .. } => {
                    return Err("diagnostic transition interrupted".into())
                }
            }
        }
        self.nodes[id] = Node {
            kind: "decision",
            hp: state.position().combat.entities.player.current_hp,
            input: Some(input),
            choices,
            inputs,
            edges,
        };
        Ok(id)
    }
}

fn panel_case(
    recipe: &Recipe,
    max_nodes: usize,
    max_depth: usize,
) -> Result<serde_json::Value, String> {
    let root = construct(recipe)?;
    let state = CombatSearchState::new(root.clone())?;
    let observation = observe(state.clone())?.1.state;
    let mut expansion = Expansion {
        root: root.clone(),
        nodes: vec![],
        max_nodes,
        max_depth,
        cutoff: None,
        deadline: Instant::now() + Duration::from_secs(5),
        engine_steps: 0,
        replay_steps: 0,
        terminals_replayed: 0,
    };
    expansion.visit(state, &mut vec![])?;
    let closed = expansion
        .nodes
        .iter()
        .all(|n| matches!(n.kind, "decision" | "win" | "loss"));
    Ok(
        json!({"recipe":recipe, "root":root, "observation":observation, "closed":closed,
        "nodes":expansion.nodes, "engine_steps":expansion.engine_steps,
        "replay_steps":expansion.replay_steps, "terminals_replayed":expansion.terminals_replayed}),
    )
}

fn write(path: &Path, data: &serde_json::Value) -> Result<(), Box<dyn std::error::Error>> {
    let file = std::fs::File::options()
        .write(true)
        .create_new(true)
        .open(path)?;
    serde_json::to_writer(file, data)?;
    Ok(())
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let directory = std::env::args()
        .nth(1)
        .ok_or("expected fresh output directory")?;
    let directory = Path::new(&directory);
    std::fs::create_dir(directory)?;
    let recipes: Vec<_> = [Mechanism::NobBlock, Mechanism::NobDraw, Mechanism::TimeWarp]
        .into_iter()
        .flat_map(|mechanism| {
            [211, 223, 227].into_iter().flat_map(move |seed| {
                [false, true].into_iter().flat_map(move |distractor| {
                    [false, true].into_iter().flat_map(move |fragile| {
                        [false, true].into_iter().map(move |condition| Recipe {
                            mechanism,
                            seed,
                            distractor,
                            condition,
                            fragile,
                        })
                    })
                })
            })
        })
        .collect();
    write(
        &directory.join("plan.json"),
        &json!({"schema":entities::SCHEMA, "recipes":recipes,
        "scope":"constructed mechanism diagnosis; no training labels or historical roots",
        "interventions":"fresh encounter and actual preparation, then explicit player/enemy HP, card zones, energy, player/enemy block, and NoDraw/TimeWarp state",
        "conditions":{"NobBlock":"block 0/5; fragile HP 2 or incoming damage + 1", "NobDraw":"NoDraw absent/present; HP incoming damage or incoming damage + 1", "TimeWarp":"counter 11, block 0, enemy HP actual Strike damage + 1 / actual Strike damage; HP incoming damage or incoming damage + 1"},
        "scope_groups":{"NobBlock":"familiar training content; constructed combination", "NobDraw":"untrained Battle Trance/NoDraw combination", "TimeWarp":"enemy not used for training"},
        "comparison":"closed full continuations only; terminal win first, winning HP second; report policy preference and complete-continuation probabilities separately",
        "max_nodes_per_case":2048, "max_depth":16, "seconds_per_case":5}),
    )?;
    let mut closed_cases = 0;
    let mut total_nodes = 0;
    for (i, recipe) in recipes.iter().enumerate() {
        let case = panel_case(recipe, 2048, 16).map_err(std::io::Error::other)?;
        write(&directory.join(format!("case-{i:02}.json")), &case)?;
        closed_cases += usize::from(case["closed"] == true);
        total_nodes += case["nodes"].as_array().unwrap().len();
    }
    println!(
        "mechanism panel | {} cases | {closed_cases} closed | {total_nodes} nodes",
        recipes.len()
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn can_win(nodes: &[serde_json::Value], id: usize) -> Option<bool> {
        match nodes[id]["kind"].as_str().unwrap() {
            "win" => Some(true),
            "loss" => Some(false),
            "decision" => {
                let results: Option<Vec<_>> = nodes[id]["edges"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|edge| can_win(nodes, edge.as_u64().unwrap() as usize))
                    .collect();
                results.map(|r| r.into_iter().any(|win| win))
            }
            _ => None,
        }
    }

    #[test]
    fn complete_continuations_require_both_play_and_stop_and_retain_controls() {
        for mechanism in [Mechanism::NobBlock, Mechanism::NobDraw, Mechanism::TimeWarp] {
            for distractor in [false, true] {
                for fragile in [false, true] {
                    for condition in [false, true] {
                        let recipe = Recipe {
                            mechanism,
                            condition,
                            seed: 211,
                            distractor,
                            fragile,
                        };
                        let case = panel_case(&recipe, 2048, 16).unwrap();
                        assert_eq!(case["closed"], true, "{recipe:?}");
                        let nodes = case["nodes"].as_array().unwrap();
                        assert_eq!(nodes[0]["inputs"][0], "EndTurn");
                        assert!(nodes[0]["inputs"][1].get("PlayCard").is_some());
                        let actual: Vec<_> = nodes[0]["edges"]
                            .as_array()
                            .unwrap()
                            .iter()
                            .map(|e| can_win(nodes, e.as_u64().unwrap() as usize).unwrap())
                            .collect();
                        let useful = match mechanism {
                            Mechanism::NobDraw => !condition,
                            _ => condition,
                        };
                        assert_eq!(actual, vec![!fragile, useful], "{recipe:?}");
                        assert_eq!(
                            case["terminals_replayed"].as_u64().unwrap() as usize,
                            nodes
                                .iter()
                                .filter(|n| n["kind"] == "win" || n["kind"] == "loss")
                                .count()
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn budget_cut_is_unknown_and_root_is_unchanged() {
        let recipe = Recipe {
            mechanism: Mechanism::NobDraw,
            condition: false,
            seed: 211,
            distractor: false,
            fragile: false,
        };
        let case = panel_case(&recipe, 1, 16).unwrap();
        assert_eq!(case["closed"], false);
        assert_eq!(case["nodes"][1]["kind"], "unknown_budget");
        assert_eq!(
            case["root"],
            serde_json::to_value(construct(&recipe).unwrap()).unwrap()
        );
    }
}
