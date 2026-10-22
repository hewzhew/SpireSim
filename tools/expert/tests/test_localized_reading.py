from copy import deepcopy
import json
from pathlib import Path
import subprocess
import sys

import pytest

from tools.expert.curated import CuratedStrategy, render_curated
from tools.expert.decision import DecisionReading, render_decision
from tools.presentation import Presentation

DIRECTORY = Path(__file__).parent / "fixtures"
ROOT = Path(__file__).parents[3]


def exports(value, prefix=""):
    if isinstance(value, dict):
        if value.get("schema", "").startswith("sts-public-decision-export-v"):
            yield prefix, value
        else:
            for key, item in value.items():
                yield from exports(item, prefix + "/" + key)
    elif isinstance(value, list):
        for index, item in enumerate(value):
            yield from exports(item, prefix + f"/{index}")


CASES = [(path.name + name, document) for path in sorted(DIRECTORY.glob("*.json"))
         for name, document in exports(json.loads(path.read_text(encoding="utf-8")))]


@pytest.mark.parametrize("name,document", CASES, ids=[name for name, _ in CASES])
def test_both_languages_render_every_frozen_reading_without_changing_facts(tmp_path, name, document):
    path = tmp_path / "decision.json"
    path.write_text(json.dumps(document, ensure_ascii=False), encoding="utf-8")
    original = path.read_bytes()
    reading = DecisionReading(path)
    views = [reading.view(part) for part in ("overview", "deck", "map", "relics", "history", "context", "candidates", "mechanics")]
    cards = reading.view("deck")["content"]
    relics = reading.view("relics")["content"]
    if cards:
        views.append(reading.view("mechanics", card_ordinals=[row["ordinal"] for row in cards]))
    if relics:
        views.append(reading.view("mechanics", relic_ordinals=[row["ordinal"] for row in relics]))
    for view in views:
        facts = deepcopy(view)
        en, zh = [render_decision(view, locale=locale) for locale in ("en", "zh-CN")]
        assert en != zh and "{{entity_" not in en + zh
        assert "{{amount}}" not in en + zh
        assert view == facts, name
        surface = document["payload"].get("strategic_reading")
        if surface:
            if view["part"] == "context":
                assert surface["context_summary"] in en and surface["context_summary"] in zh
                if surface["context_detail"]:
                    assert surface["context_detail"].rstrip() in en
                    assert surface["context_detail"].rstrip() in zh
            if view["part"] == "candidates":
                for row in surface["candidates"]:
                    assert row["brief"] in en and row["brief"] in zh
    assert path.read_bytes() == original


def test_owned_context_locations_are_typed_and_render_without_chinese_parsing(tmp_path):
    source = json.loads((DIRECTORY / "public-relic-readings.json").read_text(encoding="utf-8"))[0]["document"]
    path = tmp_path / "relics.json"
    path.write_text(json.dumps(source), encoding="utf-8")
    view = DecisionReading(path).view("context")
    assert view["content"]["relics"][0]["location"] == {"kind": "shopLocation", "index": 0}
    assert "Shop relic slot 0" in render_decision(view, locale="en")
    assert "商店遗物槽 0" in render_decision(view, locale="zh-CN")


def test_curated_source_prose_and_reading_receipts_survive_language_switch():
    package = CuratedStrategy(ROOT / "reference/expert/curated/ironclad-card-reward.json")
    reading = package.read(["future-value"], detail="provenance")
    original = deepcopy(reading)
    for locale in ("en", "zh-CN"):
        rendered = render_curated(reading, locale=locale)
        row = reading["readings"][0]["content"]
        assert row["principle"] in rendered and row["check"] in rendered and row["unless"] in rendered
        for source in row["sources"].values():
            assert source["scope_note"] in rendered and source["curator_scope_note"] in rendered
        assert reading == original
    receipt = reading["readings"][0]["revision"]
    unchanged = package.read(["future-value"], detail="provenance", known={"future-value": receipt})
    assert "unchanged" in render_curated(unchanged, locale="en")
    assert "已读内容无变化" in render_curated(unchanged, locale="zh-CN")


