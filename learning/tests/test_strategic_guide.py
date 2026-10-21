import json
from itertools import permutations
from unittest.mock import Mock

import pytest

from sts_learning.fixed_combat import FixedCombatConfig
from sts_learning.fixed_combat_environment import FixedCombatEnvironment, InstalledFixedCombatRuntime, canonical_json
from sts_learning.strategic_guide import (
    CompiledStrategicGuide, FIRST_ATTACK_GUIDE, GuidePreference,
    StrategicGuideUnresolved, resolve_preferences,
)
from sts_learning.strategic_rollout import StrategicRunController, run_rollout
from sts_learning.strategic_demonstrations import ScopedStrategicSampleRecorder, load_scoped_strategic_samples


def recipe(hp=72, *, offers=("Hemokinesis", "GhostlyArmor", "RecklessCharge"),
           floor=3, boss="Hexaghost", seed=2026101121):
    return dict(loadout=dict(seed=seed, ascension_level=20, current_hp=hp, max_hp=80,
        boss=boss, cards=[{"id": name} for name in
            ["AscendersBane", *["Strike"] * 5, *["Defend"] * 4, "Bash"]],
        relics=["BurningBlood"]), floor=floor, offers=[{"id": name} for name in offers])


@pytest.fixture
def runtime():
    pytest.importorskip("sts_learning_bridge")
    return InstalledFixedCombatRuntime(FixedCombatConfig(backend="whole_combat", atomic_nodes=None), final_act=True)


def environment(runtime, recipes):
    native = runtime.native.LearningBatchEnv.constructed_card_rewards(json.dumps(recipes))
    env = FixedCombatEnvironment(native, runtime.config, runtime.binding)
    env.baseline_manifest = runtime.baseline_manifest
    return env


def inspect(runtime, recipes):
    env = environment(runtime, recipes)
    batch = env.decision_batch(semantic=True)
    audits = [json.loads(env.strategic_decision_audit_json(slot)) for slot in batch["slot_indices"]]
    guide = CompiledStrategicGuide(runtime.schema)
    return guide, batch, audits, guide.evaluate(batch, audits)


def test_partial_order_retains_new_options_and_rejects_conflicts():
    edges = [GuidePreference(0, 1, "first"), GuidePreference(1, 2, "second")]
    assert resolve_preferences(3, edges).ordinal == 0
    result = resolve_preferences(4, edges)
    assert result.ordinal is None and result.contenders == (0, 3)
    # Even a separate apparently best option cannot hide a cycle.
    result = resolve_preferences(4, [*edges, GuidePreference(2, 0, "conflict")])
    assert result.ordinal is None and "conflicting_preferences" in result.issues


@pytest.mark.parametrize("hp,expected", [(72, "Hemokinesis"), (11, "GhostlyArmor")])
def test_relation_changes_with_hp_and_ignores_order_floor_boss_seed(runtime, hp, expected):
    recipes = [recipe(hp, floor=floor, boss=boss, seed=seed, offers=offers)
               for floor, boss, seed, offers in (
                   (1, "Hexaghost", 2026101121, ("Hemokinesis", "GhostlyArmor", "RecklessCharge")),
                   (3, "TheGuardian", 2026101122, ("RecklessCharge", "Hemokinesis", "GhostlyArmor")),
                   (4, "SlimeBoss", 2026101123, ("GhostlyArmor", "RecklessCharge", "Hemokinesis")))]
    _, _, audits, results = inspect(runtime, recipes)
    assert all(result.status == "resolved" for result in results)
    assert [audit["candidates"][result.ordinal]["card"] for audit, result in zip(audits, results)] == [expected] * 3


@pytest.mark.parametrize("card", ["Anger", "Carnage", "Cleave", "Hemokinesis", "PommelStrike", "TwinStrike"])
def test_first_attack_relations_use_native_numbers_and_real_upgrades(runtime, card):
    recipes = [recipe(offers=(card, "GhostlyArmor", "RecklessCharge")) for _ in range(2)]
    recipes[1]["offers"] = [dict(item, upgrades=1) for item in recipes[1]["offers"]]
    _, _, audits, results = inspect(runtime, recipes)
    assert [audit["candidates"][result.ordinal]["card"] for audit, result in zip(audits, results)] == [card, card]


