use serde_json::Value;
use utoipa::openapi::example::ExampleBuilder;
use utoipa::openapi::{OpenApi, RefOr};

pub struct ResponseExample {
    pub route: &'static str,
    pub file_name: &'static str,
    pub example_json: &'static str,
    pub name: Option<&'static str>,
}

macro_rules! declare_response_examples {
    ($api_version:literal: $(($route:literal, $example_file:literal $(, $name:literal)?)),* $(,)?) => {
        pub(crate) const RESPONSE_EXAMPLES: &[crate::docs::ResponseExample] = &[$(crate::docs::ResponseExample {
            route: $route,
            file_name: concat!($example_file, ".json"),
            example_json: include_str!(concat!("../../../docs/examples/", $api_version, "/", $example_file, ".json")),
            name: declare_response_examples!(@name $($name)?),
        }),*];
    };
    (@name $name:literal) => { Some($name) };
    (@name) => { None };
}
pub(crate) use declare_response_examples;

pub(crate) fn attach_examples(spec: &mut OpenApi, examples: &[ResponseExample]) {
    for ResponseExample { route, example_json, name, .. } in examples {
        let example: Value = serde_json::from_str(example_json).unwrap_or_else(|e| panic!("example for {route}: {e}"));
        let path_item = spec.paths.paths.get_mut(*route).unwrap_or_else(|| panic!("example for unknown route {route}"));
        let operation = path_item.get.as_mut().unwrap_or_else(|| panic!("{route} has no GET operation"));
        let Some(RefOr::T(response)) = operation.responses.responses.get_mut("200") else {
            panic!("{route} has no inline 200 response");
        };
        let json_content = response
            .content
            .get_mut("application/json")
            .unwrap_or_else(|| panic!("{route} 200 response is not application/json"));
        if let Some(name) = name {
            json_content.example = None;
            json_content
                .examples
                .insert((*name).to_string(), ExampleBuilder::new().summary(*name).value(Some(example)).into());
        } else {
            json_content.example = Some(example);
        }
    }
}
