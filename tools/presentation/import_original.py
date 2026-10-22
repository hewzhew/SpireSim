"""Import entity names and raw card templates from the game, without Java.

Card/relic IDs come from the existing identity owners. Only localization-key
differences are recorded here; no fuzzy matching or translated-name lookup.
Tool explanations and UI messages remain with their own renderers/catalogues.
"""
from __future__ import annotations

import argparse
import hashlib
import json
from pathlib import Path
import sys
import zipfile

REPOSITORY = Path(__file__).resolve().parents[2]
if str(REPOSITORY) not in sys.path:
    sys.path.insert(0, str(REPOSITORY))

from tools.wiki.simulator_catalog import build_simulator_catalog, _stable_identity_owner

LANGUAGES = {"en": "eng", "zh-CN": "zhs"}
RESOURCES = {"card": "cards", "relic": "relics", "potion": "potions", "enemy": "monsters"}
# These are original localization keys, not display-name aliases or protocol IDs.
KEYS = {
    "card": {},
    "relic": {"HappyFlower": "Happy Flower"},
    "potion": {
        "FirePotion": "Fire Potion", "ExplosivePotion": "Explosive Potion",
        "PoisonPotion": "Poison Potion", "WeakenPotion": "Weak Potion",
        "BlockPotion": "Block Potion", "EnergyPotion": "Energy Potion",
        "StrengthPotion": "Strength Potion", "DexterityPotion": "Dexterity Potion",
        "SwiftPotion": "Swift Potion", "AncientPotion": "Ancient Potion",
        "RegenPotion": "Regen Potion", "DistilledChaosPotion": "DistilledChaos",
        "FruitJuice": "Fruit Juice", "Elixir": "ElixirPotion",
    },
    "enemy": {
        "LouseNormal": "FuzzyLouseNormal", "LouseDefensive": "FuzzyLouseDefensive",
        "SpikeSlimeS": "SpikeSlime_S", "SpikeSlimeM": "SpikeSlime_M", "SpikeSlimeL": "SpikeSlime_L",
        "AcidSlimeS": "AcidSlime_S", "AcidSlimeM": "AcidSlime_M", "AcidSlimeL": "AcidSlime_L",
        "ShelledParasite": "Shelled Parasite", "Taskmaster": "SlaverBoss",
        "BanditPointy": "BanditChild", "OrbWalker": "Orb Walker",
        "SpireGrowth": "Serpent", "SnakeDagger": "Dagger",
    },
}
OMITTED = {
    "card.Impulse": "No corresponding card localization entry in the installed release; retain stable-ID fallback.",
    "power.GuardianThreshold": "Simulator-owned Guardian threshold state; no original power localization identity.",
    "power.GenericStrengthUp": "Original Generic Strength Up Power has no fixed NAME; its constructor receives the encounter-specific name.",
    "stance.Neutral": "Original Neutral stance NAME is empty; retain stable-ID fallback.",
}
CHARACTERS = {"ironclad": "Ironclad", "silent": "Silent", "defect": "Defect", "watcher": "Watcher"}
STANCES = {"Calm": "Calm", "Wrath": "Wrath", "Divinity": "Divinity"}
# Explicit original power localization keys, audited against original class IDs.
# Regen is monster Regenerate, while Regeneration is the decaying RegenPower.
POWERS = {
    "AccuracyPower": "Accuracy",
    "AfterImage": "After Image",
    "Amplify": "Amplify",
    "Anger": "Anger",
    "Angry": "Angry",
    "Artifact": "Artifact",
    "BackAttack": "BackAttack",
    "Barricade": "Barricade",
    "BattleHymnPower": "BattleHymn",
    "BeatOfDeath": "BeatOfDeath",
    "Berserk": "Berserk",
    "Bias": "Bias",
    "BlockReturnPower": "BlockReturnPower",
    "Blur": "Blur",
    "Brutality": "Brutality",
    "Buffer": "Buffer",
    "Burst": "Burst",
    "CannotChangeStance": "CannotChangeStancePower",
    "Choked": "Choked",
    "CollectPower": "Collect",
    "Combust": "Combust",
    "Confusion": "Confusion",
    "Constricted": "Constricted",
    "CorpseExplosion": "CorpseExplosionPower",
    "Corruption": "Corruption",
    "CreativeAI": "Creative AI",
    "Curiosity": "Curiosity",
    "CurlUp": "Curl Up",
    "DarkEmbrace": "Dark Embrace",
    "DemonForm": "Demon Form",
    "DevaForm": "DevaForm",
    "DevotionPower": "DevotionPower",
    "Dexterity": "Dexterity",
    "DexterityDown": "DexLoss",
    "DoubleDamage": "Double Damage",
    "DoubleTap": "Double Tap",
    "Draw": "Draw",
    "DrawCardNextTurn": "Draw Card",
    "DrawReduction": "Draw Reduction",
    "DuplicationPower": "DuplicationPower",
    "EchoForm": "Echo Form",
    "Electro": "Electro",
    "EndTurnDeathPower": "EndTurnDeath",
    "Energized": "Energized",
    "EnergizedBlue": "EnergizedBlue",
    "EnergyDownPower": "EnergyDownPower",
    "Entangle": "Entangled",
    "Envenom": "Envenom",
    "Equilibrium": "Equilibrium",
    "EstablishmentPower": "EstablishmentPower",
    "Evolve": "Evolve",
    "Explosive": "Explosive",
    "Fading": "Fading",
    "FeelNoPain": "Feel No Pain",
    "FireBreathing": "Fire Breathing",
    "FlameBarrier": "Flame Barrier",
    "Flight": "Flight",
    "Focus": "Focus",
    "ForesightPower": "WireheadingPower",
    "Frail": "Frail",
    "FreeAttackPower": "FreeAttackPower",
    "Heatsink": "Heatsink",
    "Hello": "Hello",
    "Hex": "Hex",
    "InfiniteBladesPower": "Infinite Blades",
    "Intangible": "Intangible",
    "IntangiblePlayer": "IntangiblePlayer",
    "Invincible": "Invincible",
    "Juggernaut": "Juggernaut",
    "LikeWaterPower": "LikeWaterPower",
    "LockOn": "Lockon",
    "Loop": "Loop",
    "LoseStrength": "Flex",
    "MagnetismPower": "Magnetism",
    "Malleable": "Malleable",
    "Mantra": "Mantra",
    "MarkPower": "PathToVictoryPower",
    "MasterRealityPower": "MasterRealityPower",
    "MayhemPower": "Mayhem",
    "MentalFortressPower": "Controlled",
    "Metallicize": "Metallicize",
    "Minion": "Minion",
    "ModeShift": "Mode Shift",
    "NextTurnBlock": "Next Turn Block",
    "Nightmare": "Night Terror",
    "NirvanaPower": "Nirvana",
    "NoBlock": "NoBlockPower",
    "NoDraw": "No Draw",
    "NoSkills": "NoSkills",
    "NoxiousFumes": "Noxious Fumes",
    "OmegaPower": "OmegaPower",
    "PainfulStabs": "Painful Stabs",
    "PanachePower": "Panache",
    "PenNibPower": "Pen Nib",
    "Phantasmal": "Phantasmal",
    "PlatedArmor": "Plated Armor",
    "Poison": "Poison",
    "Rage": "Rage",
    "Reactive": "Compulsive",
    "Rebound": "Rebound",
    "Regen": "Regenerate",
    "Regeneration": "Regeneration",
    "Regrow": "Life Link",
    "Repair": "Repair",
    "RetainCards": "Retain Cards",
    "Ritual": "Ritual",
    "Rupture": "Rupture",
    "RushdownPower": "Adaptation",
    "SadisticPower": "Sadistic",
    "Shackled": "Shackled",
    "SharpHide": "Sharp Hide",
    "Shifting": "Shifting",
    "Slow": "Slow",
    "Split": "Split",
    "SporeCloud": "Spore Cloud",
    "Stasis": "Stasis",
    "StaticDischarge": "StaticDischarge",
    "Storm": "Storm",
    "Strength": "Strength",
    "Study": "Study",
    "Surrounded": "Surrounded",
    "TheBombPower": "TheBomb",
    "Thievery": "Thievery",
    "Thorns": "Thorns",
    "ThousandCuts": "Thousand Cuts",
    "TimeWarp": "Time Warp",
    "ToolsOfTheTrade": "Tools Of The Trade",
    "Unawakened": "Unawakened",
    "Vigor": "Vigor",
    "Vulnerable": "Vulnerable",
    "WaveOfTheHandPower": "WaveOfTheHandPower",
    "Weak": "Weakened",
    "WraithForm": "Wraith Form v2",
    "WrathNextTurn": "WrathNextTurnPower"
}
# Presentation aliases for the map's named encounters. A compound encounter
# has no original NAME of its own; its separator is our presentation message.
ENCOUNTER_ENEMIES = {"JawWorm": ["JawWorm"], "SlimeBoss": ["SlimeBoss"], "Hexaghost": ["Hexaghost"],
                     "TheGuardian": ["TheGuardian"], "TheChamp": ["Champ"], "Collector": ["TheCollector"],
                     "Automaton": ["BronzeAutomaton"], "AwakenedOne": ["AwakenedOne"], "TimeEater": ["TimeEater"],
                     "DonuAndDeca": ["Donu", "Deca"], "TheHeart": ["CorruptHeart"]}