@pytest.mark.parametrize("change,issue", [
    (lambda r: r["loadout"].update(current_hp=30), "intermediate_hp_tradeoff"),
    (lambda r: r["loadout"]["cards"].append({"id": "PommelStrike"}), "outside_starter_inventory"),
    (lambda r: r["loadout"]["relics"].append("Vajra"), "uncovered_relic_configuration"),
    (lambda r: r["loadout"]["cards"][1].update(base_damage_override=50), "outside_starter_inventory"),
    (lambda r: r["offers"].append({"id": "Offering"}), "uncovered_candidate:3"),
])
def test_uncovered_dependencies_remain_unknown(runtime, change, issue):
    value = recipe()
    change(value)
    _, batch, audits, (result,) = inspect(runtime, [value])
    assert result.status == "unknown" and result.ordinal is None and issue in result.issues
    assert len(audits[0]["candidates"]) == int(batch["candidate_counts"][0])
    if issue.startswith("uncovered_candidate"):
        assert 3 in result.contenders  # Offering was not filtered or defaulted low.
        assert result.preferences      # Covered pair preferences remain inspectable.


def test_residual_known_attacks_are_not_arbitrarily_ordered(runtime):
    _, _, _, (result,) = inspect(runtime, [recipe(offers=("Hemokinesis", "PommelStrike", "GhostlyArmor"))])
    assert result.ordinal is None and result.contenders == (0, 1)
    assert result.issues == ("residual_comparison",)


@pytest.mark.parametrize("offers,expected", [
    (("Clash", "WildStrike", "Hemokinesis"), "Hemokinesis"),
    (("GhostlyArmor", "IronWave", "BodySlam"), None),
])
def test_early_starter_defaults_are_menu_order_independent(runtime, offers, expected):
    recipes = [recipe(80, floor=1, offers=order) for order in permutations(offers)]
    if expected:
        for value in recipes:
            value["loadout"]["relics"].append("NeowsLament")
    _, _, audits, results = inspect(runtime, recipes)
    assert all(result.status == "resolved" for result in results)
    assert [audit["candidates"][result.ordinal].get("card")
            for audit, result in zip(audits, results)] == [expected] * len(recipes)
    assert all(len(audit["candidates"]) == 4 for audit in audits)
    assert all({rule.rule for rule in result.rules} == {
        "healthy_starter_first_attack", "healthy_starter_hemo_over_wound",
        "early_starter_preserve_access", "critical_hp_immediate_block"} for result in results)


def set_lament_lifecycle(runtime, batch, audit, *, counter, used_up):
    """Construct a paired public lifecycle input; this is not a native run."""
    relic = next(row for row in audit["public_relics"] if row["relic"] == "NeowsLament")
    relic.update(counter=counter, used_up=used_up)
    sem = batch["semantic"]
    cat = sem["categorical"]
    identity = ((cat["field"] == runtime.schema["categorical_field"]["RelicId"])
                & (cat["value"] == 96))
    token, = cat["token_indices"][identity]
    for table, field, value in ((cat, "RelicUsedUp", int(used_up)),
                               (sem["scalar"], "RelicCounter", counter)):
        schema_table = "categorical_field" if table is cat else "scalar_field"
        mask = ((table["token_indices"] == token) & (table["field"] == runtime.schema[schema_table][field]))
        assert mask.sum() == 1
        table["value"][mask] = value


@pytest.mark.parametrize("counter,used_up", [(3, False), (2, False), (1, False), (-2, True)])
def test_lament_buffer_changes_only_reviewed_rule_premises(runtime, counter, used_up):
    for hp in (72, 11):
        value = recipe(hp)
        value["loadout"]["relics"].append("NeowsLament")
        guide, batch, audits, _ = inspect(runtime, [value])
        set_lament_lifecycle(runtime, batch, audits[0], counter=counter, used_up=used_up)
        result, = guide.evaluate(batch, audits)
        critical_rule = next(rule for rule in result.rules if rule.rule == "critical_hp_immediate_block")
        if hp == 11 and not used_up:
            assert result.ordinal is None and not result.preferences
            assert "neows_lament_buffer_changes_urgency" in result.issues
            assert "neows_lament_buffer_changes_urgency" in critical_rule.blocked_by
        else:
            assert result.status == "resolved"
            assert audits[0]["candidates"][result.ordinal]["card"] == ("Hemokinesis" if hp == 72 else "GhostlyArmor")
        # Audit's relic collection order is not a semantic identity.
        audits[0]["public_relics"].reverse()
        assert guide.evaluate(batch, audits) == (result,)


@pytest.mark.parametrize("change,blocker", [
    (lambda r: r["loadout"].update(current_hp=59), "hp_below_three_quarters"),
    (lambda r: r.update(floor=5), "outside_early_preparation"),
    (lambda r: r["loadout"]["cards"].pop(-2), "basic_block_inventory_changed"),
])
def test_preservation_default_stops_when_its_specific_premise_changes(runtime, change, blocker):
    value = recipe(80, floor=1, offers=("GhostlyArmor", "IronWave", "BodySlam"))
    change(value)
    _, _, _, (result,) = inspect(runtime, [value])
    assert result.ordinal is None and result.contenders == (0, 1, 2, 3)
    rule = next(rule for rule in result.rules if rule.rule == "early_starter_preserve_access")
    assert blocker in rule.blocked_by and rule.preference_count == 0


