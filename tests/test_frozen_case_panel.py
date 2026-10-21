import importlib.util
import json
import tempfile
import unittest
from pathlib import Path


def load_frozen_case_panel():
    module_path = Path(__file__).resolve().parents[1] / "tools" / "frozen_case_panel.py"
    spec = importlib.util.spec_from_file_location("frozen_case_panel", module_path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def sample_review():
    return {
        "schema": "combat_case_review",
        "case_path": "fixtures/combat_cases/frozen_v0a_awakened_one_1552225675_a3f48.json",
        "source": {"seed": 1552225675},
        "frozen_panel_lanes": {
            "schema": "frozen_panel_lanes_v0a",
            "lanes": [
                {
                    "lane": "baseline",
                    "search_config_summary": {
                        "max_nodes": 800000,
                        "wall_ms": 8000,
                        "setup_bias_policy": "default",
                    },
                    "review": {
                        "complete_win": False,
                        "final_hp": None,
                        "turns": None,
                        "potions_used": None,
                        "nodes_expanded": 101,
                        "elapsed_ms": 44,
                        "deadline_hit": False,
                        "facts": {
                            "diagnostic_progress": {
                                "final_hp": 0,
                                "turns": 3,
                                "potions_used": 1,
                                "living_enemy_count": 1,
                                "total_enemy_hp": 0,
                                "half_dead_enemy_count": 1,
                                "action_key_preview": ["play Whirlwind+"],
                            }
                        },
                    },
                    "key_card_lifecycle": {
                        "tracked_cards": [
                            {
                                "card": "Demon Form+0",
                                "played_in_replay": False,
                                "first_play": None,
                            }
                        ]
                    },
                },
                {
                    "lane": "key_setup_bias",
                    "search_config_summary": {
                        "max_nodes": 800000,
                        "wall_ms": 8000,
                        "setup_bias_policy": "key_card_online",
                    },
                    "review": {
                        "complete_win": True,
                        "final_hp": 12,
                        "turns": 8,
                        "potions_used": 2,
                        "nodes_expanded": 202,
                        "elapsed_ms": 55,
                        "deadline_hit": False,
                        "facts": {
                            "diagnostic_progress": {
                                "final_hp": 12,
                                "turns": 8,
                                "potions_used": 2,
                                "living_enemy_count": 0,
                                "total_enemy_hp": 0,
                                "half_dead_enemy_count": 0,
                                "action_key_preview": ["play Demon Form"],
                            }
                        },
                    },
                    "key_card_lifecycle": {
                        "tracked_cards": [
                            {
                                "card": "Demon Form+0",
                                "played_in_replay": True,
                                "first_play": {"step_index": 3},
                            }
                        ]
                    },
                },
            ],
        },
    }


class FrozenCasePanelTests(unittest.TestCase):
    def test_extracts_one_row_per_frozen_lane(self):
        panel = load_frozen_case_panel()

        rows = panel.rows_from_review(sample_review(), reviewed_at_commit="abc123")

        self.assertEqual([row["lane"] for row in rows], ["baseline", "key_setup_bias"])
        self.assertEqual(rows[0]["case_origin_seed"], 1552225675)
        self.assertEqual(rows[0]["case_id"], "frozen_v0a_awakened_one_1552225675_a3f48")
        self.assertEqual(rows[0]["outcome_tier"], "phase_complete_but_player_died")
        self.assertEqual(rows[0]["first_action_key"], "play Whirlwind+")
        self.assertIsNone(rows[0]["first_action_role"])
        self.assertFalse(rows[0]["key_card_played"])
        self.assertIsNone(rows[0]["key_card_first_play_step"])
        self.assertEqual(rows[1]["outcome_tier"], "complete_win")
        self.assertTrue(rows[1]["key_card_played"])
        self.assertEqual(rows[1]["key_card_first_play_step"], 3)
        self.assertEqual(rows[1]["search_config_summary"]["setup_bias_policy"], "key_card_online")

    def test_classifies_incomplete_without_inventing_progress(self):
        panel = load_frozen_case_panel()
        review = sample_review()
        lane_review = review["frozen_panel_lanes"]["lanes"][0]["review"]
        lane_review["deadline_hit"] = False
        lane_review["facts"] = {"diagnostic_progress": None}

        rows = panel.rows_from_review(review, reviewed_at_commit="abc123")

        self.assertEqual(rows[0]["outcome_tier"], "incomplete_or_unknown")
        self.assertIsNone(rows[0]["final_hp"])
        self.assertEqual(rows[0]["tool_status"], "ok")

    def test_writes_jsonl_and_markdown_table(self):
        panel = load_frozen_case_panel()
        rows = panel.rows_from_review(sample_review(), reviewed_at_commit="abc123")

        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            panel.write_jsonl(root / "panel_rows.jsonl", rows)
            panel.write_markdown_table(root / "panel_table.md", rows)
            loaded = [
                json.loads(line)
                for line in (root / "panel_rows.jsonl").read_text().splitlines()
            ]
            table = (root / "panel_table.md").read_text()

        self.assertEqual(len(loaded), 2)
        self.assertIn(
            "| case_id | lane | outcome_tier | complete_win | key_card_played |",
            table,
        )
        self.assertIn("| frozen_v0a_awakened_one_1552225675_a3f48 | baseline |", table)


if __name__ == "__main__":
    unittest.main()
