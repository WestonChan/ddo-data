use crate::tree::SocketOption;
use anyhow::Result;
use std::collections::{BTreeMap, BTreeSet};

pub(crate) struct SocketOptionList<T = SocketOption> {
    pub(crate) label: String,
    pub(crate) options: Vec<T>,
    pub(crate) socket: (i64, i64),
    pub(crate) item_slug: String,
    pub(crate) item_name: String,
}

pub const CARRIERLESS_SOCKET_OPTION_SETS: &str = "WITH referrers(set_id,detail) AS (
    SELECT o.set_id,'recipe ' || system.name || ' / ' || r.option || ' / option ' || o.name
      FROM crafting_recipe_set_options link JOIN socket_options o ON o.id=link.option_id
      JOIN crafting_recipes r ON r.id=link.recipe_id JOIN crafting_systems system ON system.id=r.system_id
    UNION ALL
    SELECT s.id,c.kind || ' [' || c.qualifier || '].' || c.field
      FROM corrections c JOIN socket_option_sets s ON s.slug=c.name
      WHERE c.kind IN ('socket_option','socket_option_bonus')
)
SELECT s.name,s.id,COALESCE(referrers.detail,'unreferenced: ' || s.slug),referrers.set_id IS NOT NULL
    FROM socket_option_sets s
    LEFT JOIN referrers ON referrers.set_id=s.id
    WHERE NOT EXISTS (SELECT 1 FROM item_augment_slots socket WHERE socket.option_set_id=s.id)
    ORDER BY s.slug,referrers.detail";

pub(super) fn remove_unreferenced_carrierless_sets(
    transaction: &rusqlite::Transaction<'_>,
    effects: &mut super::effects::EffectCache<'_>,
) -> Result<()> {
    let unused_sets = transaction
        .prepare(CARRIERLESS_SOCKET_OPTION_SETS)?
        .query_map([], |row| Ok((row.get::<_, i64>(1)?, row.get::<_, bool>(3)?)))?
        .collect::<rusqlite::Result<Vec<_>>>()?
        .into_iter()
        .filter_map(|(id, referenced)| (!referenced).then_some(id))
        .collect::<Vec<_>>();
    for set_id in unused_sets {
        let linked_effects = transaction.prepare("SELECT DISTINCT e.effect_id FROM socket_option_effects e JOIN socket_options o ON o.id=e.option_id WHERE o.set_id=?1")?
            .query_map([set_id], |row| row.get::<_, i64>(0))?.collect::<rusqlite::Result<Vec<_>>>()?;
        transaction.execute("DELETE FROM requirements WHERE owner_kind='modifier' AND owner_id IN (SELECT m.id FROM modifiers m JOIN socket_options o ON m.source_kind='socket_option' AND m.source_id=o.id WHERE o.set_id=?1)", [set_id])?;
        transaction.execute("DELETE FROM modifiers WHERE source_kind='socket_option' AND source_id IN (SELECT id FROM socket_options WHERE set_id=?1)", [set_id])?;
        transaction.execute("DELETE FROM socket_option_sets WHERE id=?1", [set_id])?;
        for effect_id in linked_effects {
            effects.delete_unowned(effect_id)?;
        }
    }
    Ok(())
}

#[derive(Debug, Default, Clone, PartialEq)]
pub struct ExistingSocketOptionSets {
    identities: BTreeMap<String, SocketOptionSetIdentity>,
    sockets: BTreeMap<(String, i64, String), String>,
}

#[derive(Debug, Clone, PartialEq)]
struct SocketOptionSetIdentity {
    label: String,
    name: String,
    variant: Option<String>,
}

impl ExistingSocketOptionSets {
    pub(crate) fn from_database(db: &rusqlite::Connection) -> Result<Self> {
        let mut existing = Self::default();
        let mut query = db.prepare("SELECT s.slug,l.label,s.name,s.variant FROM socket_option_sets s JOIN augment_slot_types l ON l.id=s.slot_id")?;
        for row in query.query_map([], |row| {
            Ok((row.get(0)?, SocketOptionSetIdentity { label: row.get(1)?, name: row.get(2)?, variant: row.get(3)? }))
        })? {
            let (slug, identity) = row?;
            existing.identities.insert(slug, identity);
        }
        let mut query = db.prepare("SELECT i.slug,s.sort_order,l.label,o.slug FROM item_augment_slots s JOIN items i ON i.id=s.item_id JOIN augment_slot_types l ON l.id=s.slot_id JOIN socket_option_sets o ON o.id=s.option_set_id")?;
        for row in query.query_map([], |row| Ok(((row.get(0)?, row.get(1)?, row.get(2)?), row.get(3)?)))? {
            let (socket, slug) = row?;
            existing.sockets.insert(socket, slug);
        }
        Ok(existing)
    }
}

