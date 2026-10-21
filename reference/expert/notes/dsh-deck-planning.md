# Deck planning checks for the autonomous player

Use these conditional checks with the current inventory. Obtain the actual
mechanics from displayed rules, `inventory effects` or `info <kind> <name>`.

## Keep a short working plan

At each complete decision task, rebuild three lines from its supplied actual
inventory, then maintain them within that task:

- Startup: how useful cards are reached and paid for, and what protects setup.
- Defense and damage: what covers later turns and how damage arrives.
- Next need: the largest gap before the next serious threat, and an actual
  preparation opportunity that could address it.

Describe what the deck already does, then what it needs. Ordinary attacks,
block and HP are a valid early plan. A direction requiring an unowned card or
relic is a future possibility; keep its required acquisition explicit.

Connect each preparation choice to one of these functions, including its cost
to accessing other cards. Revise the lines after meaningful inventory changes
or when the next threat changes. A restored or newly entered task uses its
actual inventory again; include the current plan at a requested checkpoint. Use
`todo_write` for concrete next tasks when useful.

## Evaluate the deck before the next addition

At a reward, shop, event or upgrade, compare the candidate with skipping or
spending the resource elsewhere. Identify its contribution to the CURRENT deck:

- Startup: access to enabling cards, their energy cost and protection while
  deploying them. Separate independent draw from draw unlocked by a power.
- Continuing defense: block, debuffs and damage prevention over later turns.
  Count consumable skills, fuel and potion reserves alongside repeatable output.
- Damage pace: how scaling starts, which attacks use it, and when damage arrives.
- Access after setup: draw, hand space, reshuffles and energy to play drawn cards.

Compare the contribution with what the owned deck already produces in the
relevant turns. For a repeated-play effect, estimate when that specific copy
can be played again and what draw/energy permits it. A card with several effects
can improve distinct functions: evaluate each effect and its timing.
When adding a draw card permanently, include the draw used to reach that new
card and the Energy spent playing it. A played card that draws one normally
replaces itself in Hand; evaluate its damage, block or other contribution with
that payment. Drawing two provides one additional hand card before other
effects. Compare an unupgraded Pommel Strike's draw 1 with the upgraded copy's
draw 2 explicitly, including an available upgrade's cost and timing. Hand
capacity, draw prevention and other triggers can change the usable access.

For combat upgrades, list the actual unupgraded targets and each added effect.
Armaments+ upgrades all eligible cards currently in Hand for that combat.
Runic Pyramid can retain a target until Armaments arrives; account for the
delay and hand space. Upgrading Second Wind increases Block per exhausted
card, Burning Pact adds a draw, and Disarm adds persistent per-hit reduction.
Feel No Pain benefits when upgraded before deployment; once deployed, its
card has left Hand. Track which targets can actually be upgraded before use.
Count a card's standalone contribution before additional engine synergies.
Evaluate each effect against its actual targets. An all-enemy Attack can still
damage and debuff a lone enemy; Thunderclap's Vulnerable can amplify later
Body Slams that turn. Compare the additional coverage with owned sources,
payment and access. Body Slam uses Block as its base damage, then receives
ordinary Attack modifiers, including Strength, Weak and Vulnerable.
Second Wind can turn Power Through's Wounds or enemy Statuses into Block and
remove them from later draws; Feel No Pain and Dark Embrace add further payoffs.

For permanent removal, compare the first draw and later cycles separately.
Count the candidate's marginal output, the Energy and access needed to remove
it during combat, and which owned Exhaust effects can actually target it.
Unupgraded Limit Break at zero Strength can spend 1 Energy to Exhaust itself;
Strike keeps recurring until another effect Exhausts it. Second Wind can
Exhaust the former but cannot Exhaust the latter. When ordinary damage is
already covered, removing a recurring weak Attack can protect later draws more
than removing a currently inactive self-Exhausting Skill. Include actual
Exhaust payoffs and feasible future enabling sources, with their acquisition
still explicit. Limit Break+ loses self-Exhaust; re-evaluate that upgrade under
the current Strength sources and removal effects.

