# V9 data files served changes

## Slug addressing (schema 38, 2026-10-18)

The first of three steps that make slugs the only public identifier (ddo-tools roadmap 4e item 7). This step only adds; every existing field and numeric path still works.

- Every detail route takes the entity's slug: `/v1/items/legendary-bracers-of-the-demons-consort`, `/v1/effects/combat-mastery`, and so on for all 16 detail routes. A numeric id still resolves on the same path. Slugs are unique per kind; effects, stats and groups are one kind. No current slug is all digits, so a numeric path can never mean a slug.
- `/v1/items` filters `quest`, `quest_chain` and `saga` take slugs, ids still accepted. An unknown value is a 400 naming it, as `set` already was (previously an unknown id returned an empty list).
- A redirected slug answers `308` with `Location` set to the current slug's path and the original query string, the data `Cache-Control`, no `ETag` and an empty body; it also works as a filter value. Redirects come from `data/redirects.toml` and last 30 days. A renamed enhancement tree or crafting system redirects its selections' or ingredients' old slugs too. Reverting a rename within the hour can loop through a cached 308.
- Every `id` or `*_id` in a response has a `slug` or `*_slug` beside it, null exactly when the id is: `effect_slug`, `set_slug`, `base_class_slug`, `feat_slug`, `spell_slug`, `class_slug`, `source_slug`, `slot_type_slug`, `clickie_slug`. `detail_path` stays numeric in this step.
- Schema 38 adds the `redirects` and `removed_slugs` tables and the `served_redirects` view to the database and `/v1/dump.sqlite`; no existing table changes.
- The deploy downloads the live `/v1/dump.sqlite` and runs `cargo xtask check-redirects`, which fails when a slug the live API serves is missing without a redirect or removed row, and when a row is older than 30 days (`cargo run -p ddo-etl -- prune-redirects --data data` removes those).

Building from the data files changes numeric IDs for named entities once. Their IDs now derive from their name slugs, so moving a file leaves its ID unchanged. Named entity responses add `slug` beside `id`.

On 44 items in two or more sets, `set` now names the item's own set. Every item has a `sets` list, including an empty list where it belongs to none. `set` remains for one release and is deprecated in favor of `sets`.

Nested rows have no identity of their own. Their array position, or an existing position field, identifies the row within its owner. This does not remove references to named entities: an `id`, `slug`, `effect_id`, `set_id` or `slot_type_id` naming another entity remains a reference to that entity.

Item details, full item list entries and augment details keep `sources[].id` and add `sources[].slug`. These identify the referenced quest, quest chain, saga, adventure pack, crafting system, vendor or event, exactly as its own route does. A `challenge` source references its adventure pack, as on main. A `starter` source has no referenced entity: its `id` remains null and its `slug` is null. A source row's database ID is never served. Feat detail `bonuses[].id` also remains: it identifies the granting effect, including a direct stat effect, and `bonuses[].slug` is that effect's slug, not the rendered bonus label.

The removed row-identity keys are:

- `modifiers[].id` wherever modifiers are nested: item and augment details, item socket options, feats and their owned stances, enhancements and selections, spells, filigrees, guild buffs, optional buffs and set tiers. This includes the corresponding `this_attack_modifiers[].id` and `follow_on_modifiers[].id` collections.
- Item detail and full item list `augment_slots[].options[].id`: the slot option's own ID, not an augment ID. Main's slot options contain no augment reference; their set references (`sets[].id`) and effect references (`effects[].effect_id`) remain.
- Crafting system detail `recipes[].id`: the recipe's own ID. Its output `recipes[].augments[].id` remains the augment ID, with that augment's slug.
- Set detail `tiers[].id` and effect detail `set_tiers.set_tiers[].id`: the threshold row's ID. Its effect references (`effects[].effect_id` on set detail; `effect_id` and `line.effect_id` on effect carriers) and the carrier's `set_id` remain.
- Feat detail `conditional_groups[].id` and owned `stances[].id` on feats, enhancements, selections and spells. Owned stances are child rows, separate from the standalone named stances in `/v1/stances`, which retain `id` and `slug`.

