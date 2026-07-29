use tokio_rusqlite::rusqlite::{Connection, TransactionBehavior};

use crate::{StoreConfig, StoreResult};

const MIGRATIONS: &[&str] = &[include_str!("../migrations/0001_initial.sql")];

pub(crate) fn configure_and_migrate(
    connection: &mut Connection,
    config: StoreConfig,
) -> StoreResult<()> {
    connection.busy_timeout(config.busy_timeout)?;
    connection.pragma_update(None, "foreign_keys", true)?;
    connection.pragma_update(None, "journal_mode", "WAL")?;
    connection.pragma_update(None, "synchronous", config.synchronous.pragma_value())?;
    apply_pending(connection)
}

fn apply_pending(connection: &mut Connection) -> StoreResult<()> {
    let current: u32 = connection.pragma_query_value(None, "user_version", |row| row.get(0))?;
    let latest = u32::try_from(MIGRATIONS.len()).expect("migration count fits in u32");
    if current > latest {
        return Err(jcode_tasker_types::TaskerError::Conflict {
            message: format!(
                "tasker database schema version {current} is newer than supported version {latest}"
            ),
        }
        .into());
    }

    for (index, sql) in MIGRATIONS.iter().enumerate().skip(current as usize) {
        let version = u32::try_from(index + 1).expect("migration index fits in u32");
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        transaction.execute_batch(sql)?;
        transaction.pragma_update(None, "user_version", version)?;
        transaction.commit()?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn migrations_are_idempotent() {
        let mut connection = Connection::open_in_memory().unwrap();
        configure_and_migrate(&mut connection, StoreConfig::default()).unwrap();
        configure_and_migrate(&mut connection, StoreConfig::default()).unwrap();
        let version: u32 = connection
            .pragma_query_value(None, "user_version", |row| row.get(0))
            .unwrap();
        assert_eq!(version, 1);
    }
}
