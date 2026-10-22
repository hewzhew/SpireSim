from dataclasses import FrozenInstanceError
import json
from pathlib import Path
import re

import pytest

from tools.presentation import Presentation


def test_instances_do_not_share_a_mutable_language():
    en, zh = Presentation("en"), Presentation("zh-CN")
    assert en.entity("card", "Strike") == "Strike"
    assert zh.entity("card", "Strike") == "打击"
    with pytest.raises(FrozenInstanceError):
        en.locale = "zh-CN"
    assert en.entity("card", "Strike") == "Strike"
    with pytest.raises(ValueError, match="unsupported text locale"):
        Presentation("fr")


def test_scalar_substitution_preserves_values_and_requires_parameters():
    view = Presentation("en")
    value = "{{not_reparsed}} <br>\n原文"
    assert view.text("expert", "curated.check", value=value) == "Check: " + value
    with pytest.raises(ValueError, match="missing presentation parameter"):
        view.text("expert", "curated.check")
    with pytest.raises(ValueError, match="unknown presentation message"):
        view.text("expert", "no_such_message")


def test_names_upgrades_and_unknown_identity_match_other_runtime_contracts():
    en, zh = Presentation("en"), Presentation("zh-CN")
    assert zh.card_name("SearingBlow", 1) == "灼热攻击+1"
    assert en.card_name("Strike", 1) == "Strike+"
    assert zh.card_name("StrikeG", 1) == "打击+"
    assert en.card_name("Strike", 2) == "Strike+2"
    assert zh.entity("relic", "BloodyIdol") == "鲜血神像"
    assert zh.entity("card", "FutureCard") == "FutureCard"
    assert zh.entity("card", "Impulse") == en.entity("card", "Impulse")


def test_expert_catalogs_have_equal_message_and_parameter_contracts():
    directory = Path(__file__).parent / "locales"
    catalogs = [json.loads((directory / f"{locale}.json").read_text(encoding="utf-8"))["expert"] for locale in ("en", "zh-CN")]

    def flatten(value, prefix=""):
        result = {}
        for key, item in value.items():
            if isinstance(item, dict):
                result.update(flatten(item, prefix + key + "."))
            else:
                assert isinstance(item, str) and item
                result[prefix + key] = item
        return result

    en, zh = map(flatten, catalogs)
    assert en.keys() == zh.keys()
    for key in en:
        assert not re.search(r"[\u4e00-\u9fff]", en[key]), key
        slots = [set(re.findall(r"\{\{(.*?)\}\}", catalog[key])) for catalog in (en, zh)]
        assert slots[0] == slots[1], key
        for locale in ("en", "zh-CN"):
            view = Presentation(locale)
            parameters = {name: "{{literal}}" for name in slots[0] if not name.startswith("entity_")}
            result = view.definition_text("expert", key, **parameters)
            assert "{{entity_" not in result
            if parameters:
                assert "{{literal}}" in result
