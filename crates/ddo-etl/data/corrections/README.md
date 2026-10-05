# Corrections

Known mistakes in Maetrim's DDOBuilderV2 files, each replaced with the right value. The ETL embeds every `*.toml` in this directory. Quest corrections apply as soon as his quests are written, before wiki quests are created and before his items' and augments' drop text is linked to quests, so drop text naming a quest by its corrected name links its loot; every other correction applies after all of his files are written. All of them apply before the [wiki overrides](../wiki/README.md) merge.

## The principle

This is the only place our data overrides his. Everywhere else his files win, and the wiki layer only adds facts he has no field for. The wiki is assumed right: when a ddowiki page contradicts a value he carries (a typo, a level no sibling shares, a bonus value, a bonus type, a name, a missing socket, a duplicate row), the wiki's value goes here, with that page as `source`. Report the disagreement upstream at [Maetrim/DDOBuilderV2](https://github.com/Maetrim/DDOBuilderV2) as well, so the correction can go away. Every correction records the value he has now, and it expires the moment his value changes: the build then leaves his new value alone and `wiki-check` asks for the entry to be deleted.

Corrections are internal. The ETL applies them and stores each applied one in the `corrections` table of `ddo.db` for provenance and for `wiki-check`'s stale-correction warnings; the API never returns them, so a corrected value reads like any other.

## The shape

One `[[correction]]` table per corrected field, in any `*.toml` file here, split by area: `corrections_quests.toml` (quests, packs, patrons), `corrections_crafting.toml` (crafting augments and their bonuses), `corrections_items.toml` and `corrections_missing_values.toml` (items, their bonuses, effects and sockets, socket labels), `corrections_sets.toml` (set tiers, their bonuses and set members), `corrections_damage.toml` (effect damage), and `corrections.toml` for the rest.

```toml
[[correction]]
kind = "augment"
name = "The Fury's Rage"
field = "min_level"
from = 318
to = 18
reason = "Every sibling augment in LostPurpose is min level 18; 318 is a typo."
source = "https://ddowiki.com/page/Lost_Purpose"
read = "2026-09-29"
```

