# Wiki overrides

Facts read from [ddowiki](https://ddowiki.com) that Maetrim's DDOBuilderV2 files have no field for. The ETL embeds every `*.toml` in this directory and merges it after his files are written.

## The principle

A wiki row may **add** a fact his files have no field for; it never replaces one he has. When the wiki and his files disagree about something he carries (a name, a level, a bonus, a socket, a drop location), the wiki is assumed right, but the fix goes in [`../corrections/`](../corrections/README.md), the one layer allowed to override him, and never here; report the disagreement upstream at [Maetrim/DDOBuilderV2](https://github.com/Maetrim/DDOBuilderV2) too. Corrections run before the wiki merge, so the files here name rows by their corrected names.

## Entities his files lack

A wiki entry may also supply a whole row his files could have written but do not, so the thing exists in the database until he adds it. Such a row carries `provenance = 'wiki'` (his rows are `provenance = 'maetrim'`), and the rule is the same for every kind: **his row wins by name**. The moment his files carry a row of that name, the wiki entry creates nothing, is counted as superseded, and `cargo xtask wiki-check` warns that it can be deleted. An entry whose name matches one of his only once case, punctuation, spaces and a trailing `(level N)` are ignored is still created, and is reported as a probable duplicate so a person can decide whether it is the same thing. Corrections never apply to a wiki row, and the API reports every such row's `source`.

This applies to items (`items*.toml`), quests (`quests*.toml`) and augments (`augments*.toml`) today, and any future kind his files lack is added the same way: a `provenance` column, the fields his rows carry, the supersede and probable-duplicate counts, the `wiki-check` warnings, and a `wiki_source_<kind>.txt` list from `cargo xtask wiki-batch`. Wiki quests are created before anything else reads the quests table, so an item entry, a `quest_loot` entry or a quest's own facts may name a wiki quest. Wiki augments are written before the rest of the wiki merge, so a crafting recipe's `augments` and a `quest_loot` entry's `rare_augments` may name one. Maetrim's own items link to a wiki quest their drop text names exactly as they link to his, with the same `loot_type` and `is_rare` rules, and `wiki-check` counts those links as `drop_text_wiki_quest_link_count`.

## Citing the source

Every entry names the page it was read from (`page`, a full `https://ddowiki.com/page/...` URL) and the date it was read (`read`, `YYYY-MM-DD`). Re-reading a page means updating that entry and its `read` date, not adding a second one. Read pages in a real browser, one page per navigation at a human pace; ddowiki's bot challenge blocks `api.php`, curl and scripted clients, and working around it is off limits. Wiki content is CC BY-SA.

## `quest_loot.toml`

One `[[quest]]` table per quest page. `rare` lists the named items the page's loot table marks as rare for that quest, and `rare_augments` the augments it marks rare, by his exact augment name (every augment row of that name is marked, since a name can repeat across families). The build already sets `is_rare` where Maetrim's text marks the drop itself: an item's drop location, or the text after `Drops in` (or `Drop Location:`) in an augment's description, has a segment (split on `;` and newlines) naming the quest with `(rare)` or `rare drop` and not `rare encounter`. An entry lists every rare drop the page marks for the quest, whether or not his text marks it too, so the file doubles as the record of what was read.

Each element of either list is a name, or `{ name = "...", chest = "..." }` when the page says which chest drops it, lower-cased as the build writes his chests (`"end chest"`, `"optional chest"`, `"althea's chest"`).

```toml
[[quest]]
name = "Book Burning"
page = "https://ddowiki.com/page/Book_Burning"
read = "2026-09-27"
rare = [
  "Buckler of the Golden Age",
  "Docent of the Crownblade",
  "Legendary Buckler of the Golden Age",
  "Legendary Docent of the Crownblade",
  "Legendary Robe of the Warblade",
  "Robe of the Warblade",
]
rare_augments = [{ name = "Lunar Gem of Magical Protection (Heroic)", chest = "end chest" }]
```

`items` lists named items the page's loot table gives for the quest that Maetrim's drop text does not link to it, and `augments` the same for augments (by his exact augment name, every row of that name). Each element is a name, or `{ name = "...", loot_type = "...", chest = "..." }` with `loot_type` one of `chest`, `raid` or `reward` (default `chest`; `reward` is the quest's own end reward, while a quest chain's or a saga's end reward goes in `quest_chains.toml` or `sagas.toml`, not here) and `chest` optional, lower-cased as above (never on a `reward`, which the quest's end gives rather than a chest; the build fails on one). A listed drop is not rare; list it in `rare` or `rare_augments` too when the page marks it rare.

```toml
[[quest]]
name = "Zawabi's Revenge"
page = "https://ddowiki.com/page/Zawabi%27s_Revenge"
read = "2026-10-01"
items = ["Ring of Baphomet"]
```

A quest links an item or augment once per loot type, so one item can be both a chest (or raid) drop and an end reward of the same quest. His text gives a quest's link a loot type from each segment naming the quest: `raid` for every segment when the quest is a raid; otherwise `reward` when the segment says `reward` and does not say `chain` or `saga` (a quest chain's or a saga's end reward comes from another NPC after several quests, not from this quest), and `chest` when the segment does not name the quest's own reward or also says `chest`. `The Tide Turns, End Chest, End Reward` makes a chest link and a reward link; `Project Nemesis, end chest; Masterminds of Sharn saga: Epic end reward` makes only Project Nemesis's raid link.

The merge writes the `items` and `augments` links first, and never changes a link his text made. An element with a `loot_type` adds a link of that type with its `chest` and `is_rare = 0` unless the quest already links the item or augment with that type, so a listed `reward` sits beside his `chest` link; an element without one adds a `chest` link only when the quest links the item or augment with no type at all. It then sets `is_rare` on the quest's `sources` rows for each (quest, item) and (quest, augment) pair, after his text has set its own; it never clears the flag, and it marks the pair's chest or raid link, or its reward link when that is the only one. When the quest does not link the item or augment at all, it adds the link with `loot_type = 'chest'`; an added link is an added fact. It never changes a `loot_type` his text produced, and it writes a `chest` only where his text named none: the build takes the chest from his segment (the phrase after the quest name, without the rarity marker or parenthetical asides, lower-cased; quests listed together share the chest named after the last of them).

A segment that names an adventure pack and no quest (`Magic of Myth Drannor, any end chest`, `The Isle of Dread, any legendary chest`) is loot from any quest of the pack, and the build writes it as an `adventure_pack` drop of that pack, with its `loot_type`, `chest` and `is_rare` read from the segment by the rules above; an item's quest-, chain- or saga-specific segments are linked first. Pack names are matched as quest names are (longest first, ignoring case, as whole words), and a quest name inside a longer pack name belongs to the pack: `The Isle of Dread, any chest` is the pack The Isle of Dread, not the wilderness quest Isle of Dread. A segment links to no pack when a recorded chain or saga takes it as an end reward, when it says `saga` or `True Elite` (a saga's reward lists), when a saga's name around the pack name is followed by a tier aside (`The Chill of Ravenloft (Heroic), any difficulty`), or when it names a store purchase (`DDO Store`, `Collector's Edition`, `Fan Bundle`, `Bonus Items Pack`). The report's `pack_loot_link_count` and `pack_augment_loot_link_count` count the pack links his item and augment drop text made. A segment naming a quest that no longer exists, listed in [`../legacy_drop_sources.toml`](../README.md), links nothing, and an item whose every segment names one is flagged `is_legacy`. A wiki item's own drop text is not read for links; its `quests` field links it to quests only.

## Validation

The build fails, naming the file and entry, when:

- a file has a table or field other than the ones above;
- `page` does not start with `https://ddowiki.com/page/`;
- `read` is not a real `YYYY-MM-DD` date;
- the same quest `name` appears twice, in one file or across files;
- `name` matches no quest in Maetrim's `Quests.xml` or `Challenges.xml` or a quest a `quests*.toml` entry creates (names must match his exactly);
- an item in `rare` or `items` matches no item in his `Items/` or an `items*.toml` entry, or an augment in `rare_augments` or `augments` no augment in his `Augments/` or an `augments*.toml` entry. Items and augments missing from both are reported upstream or given their own wiki entry, not stored here;
- an element of `rare` or `rare_augments` is neither a string nor a table of exactly `name` and `chest`;
- an element of `items` or `augments` is neither a string nor a table of `name` with optional `loot_type` and `chest`, or its `loot_type` is not `chest`, `raid` or `reward`.

Validate a file from the `ddo-data` root:

```bash
cargo xtask wiki-check
```

A clean run prints the report, whose `wiki_quest_loot_entry_count`, `wiki_rare_drop_count`, `wiki_added_quest_loot_link_count`, `wiki_rare_augment_drop_count`, `wiki_added_quest_augment_loot_link_count`, `wiki_loot_drop_count` (elements of `items`) and `wiki_loot_augment_drop_count` (elements of `augments`) count what was applied (the two `wiki_added_*` counts include the links `items` and `augments` added), `quest_augment_loot_link_count` counts the quest links his augment descriptions made, `pack_loot_link_count` and `pack_augment_loot_link_count` the pack links his items' and augments' drop text made, and `drop_text_rare_link_count` and `drop_text_rare_augment_link_count` count the rare links his text marked on its own. The tests use their own overrides in `crates/ddo-etl/tests/fixtures/wiki/`, which only name what the fixture data files carry.

## `quests.toml`

One `[[quest]]` table per quest, carrying the quest facts Maetrim's `Quests.xml` has no field for. The loader reads a file by its name: `quest_loot*.toml` holds loot entries, `quests*.toml` holds these, `crafting*.toml` holds crafting systems, `descriptions*.toml` holds descriptions, `items*.toml` holds items, `augments*.toml` holds augments, `quest_chains*.toml` holds quest chains and `sagas*.toml` holds sagas, so a quest may appear once in each of the first two; any other name fails the build. The first 548 entries were read from the one index page, [Quests by level and XP](https://ddowiki.com/page/Quests_by_level_and_XP); a later read of a quest's own page updates its entry, `page` and `read` and adds the per-page fields.

```toml
[[quest]]
name = "A Blood Pact"
page = "https://ddowiki.com/page/Quests_by_level_and_XP"
read = "2026-09-28"
free_to_play = false
legendary_level = 37
```

- `free_to_play` (required): whether the quest itself is free to play, which the wiki marks per quest; `adventure_packs.is_free_to_play` stays Maetrim's per-pack flag.
- `legendary_level` (optional): the legendary version's level. Heroic and epic levels are Maetrim's and are not repeated here.
- `zone`, `bestowed_by`, `flagging` (optional, per-page reads): the page's "Takes place in" value, the quest giver, and free text on what must be run first.

The merge fills `quests.is_free_to_play`, `legendary_level`, `zone`, `bestowed_by` and `flagging`. It never writes the columns Maetrim's files fill (`level`, `epic_level`, `pack_id`, `patron_id`, `favor`, `is_raid`, `difficulties`) on one of his quests. Besides the checks above, the build fails when `free_to_play` is missing or a key is not one of those listed; quest duration and XP are deliberately not kept, so a `duration` or `xp` key fails as an unknown field. The report's `wiki_quest_entry_count` counts what was applied.

### Quests his files lack

An entry for a quest Maetrim's `Quests.xml` and `Challenges.xml` lack creates it, following [Entities his files lack](#entities-his-files-lack), when it carries the fields his quests have besides the facts above:

```toml
[[quest]]
name = "Some New Quest"
page = "https://ddowiki.com/page/Some_New_Quest"
read = "2026-09-30"
free_to_play = false
pack = "Vecna Unleashed"
patron = "The Free Agents"
level = 32
favor = 150
is_raid = false
difficulties = ["normal", "hard", "elite", "reaper"]
legendary_level = 34
```

- `pack` (required to create): the adventure pack, spelled as his `adventure_packs.name` (what `/v1/adventure-packs` lists); it must already exist.
- `patron` (optional): a patron from his `Patrons.xml`, as `/v1/patrons` lists it.
- `level` (required to create), `epic_level` (optional): the heroic and epic levels, as his `<Levels>` gives them.
- `favor` and `is_raid` (required to create).
- `difficulties` (required to create): the difficulties offered, each `casual`, `normal`, `hard`, `elite`, `reaper` or `solo`.

Giving any of these makes all the required ones required; the build fails, naming the file, the quest and the missing field or the value, when one is missing, a difficulty is not one of the six, or `pack` or `patron` matches nothing in his files. A created quest is written with `provenance = 'wiki'` and counted in `wiki_quest_created_count`, before his items are written, so an `items*.toml` entry's `quests` may name it. An entry with these fields whose name is one of his quests creates nothing and is counted in `wiki_quest_superseded_count`; its facts (`free_to_play` and the rest) still apply to his quest as above, and `cargo xtask wiki-check` warns `warning: wiki quest "<name>" is now in Maetrim's files; delete its quest fields from <file>`. A probable duplicate is counted in `wiki_quest_probable_duplicate_count` and warned as `warning: wiki quest "<name>" may duplicate Maetrim's "<his name>"`. `cargo xtask wiki-batch` lists the quests still supplied by the wiki in `wiki_source_quests.txt`, and its `quest_pages.json` includes them. The API reports each quest's `source`, and `/v1/version` counts them as `wiki_quests`.

## `crafting.toml`

One `[[system]]` table per wiki crafting page, carrying what the wiki adds to Maetrim's crafting data: the ingredients and what each recipe costs. His files already model a system's *options* as augments (`augments.family` groups them: `Slavelords_Heroic`, `Greensteel_Legendary`, `Alchemical`, ...) and an item's crafting sockets as `item_augment_slots`, so a system here only points at his families and names his augments; it never adds an augment, an item or an innate item bonus. Upgrade and ingredient-only systems (altar tiers, "add an augment slot" rows, material steps) have no augment family: he models an upgrade as a socket on the item (`upgrade: tier 2`, `crafting: slavelords extra`), so their recipes either grant that socket with `grants_slot` or carry a `note`, and an upgraded state is never written as a new item. `crafting*.toml` files hold these; split a large system into its own `crafting_<system>.toml` when that reads better. An illustrative entry, in the shape the reading agents write:

```toml
[[system]]
name = "Slave Lords Crafting"
page = "https://ddowiki.com/page/Slave_Lords_Crafting"
read = "2026-09-27"
pack = "Against the Slave Lords"
families = ["Slavelords_Heroic", "Slavelords_Legendary"]
npc = "Ingredient Bag Vendor"

[[system.ingredient]]
name = "Broken Shackle"
tier = "heroic"
bind = "Unbound"
source = "Slave Pits of the Undercity (Part 1)"

[[system.recipe]]
tier = "heroic"
slot = "crafting: slavelords prefix"
option = "Attributes +5"
augments = ["Charisma +5", "Constitution +5", "Dexterity +5"]
cost = [{ ingredient = "Broken Shackle", quantity = 50 }]
```

- `name` (required, unique across files): the system as the page names it.
- `pack` (optional): the adventure pack or expansion, spelled as Maetrim's `adventure_packs.name` (what `/v1/adventure-packs` lists).
- `families` (optional): the augment families this system's options live in, each the name of one of his `Augments/*.Augments.xml` files before `.Augments.xml`. Leave it out (or empty) for a system with no augment family; then every recipe's `augments` must be empty.
- `npc` (optional): the crafting NPC or station, free text.
- `[[system.ingredient]]`: `name`, `tier` (`heroic`, `epic`, `legendary` or `any`), and optional free-text `bind` and `source` as the wiki writes them. A name may repeat at different tiers, never at the same one.
- `[[system.recipe]]`, one per row of the page's recipe tables, in page order: `tier` as above; `slot` (optional), the socket label as `/v1/augment-slot-types` lists it; `option`, the wiki's label for the row; `augments`, the names of his augments in the system's families that the row yields; `grants_slot` (optional), the socket label, as `/v1/augment-slot-types` lists it, that the row adds to the item (`colorless` for a "Colorless Augment Slot" row, `upgrade: tier 2` for an altar's second tier); `note` (optional) free text; `cost`, one `{ ingredient, quantity }` per ingredient.

