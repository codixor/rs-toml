//! Core benchmarks. Run with all features so the serde and derive groups are
//! included: `cargo bench --all-features --bench toml_core`.

use criterion::{BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};
use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::hint::black_box;
use std::sync::OnceLock;

/// Small config, about 1 KB.
const SMALL: &str = r#"# Service configuration
title = "demo service"
enabled = true
version = "1.4.2"
max_connections = 512
timeout = 2.5
started = 2026-11-16T08:30:00Z

[server]
host = "127.0.0.1"
port = 8080
workers = [1, 2, 4, 8]
tags = ["api", "internal", "eu-west"]

[server.tls]
cert = '/etc/demo/cert.pem'
key = '/etc/demo/key.pem'
protocols = ["TLSv1.2", "TLSv1.3"]

[database]
url = "postgres://localhost:5432/demo"
pool = { min = 2, max = 32, idle_timeout = 30.0 }
replicas = [
  { host = "db1.internal", weight = 3 },
  { host = "db2.internal", weight = 1 },
]

[logging]
level = "info"
format = "json"
targets = { stdout = true, file = "/var/log/demo.log" }

[[routes]]
path = "/health"
method = "GET"
auth = false

[[routes]]
path = "/api/v1/items"
method = "POST"
auth = true
limit = 100

[features]
experimental = ["fast-path", "zero-copy"]
retry = { attempts = 5, backoff = 1.5e-1 }
"#;

/// Deterministic Cargo-manifest-like document with `packages` sections:
/// tables, arrays of tables, escaped and multi-line strings, floats,
/// integers in several radixes and datetimes.
fn generate(packages: usize) -> String {
    let mut s = String::with_capacity(packages * 1000);
    s.push_str("[workspace]\nresolver = \"3\"\nmembers = [\n");
    for i in 0..packages.min(64) {
        let _ = writeln!(s, "  \"crates/pkg-{i}\",");
    }
    s.push_str("]\n\n");
    for i in 0..packages {
        let _ = write!(
            s,
            "[package-{i}]\n\
             name = \"pkg-{i}\"\n\
             version = \"0.{}.{}\"\n\
             edition = \"2024\"\n\
             description = \"Package {i} with \\\"quoted\\\" text,\\ttabs and unicode \\u00e9\\u4e2d escapes\\n\"\n\
             license = 'MIT OR Apache-2.0'\n\
             weight = {}.{:03}\n\
             ratio = -{}.5e-3\n\
             count = {}\n\
             hex = 0x{:x}\n\
             big = 1_000_{:03}\n\
             released = 2026-{:02}-{:02}T{:02}:15:30.250+01:00\n\
             local_date = 2025-{:02}-{:02}\n\
             keywords = [\"toml\", \"parser\", \"k{i}\"]\n\
             matrix = [[1, 2, 3], [4.5, 5.5], [\"a\", \"b\"]]\n\
             meta = {{ owner = \"team-{}\", tier = {}, active = {} }}\n\
             \n\
             [package-{i}.dependencies]\n\
             serde = {{ version = \"1.0\", features = [\"derive\"] }}\n\
             dep-{} = \"{}.0\"\n\
             \n\
             [[package-{i}.bin]]\n\
             name = \"tool-{i}\"\n\
             path = \"src/bin/tool_{i}.rs\"\n\
             \n\
             [[package-{i}.bin]]\n\
             name = \"helper-{i}\"\n\
             path = '''src/bin/helper.rs'''\n\
             \n",
            i % 17,
            i % 31,
            i % 1000,
            i % 997,
            i % 13,
            i * 7,
            i * 31 + 1,
            i % 1000,
            i % 12 + 1,
            i % 28 + 1,
            i % 24,
            i % 12 + 1,
            i % 28 + 1,
            i % 9,
            i % 5,
            i % 2 == 0,
            i % 11,
            i % 7,
        );
    }
    s
}

const MEDIUM_PACKAGES: usize = 110;