Sockets, requirements and effect links do not gain row identities. Existing reference fields on these rows remain. The audit against main at `c2e9ef35` covered every disappeared `id` path in all committed response examples and the serving queries, including feat bonuses absent from the sampled feat example. Only source IDs and feat bonus effect IDs needed restoration; recipe output augment IDs, slot option set/effect IDs and set tier effect IDs were already retained.

Both builds omit 18 icons whose image files are absent from the ba7e18e upstream image folders. Three belong to the newly supplied Khopesh of the Oozing Hunger items and Cantrip: Ray of Frost. The data files record prices for 160 Raam Lukresh and Osah Lukresh offers that the XML inputs lack. These are authored `price` rows; removing a price leaves it absent on the next build and export.

The XML and data builds both remove duplicate effect links and modifiers and both make the item `set` correction above.

## Vendor prices and saga tiers

Item and augment `sources[]` add `price`, an ordered list of `{ ingredient, quantity }` entries; sources without a price return an empty list. Vendor detail `items[]` and item detail `vendors[]` carry the same price. Every one of these surfaces marks `cost` as deprecated in its schema description. The string remains for this release and is generated uniformly from those rows on both builds, joining quantities and ingredient names with commas. It disappears next release.

These are all wording changes to existing XML vendor costs. The quantities and ingredient names are the stored price values, without pluralization.

| Vendor | Previous cost | Generated cost | Offers |
| --- | --- | --- | ---: |
| Captain Xendros | 5 Legendary Quartermaster's Chits | 5 Legendary Quartermaster's Chit | 40 |
| Captain Xendros | 5 Quartermaster's Chits | 5 Quartermaster's Chit | 40 |
| Morten Edgewright | 1 Ethereal, 1 Arcane, and 1 Silver Ingot | 1 Ethereal Ingot, 1 Arcane Ingot, 1 Silver Ingot | 1 |
| Squire Rale | Dark Scarab Powder | 1 Dark Scarab Powder | 3 |
| Squire Rale | Eerie Scarab Powder | 1 Eerie Scarab Powder | 4 |
| Squire Rale | Scarlet Scarab Powder | 1 Scarlet Scarab Powder | 3 |
| Vagan Taneer | 10 Planar Shards | 10 Planar Shard | 3 |
| Vagan Taneer | 25 Planar Shards | 25 Planar Shard | 3 |
| Vagan Taneer | 50 Planar Shards | 50 Planar Shard | 2 |

Fourteen saga rewards whose tier was null now serve `"any"`, the same tier word used by crafting recipe costs. The data files state that tier explicitly.

A chest name is nullable when the source does not name one. On the ba7e18e dataset, 126 chest drops and 18 raid drops have no chest name; reward sources cannot carry a chest.

## Order and schema

Arrays now follow the order of their owner records: race ability modifiers, effect bonuses, feat bonuses, crafting recipe costs and outputs, and other nested rows. The database stores their positions and the API reads them in that order. Set tier modifiers gained consecutive served `sort_order` values after duplicate removal; the older API returned zero for each of many such rows. Equipment slot `sort_order` retains its existing one-based values.

Default list order for bonus types, weapon types and damage types is now by name, since an ID order would change between the XML and data databases. Augments and feats use their slug to break ties after their existing name fields, including same-name racial feats. Explicit `sort` parameters keep their requested order.

