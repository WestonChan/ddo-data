use serde_json::Value;
use utoipa::openapi::{OpenApi, RefOr};

macro_rules! examples {
    ($(($path:literal, $file:literal)),* $(,)?) => {
        pub const EXAMPLES: &[(&str, &str)] = &[$(($path, include_str!(concat!("../docs/examples/", $file, ".json")))),*];
    };
}

examples! {
    ("/v1/version", "version"),
    ("/v1/stats", "stats"),
    ("/v1/bonus-types", "bonus-types"),
    ("/v1/equipment-slots", "equipment-slots"),
    ("/v1/weapon-types", "weapon-types"),
    ("/v1/damage-types", "damage-types"),
    ("/v1/augment-slot-types", "augment-slot-types"),
    ("/v1/adventure-packs", "adventure-packs"),
    ("/v1/patrons", "patrons"),
    ("/v1/quests", "quests"),
    ("/v1/items", "items"),
    ("/v1/items/{id}", "items_id"),
    ("/v1/augments", "augments"),
    ("/v1/augments/{id}", "augments_id"),
    ("/v1/sets", "sets"),
    ("/v1/sets/{id}", "sets_id"),
    ("/v1/filigrees", "filigrees"),
    ("/v1/feats", "feats"),
    ("/v1/feats/{id}", "feats_id"),
    ("/v1/races", "races"),
    ("/v1/races/{id}", "races_id"),
    ("/v1/classes", "classes"),
    ("/v1/classes/{id}", "classes_id"),
    ("/v1/enhancement-trees", "enhancement-trees"),
    ("/v1/enhancement-trees/{id}", "enhancement-trees_id"),
    ("/v1/spells", "spells"),
    ("/v1/spells/{id}", "spells_id"),
    ("/v1/clickies", "clickies"),
}

pub fn attach_examples(api: &mut OpenApi) {
    for (path, raw) in EXAMPLES {
        let example: Value = serde_json::from_str(raw).unwrap_or_else(|e| panic!("example for {path}: {e}"));
        let item = api.paths.paths.get_mut(*path).unwrap_or_else(|| panic!("example for unknown route {path}"));
        let operation = item.get.as_mut().unwrap_or_else(|| panic!("{path} has no GET operation"));
        let Some(RefOr::T(response)) = operation.responses.responses.get_mut("200") else {
            panic!("{path} has no inline 200 response");
        };
        let content = response
            .content
            .get_mut("application/json")
            .unwrap_or_else(|| panic!("{path} 200 response is not application/json"));
        content.example = Some(example);
    }
}
