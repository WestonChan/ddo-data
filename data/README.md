# Editable data files

A gear file lives under the one quest that drops it; with several quests, in the pack's `gear/`; with no quest, under the one vendor, crafting system or event that yields it. Chain and saga end rewards and challenge rewards live in the pack's `gear/`: they reward a group of quests, and the data ties challenge rewards to the pack. Everything resolves to a pack by the existing rule: authored `pack` first, otherwise unanimous derivation, never through a derived entity; otherwise `packs/shared/`. Starter gear, the store and world drops use `packs/general/`; gear with no known source uses `packs/unsourced/`. An item's `[[source]]` rows say which quest drops it.

Each TOML file states the facts for one named entity or an existing group. The build assigns numeric IDs from names and variants, independently of folders, and derives foreign keys, ordering columns and version counts. Run `ddo-etl build --data data --out ddo.db` without XML or wiki inputs.

## Editing a file, or rebuilding through the database

These files are for correcting and completing what already exists: a wrong bonus value, a missing source, a note citing the wiki, an icon spelled as its file is. Edit the file, run `ddo-etl build --data data --out ddo.db`, commit. Nothing else is needed, and the build's errors name the file, the row and the nearest existing names when a reference does not resolve.

A change that touches many entities at once is a database change, not a hand edit: a new pack with its quests, chains and gear; a new crafting system and the augments it makes; a stat or effect renamed everywhere it appears; Maetrim's update arriving. Build the database, make the change there (or import his files into it), and run `ddo-etl export --db ddo.db --out data` to write every file back in the layout the placement rule gives it. The export is the only writer that places hundreds of files consistently; editing them by hand invites the drift the round trip exists to catch.

The database path, step by step: build it, change it, write it back, prove it.

```bash
cargo run --release -p ddo-etl -- build --data data --out ddo.db
sqlite3 ddo.db "UPDATE items SET minimum_level = 32 WHERE slug = 'legendary-fang-of-the-wolf'"
cargo run --release -p ddo-etl -- export --db ddo.db --out data
cargo xtask round-trip --data data
```

The export rewrites every file in the layout the placement rule gives it, so a moved pack or a renamed entity lands where it now belongs, and files no entity owns any more are removed. Review `git diff --stat data` before committing: a change meant for one entity that touches hundreds of files means the SQL did more than intended. `ddo-etl import-ddobuilder` is this path automated for Maetrim's updates (`docs/update-from-maetrim.md`): it builds the database from the files, merges his delta into it, and exports. Nothing edits the TOML except the export and a person.

## Layout

Quests, crafting systems, vendors and events are folders holding their record and what they yield. Chains, sagas, sets, patrons, filigree sets and challenges are files in their kind folders. Quests never nest inside chains. The root has eight entries:

```
data/
  packs/
    <pack>/pack.toml  exceptions.toml
    <pack>/quests/<quest>/quest.toml + gear/<item>.toml
    <pack>/crafting/<system>/crafting.toml + gear/<item>.toml + augments/<augment>.toml
    <pack>/vendors/<vendor>/vendor.toml + gear/<item>.toml
    <pack>/events/<event>/event.toml + gear/<item>.toml
    <pack>/chains/<chain>.toml  sagas/<saga>.toml
    <pack>/sets/<set>.toml  patrons/<patron>.toml
    <pack>/filigrees/<set>.toml  challenges/<challenge>.toml
    <pack>/gear/<item>.toml  augments/<augment>.toml
    free-to-play/<patron>/…
    shared/…
    shared/augments/<family>/<augment>.toml
    shared/sentient-gems/<gem>.toml
    general/gear/<item>.toml
    unsourced/gear/<item>.toml
  characters/
    classes/  races/  feats/  spells/
    enhancements/<tree>/tree.toml + icons/
    stances.toml  guild-buffs.toml  optional-buffs.toml
  effects/
    <effect>.toml  stats/  stat-groups/  tiers/
    bonus-types.toml  equipment-slots.toml  weapon-types.toml
    damage-types.toml  item-materials.toml  triggers.toml
    augment-slot-types.toml  weapon-proficiencies.toml  clickies.toml
  upstream/
    commit.toml  excluded-items.toml  source-aliases.toml
  redirects.toml
  icons/<kind>/<Name>.png
  .schemas/
  README.md
```

`gear/` and `augments/` on a source's line sit inside that source's folder. Free to Play and the pseudo-packs use the same shape. A Free to Play entity sits under a patron only when every relevant member resolves to that patron; otherwise it sits directly under `free-to-play/<kind>/`. Concrete source records include `packs/u043-the-soul-splitter/quests/members-only/quest.toml`, `packs/u032-against-the-slave-lords/crafting/slave-lords-crafting/crafting.toml`, `packs/m3-demon-sands/vendors/masei-mkembe/vendor.toml`, and `packs/free-to-play/the-coin-lords/events/invasion-tokens/event.toml`. Vendor currencies remain the shared vocabulary `packs/shared/vendors/currencies.toml`. Vocabularies and art stay flat: they are reached by name, not browsed.