def encoded(value: object) -> bytes:
    return (json.dumps(value, ensure_ascii=False, indent=2) + "\n").encode("utf-8")


def digest(value: bytes) -> str:
    return hashlib.sha256(value).hexdigest()


def names_from_archive(jar: Path, repository: Path = REPOSITORY) -> tuple[dict, dict, dict]:
    entities, identity_sources = build_simulator_catalog(repository)
    power_path = repository / "src/content/powers/mod.rs"
    power_ids = _stable_identity_owner(power_path.read_text(encoding="utf-8"), "PowerId")
    if not power_ids:
        raise ValueError("Power identities must come from the stable PowerId owner")
    expected_powers = set(power_ids.values()) - {key.removeprefix("power.") for key in OMITTED if key.startswith("power.")}
    if set(POWERS) != expected_powers:
        raise ValueError("Original power localization bindings do not cover the current stable PowerId identities")
    identity_sources = {**identity_sources, "src/content/powers/mod.rs": digest(power_path.read_bytes())}
    stance_path = repository / "src/runtime/combat/orbs.rs"
    stance_ids = _stable_identity_owner(stance_path.read_text(encoding="utf-8"), "StanceId")
    if not stance_ids or set(stance_ids.values()) != set(STANCES) | {"Neutral"}:
        raise ValueError("Original stance bindings do not cover the current stable StanceId identities")
    identity_sources["src/runtime/combat/orbs.rs"] = digest(stance_path.read_bytes())
    result, entries, bindings = {}, {}, {}
    with zipfile.ZipFile(jar) as archive:
        def source(language: str, resource: str) -> dict:
            path = f"localization/{language}/{resource}.json"
            raw = archive.read(path)
            entries[path] = digest(raw)
            return json.loads(raw.decode("utf-8-sig"), strict=False)

        for locale, language in LANGUAGES.items():
            names = {kind: {} for kind in RESOURCES}
            records = {kind: source(language, resource) for kind, resource in RESOURCES.items()}
            bindings[locale] = {}

            def take(kind: str, stable_id: str, key: str, record: dict, field: str = "NAME", index: int | None = None) -> str:
                value = record[key][field]
                if index is not None:
                    value = value[index]
                if not isinstance(value, str) or not value.strip():
                    raise ValueError(f"Empty original name: {locale}/{kind}/{stable_id}/{key}")
                resource = RESOURCES.get(kind) or {"character": "characters", "event": "characters", "power": "powers", "stance": "stances"}[kind]
                bindings[locale][f"{kind}.{stable_id}"] = {"entry": f"localization/{language}/{resource}.json",
                                                          "key": key, "field": field, "index": index}
                return value

            for kind in RESOURCES:
                current = [entity for entity in entities if entity.kind == kind]
                if not set(KEYS[kind]) <= {entity.stable_id for entity in current}:
                    raise ValueError(f"Localization key override has no current {kind} identity")
                for entity in current:
                    if f"{kind}.{entity.stable_id}" in OMITTED:
                        if entity.external_id in records[kind]:
                            raise ValueError(f"Original resource now supplies the omitted {kind}.{entity.stable_id}")
                        continue
                    key = KEYS[kind].get(entity.stable_id, entity.external_id if kind in ("card", "relic") else entity.stable_id)
                    if not key:
                        raise ValueError(f"Missing exact original key: {kind}.{entity.stable_id}")
                    names[kind][entity.stable_id] = take(kind, entity.stable_id, key, records[kind])
            characters = source(language, "characters")
            names["character"] = {stable: take("character", stable, key, characters, "NAMES", 0) for stable, key in CHARACTERS.items()}
            names["event"] = {"Neow": take("event", "Neow", "Neow Event", characters, "NAMES", 0)}
            powers = source(language, "powers")
            for stable, key in [("GuardianThreshold", "GuardianThreshold"), ("GenericStrengthUp", "Generic Strength Up Power")]:
                if powers.get(key, {}).get("NAME", "").strip():
                    raise ValueError(f"Original resource now supplies a fixed name for the omitted power.{stable}")
            names["power"] = {stable: take("power", stable, key, powers) for stable, key in POWERS.items()}
            stances = source(language, "stances")
            if stances.get("Neutral", {}).get("NAME", "").strip():
                raise ValueError("Original resource now supplies a name for the omitted stance.Neutral")
            names["stance"] = {stable: take("stance", stable, key, stances) for stable, key in STANCES.items()}
            names["encounter"] = {}
            for encounter, group in ENCOUNTER_ENEMIES.items():
                if len(group) == 1:
                    enemy = group[0]
                    names["encounter"][encounter] = names["enemy"][enemy]
                    bindings[locale][f"encounter.{encounter}"] = {"alias": f"enemy.{enemy}"}
                elif encounter == "DonuAndDeca" and group == ["Donu", "Deca"]:
                    separator = " and " if locale == "en" else "与"
                    names["encounter"][encounter] = separator.join(names["enemy"][part] for part in ("Donu", "Deca"))
                    bindings[locale][f"encounter.{encounter}"] = {"members": ["enemy.Donu", "enemy.Deca"], "separator": separator}
                else:
                    raise ValueError(f"Encounter label requires an explicit composition: {encounter}")
            result[locale] = {kind: dict(sorted(values.items())) for kind, values in names.items()}
    manifest = {"schema": "sts-original-localization-names-v1", "producer": "tools/presentation/import_original.py",
                "game": "Slay the Spire", "languages": LANGUAGES, "entries_sha256": dict(sorted(entries.items())),
                "identity_inputs_sha256": identity_sources,
                "bindings_sha256": {locale: digest(encoded(rows)) for locale, rows in bindings.items()},
                "name_counts": {locale: {kind: len(values) for kind, values in names.items()} for locale, names in result.items()},
                "generated_names_sha256": {locale: digest(encoded(names)) for locale, names in result.items()},
                "omitted": OMITTED, "composed_labels": ["encounter.DonuAndDeca"]}
    return result, manifest, bindings