An augment name resolves to every augment of that name in any of the system's families, because names repeat within a family (Green Steel has one `+5 Fortitude Save` per element combination). A cost's `ingredient` names one declared in the same system; when that name is declared at several tiers, the one at the recipe's tier is used, then the one at `any`. `augments` may be empty when the wiki row has no augment counterpart (cleansing an item, a material step, adding a socket), but then `grants_slot` must name the socket it adds or `note` must say why.

The merge writes `crafting_systems`, `crafting_system_families`, `crafting_ingredients`, `crafting_recipes` (with `sort_order` the recipe's position in the entry and `grants_slot_id` the granted socket's `augment_slot_types` row), `crafting_recipe_augments` and `crafting_recipe_ingredients`. Besides the citation checks above, the build fails, naming the system and the value, when a system has a field or table other than the ones above; a required field is missing; a system without `families` has a recipe listing `augments`; a tier is not one of the four; an ingredient repeats at one tier; a recipe has no `augments`, no `grants_slot` and no `note`; a cost names an undeclared ingredient, repeats one, or has a quantity below 1; or `pack`, a family, a `slot` or `grants_slot` label or an augment name matches nothing in Maetrim's files (an augment name may also be a wiki augment's, written in one of the system's families by an `augments*.toml` entry). The report's `wiki_crafting_system_count`, `wiki_crafting_recipe_count` and `wiki_crafting_ingredient_count` count what was applied.

