//! Raw SQL for other crates' tests (`test-support` feature): tampering with stored rows.

use crate::SqliteStore;

impl SqliteStore {
    /// Runs one statement and panics on error.
    pub fn execute_for_tests(&self, sql: &str) -> usize {
        self.conn.execute(sql, []).expect("test SQL")
    }
    /// The sealed field records of `item`, ordered by field name.
    pub fn sealed_fields_for_tests(&self, item: &str) -> Vec<Vec<u8>> {
        let mut q = self
            .conn
            .prepare("SELECT f.sealed FROM vault_fields f JOIN vault_items i ON i.id=f.item_id WHERE i.name=?1 ORDER BY f.field")
            .expect("test SQL");
        q.query_map([item], |r| r.get(0))
            .expect("test SQL")
            .map(Result::unwrap)
            .collect()
    }
}