def test_cli_languages_keep_json_views_identical(capsys):
    from tools.expert.cli import main
    args = ["curated", "--decision", str(DIRECTORY / "public-owned-card-instances.json"), "--view", "mechanics", "--card", "0"]
    snapshots = []
    for locale in ("en", "zh-CN"):
        assert main(["--locale", locale, *args, "--json"]) == 0
        snapshots.append(json.loads(capsys.readouterr().out))
        assert main(["--locale", locale, *args]) == 0
        text = capsys.readouterr().out
        assert ("Current persistent cost 0" if locale == "en" else "当前持久费用 0") in text
        assert Presentation(locale).entity("card", "Corruption") in text
    assert snapshots[0] == snapshots[1]


@pytest.mark.parametrize("locale", ["en", "zh-CN"])
def test_cli_help_and_language_positions_use_the_same_parser(locale, capsys):
    from tools.expert.cli import main
    for args in (["--locale", locale, "curated", "--help"], ["curated", "--locale", locale, "--help"]):
        with pytest.raises(SystemExit) as stopped:
            main(args)
        assert stopped.value.code == 0
        help_text = capsys.readouterr().out
        assert ("usage:" if locale == "en" else "用法：") in help_text
        assert "--candidate" in help_text and "--locale" in help_text
        assert ("Reading ordinal" if locale == "en" else "阅读序号") in help_text
    assert main(["curated", "--locale", locale, "--view", "deck"]) == 1
    assert ("requires a public" if locale == "en" else "需要公开") in capsys.readouterr().err


def test_source_queries_keep_attributed_evidence_while_localizing_owned_text(tmp_path, capsys):
    from tools.expert.cli import main
    from tools.expert.query import ExpertKnowledgeOwner
    from tools.expert.tests.test_expert_query import _fixture
    _fixture(tmp_path)
    owner = ExpertKnowledgeOwner(tmp_path)
    en = owner.entity_evidence("残暴", locale="en")
    zh = owner.entity_evidence("残暴", locale="zh-CN")
    assert en.pop("boundary") != zh.pop("boundary")
    assert en == zh
    for locale in ("en", "zh-CN"):
        assert main(["--repository", str(tmp_path), "entity", "--locale", locale, "--name", "残暴"]) == 0
        rendered = capsys.readouterr().out
        assert ("DIRECT REVIEWED PASSAGES" if locale == "en" else "直接关联的已审阅段落") in rendered
        assert en["direct_reviewed_passages"][0]["excerpt"] in rendered


def test_damage_draw_and_energy_keep_the_existing_display_order():
    reading = DecisionReading(DIRECTORY / "public-shop-card-secondary.json")
    rows = reading.view("candidates")["content"]
    selected = next(row["ordinal"] for row in rows if row["action"].get("card") == "FlashOfSteel")
    document = reading.view("mechanics", candidate_ordinals=[selected])
    en, zh = [render_decision(document, locale=locale) for locale in ("en", "zh-CN")]
    assert en.index("Each hit uses") < en.index("card draws")
    assert zh.index("每段使用基础伤害") < zh.index("抽牌请求")
    energy = DecisionReading(DIRECTORY / "public-card-reward-energy.json")
    selected = next(row["ordinal"] for row in energy.view("candidates")["content"] if row["action"].get("card") == "Dropkick")
    document = energy.view("mechanics", candidate_ordinals=[selected])
    en, zh = [render_decision(document, locale=locale) for locale in ("en", "zh-CN")]
    assert en.index("Each hit uses") < en.index("Request Energy")
    assert zh.index("每段使用基础伤害") < zh.index("请求加费")


def test_detached_cli_does_not_import_raw_query_or_wiki():
    script = """
import sys
sys.modules['tools.expert.query'] = None
sys.modules['tools.wiki'] = None
from tools.expert.cli import main
raise SystemExit(main(['curated', '--locale', 'en', '--decision', sys.argv[1], '--view', 'mechanics', '--card', '0']))
"""
    result = subprocess.run([sys.executable, "-c", script, str(DIRECTORY / "public-owned-card-instances.json")],
                            cwd=ROOT, capture_output=True, text=True, encoding="utf-8", timeout=15)
    assert result.returncode == 0, result.stderr
    assert "Current persistent cost 0" in result.stdout
