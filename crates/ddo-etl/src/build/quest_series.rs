use super::drop_text::{insert_source_link, DroppedLoot, LootSource, SourceLink};
use super::BuildReport;
use crate::wiki::{QuestSeriesReward, WikiOverrides, WikiQuestSeries};
use anyhow::{Context, Result};
use ddo_model::enums::Provenance;
use rusqlite::{params, OptionalExtension, Transaction};

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
                "INSERT INTO quest_chains (name, pack_id, provenance, wiki_url) VALUES (?1, ?2, ?3, ?4)"
            }
            Self::Sagas => "INSERT INTO sagas (name, pack_id, provenance, wiki_url) VALUES (?1, ?2, ?3, ?4)",
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
    wiki_overrides: &WikiOverrides,
    report: &mut BuildReport,
) -> Result<()> {
    for quest_chain in &wiki_overrides.quest_chains {
        report.quest_chain_quest_link_count +=
            insert_quest_series(transaction, QuestSeriesTable::QuestChains, quest_chain)?;
        report.wiki_quest_chain_count += 1;
    }
    for saga in &wiki_overrides.sagas {
        report.saga_quest_link_count += insert_quest_series(transaction, QuestSeriesTable::Sagas, saga)?;
        report.wiki_saga_count += 1;
    }
    Ok(())
}

pub(super) fn write_wiki_quest_series_rewards(
    transaction: &Transaction,
    wiki_overrides: &WikiOverrides,
    report: &mut BuildReport,
) -> Result<()> {
    for quest_chain in &wiki_overrides.quest_chains {
        report.quest_chain_reward_count +=
            insert_quest_series_rewards(transaction, QuestSeriesTable::QuestChains, quest_chain)?;
    }
    for saga in &wiki_overrides.sagas {
        report.saga_reward_count += insert_quest_series_rewards(transaction, QuestSeriesTable::Sagas, saga)?;
    }
    Ok(())
}

fn citation<Reward>(table: QuestSeriesTable, series: &WikiQuestSeries<Reward>) -> String {
    format!("wiki {} {} {:?} ({})", series.file_name, table.kind_name(), series.name, series.page)
}

fn insert_quest_series<Reward>(
    transaction: &Transaction,
    table: QuestSeriesTable,
    series: &WikiQuestSeries<Reward>,
) -> Result<usize> {
    let citation = citation(table, series);
    let pack_id = match &series.pack {
        Some(pack_name) => Some(id_named(transaction, "SELECT id FROM adventure_packs WHERE name = ?1", pack_name)?
            .with_context(|| {
                format!("{citation}: pack {pack_name:?} is not an adventure pack in Maetrim's Quests.xml or Challenges.xml; use his spelling")
            })?),
        None => None,
    };
    transaction
        .execute(table.insert_series_sql(), params![series.name, pack_id, Provenance::Wiki.as_str(), series.page])?;
    let series_id = transaction.last_insert_rowid();
    for (sort_order, quest_name) in series.quests.iter().enumerate() {
        let quest_id = id_named(transaction, "SELECT id FROM quests WHERE name = ?1", quest_name)?.with_context(|| {
            format!("{citation}: quest {quest_name:?} is not a quest in Maetrim's Quests.xml or Challenges.xml or the wiki quests; use his spelling")
        })?;
        transaction.execute(table.insert_quest_link_sql(), params![series_id, quest_id, sort_order as i64])?;
    }
    Ok(series.quests.len())
}

fn insert_quest_series_rewards<Reward: QuestSeriesReward>(
    transaction: &Transaction,
    table: QuestSeriesTable,
    series: &WikiQuestSeries<Reward>,
) -> Result<usize> {
    let citation = citation(table, series);
    let series_id: i64 = transaction.query_row(table.series_id_sql(), params![series.name], |r| r.get(0))?;
    for reward in &series.rewards {
        let item_name = reward.name();
        let item_id = id_named(transaction, "SELECT id FROM items WHERE name = ?1", item_name)?.with_context(|| {
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
                cost: None,
                drop_text_segment: None,
                source_match: None,
            },
        )?;
    }
    Ok(series.rewards.len())
}

fn id_named(transaction: &Transaction, sql: &str, name: &str) -> Result<Option<i64>> {
    Ok(transaction.query_row(sql, params![name], |r| r.get(0)).optional()?)
}
