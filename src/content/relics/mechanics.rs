//! Reviewed relic rules shared by runtime hooks and acquisition facts.
use super::RelicId;
use crate::content::powers::PowerId;
use serde::{Deserialize, Serialize};

/// Short, reviewed effect summaries for relic inspection. Missing entries are
/// deliberately reported as unknown by the caller.
pub fn explanation(relic: RelicId) -> Option<&'static str> {
    match relic {
        RelicId::Astrolabe => Some("Choose 3 cards to transform; the replacements are upgraded."),
        RelicId::Pantograph => Some("At the start of each boss combat, heal 25 HP."),
        RelicId::IncenseBurner => Some(super::incense_burner::MECHANICS),
        RelicId::ChampionBelt => Some(super::champion_belt::MECHANICS),
        RelicId::EternalFeather => Some(super::eternal_feather::MECHANICS),
        RelicId::AncientTeaSet => Some("After entering a campfire, gain 2 extra Energy on the first turn of the next combat. Entering the campfire is sufficient; choosing Rest is not required. This is not extra Energy every turn."),
        RelicId::Lantern => Some(super::lantern::MECHANICS),
        RelicId::Anchor => Some("At the start of each combat, gain 10 Block. This protects the opening turn; it is not recurring Block every turn."),
        RelicId::ArtOfWar => Some(super::art_of_war::MECHANICS),
        RelicId::Brimstone => Some(super::brimstone::MECHANICS),
        RelicId::Pocketwatch => Some(super::pocketwatch::MECHANICS),
        RelicId::LizardTail => Some("Once per run, when lethal HP loss would defeat you, heal for 50% of Max HP, rounded down with a minimum healing request of 1. Healing modifiers and restrictions apply; Mark of the Bloom prevents revival. After triggering, the relic is used up for the rest of the run."),
        RelicId::GremlinHorn => Some("When an enemy dies while another living enemy remains, gain 1 Energy, then draw 1 card. Draw prevention and hand capacity still apply. This can trigger for multiple kills; escaped enemies do not count."),
        RelicId::HandDrill => Some("When damage breaks an enemy's positive Block down to 0, apply 2 Vulnerable to that enemy after the hit. Artifact can block this debuff. The new Vulnerable does not amplify the block-breaking hit itself."),
        RelicId::StrikeDummy => Some(super::strike_dummy::MECHANICS),
        RelicId::Necronomicon => Some("Each turn, replay your first qualifying Attack for free against the same target: its current cost for the turn must be at least 2 and it must not be a free-to-play card, or an X-cost Attack must spend at least 2 Energy. The replay applies the card's effects again. On pickup, add Necronomicurse to the permanent deck; this Curse regenerates when normally removed while Necronomicon is held."),
        RelicId::Enchiridion => Some("At the start of each combat, add a temporary random Power card from your class to your hand. It costs 0 that turn. Its Power effect applies only after you play it; the generated card is not added to your permanent deck."),
        RelicId::NilrysCodex => Some("At the end of each player turn, choose one of 3 randomly offered class cards to insert as a temporary copy at a random position in your draw pile, or skip. It supplies a future draw option rather than drawing the card immediately, and does not change your permanent deck."),
        RelicId::Torii => Some("When a normal enemy hit would deal 2-5 unblocked damage to you, reduce that damage to 1. Checks each hit after Block, so a larger hit can qualify after partial Block or Weak reduces it. HP-loss and Thorns damage do not qualify."),
        RelicId::TungstenRod => Some(super::tungsten_rod::MECHANICS),
        RelicId::UnceasingTop => Some("During your turn, when the hand is empty after queued actions finish, draw 1 card. Can trigger again if you empty the hand again. Requires cards available to draw or reshuffle, and drawing to be allowed; No Draw prevents it. It supplies no Energy for the drawn card. Unplayable or unaffordable cards left in hand prevent the empty-hand trigger."),
        RelicId::RegalPillow => Some("Whenever you Rest at a campfire, heal 15 additional HP, up to Max HP and subject to healing restrictions. It does not heal when choosing Smith or another campfire action."),
        RelicId::ChemicalX => Some("Your X-cost card effects use X + 2 while spending the ordinary X Energy. This increases the effect amount; it does not grant 2 Energy or affect fixed-cost cards."),
        RelicId::Sling => Some("At the start of elite combats only, gain 2 Strength. Does not grant Strength in normal or boss combats, including the Heart."),
        RelicId::BloodyIdol => Some("Whenever you successfully gain a positive amount of Gold, heal 5 HP, up to Max HP and subject to healing restrictions. Combat gold triggers this when claimed; merely winning does not trigger it."),
        RelicId::MummifiedHand => Some("Whenever you play a Power, one randomly chosen eligible card remaining in your hand costs 0 for this turn. The card must have a positive printed cost and a positive current cost; X-cost, unplayable and already-free cards are not eligible. Does not draw cards or make the played Power free."),
        RelicId::Waffle => Some("On pickup, gain 7 Max HP and heal to Max HP, subject to healing restrictions. Has no recurring combat effect."),
        RelicId::SingingBowl => Some(super::singing_bowl::MECHANICS),
        RelicId::FrozenEgg => Some("Whenever you add a Power card to your deck, upgrade it."),
        RelicId::MoltenEgg => Some("Whenever you add an Attack card to your deck, upgrade it. Existing deck cards are not upgraded on pickup."),
        RelicId::ToxicEgg => Some("Whenever you add a Skill card to your deck, upgrade it. Existing deck cards are not upgraded on pickup."),
        RelicId::Courier => Some(super::courier::MECHANICS),
        // Prices are owned by state::shop::shop_screen and engine::shop_handler.
        RelicId::MembershipCard => Some("Shop card, relic and potion prices are 50% lower, stacking with The Courier's item discount. Card removal is also discounted, with overrides such as Smiling Mask; use the displayed removal price. Buying Membership Card immediately discounts the remaining inventory in this shop. Inspect the updated prices after purchase."),
        RelicId::MagicFlower => Some(super::magic_flower::MECHANICS),
        // Played-card destination is resolved in cards::play_queue::handle_use_card_done.
        RelicId::StrangeSpoon => Some("Whenever a played non-Power card would Exhaust after use, it has a 50% chance to be discarded instead. Cards saved this way do not trigger on-Exhaust effects such as Dark Embrace, Feel No Pain or Sentinel. Does not save other cards Exhausted by a card effect, or cards that Exhaust because of Ethereal at turn end. With Corruption, saved Skills remain reusable at 0 cost while Corruption is active, but provide no on-Exhaust payoff on that saved play."),
        RelicId::PreservedInsect => Some("At the start of elite combats, reduce each enemy's current HP to at most 75% of its Max HP, rounded down. Does not reduce Max HP or affect normal and boss combats."),
        RelicId::BottledFlame => Some("On pickup: choose an eligible Attack card in your deck. The selected copy keeps its upgrade and is placed at the top of the draw pile for the normal opening draw each combat. It occupies one opening-hand slot; its Energy cost must still be paid when played."),
        RelicId::BottledLightning => Some("On pickup: choose an eligible Skill card in your deck. The selected copy keeps its upgrade and is placed at the top of the draw pile for the normal opening draw each combat. It occupies one opening-hand slot; its Energy cost must still be paid when played."),
        RelicId::BottledTornado => Some("On pickup: choose an eligible Power card in your deck. The selected copy keeps its upgrade and is placed at the top of the draw pile for the normal opening draw each combat. It occupies one opening-hand slot; its Energy cost must still be paid when played."),
        RelicId::WarpedTongs => Some("After your start-of-turn draw, upgrade a random upgradable card in your hand for this combat. Permanent deck upgrades are not changed."),
        RelicId::OrangePellets => Some("Play an Attack, Skill and Power in one turn to remove your debuffs. Can trigger again after another trio."),
        RelicId::Ectoplasm => Some("Gain 1 Energy each turn. Future Gold gain is prevented. Already-owned Gold remains available for purchases and removal."),
        RelicId::TinyHouse => Some("On pickup: gain 5 Max HP, 50 Gold, a potion and a card reward; upgrade one random upgradable card."),
        RelicId::RunicCube => Some("Whenever you lose HP, draw 1 card per HP-loss event. Enemy hits that reach HP and self-inflicted HP loss both trigger it. Each event with positive actual HP loss draws once; fully blocked or prevented HP loss supplies zero draws."),
        RelicId::RunicDome => Some("Gain 1 Energy each turn. Enemy intents are hidden."),
        RelicId::VelvetChoker => Some("Gain 1 Energy each turn. You may play at most 6 cards per turn."),
        RelicId::CursedKey => Some("Gain 1 Energy each turn in all combats. Opening a non-boss chest adds a random Curse immediately, even if you skip its rewards. You can leave a chest unopened: this gives no chest rewards and adds no Curse. Boss relic chests do not add a Curse."),
        RelicId::BlackStar => Some("Elite combat rewards include one additional relic."),
        RelicId::BustedCrown => Some("Gain 1 Energy each turn. Card rewards offer 2 fewer cards; this does not reduce shop or Discovery choices."),
        RelicId::QuestionCard => Some("Card rewards generated after obtaining this relic offer 1 additional card, including colorless card rewards. Already-generated reward offers keep their existing cards. Shop inventory and combat Discovery choices keep their own choice counts."),
        RelicId::CallingBell => Some("On pickup: add the unremovable Curse of the Bell and receive one random Common, Uncommon and Rare relic."),
        RelicId::CoffeeDripper => Some("Gain 1 Energy each turn. Cannot choose Rest at campfires; other healing still works."),
        RelicId::EmptyCage => Some("On pickup: choose 2 removable cards to remove from your deck. If at most 2 are removable, remove them automatically."),
        RelicId::FusionHammer => Some("Gain 1 Energy each turn. Cannot choose Smith at campfires; upgrades from cards, events and other sources still work."),
        RelicId::PandorasBox => Some("On pickup: replace every starter Strike and Defend, including upgraded copies, with random cards from your character's Common, Uncommon and Rare pools. Replacements are not chosen by the player; Egg relics can upgrade them."),
        RelicId::PhilosopherStone => Some("Gain 1 Energy each turn. All enemies start combat with 1 additional Strength."),
        RelicId::RunicPyramid => Some("Do not perform the ordinary end-of-turn hand discard. This does not grant Retain: Ethereal cards still exhaust, and other discard effects still work. The hand limit still applies."),
        RelicId::SacredBark => Some("Double potion potency. Fixed effects without a scalable potency, such as choosing among 3 discovered cards, are not doubled."),
        RelicId::SlaversCollar => Some("Gain 1 additional Energy each turn during elite and boss combats. Ordinary combats do not receive this energy."),
        RelicId::SneckoEye => Some("Draw 2 additional cards each turn. Start combat with Confusion: each draw can randomize a card's nonnegative combat cost to 0-3. Negative costs (including X-cost and negative unplayable costs) are excluded."),
        RelicId::Sozu => Some("Gain 1 Energy each turn. Cannot obtain new potions; potions already held can still be used."),
        RelicId::BlackBlood => Some("Replaces Burning Blood. At the end of each victorious combat, heal 12 HP instead of Burning Blood's 6."),
        RelicId::MarkOfPain => Some("Gain 1 Energy each turn. At combat start, shuffle 2 Wounds into your draw pile."),
        RelicId::BurningBlood => Some("At the end of each victorious combat, heal 6 HP."),
        RelicId::Pear => Some("On pickup: gain 10 Max HP and heal 10 HP, subject to healing restrictions."),
        RelicId::HappyFlower => Some("Every 3 player turns, gain 1 Energy. The turn counter carries between combats."),
        RelicId::Sundial => Some("Every 3 draw-pile shuffles, gain 2 Energy. The shuffle counter carries between combats."),
        RelicId::Abacus => Some(super::abacus::MECHANICS),
        RelicId::Calipers => Some(super::calipers::MECHANICS),
        RelicId::CaptainsWheel => Some("At the start of turn 3 of each combat, gain 18 Block."),
        RelicId::DollysMirror => Some("On pickup: choose a card in your deck and add a duplicate with the same upgrade level."),
        RelicId::Shuriken => Some("Every 3 Attack cards played in a single turn, gain 1 Strength for the rest of combat. The attack counter resets each turn; multiple groups of 3 can trigger."),
        RelicId::Kunai => Some(super::kunai::MECHANICS),
        RelicId::OddMushroom => Some(super::odd_mushroom::MECHANICS),
        RelicId::MutagenicStrength => Some("At combat start, gain 3 Strength and an equal Lose Strength debuff that removes 3 Strength at the end of your first turn. Artifact can block the removal debuff."),
        RelicId::Orichalcum => Some(super::orichalcum::MECHANICS),
        RelicId::Vajra => Some("At the start of each combat, gain 1 Strength for the rest of that combat."),
        RelicId::JuzuBracelet => Some("A normal Monster room rolled for a question-mark map node becomes an Event instead. Does not prevent fixed combats within events or fights at Monster, Elite and Boss map nodes."),
        RelicId::BronzeScales => Some("At the start of each combat, gain 3 Thorns. Whenever an enemy attacks you, deal 3 damage back per hit, even when its damage is blocked."),
        RelicId::PenNib => Some("Every tenth Attack card you play deals double damage, including each hit and enemy target of that card. Counts Attack cards, not individual hits. The counter carries between combats; at counter 9 the next Attack is doubled."),
        RelicId::Nunchaku => Some("Every tenth Attack card played grants 1 Energy and resets the counter to 0. Counts Attack cards, not individual hits. The counter carries between turns and combats; at counter 9 the next Attack grants the Energy."),
        RelicId::CeramicFish => Some("Whenever a card is added to your master deck, gain 9 Gold. Cards generated only for the current combat do not qualify."),
        RelicId::PaperFrog => Some("Your attacks against Vulnerable enemies deal 75% additional damage instead of the normal 50%."),
        RelicId::Toolbox => Some("At the start of each combat, choose one of three offered Colorless cards and add a temporary copy to your hand. It keeps its normal Energy cost and is not added to your master deck."),
        RelicId::RedMask => Some("At the start of each combat, apply 1 Weak to every enemy. An enemy's Artifact can block this application and consume one Artifact charge instead."),
        RelicId::ToyOrnithopter => Some("Whenever you use a potion, heal 5 HP, up to Max HP and subject to healing restrictions. Discarding a potion does not trigger this healing."),
        RelicId::PeacePipe => Some("At a campfire, you may choose Toke to remove one eligible card from your master deck for 0 Gold instead of another campfire action. Its cost is that campfire action opportunity; shop removal keeps its separate Gold price and purchase count. Unremovable and bottled cards are not eligible."),
        RelicId::PotionBelt => Some("On pickup, add 2 permanent potion slots. It does not supply potions or change their effects."),
        RelicId::Boot => Some("When your Attack deals 1-4 unblocked damage to an enemy, increase that damage to 5. Applies per hit and target; fully blocked hits still deal 0."),
        RelicId::CentennialPuzzle => Some("The first time you lose HP in each combat, draw 3 cards at that time. Fully blocked attacks do not trigger it. Resets for the next combat."),
        RelicId::MeatOnTheBone => Some("At the end of a victorious combat, if your positive current HP is at or below half Max HP (rounded down), heal 12 HP, subject to healing restrictions and the Max HP cap."),
        RelicId::Akabeko => Some("At the start of each combat, gain 8 Vigor: the first Attack card you play gains 8 damage on every hit and target before damage modifiers, then consumes Vigor. Skills and Powers do not consume it."),
        RelicId::OddlySmoothStone => Some("At the start of each combat, gain 1 Dexterity for that combat. Dexterity increases card-generated Block before other Block modifiers."),
        RelicId::Turnip => Some(super::turnip::MECHANICS),
        RelicId::Ginger => Some(super::ginger::MECHANICS),
        RelicId::LetterOpener => Some(super::letter_opener::MECHANICS),
        RelicId::SelfFormingClay => Some(super::self_forming_clay::MECHANICS),
        RelicId::Matryoshka => Some(super::matryoshka::MECHANICS),
        RelicId::WarPaint => Some(super::war_paint::MECHANICS),
        RelicId::SmilingMask => Some("Shop card removal costs 50 Gold, regardless of previous removal count or shop price discounts. Each shop still permits only one removal. Buying this relic immediately updates the current shop's removal price."),
        RelicId::DreamCatcher => Some("After choosing Rest at a campfire, receive a normal card reward, which may be skipped. Merely entering the campfire or choosing Smith, Recall or another action does not produce this reward."),
        RelicId::BloodVial => Some("At the start of each combat, heal 2 HP, up to Max HP and subject to healing modifiers and restrictions. This is healing, not additional Max HP."),
        RelicId::MawBank => Some("Whenever you advance to a new floor, gain 12 Gold while this relic is active. Spending Gold at a shop permanently deactivates it, including purchases and card removal. Entering a shop or spending Gold in an event does not deactivate it."),
        RelicId::BagOfMarbles => Some("At the start of each combat, apply 1 Vulnerable to every enemy. Artifact can block each enemy's application and consume one charge instead. This is an opening-turn debuff, not permanent Vulnerable."),
        RelicId::PrismaticShard => Some("Normal combat card rewards can offer cards from every character and Colorless cards while preserving the rolled rarity. It does not enlarge the choice count or change ordinary shop card pools. A character other than the Defect also gains one Orb slot."),
        RelicId::FossilizedHelix => Some("At the start of each combat, gain 1 Buffer. Buffer prevents the next damage or HP-loss event that would otherwise reduce HP, then consumes one charge. Fully blocked hits leave Buffer intact; separate hits can consume it and then deal damage."),
        RelicId::NeowsLament => Some(super::neows_lament::MECHANICS),
        RelicId::Omamori => Some(super::omamori::MECHANICS),
        RelicId::Shovel => Some(super::shovel::MECHANICS),
        RelicId::BagOfPreparation => Some("At the start of each combat, draw 2 additional cards. This adds opening-hand access once per combat; later turns use the ordinary draw count."),
        RelicId::BirdFacedUrn => Some("Whenever you play a Power card, heal 2 HP, up to Max HP and subject to healing modifiers and restrictions."),
        RelicId::BlueCandle => Some("Curse cards can be played. Playing a Curse loses 1 HP and Exhausts that card for this combat. Its permanent deck copy remains. The HP loss can trigger Rupture and follows HP-loss modifiers."),
        RelicId::Cauldron => Some("On pickup, open rewards containing 5 random potions. Choose which to collect within your available potion slots; ordinary acquisition restrictions apply."),
        RelicId::CharonsAshes => Some("Whenever a card Exhausts, deal 3 non-Attack damage to all enemies. Triggers on actual Exhaust, including cards Exhausted by other effects. Strength and Attack damage modifiers do not amplify this damage."),
        RelicId::ClockworkSouvenir => Some("At the start of each combat, gain 1 Artifact. It blocks the next debuff application and consumes its charge; it does not reduce attack damage."),
        RelicId::CultistMask => Some("Cosmetic relic with the CAW CAW greeting; has no resource or combat effect."),
        RelicId::DarkstonePeriapt => Some("Whenever a Curse is added to your permanent deck, gain 6 Max HP and heal 6 HP, subject to healing restrictions. A Curse prevented by Omamori is not obtained and does not trigger this effect."),
        RelicId::DeadBranch => Some("Whenever a card Exhausts while combat continues, add a temporary random card from your character's combat card pool to your hand. The generated card has its ordinary cost and is not added to your permanent deck. Hand capacity and active card-generation modifiers apply."),
        RelicId::DuVuDoll => Some("At the start of each combat, gain 1 Strength for each Curse in your permanent deck, including Ascender's Bane. The displayed counter tracks that Curse count. Curses remain in the deck and can still occupy draws."),
        RelicId::FaceOfCleric => Some("After each victorious combat, gain 1 Max HP and heal 1 HP, subject to healing modifiers and restrictions."),
        RelicId::FrozenEye => Some("Allows inspecting the draw pile in its actual order. Viewing it does not draw, reorder or reveal the result of a future shuffle."),
        RelicId::GamblingChip => Some("After the opening draw of each combat, choose any number of cards from your hand to discard, then draw the same number. You may keep the entire hand. This happens once per combat and does not grant Energy."),
        RelicId::Girya => Some("At a campfire, choose Lift instead of another campfire action to increase this relic's counter by 1, up to 3. Start each combat with Strength equal to the counter. Merely entering a campfire does not increase it."),
        RelicId::GoldenIdol => Some("Combat Gold rewards receive a 25% bonus, rounded to the nearest integer. The bonus is received when claiming the reward; this does not multiply every source of Gold."),
        RelicId::GremlinMask => Some("At the start of each combat, apply 1 Weak to yourself. Artifact or Weak immunity can prevent the application."),
        RelicId::HornCleat => Some("At the start of turn 2 of each combat, gain 14 Block."),
        RelicId::IceCream => Some("Unspent Energy carries from one player turn to the next within the same combat, in addition to the next turn's normal Energy. This preserves unused Energy rather than creating more each turn."),
        RelicId::InkBottle => Some("Every tenth card played draws 1 card and resets the counter to 0. Counts played cards of every type; the counter carries between turns and combats. Draw prevention and hand capacity still apply."),
        RelicId::MarkOfTheBloom => Some("All healing is prevented for the rest of the run, including Rest, combat healing, act-transition healing and Lizard Tail revival. Max HP can still increase, but the associated healing is prevented."),
        RelicId::MealTicket => Some("Whenever you enter a shop room, heal 15 HP, up to Max HP and subject to healing restrictions. Purchasing is not required; buying this relic after entering does not repeat the entry heal."),
        RelicId::MedicalKit => Some("Status cards can be played and Exhaust after use. Otherwise unplayable Statuses can be played without Energy; playable Statuses such as Slimed retain their normal cost. Exhaust triggers still apply. This enables Statuses, not Curses."),
        RelicId::MercuryHourglass => Some("At the start of each player turn, deal 3 non-Attack damage to all enemies. Strength and Attack damage modifiers do not amplify this damage."),
        RelicId::NlothsGift => Some("Triples the rare-card threshold when generating normal combat card rewards. The existing rarity randomizer still applies; this is not a fixed tripled probability for every card source. Boss card rewards already use Rare cards."),
        RelicId::NlothsMask => Some("The next normal chest you open loses one relic reward, along with its linked Sapphire key if present. Opening the chest spends this effect for the run. Other chest rewards remain."),
        RelicId::OldCoin => Some("On pickup, gain 300 Gold, subject to Gold-gain restrictions. Has no recurring combat effect."),
        RelicId::OrnamentalFan => Some("Every 3 Attack cards played in a single turn, gain 4 Block. Counts cards rather than hits or targets. The attack counter resets each turn and can trigger for multiple groups of 3; Dexterity does not modify this relic-generated Block."),
        RelicId::Orrery => Some("On pickup, open 5 normal card reward choices in ordinary contexts, each of which may be skipped. Existing card-reward count modifiers apply. On a treasure reward screen, add 4 card reward choices. Cards enter your deck only when selected."),
        RelicId::PrayerWheel => Some("Normal Monster room victories produce an additional card reward choice, which can be skipped independently. Elite, Boss and event combats do not receive this extra reward."),
        RelicId::RedSkull => Some(super::red_skull::MECHANICS),
        RelicId::SpiritPoop => Some("Souvenir from the Bonfire Spirits event; has no resource or combat effect."),
        RelicId::SsserpentHead => Some("Whenever you enter an Event room, gain 50 Gold, subject to Gold-gain restrictions. A question-mark node that resolves to combat, a shop or a chest is not an Event room."),
        RelicId::StoneCalendar => Some("At the end of your seventh turn in each combat, deal 52 non-Attack damage to all enemies. This triggers once per combat; Strength and Attack damage modifiers do not amplify it."),
        RelicId::Strawberry => Some("On pickup, gain 7 Max HP and heal 7 HP, subject to healing restrictions."),
        RelicId::Mango => Some("On pickup, gain 14 Max HP and heal 14 HP, subject to healing restrictions."),
        RelicId::ThreadAndNeedle => Some("At the start of each combat, gain 4 Plated Armor. At the end of each player turn, gain Block equal to its remaining amount; each enemy Attack hit that causes HP loss removes 1 Plated Armor. Fully blocked hits preserve it."),
        RelicId::TinyChest => Some("Every fourth question-mark room entered becomes a Treasure room, and the counter resets to 0. The counter carries between Acts. Ordinary fixed Treasure nodes do not advance this counter."),
        RelicId::Whetstone => Some("On pickup, permanently upgrade up to 2 randomly selected upgradable Attack cards already in your deck. With fewer than 2 eligible cards, upgrade all eligible copies. Distinct copies are selected; Skills and Powers are not eligible."),
        RelicId::WhiteBeastStatue => Some("Combat potion drops have a 100% chance, subject to the reward-screen size limit: a potion is not added when 4 or more reward entries already exist. Collecting it still requires a free slot or replacing a held potion, and Sozu still prevents obtaining it."),
        RelicId::WingBoots => Some("Provides 3 flights for the run. Each flight can select a visible map node on the next reachable row without following the current node's outgoing connection. Ordinary connected travel spends no charge; a flight does not skip rows."),
        RelicId::Circlet | RelicId::RedCirclet => Some("Fallback relic with no resource or combat effect."),
        _ => None,
    }
}