The schema on the old API was version 32. The data-file work first reserved 33 during development without landing a distinct schema at that number. Version 34 added ordering columns to class feat slots and crafting ingredients, recipe outputs and recipe costs, plus recipe `focus_path`. Version 35 added named-entity slugs and the `entity_identity` index. Version 36 adds `race_ability_modifiers.sort_order` so an ability list retains its source order without relying on stat IDs or row IDs. Version 37 replaces the dataset export blob with citation, removal, note, pack-exception and source-alias tables; `dataset_version` keeps only the upstream SHA and build time. It also adds `sort_order` to `item_materials`, `triggers`, `weapon_proficiencies` and `vendor_currencies`. Currency evidence moves from its `citation` and `read` columns into `citations`, like other named entities. Source records live in `quest_drops`, `pack_drops`, `quest_chain_rewards`, `saga_rewards`, `vendor_offers`, `event_rewards`, `starter_gear`, `crafting_outputs` and `challenge_rewards`, with a read-only `sources` union. Ordered `vendor_offer_costs` rows reference an ingredient kind and ID; currency names live in `vendor_currencies`. Named tables require a non-null slug. Source owners and price ingredients require kind/ID pairs, checked against their target tables; absent drop-text provenance stays paired null.

The data files' icon layout adds `stances/` images, copies enhancement header images beside their trees or into `icons/enhancements/`, and copies 69 filigree images into `icons/sets/`. Icon names now use the PNG's exact case so the paths also work on a case-sensitive server. The data icon command serves each image kind in its own folder; enhancement artwork is flattened from tree-local folders into the served `enhancements/` folder. Against the 8,719 XML-served PNGs, the data files add 382 copies: 266 stances, 69 sets, 25 enhancements, 19 augments, and 3 feats. The 25 enhancement copies include 24 tree headers. These copies give references a file in their own kind; the original files remain available.
The inherited data PNG filenames are restored to upstream casing, with corresponding icon references corrected on both build paths; the complete spelling audit is in `data-icons.md`. No existing served PNG path is removed or renamed.
The Blight Caster spell Stormrage now serves `StormRage`, the exact name of its PNG, on both build paths; its data-file note and XML correction record Maetrim's `Stormrage` spelling.

## Pack release metadata

Adventure-pack list and detail responses add nullable `update` and `released` fields. `released` is an ISO date (`YYYY-MM-DD`); both fields are sortable on the list route. The data files record updates for all 67 packs and dates for 65. Delera's Tomb and Ruins of Threnal shipped in the public beta and retain null dates.

The XML input supplies pack names through quests, with no release metadata. Its two new columns therefore remain null. The XML-versus-data allow-list retains the 160 vendor-price rows and adds the authored release values, their 67 citations and 22 research annotations, and the resulting pack responses. Release notes create no correction rows. Folder prefixes and the characters/mechanics/upstream reorganization change no IDs, slugs or other authored rows.

## Round 10 image and integrity checks

Optional-buff `icon` values now give a path relative to `/icons`, without `.png`, on both builds. The 131 available images use `spells/<name>`, `feats/<name>`, `enhancements/<name>` or `enhancements/<tree>/<name>` as appropriate. Single-kind owners retain their bare image names. Clients can fetch `/icons/<icon>.png` for an optional buff without guessing its image kind. Tree-local paths and the existing flat enhancement paths are hard links to the same generated image; no source PNG is duplicated or removed.

Seven optional-buff icons become null because the referenced artwork is absent: Shadowblade and Thieves Tools: Masterwork, +1, +2, +3, +4 and +5. Each has an icon note preserving Maetrim's reference. These join the 18 previously documented missing images. Every image field, including enhancement backgrounds and both class icons, is validated against its declaration with exact filename case.

`requirements.sort_order` again means the zero-based position within its group, as on main. An intermediate branch build assigned an interleaved rank across groups (Vorpal Strikes: `0, 3, 1, 4, 2`); both builds now retain `0, 1, 0, 1, 0`. Group indexes and served requirement arrays are unchanged.

`check-db` runs database checks without XML or data files and reads pack exceptions from `pack_exceptions`. With an explicit `--source`, it also runs `buff_types_not_classified` and `effect_definitions_with_partially_mapped_targets`: these audit how Maetrim's buff definitions map to our vocabulary, including source facts that are not database records. The stored-modifier check `effect_types_not_classified` always runs. These two source audits belong to XML reading and, later, the importer.

