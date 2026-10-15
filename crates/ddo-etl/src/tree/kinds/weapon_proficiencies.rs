use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct WeaponProficienciesFile {
    pub(crate) row: Vec<super::VocabularyTerm>,
}

declare_vocabulary_file!("weapon_proficiencies", WeaponProficienciesFile);
