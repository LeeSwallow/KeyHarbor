//! How fast masking is (05 §5.4). Measurements, not checks: they are ignored by default and
//! print a table. Run with a release build so the numbers mean something:
//!
//! ```sh
//! cargo test --release -p keyharbor-guard --test masking_throughput -- --ignored --nocapture
//! ```

use keyharbor_domain::vault::{FieldName, ItemName, PlaceholderRef};
use keyharbor_guard::pep::mask::{Masker, StreamMasker};
use std::time::{Duration, Instant};
use zeroize::Zeroizing;

/// Vault sizes measured, in values (each value adds its variants as patterns).
const VALUE_COUNTS: &[usize] = &[10, 100, 1_000];
/// Text sizes measured, in bytes.
const TEXT_SIZES: &[usize] = &[64 * 1024, 1024 * 1024];
/// Chunk size for the stream measurement: about one pipe read.
const CHUNK: usize = 4096;
/// Each measurement repeats until it has run at least this long, and the best of
/// [`ROUNDS`] is reported (the others are noise from the machine).
const MIN_RUN: Duration = Duration::from_millis(200);
const ROUNDS: usize = 5;

fn masker(values: usize) -> Masker {
    Masker::build((0..values).map(|n| {
        (
            PlaceholderRef {
                item: ItemName::parse(&format!("item{n}")).unwrap(),
                field: FieldName::parse("value").unwrap(),
            },
            Zeroizing::new(value(n)),
        )
    }))
    .unwrap()
}

/// A synthetic value whose bytes vary like a real secret's, with characters that have
/// escaped and encoded variants.
fn value(n: usize) -> String {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325 ^ n as u64;
    let mut out = String::new();
    for _ in 0..20 {
        h = h.wrapping_mul(0x100_0000_01b3).rotate_left(17) ^ 0x9e37_79b9;
        out.push(char::from(
            b"abcdefghijkmnpqrstuvwxyzABCDEFGHJKLMNPQRSTUVWXYZ23456789"[(h % 56) as usize],
        ));
    }
    out.insert_str(10, "+/ &");
    out
}

/// Page-like text with no value in it, and one value every 4 KiB, so both the scan and the
/// replacement are paid for.
fn text(size: usize) -> String {
    let filler = "<div class=\"row\">Order 1234 shipped to Seoul, 2 items</div>\n";
    let mut out = String::with_capacity(size + 64);
    let mut n = 0;
    while out.len() < size {
        if out.len() / CHUNK > n {
            out.push_str(&value(n % 10));
            n += 1;
        }
        out.push_str(filler);
    }
    out
}

/// MiB processed per second by `f`, best of [`ROUNDS`] runs of at least [`MIN_RUN`].
fn throughput(bytes: usize, mut f: impl FnMut()) -> f64 {
    (0..ROUNDS)
        .map(|_| one_round(bytes, &mut f))
        .fold(0.0, f64::max)
}

fn one_round(bytes: usize, f: &mut impl FnMut()) -> f64 {
    let start = Instant::now();
    let mut runs = 0u32;
    while start.elapsed() < MIN_RUN || runs == 0 {
        f();
        runs += 1;
    }
    let mib = (bytes as f64 * f64::from(runs)) / (1024.0 * 1024.0);
    mib / start.elapsed().as_secs_f64()
}

#[test]
#[ignore = "measurement; run with --ignored --nocapture"]
fn masking_throughput_by_vault_and_text_size() {
    println!("\n| values | build ms | text KiB | redact MiB/s | stream MiB/s |");
    println!("|---:|---:|---:|---:|---:|");
    for &values in VALUE_COUNTS {
        let start = Instant::now();
        let masker = masker(values);
        let build = start.elapsed().as_secs_f64() * 1000.0;
        for &size in TEXT_SIZES {
            let text = text(size);
            let redact = throughput(text.len(), || {
                std::hint::black_box(masker.redact(&text));
            });
            let stream = throughput(text.len(), || {
                let mut s = StreamMasker::new(&masker);
                for chunk in text.as_bytes().chunks(CHUNK) {
                    std::hint::black_box(s.push(chunk));
                }
                std::hint::black_box(s.flush());
            });
            println!(
                "| {values} | {build:.1} | {} | {redact:.0} | {stream:.0} |",
                size / 1024
            );
        }
    }
}

#[test]
#[ignore = "measurement; run with --ignored --nocapture"]
fn measured_text_is_masked() {
    // The measurement is meaningless if nothing is replaced.
    let masker = masker(10);
    let masked = masker.redact(&text(64 * 1024));
    assert!(masked.contains("{{secret:item0.value}}"));
    assert!(!masked.contains(&value(0)));
}
