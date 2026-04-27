use agent_runner::session_store::{SessionStore, session_schema_sql};

#[test]
fn session_schema_sql_creates_two_session_tables() {
    let sql = session_schema_sql();

    assert!(sql.contains("CREATE TABLE IF NOT EXISTS runner_sessions"));
    assert!(sql.contains("CREATE TABLE IF NOT EXISTS runner_session_aliases"));
    assert!(sql.contains("runner_sessions_platform_account_conversation_idx"));
    assert!(!sql.contains("CREATE TABLE IF NOT EXISTS sessions"));
    assert!(!sql.contains("CREATE TABLE IF NOT EXISTS session_aliases"));
}

#[test]
fn session_store_persists_existing_mapping() {
    let Some(scope) = PostgresSessionScope::new() else {
        return;
    };

    let external = scope.external("qq-user-1");
    let account = scope.account("qq:bot_uin:123");
    let first = scope
        .store
        .get_or_create_bound_session(&external, "qq", &account, "private:1")
        .unwrap();
    let second = scope
        .store
        .get_or_create_bound_session(&external, "qq", &account, "private:1")
        .unwrap();

    assert_eq!(first.external_session_id, external);
    assert_eq!(first.claude_session_id, second.claude_session_id);
    assert!(first.is_new);
    assert!(!second.is_new);
}

#[test]
fn session_store_uses_distinct_claude_ids_for_distinct_external_sessions() {
    let Some(scope) = PostgresSessionScope::new() else {
        return;
    };

    let account = scope.account("qq:bot_uin:123");
    let first = scope
        .store
        .get_or_create_bound_session(&scope.external("qq-user-1"), "qq", &account, "private:1")
        .unwrap();
    let second = scope
        .store
        .get_or_create_bound_session(&scope.external("qq-user-2"), "qq", &account, "private:2")
        .unwrap();

    assert_ne!(first.claude_session_id, second.claude_session_id);
}

#[test]
fn bound_session_api_uses_conversation_scoped_storage() {
    let Some(scope) = PostgresSessionScope::new() else {
        return;
    };

    let account = scope.account("qq:bot_uin:123");
    let first = scope
        .store
        .get_or_create_bound_session(
            &scope.external("qq-user-1"),
            "qq",
            &account,
            "qq:group:5566",
        )
        .unwrap();
    let second = scope
        .store
        .get_or_create_bound_session(
            &scope.external("qq-user-2"),
            "qq",
            &account,
            "qq:group:5566",
        )
        .unwrap();
    let third = scope
        .store
        .get_or_create_bound_session(
            &scope.external("qq-user-3"),
            "qq",
            &account,
            "qq:group:7788",
        )
        .unwrap();

    assert_eq!(first.claude_session_id, second.claude_session_id);
    assert_ne!(first.claude_session_id, third.claude_session_id);
}

#[test]
fn external_session_alias_does_not_override_conversation_scoping() {
    let Some(scope) = PostgresSessionScope::new() else {
        return;
    };

    let external = scope.external("qq-user-1");
    let account = scope.account("qq:bot_uin:123");
    let first = scope
        .store
        .get_or_create_bound_session(&external, "qq", &account, "qq:private:1")
        .unwrap();
    let err = scope
        .store
        .get_or_create_bound_session(&external, "qq", &account, "qq:private:2")
        .unwrap_err();

    assert!(matches!(
        err,
        agent_runner::session_store::SessionStoreError::SessionConflict { .. }
    ));
    let fetched = scope.store.get_session(&external).unwrap().unwrap();
    assert_eq!(fetched.claude_session_id, first.claude_session_id);
    assert_eq!(fetched.conversation_id, "qq:private:1");
}

#[test]
fn session_store_reset_session_rotates_claude_session_id() {
    let Some(scope) = PostgresSessionScope::new() else {
        return;
    };

    let external = scope.external("qq-user-1");
    let account = scope.account("qq:bot_uin:123");
    let first = scope
        .store
        .get_or_create_bound_session(&external, "qq", &account, "private:1")
        .unwrap();
    let reset = scope
        .store
        .reset_bound_session(&external, "qq", &account, "private:1")
        .unwrap();
    let fetched = scope.store.get_session(&external).unwrap().unwrap();

    assert_ne!(first.claude_session_id, reset.claude_session_id);
    assert_eq!(reset.claude_session_id, fetched.claude_session_id);
    assert!(reset.is_new);
}

#[test]
fn group_sessions_are_keyed_by_conversation_not_actor() {
    let Some(scope) = PostgresSessionScope::new() else {
        return;
    };

    let account = scope.account("qq:bot_uin:123");
    let first = scope
        .store
        .get_or_create_conversation_session("qq", &account, "qq:group:5566")
        .unwrap();

    let second = scope
        .store
        .get_or_create_conversation_session("qq", &account, "qq:group:5566")
        .unwrap();

    let third = scope
        .store
        .get_or_create_conversation_session("qq", &account, "qq:group:7788")
        .unwrap();

    assert_eq!(first.claude_session_id, second.claude_session_id);
    assert_ne!(first.claude_session_id, third.claude_session_id);
}

#[test]
fn conversation_and_bound_session_apis_share_the_same_underlying_session() {
    let Some(scope) = PostgresSessionScope::new() else {
        return;
    };

    let external = scope.external("qq-user-1");
    let account = scope.account("qq:bot_uin:123");
    let canonical = scope
        .store
        .get_or_create_conversation_session("qq", &account, "qq:group:5566")
        .unwrap();
    let bound = scope
        .store
        .get_or_create_bound_session(&external, "qq", &account, "qq:group:5566")
        .unwrap();

    assert_eq!(canonical.claude_session_id, bound.claude_session_id);
    assert_eq!(bound.platform_account, account);
    assert_eq!(
        scope
            .store
            .get_session(&external)
            .unwrap()
            .unwrap()
            .session_key,
        canonical.session_key
    );
}

struct PostgresSessionScope {
    store: SessionStore,
    suffix: String,
}

impl PostgresSessionScope {
    fn new() -> Option<Self> {
        let Some(url) = std::env::var("DOGBOT_TEST_DATABASE_URL").ok() else {
            eprintln!("DOGBOT_TEST_DATABASE_URL unset; skipping postgres integration test");
            return None;
        };
        let store = SessionStore::open_database_url(url).unwrap();
        store.initialize_schema().unwrap();
        Some(Self {
            store,
            suffix: uuid::Uuid::new_v4().to_string(),
        })
    }

    fn external(&self, value: &str) -> String {
        format!("{value}:{}", self.suffix)
    }

    fn account(&self, value: &str) -> String {
        format!("{value}:test:{}", self.suffix)
    }
}
