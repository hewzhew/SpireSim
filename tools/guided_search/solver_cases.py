"""Declared A20 loadouts for full-information solver diagnostics, not natural runs.

No draw, enemy HP or RNG outcome is inspected when choosing these recipes.
Use separate seed sets for development and held-out measurements.
"""
import argparse
import copy
import json
from pathlib import Path


def cases(seeds):
    starter = [("Strike", 0)] * 5 + [("Defend", 0)] * 4 + [
        ("Bash", 0), ("AscendersBane", 0)
    ]
    early = starter + [("FlashOfSteel", 0), ("ShrugItOff", 0)]
    elite = starter + [("Bludgeon", 0), ("ShrugItOff", 0), ("Cleave", 0)]
    boss = [("Strike", 0)] * 3 + [("Defend", 0)] * 3 + [
        ("Bash", 1), ("AscendersBane", 0), ("Carnage", 1),
        ("PommelStrike", 1), ("ShrugItOff", 0), ("Inflame", 1),
        ("Disarm", 0), ("Impervious", 0), ("Headbutt", 0),
    ]
    late = [("Strike", 0)] * 2 + [("Defend", 0)] * 2 + [
        ("Bash", 1), ("AscendersBane", 0), ("DemonForm", 1),
        ("PommelStrike", 1), ("ShrugItOff", 1), ("ShrugItOff", 0),
        ("Impervious", 0), ("Disarm", 1), ("Reaper", 0),
        ("HeavyBlade", 1), ("LimitBreak", 1), ("Offering", 0),
        ("Armaments", 1), ("TrueGrit", 1),
    ]
    families = [
        ("JawWorm", 1, "MonsterRoom", early, [], ["FirePotion"]),
        ("Cultist", 1, "MonsterRoom", early, [], ["FirePotion"]),
        ("TwoLouse", 1, "MonsterRoom", early, [], ["FirePotion"]),
        ("GremlinNob", 1, "MonsterRoomElite", elite, [], ["FirePotion"]),
        ("Lagavulin", 1, "MonsterRoomElite", elite, [], ["FirePotion"]),
        ("ThreeSentries", 1, "MonsterRoomElite", elite, [], ["FirePotion"]),
        ("TheGuardian", 1, "MonsterRoomBoss", boss, ["Vajra"], ["FirePotion", "DexterityPotion"]),
        ("Hexaghost", 1, "MonsterRoomBoss", boss, ["Vajra"], ["FirePotion", "DexterityPotion"]),
        ("SlimeBoss", 1, "MonsterRoomBoss", boss, ["Vajra"], ["FirePotion", "DexterityPotion"]),
        ("TimeEater", 3, "MonsterRoomBoss", late, ["CoffeeDripper", "BagOfPreparation", "Vajra"], ["DuplicationPotion", "SwiftPotion"]),
        ("AwakenedOne", 3, "MonsterRoomBoss", late, ["CoffeeDripper", "BagOfPreparation", "Vajra"], ["DuplicationPotion", "SwiftPotion"]),
        ("TheHeart", 4, "MonsterRoomBoss", late, ["CoffeeDripper", "BagOfPreparation", "Vajra"], ["DuplicationPotion", "SwiftPotion"]),
    ]
    return [
        {"label": f"{enemy}/seed-{seed}", "source": "constructed", "spec": {
            "seed": seed, "encounter": enemy, "act": act, "room": room,
            "hp": 68 if act == 1 else 75, "deck": deck, "relics": relics,
            "potions": potions, "max_potion_uses": None,
        }}
        for seed in seeds
        for enemy, act, room, deck, relics, potions in families
    ]


def potion_cases(seeds):
    """Conditional inventory hypotheses, not an estimated potion distribution.

    Each encounter/seed group shares deck, relics, HP and initialization RNG.
    Only inventory changes. Reasons describe why to test a reserve; they are
    not action instructions, utility coefficients or natural acquisition claims.
    """
    early = [
        ("empty", [], "inventory_control", "No available consumable."),
        ("fire", ["FirePotion"], "early_immediate", "Damage may remove an enemy attack and free a slot early."),
        ("block", ["BlockPotion"], "early_immediate", "Defense may cover a dangerous attack without playing a skill."),
        ("strength", ["StrengthPotion"], "early_scaling", "Value depends on how many attacks can use the extra strength."),
    ]
    late = [
        ("empty", [], "inventory_control", "No available consumable."),
        ("power_duplication", ["PowerPotion", "DuplicationPotion"], "planned_reserve_hypothesis", "Try a generated persistent effect and a chosen card repetition."),
        ("memory_gambler", ["LiquidMemories", "GamblersBrew"], "planned_reserve_hypothesis", "Recover a key card and repair a poor hand at the needed turn."),
        ("duplication_swift", ["DuplicationPotion", "SwiftPotion"], "planned_reserve_hypothesis", "Extra draw and card repetition may support startup or survival."),
        ("power_fairy", ["PowerPotion", "FairyPotion"], "planned_reserve_hypothesis", "Try a persistent effect with a reserve against lethal damage."),
        ("steel_weak", ["EssenceOfSteel", "WeakenPotion"], "adverse_inventory_hypothesis", "A less suitable reserve remains possible; test its actual use without assuming failure."),
        ("fire_dexterity", ["FirePotion", "DexterityPotion"], "inventory_control", "Early consumables carried unusually late, without claiming this is normal management."),
    ]
    rows = []
    for base in cases(seeds):
        encounter = base["spec"]["encounter"]
        if encounter == "GremlinNob":
            variants = early
        elif encounter == "ThreeSentries":
            variants = [early[0], early[1], ("explosive", ["ExplosivePotion"], "early_immediate", "Area damage may remove multiple enemy attacks.")]
        elif encounter == "TheHeart":
            variants = late
        elif encounter in ("TimeEater", "AwakenedOne"):
            variants = late[:3]
        else:
            continue
        for variant, potions, role, reason in variants:
            row = copy.deepcopy(base)
            row["label"] += f"/{variant}"
            row["spec"]["potions"] = potions
            row["inventory_context"] = {"group": base["label"], "role": role, "reason": reason}
            rows.append(row)
    return rows


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--seeds", type=int, nargs="+", required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--suite", choices=("coverage", "potions"), default="coverage")
    args = parser.parse_args()
    args.output.parent.mkdir(parents=True, exist_ok=True)
    build = potion_cases if args.suite == "potions" else cases
    args.output.write_text(json.dumps(build(args.seeds), indent=2) + "\n", encoding="utf-8")
