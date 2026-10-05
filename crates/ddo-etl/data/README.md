# ETL data files

The vocabularies the ETL maps Maetrim's DDOBuilderV2 files through. Each is embedded in the binary at compile time, so a change here needs a rebuild, and a value one of them cannot place fails the build on purpose.

- `effect_map.toml`: his item buff families and effect types, as stats, bonus types and effects.
- `legacy_drop_sources.toml`: the names in his drop text of quests that no longer exist, so the items only they dropped are flagged legacy (below).
- `source_aliases.toml`: the station or place names his drop text uses for a source recorded under another name: a crafting system's station, the place a challenge pack's rewards are turned in, or the place a vendor takes a turn-in (below).
- `corrections/`: his values a ddowiki page contradicts; see [`corrections/README.md`](corrections/README.md).
- `wiki/`: facts read from ddowiki that his files have no field for; see [`wiki/README.md`](wiki/README.md).

## `effect_map.toml`

`effects` is the single table for named effects, groups and stats. The seeded stat ids stay fixed; `is_stat = 1` rows have a `category`, `is_group = 1` rows name reusable groups, and all other effects are numbered after the stats. Names are unique across all kinds. A same-named effect granting only that stat is the stat row itself. Speed, Deception and Command are named effects that grant other stats. Attack Speed is split into Melee Attack Speed and Ranged Attack Speed. `effect_bonuses` targets a stat or a group; a group contains only stat members reading slot 1 and the link's type, with no nested groups. `owner_bonuses` expands group targets one level and retains `group_effect_id` on every expanded bonus. A direct link to a stat row already states its stat, value and bonus type. A direct stat link must carry a type and exactly one value slot. Its missing value is a WARN until corrected. Stat 135, Physical and Magical Resistance Rating, was retired; Sheltering now groups Physical Resistance Rating and Magical Resistance Rating.

When a buff definition has no usable display title, its effect name comes from the buff type split into words, without `Number`, `Numeral` or `Roman Numeral` markers. A skill group's description states its ability, amount and link bonus type.

`[family]` maps item `<Buff>` types; its `owner_type_precedence` list records source-kind and definition/text disagreements where the owner type wins over the definition. `[effect.fixed]`, `[effect.targeted]`, `[effect.by_item]` and `[effect.by_item_default]` map their `<Effect>` types and effects on feats, augments, sets and options. `[effect.targeted]` maps exact target words to stat lists, such as `All` attack alacrity to both melee and ranged speed. A target list covering every bow, every crossbow and Dart is the ranged class; other thrown weapons are not required. A list covering every melee weapon type is the melee class. A shorter weapon list stays qualified text with its melee or ranged kind. `[effect.companion_targets]` drops a companion word only when a sibling target maps; an unknown target keeps the whole line as text. `[effect.energy_target_artifacts]` lists stats on which an energy qualifier contradicts the stat's scope. `[effect.engine_only]` names mechanics that do not block mapped siblings. `[names]` resolves family-name collisions, `[tier_groups]` gives ordered steps, and the alias sections normalize Maetrim's item, weapon and bonus-type words. A class, weapon, school, energy or alchemical target in the qualifier vocabulary keeps the effect as qualified text unless it contradicts the stat. His other targets are treated as copy-paste artifacts and map without a qualifier.

