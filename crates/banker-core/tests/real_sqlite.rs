//! Recovery against databases written by real SQLite. See fixtures/README.md.

use banker_core::sqlite::recover;

const NOW: i64 = 1_760_000_000;

fn message_number(text: &str) -> Option<u32> {
    let rest = &text[text.find("message number ")? + "message number ".len()..];
    rest.split(' ').next()?.parse().ok()
}

#[test]
fn recovers_deleted_messages_with_times_and_never_live_ones() {
    let data = include_bytes!("fixtures/messages.db").to_vec();
    let r = recover(data, None, NOW).unwrap();
    let deleted = |n: u32| n == 16 || n == 41 || n == 98 || n >= 150;

    let mut whole = 0;
    for f in &r.fragments {
        if let Some(n) = message_number(&f.text) {
            assert!(deleted(n), "live row reported as deleted: {:?}", f.text);
            if f.text.ends_with("about lunch plans") && f.time.is_some() {
                whole += 1;
                // timestamp column = 1_730_000_000_000 + n * 60_000 ms
                assert_eq!(f.time, Some(1_730_000_000 + n as i64 * 60));
                assert!(!f.time_estimated);
                assert_eq!(f.table.as_deref(), Some("messages"));
            }
        }
    }
    assert!(whole >= 25, "only {whole} whole deleted messages recovered");
}

#[test]
fn recovers_a_deletion_that_only_exists_in_the_wal() {
    let db = include_bytes!("fixtures/contacts.db").to_vec();
    let wal = include_bytes!("fixtures/contacts.db-wal");
    let r = recover(db, Some(wal), NOW).unwrap();
    let texts: Vec<&str> = r.fragments.iter().map(|f| f.text.as_str()).collect();
    assert!(texts.iter().any(|t| t.contains("Bob Jones")), "{texts:?}");
    assert!(!texts
        .iter()
        .any(|t| t.contains("Alice") || t.contains("Carol")));
}