When two cards supply the same effect, compare its amount, duration, targets,
access and destination after use. A single Exhausting debuff and a repeatable
two-round debuff cover different future attack windows. For conditional block,
identify how much block is already available when the card is accessed; for
later cycles, track what remains after Skills have Exhausted.
For retained Block, track both production and expenditure against enemy attacks.
End-of-turn Block arrives after player actions; it helps a later Body Slam only
to the extent it survives the intervening attacks.
Calculate the reserve after incoming damage. With no other effects, gaining
3 end-of-turn Block against a 20-damage attack prevents 3 damage and leaves
0 Block for the next turn. Retaining Block preserves a positive balance; its
growth depends on production exceeding actual expenditure.
Evaluate Strength changes per hit and over all later attack windows. Reducing
Strength by 2 reduces each affected hit's base damage by 2; on 15 hits that is
30 before Weak, Vulnerable, Block and Torii. Check Artifact and any enemy
debuff-clearing phase before deciding when to apply the reduction.
For a Strength-reduction upgrade, count the extra reduction on each affected
hit across its actual duration, then apply damage floors, modifiers and Torii.
Artifact may prevent either version from applying; compare how it can be
removed. Ordinary enemy Strength gains add to the reduced value. Distinguish
those gains from an explicit effect that removes negative Strength.
For temporary Dexterity, count all affected card-generated Block events,
including each card Exhausted by Second Wind. With no Frail and other modifiers
fixed, +5 Dexterity adds 5 Block to Power Through and 20 more if Second Wind
then Exhausts four cards. Barricade can preserve the unspent Block after the
Dexterity expires; subsequent Body Slams can use that remaining reserve.

Recheck these functions as the deck changes. Each added card changes both its
own function and access to existing cards. For a large deck, identify the draw
and independent cards that make its early hands work. Ask what covers a key
power arriving one cycle late, and what resources remain after that fallback.
Use a concrete likely delay to test the plan.

Count each enabling power's payment before the free skills or triggered gains
it unlocks. A Mummified Hand discount applies to an eligible card selected by
its actual trigger. On a low-pressure turn, compare conserving resources with
drawing toward setup; inspect what another draw can reach and pay for.

Follow modifier amounts through their actual timing. Flex adds Strength and
queues removal of that same amount at turn end. Limit Break played afterward
doubles current Strength; the queued removal still removes its original amount,
so some extra Strength remains. Count this sequence only when both cards can
be accessed and played together. Unplayable cards still occupy draws and hand
space; an Ethereal card leaves the future cycle only after it Exhausts.

Compare upgrades under the same Strength, Energy, targets and other modifiers.
A Smith action changes one selected copy. Count the marginal improvement of
that copy, how often it is accessed, and its effect on the next threat.
For draw upgrades, compare the added access and usable resources in the current
engine. Separate the card's existing output from the upgrade's added output.

For combat diagnostics, inspect setup timing, early losses, resource use and
damage pace in each fixed case. Use those concrete outcomes to investigate the
weak function and a feasible preparation change.
Locate key deployments in the actual turn-by-turn review when explaining a
line's result; the final HP alone does not establish when setup happened.
The compact review shows each turn's actual starting Block and powers and its
Block before the End Turn action. Follow the next turn's starting reserve to
see what survived turn-end effects and enemy actions. For a potion explanation,
check the successor's actual powers. Each Artifact charge blocks one debuff
application: a move applying Vulnerable, Weak and Frail contains three separate
applications. Identify which effect was blocked and which still applied from
the actual following-turn review.

For a future threat, separate current resources from resources expected at
arrival. If an attack scales with player HP or a status, calculate it when
that value becomes known. Compare block, debuffs, HP and potions as protection
alongside the damage pace needed before enemy scaling overwhelms the deck.

## Exhaust engines: access, payment and fuel

Sentinel supplies block and, when Exhausted, energy. Link that energy to actual
draw sources and payable cards. With Corruption active, used Skills cost zero
and Exhaust; track which resources survive into later cycles.
For a conditional payoff, identify the affected card/entity and the action
that can produce the condition. Sentinel needs Sentinel itself to Exhaust;
another card's self-Exhaust does not Exhaust Sentinel. Name a usable source
such as a targeted Exhaust effect or deployed Corruption before counting that
energy in the current deck.