def card_texts_from_archive(jar: Path, bindings: dict) -> dict:
    """Preserve original fields verbatim; card owners select their meaning."""
    cards, entries, card_bindings = {}, {}, {}
    with zipfile.ZipFile(jar) as archive:
        for locale, language in LANGUAGES.items():
            entry = f"localization/{language}/cards.json"
            raw = archive.read(entry)
            entries[entry] = digest(raw)
            source = json.loads(raw.decode("utf-8-sig"), strict=False)
            cards[locale], card_bindings[locale] = {}, {}
            for identity, binding in sorted(bindings[locale].items()):
                if not identity.startswith("card."):
                    continue
                if binding["entry"] != entry:
                    raise ValueError(f"Card template binding has another source: {identity}")
                row = source[binding["key"]]
                if not isinstance(row["DESCRIPTION"], str) or not row["DESCRIPTION"].strip():
                    raise ValueError(f"Missing original card description: {identity}")
                selected = {field: row[field] for field in ("DESCRIPTION", "UPGRADE_DESCRIPTION", "EXTENDED_DESCRIPTION") if field in row}
                if "UPGRADE_DESCRIPTION" in selected and not isinstance(selected["UPGRADE_DESCRIPTION"], str):
                    raise ValueError(f"Invalid original upgrade description: {identity}")
                extended = selected.get("EXTENDED_DESCRIPTION", [])
                if not isinstance(extended, list) or not all(isinstance(text, str) for text in extended):
                    raise ValueError(f"Invalid original extended description: {identity}")
                cards[locale][identity.removeprefix("card.")] = selected
                card_bindings[locale][identity] = binding
    return {"schema": "sts-original-card-text-v1", "entries_sha256": entries,
            "bindings_sha256": {locale: digest(encoded(rows)) for locale, rows in card_bindings.items()},
            "cards": cards}


