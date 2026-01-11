use rusqlite::{Connection, TransactionBehavior};
use tracing::info;

use crate::error::Result;
use crate::paths::database_path;

pub fn init_connection() -> Result<Connection> {
    let path = database_path()?;
    let mut conn = Connection::open(path)?;
    configure_connection(&mut conn)?;
    apply_migrations(&mut conn)?;
    Ok(conn)
}

pub fn init_in_memory() -> Result<Connection> {
    let mut conn = Connection::open_in_memory()?;
    configure_connection(&mut conn)?;
    apply_migrations(&mut conn)?;
    Ok(conn)
}

fn configure_connection(conn: &mut Connection) -> Result<()> {
    conn.pragma_update(None, "foreign_keys", &true)?;
    Ok(())
}

fn apply_migrations(conn: &mut Connection) -> Result<()> {
    let mut current: u32 = conn.pragma_query_value(None, "user_version", |row| row.get(0))?;
    if current < 1 {
        info!("applying schema v1");
        let tx = conn.transaction_with_behavior(TransactionBehavior::Exclusive)?;
        tx.execute_batch(
            r#"
            CREATE TABLE IF NOT EXISTS settings (
                key TEXT PRIMARY KEY,
                value TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS profiles (
                profile_id TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                type TEXT NOT NULL,
                host TEXT NOT NULL,
                port INTEGER NOT NULL,
                user TEXT NOT NULL,
                danger_level TEXT NOT NULL,
                "group" TEXT,
                tags_json TEXT NOT NULL,
                note TEXT,
                client_overrides_json TEXT,
                created_at INTEGER NOT NULL,
                updated_at INTEGER NOT NULL,
                last_used_at INTEGER
            );

            CREATE TABLE IF NOT EXISTS ssh_forwards (
                id INTEGER PRIMARY KEY,
                profile_id TEXT NOT NULL,
                name TEXT NOT NULL,
                kind TEXT NOT NULL,
                listen TEXT NOT NULL,
                dest TEXT NOT NULL,
                FOREIGN KEY(profile_id) REFERENCES profiles(profile_id) ON DELETE CASCADE
            );

            CREATE TABLE IF NOT EXISTS ssh_jump (
                profile_id TEXT PRIMARY KEY,
                jump_profile_id TEXT NOT NULL,
                FOREIGN KEY(profile_id) REFERENCES profiles(profile_id) ON DELETE CASCADE,
                FOREIGN KEY(jump_profile_id) REFERENCES profiles(profile_id) ON DELETE CASCADE
            );

            CREATE TABLE IF NOT EXISTS secrets (
                secret_id TEXT PRIMARY KEY,
                kind TEXT NOT NULL,
                label TEXT NOT NULL,
                ciphertext BLOB NOT NULL,
                nonce BLOB NOT NULL,
                created_at INTEGER NOT NULL,
                updated_at INTEGER NOT NULL
            );

            CREATE TABLE IF NOT EXISTS cmdsets (
                cmdset_id TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                vars_json TEXT
            );

            CREATE TABLE IF NOT EXISTS cmdsteps (
                id INTEGER PRIMARY KEY,
                cmdset_id TEXT NOT NULL,
                ord INTEGER NOT NULL,
                cmd TEXT NOT NULL,
                timeout_ms INTEGER,
                on_error TEXT NOT NULL,
                parser_spec TEXT NOT NULL,
                FOREIGN KEY(cmdset_id) REFERENCES cmdsets(cmdset_id) ON DELETE CASCADE
            );

            CREATE TABLE IF NOT EXISTS parsers (
                parser_id TEXT PRIMARY KEY,
                type TEXT NOT NULL,
                definition TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS configsets (
                config_id TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                hooks_cmdset_id TEXT,
                FOREIGN KEY(hooks_cmdset_id) REFERENCES cmdsets(cmdset_id) ON DELETE SET NULL
            );

            CREATE TABLE IF NOT EXISTS configfiles (
                id INTEGER PRIMARY KEY,
                config_id TEXT NOT NULL,
                src TEXT NOT NULL,
                dest TEXT NOT NULL,
                mode TEXT,
                "when" TEXT NOT NULL,
                FOREIGN KEY(config_id) REFERENCES configsets(config_id) ON DELETE CASCADE
            );

            CREATE TABLE IF NOT EXISTS op_logs (
                id INTEGER PRIMARY KEY,
                ts INTEGER NOT NULL,
                op TEXT NOT NULL,
                profile_id TEXT,
                client_used TEXT,
                ok INTEGER NOT NULL,
                exit_code INTEGER,
                duration_ms INTEGER,
                meta_json TEXT,
                FOREIGN KEY(profile_id) REFERENCES profiles(profile_id) ON DELETE SET NULL
            );

            PRAGMA user_version = 1;
            "#,
        )?;
        tx.commit()?;
        current = 1;
    }
    if current < 2 {
        info!("applying schema v2");
        let tx = conn.transaction_with_behavior(TransactionBehavior::Exclusive)?;
        tx.execute_batch(
            r#"
            ALTER TABLE profiles ADD COLUMN initial_send TEXT;
            PRAGMA user_version = 2;
            "#,
        )?;
        tx.commit()?;
        current = 2;
    }
    if current < 3 {
        info!("applying schema v3");
        let tx = conn.transaction_with_behavior(TransactionBehavior::Exclusive)?;
        tx.execute_batch(
            r#"
            CREATE TABLE IF NOT EXISTS settings_new (
                scope TEXT NOT NULL,
                key TEXT NOT NULL,
                value TEXT NOT NULL,
                PRIMARY KEY(scope, key)
            );

            INSERT INTO settings_new (scope, key, value)
            SELECT 'global', key, value FROM settings;

            DROP TABLE settings;
            ALTER TABLE settings_new RENAME TO settings;

            PRAGMA user_version = 3;
            "#,
        )?;
        tx.commit()?;
        current = 3;
    }
    Ok(())
}
