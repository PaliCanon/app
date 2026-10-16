//! `xtask data`: copies the map's six data files from the map crate's
//! checkout into the dist, under `/map-data/`.

/// The six files, by name (the front's `data::FILES` is tested equal).
pub const FILES: [&str; 6] = [
    "graph.json",
    "graph-sim.json",
    "graph-sim.bin",
    "translations.json",
    "blurbs-en.json",
    "blurbs-fr.json",
];
