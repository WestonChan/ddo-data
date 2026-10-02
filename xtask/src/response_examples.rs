use anyhow::{bail, ensure, Context, Result};
use axum::body::Body;
use axum::http::{Request, StatusCode};
use ddo_api::{app, AppState};
use http_body_util::BodyExt;
use serde_json::{Map, Value};
use std::collections::BTreeSet;
use std::path::Path;
use tower::ServiceExt;

const TRIMMED_ARRAY_LENGTH: usize = 3;
const VERSION_EXAMPLE_NAME: &str = "version";
const EXAMPLES_KEPT_UNTIL_A_WIKI_FILE_RECORDS_A_ROW: &[&str] =
    &["quest-chains", "quest-chains_id", "sagas", "sagas_id", "vendors", "vendors_id", "events", "events_id"];

pub struct ExampleRequest<'a> {
    pub example_name: &'a str,
    pub path: &'a str,
}

const fn sample(example_name: &'static str, path: &'static str) -> ExampleRequest<'static> {
    ExampleRequest { example_name, path }
}

pub const EXAMPLE_REQUESTS: &[ExampleRequest<'static>] = &[
    sample("version", "/v1/version"),
    sample("stats", "/v1/stats"),
    sample("bonus-types", "/v1/bonus-types"),
    sample("enchantments", "/v1/enchantments?q=absorption"),
    sample("equipment-slots", "/v1/equipment-slots"),
    sample("weapon-types", "/v1/weapon-types"),
    sample("damage-types", "/v1/damage-types"),
    sample("augment-slot-types", "/v1/augment-slot-types"),
    sample("adventure-packs", "/v1/adventure-packs"),
    sample("adventure-packs_id", "/v1/adventure-packs/2"),
    sample("patrons", "/v1/patrons"),
    sample("quests", "/v1/quests"),
    sample("quests_id", "/v1/quests/86"),
    sample("quest-chains", "/v1/quest-chains"),
    sample("quest-chains_id", "/v1/quest-chains/1"),
    sample("sagas", "/v1/sagas"),
    sample("sagas_id", "/v1/sagas/1"),
    sample("vendors", "/v1/vendors"),
    sample("vendors_id", "/v1/vendors/1"),
    sample("events", "/v1/events"),
    sample("events_id", "/v1/events/1"),
    sample("items", "/v1/items?q=ring&enchantment=Strength&enchantment=Charisma&enchantment=Manslayer&limit=3"),
    sample("items_id", "/v1/items/7631"),
    sample("augments", "/v1/augments?slot=sun&limit=2"),
    sample("augments_id", "/v1/augments/1902"),
    sample("crafting-systems", "/v1/crafting-systems"),
    sample("crafting-systems_id", "/v1/crafting-systems/1"),
    sample("sets", "/v1/sets"),
    sample("sets_id", "/v1/sets/54"),
    sample("filigrees", "/v1/filigrees"),
    sample("sentient-gems", "/v1/sentient-gems"),
    sample("feats", "/v1/feats?q=cleave&limit=2"),
    sample("feats_id", "/v1/feats/12"),
    sample("stances", "/v1/stances"),
    sample("guild-buffs", "/v1/guild-buffs"),
    sample("optional-buffs", "/v1/optional-buffs"),
    sample("races", "/v1/races"),
    sample("races_id", "/v1/races/1"),
    sample("classes", "/v1/classes"),
    sample("classes_id", "/v1/classes/28"),
    sample("enhancement-trees", "/v1/enhancement-trees"),
    sample("enhancement-trees_id", "/v1/enhancement-trees/11"),
    sample("spells", "/v1/spells?q=fireball&limit=2"),
    sample("spells_id", "/v1/spells/359"),
    sample("clickies", "/v1/clickies"),
];

#[derive(Debug)]
pub struct WrittenExample {
    pub file_name: String,
    pub size_bytes: usize,
}

