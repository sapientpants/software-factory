# 20 - The stack of 19.2 resolved as one set

Recorded 2026-11-27, for issue #123: whether the crates of the table of SPEC 19.2 resolve together, as the M0 row of SPEC 22 asks, before any of them joins a manifest of the workspace.

Method: the session wrote the throwaway package of section 2 in its scratch directory, outside the repository, and ran `cargo generate-lockfile` in it with cargo 1.100.0, the toolchain of `rust-toolchain.toml`. The package is no member of the workspace, and the repository's `Cargo.lock` was not touched. The versions of section 3 are read from the lockfile that the command wrote. Nothing was built, and the command downloaded the index of crates.io and nothing else. The one date of a release in section 4 was read from the API of crates.io (`/api/v1/crates/process-wrap/versions`) the same day. A statement marked **(inference)** is a reading of cargo's behaviour and was not observed here.

## 1. What the package names

- **Every crate of the table that a manifest of the workspace may name.** That is every library crate of a row, `gix` included, which the row allows read-only at most.
- **At the exact version where the row has one** (`=x.y.z`): `hyper-util`, `seccompiler`, `icu_casemap`, `icu_normalizer`, `icu_properties` and `pulldown-cmark`.
- **Within the series where the row gives one**: `tokio` 1, `process-wrap` 10, `octocrab` 0.54, `axum` 0.8, `hyper` 1, `rusqlite` 0.43, `memchr` 2, `clap` 4 and `opentelemetry` 0.33. Cargo reads `"0.54"` as `>=0.54.0, <0.55.0` **(inference)**.
- **At any version where the row gives none** (`"*"`), so that cargo picks the newest that resolves with the rest.
- **With the features the rows name**: `rusqlite` with `bundled`, `pulldown-cmark` without its default features, `clap` with `derive`, and `hyper` with `http1` and not `http2`, which other crates of the set may still turn on. The others get their defaults and the features that a use in the table plainly needs: `tokio` `full`, `serde` `derive`, `hyper-util` `tokio`, `process-wrap` `tokio1`, and `hyper` `server` and `client`, the two sides of the egress proxy.
- **`seccompiler` for Linux targets alone**, as its row says. `cargo generate-lockfile` resolves every target, so the lockfile holds it either way.
- **`rust-version = "1.100"`, edition 2024**, as the workspace has (19.1). Edition 2024 implies resolver 3, which picks the newest version whose `rust-version` the toolchain meets **(inference)**; the command's own line "to highest Rust 1.100 compatible versions" in section 3 agrees.

Left out:

- **The programs of the table**: `cargo-nextest`, `cargo-mutants`, `cargo-llvm-cov`, `cargo-deny`, `cargo-shear`, `typos`, `cargo-dist` and `cargo-auditable`. No manifest of the workspace names them, so none joins its tree. The first six are pinned in `mise.toml` on 2026-11-27, `cargo-dist` and `cargo-auditable` in no file yet.
- **What is no crate**: the `git` CLI, mise, Lima and `actions/checkout`.
- **The failpoints of the testing row.** The row names no crate for them.

## 2. The package

`Cargo.toml`, with an empty `src/lib.rs` beside it:

```toml
[package]
name = "stack-set"
version = "0.0.0"
edition = "2024"
rust-version = "1.100"
publish = false

[workspace]

[dependencies]
tokio = { version = "1", features = ["full"] }
tokio-util = "*"
process-wrap = { version = "10", features = ["tokio1"] }
gix = "*"
octocrab = "0.54"
reqwest = "*"
axum = "0.8"
hyper = { version = "1", features = ["http1", "server", "client"] }
hyper-util = { version = "=0.1.21", features = ["tokio"] }
rusqlite = { version = "0.43", features = ["bundled"] }
serde = { version = "*", features = ["derive"] }
toml = "*"
schemars = "*"
icu_casemap = "=2.4.0"
icu_normalizer = "=2.4.0"
icu_properties = "=2.4.0"
memchr = "2"
pulldown-cmark = { version = "=0.13.6", default-features = false }
clap = { version = "4", features = ["derive"] }
thiserror = "*"
anyhow = "*"
tracing = "*"
tracing-subscriber = "*"
opentelemetry = "0.33"
insta = "*"
wiremock = "*"
proptest = "*"

[target.'cfg(target_os = "linux")'.dependencies]
seccompiler = "=0.5.0"
```