- `kind`: the table the row is in: `item`, `augment`, `quest`, `feat`, `enhancement`, `race`, `class`, `adventure_pack`, `patron`, `set_bonus`, `spell` or `effect`; or `augment_bonus` (one bonus of an augment), `item_bonus` (one bonus of an item, or a bonus it lacks), `set_tier` (a tier of a set), `set_tier_bonus` (one bonus of a tier), `item_effect` (an effect an item lacks), `effect_damage` (one damage row of an effect), `item_socket` (an item's sockets) or `socket_label` (an `augment_slot_types` row).
- `name`: the row, by his exact name (for `augment_bonus` the augment's, for `item_bonus`, `item_effect` and `item_socket` the item's, for `socket_label` the label as `/v1/augment-slot-types` lists it, such as `"crafting: zentarim attuned"`). Augment, feat and enhancement names repeat (across augment families, class files and trees), so a correction of one of those applies to every row with that name. An `item`, `item_bonus`, `item_effect` or `item_socket` correction matches only his items, never an item the wiki supplied.
- `family` (optional, `augment` and `augment_bonus` only): narrows the correction to the rows of that name in that augment family.
- `stat` and `bonus_type` (`augment_bonus`, `item_bonus` and `set_tier_bonus` `value`, `bonus_type` and `remove`; also `item_bonus` and `set_tier_bonus` `scale` and `rounding`): the bonus to correct, by the stat row's `effects.name` and his `bonus_types.name` it has now, or `bonus_type = "null"` for a bonus his files leave untyped.
- `bonus_value` (optional, with `stat` and `bonus_type`): narrows the correction to the bonus with that value, for an item, augment or set tier carrying two bonuses on one stat with one type (Embrace of the Spider Queen's two untyped Fortification buffs).
- `equipped_count` (`set_tier` and `set_tier_bonus` only): the tier's positive item or augment count; it also narrows a tier bonus to that one tier.
- `field`: a field from the allow-list below.
- `from`: his current value; `to`: the value to write. Each is an integer, a float, a string, or the literal string `"null"` for NULL. Flags are `0` or `1`; a field that refers to another row takes that row's name. An item, augment or tier `remove` takes `from = 0` (his files still carry the row) and `to = 1`; a bonus `remove` takes its current integer value as `from` and `to = "null"`, or `from = "null"` and `to = 1` when his bonus lacks a value; an `add` takes `from = "null"`.
- `reason`: why his value is wrong, in one sentence.
- `source`: the `https://` URL that shows the right value.
- `read`: the date the source was read, `YYYY-MM-DD`.

## The allow-list

Only scalar columns can be corrected, never an id or a row's name (except where a rename is the correction):

| `kind` | fields |
|---|---|
| `item` | `minimum_level`, `enhancement_bonus`, `description`, `drop_location`, `race_required`, `accepts_sentience`, `is_minor_artifact`, `is_legacy` (flag an old version the wiki says no longer drops), `slot` (an `equipment_slots.name`), `item_category`, `item_type`, `material` (an `item_materials.name`), `set_bonus` (a set's name; the item's set link moves with it), `name` (a rename; `wiki_url` follows), `remove` |
| `augment` | `min_level`, `description`, `effect_description`, `family`, `name` (a rename), `remove` |
| `quest` | `level`, `epic_level`, `favor`, `is_raid`, `pack` (an `adventure_packs.name`), `patron` (a `patrons.name`), `name` (a rename) |
| `feat`, `enhancement`, `race`, `class`, `spell` | `description` |
| `adventure_pack`, `patron`, `set_bonus` | `name`, which renames the row; a renamed set is renamed in every item's and augment's `set_bonus` too |
| `effect` | `name`, `verbose_name_template`, `description_template`, `default_value`, `constant` (requires `stat` to name its bonus row) |
| `augment_bonus` | `value` (an integer), `bonus_type` (a `bonus_types.name`), `add` (`to = { stat = "...", bonus_type = "...", value = N }`), `remove`, `dedupe` (the current and intended counts of one identical stat, type and value) |
| `item_bonus` | `value` (an integer), `bonus_type` (a `bonus_types.name`), `scale` (a positive float), `rounding` (`down`, `up` or `nearest`), `add` (`to = { stat = "...", bonus_type = "...", value = N }`), `remove` |
| `set_tier` | `add` (`to = { equipped_count = N, description = "..." }`), `description`, `remove` |
| `set_tier_bonus` | `value` (an integer), `bonus_type` (a `bonus_types.name`), `scale` (a positive float), `rounding` (`down`, `up` or `nearest`), `add` (`to = { stat = "...", bonus_type = "...", value = N }`), `remove` |
| `item_effect` | `add` (`to` = the effect's name, or `{ name = "...", description = "...", value = N }`), `value` (requires `effect` to name its link) |
| `effect_damage` | `add` or `remove` (`to` or `from` = `{ trigger = "...", damage_type = "...", dice_number = N, dice_sides = N, dice_bonus = N, amount_from = N, scale = N, sort_order = N }`) |
| `item_socket` | `add` (`to` = the socket label to add) |
| `socket_label` | `name`, which renames the label everywhere it is used; when `to` is already a label the two merge |

- An item, augment or tier `remove` drops the row and its child rows (bonuses, effects, sockets, quest loot, set links, modifiers; crafting recipes that name it would then fail the wiki merge). Use it only for a row the wiki shows is a duplicate or a tier the wiki unambiguously does not grant.
- `augment_bonus` `value` and `bonus_type` rewrite the matching stat row or owner value on every augment of that name. Other owners keep their family and values. The augment's `modifiers` are his and are not rewritten. `add` appends an effect link to the augment; it is stale once the augment already carries that stat and type.
- `augment_bonus` `dedupe` removes only identical extra links for the named stat, type and `bonus_value`; it is stale once the number of such links changes.
- An `effect` correction changes the shared family for all owners. Its `constant` field selects one static bonus row by `stat`; scalar text and default corrections change the family's templates or fallback amount.
- `item_bonus` `value` and `bonus_type` rewrite the item's effect the same way, and `add` appends a link to the item. An add is stale once the item already carries that stat and type. A `bonus_type` correction from `"null"` also types an untyped stat row of one of the item's augment slot options, matched by stat and `bonus_value` like the item's own.
- `set_tier_bonus` uses the same replacement, append and removal rules within the set's `equipped_count` tier. A `value` correction may name a group that the tier links once, replacing the matching source stat members together. Every structured stat has a named bonus type, either on its family or its owner link. A tier `add` creates its description and is stale once that tier exists. A tier `remove` deletes its effects and modifiers with it.
- `item_bonus` and `set_tier_bonus` `scale` and `rounding` identify a family's stat row through the named owner. The transform belongs to that family, so the correction changes the effective amount for every owner linked to it. A static row uses scale 1 and `down` and cannot be scaled.
- `augment_bonus`, `item_bonus` and `set_tier_bonus` `remove` delete only the matching owner's effect link, preserving any effect other owners use and leaving modifiers unchanged. They use the same `stat`, `bonus_type` and optional `bonus_value` qualifiers as `value`; `from` is the current integer value and `to = "null"`, or `from = "null"` and `to = 1` for a valueless source bonus. When multiple links match the stat and type, `bonus_value` is required; without it, the build fails naming the owner, stat, type and matching values. Removal is stale once the bonus is absent or its value changes.
- A `bonus_type` correction's `from` is the `bonus_type` it names: his current type, or `"null"` when his buff or effect has none. One from `"null"` is applied while the effect is written, because the ETL refuses an unresolved stat type, naming the item or augment, the buff or effect and the stat; it is stale once his bonus has a type, or once he carries no bonus on that stat (with that `bonus_value`).
- `item_effect` `add` appends a text-only `item_effects` link. It reuses his text-only effect when one has that name ignoring case, spaces and the punctuation `- : , . '` (as the wiki items writer does), and otherwise creates an effect, with `description` and `value` when supplied; it is stale once the item carries a matching effect.
- `item_effect` `value` changes only the named item's link to the named effect; a null link value lets the family's default fill it.
- `effect_damage` `add` or `remove` changes an effect's dice row by its trigger, damage type, dice and order. A range from minimum to maximum becomes 1d(max−min+1)+(min−1); Acid II's 2 to 8 is 1d7+1. The effect must already be written by an owner.
- `item_socket` `add` appends an `item_augment_slots` row with that label, which must be one his files use; it is stale once the item carries the label.
- `socket_label` `name` into a new label keeps the prefix (`crafting: `) and updates the variant; a label with a qualifier (`isle of dread: scale (weapon)`) can only merge into an existing label.

A quest's `is_free_to_play` and `legendary_level` come from the wiki's `quests.toml`, not his files, so they are corrected there. Within each of those two passes renames run after every other correction, so every correction (and `pack`, `patron`, `set_bonus` and `item_socket` values) names rows by his names; the wiki merge runs after the renames, so wiki files (crafting `augments` and `slot` fields, `quest_loot`, descriptions, packs) use the corrected name, and the build fails on a wiki file that still names his old spelling. The one exception is a wiki items file (`items*.toml`): wiki items are written before corrections run, so their `augment_slots` and `set_bonus` use his labels and names, and a later `socket_label` rename moves their sockets with his.

## Validation

The build fails, naming the file and the correction, when:

- a file has a table or field other than the ones above;
- `kind` is not one of the listed kinds, or `field` is not in that kind's allow-list;
- `family` is set on a kind other than `augment` or `augment_bonus`, `equipped_count` is missing on a set tier correction or set on another kind, or `stat` and `bonus_type` are missing from (or `stat`, `bonus_type` or `bonus_value` set outside) an `augment_bonus`, `item_bonus` or `set_tier_bonus` `value`, `bonus_type` or `remove` correction;
- the same (`kind`, `name`, qualifier, `field`) appears twice, in one file or across files; the qualifier is the `family`, the bonus's stat and type (and `bonus_value`), or an added effect's name or socket's label, so one augment can have several of its bonuses corrected;
- `from` or `to` does not fit the field (a string for an integer, `2` for a flag, `"null"` for a field that cannot be NULL), or `to` equals `from`;
- `reason` is empty, `source` is not an `https://` URL, or `read` is not a real `YYYY-MM-DD` date;
- `name` matches no row of that kind (in that `family`) in his files, unless the correction is a rename (`field = "name"`) and a row named `to` already exists, which means he fixed the spelling himself: that rename is counted stale, not failed, and the wiki files that use the corrected name keep working;
- a `slot`, `material`, `pack`, `patron`, `set_bonus`, stat, bonus type or socket label names nothing in his files, or a rename's `to` is already another row's name (for an augment, another augment in the same family).

When `from` equals his stored value (on every row with that name), the build writes `to` and records the correction in the `corrections` table, with its qualifier. When it does not, the build writes nothing for that entry and counts it as stale; a stale correction never fails the build.

## Checking

From the `ddo-data` root:

```bash
cargo xtask wiki-check
```

It builds into memory in a few seconds and prints `correction_applied_count` and `correction_stale_count` after the wiki counts, and one warning per stale correction under `warnings:` (`warning: correction <kind> "<name>".<field> expects <from> but Maetrim now has <value>; delete it from <file>`, or for a rename he has made himself `warning: correction <kind> "<name>".name is done upstream: "<to>" exists; delete it from <file>`). `--corrections DIR` checks a directory of drafts in place of the embedded files. The tests use their own corrections in `crates/ddo-etl/tests/fixtures/corrections/`, which only name what the fixture data files carry.
