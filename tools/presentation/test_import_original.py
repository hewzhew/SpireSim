import json
import zipfile

import pytest

from tools.presentation import import_original as owner


@pytest.fixture
def sources():
    entities, _ = owner.build_simulator_catalog(owner.REPOSITORY)
    result = {}
    for language in owner.LANGUAGES.values():
        for kind, resource in owner.RESOURCES.items():
            rows = {}
            for entity in entities:
                if entity.kind != kind or f"{kind}.{entity.stable_id}" in owner.OMITTED:
                    continue
                key = owner.KEYS[kind].get(entity.stable_id, entity.external_id if kind in ("card", "relic") else entity.stable_id)
                rows[key] = {"NAME": f"{language}:{key}", "DESCRIPTION": "!D! NL !B! !M!"}
            result[f"localization/{language}/{resource}.json"] = rows
        result[f"localization/{language}/potions.json"]["Elixir"] = {"NAME": "deprecated-name"}
        result[f"localization/{language}/cards.json"]["Strike_R"]["NAME"] = "Strike"
        result[f"localization/{language}/cards.json"]["Strike_G"]["NAME"] = "Strike"
        characters = {key: {"NAMES": [f"{language}:{key}"]} for key in owner.CHARACTERS.values()}
        characters["Neow Event"] = {"NAMES": [f"{language}:Neow"]}
        result[f"localization/{language}/characters.json"] = characters
        result[f"localization/{language}/powers.json"] = {key: {"NAME": f"{language}:{key}"} for key in owner.POWERS.values()}
        result[f"localization/{language}/stances.json"] = {key: {"NAME": f"{language}:{key}"} for key in owner.STANCES.values()}
        result[f"localization/{language}/events.json"] = {key: {"NAME": f"{language}:{key}"} for key in owner.EVENTS.values()}
        result[f"localization/{language}/events.json"]["FaceTrader"]["DESCRIPTIONS"] = [f'{language}:scene {index} NL ~original~' for index in range(5)]
    return result


def archive(tmp_path, sources):
    path = tmp_path / "game.jar"
    with zipfile.ZipFile(path, "w") as output:
        for name, rows in sources.items():
            output.writestr(name, owner.encoded(rows))
    return path


def test_exact_source_keys_preserve_identity_and_skip_deprecated_names(tmp_path, sources):
    names, manifest, bindings = owner.names_from_archive(archive(tmp_path, sources))
    assert names["en"]["card"]["Strike"] == names["en"]["card"]["StrikeG"] == "Strike"
    assert names["zh-CN"]["potion"]["Elixir"] == "zhs:ElixirPotion"
    assert names["en"]["potion"]["WeakenPotion"] == "eng:Weak Potion"
    assert names["zh-CN"]["relic"]["HappyFlower"] == "zhs:Happy Flower"
    assert names["en"]["power"]["Weak"] == "eng:Weakened"
    assert names["en"]["power"]["Regen"] == "eng:Regenerate"
    assert names["en"]["power"]["Regeneration"] == "eng:Regeneration"
    assert names["en"]["power"]["DexterityDown"] == "eng:DexLoss"
    assert names["en"]["power"]["RetainCards"] == "eng:Retain Cards"
    assert "GenericStrengthUp" not in names["en"]["power"]
    assert "GuardianThreshold" not in names["en"]["power"]
    assert manifest["name_counts"]["en"]["power"] == 134
    assert "src/content/powers/mod.rs" in manifest["identity_inputs_sha256"]
    assert bindings["zh-CN"]["potion.Elixir"]["key"] == "ElixirPotion"
    assert bindings["zh-CN"]["potion.Elixir"]["entry"] == "localization/zhs/potions.json"
    assert "Impulse" not in names["en"]["card"]
    assert manifest["name_counts"]["en"]["card"] == 370
    assert len(manifest["entries_sha256"]) == 16
    assert manifest["name_counts"]["en"]["event"] == 53
    assert names["zh-CN"]["event"]["Ssssserpent"] == "zhs:Liars Game"
    assert names["en"]["event"]["BackTotheBasics"] == "eng:Back to Basics"
    assert names["zh-CN"]["event"]["Neow"] == "zhs:Neow"
    assert "BonfireSpirits" not in names["en"]["event"]
    assert bindings["en"]["event.FountainOfCurseCleansing"]["entry"] == "localization/eng/events.json"
    assert bindings["en"]["event.FountainOfCurseCleansing"]["key"] == "Fountain of Cleansing"
    assert bindings["en"]["event.Neow"]["entry"] == "localization/eng/characters.json"
    assert names["zh-CN"]["stance"]["Wrath"] == "zhs:Wrath"
    assert names["en"]["encounter"]["TheHeart"] == names["en"]["enemy"]["CorruptHeart"]
    assert names["zh-CN"]["encounter"]["DonuAndDeca"] == "zhs:Donu与zhs:Deca"


def test_card_templates_keep_original_fields_and_do_not_guess_upgrade_semantics(tmp_path, sources):
    for language in owner.LANGUAGES.values():
        cards = sources[f"localization/{language}/cards.json"]
        cards["Body Slam"].update(DESCRIPTION="Deal damage equal to your Block.", UPGRADE_DESCRIPTION=" NL (Deals !D! damage.)")
        cards["Armaments"].update(UPGRADE_DESCRIPTION="Upgrade all cards.", EXTENDED_DESCRIPTION=["raw #ytext", ""])
    jar = archive(tmp_path, sources)
    _, _, bindings = owner.names_from_archive(jar)
    imported = owner.card_texts_from_archive(jar, bindings)
    assert imported["cards"]["en"]["BodySlam"] == {"DESCRIPTION": "Deal damage equal to your Block.", "UPGRADE_DESCRIPTION": " NL (Deals !D! damage.)"}
    assert imported["cards"]["zh-CN"]["Armaments"]["EXTENDED_DESCRIPTION"] == ["raw #ytext", ""]
    assert imported["cards"]["en"]["ShrugItOff"]["DESCRIPTION"] == "!D! NL !B! !M!"
    assert len(imported["cards"]["en"]) == 370
    assert "Impulse" not in imported["cards"]["en"]