def update(jar: Path, directory: Path, *, write: bool = False) -> dict:
    names, manifest, bindings = names_from_archive(jar)
    card_texts = card_texts_from_archive(jar, bindings)
    if any(manifest["entries_sha256"][entry] != sha for entry, sha in card_texts["entries_sha256"].items()):
        raise ValueError("Original resources changed during import")
    outputs, changed = {}, []
    for locale, sections in names.items():
        file = directory / "locales" / f"{locale}.json"
        original = file.read_bytes()
        catalog = json.loads(original)
        for kind, values in sections.items():
            catalog["game"][kind] = values
        # Keep the original layout if the generated names are already equal.
        outputs[file] = original if json.loads(original) == catalog else encoded(catalog)
    outputs[directory / "original-names.json"] = encoded(manifest)
    outputs[directory / "original-card-text.json"] = encoded(card_texts)
    for file, content in outputs.items():
        if not file.exists() or file.read_bytes() != content:
            changed.append(file.name)
    if write:
        for file, content in outputs.items():
            if not file.exists() or file.read_bytes() != content:
                file.write_bytes(content)
    return {"passed": write or not changed, "mode": "write" if write else "check", "changed": changed,
            "manifest": manifest, "bindings": bindings}


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--jar", required=True, type=Path, help="Installed desktop-1.0.jar; only its ZIP resources are read")
    parser.add_argument("--write", action="store_true", help="Update generated name sections and provenance; default verifies")
    parser.add_argument("--report", type=Path, help="Save complete source bindings and comparison result outside source files")
    args = parser.parse_args()
    result = update(args.jar, Path(__file__).resolve().parent, write=args.write)
    if args.report:
        with args.report.open("xb") as output:
            output.write(encoded(result))
    print(json.dumps({key: value for key, value in result.items() if key != "bindings"}, ensure_ascii=False))
    return 0 if result["passed"] else 1


if __name__ == "__main__":
    raise SystemExit(main())
