use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct ItemMaterialsFile {
    pub(crate) row: Vec<super::VocabularyTerm>,
}

declare_vocabulary_file!("item_materials", ItemMaterialsFile);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vocabulary_order_follows_the_file() {
        let file: ItemMaterialsFile =
            toml::from_str("[[row]]\nname = 'Silver'\n[[row]]\nname = 'Adamantine'\n").unwrap();
        let db = rusqlite::Connection::open_in_memory().unwrap();
        db.execute_batch(ddo_model::ddl()).unwrap();
        file.write_vocabulary(
            &db,
            std::path::Path::new("effects/item-materials.toml"),
            &mut crate::tree::ids::StableIds::default(),
        )
        .unwrap();
        let restored = ItemMaterialsFile::from_rows(&db).unwrap();
        assert_eq!(toml::to_string(&restored).unwrap(), toml::to_string(&file).unwrap());
    }
}