`<pack>` is the folder derived from the pack record's `update`: `Update 39` gives `u039-<slug>`, `Module 1.4` gives `m1-<slug>`, and `Public Beta Test` gives `m0-<slug>`. Module patch numbers are dropped; update numbers use at least three digits. Free to Play and the three pseudo-packs keep their unprefixed folders. A pack's slug and ID still derive from its name. The filename-identity check rejects a folder that disagrees with its record and names the expected folder. XML exports lack release metadata and retain the name-only folder until it is authored.

Each pack records its `update` and, when known, `released` date as `YYYY-MM-DD`, with a `[[citation]]`. Delera's Tomb and Ruins of Threnal have `m0-` folders and no known release date. Research qualifications are `[[note]]` entries with `kind = "release"`, `reason`, `url` and `read`; they are annotations, never correction rows. A citation has no page-title field.

`shared`, `general` and `unsourced` are reserved pseudo-packs, without `pack.toml` or database pack rows. A real pack cannot take one of those slugs. `general` contains starter equipment, DDO Store items and world drops; `unsourced` contains items with no known source after the existing drop-text and version-family rules below.

Pack placement follows one rule: an authored `pack` wins; otherwise every related member must resolve to the same pack. Empty, unresolved or conflicting membership stays shared. Sets use their member items and augments; quest chains and sagas use their quests; crafting systems use their item and augment outputs, including recipe outputs; vendors use their offered items; events use their rewards; patrons use their quests. Challenges use their authored pack. Sets, patrons and challenges are files because they yield no individually linked rewards. Challenge reward sources identify a pack, so those rewards live in that pack's `gear/`.

Gear placement has this precedence:

1. Exactly one referenced quest wins, even beside other source kinds or packs: its `gear/` holds the item.
2. Several quests use the pack's `gear/` when their known authored source packs agree, otherwise `packs/shared/gear/`.
3. With no quest, one vendor, crafting system or event source owns the `gear/` folder. Repeated rows referencing that same source count as one. Other mixed sources and chain, saga and challenge rewards use the pack's `gear/`, or shared gear when there is no single pack.
4. Starter sources use `packs/general/gear/`. With no source rows, existing drop text identifies DDO Store, random world drops or a pack; otherwise the item uses `packs/unsourced/gear/`.

An item otherwise under `packs/unsourced/gear/` can use its version family: strip the existing Epic or Legendary prefix and `(level N)` suffix, then consider only siblings with sources. If every sourced sibling resolves to the same pack, the unsourced version lives under that pack's `gear/`. This changes only the file location; it does not recreate source rows.

Augments whose family belongs to exactly one crafting system follow that system's record into its `augments/` folder. Other augments keep their family grouping at `packs/shared/augments/<family>/<augment>.toml`. Here `family` means Maetrim's augment family, the API's `augments.family`. No crafting record is invented for a family. Every augment file states `family` explicitly, including all crafting outputs; the build never infers it from a folder.

Filigree set files stay whole. Every member's `menu` must resolve to the same pack, using existing adventure-pack entries in `upstream/source-aliases.toml` before exact pack-name comparison. Otherwise the file stays in `packs/shared/filigrees/`. The current split is 26 pack files and 39 shared files; no new alias was added. Sentient gems have no pack and each has a file at `packs/shared/sentient-gems/<gem>.toml`.

`characters/` contains what a character trains or toggles. `effects/` contains stats, effects, stat groups, effect tiers and game vocabularies, including item clickies. `upstream/` contains the recorded commit, exclusions and source aliases used when reading Maetrim's files.

Member items and augments resolve through their sources' authored packs. Derivation never follows another derived entity. When checking a vendor, crafting system or event's members, its own source link is excluded: an owner cannot establish its own derived pack. Known source packs must agree; a member with no known pack is unresolved. An authored pack that disagrees with a unanimous member pack produces a WARN while retaining the authored value.

The reserved record names `pack.toml`, `quest.toml`, `crafting.toml`, `vendor.toml` and `event.toml` take their identity from their containing folder, even when the entity itself is named Pack, Quest or another reserved word. Other entity filenames match the slug of the name and any stated variant. The build and export share `narrowest_folder`, one folder-selection function for all entity kinds: a WARN names a misplaced entity, its current path and the expected move. Changing sources or memberships changes placement without changing authored packs, sources or entity IDs. Moving a file alone preserves its ID; export returns it to the implied folder.

The nine vocabulary filenames shown under `effects/` are reserved; a named effect cannot take one of those names. References are display names as the game writes them; filenames are their slugs plus any stated variant. A build collects unresolved references across the dataset and prints the file, array and one-based index, missing kind and name, expected path, and up to three nearest names with their actual paths. When the containing group is unknown, the expected path uses a pattern such as `characters/feats/<category>.toml`; a price ingredient lists both the vendor-currency and crafting-ingredient patterns, one per line. TOML syntax errors stop reading immediately.

