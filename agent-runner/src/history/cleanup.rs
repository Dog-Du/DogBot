use super::store::HistoryStore;

pub fn purge_expired_history(store: &HistoryStore) -> rusqlite::Result<()> {
    store.delete_expired_messages()?;
    store.delete_orphaned_assets()?;
    Ok(())
}
