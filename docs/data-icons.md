# Data icons

The folder under `data/icons/<kind>/` holds Maetrim's image categories, such as `items`, `augments`, `stances`, or `enhancements`.

Items and augments use these shared image categories wherever they live under `data/packs/`, including the `shared`, `general` and `unsourced` pseudo-packs. Gear files live in `gear/` beneath a quest, crafting system, vendor, event, or pack; their icon kind remains `items`. Each sentient gem lives in `packs/shared/sentient-gems/<gem>.toml` and uses the `sentient-gems` icon kind. Moving a source record and its rewards does not move their PNGs. The data reader and icon lookup use the same file-kind classification; source records themselves have no item icon.

An enhancement tree lives at `characters/enhancements/<tree>/tree.toml`. Artwork referenced by only that tree lives beside it in `characters/enhancements/<tree>/icons/`. Artwork referenced by several trees stays in `icons/enhancements/`. The build looks in the owning tree first, then the global enhancement kind. The build requires exact spelling, rejects icons in another kind, and rejects unreferenced art in a tree's own folder.

`ddo-etl export --db` writes TOML from database tables only; PNGs are copied separately for a round trip because the database does not contain image bytes. `ddo-etl icons --data` combines the tree-local and global files into the flat `enhancements/` folder served by the API and copies the other image kinds without renaming them.

## Remaining case corrections

The following 26 corrections cover 27 rows: the Henshin Mystic selection occurs in both tree versions. Each data-file note sits beside its icon; the corresponding XML correction records the same upstream spelling. Nested spell stances, enhancement selections, and selection DCs retain their notes at that row.

| Kind | Owner and nested row | Upstream icon | PNG spelling |
| --- | --- | --- | --- |
| feat | Mithral Body | `Warforgedfeat` | `WarforgedFeat` |
| feat | Mithral Fluidity | `Warforgedfeat` | `WarforgedFeat` |
| spell | Lesser Evolution / IntEvolution | `intelligence` | `Intelligence` |
| spell | Evolution / IntEvolution | `intelligence` | `Intelligence` |
| spell | Greater Evolution / IntEvolution | `intelligence` | `Intelligence` |
| enhancement_tree | Shadar-Kai | `Shadar-Kai` | `Shadar-kai` |
| enhancement | Half-Elf: Elven Dexterity / Human Adaptability | `halfElfAbility` | `HalfElfAbility` |
| enhancement | Battle Engineer: Thunder Shock Weapon / Thunder-Shock Weapon (Crossbow) | `BEthunderShockWeaponCrossbow` | `BEThunderShockWeaponCrossbow` |
| enhancement | Battle Engineer: Thunder Shock Weapon / Thunder-Shock Weapon (Melee) | `BEthunderShockWeaponMelee` | `BEThunderShockWeaponMelee` |
| enhancement | Swashbuckler: Action Boost / Doubleshot | `SBDoubleshotboost` | `SBDoubleShotBoost` |
| enhancement | Swashbuckler: Swashbuckling Style II / Slap in the Face | `SBSlapIntheFace` | `SBSlapInTheFace` |
| enhancement | Half-Elf: Dilettante Special Ability I / Improved Spellcasting I | `HalfElfImprovedSpellCasting` | `HalfElfImprovedSpellcasting` |
| enhancement | Half-Elf: Dilettante Special Ability II / Improved Spellcasting II | `HalfElfImprovedSpellCasting` | `HalfElfImprovedSpellcasting` |
| enhancement | Henshin Mystic: Elemental Ki Strikes / Fists of Iron | `HMFIstsOfIron` | `HMFistsOfIron` |
| enhancement | Knight of the Chalice: Action Boost / Attack Boost | `KotCAttackBoost` | `KoTCAttackBoost` |
| enhancement | Tempest: Action Boost / Haste Boost | `TempestHasteBoost` | `TempestHasteboost` |
| enhancement | Arcane Trickster: Stolen Spell II / Stolen Spell - Sound Burst | `ATSoundBurst` | `ATSoundburst` |
| enhancement | Shadowdancer: Spell Focus / Abjuration | `ShadowdancerSpellFocusAbjuration` | `ShadowDancerSpellFocusAbjuration` |
| enhancement | Shadowdancer: Spell Focus / Enchantment | `ShadowdancerSpellFocusEnchantment` | `ShadowDancerSpellFocusEnchantment` |
| enhancement | Shadowdancer: Spell Focus / Illusion | `ShadowdancerSpellFocusIllusion` | `ShadowDancerSpellFocusIllusion` |
| enhancement | Shadowdancer: Spell Focus / Necromancy | `ShadowdancerSpellFocusNecromancy` | `ShadowDancerSpellFocusNecromancy` |
| enhancement | Archmage: School Mastery / Primary Spell Focus: Illusion | `AMPrimarySpellFocusIllusion` | `AMPrimarySpellFocusillusion` |
| enhancement | Archmage: School Mastery II / Primary Spell Focus: Illusion | `AMPrimarySpellFocusIllusion` | `AMPrimarySpellFocusillusion` |
| clickie | Greater Inferno | `Firestorm` | `FireStorm` |
| enhancement | Battle Engineer: Thunder Shock Weapon / Thunder-Shock Weapon (Crossbow) / Battle Engineer: Thunder-Shock Weapon (Crossbow) | `BEthunderShockWeaponCrossbow` | `BEThunderShockWeaponCrossbow` |
| enhancement | Battle Engineer: Thunder Shock Weapon / Thunder-Shock Weapon (Melee) / Battle Engineer: Thunder-Shock Weapon (Melee) | `BEthunderShockWeaponMelee` | `BEThunderShockWeaponMelee` |