## Duplicate audit on the 31ef0201 dataset

The old API reported 1,065 feat effect links, 28,654 item effect links, 17,626 modifiers and 9,919 requirements. The shared XML and data pipelines remove 11, 41 and 5 duplicate rows respectively. Three requirements belonged to one of the removed modifiers. These counts describe the same 31ef0201 input; ba7e18e also adds and retires upstream records.

| Owner | Repeated effect link | Copies before cleanup |
| --- | --- | ---: |
| Greater Shield Mastery | Physical Resistance Rating +5 | 4 |
| Improved Critical: Piercing Weapons | Critical Threat Range (Dagger) | 2 |
| Improved Shield Mastery | Physical Resistance Rating +2 | 2 |
| Improved Shield Mastery | Physical Resistance Rating +5 | 2 |
| Knight's Training | Critical Threat Range (Battle Axe) | 2 |
| Scion of the Astral Plane | Doubleshot +4 | 2 |
| Scion of the Astral Plane | Doublestrike +4 | 2 |
| Shield Mastery | Physical Resistance Rating +3 | 2 |
| Shield Mastery Specialty | Physical Resistance Rating +10 | 2 |

Those nine feat groups account for eleven removed links. The item links removed are:

| Owner | Repeated effect link | Copies before cleanup |
| --- | --- | ---: |
| Barovian's Sickle | Silver, Alchemical | 2 |
| Breeze | Telekinetic | 2 |
| Circle of Hatred | Incite +20 | 2 |
| Dark Blue Ioun Stone | Listen +3 | 2 |
| Epic Cavalry Plate (level 24) | Greater False Life | 2 |
| Epic Mournlode Light Mace | Undead Bane 4 | 2 |
| Epic Mournlode Longsword | Undead Bane 4 | 2 |
| Epic Mournlode Maul | Undead Bane 4 | 2 |
| Epic Mournlode Shortsword | Undead Bane 4 | 2 |
| Epic Mournlode Warhammer | Undead Bane 4 | 2 |
| Epic Ring of the Buccaneer (legacy) (level 20) | Good Luck +1 | 2 |
| Epic Templar's Justice | Evil Characters | 2 |
| Epic Templar's Retribution | Evil Characters | 2 |
| Legendary Bow of the Elements (Air) | Shocking Blast VI | 2 |
| Legendary Templar's Justice | Evil Characters | 2 |
| Legendary Templar's Retribution | Evil Characters | 2 |
| Libram of Silver Magic | Charisma +10; Intelligence +10; Spell Lore I +1 | 2 of each |
| Lynnabel's Bug Killer (level 5) | Vermin Bane 2 | 4 |
| Lynnabel's Bug Killer (level 10) | Vermin Bane 3 | 4 |
| Lynnabel's Bug Killer (level 15) | Vermin Bane 4 | 4 |
| Lynnabel's Bug Killer (level 20) | Vermin Bane 4 | 4 |
| Lynnabel's Bug Killer (level 25) | Vermin Bane 5 | 4 |
| Lynnabel's Bug Killer (level 30) | Vermin Bane 6 | 4 |
| Radiant Arc | Everbright | 2 |
| Ring of Lies | Deception +3 | 2 |
| Templar's Justice | Evil Characters | 2 |
| Templar's Retribution | Evil Characters | 2 |

Those 29 item groups account for 41 removed links. The five repeated modifiers were Vile Chemist: Hidden Blades III (`WeaponCriticalMultiplierClass`, with three requirements), Swashbuckler: Swashbuckling (`Weapon_CriticalRange`), Grandmaster of Flowers: Disciple of Material (`PRR`), Grandmaster of Flowers: Disciple of Morality (`MRR`), and Martial: Flaming Burst, Flaming Blast, Seeker +10 (`WeaponOtherDamageBonusCritical`). Each had two copies.

Completionist had no identical feat effect link in the 31ef0201 database. Its observed `bonuses[]` change was order: effect bonuses had been sorted by stat name during rendering. The response now follows each effect's stored bonus position.

