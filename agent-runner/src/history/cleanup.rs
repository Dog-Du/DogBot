use super::store::{HistoryStore, HistoryStoreError};

pub fn purge_expired_history(store: &HistoryStore) -> Result<(), HistoryStoreError> {
    store.delete_expired_messages()?;
    store.delete_orphaned_assets()?;
    Ok(())
}
