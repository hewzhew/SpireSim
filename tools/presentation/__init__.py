"""Read-only Python consumer of the same catalogs used by Rust and Node."""

from dataclasses import dataclass
from functools import cache
from importlib.resources import files
import json
import re


@cache
def _catalog(locale: str) -> dict:
    return json.loads(files(__package__).joinpath("locales", locale + ".json").read_text(encoding="utf-8"))


@dataclass(frozen=True)
class Presentation:
    locale: str = "zh-CN"

    def __post_init__(self):
        if self.locale not in {"en", "zh-CN"}:
            raise ValueError(f"unsupported text locale: {self.locale}; use en or zh-CN")

    def entry(self, namespace: str, key: str) -> str | None:
        for locale in dict.fromkeys((self.locale, "en")):
            value = _catalog(locale).get(namespace)
            for part in key.split("."):
                value = value.get(part) if isinstance(value, dict) else None
            if isinstance(value, str):
                return value
        return None

    def entity(self, kind: str, identity: str) -> str:
        label = self.entry("game", f"{kind}.{identity}")
        return identity if label is None else label

    def card_name(self, identity: str, upgrades: int = 0) -> str:
        name = self.entity("card", identity)
        if upgrades == 0:
            return name
        return name + ("+" if upgrades == 1 and identity != "SearingBlow" else f"+{upgrades}")

    def text(self, namespace: str, key: str, **values) -> str:
        return self._format(namespace, key, values, definitions=False)

    def definition_text(self, namespace: str, key: str, **values) -> str:
        return self._format(namespace, key, values, definitions=True)

    def _format(self, namespace, key, values, *, definitions):
        template = self.entry(namespace, key)
        if template is None:
            raise ValueError(f"unknown presentation message: {namespace}:{key}")

        def replace(match):
            name = match.group(1).strip()
            if definitions and name.startswith("entity_"):
                parts = name.removeprefix("entity_").split("_", 1)
                if (len(parts) != 2 or parts[0] not in {
                    "card", "relic", "potion", "enemy", "enemyMove", "power",
                    "stance", "event", "character", "encounter",
                } or self.entry("game", ".".join(parts)) is None):
                    raise ValueError(f"unknown definition entity: {name}")
                return self.entity(*parts)
            if name not in values:
                raise ValueError(f"missing presentation parameter: {namespace}:{key}:{name}")
            return str(values[name])

        # Only the source template is parsed. Inserted values remain verbatim.
        if "{{" in re.sub(r"\{\{.*?\}\}", "", template):
            raise ValueError(f"invalid presentation template: {namespace}:{key}")
        return re.sub(r"\{\{(.*?)\}\}", replace, template)