Dark Embrace draws when a card Exhausts while the power is active. Plan how to
reach and pay for it, then establish the exhaust triggers. Burning Pact normally
Exhausts another card and supplies its own draw. With Dark Embrace active, that
target supplies an additional draw. Pact can recur; track its actual available
hand targets as fuel. Enemy-generated Ethereal Statuses can also supply triggers,
for example Sentries' Dazed with Feel No Pain. Query the current enemy and
Status mechanics when using this interaction in a preparation decision.

Deploying Corruption can spend the last Energy while leaving zero-cost Skills
playable on that turn. Compare the immediate playable hand with the value of
preserving paid Skill cycles for later. Count block, draw and damage after the
exhaust fuel is spent. A reusable attack/relic cycle, such as upgraded Pommel
Strikes with Sundial, needs enough net draw, energy, hand space and reshuffling
in the actual deck.

Battle Trance draws first, then blocks further draw that turn. Place other draw
before it when needed. Corruption also Exhausts Battle Trance and Limit Break+;
plans using repeated copies must account for their access or recovery. Compare
one strong Strength doubling with delaying Corruption for further doublings.

## Scope, routes and resources

Anger's copies belong to the current combat. Evaluate its zero-cost damage in
an energy-limited opening and its effect on later combat draws. Query a card's
scope and flags before projecting its effects into the permanent deck.

Count opportunities along one connected future path. Parallel nodes are
alternative routes. When deferring a key, name its remaining acquisition
opportunity. Compare Recall with the best available campfire action and the
remaining campfires.
At a route change, check key reachability from the chosen branch again. A
nearby elite can lead to an entirely different downstream path; match its
actual exits to the required key before treating it as an extra-reward detour.

For a three-Attack relic such as Kunai, count playable Attack cards, Energy and
access alongside defense/setup demands. A multi-hit Attack counts as one card.
For retaliation, use the actual trigger: Guardian's Sharp Hide responds once
per Attack card. Compare the gained scaling with that turn's resource and HP
cost.

When planning a later shop, track the current balance, purchases and rewards
along the connected route; update with actual income as it is received. Inspect
the later shop's actual stock before assigning that money to a specific item.
Act 3 and Act 4 bosses advance without a normal reward screen: their room gold
is not claimable. Budget the Act 4 shop from other actual reward income.

## Preparing for the Heart

Query the actual Heart rules. Check the first attack pair after its debuff turn,
including Vulnerable, Frail, added Statuses and Beat of Death. Identify protection
before the engine starts, continuing block after one-shot protection is spent,
and the real damage pace under the per-turn damage limit.

Match reserved potions to specific gaps: reaching setup, surviving a dangerous
turn, or accelerating damage. Consider replacement opportunities and slot
pressure. Evaluate the remaining shops, rewards and upgrades together; connect
each available preparation to a missing deck function.
Compare potion combinations as well as individual bottles. Artifact from an
Ancient Potion can block Speed Potion's Dexterity Down when applied before
Speed, leaving its Dexterity gain for the combat. Track any intervening debuff
that could consume the Artifact first. Count the resulting card-generated
Block over affected turns, including each separate Second Wind Block event
and Frail, then compare with the inventory and Gold spent on both bottles.
For damage-based healing, estimate actual HP damage after enemy Block and
damage-limiting powers, then apply healing modifiers. With zero Strength,
an unupgraded Reaper hitting one enemy for 4 actual HP damage heals 6 with
Magic Flower. It Exhausts, so this is one heal from that copy until an owned
effect returns it. Treat it as an available recovery window and count how its
2 Energy payment fits that turn's defense. Recompute if Vulnerable, Strength,
Weak, multiple targets or the Heart's remaining damage allowance changes.
Price a potion replacement using the currently held and actually offered
potions. Future shops are opportunities to inspect new stock. Check a setup
potion's marginal Energy against the deck's current ordinary Energy and actual
enabling-power payments when comparing it with immediate protection.

Evaluate extra damage under the Heart's per-turn cap by how much damage budget
remains in that turn. Automatic retaliation can fill unused budget, reduce the
attacks needed to reach the cap, or finish a later turn. Compare its contribution
across both attack turns, including setup turns with little player damage.
For a block-and-draw Skill, count its actual block, Beat of Death cost, active
Exhaust payoffs and cards accessed; its marginal value depends on that complete
sequence and whether it recurs or is spent.
