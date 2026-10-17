# DDOBuilderV2 import fixtures

The `ba7e18e/DataFiles` world comes from upstream commit
`ba7e18eecafb1bb5fd6b6cf52cc0b6395bbeb20c`. XML files are verbatim byte
subsequences of that checkout, made by deleting complete elements only. Headers,
comments and retained nested elements are unchanged. PNGs are verbatim copies
in their upstream kind folders, including cross-kind dc art. No placeholder art
is used. Missing optional directories contain no retained entities.

The two Ravager enhancements live in separate Barbarian and Fighter tree files;
both headers are retained. The future d1350a0c fixture will be an overlay trimmed
to the same elements.

SHA columns below are SHA-256 of the full upstream file and the retained fixture.

| File under `ba7e18e/DataFiles` | Kept | Upstream SHA-256 | Fixture SHA-256 |
|---|---|---|---|
| `ClassImages/Alchemist.png` | Verbatim PNG referenced by retained XML | `7e6457558942589f379338e1de4349fca0d80932ad03b00c5219d258ca49d45a` | `7e6457558942589f379338e1de4349fca0d80932ad03b00c5219d258ca49d45a` |
| `ClassImages/Artificer.png` | Verbatim PNG referenced by retained XML | `bb506fe8fe3d5ddd3f115298a1760703fd9479908519b963b54c2a7adfebde77` | `bb506fe8fe3d5ddd3f115298a1760703fd9479908519b963b54c2a7adfebde77` |
| `ClassImages/Barbarian.png` | Verbatim PNG referenced by retained XML | `d6f0e105fcea8f7a462fbb4683dff8d95430e1b5af4d6dc7cf6b73e94e81e1b5` | `d6f0e105fcea8f7a462fbb4683dff8d95430e1b5af4d6dc7cf6b73e94e81e1b5` |
| `ClassImages/Fighter.png` | Verbatim PNG referenced by retained XML | `aa8efe4983a6379f8176f502fde323f15e3381e79249cfe95279d6f3170258c4` | `aa8efe4983a6379f8176f502fde323f15e3381e79249cfe95279d6f3170258c4` |
| `EnhancementImages/ATShockGauntlet.png` | Verbatim PNG referenced by retained XML | `2e5fa4b8b86eed455d245c291e2ea12020c1669871c95633d0eff6c8d1aa0a9f` | `2e5fa4b8b86eed455d245c291e2ea12020c1669871c95633d0eff6c8d1aa0a9f` |
| `EnhancementImages/ActionBoostAttack.png` | Verbatim PNG referenced by retained XML | `7cc59e5c99f81e8974666b6a0606c0c98b913a43308d1dd260f2d126b4b9df9e` | `7cc59e5c99f81e8974666b6a0606c0c98b913a43308d1dd260f2d126b4b9df9e` |
| `EnhancementImages/Artificer.png` | Verbatim PNG referenced by retained XML | `8220aad4047616fe7be57a75a920b83d92a52c008d0b5f5cf21a06d387fe85b8` | `8220aad4047616fe7be57a75a920b83d92a52c008d0b5f5cf21a06d387fe85b8` |
| `EnhancementImages/Barbarian.png` | Verbatim PNG referenced by retained XML | `6f0b5dadbfc13f0a579e33114e56ed2dcf66916d04ec782838630253418d86d0` | `6f0b5dadbfc13f0a579e33114e56ed2dcf66916d04ec782838630253418d86d0` |
| `EnhancementImages/BomAlchemicalResistance.png` | Verbatim PNG referenced by retained XML | `b704e62af88d6663832f802c0fc734f8b0715da7c02b3d55d275ee9d2a5a11a5` | `b704e62af88d6663832f802c0fc734f8b0715da7c02b3d55d275ee9d2a5a11a5` |
| `EnhancementImages/BomAugmentation.png` | Verbatim PNG referenced by retained XML | `6879c34cfb5ae719875c17d9e8e70e1fcb91bbe94cb3560b993fa5c9d5ea6ba8` | `6879c34cfb5ae719875c17d9e8e70e1fcb91bbe94cb3560b993fa5c9d5ea6ba8` |
| `EnhancementImages/BomBurningAmbition.png` | Verbatim PNG referenced by retained XML | `e954d86c641007ca837f247a2d090a8ef5376749489baebb718f3c656b727b67` | `e954d86c641007ca837f247a2d090a8ef5376749489baebb718f3c656b727b67` |
| `EnhancementImages/BomMultivial.png` | Verbatim PNG referenced by retained XML | `e5dc60f5c412f736857d52aa6a7eb7f53e01e2c84a43b59b2529f4f347fed1b9` | `e5dc60f5c412f736857d52aa6a7eb7f53e01e2c84a43b59b2529f4f347fed1b9` |
| `EnhancementImages/BomSpellvialSelection.png` | Verbatim PNG referenced by retained XML | `13756d47153c99f5003a54568769ff1eca11eab6a0089521db5d6513ceb71e43` | `13756d47153c99f5003a54568769ff1eca11eab6a0089521db5d6513ceb71e43` |
| `EnhancementImages/BomSpellvialSelectionII.png` | Verbatim PNG referenced by retained XML | `10592975e685d0c15a536571d25cde824cc194056e0774cc777d13572f183f79` | `10592975e685d0c15a536571d25cde824cc194056e0774cc777d13572f183f79` |
| `EnhancementImages/BomVenomVial.png` | Verbatim PNG referenced by retained XML | `1557fd088681464881e0580e0ae3593091619f351ab4cd08c55cafa15f781ba5` | `1557fd088681464881e0580e0ae3593091619f351ab4cd08c55cafa15f781ba5` |
| `EnhancementImages/BomVialOfAcid.png` | Verbatim PNG referenced by retained XML | `02c373d2cd17a77e1184edf2125d9f7979073b0116ad2d484d85547bed9c933d` | `02c373d2cd17a77e1184edf2125d9f7979073b0116ad2d484d85547bed9c933d` |
| `EnhancementImages/BomVialOfFlame.png` | Verbatim PNG referenced by retained XML | `8e557b8c595e9ae0dd9e61f7c87e6a9ab2bc9ff7c515fc0b809a0dda2469b74d` | `8e557b8c595e9ae0dd9e61f7c87e6a9ab2bc9ff7c515fc0b809a0dda2469b74d` |
| `EnhancementImages/BomVialOfFrost.png` | Verbatim PNG referenced by retained XML | `84598f502a21d9bb96173b3948053a8013c9d3fcda255cc09f5726a99603540a` | `84598f502a21d9bb96173b3948053a8013c9d3fcda255cc09f5726a99603540a` |
| `EnhancementImages/BomVialOfSparks.png` | Verbatim PNG referenced by retained XML | `eec5117d8a05e615b8ebfdf7e7ad9eccc34e2a9ed2feef2ce71eae374e0ee613` | `eec5117d8a05e615b8ebfdf7e7ad9eccc34e2a9ed2feef2ce71eae374e0ee613` |
| `EnhancementImages/Fighter.png` | Verbatim PNG referenced by retained XML | `d4e73a60541f36fb94a6425890dd57598411c9e6e9fdf151e8e48546e39a823c` | `d4e73a60541f36fb94a6425890dd57598411c9e6e9fdf151e8e48546e39a823c` |
| `EnhancementImages/Half-Orc.png` | Verbatim PNG referenced by retained XML | `a0e38d5a5af122eb00a418af4be25d5419a4612bd78ed186a78f465713a71c7d` | `a0e38d5a5af122eb00a418af4be25d5419a4612bd78ed186a78f465713a71c7d` |
| `EnhancementImages/HalfOrcLesserDragonmarkOfFinding.png` | Verbatim PNG referenced by retained XML | `e9ed2f0ccabaef1b6f15854074dc9958eb6670af88466e52b9992ed15c46e7a6` | `e9ed2f0ccabaef1b6f15854074dc9958eb6670af88466e52b9992ed15c46e7a6` |
| `EnhancementImages/RavBarbarianPowerAttack.png` | Verbatim PNG referenced by retained XML | `7b9e46818ead89c9d7393ade8515e43ce95719d6da4a326b4ecef367a6f8d43f` | `7b9e46818ead89c9d7393ade8515e43ce95719d6da4a326b4ecef367a6f8d43f` |
| `EnhancementImages/ReaperTree.png` | Verbatim PNG referenced by retained XML | `351c36ed0fe36bdf77f2b9678db32cbb602793c1e9fc2b8c0141383480dad583` | `351c36ed0fe36bdf77f2b9678db32cbb602793c1e9fc2b8c0141383480dad583` |
| `EnhancementImages/SDDefenseBoost.png` | Verbatim PNG referenced by retained XML | `34195660ec5b604934d5d32c802eef55048b84711410531df711e000e18934ea` | `34195660ec5b604934d5d32c802eef55048b84711410531df711e000e18934ea` |
| `EnhancementTrees/Alchemist_Bombardier.tree.xml` | Tree header and BomCore1, BomCore6, BomSpellVialSelection, BomSpellvialSelectionII, BomSpellvialSelectionIII, BomBurningAmbition, BomAugmentation | `b2f0178f4d82f8971cb1c3fde4ab391ae7213eb7a6be613250a68cf564fac1ca` | `7959bbd965d660ba06ff92bd277b914147be1d28cd0bb08da4cec80ecd3ea724` |
| `EnhancementTrees/Artificer_Arcanotechnician.tree.xml` | Tree header and ATShockGauntlet | `86acd0734d07f4fce7bc0996dea81d4e27e53640cd2c87f1c0adc6efa4322bf6` | `d099e148d65f0622ae21692f9020de3eb646b2f245d01ca7d81be5d4d364415c` |
| `EnhancementTrees/Barbarian_Ravager.tree.xml` | Tree header and RavBarbarianPowerAttack | `416dd2dcefa9f2fbf27e7cccac0546664bd73bd0257f83d413d05d0acd0b7469` | `f35e450eed3b7da95864efef5d48f6ebeb33ba08775ac77ce1b3b133a17994fa` |
| `EnhancementTrees/Fighter_Ravager.tree.xml` | Tree header and DLRavDraconicPowerAttack | `99534e5d2e66e5a6dd36657d025860fa219fe577b08b967849db2e36bdc9bdb0` | `a7c444f2b7fbe8954d37f5b728875f97cdd34dcf4d42535c85975904b2bb1ff3` |
| `EnhancementTrees/Fighter_StalwartDefender.tree.xml` | Tree header and SDDefenseBoost | `7f80190a192850efeb5c2472d236a86e0fc8f1b5193aa5d6491a8b790c88f2ee` | `05828dfa04a3c58abd8bade0739813ab43e774f821b5e6ccaf33582adc8a01e5` |
| `EnhancementTrees/Half-Orc.tree.xml` | Tree header and HalfOrcLesserDragonmarkOfFinding | `e166bc6aba3886bcf1b8409a7f4f6c4af720bdb7cd839bc47a9ae66237a18b14` | `27def2b97712e4c4b3922aed68f4ddce5d6a17ce5a6fe4d0bf16b3be5aae180f` |
| `FeatImages/Alertness.png` | Verbatim PNG referenced by retained XML | `4f627f382083efc9cead51cea8c9446dead51355f66a8557a2d50a0531a465e2` | `4f627f382083efc9cead51cea8c9446dead51355f66a8557a2d50a0531a465e2` |
| `FeatImages/Attack.png` | Verbatim PNG referenced by retained XML | `71ffdddc348ffe5660d4c059e7dedac91c1b5a2a51072e22858cc80826b7f39e` | `71ffdddc348ffe5660d4c059e7dedac91c1b5a2a51072e22858cc80826b7f39e` |
| `FeatImages/Blocking.png` | Verbatim PNG referenced by retained XML | `866f27cc219ab5fbe18740d442cf4eaa2fd85c5a050930d2513fd6f8c348660f` | `866f27cc219ab5fbe18740d442cf4eaa2fd85c5a050930d2513fd6f8c348660f` |
| `FeatImages/Half-Orc.png` | Verbatim PNG referenced by retained XML | `a0e38d5a5af122eb00a418af4be25d5419a4612bd78ed186a78f465713a71c7d` | `a0e38d5a5af122eb00a418af4be25d5419a4612bd78ed186a78f465713a71c7d` |
| `FeatImages/Knock.png` | Verbatim PNG referenced by retained XML | `bac4e8478bf60978c2f40867a16d314420949baf6fee48d5aa5afe6af0dbfc66` | `bac4e8478bf60978c2f40867a16d314420949baf6fee48d5aa5afe6af0dbfc66` |
| `Feats.xml` | Attack and Alertness, unchanged nested elements | `7298af081ba1a01b281f70bd2161b4c3789fa0ca5d674accaba532d32b1a00d9` | `ac3b0a097e78c469d2ef41ffb01a163989d5e0a98e415a063c2914b598669665` |
| `GuildBuffs.xml` | Root only; all GuildBuff elements deleted | `688ef9057ddbebe9e4af0789ad9364c2d33b9a6ad0456e1d7d072a97fc90b786` | `77af62fdbe993c2c465dfe3c48d4127cbceac94c855a2ba054184b9af1cccad5` |
| `ItemBuffs.xml` | Root only; all Buff elements deleted | `b4daa7b065c8a93a2f9c1d0cce7d6774ca3e7eff6ae2222b4753ea0c92723f46` | `af640a3eae449311ec54a6a1b0ad6375c4c8b2aaec95f7e4ccabd408ced44505` |
| `ItemClickies.xml` | Root only; all Spell elements deleted | `8cedc8dc4832ae9e233c72b4a74dbb73c159f07afa4b83dbf2d63ad7ff801c89` | `01ad3e655494483b7690ca6854bc2701ee51befc4dcbfbffb51ae36bb6aba505` |
| `Patrons.xml` | Patrons of retained quests: Battlehammer Expedition | `c8c135721b10ae31e05a768c8e95842d543a7f079ac5484ff66b58a2c2d3ca8f` | `a49f02ab6f84c8781d5d2b3dfb9354d735f86abc747318bf3d678b563d13bf60` |
| `Quests.xml` | All 14 Terror of Demogorgon quests | `203190d82828e6dac95e6052caec0e2357a3ad3eac6949f99788abb28d7a1b66` | `3ccb32238f1b06aa3ee3b8d9592267a2c3b0b10e573712ab7678bfdd17ac020e` |
| `SelfAndPartyBuffs.xml` | Root only; all OptionalBuff elements deleted | `81562ea9abbb4d9f9a83f89250889fa06270603b64f686c02bf25a170bdc4fb3` | `f0372c41db408d817ca6b9bb66518c06cba4d37732bcba6c48beca579463159b` |
| `Sentient.gems.xml` | Root only; all Gem elements deleted | `af948e03992f7a5e4cceb9b658a8c714c06d6b285b6f8f3ec213c91ff06297d4` | `1ccd2664f112f5bfd53864c80e39695850b4231afc29ee9ba2dc25401754e56a` |
| `SetBonusImages/Barbarian.png` | Verbatim PNG referenced by retained XML | `d6f0e105fcea8f7a462fbb4683dff8d95430e1b5af4d6dc7cf6b73e94e81e1b5` | `d6f0e105fcea8f7a462fbb4683dff8d95430e1b5af4d6dc7cf6b73e94e81e1b5` |
| `SetBonusImages/Fighter.png` | Verbatim PNG referenced by retained XML | `aa8efe4983a6379f8176f502fde323f15e3381e79249cfe95279d6f3170258c4` | `aa8efe4983a6379f8176f502fde323f15e3381e79249cfe95279d6f3170258c4` |
| `SetBonuses.xml` | Root only; all SetBonus elements deleted | `90e36e61a096897bc66297b301adbf9a6eeae95e9444fa18a6809b7af26e6991` | `ec95c9c4a95c76b648e9583c5e9eb0e8608015b2469b602336b12f9aadf77ac9` |
| `SpellImages/BomVenomVial.png` | Verbatim PNG referenced by retained XML | `1557fd088681464881e0580e0ae3593091619f351ab4cd08c55cafa15f781ba5` | `1557fd088681464881e0580e0ae3593091619f351ab4cd08c55cafa15f781ba5` |
| `SpellImages/Diplomacy.png` | Verbatim PNG referenced by retained XML | `517aec5e32588b493b04424f00dc886412104a0c0c16daaf523b3944147162ed` | `517aec5e32588b493b04424f00dc886412104a0c0c16daaf523b3944147162ed` |
| `SpellImages/Knock.png` | Verbatim PNG referenced by retained XML | `57179283f14832263a9895fd93f17191eb8b2d0e555dab658d4bf6596a3e1db2` | `57179283f14832263a9895fd93f17191eb8b2d0e555dab658d4bf6596a3e1db2` |
| `Spells.xml` | Root only; all Spell elements deleted | `e5b9168ce08e0132f72268aa61c977ef6279ea366320d2ec76ff1349742b13bd` | `108369978c15633e72fcc2734f1d39fb123a19e23f207403cd11bf048402a783` |
| `Stances.xml` | Root only; all Stance elements deleted | `0ef76cf3204fc538b3f1d5cb81a26225a3c9e1a0fcb22a18252321888a5bb66a` | `b190b54d446b35058b7c276fcab0716a384f62897705745171fbc0ce6e912648` |
| `UIImages/Alchemist.png` | Verbatim PNG referenced by retained XML | `e373e396eaa5f210e073b509dde614d63ce2d903fc4d9e297d9f8f73868655dd` | `e373e396eaa5f210e073b509dde614d63ce2d903fc4d9e297d9f8f73868655dd` |
| `UIImages/Artificer.png` | Verbatim PNG referenced by retained XML | `b458969477a06345c935743ab80748cf17fce840624f3c44b107ae756ee17362` | `b458969477a06345c935743ab80748cf17fce840624f3c44b107ae756ee17362` |
| `UIImages/ArtificerBackground.png` | Verbatim PNG referenced by retained XML | `eacadb10898d44bfbbd7f8dd08f76e1443ddeae2a5cff2293ae9006ab63027d4` | `eacadb10898d44bfbbd7f8dd08f76e1443ddeae2a5cff2293ae9006ab63027d4` |
| `UIImages/Barbarian.png` | Verbatim PNG referenced by retained XML | `9071f69e21b4aee05401f2147b20fe3e286dc75ec10f32784062805fdec80faa` | `9071f69e21b4aee05401f2147b20fe3e286dc75ec10f32784062805fdec80faa` |
| `UIImages/BarbarianBackground.png` | Verbatim PNG referenced by retained XML | `5a6fe7ea1b40a49f124717546edab0257a57d6503101e3b6b758c8fe1eff0408` | `5a6fe7ea1b40a49f124717546edab0257a57d6503101e3b6b758c8fe1eff0408` |
| `UIImages/Blocking.png` | Verbatim PNG referenced by retained XML | `866f27cc219ab5fbe18740d442cf4eaa2fd85c5a050930d2513fd6f8c348660f` | `866f27cc219ab5fbe18740d442cf4eaa2fd85c5a050930d2513fd6f8c348660f` |
| `UIImages/Bluff.png` | Verbatim PNG referenced by retained XML | `e510a349bdf7417ae46c4a9f9695d9e7b3a9b5abd5ea18633807b8dea6e65aa6` | `e510a349bdf7417ae46c4a9f9695d9e7b3a9b5abd5ea18633807b8dea6e65aa6` |
| `UIImages/Diplomacy.png` | Verbatim PNG referenced by retained XML | `517aec5e32588b493b04424f00dc886412104a0c0c16daaf523b3944147162ed` | `517aec5e32588b493b04424f00dc886412104a0c0c16daaf523b3944147162ed` |
| `UIImages/Fighter.png` | Verbatim PNG referenced by retained XML | `058e561c3277aac27e37af964cda1e8d0a630578cbb19b4a150666480fad86a3` | `058e561c3277aac27e37af964cda1e8d0a630578cbb19b4a150666480fad86a3` |
| `UIImages/FighterBackground.png` | Verbatim PNG referenced by retained XML | `8969b5f98a8c09890c60f933573d10b74d36b9e127fee6f0a998df5a6146769e` | `8969b5f98a8c09890c60f933573d10b74d36b9e127fee6f0a998df5a6146769e` |
| `UIImages/Half-Orc.png` | Verbatim PNG referenced by retained XML | `f61ab9cbd1dcdfd283bea655ae70b609226b4be97fc8796aa8985c9f385ff602` | `f61ab9cbd1dcdfd283bea655ae70b609226b4be97fc8796aa8985c9f385ff602` |
| `UIImages/Half-OrcBackground.png` | Verbatim PNG referenced by retained XML | `33c6e79de34422f8542ce4b0bc5744375432fab391897239b72be8ada89883cf` | `33c6e79de34422f8542ce4b0bc5744375432fab391897239b72be8ada89883cf` |
| `UIImages/Intimidate.png` | Verbatim PNG referenced by retained XML | `031f71d1e2a68f2f33d344bffa1816ff37f5aa18fe4410fe5e88c2f512cf8000` | `031f71d1e2a68f2f33d344bffa1816ff37f5aa18fe4410fe5e88c2f512cf8000` |
| `UIImages/ReaperTree.png` | Verbatim PNG referenced by retained XML | `7a1642bba79923680a9fce25a1534bfc8451bb1eac65daa3884598bfa143da65` | `7a1642bba79923680a9fce25a1534bfc8451bb1eac65daa3884598bfa143da65` |