@pytest.mark.parametrize("card", ["IronWave", "BodySlam", "GhostlyArmor"])
def test_upgraded_small_functions_are_not_silently_discarded(runtime, card):
    value = recipe(80, offers=("GhostlyArmor", "IronWave", "BodySlam"))
    next(row for row in value["offers"] if row["id"] == card)["upgrades"] = 1
    _, _, audits, (result,) = inspect(runtime, [value])
    assert result.ordinal is None
    assert next(i for i, row in enumerate(audits[0]["candidates"]) if row.get("card") == card) in result.contenders


def test_hemo_wound_relation_checks_actual_upgrade_damage(runtime):
    values = [recipe(offers=("Hemokinesis", "WildStrike", "Clash")) for _ in range(2)]
    for value in values:
        value["offers"][1]["upgrades"] = 1
    values[1]["offers"][0]["upgrades"] = 1
    _, _, _, (tradeoff, covered) = inspect(runtime, values)
    assert tradeoff.ordinal is None and tradeoff.contenders == (0, 1)
    assert covered.ordinal == 0


def test_new_rule_keeps_uncovered_option_and_mechanism_holes_visible(runtime):
    value = recipe(80, offers=("GhostlyArmor", "IronWave", "BodySlam", "Offering"))
    _, _, audits, (result,) = inspect(runtime, [value])
    assert result.ordinal is None and result.contenders == (3, 4)
    assert "uncovered_candidate:3" in result.issues and len(audits[0]["candidates"]) == 5
    guide, batch, audits, _ = inspect(runtime, [recipe(offers=("Clash", "WildStrike", "Hemokinesis"))])
    table = batch["semantic"]["categorical"]
    keep = table["field"] != runtime.schema["categorical_field"]["CardStatusCreationDestination"]
    for key in table:
        table[key] = table[key][keep]
    result, = guide.evaluate(batch, audits)
    assert result.ordinal is None and 1 in result.contenders
    assert "missing_or_uncovered_candidate_numbers:1" in result.issues


def test_paired_native_identity_and_lifecycle_are_required(runtime):
    guide, batch, audits, _ = inspect(runtime, [recipe()])
    audits[0]["candidates"][0]["card"] = "PommelStrike"
    with pytest.raises(ValueError, match="audit card differs"):
        guide.evaluate(batch, audits)
    value = recipe()
    value["loadout"]["relics"].append("NeowsLament")
    guide, batch, audits, _ = inspect(runtime, [value])
    lament = next(row for row in audits[0]["public_relics"] if row["relic"] == "NeowsLament")
    lament["counter"] = 2
    with pytest.raises(ValueError, match="audit relics differ"):
        guide.evaluate(batch, audits)
    del lament["counter"]
    result, = guide.evaluate(batch, audits)
    assert result.ordinal is None and "missing_neows_lament_lifecycle" in result.issues


def test_hp_defaults_have_an_explicit_unresolved_interval(runtime):
    _, _, _, results = inspect(runtime, [recipe(hp) for hp in (20, 21, 39, 40)])
    assert [result.ordinal for result in results] == [1, None, None, 0]


@pytest.mark.parametrize("change,issue", [
    ("inventory", "missing_inventory_numbers"),
    ("hp", "missing_act_or_hp"),
    ("act", "outside_act1_ironclad"),
    ("complete", "incomplete_public_facts"),
    ("nonfinite", "nonfinite_public_number"),
])
def test_missing_or_outside_public_facts_do_not_receive_defaults(runtime, change, issue):
    guide, batch, audits, _ = inspect(runtime, [recipe()])
    sem = batch["semantic"]
    if change == "inventory":
        del audits[0]["public_deck"][0]["base_damage_override"]
    elif change == "complete":
        sem["completeness"][0] = runtime.schema["completeness"]["NotEncoded"]
    else:
        table = sem["scalar"]
        field = "CurrentHp" if change == "hp" else "Act" if change == "act" else "CardDefinitionDamage"
        mask = table["field"] == runtime.schema["scalar_field"][field]
        if change == "hp":
            for key in table:
                table[key] = table[key][~mask]
        else:
            table["value"][mask] = 2 if change == "act" else float("nan")
    result, = guide.evaluate(batch, audits)
    assert result.ordinal is None and issue in result.issues