To check a draft without embedding it, point the check at a directory holding only that file: `cargo xtask wiki-check --wiki /path/to/draft-dir`. It reads that directory in place of the embedded files, builds into memory, and prints the wiki counts or the validation error in a few seconds. After the counts, a `warnings:` section lists each augment in a system's `families` that no recipe in that system yields (`warning: <system>: <family> augment "<name>" has no recipe`); a warning never fails the check, and many are expected (drop-only augments, a shared family such as `Named`). A second group reads Maetrim's socket labels, not the wiki files: labels that collide once case, spaces and punctuation are dropped and `zentarim`/`upgradable` are respelled `zhentarim`/`upgradeable` (`warning: socket labels differ only by spelling: "<a>" / "<b>"`). A recipe's `slot` must still name the exact label the recipe's augments use. A `quest_loot` entry whose `items` or `augments` names an end reward of a quest chain or saga that includes the quest is flagged, since that reward belongs to the chain or saga (`warning: quest_loot "<quest>" lists "<item>", which is already a reward of quest chain "<chain>"; a chain's end reward belongs only in quest_chains.toml`, and the same with `saga` and `sagas.toml`). A third group flags each recipe `note` that says the wiki and his files disagree, so it can become a correction in [`../corrections/`](../corrections/README.md): any note containing `his value stands`, `Maetrim spells`, `Maetrim names` or `his augment`, ignoring case (`warning: crafting note is a correction candidate: <system> / <option>: <note>`). `cargo xtask wiki-batch` writes the item, augment and quest name lists a reading agent matches against, and `blank_descriptions.txt` (below), under `target/wiki-batch/`.

