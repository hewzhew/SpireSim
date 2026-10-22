"""Selective offline views of a validated Rust public-decision export.

This reader owns neither the game nor action resolution. Export integrity is
checked here; semantic identity is validated by the Rust producer. A detached
export makes no claim about the current live head or Java parity.
"""

from __future__ import annotations

from copy import deepcopy
import json
from pathlib import Path
from typing import Any

from .curated import CuratedStrategy, _digest, _require, _unique_object

READING_SCHEMA = "sts-public-decision-reading-v2"


class DecisionReading:
    def __init__(self, path: Path):
        self.path = path.resolve(strict=True)
        self._bytes = self.path.read_bytes()
        try:
            document = json.loads(self._bytes, object_pairs_hook=_unique_object)
            _require(
                set(document) == {"schema", "payload", "payload_sha256"}
                and document["schema"]
                in {
                    "sts-public-decision-export-v1",
                    "sts-public-decision-export-v2",
                    "sts-public-decision-export-v3",
                },
                "unsupported public decision export",
            )
            self._payload = document["payload"]
            self.revision = document["payload_sha256"]
            _require(
                _digest(self._payload) == self.revision, "public export bytes changed"
            )
            _require(
                set(self._payload)
                == (
                    {"source", "producer", "decision"}
                    | (
                        {"card_mechanics"}
                        if not document["schema"].endswith("v1")
                        else set()
                    )
                    | (
                        {"strategic_reading"}
                        if document["schema"].endswith("v3")
                        else set()
                    )
                    | (
                        {"potion_mechanics", "relic_mechanics", "deck_mechanics"} & self._payload.keys()
                        if document["schema"].endswith("v3")
                        else set()
                    )
                ),
                "invalid public export payload",
            )
            self._source = self._payload["source"]
            _require(
                self._source["kind"] in {"run_play_head", "run_play_scratch_head", "constructed_contract_case"}
                and self._payload["producer"]["component"] == "sts_run_play",
                "unknown public export producer or source",
            )
            self._decision = self._payload["decision"]
            observation = self._decision["observation"]
            _require(observation["domain"] == "strategic", "strategic reading required")
            self._observation = observation["observation"]
            self._information = self._decision["information"]
            _require(
                self._information["observation"]["schema_name"]
                == "TaskStrategicPublicObservationV3",
                "unsupported public observation schema",
            )
            self.site = self._decision["site"]
            _require(
                self.site == self._observation["decision_site"],
                "public decision sites disagree",
            )
            context_kind = "event" if self.site == "neow" else self.site
            _require(
                self._observation["context"]["kind"] == context_kind,
                "public decision context disagrees with site",
            )
            candidates = self._decision["candidates"]
            ids = self._information["candidate_surface"]["ordered_candidate_ids"]
            _require(
                len(candidates) == len(ids) and len(set(ids)) == len(ids),
                "public candidate identities disagree with candidates",
            )
            _require(
                all(row["domain"] == "strategic" for row in candidates),
                "non-strategic candidate in strategic reading",
            )
            self._surface = self._payload.get("strategic_reading")
            self._presentations = {}
            if "strategic_reading" in self._payload:
                self._presentations = _validate_surface(self._surface, self._decision)
            else:
                # Older captures have no typed-owner display projection.
                _require(
                    self.site == "card_reward",
                    "this legacy export only supports card-reward reading; use a current strategic export",
                )
                for candidate in candidates:
                    _candidate_label(candidate["action"])
            self._mechanics = None
            self._potion_mechanics = {}
            self._relic_mechanics = {}
            self._deck_mechanics = {}
            if "deck_mechanics" in self._payload:
                from .deck_mechanics import validate_deck_mechanics

                self._deck_mechanics = validate_deck_mechanics(
                    self._payload["deck_mechanics"], self._information["snapshot_id"],
                    self._observation["cards"],
                )
            if "relic_mechanics" in self._payload:
                from .relic_mechanics import validate_relic_mechanics

                self._relic_mechanics = validate_relic_mechanics(
                    self._payload["relic_mechanics"], self._information["snapshot_id"],
                    self._observation, self._candidates(),
                )
            if "potion_mechanics" in self._payload:
                from .potion_mechanics import validate_potion_mechanics

                self._potion_mechanics = validate_potion_mechanics(
                    self._payload["potion_mechanics"], self._information["snapshot_id"], self._candidates(),
                )
            if "card_mechanics" in self._payload:
                from .mechanics import validate_card_mechanics

                self._mechanics = validate_card_mechanics(
                    self._payload["card_mechanics"],
                    self._information["snapshot_id"],
                    self._candidates(),
                )
        except (KeyError, TypeError, AttributeError) as error:
            raise ValueError("invalid public decision reading structure") from error

    def _unchanged(self) -> None:
        _require(
            self.path.read_bytes() == self._bytes, "public export changed; reload it"
        )

    def knowledge(
        self, package: CuratedStrategy, ids: list[str], *, detail="brief", known=None
    ):
        self._unchanged()
        # Explicit enum mapping, never display-name/NLP matching.
        character = {
            "ironclad": "Ironclad",
            "silent": "Silent",
            "defect": "Defect",
            "watcher": "Watcher",
        }.get(self._observation["run"]["player_class"])
        _require(
            package.supports_decision(character, self.site),
            "knowledge scope does not match the exported decision",
        )
        return (
            package.read(ids, detail=detail, known=known) if ids else package.catalog()
        )

    def view(
        self, part: str = "overview", *, candidate_ordinals: list[int] | None = None,
        card_ordinals: list[int] | None = None,
        relic_ordinals: list[int] | None = None,
    ) -> dict[str, Any]:
        self._unchanged()
        _require(
            sum(selection is not None for selection in (candidate_ordinals, card_ordinals, relic_ordinals)) <= 1,
            "candidate, deck card, and owned relic selections cannot be mixed",
        )
        _require(
            candidate_ordinals is None or part in {"mechanics", "candidates"},
            "candidate selection requires the candidates or mechanics view",
        )
        _require(card_ordinals is None or part == "mechanics",
                 "deck card selection requires mechanics")
        _require(relic_ordinals is None or part == "mechanics",
                 "owned relic selection requires mechanics")
        rows = self._candidates()
        selected = (
            list(range(len(rows))) if candidate_ordinals is None else candidate_ordinals
        )
        if candidate_ordinals is not None:
            _require(
                bool(selected)
                and all(type(i) is int and 0 <= i < len(rows) for i in selected)
                and len(selected) == len(set(selected)),
                "invalid or duplicate reading candidate ordinal",
            )
        observation = self._observation
        if card_ordinals is not None:
            available = {card["ordinal"] for card in observation["cards"]}
            _require(bool(card_ordinals) and all(type(i) is int and i in available for i in card_ordinals)
                     and len(card_ordinals) == len(set(card_ordinals)), "invalid or duplicate deck card ordinal")
        if relic_ordinals is not None:
            _require(
                bool(relic_ordinals)
                and all(type(i) is int and 0 <= i < len(observation["relics"]) for i in relic_ordinals)
                and len(relic_ordinals) == len(set(relic_ordinals)),
                "invalid or duplicate owned relic ordinal",
            )
        if part == "overview":
            content = {
                "run": observation["run"],
                "run_goal": observation["run_goal"],
                "deck_size": len(observation["cards"]),
                "relic_count": len(observation["relics"]),
                "potions": observation["potions"],
                "map_position": {
                    key: observation["public_map"][key]
                    for key in ("current_x", "current_y", "boss")
                },
                "candidates": self._candidates(),
                "context_summary": (
                    self._surface["context_summary"] if self._surface else "选牌"
                ),
            }
        elif part == "deck":
            content = observation["cards"]
        elif part == "map":
            content = observation["public_map"]
        elif part == "relics":
            content = [{"ordinal": i, "relic": row["relic"]}
                       for i, row in enumerate(observation["relics"])]
        elif part == "history":
            content = observation["public_history"]
        elif part == "candidates":
            content = [dict(rows[i]) for i in selected]
            if self._surface and candidate_ordinals is not None:
                for row in content:
                    row["label"] = self._presentations[row["candidate_id"]]["detail"]
        elif part == "context":
            from .relic_mechanics import context_relics

            content = {
                "public": observation["context"],
                "summary": (
                    self._surface["context_summary"] if self._surface else "选牌"
                ),
                "detail": self._surface["context_detail"] if self._surface else "",
                "relics": [{**row, "mechanics": self._relic_mechanics.get(row["relic"])}
                           for row in context_relics(observation["context"])],
            }
        elif part == "mechanics" and card_ordinals is not None:
            deck = {card["ordinal"]: card for card in observation["cards"]}
            content = {"basis": "public_instance_and_shared_card_mechanics", "coverage": "partial",
                       "cards": [{"card": deck[i], "facts": self._deck_mechanics.get(i)} for i in card_ordinals]}
        elif part == "mechanics" and relic_ordinals is not None:
            content = {
                "basis": "public_observation_and_shared_relic_mechanics", "coverage": "partial",
                "relics": [
                    {"ordinal": i, "observed": observation["relics"][i],
                     "facts": self._relic_mechanics.get(observation["relics"][i]["relic"])}
                    for i in relic_ordinals
                ],
            }
        elif part == "mechanics":
            from .relic_mechanics import candidate_relic

            _require(
                self._mechanics is not None,
                "this export has no card mechanics; use a current owner export",
            )
            content = {
                "basis": "simulator_definition_and_reviewed_effects",
                "coverage": "partial",
                "candidates": [
                    {
                        **rows[i],
                        "label": rows[i].get("label")
                        or _candidate_label(rows[i]["action"]),
                        "facts": self._mechanics.get(rows[i]["candidate_id"]),
                        "potion_facts": self._potion_mechanics.get(rows[i]["candidate_id"]),
                        "relic_facts": self._relic_mechanics.get(candidate_relic(rows[i]["action"])),
                        "owned_card": self._candidate_owned_card(rows[i]["action"]),
                    }
                    for i in selected
                ],
            }
        else:
            raise ValueError("unknown public reading view")
        return deepcopy(
            {
                "schema": READING_SCHEMA,
                "export_revision": self.revision,
                "public_snapshot_id": self._information["snapshot_id"],
                # Session names can encode the seed. Keep provenance in the
                # export, not in ordinary public views (including --json).
                "source": {
                    "kind": self._source["kind"],
                    **(
                        {"head_revision": self._source["head_revision"]}
                        if self._source["kind"] in {"run_play_head", "run_play_scratch_head"}
                        else {}
                    ),
                },
                "freshness": "detached_export_not_checked_against_live_head",
                "candidate_surface": self._information["candidate_surface"]["kind"],
                "part": part,
                "selected_candidates": candidate_ordinals is not None,
                "selected_cards": card_ordinals is not None,
                "selected_relics": relic_ordinals is not None,
                "content": content,
            }
        )

    def _candidate_owned_card(self, action):
        if action["kind"] not in {"smith", "toke", "remove_card"}:
            return None
        ordinal = action.get("card_ordinal")
        _require(type(ordinal) is int, "invalid owned-card candidate ordinal")
        card = next((c for c in self._observation["cards"] if c["ordinal"] == ordinal), None)
        _require(card is not None and card["card"] == action.get("card") and card["upgrades"] == action.get("upgrades"),
                 "owned-card candidate differs from public instance")
        return {"card": card, "facts": self._deck_mechanics.get(card["ordinal"])}

    def _candidates(self):
        return [
            {
                "ordinal": ordinal,
                "candidate_id": cid,
                "action": row["action"],
                **(
                    {"label": self._presentations[cid]["brief"]}
                    if self._surface
                    else {}
                ),
            }
            for ordinal, (cid, row) in enumerate(
                zip(
                    self._information["candidate_surface"]["ordered_candidate_ids"],
                    self._decision["candidates"],
                    strict=True,
                )
            )
        ]