Run `ddo-etl find "<text>"` to search names and slugs case-insensitively, or add `--data <directory>` for another tree. It reads the files directly, so it works before a build succeeds. Each line is `<kind>  <name>  <path>`; no matches returns the five nearest names with their paths. For example, `ddo-etl find "Death House"` prints `quest  Death House  packs/u037-mists-of-ravenloft/quests/death-house/quest.toml`.

`ddo-etl export --db ddo.db --out <directory>` writes the TOML from database tables. It does not write PNGs, since image bytes are not in the database. Copy the data files' PNG files into the export directory before building it again. An icon reference uses the PNG's exact case and the matching `icons/<kind>/` folder. Enhancement icons used by one tree live beside that tree; shared enhancement images live in `icons/enhancements/`.

The database's `dataset_version` records the upstream SHA from `upstream/commit.toml` and the build time. Correction notes are `[[note]]` entries on the entity with our value; removed upstream records are `[[removed]]` entries on the surviving entity. The export reconstructs these entries from the `corrections`, `entity_notes`, `removed_entities`, and `citations` tables.

Default-false booleans must be omitted when false; writing `is_legacy = false`, for example, fails the build. Derived weapon `damage` and `critical` strings also cannot be written. The build derives them from weapon fields.

`[[source]]` records where an item or augment comes from, using the API's source names. Vendor sources can carry ordered `[[source.price]]` entries with `ingredient` and positive `quantity`; the deprecated served `cost` string is generated from those entries.

## Redirects

The API addresses entities by slug, so renaming an entity (or changing its `variant`) changes the slug every link and saved build holds. `redirects.toml` keeps old slugs answering for 30 days. It names slugs, not display names, because the old name no longer exists anywhere else.

```toml
[[redirect]]
kind = "items"
from = "legendary-old-name"
to = "legendary-new-name"
since = "2026-10-20"
```

A `[[redirect]]` makes `/v1/<kind>/<from>` answer 308 to `<to>` and lets `from` stand in for `to` in filters. `kind` is the API's route word (`items`, `augments`, `sets`, `quest-chains`, …; kinds with no route use their kebab-case table name). A renamed enhancement tree or crafting system redirects its selections' or ingredients' old slugs too, without rows of their own. An entity deleted for good is a `[[retired]]` row (`kind`, `slug`, `since`, `reason`); its slug answers 404, and the row only tells the deploy check the disappearance was intended. A retired slug is ours, served by the API and now gone with no successor; it is not the `[[removed]]` array on an entity file, which names an upstream duplicate merged into that survivor.

The build fails, naming both sides, when a `from` or retired slug is still current, when a `to` is not current (retarget it: redirects never chain), when a slug appears twice or is both redirected and retired, or when a slug is all digits. The deploy compares the build with the live API and fails on any slug it served that is now missing without a row here, so a rename or deletion, by hand or through `import-ddobuilder`, is answered in this file before it lands. Rows expire: `cargo lint` and the deploy fail on a row older than 30 days or dated in the future, and `cargo run -p ddo-etl -- prune-redirects --data data` deletes the expired ones. Builds stored elsewhere (V8, Phase 5) apply the `served_redirects` view as a data migration at the deploy that ships a redirect, so 30 days only has to cover bookmarks and shared links.

## Requirement groups

Plain `[[requirement]]` entries are shorthand for one `all` group. When there are several groups, write every group explicitly in order, including `all` groups between others:

```toml
[[requirement_group]]
kind = "one_of"
[[requirement_group.requirement]]
req_type = "ClassMinLevel"
items = ["Monk"]
value = 12
[[requirement_group.requirement]]
req_type = "ClassMinLevel"
items = ["Sacred Fist"]
value = 12

[[requirement_group]]
kind = "all"
[[requirement_group.requirement]]
req_type = "Level"
value = 21

[[requirement_group]]
kind = "none_of"
[[requirement_group.requirement]]
req_type = "Feat"
items = ["Insightful Fortitude"]
```

The build assigns group indexes by position and requirement order within each group. Do not mix plain requirements and explicit groups for the same owner and phase. A feat's automatic acquisition requirements use `phase = "auto_acquire"` on each requirement row; omitted phase means its ordinary prerequisites. All rows in a group must have the same phase. Nested owners use the same structure beneath their own table prefix.

## Image references

Image fields use exact PNG case without the `.png` extension. Single-kind owners use bare names and reject qualified paths. Optional buffs can use several image kinds, so they require paths from the icons root, such as `icon = "spells/Fireball"` or `icon = "enhancements/spellsinger/SSArcaneAid"`. The latter refers to `characters/enhancements/spellsinger/icons/SSArcaneAid.png` under `data/`. A missing image is omitted with a `[[note]]` on `icon` preserving the upstream reference; it is served as null. Export reports unowned PNGs it cannot remove and exits non-zero when it reports a HARD layout problem.
