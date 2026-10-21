"""Strict no-build adapter for stable simulator entity identities."""

from __future__ import annotations

from dataclasses import dataclass
import hashlib
import json
from pathlib import Path
import re
from typing import Any, Iterable


SIMULATOR_CATALOG_SCHEMA = "sts-simulator-entity-catalog-v5"
RELIC_IDENTITY_CONTRACT_SOURCE = "src/content/relics/identity_v1.json"
EVENT_ALIAS_SOURCE = "src/state/events/identity.rs"
ENCOUNTER_FACTORY_SOURCE = "src/content/monsters/factory.rs"
ENCOUNTER_INITIAL_POSSIBLE_MEMBER = "encounter_initial_possible_member"


@dataclass(frozen=True)
class SimulatorEntity:
    entity_key: str
    kind: str
    stable_id: str
    display_name: str | None
    external_id: str | None
    external_aliases: tuple[str, ...]
    source_path: str
    source_line: int


@dataclass(frozen=True)
class SimulatorRelation:
    relation_type: str
    source_entity_key: str
    target_entity_key: str
    source_path: str
    source_line: int


SimulatorEntityIdentity = tuple[
    str,
    str,
    str | None,
    str | None,
    tuple[str, ...],
]


def simulator_entity_identity_manifest(
    entities: Iterable[SimulatorEntity],
) -> dict[str, SimulatorEntityIdentity]:
    """Return only identity-bearing fields, excluding source layout/provenance."""
    return {
        entity.entity_key: (
            entity.kind,
            entity.stable_id,
            entity.display_name,
            entity.external_id,
            entity.external_aliases,
        )
        for entity in entities
    }