def test_missing_self_hp_request_and_joint_preparation_stop(runtime):
    guide, batch, audits, _ = inspect(runtime, [recipe()])
    candidate = int(batch["semantic"]["candidate_token_indices"][0])
    table = batch["semantic"]["scalar"]
    keep = ~((table["token_indices"] == candidate)
             & (table["field"] == runtime.schema["scalar_field"]["CardSelfHpLossOnPlay"]))
    for key in table:
        table[key] = table[key][keep]
    result, = guide.evaluate(batch, audits)
    assert result.ordinal is None and "missing_or_uncovered_candidate_numbers:0" in result.issues
    # An unseen second reward collection is not flattened into a single pick.
    _, batch, audits, _ = inspect(runtime, [recipe()])
    kinds = batch["semantic"]["token"]["kind"]
    pending = [i for i, kind in enumerate(kinds) if kind == runtime.schema["token_kind"]["PendingReward"]]
    assert pending
    # A pending relic can change preparation before this card choice.
    categories = batch["semantic"]["categorical"]
    mask = ((categories["token_indices"] == pending[0])
            & (categories["field"] == runtime.schema["categorical_field"]["RewardKind"]))
    categories["value"][mask] = runtime.schema["reward_kind"]["Relic"]
    result, = guide.evaluate(batch, audits)
    assert result.ordinal is None and "joint_reward_preparation_unresolved" in result.issues


def test_controller_adopts_with_guide_owner_and_restores_same_binding(runtime, tmp_path):
    env = environment(runtime, [recipe()])
    guide = CompiledStrategicGuide(runtime.schema)
    sink = ScopedStrategicSampleRecorder()
    controller = StrategicRunController(env, card_reward_guide=guide, decision_sink=sink)
    controller._choose()
    assert env.ready
    decision, = controller._pending
    assert decision["owner"] == "guide" and decision["guide"]["status"] == "resolved"
    assert decision["audit"]["candidates"][decision["ordinal"]]["card"] == "Hemokinesis"
    sink.save(tmp_path / "observed", schema=runtime.schema, producer=controller.manifest, terminal_outcome=None)
    _, samples = load_scoped_strategic_samples(tmp_path / "observed")
    assert samples[0][1]["label_source"] == "compiled_guide"
    assert samples[0][1]["expert_target"] is None and not samples[0][1]["committed"]
    restored = StrategicRunController.from_memory(env, controller.memory(), card_reward_guide=guide)
    assert canonical_json(restored._pending) == canonical_json(controller._pending)
    with pytest.raises(ValueError, match="binding mismatch"):
        StrategicRunController.from_memory(env, controller.memory())


def test_mixed_cohort_cannot_submit_supported_peer_when_another_is_unknown(runtime):
    env = environment(runtime, [recipe(), recipe(offers=("Offering", "GhostlyArmor", "RecklessCharge"))])
    before = env.checkpoint_bytes(max_bytes=64 * 1024 * 1024)
    env.choose = Mock(wraps=env.choose)
    controller = StrategicRunController(env, card_reward_guide=CompiledStrategicGuide(runtime.schema))
    with pytest.raises(StrategicGuideUnresolved) as error:
        controller._choose()
    env.choose.assert_not_called()
    assert env.checkpoint_bytes(max_bytes=64 * 1024 * 1024) == before
    assert len(error.value.prepared.payload["slot_indices"]) == 2
    assert len(error.value.decisions[0]["audit"]["candidates"]) == 4


def test_other_sites_keep_baseline_behavior_and_inputs(runtime):
    plain = runtime.environment([2026101129], 20)
    # Reuse the same explicit source boundary. Fresh pools carry distinct
    # decision identities even when their game seeds match.
    guided = runtime.environment_from_checkpoint(plain.checkpoint_bytes(max_bytes=64 * 1024 * 1024),
                                                 expected_slots=1, max_bytes=64 * 1024 * 1024)
    baseline = StrategicRunController(plain)
    controller = StrategicRunController(guided, card_reward_guide=CompiledStrategicGuide(runtime.schema))
    baseline._choose()
    controller._choose()
    assert controller._pending == baseline._pending
    assert plain.checkpoint_bytes(max_bytes=64 * 1024 * 1024) == guided.checkpoint_bytes(max_bytes=64 * 1024 * 1024)


def test_fresh_rollout_receipt_distinguishes_guide_and_preserves_unresolved_menu(tmp_path, runtime):
    result = run_rollout(tmp_path / "prefix", seed=2026101131, max_steps=12,
                         card_reward_guide=FIRST_ATTACK_GUIDE, capture_samples=True, progress=lambda _: None)
    assert result["stop"] in {"guide_unresolved", "combat_search_interruption", "step_limit"}
    assert result["committed_model_decisions"] == 0
    if result["stop"] == "guide_unresolved":
        receipt = json.loads((tmp_path / "prefix" / "guide-unresolved.json").read_text())
        assert receipt["adopted"] is False and receipt["decisions"]
        assert len(receipt["decisions"][0]["audit"]["candidates"]) >= 4
    assert (tmp_path / "prefix" / "endpoint.bin").is_file()
