use agent_runner::context::object_store::ContextObjectStore;
use rusqlite::Connection;

#[derive(Debug, Clone, PartialEq, Eq)]
struct ColumnInfo {
    name: String,
    ty: String,
    notnull: bool,
    pk: i32,
}

fn table_info(conn: &Connection, table: &str) -> Vec<ColumnInfo> {
    // `PRAGMA table_info(...)` accepts either an identifier or a string literal.
    // Use a string literal to avoid identifier quoting edge cases in tests.
    let sql = format!("PRAGMA table_info('{}')", table.replace('\'', "''"));
    let mut stmt = conn.prepare(&sql).expect("prepare PRAGMA table_info");
    let rows = stmt
        .query_map([], |row| {
            Ok(ColumnInfo {
                name: row.get::<_, String>(1)?,
                ty: row.get::<_, String>(2)?,
                notnull: row.get::<_, i32>(3)? != 0,
                pk: row.get::<_, i32>(5)?,
            })
        })
        .expect("query PRAGMA table_info");

    rows.map(|r| r.expect("row decode")).collect()
}

fn assert_column(
    cols: &[ColumnInfo],
    idx: usize,
    name: &str,
    ty: &str,
    pk: i32,
    notnull: Option<bool>,
) {
    let col = cols.get(idx).unwrap_or_else(|| {
        panic!("missing column at index {idx}; got {cols:?}");
    });
    assert_eq!(col.name, name, "unexpected column name at index {idx}");
    assert_eq!(col.ty, ty, "unexpected column type for {name}");
    assert_eq!(col.pk, pk, "unexpected pk flag for {name}");
    if let Some(expected) = notnull {
        assert_eq!(col.notnull, expected, "unexpected notnull for {name}");
    }
}

#[test]
fn object_store_creates_required_tables() {
    let tmp = tempfile::tempdir().expect("create tempdir");
    let db_path = tmp.path().join("control_plane.sqlite3");

    let store = ContextObjectStore::open(&db_path).expect("open object store");
    let tables = store.table_names().expect("read table names");

    for required in [
        "context_objects",
        "memory_candidates",
        "conversation_authorizations",
    ] {
        assert!(
            tables.iter().any(|t| t == required),
            "missing required table {required}; got {tables:?}"
        );
    }
}

#[test]
fn object_store_schema_has_required_columns() {
    let tmp = tempfile::tempdir().expect("create tempdir");
    let db_path = tmp.path().join("control_plane.sqlite3");

    // Create schema via the store, then validate schema details using raw SQLite introspection.
    let _store = ContextObjectStore::open(&db_path).expect("open object store");
    let conn = Connection::open(&db_path).expect("open sqlite directly");

    let context_objects = table_info(&conn, "context_objects");
    assert_eq!(context_objects.len(), 4, "unexpected context_objects schema");
    assert_column(&context_objects, 0, "object_id", "TEXT", 1, None);
    assert_column(&context_objects, 1, "object_type", "TEXT", 0, Some(true));
    assert_column(&context_objects, 2, "object_json", "TEXT", 0, Some(true));
    assert_column(
        &context_objects,
        3,
        "created_at_epoch_secs",
        "INTEGER",
        0,
        Some(true),
    );

    let memory_candidates = table_info(&conn, "memory_candidates");
    assert_eq!(memory_candidates.len(), 5, "unexpected memory_candidates schema");
    assert_column(&memory_candidates, 0, "candidate_id", "TEXT", 1, None);
    assert_column(&memory_candidates, 1, "actor_id", "TEXT", 0, Some(true));
    assert_column(
        &memory_candidates,
        2,
        "conversation_id",
        "TEXT",
        0,
        Some(true),
    );
    assert_column(&memory_candidates, 3, "content", "TEXT", 0, Some(true));
    assert_column(
        &memory_candidates,
        4,
        "created_at_epoch_secs",
        "INTEGER",
        0,
        Some(true),
    );

    let conversation_authorizations = table_info(&conn, "conversation_authorizations");
    assert_eq!(
        conversation_authorizations.len(),
        4,
        "unexpected conversation_authorizations schema"
    );
    assert_column(
        &conversation_authorizations,
        0,
        "conversation_id",
        "TEXT",
        1,
        Some(true),
    );
    assert_column(
        &conversation_authorizations,
        1,
        "actor_id",
        "TEXT",
        2,
        Some(true),
    );
    assert_column(&conversation_authorizations, 2, "scope", "TEXT", 3, Some(true));
    assert_column(
        &conversation_authorizations,
        3,
        "created_at_epoch_secs",
        "INTEGER",
        0,
        Some(true),
    );
}
