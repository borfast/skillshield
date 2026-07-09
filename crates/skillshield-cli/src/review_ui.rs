use skillshield_core::entry::Entry;

pub struct Group {
    pub key: String,
    pub entries_idx: Vec<usize>,
}

pub fn group_entries(entries: &[Entry]) -> Vec<Group> {
    let mut order: Vec<String> = Vec::new();
    let mut groups: std::collections::HashMap<String, Vec<usize>> = std::collections::HashMap::new();
    for (i, e) in entries.iter().enumerate() {
        let key = e.source_rule.clone();
        if !groups.contains_key(&key) {
            order.push(key.clone());
        }
        groups.entry(key).or_default().push(i);
    }
    order
        .into_iter()
        .map(|key| {
            let entries_idx = groups.remove(&key).unwrap();
            Group { key, entries_idx }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use skillshield_core::entry::{Entry, EntryKind};

    fn e(path: &str, rule: &str) -> Entry {
        Entry {
            path: path.into(), kind: EntryKind::File, digest: Some("sha256:1".into()),
            symlink_target: None, size: 1, mtime: 0, unhashed: false, source_rule: rule.into(),
        }
    }

    #[test]
    fn groups_by_source_rule() {
        let entries = vec![e("/a", "claude.skills"), e("/b", "claude.skills"), e("/c", "codex.home")];
        let groups = group_entries(&entries);
        assert_eq!(groups.len(), 2);
        assert_eq!(groups[0].key, "claude.skills");
        assert_eq!(groups[0].entries_idx, vec![0, 1]);
        assert_eq!(groups[1].key, "codex.home");
    }
}