/// The compact flag also stores several temporary trigger states. Keep its
/// player-facing meaning with the relic rules rather than calling every flag
/// permanent consumption.
pub fn trigger_state_description(relic: RelicId, flag: bool) -> Option<&'static str> {
    if relic == RelicId::RedSkull {
        return Some(if flag {
            "low-HP Strength bonus active"
        } else {
            "low-HP Strength bonus inactive"
        });
    }
    if !flag {
        return None;
    }
    Some(match relic {
        RelicId::Lantern => "opening Energy bonus spent this combat",
        RelicId::AncientTeaSet => "opening Energy check completed this combat",
        RelicId::CentennialPuzzle => "draw trigger spent this combat",
        RelicId::GamblingChip => "opening redraw completed this combat",
        RelicId::Necronomicon => "Attack replay spent this turn",
        RelicId::HoveringKite => "discard Energy trigger spent this turn",
        RelicId::UnceasingTop => "disabled until this turn ends",
        _ => "used up",
    })
}

/// Applies to getRewardCards/getColorlessRewardCards, including callers such
/// as Dream Catcher and Orrery. It does not modify shop or Discovery choices.
pub fn relic_card_reward_choice_delta_v1(relic: RelicId) -> Option<i32> {
    match relic {
        RelicId::BustedCrown => Some(-2),
        RelicId::QuestionCard => Some(1),
        _ => None,
    }
}