## `descriptions.toml`

One `[[entry]]` table per item, augment, race, feat or enhancement whose description Maetrim's files leave blank, carrying the description text from its wiki page.

```toml
[[entry]]
kind = "item"
name = "Royal Guard Mask"
page = "https://ddowiki.com/page/Item:Royal_Guard_Mask"
read = "2026-09-29"
description = "..."
```

- `kind` (required): `item`, `augment`, `race`, `feat` or `enhancement`.
- `name` (required, unique per `kind` across files): the name exactly as Maetrim's `items.name`, `augments.name`, `races.name`, `feats.name` or `enhancements.name` spells it. An augment, feat or enhancement name applies to every row with that name, since names repeat across augment families, class files and enhancement trees.
- `description` (required): the page's description text, not empty and with no leading or trailing whitespace.

The merge writes the description only where Maetrim's is empty (or, for an augment, where his text is the `Drops in: ?` placeholder or ends in a `Drops in: ?` line, which is then the only line replaced), so a wiki description never replaces text he wrote; for such an augment, write the drop line in his form (`Drops in: The House of Gems, end chest`): the build links the augment to the quests that line names, with their chests and rare markers, exactly as it links his own drop text, and before any `rare_augments` entry marks one. Besides the citation checks above, the build fails, naming the entry, when a field is missing or unknown, `kind` is not one of the five, `description` is empty or untrimmed, a (`kind`, `name`) pair repeats, or `name` matches no row of that kind. The report's `wiki_description_entry_count` counts the entries applied, `wiki_description_filled_count` the rows filled `wiki_description_skipped_count` the rows left alone because he already had a description, and `wiki_description_augment_link_count` the quest links made from filled augment descriptions. `cargo xtask wiki-batch` writes the work list, `blank_descriptions.txt`: one `kind<TAB>name<TAB>suggested page URL` line per name still blank after the current overrides.