def file_sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as input_file:
        for chunk in iter(lambda: input_file.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def _line_number(text: str, offset: int) -> int:
    return text.count("\n", 0, offset) + 1


def _rust_block(text: str, marker: str) -> tuple[str, int]:
    marker_offset = text.find(marker)
    if marker_offset < 0:
        raise ValueError(f"Rust owner marker was not found: {marker}")
    opening = text.find("{", marker_offset + len(marker))
    if opening < 0:
        raise ValueError(f"Rust owner marker has no block: {marker}")
    depth = 0
    in_string = False
    escaped = False
    line_comment = False
    block_comment = 0
    index = opening
    while index < len(text):
        char = text[index]
        following = text[index + 1] if index + 1 < len(text) else ""
        if line_comment:
            if char == "\n":
                line_comment = False
            index += 1
            continue
        if block_comment:
            if char == "/" and following == "*":
                block_comment += 1
                index += 2
            elif char == "*" and following == "/":
                block_comment -= 1
                index += 2
            else:
                index += 1
            continue
        if in_string:
            if escaped:
                escaped = False
            elif char == "\\":
                escaped = True
            elif char == '"':
                in_string = False
            index += 1
            continue
        if char == "/" and following == "/":
            line_comment = True
            index += 2
            continue
        if char == "/" and following == "*":
            block_comment = 1
            index += 2
            continue
        if char == '"':
            in_string = True
        elif char == "{":
            depth += 1
        elif char == "}":
            depth -= 1
            if depth == 0:
                return text[opening + 1 : index], opening + 1
        index += 1
    raise ValueError(f"Rust owner block is not balanced: {marker}")


def _rust_code_projection(text: str) -> str:
    """Blank comments and strings while preserving offsets and Rust punctuation."""
    projected = list(text)
    in_string = False
    escaped = False
    line_comment = False
    block_comment = 0
    index = 0
    while index < len(text):
        char = text[index]
        following = text[index + 1] if index + 1 < len(text) else ""
        if line_comment:
            if char == "\n":
                line_comment = False
            else:
                projected[index] = " "
            index += 1
            continue
        if block_comment:
            projected[index] = " "
            if char == "/" and following == "*":
                projected[index + 1] = " "
                block_comment += 1
                index += 2
            elif char == "*" and following == "/":
                projected[index + 1] = " "
                block_comment -= 1
                index += 2
            else:
                index += 1
            continue
        if in_string:
            projected[index] = " "
            if escaped:
                escaped = False
            elif char == "\\":
                escaped = True
            elif char == '"':
                in_string = False
            index += 1
            continue
        if char == "/" and following == "/":
            projected[index] = " "
            projected[index + 1] = " "
            line_comment = True
            index += 2
            continue
        if char == "/" and following == "*":
            projected[index] = " "
            projected[index + 1] = " "
            block_comment = 1
            index += 2
            continue
        if char == '"':
            projected[index] = " "
            in_string = True
        index += 1
    if in_string or block_comment:
        raise ValueError("Rust source has an unterminated string or block comment")
    return "".join(projected)


def _matching_brace(code: str, opening: int, owner: str) -> int:
    depth = 0
    for index in range(opening, len(code)):
        if code[index] == "{":
            depth += 1
        elif code[index] == "}":
            depth -= 1
            if depth == 0:
                return index
    raise ValueError(f"Rust owner block is not balanced: {owner}")


def _enum_variants(text: str, enum_name: str) -> dict[str, int]:
    body, body_offset = _rust_block(text, f"pub enum {enum_name}")
    variants: dict[str, int] = {}
    offset = body_offset
    for line in body.splitlines(keepends=True):
        code = line.split("//", 1)[0].strip()
        if code:
            match = re.fullmatch(
                r"([A-Za-z][A-Za-z0-9_]*)(?:\s*=>\s*\(\s*\"[^\"]+\"\s*,\s*\d+\s*\))?\s*,",
                code,
            )
            if not match:
                raise ValueError(
                    f"unsupported {enum_name} variant syntax at line {_line_number(text, offset)}: {code}"
                )
            variants[match.group(1)] = _line_number(text, offset)
        offset += len(line)
    if not variants:
        raise ValueError(f"Rust enum contains no variants: {enum_name}")
    return variants


def _stable_identity_owner(text: str, enum_name: str) -> dict[str, str] | None:
    """Read the explicit stable-gameplay-ID macro entries when present.

    Small test fixtures and identity kinds not yet owned by the shared macro may
    still use ordinary enum syntax, in which case the caller keeps its existing
    owner. A partially converted enum is rejected instead of silently deriving
    the remaining stable IDs from Rust symbols.
    """

    body, body_offset = _rust_block(text, f"pub enum {enum_name}")
    entries: dict[str, str] = {}
    ordinals: dict[int, str] = {}
    ordinary_variants: list[str] = []
    offset = body_offset
    for line in body.splitlines(keepends=True):
        code = line.split("//", 1)[0].strip()
        if not code:
            offset += len(line)
            continue
        explicit = re.fullmatch(
            r"([A-Za-z][A-Za-z0-9_]*)\s*=>\s*\(\s*\"([^\"]+)\"\s*,\s*(\d+)\s*\)\s*,",
            code,
        )
        if explicit:
            variant, stable_id, ordinal_raw = explicit.groups()
            ordinal = int(ordinal_raw)
            if stable_id in entries.values():
                raise ValueError(f"{enum_name} repeats stable ID: {stable_id}")
            if ordinal in ordinals:
                raise ValueError(
                    f"{enum_name} stable ordinal {ordinal} is shared by "
                    f"{ordinals[ordinal]} and {variant}"
                )
            entries[variant] = stable_id
            ordinals[ordinal] = variant
        else:
            ordinary = re.fullmatch(r"([A-Za-z][A-Za-z0-9_]*)\s*,", code)
            if not ordinary:
                raise ValueError(
                    f"unsupported {enum_name} stable identity syntax at line "
                    f"{_line_number(text, offset)}: {code}"
                )
            ordinary_variants.append(ordinary.group(1))
        offset += len(line)

    if entries and ordinary_variants:
        raise ValueError(
            f"{enum_name} mixes explicit stable identities with runtime-only variants: "
            f"{ordinary_variants}"
        )
    if not entries:
        return None
    expected_ordinals = set(range(len(entries)))
    if set(ordinals) != expected_ordinals:
        raise ValueError(
            f"{enum_name} stable ordinals are not contiguous 0..{len(entries) - 1}"
        )
    return entries


def _match_string_arms(text: str, marker: str, type_name: str) -> dict[str, str]:
    body, _ = _rust_block(text, marker)
    return dict(
        re.findall(
            rf"{re.escape(type_name)}::([A-Za-z][A-Za-z0-9_]*)\s*=>\s*\"([^\"]+)\"",
            body,
        )
    )


def _require_exhaustive(owner: str, variants: dict[str, int], mapping: dict[str, Any]) -> None:
    missing = sorted(set(variants) - set(mapping))
    extra = sorted(set(mapping) - set(variants))
    if missing or extra:
        raise ValueError(f"{owner} mapping is not exhaustive; missing={missing}, extra={extra}")


def _relic_identity_contract(
    repository: Path,
) -> tuple[dict[str, str], dict[str, str], dict[str, tuple[str, ...]]]:
    path = repository / RELIC_IDENTITY_CONTRACT_SOURCE
    try:
        root = json.loads(path.read_text(encoding="utf-8"))
        schema_name = root["schema_name"]
        schema_version = root["schema_version"]
        entries = root["entries"]
    except (KeyError, TypeError, json.JSONDecodeError) as error:
        raise ValueError(f"relic identity contract is invalid: {error}") from error
    if schema_name != "RelicIdentityContract" or schema_version != 1:
        raise ValueError(
            f"unsupported relic identity contract: {schema_name!r} v{schema_version!r}"
        )
    if not isinstance(entries, list) or not entries:
        raise ValueError("relic identity contract entries must be a non-empty array")

    stable_ids: dict[str, str] = {}
    external_ids: dict[str, str] = {}
    external_aliases: dict[str, tuple[str, ...]] = {}
    stable_owners: dict[str, str] = {}
    external_owners: dict[str, str] = {}
    normalized_external_owners: dict[str, str] = {}
    for index, entry in enumerate(entries):
        if not isinstance(entry, dict) or set(entry) != {
            "stable_id",
            "runtime_symbol",
            "external_id",
            "external_aliases",
        }:
            raise ValueError(f"relic identity entry {index} has an invalid shape")
        stable_id = entry["stable_id"]
        runtime_symbol = entry["runtime_symbol"]
        external_id = entry["external_id"]
        aliases = entry["external_aliases"]
        if not all(
            isinstance(value, str) and value
            for value in (stable_id, runtime_symbol, external_id)
        ) or not isinstance(aliases, list) or not all(
            isinstance(alias, str) and alias for alias in aliases
        ):
            raise ValueError(f"relic identity entry {index} contains an invalid value")
        if (
            stable_id.strip() != stable_id
            or ":" in stable_id
            or any(ord(character) < 32 or ord(character) == 127 for character in stable_id)
        ):
            raise ValueError(
                f"relic stable ID must be a trimmed entity-key component: {stable_id!r}"
            )
        if not re.fullmatch(r"[A-Za-z][A-Za-z0-9_]*", runtime_symbol):
            raise ValueError(
                f"relic runtime symbol is not a Rust identifier: {runtime_symbol!r}"
            )
        if runtime_symbol in stable_ids:
            raise ValueError(f"relic identity repeats runtime symbol: {runtime_symbol}")
        if stable_id in stable_owners:
            raise ValueError(
                f"relic stable ID {stable_id!r} is shared by "
                f"{stable_owners[stable_id]} and {runtime_symbol}"
            )
        stable_owners[stable_id] = runtime_symbol
        stable_ids[runtime_symbol] = stable_id
        external_ids[runtime_symbol] = external_id
        external_aliases[runtime_symbol] = tuple(aliases)
        for external in (external_id, *aliases):
            if external in external_owners:
                raise ValueError(
                    f"external relic ID {external!r} is shared by "
                    f"{external_owners[external]} and {runtime_symbol}"
                )
            external_owners[external] = runtime_symbol
            normalized = "".join(
                character.lower()
                for character in external
                if character.isascii() and character.isalnum()
            )
            if not normalized:
                raise ValueError(
                    f"external relic ID {external!r} has no alphanumeric identity"
                )
            normalized_owner = normalized_external_owners.get(normalized)
            if normalized_owner is not None and normalized_owner != runtime_symbol:
                raise ValueError(
                    f"normalized external relic ID {normalized!r} is shared by "
                    f"{normalized_owner} and {runtime_symbol}"
                )
            normalized_external_owners[normalized] = runtime_symbol
    return stable_ids, external_ids, external_aliases


def _event_identity_owner(
    repository: Path,
) -> tuple[dict[str, str], dict[str, tuple[str, ...]], set[str]]:
    event_path = repository / "src/state/events/mod.rs"
    event_text = event_path.read_text(encoding="utf-8")
    variants = _enum_variants(event_text, "EventId")
    retired_match = re.search(
        r"pub const RETIRED_EVENT_IDS:\s*&\[EventId\]\s*=\s*&\[(.*?)\];",
        event_text,
        flags=re.DOTALL,
    )
    if retired_match is None:
        raise ValueError("EventId retired identity owner was not found")
    retired_body = retired_match.group(1)
    retired_pattern = re.compile(r"EventId::([A-Za-z][A-Za-z0-9_]*)\s*,?")
    retired = retired_pattern.findall(retired_body)
    if retired_pattern.sub("", retired_body).strip(" \t\r\n,"):
        raise ValueError("EventId retired identity owner has unsupported syntax")
    if len(retired) != len(set(retired)):
        raise ValueError("EventId retired identity owner repeats a variant")
    unknown_retired = sorted(set(retired) - set(variants))
    if unknown_retired:
        raise ValueError(f"EventId retired identity owner has unknown variants: {unknown_retired}")

    alias_text = (repository / EVENT_ALIAS_SOURCE).read_text(encoding="utf-8")
    alias_match = re.search(
        r"pub const LEGACY_EVENT_NAME_ALIASES:\s*&\[\(&str,\s*EventId\)\]\s*=\s*&\[(.*?)\];",
        alias_text,
        flags=re.DOTALL,
    )
    if alias_match is None:
        raise ValueError("EventId legacy name alias owner was not found")
    alias_body = alias_match.group(1)
    pair_pattern = re.compile(
        r'\(\s*"([^"\\]+)"\s*,\s*EventId::([A-Za-z][A-Za-z0-9_]*)\s*,?\s*\)\s*,?',
        flags=re.DOTALL,
    )
    pairs = pair_pattern.findall(alias_body)
    if pair_pattern.sub("", alias_body).strip():
        raise ValueError("EventId live alias owner has unsupported syntax")
    aliases: dict[str, list[str]] = {}
    alias_owners: dict[str, str] = {}
    for alias, variant in pairs:
        if variant not in variants:
            raise ValueError(f"EventId live alias has unknown variant: {alias!r} -> {variant}")
        if variant in retired:
            raise ValueError(f"EventId live alias points at retired variant: {alias!r} -> {variant}")
        prior = alias_owners.get(alias)
        if prior is not None:
            raise ValueError(
                f"EventId live alias is repeated: {alias!r} -> {prior}, {variant}"
            )
        alias_owners[alias] = variant
        aliases.setdefault(variant, []).append(alias)

    active_variants = {
        variant: line for variant, line in variants.items() if variant not in retired
    }
    _require_exhaustive("EventId live aliases", active_variants, aliases)

    display_match = re.search(
        r"pub const fn event_display_name_v1\s*\(\s*event_id:\s*EventId\s*\)\s*->\s*Option<&'static str>\s*\{\s*Some\(match event_id\s*\{(.*?)\}\)\s*\}",
        alias_text,
        flags=re.DOTALL,
    )
    if display_match is None:
        raise ValueError("EventId display-name owner was not found")
    display_body = display_match.group(1)
    display_arm_pattern = re.compile(
        r'EventId::([A-Za-z][A-Za-z0-9_]*)\s*=>\s*"([^"\\]+)"\s*,?'
    )
    absent_arm_pattern = re.compile(
        r"EventId::([A-Za-z][A-Za-z0-9_]*)\s*=>\s*return\s+None\s*,?"
    )
    display_names = dict(display_arm_pattern.findall(display_body))
    absent_display = absent_arm_pattern.findall(display_body)
    unparsed_display = display_arm_pattern.sub("", display_body)
    unparsed_display = absent_arm_pattern.sub("", unparsed_display)
    if unparsed_display.strip():
        raise ValueError("EventId display-name owner has unsupported syntax")
    if len(display_names) + len(absent_display) != len(variants):
        raise ValueError("EventId display-name owner repeats or omits a variant")
    _require_exhaustive("EventId display name", active_variants, display_names)
    if set(absent_display) != set(retired):
        raise ValueError("EventId display-name absences must exactly match retired variants")
    return (
        display_names,
        {variant: tuple(values) for variant, values in aliases.items()},
        set(retired),
    )


def _enemy_id_return_helpers(text: str) -> dict[str, str]:
    code = _rust_code_projection(text)
    signature = re.compile(
        r"\bfn\s+([a-z][A-Za-z0-9_]*)\s*\([^)]*\)\s*->\s*EnemyId\s*\{",
        flags=re.DOTALL,
    )
    helpers: dict[str, str] = {}
    for match in signature.finditer(code):
        opening = match.end() - 1
        closing = _matching_brace(code, opening, match.group(1))
        helpers[match.group(1)] = code[opening + 1 : closing]
    return helpers


def _enemy_id_helper_members(
    helper: str,
    helpers: dict[str, str],
    stack: tuple[str, ...] = (),
) -> set[str]:
    if helper in stack:
        raise ValueError(f"EnemyId-returning helper cycle is unsupported: {stack + (helper,)}")
    body = helpers[helper]
    members = set(
        re.findall(r"\bEnemyId::([A-Za-z][A-Za-z0-9_]*)", body)
    )
    for nested in helpers:
        if nested != helper and re.search(rf"\b{re.escape(nested)}\s*\(", body):
            members.update(
                _enemy_id_helper_members(nested, helpers, stack + (helper,))
            )
    if not members:
        raise ValueError(
            f"used EnemyId-returning helper has no explicit possible values: {helper}"
        )
    return members


def _encounter_member_arms(text: str) -> dict[str, tuple[set[str], int]]:
    body, body_offset = _rust_block(text, "match encounter")
    code = _rust_code_projection(body)
    arm_pattern = re.compile(
        r"^[ \t]*(EncounterId::[A-Za-z][A-Za-z0-9_]*"
        r"(?:[ \t\r\n]*\|[ \t\r\n]*EncounterId::[A-Za-z][A-Za-z0-9_]*)*)"
        r"[ \t\r\n]*=>[ \t\r\n]*\{",
        flags=re.MULTILINE,
    )
    helpers = _enemy_id_return_helpers(text)
    arms: dict[str, tuple[set[str], int]] = {}
    for match in arm_pattern.finditer(code):
        opening = match.end() - 1
        if code[:opening].count("{") != code[:opening].count("}"):
            continue
        closing = _matching_brace(code, opening, "match encounter arm")
        arm_code = code[opening + 1 : closing]
        encounters = re.findall(
            r"EncounterId::([A-Za-z][A-Za-z0-9_]*)", match.group(1)
        )
        members = set(
            re.findall(r"\bEnemyId::([A-Za-z][A-Za-z0-9_]*)", arm_code)
        )
        for helper in helpers:
            if re.search(rf"\b{re.escape(helper)}\s*\(", arm_code):
                members.update(_enemy_id_helper_members(helper, helpers))
        source_line = _line_number(text, body_offset + match.start())
        for encounter in encounters:
            if encounter in arms:
                raise ValueError(
                    f"build_encounter repeats EncounterId match arm: {encounter}"
                )
            arms[encounter] = (set(members), source_line)
    return arms


def build_simulator_relations(repository: Path) -> list[SimulatorRelation]:
    repository = repository.resolve()
    factory_path = repository / ENCOUNTER_FACTORY_SOURCE
    factory_text = factory_path.read_text(encoding="utf-8")
    encounters = _enum_variants(factory_text, "EncounterId")
    enemy_text = (repository / "src/content/monsters/mod.rs").read_text(encoding="utf-8")
    enemies = _enum_variants(enemy_text, "EnemyId")
    arms = _encounter_member_arms(factory_text)
    _require_exhaustive("build_encounter arms", encounters, arms)

    relations: list[SimulatorRelation] = []
    for encounter in encounters:
        members, source_line = arms[encounter]
        if not members:
            raise ValueError(
                f"build_encounter arm has no initial possible members: {encounter}"
            )
        unknown = sorted(members - set(enemies))
        if unknown:
            raise ValueError(
                f"build_encounter arm has unknown EnemyId members: {encounter}: {unknown}"
            )
        for enemy in enemies:
            if enemy in members:
                relations.append(
                    SimulatorRelation(
                        relation_type=ENCOUNTER_INITIAL_POSSIBLE_MEMBER,
                        source_entity_key=f"encounter:{encounter}",
                        target_entity_key=f"enemy:{enemy}",
                        source_path=ENCOUNTER_FACTORY_SOURCE,
                        source_line=source_line,
                    )
                )
    return relations


def _source_entities(
    *,
    repository: Path,
    relative_path: str,
    enum_name: str,
    kind: str,
    stable_ids: dict[str, str] | None = None,
    display_names: dict[str, str] | None = None,
    external_ids: dict[str, str] | None = None,
    external_aliases: dict[str, tuple[str, ...]] | None = None,
    excluded_variants: set[str] | None = None,
) -> list[SimulatorEntity]:
    path = repository / relative_path
    text = path.read_text(encoding="utf-8")
    variants = _enum_variants(text, enum_name)
    excluded = excluded_variants or set()
    unknown_excluded = sorted(excluded - set(variants))
    if unknown_excluded:
        raise ValueError(f"{enum_name} exclusions are unknown variants: {unknown_excluded}")
    variants = {variant: line for variant, line in variants.items() if variant not in excluded}
    if stable_ids is not None:
        _require_exhaustive(f"{enum_name} stable ID", variants, stable_ids)
    if display_names is not None:
        _require_exhaustive(f"{enum_name} display name", variants, display_names)
    if external_ids is not None:
        _require_exhaustive(f"{enum_name} external id", variants, external_ids)
    if external_aliases is not None:
        _require_exhaustive(f"{enum_name} external aliases", variants, external_aliases)
    return [
        SimulatorEntity(
            entity_key=f"{kind}:{stable_ids.get(variant, variant) if stable_ids else variant}",
            kind=kind,
            stable_id=stable_ids.get(variant, variant) if stable_ids else variant,
            display_name=display_names.get(variant) if display_names else None,
            external_id=external_ids.get(variant) if external_ids else None,
            external_aliases=external_aliases.get(variant, ()) if external_aliases else (),
            source_path=relative_path.replace("\\", "/"),
            source_line=line,
        )
        for variant, line in variants.items()
    ]


def build_simulator_catalog(repository: Path) -> tuple[list[SimulatorEntity], dict[str, str]]:
    repository = repository.resolve()
    cards_text = (repository / "src/content/cards/mod.rs").read_text(encoding="utf-8")
    card_ids = _match_string_arms(cards_text, "pub fn java_id", "CardId")
    card_stable_ids = _stable_identity_owner(cards_text, "CardId")

    potions_text = (repository / "src/content/potions/mod.rs").read_text(encoding="utf-8")
    potion_stable_ids = _stable_identity_owner(potions_text, "PotionId")
    potion_body, _ = _rust_block(potions_text, "pub fn get_potion_definition")
    potion_names = dict(
        re.findall(
            r"PotionId::([A-Za-z][A-Za-z0-9_]*)\s*=>\s*PotionDefinition\s*\{.*?\bname:\s*\"([^\"]+)\"",
            potion_body,
            flags=re.DOTALL,
        )
    )

    monsters_text = (repository / "src/content/monsters/mod.rs").read_text(encoding="utf-8")
    enemy_stable_ids = _stable_identity_owner(monsters_text, "EnemyId")
    enemy_names = _match_string_arms(monsters_text, "pub fn get_name", "EnemyId")
    relic_stable_ids, relic_external_ids, relic_external_aliases = _relic_identity_contract(
        repository
    )
    event_display_names, event_aliases, retired_event_ids = _event_identity_owner(repository)
    encounter_text = (repository / ENCOUNTER_FACTORY_SOURCE).read_text(encoding="utf-8")
    encounter_stable_ids = _stable_identity_owner(encounter_text, "EncounterId")
    events_text = (repository / "src/state/events/mod.rs").read_text(encoding="utf-8")
    event_stable_ids = _stable_identity_owner(events_text, "EventId")
    live_event_stable_ids = (
        {
            variant: stable_id
            for variant, stable_id in event_stable_ids.items()
            if variant not in retired_event_ids
        }
        if event_stable_ids is not None
        else None
    )

    entities: list[SimulatorEntity] = []
    entities.extend(
        _source_entities(
            repository=repository,
            relative_path="src/content/cards/mod.rs",
            enum_name="CardId",
            kind="card",
            stable_ids=card_stable_ids,
            external_ids=card_ids,
        )
    )
    entities.extend(
        _source_entities(
            repository=repository,
            relative_path="src/content/potions/mod.rs",
            enum_name="PotionId",
            kind="potion",
            stable_ids=potion_stable_ids,
            display_names=potion_names,
        )
    )
    entities.extend(
        _source_entities(
            repository=repository,
            relative_path="src/content/relics/mod.rs",
            enum_name="RelicId",
            kind="relic",
            stable_ids=relic_stable_ids,
            external_ids=relic_external_ids,
            external_aliases=relic_external_aliases,
        )
    )
    entities.extend(
        _source_entities(
            repository=repository,
            relative_path="src/content/monsters/mod.rs",
            enum_name="EnemyId",
            kind="enemy",
            stable_ids=enemy_stable_ids,
            display_names=enemy_names,
        )
    )
    entities.extend(
        _source_entities(
            repository=repository,
            relative_path="src/content/monsters/factory.rs",
            enum_name="EncounterId",
            kind="encounter",
            stable_ids=encounter_stable_ids,
        )
    )
    entities.extend(
        _source_entities(
            repository=repository,
            relative_path="src/state/events/mod.rs",
            enum_name="EventId",
            kind="event",
            stable_ids=live_event_stable_ids,
            display_names=event_display_names,
            external_aliases=event_aliases,
            excluded_variants=retired_event_ids,
        )
    )
    keys = [entity.entity_key for entity in entities]
    if len(keys) != len(set(keys)):
        raise ValueError("simulator entity catalog contains duplicate keys")
    source_paths = {entity.source_path for entity in entities}
    source_paths.add(RELIC_IDENTITY_CONTRACT_SOURCE)
    source_paths.add(EVENT_ALIAS_SOURCE)
    source_hashes = {path: file_sha256(repository / path) for path in source_paths}
    return entities, dict(sorted(source_hashes.items()))


def simulator_catalog_summary(repository: Path) -> dict[str, Any]:
    entities, source_hashes = build_simulator_catalog(repository)
    relations = build_simulator_relations(repository)
    kinds: dict[str, dict[str, int]] = {}
    for entity in entities:
        item = kinds.setdefault(
            entity.kind,
            {
                "count": 0,
                "with_display_name": 0,
                "with_external_id": 0,
                "with_external_aliases": 0,
            },
        )
        item["count"] += 1
        item["with_display_name"] += int(entity.display_name is not None)
        item["with_external_id"] += int(entity.external_id is not None)
        item["with_external_aliases"] += int(bool(entity.external_aliases))
    return {
        "schema": SIMULATOR_CATALOG_SCHEMA,
        "entity_count": len(entities),
        "kinds": kinds,
        "relation_count": len(relations),
        "relations": {
            ENCOUNTER_INITIAL_POSSIBLE_MEMBER: {
                "count": len(relations),
                "source_entity_count": len(
                    {relation.source_entity_key for relation in relations}
                ),
                "target_entity_count": len(
                    {relation.target_entity_key for relation in relations}
                ),
            }
        },
        "source_hashes": source_hashes,
    }
