use super::kinds::*;
use super::schemas::FileTemplate;

macro_rules! exemplar {
    ($file:ty, $text:expr) => {
        impl FileTemplate for $file {
            const EXEMPLAR: &'static str = $text;
        }
    };
}

macro_rules! whole_entity_citation {
    ($prefix:literal) => {
        concat!(
            "\n[[",
            $prefix,
            "citation]]\nurl = \"https://ddowiki.com/page/Template\"\nread = \"2026-10-18\"\nnote = \"Template: the page read and what it confirms.\"\n"
        )
    };
}

exemplar!(
    AugmentFile,
    concat!(
        "name = \"Template Augment\"\ndescription = \"Template description.\"\nfamily = \"Named\"\nmin_level = 1\n\n[[slot]]\nslot = \"red\"\n",
        whole_entity_citation!("")
    )
);
exemplar!(
    AugmentSlotTypesFile,
    concat!(
        "[[row]]\nfamily = \"crafting\"\nlabel = \"crafting: template socket\"\nvariant = \"template socket\"\n",
        whole_entity_citation!("row.")
    )
);
exemplar!(BonusTypesFile, concat!("[[row]]\nname = \"Template Bonus\"\n", whole_entity_citation!("row.")));
exemplar!(
    ClassFile,
    concat!(
        "name = \"Template Class\"\nalignments = [\"Any\"]\nbab = [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20]\ndescription = \"Template description.\"\nfortitude = \"good\"\nhit_points = 10\nlarge_icon = \"Fighter\"\nreflex = \"poor\"\nskill_points = 2\nsmall_icon = \"Fighter_Small\"\nspell_points_per_level = [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]\nwill = \"poor\"\n",
        whole_entity_citation!("")
    )
);
exemplar!(
    ClickiesFile,
    concat!(
        "[[row]]\nname = \"Template Clickie\"\ndescription = \"Template description.\"\nicon = \"Beholder\"\n",
        whole_entity_citation!("row.")
    )
);
exemplar!(
    CraftingSystemFile,
    concat!("name = \"Template Crafting\"\npage = \"https://ddowiki.com/page/Template\"\n", whole_entity_citation!(""))
);
exemplar!(
    DamageTypesFile,
    concat!("[[row]]\ncategory = \"elemental\"\nname = \"Template Damage\"\n", whole_entity_citation!("row."))
);
exemplar!(
    EffectFile,
    concat!(
        "name = \"Template Effect\"\ndescription_template = \"Template description.\"\nverbose_name_template = \"Template Effect\"\n",
        whole_entity_citation!("")
    )
);
exemplar!(EffectTierFile, concat!("name = \"Template Tier\"\n", whole_entity_citation!("")));
exemplar!(
    EnhancementTreeFile,
    concat!(
        "name = \"Template Tree\"\nbackground = \"ElfBackground\"\nicon = \"AAAbilityScore\"\nkind = \"racial\"\nversion = 1\n",
        whole_entity_citation!("")
    )
);
exemplar!(
    EquipmentSlotsFile,
    concat!("[[row]]\ncategory = \"accessory\"\nname = \"Template Slot\"\n", whole_entity_citation!("row."))
);
exemplar!(EventFile, concat!("name = \"Template Event\"\n", whole_entity_citation!("")));
exemplar!(
    FeatCategoryFile,
    concat!(
        "[[row]]\nname = \"Template Feat\"\nacquire = \"Train\"\ndescription = \"Template description.\"\n\n[[row.group]]\ngroup_name = \"Deity\"\n",
        whole_entity_citation!("row.")
    )
);
exemplar!(FiligreeSetFile, concat!("[[row]]\nname = \"Template Filigree\"\ndescription = \"Template description.\"\nicon = \"Electrocution\"\nmenu = \"Ravenloft\"\nset = \"Electrocution\"\n", whole_entity_citation!("row.")));
exemplar!(
    StatGroupFile,
    concat!("name = \"Template Stat Group\"\ndescription_template = \"Template description.\"\nverbose_name_template = \"Template Stat Group\"\n", whole_entity_citation!(""))
);
exemplar!(
    GuildBuffsFile,
    concat!(
        "[[row]]\nname = \"Template Guild Buff\"\ndescription = \"Template description.\"\nguild_level = 1\n",
        whole_entity_citation!("row.")
    )
);
exemplar!(
    ItemFile,
    concat!(
        "name = \"Template Item\"\ndescription = \"Template description.\"\nitem_category = \"Jewelry\"\nminimum_level = 1\nslot = \"Ring\"\n",
        whole_entity_citation!("")
    )
);
exemplar!(ItemMaterialsFile, concat!("[[row]]\nname = \"Template Material\"\n", whole_entity_citation!("row.")));
exemplar!(
    OptionalBuffsFile,
    concat!(
        "[[row]]\nname = \"Template Buff\"\ndescription = \"Template description.\"\n",
        whole_entity_citation!("row.")
    )
);
exemplar!(PackFile, concat!("name = \"Template Pack\"\n", whole_entity_citation!("")));
exemplar!(PatronFile, concat!("name = \"Template Patron\"\n", whole_entity_citation!("")));
exemplar!(
    QuestFile,
    concat!(
        "name = \"Template Quest\"\npack = \"Free to Play\"\nfavor = 1\ndifficulties = [\"normal\"]\n\n[[run]]\ntier = \"heroic\"\nlevel = 1\n",
        whole_entity_citation!("")
    )
);
exemplar!(
    QuestSeriesFile,
    concat!(
        "name = \"Template Series\"\npack = \"Free to Play\"\nquests = [\"The Grotto\"]\n",
        whole_entity_citation!("")
    )
);
exemplar!(
    RaceFile,
    concat!("name = \"Template Race\"\ndescription = \"Template description.\"\nshort_name = \"Template\"\nstarting_world = \"Forgotten Realms\"\n", whole_entity_citation!(""))
);
exemplar!(
    SentientGemFile,
    concat!(
        "name = \"Template Gem\"\ndescription = \"Template description.\"\nicon = \"SentientJewel_Green\"\n",
        whole_entity_citation!("")
    )
);
exemplar!(SetFile, concat!("name = \"Template Set\"\n", whole_entity_citation!("")));
exemplar!(
    SpellListFile,
    concat!(
        "[[row]]\nname = \"Template Spell\"\ndescription = \"Template description.\"\n",
        whole_entity_citation!("row.")
    )
);
exemplar!(
    StancesFile,
    concat!(
        "[[row]]\nname = \"Template Stance\"\ndescription = \"Template description.\"\nicon = \"TwoWeaponFighting\"\n",
        whole_entity_citation!("row.")
    )
);
exemplar!(StatFile, concat!("name = \"Template Stat\"\ncategory = \"magical\"\n", whole_entity_citation!("")));
exemplar!(TriggersFile, concat!("[[row]]\nname = \"Template Trigger\"\n", whole_entity_citation!("row.")));
exemplar!(
    VendorFile,
    concat!(
        "name = \"Template Vendor\"\nlocation = \"Template location.\"\npack = \"Free to Play\"\n",
        whole_entity_citation!("")
    )
);
exemplar!(VendorCurrenciesFile, concat!("[[row]]\nname = \"Template Token\"\n", whole_entity_citation!("row.")));
exemplar!(
    WeaponProficienciesFile,
    concat!("[[row]]\nname = \"Template Proficiency\"\n", whole_entity_citation!("row."))
);
exemplar!(
    WeaponTypesFile,
    concat!("[[row]]\nname = \"Template Weapon\"\nproficiency = \"Simple\"\n", whole_entity_citation!("row."))
);
