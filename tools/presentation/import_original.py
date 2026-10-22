"""Import entity names from an installed game's localization, without Java.

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

from tools.wiki.simulator_catalog import build_simulator_catalog

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
OMITTED = {"card.Impulse": "No corresponding card localization entry in the installed release; retain stable-ID fallback."}
CHARACTERS = {"ironclad": "Ironclad", "silent": "Silent", "defect": "Defect", "watcher": "Watcher"}
POWERS = {"Vulnerable": "Vulnerable", "Weak": "Weakened", "Frail": "Frail"}
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
                resource = RESOURCES.get(kind) or {"character": "characters", "event": "characters", "power": "powers"}[kind]
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
            names["power"] = {stable: take("power", stable, key, powers) for stable, key in POWERS.items()}
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


def update(jar: Path, directory: Path, *, write: bool = False) -> dict:
    names, manifest, bindings = names_from_archive(jar)
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