## Restored upstream filename casing

A final comparison of the served folders found 45 PNGs whose filenames had been changed in earlier data exports, including six stance copies and one set copy. Their 38 distinct names are restored below. These require 59 additional correction entries covering 63 database rows; repeated enhancement versions share their correction. Notes stay beside the affected icon, including filigrees, enhancement DCs, and feat and enhancement stances. Export no longer renames an image to match a reference.

| Earlier data spelling | Upstream PNG spelling | Files restored |
| --- | --- | ---: |
| `AMMAsterOfMagic` | `AMMasterOfMagic` | 1 |
| `ArtificercraftMastery` | `ArtificerCraftMastery` | 1 |
| `BlackDragonBolt` | `BlackdragonBolt` | 1 |
| `DSHeadShot` | `DSHeadshot` | 1 |
| `Darkhallow` | `DarkHallow` | 2 |
| `EKBattleMage` | `EKBattlemage` | 1 |
| `ElfBindingShadows` | `ElfbindingShadows` | 1 |
| `EpicPastLifeColorsOftheQueen` | `EpicPastLifeColorsOfTheQueen` | 2 |
| `EpicPastLifeSkillMAstery` | `EpicPastLifeSkillMastery` | 2 |
| `FateSingersRepertoire` | `FatesingersRepertoire` | 1 |
| `IconicPastLifeShadar-Kai` | `IconicPastLifeShadar-kai` | 2 |
| `KotCAvengingCleave` | `KoTCAvengingCleave` | 1 |
| `KotCCensureDemons` | `KoTCCensureDemons` | 1 |
| `KotCCensureOutsiders` | `KoTCCensureOutsiders` | 1 |
| `KotCChampionOfGood` | `KoTCChampionOfGood` | 1 |
| `KotCCourageOfHeaven` | `KoTCCourageOfHeaven` | 1 |
| `KotCDivineMight` | `KoTCDivineMight` | 2 |
| `KotCDivineSacrifice` | `KoTCDivineSacrifice` | 1 |
| `KotCExaltedCleave` | `KoTCExaltedCleave` | 1 |
| `KotCExaltedSmite` | `KoTCExaltedSmite` | 1 |
| `KotCExtraSmite` | `KoTCExtraSmite` | 1 |
| `KotCHolyRetribution` | `KoTCHolyRetribution` | 1 |
| `KotCImprovedCourageOfHeaven` | `KoTCImprovedCourageOfHeaven` | 1 |
| `KotCImprovedTurning` | `KoTCImprovedTurning` | 1 |
| `KotCMeleeRangedPowerBoost` | `KoTCMeleeRangedPowerBoost` | 1 |
| `KotCSlayerOfEvil` | `KoTCSlayerOfEvil` | 2 |
| `KotCVigorOfLife` | `KoTCVigorOfLife` | 1 |
| `MobileSpellCasting` | `MobileSpellcasting` | 1 |
| `OSLessonsOfTravel` | `OSLEssonsOfTravel` | 1 |
| `RSHEalingDomain` | `RSHealingDomain` | 1 |
| `RavFuriousrage` | `RavFuriousRage` | 1 |
| `SESpellTearing` | `SESpelltearing` | 1 |
| `SSMaestroOfLifeAndDeath` | `SSMaestroOfLifeAnddeath` | 1 |
| `ShSpringsResurgence` | `SHSpringsResurgence` | 1 |
| `ShintaoMeditationOFWar` | `ShintaoMeditationOfWar` | 2 |
| `WSHoarFrost` | `WSHoarfrost` | 1 |
| `Wildstrike` | `WildStrike` | 1 |
| `toughenConstruct` | `ToughenConstruct` | 1 |