The four names `Base`, `Elemental Energy`, `Luck` and `Reaper` called out by the old-to-new API crawl are bonus types within `/v1/effects` entries, not effect names. All four bonus types and their effect-vocabulary associations occur on both database sides; their reported array positions shifted when the surrounding effect list moved. The crawl compared 31ef0201 with ba7e18e, so the array index alone cannot establish that one of these names was added or removed.

## Completed 31ef0201-to-ba7e18e crawl

The round 9 crawl used the old API's own `/v1/dump.sqlite` for response identities and compared the branch's ba7e18e XML and data databases as its table baseline. It completed 104 tables and 15,419 response comparisons: 160 table differences and 15,292 response differences. The table differences are exactly the authored Lukresh prices. Detail coverage was 15,201 paired, 32 removed, 21 added and 388 failed to pair.

All 388 failed details have an ambiguous baseline name with no `variant`, with the same number of rows of that name on both sides. They comprise 172 augment, 40 feat, 105 enhancement-tree, 39 effect, 29 crafting-system, two class and one race detail. Some failures arise from a nested reference rather than the detail's own name. The 121 ambiguous names occur in augments, feats, crafting ingredients, enhancements and enhancement selections; they include the Green Steel pairs, Twilight, Legendary Affirmation, Lay on Hands, Watchful Eye, repeated ingredients and repeated enhancement choices. There is no other failure cause. The log prints 572 ambiguity diagnostics because a nested failure can appear on both sides; eight of those diagnostics concern four list pages. Those details and list pages are not verified equivalent by this crawl.

The JSON differences cover 465 path shapes, counting the route, a path with numeric array positions replaced by `[]`, and whether a value, length or key differs. Of these, 190 contain only the documented changes above, 214 contain only upstream-version changes, and 61 contain both. No path shape remains unexplained. Among the 258,366 individual JSON diagnostics, 228,191 reflect documented changes and 30,171 reflect upstream changes. Four aggregate count diagnostics combine both causes: feat effect links, item effect links, modifiers and requirements. A response can contribute many diagnostics, so these are not counts of changed entities.

The upstream differences include these families:

- Fourteen feats are added and fifteen retired, including the nine new cantrips and the revised Master feats. Seven derived effect names are added and seventeen retired. The resulting list-position shifts also change the names, IDs, counts and bonus-type fields compared at each array index.
- Maetrim now supplies 108 previously wiki-supplied items and the quest Terror of the Demon Lords. Their descriptions, icons, weapon and armor stats, effect lines, sockets, clickies and wiki URLs follow those records. Twenty raid drops acquire `end chest` from their upstream drop text. The 106 newly filled item icons are distinct from the documented omission of the two Null Blade icons.
- Spell balance and class-list edits change damage, maximum caster level, costs, descriptions and class associations. Class automatic feat arrays change with the cantrips. Feat and enhancement edits change ranks, costs, prerequisites, DCs, modifiers and descriptions; these are not changes to requirement-group indexing.
- Lamordia augment target and amount edits change effect links and their carrier lists; Cannith augment levels gain a tier. Purity's rare Healing Amplification filigree changes PRR to MRR, Frozen Wanderer changes Cold Spell Critical Chance to Damage, and Champion of the Twins gains set membership. Derived effect templates and carrier values follow the changed upstream buffs and owners.

The comparison walks arrays by position. Recipe ingredient/output reordering and effect-bonus ordering are documented changes; an upstream addition or removal also shifts later array positions. The audit compares owner/link membership and resolved bonus values independently of those positions before attributing mixed paths. It does not treat every changed array element as a new balance change or every missing old effect name as lost data.

## Icons served from the data files (2026-10-17)

The deploy builds icons from the data files (`ddo-etl icons --data data`). Every path the XML build served (8,735) is served with identical bytes; 3,014 further paths are served that the XML build did not place, the tree-local enhancement images and the per-kind copies the data files hold. No path disappeared.
