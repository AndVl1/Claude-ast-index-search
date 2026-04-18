//! Microbenchmarks for `parsers::parse_file_symbols`.
//!
//! Measures end-to-end parse + ref-extraction time on representative
//! real-world snippets across three languages:
//!   * Rust       — pulled from `src/commands/files.rs` (a real source file
//!                  in this very repo, so the bench input grows with the
//!                  codebase rather than going stale).
//!   * Kotlin     — fixture under `benches/fixtures/sample.kt`.
//!   * TypeScript — fixture under `benches/fixtures/sample.ts`.
//!
//! Run with:
//!     cargo bench --bench parser
//!     cargo bench --bench parser -- --quick

use criterion::{black_box, criterion_group, criterion_main, Criterion, Throughput};

use ast_index::parsers::{parse_file_symbols, FileType};

const RUST_SNIPPET: &str = include_str!("../src/commands/files.rs");
const KOTLIN_SNIPPET: &str = include_str!("fixtures/sample.kt");
const TYPESCRIPT_SNIPPET: &str = include_str!("fixtures/sample.ts");

fn bench_parse(c: &mut Criterion) {
    let mut group = c.benchmark_group("parse_file_symbols");

    for (lang, src, ftype) in [
        ("rust", RUST_SNIPPET, FileType::Rust),
        ("kotlin", KOTLIN_SNIPPET, FileType::Kotlin),
        ("typescript", TYPESCRIPT_SNIPPET, FileType::TypeScript),
    ] {
        group.throughput(Throughput::Bytes(src.len() as u64));
        group.bench_function(lang, |b| {
            b.iter(|| {
                let res = parse_file_symbols(black_box(src), ftype).expect("parse ok");
                // Defeat dead-code elimination on the returned vectors.
                black_box(res);
            });
        });
    }

    group.finish();
}

criterion_group!(benches, bench_parse);
criterion_main!(benches);
