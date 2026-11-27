//! The engine reads no clock, draws nothing at random and performs no I/O
//! (SPEC ORCH-1): its source names none of the paths through which a Rust
//! program does, and the crate has no dependency that would bring one in
//! (19.1, which the structural test of `xtask` holds).

/// Paths through which code reads the time, draws at random or does I/O, and
/// the maps whose order of iteration is drawn at random.
const FORBIDDEN: &[&str] = &[
    "std::time",
    "SystemTime",
    "Instant",
    "std::fs",
    "std::io",
    "std::net",
    "std::process",
    "std::env",
    "std::thread",
    "rand::",
    "HashMap",
    "HashSet",
];

#[test]
fn the_engine_names_no_clock_randomness_or_io() {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/src/engine.rs");
    let source = std::fs::read_to_string(path).expect("the engine's source");
    for name in FORBIDDEN {
        assert!(!source.contains(name), "the engine names {name}");
    }
}