## `items.toml`

One `[[item]]` table per wiki item page for a named item Maetrim's `Items/` does not have, so the item exists in the database until his files carry it, under the rule in [Entities his files lack](#entities-his-files-lack). A tiered item is recorded once per level variant, each its own entry named `<Name> (Level N)`, matching the way his files name tiered items.

```toml
[[item]]
name = "Longsword of the Oozing Hunger"
page = "https://ddowiki.com/page/Item:Longsword_of_the_Oozing_Hunger"
read = "2026-09-29"
slot = "Main Hand"
category = "Weapon"
item_type = "Longsword"
minimum_level = 29
enhancement_bonus = 15
material = "Steel"
drop_location = "Sealed Altar, crafted from ..."
quests = [{ name = "Some Quest", loot_type = "chest" }]
augment_slots = ["red", "colorless"]
bonuses = [{ stat = "Strength", bonus_type = "Enhancement", value = 15 }]
effects = [{ name = "Vorpal", description = "...", value = 3, target = "..." }]

[item.weapon]
damage_dice_count = 2
damage_dice_sides = 8
damage_dice_bonus = 0
critical_threat_range = 2
critical_multiplier = 2
handedness = "One-handed"
dr_bypass = ["Magic", "Slash"]
```

- `name` (required, unique across files), `slot` (an `equipment_slots.name`, as `/v1/equipment-slots` lists it), `category` (`Armor`, `Shield`, `Weapon`, `Jewelry` or `Clothing`), `minimum_level` and `drop_location` (the page's source line, free text, written in his form: `Sealed Altar, Turn in ...`) are required. The build links the item to the adventure pack (a chest or reward drop, as for his items), crafting system, challenge pack, vendor or event its `drop_location` names, as it does his items' drop text (see [`../README.md`](../README.md) on `source_aliases.toml`); quests come only from `quests`.
- `item_type`: for a weapon or shield, a weapon type as `/v1/weapon-types` lists it (required); for armor, the armor type (`Cloth`, `Light`, `Medium`, `Heavy`, `Docent`), or omitted; otherwise free text or omitted.
- `enhancement_bonus`, `material` (a material his items carry), `race_required`, `description` (flavour text) and `set` (a set in his `SetBonuses.xml` or `FiligreeSets/`) are optional; `accepts_sentience` and `is_minor_artifact` default to `false`.
- `quests`: each `{ name, loot_type }`, the name spelled as his `Quests.xml` or `Challenges.xml` has it (or a quest a `quests*.toml` entry creates) and `loot_type` one of `chest`, `raid`, `reward`.
- `augment_slots`: socket labels as `/v1/augment-slot-types` lists them, in the item's order.
- `bonuses`: each `{ stat, bonus_type, value }` with optional `value2`; the stat and bonus type as `/v1/stats` and `/v1/bonus-types` name them.
- `effects`: each `{ name }` with optional `description`, `value` and `target`. A name he already uses reuses his effect and his description; a new name creates the effect with the given description. Effect names are matched to his ignoring case, spaces, hyphens, colons, commas, periods and apostrophes, so `Nightmare Guard` reuses his `NightmareGuard` and `Rune Arm Imbue: Light IV` his `Rune Arm Imbue - Light IV`, with his description; two of his own names are never merged. `cargo xtask wiki-check` warns about a wiki effect that still differs from one of his only by other characters that are not letters or digits.
- `[item.weapon]` (required for `Weapon`, allowed for `Shield`): `handedness` (`One-handed`, `Two-handed`, `Off-hand` or `Thrown`) is required, the weapon type being the item's `item_type`; `damage_dice_count`, `damage_dice_sides`, `critical_threat_range` (the count of threatening faces: `19-20` is 2), `critical_multiplier`, `damage_dice_bonus`, `damage_multiplier` (the `[W]` multiplier, when the page gives one) and `dr_bypass` are optional, and each one left out is written as NULL. Dice are optional for rune arms (`slot = "Runearm"`, `item_type = "Rune Arm"`, `handedness = "Off-hand"`), which carry none, as none of Maetrim's do; give them for every other weapon the page lists them for.
- `[item.armor]` (required for `Armor` and `Shield`, only for them): `armor_type` (`Shield` for shields, otherwise one of the armor types, matching `item_type`), and optional `armor_bonus`, `max_dex_bonus`, `arcane_spell_failure`, `armor_check_penalty` and `shield_bonus`.

The merge runs after his items, sets and sockets are written. An entry whose `name` equals one of his item names writes nothing and is counted in `wiki_item_superseded_count`; every other entry writes an `items` row with `provenance = 'wiki'` and `wiki_url` its `page`, plus its weapon or armor stats, bonuses, effects, sockets, quest links (never rare) and set link, and is counted in `wiki_item_written_count`. An entry whose name matches one of his once case, punctuation, spaces and a trailing `(level N)` are ignored is still written, and is counted in `wiki_item_probable_duplicate_count` so a person can decide whether it is the same item. Besides the citation checks above, the build fails, naming the file, the item and the value, when a field is missing or unknown, a value is outside the vocabularies above, a weapon lacks `[item.weapon]`, an item carries a table its category does not allow, or a quest, material, set or socket label matches nothing in his files. `cargo xtask wiki-check` prints the three counts and, under `warnings:`, one line per superseded entry (`warning: wiki item "<name>" is now in Maetrim's files; delete it from <file>`) one per probable duplicate (`warning: wiki item "<name>" may duplicate Maetrim's "<his name>"`), and one per written entry whose `minimum_level` and `drop_location` equal one of his items' and whose name is that item's name followed by a space and a parenthesised suffix, as a looks variant is (`warning: wiki item "<name>" looks like a variant of Maetrim's "<his name>" (same level and drop location)`). `cargo xtask wiki-batch` writes `wiki_source_items.txt`, the names still supplied by the wiki; its `item_names.txt` lists only his items. The API reports each item's `source`, and `/v1/version` counts them as `wiki_items`.

## `augments.toml`

One `[[augment]]` table per wiki augment page for an augment Maetrim's `Augments/*.Augments.xml` do not have, so it exists in the database until his files carry it, under the rule in [Entities his files lack](#entities-his-files-lack).

```toml
[[augment]]
name = "Some New Gem"
page = "https://ddowiki.com/page/Item:Some_New_Gem"
read = "2026-10-01"
family = "Named"
min_level = 30
slots = ["green", "colorless"]
description = "...\nDrops in: Some Quest, end chest"
effect_description = "..."
set = "Perfect Silence"
bonuses = [{ stat = "Strength", bonus_type = "Insight", value = 3 }]
```

- `name`, `family`, `min_level`, `slots` and `description` are required; `name` is unique per family across files, since his augment names repeat across families.
- `family`: one of his families, the name of one of his `Augments/*.Augments.xml` files before `.Augments.xml` (what `/v1/augments` returns as `family`). A wiki augment never starts a family of its own: his families are what crafting systems and the `family` filter group by, so an augment whose family he lacks is reported upstream instead. A crafting system that lists the family then finds the wiki augment by name, as it finds his.
- `slots`: the socket labels the augment fits, as `/v1/augment-slot-types` lists them; at least one.
- `description`: the page's description. Write its drop line in his form (`Drops in: The House of Gems, end chest`) and the build links the augment to those quests exactly as it links his own drop text.
- `effect_description` (optional): the page's effect text, as his `<EffectDescription>` carries it.
- `set` (optional): a set in his `SetBonuses.xml` or `FiligreeSets/` that slotting the augment counts towards.
- `bonuses` (optional): each `{ stat, bonus_type, value }` with optional `value2`, written exactly as an item's `bonuses`.

Named effects (an item's `effects`) are not an augment field: his augments carry their effects as `<Effect>` modifiers, which the wiki cannot supply, so text the bonuses cannot express goes in `effect_description`.

The merge runs after his augments and sets are written. An entry whose `name` is already one of his augments in the same `family` writes nothing and is counted in `wiki_augment_superseded_count`; a name he uses only in another family is a different augment and is written. Every other entry writes an `augments` row with `provenance = 'wiki'`, its sockets, bonuses, set link and drop-text quest links, and is counted in `wiki_augment_written_count`; one whose name matches one of his in the same family once case, punctuation, spaces and a trailing `(level N)` are ignored is still written, and counted in `wiki_augment_probable_duplicate_count`. Corrections never apply to a wiki augment. Besides the citation checks above, the build fails, naming the file, the augment and the value, when a field is missing or unknown, `slots` is empty, a stat or bonus type is outside `/v1/stats` and `/v1/bonus-types`, or the family, a socket label or the set matches nothing in his files. `cargo xtask wiki-check` prints the three counts and, under `warnings:`, one line per superseded entry (`warning: wiki augment "<name>" is now in Maetrim's files; delete it from <file>`) and one per probable duplicate (`warning: wiki augment "<name>" may duplicate Maetrim's "<his name>"`). `cargo xtask wiki-batch` writes `wiki_source_augments.txt`, the augments still supplied by the wiki as `family<TAB>name<TAB>min_level` (the form of `augment_names.txt`, which lists only his). The API reports each augment's `source`, and `/v1/version` counts them as `wiki_augments`.

## `quest_chains.toml` and `sagas.toml`

Quest chains and sagas give an end reward from an NPC after several quests, so their rewards are `sources` rows of their own kind (`kind` `quest_chain` or `saga`, with no `loot_type`), not a quest's loot. A quest chain is what ddowiki calls a story arc (The Lost Seekers, Cult of the Six), and Maetrim's drop text names its reward as `<chain>, End reward` or `<chain>, quest chain end reward`; a saga is the saga system's reward NPC (Masterminds of Sharn, The Haunting of Saltmarsh), named as `<saga> saga: Epic end reward` or `Dread saga legendary end reward`, and its rewards come in heroic, epic and legendary lists. His files model neither, so every row comes from these files, with `provenance = 'wiki'` and `wiki_url` the entry's `page`. `quest_chains*.toml` holds `[[chain]]` tables and `sagas*.toml` holds `[[saga]]` tables:

```toml
[[chain]]
name = "The Lost Seekers"
page = "https://ddowiki.com/page/The_Lost_Seekers"
read = "2026-10-02"
pack = "Free to Play"
quests = ["Redemption", "The Grotto"]
rewards = ["Acrobat's Ring", { name = "Some Rare Reward", rare = true }]

[[saga]]
name = "Masterminds of Sharn"
page = "https://ddowiki.com/page/Masterminds_of_Sharn_(saga)"
read = "2026-10-02"
pack = "Masterminds of Sharn"
quests = ["Project Nemesis", "Too Hot to Handle"]
rewards = [{ name = "Band of Diani ir'Wynarn", tier = "epic" }, "Some Untiered Reward"]
```

- `name` (required, unique per kind across files): the chain or saga as his drop text names it, so the build can link his items to it (below); the page title when his text names none.
- `pack` (optional): the adventure pack, spelled as his `adventure_packs.name`.
- `quests` (optional): the quests in order, each spelled as his `Quests.xml` or `Challenges.xml` has it or a quest a `quests*.toml` entry creates; each at most once.
- `rewards` (optional): the end rewards, each an item name (his, or an `items*.toml` entry's) or `{ name, rare }` for a chain and `{ name, tier, rare }` for a saga, with `rare` defaulting to `false` and `tier` one of `heroic`, `epic` or `legendary` (left out when the page gives none). A reward appears once per tier.

The build writes `quest_chains` and `sagas` and their `quest_chain_quests` and `saga_quests` links (with `sort_order` the quest's position) right after the wiki quests, before his items are written, so his items' drop text links to them, and their reward `sources` rows after the rest of the wiki merge, so a reward may name a wiki item. Besides the citation checks above, the build fails, naming the file, the entry and the value, when a field is unknown, a quest or reward repeats, a tier is not one of the three, a chain reward has a `tier`, or `pack`, a quest or a reward matches nothing in his files or the wiki files. The report's `wiki_quest_chain_count`, `quest_chain_quest_link_count` and `quest_chain_reward_count` count the chains, quest links and listed rewards applied, and `wiki_saga_count`, `saga_quest_link_count` and `saga_reward_count` the same for sagas; `cargo xtask wiki-check` prints them. His drop text credits a chain or saga in a segment that says `reward` and is no quest's own end reward (it names no quest, or says `chain` or `saga`, as step one of linking quests decides). In such a segment the build matches the recorded chain and saga names, longest first, ignoring case and as whole words as it matches quest names, trying sagas first when the segment says `saga` and chains first otherwise, and stops at the first kind that matches. Each match adds a `quest_chain` source row, or a `saga` one whose `tier` is the first of `heroic`, `epic` or `legendary` the segment says from the saga's last mention on (null when it says none, so `Vecna Unleashed, any legendary end chest or Vecna Unleashed (epic) saga end reward` is epic), with `is_rare` from the segment's rarity marker; a wiki `rewards` element for the same item (and tier) only adds rarity. `The Lost Seekers, End reward` links to The Lost Seekers, `Masterminds of Sharn saga: Epic end reward` to the saga's epic tier, and `or The Haunting of Saltmarsh (Epic) saga end reward` to that saga though its name holds the quest Saltmarsh's, which keeps only the chest link its own segment gives it. A segment naming no recorded chain or saga (`Advance to level 15, End reward`, `Random, level ~4 chests/rewards`) links nothing. The report's `drop_text_quest_chain_reward_count` and `drop_text_saga_reward_count` count the rows his text added.

`cargo xtask wiki-batch` writes the work list: `quest_chain_names.txt` and `saga_names.txt`, one `name<TAB>item count` line per name his items' drop text gives in such a segment that names no recorded chain or saga: the text before the first comma, parenthesis, ` saga`, ` quest chain`, ` chain end` or ` end reward`, without a leading `or`, `and` or `also`, in `saga_names.txt` when the segment says `saga`. Many are not chains at all (`Advance to level 15`, `Random`, `Special event items`); a reader records only the real ones. `unlinked_drop_segments.txt` lists, as `head<TAB>item count` with the most common first, the head (the text before the first comma or parenthesis) of every segment of his items' drop text that links to no quest, chain, saga or pack: unrecorded chains and sagas, wilderness areas his `Quests.xml` lacks, store purchases and free text.

## `vendors.toml` and `events.toml`

Many items are bought or traded for tokens from an NPC, or handed out by a festival, and his drop text names that source (`Morten Edgewright, Turn in 1 Ethereal Ingot`, `Treasure of Crystal Cove, Turn in ...`, `The Night Revels, Turn in ...`). His files have no table for either, so every row comes from these files, with `provenance = 'wiki'` and `wiki_url` the entry's `page`. `vendors*.toml` holds `[[vendor]]` tables and `events*.toml` holds `[[event]]` tables:

```toml
[[vendor]]
name = "Morten Edgewright"
page = "https://ddowiki.com/page/Morten_Edgewright"
read = "2026-10-02"
location = "House Jorasco"
pack = "Free to Play"
items = [{ name = "Ethereal Great Crossbow", cost = "1 Ethereal Ingot" }]

[[event]]
name = "Treasure of Crystal Cove"
page = "https://ddowiki.com/page/Treasure_of_Crystal_Cove"
read = "2026-10-02"
items = ["Admiral's Tricorne"]
```

- `name` (required, unique across files): the name as his drop text spells it, so his items link to it.
- `page`, `read`: the citation, as above.
- `location` (vendor, optional): where the vendor stands, free text as the wiki writes it.
- `pack` (vendor, optional): an adventure pack as his files spell it.
- `items` (optional): the items the vendor sells or trades, or the event rewards, each an item name (his, or an `items*.toml` entry's), and for a vendor `{ name, cost }` with `cost` what the page says it asks, free text.

The build writes the `vendors` and `events` rows right after the quest chains and sagas, before his items are written, so his items' drop text links to them: a segment naming a recorded vendor or event (as whole words, ignoring the case of every letter after the first, longest first, anywhere in the segment) adds a `vendor` or `event` row in `sources` for the item or augment, with `is_rare` from the segment's rarity marker. The listed `items` are linked after the rest of the wiki merge, so one may name a wiki item; a listed item his text already links keeps one row and gains the `cost`. Besides the citation checks, the build fails, naming the file, the entry and the value, when a field is unknown, an item repeats, or `pack` or an item matches nothing in his files or the wiki files. The report's `wiki_vendor_count`, `vendor_item_count`, `wiki_event_count` and `event_item_count` count the rows and listed items, and `drop_text_vendor_source_count` and `drop_text_event_source_count` the rows his text added; `cargo xtask wiki-check` prints them. Until a file records one, `check-db` in the deploy workflow allows the two tables to be empty.

`cargo xtask wiki-batch` writes the work lists: `vendor_names.txt` and `event_names.txt`, one `name<TAB>item count` line per head of a segment of his drop text that links to no source and says `turn in`, `sells`, `exchange` or `trade` (the head is the text before the first comma, parenthesis, colon or ` sells `). A head naming an anniversary, festival, Festivult, revels, invasion, Crystal Cove, Mimic Hunt, special event or Timeline Fragment goes in `event_names.txt`, any other in `vendor_names.txt`; the split is a guess from the name, so a crafting station or area may appear among the vendors and should be skipped.
