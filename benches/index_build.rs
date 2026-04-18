//! Microbenchmark for `indexer::index_directory` over a small synthetic
//! project laid out in a `TempDir`.
//!
//! The fixture is generated deterministically once per process: ~12 source
//! files (Rust, Kotlin, TypeScript, Python, Go) totalling a few hundred
//! lines. Each iteration:
//!   * creates a fresh SQLite DB inside the same temp dir,
//!   * walks the synthetic project,
//!   * tears the DB down before the next iteration.
//!
//! This stays well under a few seconds at default sample size; we further
//! shrink the sample to avoid blowing the bench-time budget on macOS where
//! tree-sitter loads add fixed cost per call.
//!
//! Run with:
//!     cargo bench --bench index_build
//!     cargo bench --bench index_build -- --quick

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::time::Duration;

use criterion::{criterion_group, criterion_main, Criterion};
use tempfile::TempDir;

use ast_index::{db, indexer};

const RUST_FILE: &str = r#"
pub struct Counter { value: u64 }

impl Counter {
    pub fn new() -> Self { Self { value: 0 } }
    pub fn inc(&mut self) { self.value += 1; }
    pub fn get(&self) -> u64 { self.value }
}

pub trait Reporter {
    fn report(&self, label: &str, value: u64);
}

pub struct StdoutReporter;
impl Reporter for StdoutReporter {
    fn report(&self, label: &str, value: u64) {
        println!("{label}={value}");
    }
}

pub fn run(reporter: &dyn Reporter, ticks: u64) {
    let mut c = Counter::new();
    for _ in 0..ticks { c.inc(); }
    reporter.report("ticks", c.get());
}
"#;

const KOTLIN_FILE: &str = r#"
package bench.synth

interface Greeter { fun hello(name: String): String }

class FormalGreeter(private val title: String) : Greeter {
    override fun hello(name: String) = "$title $name"
}

data class Person(val first: String, val last: String) {
    val full: String get() = "$first $last"
}

object Greetings {
    fun greetAll(g: Greeter, people: List<Person>): List<String> =
        people.map { g.hello(it.full) }
}
"#;

const TS_FILE: &str = r#"
export interface Repo<T> {
    get(id: string): Promise<T | null>;
    put(id: string, value: T): Promise<void>;
}

export class MemRepo<T> implements Repo<T> {
    private data = new Map<string, T>();
    async get(id: string) { return this.data.get(id) ?? null; }
    async put(id: string, value: T) { this.data.set(id, value); }
}

export type Result<T, E = Error> =
    | { ok: true; value: T }
    | { ok: false; error: E };

export function ok<T>(value: T): Result<T> { return { ok: true, value }; }
export function err<E>(error: E): Result<never, E> { return { ok: false, error }; }
"#;

const PYTHON_FILE: &str = r#"
from dataclasses import dataclass
from typing import Iterable, Optional

@dataclass
class Item:
    sku: str
    qty: int
    price: float

class Cart:
    def __init__(self) -> None:
        self._items: list[Item] = []

    def add(self, item: Item) -> None:
        self._items.append(item)

    def total(self) -> float:
        return sum(i.qty * i.price for i in self._items)

    def find(self, sku: str) -> Optional[Item]:
        for it in self._items:
            if it.sku == sku:
                return it
        return None

def cart_from(items: Iterable[Item]) -> Cart:
    c = Cart()
    for it in items:
        c.add(it)
    return c
"#;

const GO_FILE: &str = r#"
package synth

type Stack struct { items []int }

func NewStack() *Stack { return &Stack{} }

func (s *Stack) Push(v int) { s.items = append(s.items, v) }

func (s *Stack) Pop() (int, bool) {
    n := len(s.items)
    if n == 0 { return 0, false }
    v := s.items[n-1]
    s.items = s.items[:n-1]
    return v, true
}

func (s *Stack) Len() int { return len(s.items) }
"#;

/// Build a synthetic project on disk once. Returns the project root path
/// (a stable subdirectory inside a `TempDir` held for the bench lifetime).
fn synth_project() -> &'static Path {
    static ROOT: OnceLock<(TempDir, PathBuf)> = OnceLock::new();
    let (_keep, root) = ROOT.get_or_init(|| {
        let tmp = TempDir::new().expect("tempdir");
        let root = tmp.path().join("project");
        fs::create_dir_all(root.join("src")).unwrap();
        fs::create_dir_all(root.join("kotlin")).unwrap();
        fs::create_dir_all(root.join("web")).unwrap();
        fs::create_dir_all(root.join("py")).unwrap();
        fs::create_dir_all(root.join("go")).unwrap();

        // ~12 files: 3 Rust, 3 Kotlin, 2 TS, 2 Python, 2 Go.
        for i in 0..3 {
            fs::write(root.join(format!("src/lib{i}.rs")), RUST_FILE).unwrap();
        }
        for i in 0..3 {
            fs::write(root.join(format!("kotlin/Module{i}.kt")), KOTLIN_FILE).unwrap();
        }
        for i in 0..2 {
            fs::write(root.join(format!("web/mod{i}.ts")), TS_FILE).unwrap();
        }
        for i in 0..2 {
            fs::write(root.join(format!("py/mod{i}.py")), PYTHON_FILE).unwrap();
        }
        for i in 0..2 {
            fs::write(root.join(format!("go/mod{i}.go")), GO_FILE).unwrap();
        }
        (tmp, root)
    });
    root.as_path()
}

fn bench_index_build(c: &mut Criterion) {
    let project = synth_project();

    let mut group = c.benchmark_group("index_build");
    // Each iter walks ~12 files + writes SQLite — keep the sample tight so
    // total wall time stays bounded even outside `--quick` mode.
    group.sample_size(10);
    group.measurement_time(Duration::from_secs(5));

    group.bench_function("synthetic_~12_files", |b| {
        b.iter_with_setup(
            || {
                // Fresh DB dir per iteration so we measure cold-build cost.
                let dir = TempDir::new().expect("db tempdir");
                let conn = db::open_db(dir.path()).unwrap();
                db::init_db(&conn).unwrap();
                drop(conn);
                dir
            },
            |dir| {
                let mut conn = db::open_db(dir.path()).unwrap();
                let res = indexer::index_directory(&mut conn, project, false, false).unwrap();
                criterion::black_box(res);
            },
        );
    });

    group.finish();
}

criterion_group!(benches, bench_index_build);
criterion_main!(benches);