fn medium() -> &'static str {
    static DOC: OnceLock<String> = OnceLock::new();
    DOC.get_or_init(|| generate(MEDIUM_PACKAGES))
}

fn large() -> &'static str {
    static DOC: OnceLock<String> = OnceLock::new();
    // Under 10 MB; generated once per bench process.
    DOC.get_or_init(|| generate(11_000))
}

fn parse_group(c: &mut Criterion) {
    let mut g = c.benchmark_group("parse");
    for (name, src) in [("small", SMALL), ("medium", medium()), ("large", large())] {
        g.throughput(Throughput::Bytes(src.len() as u64));
        g.bench_with_input(BenchmarkId::from_parameter(name), src, |b, src| {
            b.iter(|| {
                let arena = rs_toml::Arena::new();
                let doc = rs_toml::parse(black_box(src), &arena).unwrap();
                black_box(doc.table().len());
            });
        });
    }
    g.finish();
}

fn emit_group(c: &mut Criterion) {
    let mut g = c.benchmark_group("emit");
    for (name, src) in [("small", SMALL), ("medium", medium())] {
        let arena = rs_toml::Arena::new();
        let doc = rs_toml::parse(src, &arena).unwrap();
        g.throughput(Throughput::Bytes(src.len() as u64));
        g.bench_function(BenchmarkId::new("format", name), |b| {
            b.iter(|| black_box(rs_toml::to_string(black_box(doc.table())).unwrap()));
        });
        g.bench_function(BenchmarkId::new("preserved", name), |b| {
            let fmt = rs_toml::Formatting::preserved_from(&doc);
            b.iter(|| black_box(fmt.format(black_box(doc.table())).unwrap()));
        });
    }
    let mut values = BTreeMap::new();
    for i in 0..64 {
        values.insert(
            format!("key_{i:02}"),
            format!("value \"{i}\"\twith escapes"),
        );
    }
    g.bench_function("to_string_map", |b| {
        b.iter(|| black_box(rs_toml::to_string(black_box(&values)).unwrap()));
    });
    g.finish();
}

fn typed_round_trip(c: &mut Criterion) {
    let source = "name = 'demo'\nmode = 'debug'\n";
    c.bench_function("typed_round_trip", |b| {
        b.iter(|| {
            let parsed: BTreeMap<String, String> = rs_toml::from_str(black_box(source)).unwrap();
            black_box(rs_toml::to_string(&parsed).unwrap());
        });
    });
}

#[cfg(feature = "serde")]
mod serde_bench {
    use super::{BTreeMap, BenchmarkId, Criterion, Throughput, black_box, large, medium};
    use serde::Deserialize;

    #[derive(Deserialize)]
    #[allow(dead_code, reason = "fields exist to be deserialized")]
    pub struct Package {
        name: String,
        version: String,
        description: String,
        weight: f64,
        count: i64,
        keywords: Vec<String>,
        meta: Meta,
        bin: Vec<Bin>,
    }
    #[derive(Deserialize)]
    #[allow(dead_code, reason = "fields exist to be deserialized")]
    pub struct Workspace {
        resolver: String,
        members: Vec<String>,
    }

    #[derive(Deserialize)]
    #[allow(dead_code, reason = "fields exist to be deserialized")]
    pub struct Doc {
        workspace: Workspace,
        #[serde(flatten)]
        packages: BTreeMap<String, Package>,
    }

    #[derive(Deserialize)]
    #[allow(dead_code, reason = "fields exist to be deserialized")]
    pub struct Meta {
        owner: String,
        tier: u32,
        active: bool,
    }
    #[derive(Deserialize)]
    #[allow(dead_code, reason = "fields exist to be deserialized")]
    pub struct Bin {
        name: String,
        path: String,
    }