def _validate_surface(surface, decision):
    _require(
        isinstance(surface, dict)
        and set(surface)
        == {
            "schema",
            "public_snapshot_id",
            "observation_sha256",
            "decision_site",
            "context_summary",
            "context_detail",
            "candidates",
        },
        "invalid strategic reading surface",
    )
    _require(
        surface["schema"] == "sts-strategic-reading-surface-v1"
        and surface["public_snapshot_id"] == decision["information"]["snapshot_id"]
        and surface["decision_site"] == decision["site"],
        "strategic reading belongs to another decision",
    )
    _require(
        surface["observation_sha256"] == _digest(decision["observation"]),
        "strategic context description is bound to a different observation",
    )
    _require(
        isinstance(surface["context_summary"], str)
        and surface["context_summary"].strip()
        and isinstance(surface["context_detail"], str),
        "invalid strategic context reading",
    )
    ids = decision["information"]["candidate_surface"]["ordered_candidate_ids"]
    _require(
        isinstance(surface["candidates"], list)
        and len(surface["candidates"]) == len(ids),
        "strategic reading omits candidates",
    )
    presentations = {}
    for row in surface["candidates"]:
        _require(
            isinstance(row, dict)
            and set(row) == {"candidate_id", "candidate_sha256", "brief", "detail"},
            "invalid strategic candidate reading",
        )
        cid = row["candidate_id"]
        _require(
            isinstance(cid, str) and cid in ids and cid not in presentations,
            "duplicate or foreign strategic candidate reading",
        )
        _require(
            all(
                isinstance(row[key], str) and row[key].strip()
                for key in ("brief", "detail")
            ),
            "missing strategic candidate description",
        )
        presentations[cid] = row
    for cid, candidate in zip(ids, decision["candidates"], strict=True):
        _require(
            presentations[cid]["candidate_sha256"] == _digest(candidate),
            "strategic candidate description is bound to different typed content",
        )
    return presentations