#[derive(Clone)]
pub(crate) struct GroupedSocketOptionSet<T = SocketOption> {
    pub(crate) label: String,
    pub(crate) name: String,
    pub(crate) variant: Option<String>,
    pub(crate) slug: String,
    pub(crate) options: Vec<T>,
    pub(crate) sockets: Vec<(i64, i64)>,
}

pub(crate) fn group_socket_options_into_sets<T: Clone + PartialEq>(
    sockets: Vec<SocketOptionList<T>>,
) -> Result<Vec<GroupedSocketOptionSet<T>>> {
    let item_names = sockets.iter().map(|socket| (socket.socket, socket.item_name.clone())).collect::<BTreeMap<_, _>>();
    let item_slugs = sockets.iter().map(|socket| (socket.socket, socket.item_slug.clone())).collect::<BTreeMap<_, _>>();
    let mut groups = Vec::<GroupedSocketOptionSet<T>>::new();
    for socket in sockets.into_iter().filter(|socket| !socket.options.is_empty()) {
        if let Some(group) =
            groups.iter_mut().find(|group| group.label == socket.label && group.options == socket.options)
        {
            group.sockets.push(socket.socket);
        } else {
            groups.push(GroupedSocketOptionSet {
                label: socket.label,
                name: socket.item_name,
                variant: None,
                slug: String::new(),
                options: socket.options,
                sockets: vec![socket.socket],
            });
        }
    }
    groups.sort_by_key(|group| group.sockets.iter().map(|socket| (&item_slugs[socket], socket.1)).min().unwrap());
    let mut reserved = std::collections::BTreeSet::new();
    for group in &mut groups {
        let carrier = group.sockets.iter().min_by_key(|socket| (&item_slugs[socket], socket.1)).unwrap();
        group.name.clone_from(&item_names[carrier]);
        group.variant = reserve_set_variant(&mut reserved, &group.label, &group.name);
        group.slug = crate::tree::socket_option_set_slug(&group.label, &group.name, group.variant.as_deref());
        group.sockets.sort_unstable();
    }
    Ok(groups)
}

fn reserve_set_variant(
    reserved: &mut BTreeSet<(String, String, Option<String>)>,
    label: &str,
    name: &str,
) -> Option<String> {
    for ordinal in 1.. {
        let variant = (ordinal > 1).then(|| ordinal.to_string());
        if reserved.insert((label.to_string(), name.to_string(), variant.clone())) {
            return variant;
        }
    }
    unreachable!()
}

