//! ego lite (ADR-0018): finding it ([`probe`]). The CDP relay that drives it comes with #121.

pub mod probe;

/// A fresh private directory for a unit test (the crate takes no test-only dependencies).
#[cfg(test)]
pub(crate) fn test_dir(tag: &str) -> std::path::PathBuf {
    use std::sync::atomic::{AtomicU32, Ordering};
    static NEXT: AtomicU32 = AtomicU32::new(0);
    let dir = std::env::temp_dir().join(format!(
        "kh-{tag}-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::SeqCst)
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}