## 3. What it resolved

`cargo generate-lockfile` exited with 0 on 2026-11-27:

```text
    Updating crates.io index
     Locking 439 packages to highest Rust 1.100 compatible versions
      Adding generic-array v0.14.7 (available: v0.14.9)
      Adding matchit v0.8.4 (available: v0.8.6)
      Adding process-wrap v10.0.2 (available: v11.0.0)
```

The lockfile holds 440 packages, the package itself among them. Each crate that the package names resolved to one version:

| Crate | Row | Resolved |
|---|---|---|
| `tokio` | 1.x | 1.54.1 |
| `tokio-util` | none | 0.7.20 |
| `process-wrap` | 10 | 10.0.2 |
| `gix` | none | 0.91.0 |
| `octocrab` | 0.54.x | 0.54.9 |
| `reqwest` | none | 0.13.5 |
| `axum` | 0.8 | 0.8.9 |
| `hyper` | 1.x | 1.13.0 |
| `hyper-util` | =0.1.21 | 0.1.21 |
| `seccompiler` | =0.5.0 | 0.5.0 |
| `rusqlite` | 0.43 | 0.43.0 |
| `serde` | none | 1.0.229 |
| `toml` | none | 1.2.3+spec-1.1.0 |
| `schemars` | none | 1.2.3 |
| `icu_casemap` | =2.4.0 | 2.4.0 |
| `icu_normalizer` | =2.4.0 | 2.4.0 |
| `icu_properties` | =2.4.0 | 2.4.0 |
| `memchr` | 2 | 2.8.3 |
| `pulldown-cmark` | =0.13.6 | 0.13.6 |
| `clap` | 4 | 4.6.10 |
| `thiserror` | none | 2.0.21 |
| `anyhow` | none | 1.0.105 |
| `tracing` | none | 0.1.44 |
| `tracing-subscriber` | none | 0.3.23 |
| `opentelemetry` | 0.33 | 0.33.1 |
| `insta` | none | 1.49.0 |
| `wiremock` | none | 0.6.5 |
| `proptest` | none | 1.12.0 |

So no row has to change for the set to resolve.

## 4. What the lockfile says beside that

- **`hyper` 1.13.0, `axum` 0.8.9 and `reqwest` 0.13.5** are the versions that the egress-proxy row names as resolved together on 2026-11-26. `memchr` 2.8.3 is the version that the literal-search row names.
- **`icu_properties` resolved to 2.4.0** with `octocrab` and `reqwest` in the set too. The outbound-text row had observed that version with `icu_casemap` and `icu_normalizer` alone, and the path-patterns row says that `octocrab` and `reqwest` bring in `icu_normalizer`.
- **`process-wrap` 11.0.0 exists.** crates.io gives it the date 2026-11-24, after the date of the table, so the row's series 10 was the current one on 2026-11-18. A move to 11 is a change of the row, and none is made here.
- **Fifteen crates appear at two or three versions**, each a dependency of a dependency: `base64`, `bitflags`, `core-foundation`, `cpufeatures`, `defmt`, `getrandom`, `hashbrown`, `lock_api`, `parking_lot`, `parking_lot_core`, `rand_core`, `redox_syscall`, `syn` (2 and 3), `untrusted` and `windows-sys`. No crate that the package names is among them. `deny.toml` sets no policy on several versions of a crate on 2026-11-27, and this record decides none.
- **The licences were not read.** `cargo deny` reads them when a crate joins a manifest (DEV-2).
