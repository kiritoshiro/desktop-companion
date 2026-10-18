//! The progression and content names the Adventure modules use, gathered in
//! one place: `dc_state::progression` (port of state/progression.py) and
//! `dc_content::enemy_kinds`, plus the small ordered-dict helpers this crate's
//! own data uses.

pub use dc_content::OrderedMap;
pub use dc_state::progression::{
    ABILITY_TREE, ARMOR_CATALOG, ARMOR_SETS, ARMOR_TIERS, AbilityNode, ArmorItem, ArmorSet, ITEM_STATS, MAX_ITEM_LEVEL,
    MAX_LEVEL, ProgressionState, ability_by_id, armor_by_id, armor_set, equipped_items, item_at_level, set_pieces_worn,
};

/// A string-keyed map that keeps insertion order, like a Python dict.
pub type Dict<V> = Vec<(String, V)>;

/// `d.get(key)` on a [`Dict`].
pub fn dict_get<'a, V>(d: &'a Dict<V>, key: &str) -> Option<&'a V> {
    d.iter().find(|(k, _)| k == key).map(|(_, v)| v)
}

/// `d[key] = value` on a [`Dict`]: replaces in place, else appends.
pub fn dict_set<V>(d: &mut Dict<V>, key: &str, value: V) {
    if let Some(slot) = d.iter_mut().find(|(k, _)| k == key) {
        slot.1 = value;
    } else {
        d.push((key.to_string(), value));
    }
}

/// `d.pop(key, None)` on a [`Dict`].
pub fn dict_pop<V>(d: &mut Dict<V>, key: &str) -> Option<V> {
    let index = d.iter().position(|(k, _)| k == key)?;
    Some(d.remove(index).1)
}

/// `kind in ENEMY_KINDS`.
pub fn is_enemy_kind(id: &str) -> bool {
    dc_content::enemy_kinds::enemy_kind(id).is_some()
}

/// A [`Dict`] as an [`OrderedMap`].
pub fn to_ordered<V: Clone>(d: &Dict<V>) -> OrderedMap<V> {
    let mut m = OrderedMap::new();
    for (k, v) in d {
        m.insert(k.clone(), v.clone());
    }
    m
}

/// `ProgressionState(level=level)`.
pub fn state_at_level(level: i64) -> ProgressionState {
    ProgressionState { level, ..ProgressionState::default() }
}

/// `{slot: item for slot, item in equipped.items() if keep(item)}`.
pub fn retain_equipped(state: &mut ProgressionState, keep: impl Fn(&str) -> bool) {
    let mut kept = OrderedMap::new();
    for (slot, item) in state.equipped.iter() {
        if keep(item) {
            kept.insert(slot, item.clone());
        }
    }
    state.equipped = kept;
}
