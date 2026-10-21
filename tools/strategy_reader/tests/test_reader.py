from __future__ import annotations

import json
from pathlib import Path
import shutil

import pytest

from tools.strategy_reader.cli import render_many
from tools.strategy_reader.reader import list_subjects, lookup


REPOSITORY = Path(__file__).resolve().parents[3]


def test_subject_lookup_uses_block_tags_and_explicit_aliases() -> None:
    for term in ("狗圈", "Velvet Choker", "VelvetChoker", "VELVET_CHOKER", "velvet-choker"):
        result = lookup(REPOSITORY, term)
        assert result["subject"]["name"] == "Velvet Choker"
        assert result["source_lines"]

    assert lookup(REPOSITORY, "Runic_Dome")["subject"]["name"] == "Runic Dome"
    assert lookup(REPOSITORY, "CoffeeDripper")["subject"]["name"] == "Coffee Dripper"
    assert lookup(REPOSITORY, "PhilosophersStone")["subject"]["name"] == "Philosopher's Stone"
    assert list_subjects(REPOSITORY)
    with pytest.raises(ValueError, match="unknown subject"):
        lookup(REPOSITORY, "not-a-relic")


def test_block_can_be_edited_and_reordered_without_changing_its_reference(tmp_path: Path) -> None:
    data = tmp_path / "reference/strategy"
    shutil.copytree(REPOSITORY / "reference/strategy", data)
    path = data / "corpus/chaoxi-boss-relics.json"
    document = json.loads(path.read_text(encoding="utf-8"))
    block = next(row for row in document["blocks"] if row["key"] == "b04")
    block["text"] = "剪刀，重新清理后的段落。"
    block["subjects"] = ["Mark of Pain"]
    block["author"] = "整理者"
    document["blocks"].reverse()
    path.write_text(json.dumps(document, ensure_ascii=False), encoding="utf-8")
    result = lookup(tmp_path, "MarkOfPain")
    assert result["subject"]["name"] == "Mark of Pain"
    quoted = next(row for row in result["passages"] if row["id"] == "tieba-10072740344:b04")
    assert quoted["text"] == "剪刀，重新清理后的段落。"
    assert quoted["author"] == "整理者"


def test_subject_page_can_change_its_judgment_without_rewriting_sources(tmp_path: Path) -> None:
    shutil.copytree(REPOSITORY / "reference/strategy", tmp_path / "reference/strategy")
    page = tmp_path / "reference/strategy/pages/runic-dome.md"
    original = page.read_text(encoding="utf-8")
    page.write_text(original.replace("每回合多 1 点能量。", "每回合多一费。"), encoding="utf-8")

    result = lookup(tmp_path, "Runic_Dome")
    assert result["notes"][0]["text"].startswith("每回合多一费。")
    assert result["source_lines"] == lookup(REPOSITORY, "Runic Dome")["source_lines"]
    comparison = render_many([result, lookup(tmp_path, "Cursed Key")])
    assert "Runic Dome：每回合多一费。" in comparison
    assert "Cursed Key：每回合多 1 点能量" in comparison


def test_comparison_keeps_each_subjects_judgment_and_source_links() -> None:
    output = render_many([lookup(REPOSITORY, term) for term in ("Runic Dome", "Cursed Key", "狗圈")])
    assert "1. Runic Dome：每回合多 1 点能量" in output
    assert "2. Cursed Key：每回合多 1 点能量" in output
    assert "3. Velvet Choker：每回合多 1 点能量" in output
    assert "comment/mcrah82/" in output
    assert "comment/irri741/" in output
    assert "tieba.baidu.com/p/10072740344" in output
