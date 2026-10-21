from __future__ import annotations

from pathlib import Path
import tempfile
import unittest

from tools.wiki.simulator_catalog import (
    ENCOUNTER_INITIAL_POSSIBLE_MEMBER,
    build_simulator_relations,
)


def _write(repository: Path, relative: str, body: str) -> None:
    path = repository / relative
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(body, encoding="utf-8")


def _fixture(repository: Path, encounter_body: str) -> None:
    _write(
        repository,
        "src/content/monsters/mod.rs",
        """
pub enum EnemyId {
    BronzeAutomaton,
    Spiker,
    Repulsor,
}
""",
    )
    _write(repository, "src/content/monsters/factory.rs", encounter_body)


class SimulatorCatalogTests(unittest.TestCase):
    def test_encounter_members_are_exhaustive_and_expand_enemy_helpers(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            repository = Path(temporary)
            _fixture(
                repository,
                """
pub enum EncounterId {
    Automaton,
    Shapes,
    ShapesEvent,
}
pub fn build_encounter(encounter: EncounterId) {
    match encounter {
        EncounterId::Automaton => {
            spawn_monster(EnemyId::BronzeAutomaton);
        }
        EncounterId::Shapes | EncounterId::ShapesEvent => {
            spawn_monster(get_shape());
        }
    }
}
fn get_shape() -> EnemyId {
    match choose() {
        0 => EnemyId::Spiker,
        _ => EnemyId::Repulsor,
    }
}
fn unused_identity(enemy: EnemyId) -> EnemyId {
    enemy
}
""",
            )

            relations = build_simulator_relations(repository)
            members: dict[str, set[str]] = {}
            for relation in relations:
                self.assertEqual(
                    relation.relation_type, ENCOUNTER_INITIAL_POSSIBLE_MEMBER
                )
                members.setdefault(relation.source_entity_key, set()).add(
                    relation.target_entity_key
                )
            self.assertEqual(
                members,
                {
                    "encounter:Automaton": {"enemy:BronzeAutomaton"},
                    "encounter:Shapes": {"enemy:Spiker", "enemy:Repulsor"},
                    "encounter:ShapesEvent": {"enemy:Spiker", "enemy:Repulsor"},
                },
            )

    def test_encounter_members_reject_missing_or_repeated_arms(self) -> None:
        cases = (
            ("", "not exhaustive"),
            (
                "EncounterId::Automaton => { spawn_monster(EnemyId::BronzeAutomaton); }",
                "repeats EncounterId",
            ),
        )
        for second_arm, message in cases:
            with self.subTest(message=message), tempfile.TemporaryDirectory() as temporary:
                repository = Path(temporary)
                _fixture(
                    repository,
                    f"""
pub enum EncounterId {{
    Automaton,
    Missing,
}}
pub fn build_encounter(encounter: EncounterId) {{
    match encounter {{
        EncounterId::Automaton => {{ spawn_monster(EnemyId::BronzeAutomaton); }}
        {second_arm}
    }}
}}
""",
                )

                with self.assertRaisesRegex(ValueError, message):
                    build_simulator_relations(repository)