def _card(card: str, upgrades: int) -> str:
    return f"{card}+{upgrades}" if upgrades else card


def _candidate_label(action: dict[str, Any]) -> str:
    kind = action["kind"]
    fields = {
        "take_card": {"reward_item_index", "option_index", "card", "upgrades"},
        "skip_card_reward": {"reward_item_index"},
        "open_card_reward": {"reward_item_index"},
        "singing_bowl": {"reward_item_index"},
        "use_run_potion": {"slot", "potion"},
        "discard_run_potion": {"slot", "potion"},
        "cancel": {"site"},
        "proceed": {"site"},
    }
    _require(
        kind in fields and set(action) == fields[kind] | {"kind"},
        f"unsupported card-reward candidate view: {kind}",
    )
    if kind == "take_card":
        return f"拿取 {_card(action['card'], action['upgrades'])}（{_reward_location(action)} / 选项 {action['option_index']}）"
    if kind in {"skip_card_reward", "open_card_reward", "singing_bowl"}:
        label = {
            "skip_card_reward": "跳过选牌",
            "open_card_reward": "打开选牌",
            "singing_bowl": "使用颂钵",
        }[kind]
        return f"{label}（{_reward_location(action)}）"
    if kind in {"use_run_potion", "discard_run_potion"}:
        verb = "使用" if kind == "use_run_potion" else "丢弃"
        return f"{verb} {action['potion']}（槽 {action['slot']}）"
    if kind in {"cancel", "proceed"}:
        verb = "返回" if kind == "cancel" else "继续"
        return f"{verb}（{action['site']}）"
    raise ValueError(f"unsupported card-reward candidate view: {kind}")