`[groups]` declares reusable stat lists; a group row is materialized only when an owner or effect grants it. `[skill_ability_groups]` records the curated skill-to-ability list from [DDO Wiki Skills](https://ddowiki.com/page/Skills), read 2026-10-05. `SkillBonusAbility` with an ability target links its matching skill group directly, carrying the owner's value and type; no same-meaning wrapper effect is written. Command grants Charisma Skills from slot 1, plus Hide Penalty from slot 2. All Ability Scores and Sheltering are groups; Melee and Ranged Power and Doublestrike and Doubleshot are declared groups that appear only if the build finds a source granting them through one amount and type source. `/v1/effects` lists groups with `kind=group`, and `bonus=<group name>` or any member stat finds owners through the expanded view.

The family stores its text and description templates and optional defaults. The amount count is derived from `{1}` and `{2}` in the templates; a stat row takes one value. A link stores only what its owner states, may omit a trailing amount, and can have no first value when Maetrim omits it. The effective amount for each slot is `COALESCE(link value, effect default)`. An unresolved slot contributes no bonus and renders empty. `effect_links_missing_first_amount` lists named-effect links with no first value or default. A text-only effect can carry values; `effects_text_only_with_item_values` lists those candidates for a stat decision. A static bonus uses `amount_from = 0` and its final `constant`; a slot-reading bonus may use positive `scale` with `down`, `up` or `nearest` rounding. `owner_bonuses` is the shared view over every owner link and stat rule. It omits resolved zero amounts because zero grants no bonus; `effect_links_with_only_zero_bonuses` lists those owner links for review. Its `amount_source` is `owner`, `default` or `constant`; `scale` records a derived amount's multiplier. A stat's bonus type comes from its link. A named effect's `%b1` templates take the link type; without `%b1`, a real fixed `<Bonus>` in the definition wins over the owner's default `<BonusType>`. An owner supplies the type when the definition says `Not Set` or source kinds define different types for the same stat, such as Illusion Save and Lay on Hands Uses. Each owner keeps its own type, never a union of other owners' types.

Items, augments, feats, augment-slot options and set tiers all use the same effect identity and cached writer. Six ability bonuses of one type and value use `All Ability Scores` with six stat rules; other set facts use their respective effects, while prose with no structured bonus remains text-only. `effect_tier_groups` groups plain effect steps. A set tier can carry prose and structured lines when they state separate facts. `triggers` names events from Maetrim's “On …” display prefixes, separately from future state conditions. `effect_damage` holds typed dice on a trigger: a stated range from minimum to maximum is 1d(max−min+1)+(min−1), and `amount_from` 1 or 2 multiplies the roll by that link slot and `scale`. A nullable `modifiers.effect_id` records which effect a raw modifier resolves to. `effects_with_dice_but_no_damage_rows` groups dice modifiers lacking damage rows, including those with no resolved effect, by source kind and effect type; dice backfill remains a later data pass.

The map loader rejects unknown stats and aliases, conflicting mappings, invalid templates, and repeated tier ranks. The writer fails when a mapped bonus lacks a type, naming its owner, effect and stat; a cited `bonus_type` correction can supply the missing type. Corrections for item, augment and set bonuses change links or stat rules through the same cached writer, and `effect_damage` corrections add or remove cited dice rows. The build preserves wiki-created entries only until Maetrim carries the same corrected name; his row then wins and the wiki entry is removed.

## Items the build leaves out or flags

His `Items/` files carry some items that a player cannot get or that carry nothing a build uses. The build writes none of an item whose slots are all cosmetic, nor of a cosmetic shield (his `Cosmetic Shield` weapon type in the off-hand, which has no shield bonus, armour or effect), and records each in `excluded_items` with its reason (`cosmetic-only slots` or `cosmetic shield`), so a consumer can tell an item left out on purpose from one that is missing. A wiki vendor or event that lists an excluded item fails the build, naming the reason, so the listing drops it.

An item that once dropped but no longer does is written in full and flagged instead: `items.is_legacy = 1`. The build flags an item when its `DropLocation` has at least one segment (the text between `;` or line breaks) and every segment names a text in `legacy_drop_sources.toml` and no quest, quest chain, saga or adventure pack of his or of a wiki file. An item with any other segment is current, and its legacy segments simply link nothing. A legacy text that later becomes a quest's name again (a quest correction renaming one, say) is a current quest in every segment that names it.

Two other kinds of legacy item are flagged the same way. An item whose name carries `(legacy)` or `(historic)` as a parenthesised word, ignoring case, is an old version his files keep beside the current one (`Brawling Gloves (legacy) (level 12)`, `Allegiance (historic)`), whether it comes from his files or a wiki items file. An old version the wiki says was replaced and no longer drops, under a name that does not say so, is flagged by an `is_legacy` correction (`from = 0`, `to = 1`) citing the page; see [`corrections/README.md`](corrections/README.md). `/v1/items` leaves every legacy item out of its rows and `total` unless `include_legacy=true`, `/v1/items/{id}` serves one with `is_legacy: true`, `/v1/version` counts them as `legacy_items`, and the build report as `legacy_item_count`.

## `legacy_drop_sources.toml`

The case the file was made for: Update 56 retired the quests Temple of Elemental Evil Part One and Part Two and replaced them with the Temple of Elemental Evil area quests, whose loot tables differ. His files no longer carry the two quests, but 553 of his items still say `Temple of Elemental Evil Part One, and Temple of Elemental Evil Part Two any chest`, `Temple of Elemental Evil Part Two, Elemental nodes chests` and the like, and those are the items flagged.

```toml
[[legacy]]
text = "Temple of Elemental Evil Part One"
reason = "The quest was retired in Update 56, replaced by the Temple of Elemental Evil area quests, which have their own loot tables."
source = "https://ddowiki.com/page/Temple_of_Elemental_Evil_Part_One"
read = "2026-10-02"
```

- `text`: the name as his segments spell it. It is matched as quest names are: longest first, as whole words, ignoring the case of every letter after the first.
- `reason`: why the source no longer drops loot, in one sentence.
- `source`: the `https://` URL that shows it.
- `read`: the date the source was read, `YYYY-MM-DD`.

A name his text uses for a source that still exists under another name is not a legacy source. Animus says `Devil Assault (quest), End Chest`, while his quests are `Devil Assault (Normal)`, `(Hard)` and `(Elite)`: the segment links to the pack Devil Assault, and the wiki loot file links the item to the three quests.

The build fails, naming the entry, when the file has a field other than these, `text` or `reason` is empty, `source` is not an `https://` URL, `read` is not a real date, or the same `text` appears twice ignoring case. `cargo xtask wiki-check` prints `legacy_source_flagged_count`, the items flagged, and `wiki-batch`'s `unlinked_drop_segments.txt` leaves out every segment that names a legacy text.

## `source_aliases.toml`

His drop text often names the place an item is made rather than the crafting system that makes it: `Magma Forge, Crafted from various ingredients` is Thunder-Forged crafting, and `Lahar, Turn in Nebula Fragment` is Nebula Fragment Crafting. A segment that names a wiki crafting system (`crafting_systems.name`, as whole words, ignoring the case of every letter after the first, longest first) links the item or augment to it as a `crafting_system` source (`Catalyst Crafting, Turn in ...`, `Viktranium Experiment crafting, Turn in ...`); this file maps the other names to a system, matched against the segment's head, the text before its first comma, parenthesis or colon, ignoring case.

```toml
[[crafting_system]]
text = "Magma Forge"
system = "Thunder-Forged"
reason = "The Magma Forge in the Ruins of Thunderholme is where Thunder-Forged items are crafted."
```

- `text`: the head as his segments spell it.
- `system`: a crafting system's name in `wiki/crafting*.toml`; an alias whose system the wiki files lack links nothing.
- `reason`: why the name stands for that system, in one sentence.

The build fails, naming the entry, when the file has a field or table other than these, a field is blank, or the same `text` appears twice ignoring case (with the same `contains`, for a vendor alias). A test checks that every `system` names a crafting system in the embedded wiki files. The build report counts the rows written as `drop_text_crafting_system_source_count`, and `wiki-batch`'s `unlinked_drop_segments.txt` leaves out every segment that links one.

Every crafting series his drop text names is a system in `wiki/crafting*.toml`, so each such segment links its loot; the "Restored" Dragoncraft, Elfcraft and Giantcraft armours (`... Turn in 10 Restored Dragon Relics` and the Elven and Giant equivalents) link to their system by that head, though the wiki has no recipe page for them.

A `[[challenge]]` entry (`text`, `pack`, `reason`) does the same for a challenge pack, an adventure pack of Maetrim's `Challenges.xml`: a segment whose head is its `text` links the item or augment as a `challenge` source of that pack, the reward its challenges' ingredients or commendations buy. His text names no challenge, only the place: `Vaults of the Artificers, Turn in various challenge ingredients` (the House Cannith challenges, whose pack in his files is Secrets of the Artificers; nine of them are in Free to Play, which the link does not name) and `Eveningstar, Turn in 5 Commendations: ...` (Eveningstar Challenge Pack). The build report counts `drop_text_challenge_source_count`, and lists in `unresolved_source_aliases` the texts of every alias whose system or pack the build lacks.

A `[[vendor]]` entry (`text`, an optional `contains`, `vendor`, `reason`) links the item or augment as a `vendor` source of a wiki vendor (`wiki/vendors*.toml`) when the segment's head is its `text` and, if `contains` is given, the segment holds that text too, ignoring case. `contains` tells apart two vendors at one place: `Blue Water Inn, Turn in 40 Vistani Talismans` is Raam Lukresh's and `Blue Water Inn, Turn in 60 Vistani Totems` Osah Lukresh's, and `Necropolis, Turn in Dark Scarab Powder` is Squire Rale's while other Necropolis heads are not. Two entries may share a `text` only with different `contains`. A segment that names a wiki vendor itself (`Necropolis, Turn in Eerie Scarab Powder to Squire Rale`) needs no alias. The build report counts `drop_text_vendor_source_count`, a test checks that every `vendor` names a wiki vendor, and `unresolved_source_aliases` lists the texts of the ones the build lacks.

No alias is needed for an iconic hero's starter gear: a segment whose head is `Advance to level N` links the item as a `starter` source with `character_level` N (his text gives only `Advance to level 15, End reward`), counted as `drop_text_starter_source_count`. The level is the only fact his text gives, so the kind needs no table of its own.

A `[[crafting_system]]` entry may also carry `min_minimum_level` and `max_minimum_level` (either or both, inclusive): it then links only loot whose minimum level falls in that range, so one station can stand for two systems. `Altar of Fecundity, Manufactured Ingredient Recipes` (93 items) is the case: the Altar makes the blanks of both Green Steel items (minimum level 11 or 12, so `max_minimum_level = 20`, 45 items) and Legendary Green Steel items (minimum level 26, so `min_minimum_level = 21`, 48 items). Two entries may share a `text` only when their level ranges do not overlap, and the build fails on a `min_minimum_level` above the `max_minimum_level`.