pub(crate) fn matched_socket_option_sets<T: Clone + PartialEq>(
    sockets: Vec<SocketOptionList<T>>,
    existing: &ExistingSocketOptionSets,
) -> Result<Vec<GroupedSocketOptionSet<T>>> {
    let item_names = sockets.iter().map(|socket| (socket.socket, socket.item_name.clone())).collect::<BTreeMap<_, _>>();
    let item_slugs = sockets.iter().map(|socket| (socket.socket, socket.item_slug.clone())).collect::<BTreeMap<_, _>>();
    let first_socket = |group: &GroupedSocketOptionSet<T>| {
        group.sockets.iter().map(|socket| (item_slugs[socket].clone(), socket.1)).min().unwrap()
    };
    let mut candidates = Vec::new();
    for group in group_socket_options_into_sets(sockets)? {
        let mut partitions = BTreeMap::<Option<String>, Vec<(i64, i64)>>::new();
        for socket in &group.sockets {
            let previous = existing.sockets.get(&(item_slugs[socket].clone(), socket.1, group.label.clone())).cloned();
            partitions.entry(previous).or_default().push(*socket);
        }
        if partitions.keys().filter(|key| key.is_some()).count() == 1 {
            let unmatched = partitions.remove(&None).unwrap_or_default();
            partitions.values_mut().next().unwrap().extend(unmatched);
        }
        for (previous, sockets) in partitions {
            let mut candidate = group.clone();
            candidate.sockets = sockets;
            candidate.sockets.sort_unstable();
            candidates.push((previous, candidate));
        }
    }
    let mut retained = BTreeMap::<String, usize>::new();
    for (index, (previous, candidate)) in candidates.iter().enumerate() {
        let Some(previous) = previous else { continue };
        match retained.get(previous) {
            Some(&winner)
                if (std::cmp::Reverse(candidates[winner].1.sockets.len()), first_socket(&candidates[winner].1))
                    <= (std::cmp::Reverse(candidate.sockets.len()), first_socket(candidate)) => {}
            _ => {
                retained.insert(previous.clone(), index);
            }
        }
    }
    for (index, (previous, _)) in candidates.iter_mut().enumerate() {
        if previous.as_ref().is_some_and(|slug| retained[slug] != index) {
            *previous = None;
        }
    }
    candidates.sort_by_key(|(_, candidate)| first_socket(candidate));
    let mut reserved = existing
        .identities
        .values()
        .map(|identity| (identity.label.clone(), identity.name.clone(), identity.variant.clone()))
        .collect::<std::collections::BTreeSet<_>>();
    for (previous, candidate) in &mut candidates {
        if let Some(previous) = previous {
            let identity = &existing.identities[previous];
            candidate.name.clone_from(&identity.name);
            candidate.variant.clone_from(&identity.variant);
            candidate.slug.clone_from(previous);
        } else {
            let carrier = candidate.sockets.iter().min_by_key(|socket| (&item_slugs[socket], socket.1)).unwrap();
            candidate.name.clone_from(&item_names[carrier]);
            candidate.variant = reserve_set_variant(&mut reserved, &candidate.label, &candidate.name);
            candidate.slug =
                crate::tree::socket_option_set_slug(&candidate.label, &candidate.name, candidate.variant.as_deref());
        }
    }
    Ok(candidates.into_iter().map(|(_, candidate)| candidate).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn socket(item_id: i64) -> SocketOptionList {
        #[derive(serde::Deserialize)]
        struct ItemSockets {
            socket: Vec<SocketOptions>,
        }
        #[derive(serde::Deserialize)]
        struct SocketOptions {
            slot: String,
            #[serde(default)]
            option: Vec<SocketOption>,
        }
        let item: ItemSockets =
            toml::from_str(include_str!("../../tests/fixtures/data/packs/unsourced/gear/celestial-emerald-ring.toml"))
                .unwrap();
        let socket = item.socket.into_iter().find(|socket| !socket.option.is_empty()).unwrap();
        SocketOptionList {
            label: socket.slot,
            options: socket.option,
            socket: (item_id, 0),
            item_slug: format!("item-{item_id}"),
            item_name: format!("Item {item_id}"),
        }
    }

    #[test]
    fn identical_option_lists_on_one_label_become_one_set() {
        let sets = group_socket_options_into_sets(vec![socket(1), socket(2)]).unwrap();
        assert_eq!(sets.len(), 1);
        assert_eq!(sets[0].sockets, [(1, 0), (2, 0)]);
        assert!(sets[0].variant.is_none());
    }

    #[test]
    fn the_same_options_on_two_labels_are_two_sets() {
        let mut second = socket(2);
        second.label = "crafting: almost there".into();
        let sets = group_socket_options_into_sets(vec![socket(1), second]).unwrap();
        assert_eq!(sets.len(), 2);
        assert_ne!(sets[0].slug, sets[1].slug);
        assert!(sets.iter().all(|set| set.variant.is_none()));
    }

    #[test]
    fn new_sets_take_their_lowest_carriers_name() {
        let mut second = socket(2);
        second.options[0].min_level = Some(99);
        let sets = group_socket_options_into_sets(vec![socket(1), second]).unwrap();
        assert_eq!(sets.len(), 2);
        assert_eq!(sets[0].name, "Item 1");
        assert_eq!(sets[1].name, "Item 2");
        let first = sets.iter().find(|set| set.sockets.contains(&(1, 0))).unwrap();
        let second = sets.iter().find(|set| set.sockets.contains(&(2, 0))).unwrap();
        assert_eq!(first.variant, None);
        assert_eq!(second.variant, None);
        assert_ne!(sets[0].slug, sets[1].slug);
    }

    #[test]
    fn grouping_is_order_insensitive_across_items_and_order_sensitive_within_a_socket() {
        let first = group_socket_options_into_sets(vec![socket(1), socket(2)]).unwrap();
        let reversed = group_socket_options_into_sets(vec![socket(2), socket(1)]).unwrap();
        assert_eq!(first[0].slug, reversed[0].slug);
        assert_eq!(first[0].sockets, reversed[0].sockets);
        let mut second = socket(2);
        assert!(second.options.len() > 1);
        second.options.reverse();
        let sets = group_socket_options_into_sets(vec![socket(1), second]).unwrap();
        assert_eq!(sets.len(), 2);
        assert_ne!(sets[0].slug, sets[1].slug);
    }

    #[test]
    fn a_gem_of_many_facets_sized_option_list_builds_with_a_short_slug() {
        #[derive(serde::Deserialize)]
        struct ExportedSet {
            name: String,
            option: Vec<SocketOption>,
        }
        let source: ExportedSet = toml::from_str(include_str!(
            "../../../../data/packs/shared/sockets/crafting-set-bonus-1/epic-gem-of-many-facets.toml"
        ))
        .unwrap();
        let expected = source.option;
        assert_eq!(expected.len(), 27);
        let mut db = rusqlite::Connection::open_in_memory().unwrap();
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../data");
        crate::tree::build_from_data(&root, &mut db).unwrap();
        let id: i64 = db.query_row("SELECT s.option_set_id FROM item_augment_slots s JOIN items i ON i.id=s.item_id JOIN augment_slot_types l ON l.id=s.slot_id WHERE i.name='Gem of Many Facets' AND l.label='crafting: set bonus 1'", [], |row| row.get(0)).unwrap();
        let carrier: String = db.query_row("SELECT i.name FROM items i JOIN item_augment_slots s ON s.item_id=i.id WHERE s.option_set_id=?1 ORDER BY i.slug LIMIT 1", [id], |row| row.get(0)).unwrap();
        let path = crate::tree::DataFilePaths::from_database(&db).unwrap().path("socket_option_sets", id).unwrap();
        assert!(path.file_name().unwrap().len() < 100);
        let output = tempfile::tempdir().unwrap();
        crate::tree::export_database(&db, output.path()).unwrap();
        let restored: ExportedSet =
            toml::from_str(&std::fs::read_to_string(output.path().join(path)).unwrap()).unwrap();
        assert_eq!(restored.name, carrier);
        assert_eq!(restored.option, expected);
    }

    #[test]
    fn open_sockets_produce_no_option_sets() {
        let mut open = socket(1);
        open.options.clear();
        assert!(group_socket_options_into_sets(vec![open]).unwrap().is_empty());
        assert!(group_socket_options_into_sets(Vec::<SocketOptionList>::new()).unwrap().is_empty());
    }

    fn existing_sets(sockets: Vec<SocketOptionList>) -> ExistingSocketOptionSets {
        let item_slugs =
            sockets.iter().map(|socket| (socket.socket, socket.item_slug.clone())).collect::<BTreeMap<_, _>>();
        let mut existing = ExistingSocketOptionSets::default();
        for group in group_socket_options_into_sets(sockets).unwrap() {
            for socket in group.sockets {
                existing
                    .sockets
                    .insert((item_slugs[&socket].clone(), socket.1, group.label.clone()), group.slug.clone());
            }
            existing.identities.insert(
                group.slug,
                SocketOptionSetIdentity { label: group.label, name: group.name, variant: group.variant },
            );
        }
        existing
    }

    #[test]
    fn identical_upstream_options_keep_their_distinct_existing_sets() {
        let mut corrected = socket(2);
        corrected.options[0].min_level = Some(99);
        let existing = existing_sets(vec![socket(1), corrected]);
        let matched = matched_socket_option_sets(vec![socket(1), socket(2)], &existing).unwrap();
        assert_eq!(matched.len(), 2);
        assert_eq!(
            matched.iter().map(|set| set.slug.clone()).collect::<std::collections::BTreeSet<_>>(),
            existing.identities.keys().cloned().collect()
        );
    }

    #[test]
    fn the_largest_upstream_group_keeps_the_existing_identity() {
        let existing = existing_sets(vec![socket(1), socket(2), socket(3)]);
        let mut changed = socket(1);
        changed.options[0].min_level = Some(99);
        let matched = matched_socket_option_sets(vec![changed, socket(2), socket(3)], &existing).unwrap();
        let kept = matched.iter().find(|set| set.sockets.contains(&(2, 0))).unwrap();
        assert_eq!(kept.slug, *existing.identities.keys().next().unwrap());
        assert_eq!(kept.sockets, [(2, 0), (3, 0)]);
        let added = matched.iter().find(|set| set.sockets.contains(&(1, 0))).unwrap();
        assert_eq!(added.name, "Item 1");
        assert_eq!(added.variant.as_deref(), Some("2"));
    }

    #[test]
    fn equal_upstream_groups_keep_the_identity_by_item_slug() {
        let existing = existing_sets(vec![socket(1), socket(2)]);
        let mut changed = socket(2);
        changed.options[0].min_level = Some(99);
        let matched = matched_socket_option_sets(vec![changed, socket(1)], &existing).unwrap();
        assert_eq!(
            matched.iter().find(|set| set.sockets.contains(&(1, 0))).unwrap().slug,
            *existing.identities.keys().next().unwrap()
        );
    }
}