def _reward_location(action):
    item = action["reward_item_index"]
    return "独立选牌页" if item is None else f"奖励项 {item}"


def _render_candidates(candidates):
    return [
        f"[{row['ordinal']}] {row.get('label') or _candidate_label(row['action'])}"
        for row in candidates
    ]


def render_decision(document: dict[str, Any]) -> str:
    _require(document.get("schema") == READING_SCHEMA, "unsupported public decision reading schema")
    source = document["source"]
    origin = (
        f"Rust h{source['head_revision']}"
        if source["kind"] in {"run_play_head", "run_play_scratch_head"}
        else "构造案例"
    )
    lines = [
        f"{origin} · 离线公开局面；未检查当前会话，序号仅供本次阅读。",
        f"候选范围：{document['candidate_surface']}；"
        + (
            "展开所选已持有遗物；当前候选可读 candidates。"
            if document.get("selected_relics") else
            "展开所选牌组实例，不授予升级或移除动作；当前候选可读 candidates。"
            if document.get("selected_cards") else
            "展开本次所选候选，完整列表可读 candidates。"
            if document["part"] == "mechanics" or document.get("selected_candidates")
            else (
                "保留 owner 提供的全部候选。"
                if document["part"] in {"overview", "candidates"}
                else "当前为局部视图，完整列表可读 candidates。"
            )
        ),
    ]
    content, part = document["content"], document["part"]
    if part == "overview":
        run = content["run"]
        keys = "/".join(
            f"{name}{'有' if key else '无'}"
            for name, key in zip(("红", "蓝", "绿"), run["keys"], strict=True)
        )
        lines.extend(
            [
                f"{run['player_class']} A{run['ascension_level']} · Act {run['act']} / 层 {run['floor']} · 目标 {content['run_goal']}",
                f"HP {run['current_hp']}/{run['max_hp']} · 金币 {run['gold']} · 钥匙 {keys}",
                f"牌组 {content['deck_size']} 张 · 遗物 {content['relic_count']} 件",
                "药水（容量 "
                + str(run["potion_capacity"])
                + "）："
                + "；".join(
                    f"槽 {slot['slot']} {slot['potion']['potion'] if slot['potion'] else '空'}"
                    for slot in content["potions"]
                ),
            ]
        )
        position = content["map_position"]
        lines.append(content["context_summary"])
        lines.append(
            f"地图坐标 ({position['current_x']},{position['current_y']}) · Boss {position['boss'] or 'Unknown'}；拓扑按需展开。"
        )
        lines.extend(_render_candidates(content["candidates"]))
        lines.append(
            "可展开：deck / map / relics / history / context / candidates / mechanics；后两项可指定候选序号，mechanics 也可用 --card 选牌组序号或 --relic 选已持有遗物序号。"
        )
    elif part == "deck":
        for card in content:
            modifiers = []
            for field in (
                "misc_value",
                "base_damage_override",
                "base_block_override",
                "cost_modifier",
            ):
                value = card[field]
                if value is not None and (value != 0 or field.endswith("override")):
                    modifiers.append(f"{field}={value}")
            lines.append(
                f"[{card['ordinal']}] {_card(card['card'], card['upgrades'])}"
                + (" · " + ", ".join(modifiers) if modifiers else "")
            )
    elif part == "map":
        lines.append(
            f"当前 ({content['current_x']},{content['current_y']}) · Boss {content['boss'] or 'Unknown'}"
        )
        for node in content["nodes"]:
            edges = ", ".join(
                f"({e['destination_x']},{e['destination_y']})" for e in node["edges"]
            )
            lines.append(
                f"({node['x']},{node['y']}) {node['room']}"
                + (" · 绿钥匙" if node["has_emerald_key"] else "")
                + f" -> {edges or '无出边记录'}"
            )
    elif part == "relics":
        for row in content:
            lines.append(f"[{row['ordinal']}] {row['relic']}")
        lines.append("用 mechanics --relic N 展开当前观测与部分机制，可重复 --relic 比较多件。"
                     if content else "暂无已持有遗物。")
    elif part == "history":
        lines.extend(
            f"{key}：{value if value is not None else 'Unknown'}"
            for key, value in content.items()
        )
    elif part == "candidates":
        lines.extend(_render_candidates(content))
    elif part == "context":
        lines.append(content["summary"])
        if content["detail"]:
            lines.append(content["detail"].rstrip())
        if content["relics"]:
            from .relic_mechanics import RELIC_READING_SCOPE, render_relic_mechanics

            lines.append("可见遗物机制；是否可购买/领取仍看原库存与候选。")
            if any(row["mechanics"] is not None for row in content["relics"]):
                lines.append(RELIC_READING_SCOPE)
            for row in content["relics"]:
                lines.append(f"{row['location']} · {row['relic']}")
                lines.extend("  " + text for text in render_relic_mechanics(row["mechanics"]))
    elif part == "mechanics":
        from .mechanics import render_mechanics

        lines.extend(render_mechanics(content))
    return "\n".join(lines)
