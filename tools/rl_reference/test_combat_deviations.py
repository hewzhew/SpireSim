import pytest

from tools.rl_reference.combat_deviations import groups, selected_states


def reading(hp, hits, damage):
    return {"choices": [{"kind": "play", "hand": 0, "target": 0},
                        {"kind": "play", "hand": 1, "target": None}, {"kind": "end_turn"}],
            "combat": {"hand": [{"damage": 6, "damage_by_target": [6]}, {"block": 5}],
                       "monsters": [{"alive": True, "hp": [hp, 45],
                                     "intent": {"hits": hits, "total_damage": damage}}]}}


def test_nonattack_null_damage_and_unobserved_attack_are_distinct():
    assert not groups(reading(10, 0, None))["incoming_exceeds_block"]
    assert groups(reading(10, 1, 12))["incoming_exceeds_block"]
    with pytest.raises(ValueError, match="unobserved"):
        groups(reading(10, 1, None))


def test_mechanical_state_selection_is_deduplicated_and_block_aware():
    row = {"decisions": [{"reading": reading(45, 0, None)}, {"reading": reading(6, 1, 12)},
                         {"reading": reading(3, 1, 12)}]}
    chosen = list(selected_states(row))
    assert len(chosen) == 1 and chosen[0][0] == 1
    assert chosen[0][1] == ["attack_defense", "single_card_lethal"]
    row["decisions"][1]["reading"]["combat"]["monsters"][0]["block"] = 1
    assert [index for index, _, _ in selected_states(row)] == [1, 2]
