use serde_json::Value;
use utoipa::openapi::{OpenApi, RefOr};

macro_rules! examples {
    ($version:literal: $(($path:literal, $file:literal)),* $(,)?) => {
        pub const EXAMPLES: &[(&str, &str)] =
            &[$(($path, include_str!(concat!("../../../docs/examples/", $version, "/", $file, ".json")))),*];
    };
}
pub(crate) use examples;

pub fn attach_examples(api: &mut OpenApi, examples: &[(&str, &str)]) {
    for (path, raw) in examples {
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