pub fn write_response_examples(
    db_path: &Path,
    requests: &[ExampleRequest],
    examples_dir: &Path,
) -> Result<Vec<WrittenExample>> {
    reject_unmatched_requests(requests)?;
    let state = AppState::open(db_path)?;
    let runtime = tokio::runtime::Builder::new_current_thread().enable_all().build()?;
    let mut example_texts = Vec::with_capacity(requests.len());
    for request in requests {
        let example_path = examples_dir.join(example_file_name(request.example_name));
        let sampled_response = runtime.block_on(sampled_response(&state, request.path))?;
        if let Some(committed_text) = committed_example_to_keep(request.example_name, &sampled_response, &example_path)
        {
            example_texts.push((example_file_name(request.example_name), committed_text));
            continue;
        }
        let mut example = sampled_response
            .into_json()
            .with_context(|| format!("example {}: GET {}", request.example_name, request.path))?;
        if request.example_name == VERSION_EXAMPLE_NAME {
            keep_committed_api_commit(&mut example, &example_path)?;
        }
        let example_text = serde_json::to_string_pretty(&trimmed_example(example))? + "\n";
        example_texts.push((example_file_name(request.example_name), example_text));
    }
    example_texts
        .into_iter()
        .map(|(file_name, example_text)| {
            let example_path = examples_dir.join(&file_name);
            std::fs::write(&example_path, &example_text)
                .with_context(|| format!("writing {}", example_path.display()))?;
            Ok(WrittenExample { file_name, size_bytes: example_text.len() })
        })
        .collect()
}

fn example_file_name(example_name: &str) -> String {
    format!("{example_name}.json")
}

fn reject_unmatched_requests(requests: &[ExampleRequest]) -> Result<()> {
    let requested_file_names: BTreeSet<String> =
        requests.iter().map(|request| example_file_name(request.example_name)).collect();
    ensure!(requested_file_names.len() == requests.len(), "two sample requests write the same example file");
    let documented_file_names: BTreeSet<String> =
        ddo_api::v1_response_examples().iter().map(|example| example.file_name.to_string()).collect();
    let unsampled: Vec<_> = documented_file_names.difference(&requested_file_names).collect();
    let undocumented: Vec<_> = requested_file_names.difference(&documented_file_names).collect();
    if !unsampled.is_empty() || !undocumented.is_empty() {
        bail!(
            "sample requests do not match RESPONSE_EXAMPLES: no sample for {unsampled:?}; no documented route for {undocumented:?}"
        );
    }
    Ok(())
}

struct SampledResponse {
    status: StatusCode,
    body_bytes: Vec<u8>,
}

impl SampledResponse {
    fn into_json(self) -> Result<Value> {
        ensure!(
            self.status == StatusCode::OK,
            "returned {}: {}",
            self.status,
            String::from_utf8_lossy(&self.body_bytes)
        );
        serde_json::from_slice(&self.body_bytes).context("response is not JSON")
    }

    fn is_empty_or_missing(&self) -> bool {
        self.status == StatusCode::NOT_FOUND
            || (self.status == StatusCode::OK
                && serde_json::from_slice::<Value>(&self.body_bytes).is_ok_and(|body| body == Value::Array(Vec::new())))
    }
}

async fn sampled_response(state: &AppState, path: &str) -> Result<SampledResponse> {
    let response = app(state.clone()).oneshot(Request::get(path).body(Body::empty())?).await?;
    let status = response.status();
    let body_bytes = response.into_body().collect().await?.to_bytes().to_vec();
    Ok(SampledResponse { status, body_bytes })
}

fn committed_example_to_keep(
    example_name: &str,
    sampled_response: &SampledResponse,
    example_path: &Path,
) -> Option<String> {
    if !EXAMPLES_KEPT_UNTIL_A_WIKI_FILE_RECORDS_A_ROW.contains(&example_name) || !sampled_response.is_empty_or_missing()
    {
        return None;
    }
    std::fs::read_to_string(example_path).ok()
}

fn keep_committed_api_commit(version_example: &mut Value, committed_version_path: &Path) -> Result<()> {
    let Ok(committed_text) = std::fs::read_to_string(committed_version_path) else {
        return Ok(());
    };
    let committed_version: Value = serde_json::from_str(&committed_text)
        .with_context(|| format!("parsing {}", committed_version_path.display()))?;
    if let (Some(committed_api_commit), Some(served_fields)) =
        (committed_version.get("api_commit"), version_example.as_object_mut())
    {
        served_fields.insert("api_commit".to_string(), committed_api_commit.clone());
    }
    Ok(())
}

fn trimmed_example(example: Value) -> Value {
    match example {
        Value::Array(elements) => {
            Value::Array(elements.into_iter().take(TRIMMED_ARRAY_LENGTH).map(trimmed_example).collect())
        }
        Value::Object(fields) => {
            let mut sorted_fields: Vec<(String, Value)> = fields.into_iter().collect();
            sorted_fields.sort_by(|(left_key, _), (right_key, _)| left_key.cmp(right_key));
            Value::Object(
                sorted_fields.into_iter().map(|(key, field)| (key, trimmed_example(field))).collect::<Map<_, _>>(),
            )
        }
        scalar => scalar,
    }
}