/// The player receive-power hook prevents this new application before Artifact.
/// Existing powers are unchanged; this is not a cleanse or a current Power.
pub fn relic_prevent_player_debuff_before_artifact_v1(relic: RelicId) -> Option<PowerId> {
    match relic {
        RelicId::Ginger => Some(super::ginger::PREVENTED_POWER_ID),
        RelicId::Turnip => Some(super::turnip::PREVENTED_POWER_ID),
        _ => None,
    }
}

/// A request on surviving combat victory. Actual healing keeps its restrictions,
/// modifiers and HP cap; this is not an immediate or guaranteed HP gain.
pub fn relic_victory_healing_request_v1(relic: RelicId) -> Option<i32> {
    match relic {
        RelicId::BurningBlood => Some(super::burning_blood::VICTORY_HEAL_AMOUNT),
        RelicId::BlackBlood => Some(super::black_blood::VICTORY_HEAL_AMOUNT),
        _ => None,
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RelicCombatPotionDropOverrideV1 {
    pub chance_percent: i32,
    pub blocked_at_reward_entry_count: usize,
}

/// A reward-generation rule, not an occupied potion slot or a pickup guarantee.
/// The size gate takes precedence; collection still uses its ordinary rules.
pub fn relic_combat_potion_drop_override_v1(
    relic: RelicId,
) -> Option<RelicCombatPotionDropOverrideV1> {
    (relic == RelicId::WhiteBeastStatue).then_some(RelicCombatPotionDropOverrideV1 {
        chance_percent: super::white_beast_statue::POTION_DROP_CHANCE_OVERRIDE_PERCENT,
        blocked_at_reward_entry_count:
            crate::state::rewards::generator::POTION_DROP_BLOCKED_AT_REWARD_ENTRY_COUNT,
    })
}

/// Extra independently generated rows, only when a normal MonsterRoom creates
/// card rewards. Elite, boss and event rooms do not qualify.
pub fn relic_additional_normal_monster_card_reward_rows_v1(relic: RelicId) -> Option<u8> {
    (relic == RelicId::PrayerWheel).then_some(1)
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RelicGoldGainRuleV1 {
    PreventGoldGain,
}

pub fn relic_gold_gain_rule_v1(relic: RelicId) -> Option<RelicGoldGainRuleV1> {
    (relic == RelicId::Ectoplasm).then_some(RelicGoldGainRuleV1::PreventGoldGain)
}

pub fn relic_prevents_gold_gain(relic: RelicId) -> bool {
    relic_gold_gain_rule_v1(relic) == Some(RelicGoldGainRuleV1::PreventGoldGain)
}

/// One immediate healing request per successful positive gold gain, independent
/// of the gold amount. The current healing owner applies modifiers and caps.
pub fn relic_gold_gain_healing_v1(relic: RelicId) -> Option<i32> {
    (relic == RelicId::BloodyIdol).then_some(super::bloody_idol::HEAL_AMOUNT)
}

/// This only controls ordinary end-turn discard. It does not grant Retain,
/// prevent Ethereal exhaust, or block discards requested by another effect.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RelicEndTurnDiscardRuleV1 {
    SkipOrdinaryDiscard,
}

pub fn relic_end_turn_discard_rule_v1(relic: RelicId) -> Option<RelicEndTurnDiscardRuleV1> {
    (relic == RelicId::RunicPyramid).then_some(RelicEndTurnDiscardRuleV1::SkipOrdinaryDiscard)
}

/// Only the named campfire option is blocked; other healing and upgrades keep
/// their own rules. Missing here is not a claim of complete relic coverage.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RelicCampfireRestrictionV1 {
    Rest,
    Smith,
}

pub fn relic_campfire_restriction_v1(relic: RelicId) -> Option<RelicCampfireRestrictionV1> {
    match relic {
        RelicId::CoffeeDripper => Some(RelicCampfireRestrictionV1::Rest),
        RelicId::FusionHammer => Some(RelicCampfireRestrictionV1::Smith),
        _ => None,
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RelicBattleStartScopeV1 {
    EveryCombat,
    EliteCombat,
}

impl RelicBattleStartScopeV1 {
    pub fn applies(self, is_elite_fight: bool) -> bool {
        self == Self::EveryCombat || is_elite_fight
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RelicBattleStartPowerKindV1 {
    Strength,
    Thorns,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RelicBattleStartPowerV1 {
    pub scope: RelicBattleStartScopeV1,
    pub power: RelicBattleStartPowerKindV1,
    pub amount: i32,
}

pub fn relic_battle_start_power_v1(relic: RelicId) -> Option<RelicBattleStartPowerV1> {
    let (scope, power, amount) = match relic {
        RelicId::BronzeScales => (
            RelicBattleStartScopeV1::EveryCombat,
            RelicBattleStartPowerKindV1::Thorns,
            super::bronze_scales::THORNS_AMOUNT,
        ),
        RelicId::Sling => (
            RelicBattleStartScopeV1::EliteCombat,
            RelicBattleStartPowerKindV1::Strength,
            super::sling::ELITE_STRENGTH_AMOUNT,
        ),
        _ => return None,
    };
    Some(RelicBattleStartPowerV1 {
        scope,
        power,
        amount,
    })
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RelicShuffleEnergyV1 {
    pub shuffle_trigger_at: i32,
    pub energy: i32,
    pub initial_counter: i32,
    pub counter_persists_between_combats: bool,
}

pub fn relic_shuffle_energy_v1(relic: RelicId) -> Option<RelicShuffleEnergyV1> {
    (relic == RelicId::Sundial).then(|| RelicShuffleEnergyV1 {
        shuffle_trigger_at: super::sundial::SHUFFLES_PER_TRIGGER,
        energy: super::sundial::ENERGY_AMOUNT,
        initial_counter: super::RelicState::new(relic).counter,
        counter_persists_between_combats: true,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::powers::PowerId;
    use crate::runtime::action::Action;

    #[test]
    fn every_ironclad_pool_and_shared_special_relic_has_a_rule() {
        use super::super::{build_relic_pool, RelicTier};
        for tier in [
            RelicTier::Starter,
            RelicTier::Common,
            RelicTier::Uncommon,
            RelicTier::Rare,
            RelicTier::Boss,
            RelicTier::Shop,
            RelicTier::Special,
        ] {
            for relic in build_relic_pool(tier, "Ironclad") {
                assert!(
                    explanation(relic).is_some_and(|rule| !rule.is_empty()),
                    "{relic:?}"
                );
            }
        }
        for relic in [RelicId::Circlet, RelicId::RedCirclet] {
            assert!(explanation(relic).is_some(), "{relic:?}");
        }
    }

    #[test]
    fn passive_request_readings_share_runtime_amounts_and_leave_other_effects_unknown() {
        for (relic, power, modify) in [
            (
                RelicId::Ginger,
                PowerId::Weak,
                super::super::ginger::on_receive_power_modify as fn(PowerId, i32) -> i32,
            ),
            (
                RelicId::Turnip,
                PowerId::Frail,
                super::super::turnip::on_receive_power_modify as fn(PowerId, i32) -> i32,
            ),
        ] {
            assert_eq!(
                relic_prevent_player_debuff_before_artifact_v1(relic),
                Some(power)
            );
            assert_eq!(modify(power, 2), 0);
            assert_eq!(modify(PowerId::Vulnerable, 2), 2);
        }
        let mut state = crate::test_support::blank_test_combat();
        for (relic, actions) in [
            (
                RelicId::BurningBlood,
                super::super::burning_blood::BurningBlood::on_victory(&state),
            ),
            (
                RelicId::BlackBlood,
                super::super::black_blood::BlackBlood::on_victory(&state),
            ),
        ] {
            assert_eq!(
                actions[0].action,
                Action::Heal {
                    target: 0,
                    amount: relic_victory_healing_request_v1(relic).unwrap(),
                }
            );
        }
        state.entities.player.current_hp = 0;
        assert!(super::super::burning_blood::BurningBlood::on_victory(&state).is_empty());
        assert!(super::super::black_blood::BlackBlood::on_victory(&state).is_empty());
        let potion = relic_combat_potion_drop_override_v1(RelicId::WhiteBeastStatue).unwrap();
        assert_eq!(
            (potion.chance_percent, potion.blocked_at_reward_entry_count),
            (100, 4)
        );
        assert!(relic_prevent_player_debuff_before_artifact_v1(RelicId::OddMushroom).is_none());
        assert!(relic_combat_potion_drop_override_v1(RelicId::Sozu).is_none());
        assert!(relic_victory_healing_request_v1(RelicId::MeatOnTheBone).is_none());
    }

    #[test]
    fn battle_start_readings_preserve_elite_only_scope_and_reactive_power() {
        let mut state = crate::test_support::blank_test_combat();
        state.meta.is_boss_fight = true;
        let sling = relic_battle_start_power_v1(RelicId::Sling).unwrap();
        assert!(!sling.scope.applies(state.meta.is_elite_fight));
        assert!(super::super::sling::at_battle_start(&state).is_empty());
        state.meta.is_elite_fight = true;
        assert!(sling.scope.applies(state.meta.is_elite_fight));
        assert!(matches!(
            super::super::sling::at_battle_start(&state)[0].action,
            Action::ApplyPower {
                power_id: PowerId::Strength,
                amount: 2,
                ..
            }
        ));
        let bronze = relic_battle_start_power_v1(RelicId::BronzeScales).unwrap();
        assert_eq!(bronze.power, RelicBattleStartPowerKindV1::Thorns);
        assert!(bronze.scope.applies(false));
        assert!(matches!(
            super::super::bronze_scales::BronzeScales::at_battle_start(0)[0].action,
            Action::ApplyPower {
                power_id: PowerId::Thorns,
                amount: 3,
                ..
            }
        ));
    }

    #[test]
    fn sundial_reading_keeps_cross_combat_progress_and_exact_trigger() {
        let rule = relic_shuffle_energy_v1(RelicId::Sundial).unwrap();
        assert_eq!(rule.initial_counter, 0);
        assert_eq!(rule.shuffle_trigger_at, 3);
        let mut relic = super::super::RelicState::new(RelicId::Sundial);
        relic.counter = 2;
        let mut carried =
            super::super::persistent_state::post_combat_persistent_relic_state_v1(&relic);
        assert!(rule.counter_persists_between_combats);
        assert_eq!(carried.counter, 2);
        let actions = super::super::sundial::on_shuffle(&mut carried);
        assert_eq!(carried.counter, 0);
        assert_eq!(actions[0].action, Action::GainEnergy { amount: 2 });
        carried.counter = 3;
        assert!(super::super::sundial::on_shuffle(&mut carried).is_empty());
        assert_eq!(
            carried.counter, 4,
            "preserve exact ++counter == 3, not >= or modulo"
        );
        assert!(relic_shuffle_energy_v1(RelicId::HappyFlower).is_none());
    }
}