    pub fn group(c: &mut Criterion) {
        let mut g = c.benchmark_group("serde");
        for (name, src) in [("medium", medium()), ("large", large())] {
            g.throughput(Throughput::Bytes(src.len() as u64));
            g.bench_with_input(BenchmarkId::new("from_str", name), src, |b, src| {
                b.iter(|| {
                    let v: serde::de::IgnoredAny =
                        rs_toml::serde::from_str(black_box(src)).unwrap();
                    black_box(v);
                });
            });
        }
        let src = medium();
        g.throughput(Throughput::Bytes(src.len() as u64));
        g.bench_function("structs/medium", |b| {
            b.iter(|| {
                let v: Doc = rs_toml::serde::from_str(black_box(src)).unwrap();
                black_box(v);
            });
        });
        g.finish();
    }
}

#[cfg(feature = "derive")]
mod derive_bench {
    use super::{BTreeMap, Criterion, Throughput, black_box, medium};

    #[derive(rs_toml::Toml)]
    #[toml(Toml)]
    #[allow(dead_code, reason = "fields exist to be deserialized")]
    pub struct Meta {
        owner: String,
        tier: u32,
        active: bool,
    }

    #[derive(rs_toml::Toml)]
    #[toml(Toml)]
    #[allow(dead_code, reason = "fields exist to be deserialized")]
    pub struct Bin {
        name: String,
        path: String,
    }

    #[derive(rs_toml::Toml)]
    #[toml(Toml)]
    #[allow(dead_code, reason = "fields exist to be deserialized")]
    pub struct Package {
        name: String,
        version: String,
        description: String,
        weight: f64,
        count: i64,
        keywords: Vec<String>,
        meta: Meta,
        bin: Vec<Bin>,
    }

    #[derive(rs_toml::Toml)]
    #[toml(FromToml)]
    pub struct Workspace {
        resolver: String,
        members: Vec<String>,
    }

    pub fn group(c: &mut Criterion) {
        let mut g = c.benchmark_group("derive");
        let src = medium();
        g.throughput(Throughput::Bytes(src.len() as u64));
        g.bench_function("from_toml/medium", |b| {
            b.iter(|| {
                let v: BTreeMap<String, Package> = {
                    // Skip the [workspace] table: it is a different shape.
                    let arena = rs_toml::Arena::new();
                    let mut doc = rs_toml::parse(black_box(src), &arena).unwrap();
                    let mut helper = doc.table_helper();
                    let ws: Option<Workspace> = helper.optional("workspace");
                    black_box(ws.map(|w| w.resolver.len() + w.members.len()));
                    let mut out = BTreeMap::new();
                    for i in 0..super::MEDIUM_PACKAGES {
                        let key = format!("package-{i}");
                        if let Some(p) = helper.optional::<Package>(&key) {
                            out.insert(key, p);
                        }
                    }
                    out
                };
                black_box(v.len());
            });
        });
        let s = "name = 'pkg'\nversion = '1.0'\ndescription = \"d\\n\"\nweight = 1.5\ncount = 3\nkeywords = ['a', 'b']\nmeta = { owner = 'o', tier = 2, active = true }\n[[bin]]\nname = 'x'\npath = 'y'\n";
        g.throughput(Throughput::Bytes(s.len() as u64));
        g.bench_function("round_trip/small", |b| {
            b.iter(|| {
                let p: Package = rs_toml::from_str(black_box(s)).unwrap();
                black_box(rs_toml::to_string(&p).unwrap());
            });
        });
        g.finish();
    }
}

#[cfg(feature = "serde")]
fn serde_group(c: &mut Criterion) {
    serde_bench::group(c);
}
#[cfg(not(feature = "serde"))]
fn serde_group(_: &mut Criterion) {}

#[cfg(feature = "derive")]
fn derive_group(c: &mut Criterion) {
    derive_bench::group(c);
}
#[cfg(not(feature = "derive"))]
fn derive_group(_: &mut Criterion) {}

criterion_group!(
    toml_core,
    parse_group,
    emit_group,
    typed_round_trip,
    serde_group,
    derive_group
);
criterion_main!(toml_core);