def test_event_templates_keep_only_consumed_original_scenes_with_their_source(tmp_path, sources):
    jar = archive(tmp_path, sources)
    _, manifest, bindings = owner.names_from_archive(jar)
    imported = owner.event_texts_from_archive(jar, bindings)
    for locale, language in owner.LANGUAGES.items():
        assert list(imported["events"][locale]) == ["FaceTrader"]
        assert imported["events"][locale]["FaceTrader"]["DESCRIPTIONS"] == sources[f"localization/{language}/events.json"]["FaceTrader"]["DESCRIPTIONS"]
    assert all(manifest["entries_sha256"][entry] == digest for entry, digest in imported["entries_sha256"].items())


def test_changed_event_scene_layout_requires_review(tmp_path, sources):
    sources["localization/eng/events.json"]["FaceTrader"]["DESCRIPTIONS"].append("new source layout")
    jar = archive(tmp_path, sources)
    _, _, bindings = owner.names_from_archive(jar)
    with pytest.raises(ValueError, match="layout requires review"):
        owner.event_texts_from_archive(jar, bindings)


def test_english_display_aliases_are_not_used_for_lookup(tmp_path, sources, monkeypatch):
    from dataclasses import replace

    original = owner.build_simulator_catalog
    def changed(repository):
        entities, hashes = original(repository)
        return [replace(entity, display_name="an unrelated translated label") for entity in entities], hashes
    monkeypatch.setattr(owner, "build_simulator_catalog", changed)
    names, _, _ = owner.names_from_archive(archive(tmp_path, sources))
    assert names["zh-CN"]["potion"]["Elixir"] == "zhs:ElixirPotion"
    assert names["en"]["card"]["ShrugItOff"] == "eng:Shrug It Off"


def test_missing_chinese_entry_stops_instead_of_english_or_name_fallback(tmp_path, sources):
    cards = sources["localization/zhs/cards.json"]
    cards["ShrugItOff"] = cards.pop("Shrug It Off")
    with pytest.raises(KeyError, match="Shrug It Off"):
        owner.names_from_archive(archive(tmp_path, sources))


def test_empty_original_names_stop_the_import(tmp_path, sources):
    sources["localization/eng/potions.json"]["ElixirPotion"]["NAME"] = " "
    with pytest.raises(ValueError, match="Empty original name"):
        owner.names_from_archive(archive(tmp_path, sources))


def test_a_new_source_for_an_omitted_card_requires_review(tmp_path, sources):
    sources["localization/eng/cards.json"]["Impulse"] = {"NAME": "new-release-name"}
    with pytest.raises(ValueError, match="now supplies the omitted"):
        owner.names_from_archive(archive(tmp_path, sources))


def test_dynamic_and_auxiliary_power_names_require_review_when_the_original_supplies_a_fixed_name(tmp_path, sources):
    for key in ["Generic Strength Up Power", "GuardianThreshold"]:
        original = sources["localization/eng/powers.json"].get(key)
        sources["localization/eng/powers.json"][key] = {"NAME": "new fixed original name"}
        with pytest.raises(ValueError, match="now supplies a fixed name"):
            owner.names_from_archive(archive(tmp_path, sources))
        if original is None:
            del sources["localization/eng/powers.json"][key]
        else:
            sources["localization/eng/powers.json"][key] = original


def test_write_and_check_preserve_authored_messages_and_detect_drift(tmp_path, sources):
    jar = archive(tmp_path, sources)
    directory = tmp_path / "catalogue"
    (directory / "locales").mkdir(parents=True)
    for locale in owner.LANGUAGES:
        (directory / "locales" / f"{locale}.json").write_bytes(owner.encoded({"game": {
            "card": {"Bash": "old-name"}, "potionRule": {"Elixir": "Our full mechanism explanation."},
            "effect": {"damage": "Deal {{damage}} damage."}}, "replay": {"button": "Next"}}))
    assert not owner.update(jar, directory)["passed"]
    assert owner.update(jar, directory, write=True)["passed"]
    assert owner.update(jar, directory)["changed"] == []
    file = directory / "locales/en.json"
    catalog = json.loads(file.read_bytes())
    assert catalog["game"]["potionRule"]["Elixir"] == "Our full mechanism explanation."
    assert catalog["game"]["effect"]["damage"] == "Deal {{damage}} damage."
    assert catalog["replay"]["button"] == "Next"
    catalog["game"]["card"]["Bash"] = "accidental-translation"
    file.write_bytes(owner.encoded(catalog))
    assert owner.update(jar, directory)["changed"] == ["en.json"]


def test_failed_source_check_leaves_catalogues_untouched(tmp_path, sources):
    directory = tmp_path / "catalogue"
    (directory / "locales").mkdir(parents=True)
    for locale in owner.LANGUAGES:
        (directory / "locales" / f"{locale}.json").write_text('{"game":{}}', encoding="utf-8")
    del sources["localization/zhs/relics.json"]["Burning Blood"]
    with pytest.raises(KeyError, match="Burning Blood"):
        owner.update(archive(tmp_path, sources), directory, write=True)
    assert (directory / "locales/en.json").read_text(encoding="utf-8") == '{"game":{}}'
    assert not (directory / "original-names.json").exists()
