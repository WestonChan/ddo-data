use super::drop_text::{insert_source_link, DroppedLoot, LootSource, SourceLink};
use super::wiki::OverlayReferences;
use super::BuildReport;
use crate::wiki::{QuestSeriesReward, WikiOverrides, WikiQuestSeries};
use anyhow::{Context, Result};
use ddo_model::enums::Provenance;
use rusqlite::{params, Transaction};

#[derive(Clone, Copy)]
pub(super) enum QuestSeriesTable {
    QuestChains,
    Sagas,
}

impl QuestSeriesTable {
    fn kind_name(self) -> &'static str {
        match self {
            Self::QuestChains => "quest chain",
            Self::Sagas => "saga",
        }
    }

    fn insert_series_sql(self) -> &'static str {
        match self {
            Self::QuestChains => {
                "INSERT INTO quest_chains (name, pack_id, provenance, wiki_url, slug) VALUES (?1, ?2, ?3, ?4, ?5)"
            }
            Self::Sagas => "INSERT INTO sagas (name, pack_id, provenance, wiki_url, slug) VALUES (?1, ?2, ?3, ?4, ?5)",
        }
    }

    fn series_id_sql(self) -> &'static str {
        match self {
            Self::QuestChains => "SELECT id FROM quest_chains WHERE name = ?1",
            Self::Sagas => "SELECT id FROM sagas WHERE name = ?1",
        }
    }

    fn insert_quest_link_sql(self) -> &'static str {
        match self {
            Self::QuestChains => "INSERT INTO quest_chain_quests (chain_id, quest_id, sort_order) VALUES (?1, ?2, ?3)",
            Self::Sagas => "INSERT INTO saga_quests (saga_id, quest_id, sort_order) VALUES (?1, ?2, ?3)",
        }
    }

    pub(super) fn loot_source(self, series_id: i64) -> LootSource {
        match self {
            Self::QuestChains => LootSource::QuestChain(series_id),
            Self::Sagas => LootSource::Saga(series_id),
        }
    }
}

pub(super) fn write_wiki_quest_series(
    transaction: &Transaction,
    references: &OverlayReferences,
    wiki_overrides: &WikiOverrides,
    report: &mut BuildReport,
) -> Result<()> {
    for quest_chain in &wiki_overrides.quest_chains {
        report.quest_chain_quest_link_count +=
            insert_quest_series(transaction, references, QuestSeriesTable::QuestChains, quest_chain)?;
        report.wiki_quest_chain_count += 1;
    }
    for saga in &wiki_overrides.sagas {
        report.saga_quest_link_count += insert_quest_series(transaction, references, QuestSeriesTable::Sagas, saga)?;
        report.wiki_saga_count += 1;
    }
    Ok(())
}

pub(super) fn write_wiki_quest_series_rewards(
    transaction: &Transaction,
    references: &OverlayReferences,
    wiki_overrides: &WikiOverrides,
    report: &mut BuildReport,
) -> Result<()> {
    for quest_chain in &wiki_overrides.quest_chains {
        report.quest_chain_reward_count +=
            insert_quest_series_rewards(transaction, references, QuestSeriesTable::QuestChains, quest_chain)?;
    }
    for saga in &wiki_overrides.sagas {
        report.saga_reward_count +=
            insert_quest_series_rewards(transaction, references, QuestSeriesTable::Sagas, saga)?;
    }
    Ok(())
}

fn citation<Reward>(table: QuestSeriesTable, series: &WikiQuestSeries<Reward>) -> String {
    format!("wiki {} {} {:?} ({})", series.file_name, table.kind_name(), series.name, series.wiki_url)
}

fn insert_quest_series<Reward>(
    transaction: &Transaction,
    references: &OverlayReferences,
    table: QuestSeriesTable,
    series: &WikiQuestSeries<Reward>,
) -> Result<usize> {
    let citation = citation(table, series);
    let pack_id = match &series.pack {
        Some(pack_name) => Some(references.id(transaction, "adventure_packs", pack_name)?
            .with_context(|| {
                format!("{citation}: pack {pack_name:?} is not an adventure pack in Maetrim's Quests.xml or Challenges.xml; use his spelling")
            })?),
        None => None,
    };
    transaction.execute(
        table.insert_series_sql(),
        params![series.name, pack_id, Provenance::Wiki.as_str(), series.wiki_url, crate::tree::slug(&series.name)],
    )?;
    let series_id = transaction.last_insert_rowid();
    for (sort_order, quest_name) in series.quests.iter().enumerate() {
        let quest_id = references.id(transaction, "quests", quest_name)?.with_context(|| {
            format!("{citation}: quest {quest_name:?} is not a quest in Maetrim's Quests.xml or Challenges.xml or the wiki quests; use his spelling")
        })?;
        transaction.execute(table.insert_quest_link_sql(), params![series_id, quest_id, sort_order as i64])?;
    }
    Ok(series.quests.len())
}

fn insert_quest_series_rewards<Reward: QuestSeriesReward>(
    transaction: &Transaction,
    references: &OverlayReferences,
    table: QuestSeriesTable,
    series: &WikiQuestSeries<Reward>,
) -> Result<usize> {
    let citation = citation(table, series);
    let series_id: i64 = transaction.query_row(table.series_id_sql(), params![series.name], |r| r.get(0))?;
    for reward in &series.rewards {
        let item_name = reward.name();
        let item_id = references.id(transaction, "items", item_name)?.with_context(|| {
            format!(
                "{citation}: reward {item_name:?} is not in Maetrim's items or a wiki item; names must match exactly"
            )
        })?;
        insert_source_link(
            transaction,
            &SourceLink {
                source: table.loot_source(series_id),
                loot: DroppedLoot::Item(item_id),
                loot_type: None,
                is_rare: reward.is_rare(),
                chest: None,
                tier: reward.tier(),
                drop_text_segment: None,
                source_match: None,
            },
        )?;
    }
    Ok(series.rewards.len())
}
