//! cargo run --release -p net_core --bin matrix -- <path.bin> <out.json>
//! Runs the 96 cases of spike 4 on a recorded path and writes the raw results.
use net_core::replay::{run_matrix, to_json, Trajectory};

fn main() {
    let mut a = std::env::args().skip(1);
    let path = a.next().expect("path of the recorded trajectory");
    let out = a.next().unwrap_or_else(|| "results/net-matrix.json".into());
    let tr = Trajectory::parse(&std::fs::read(&path).expect("read path")).expect("parse path");
    let results = run_matrix(&tr);
    std::fs::write(&out, to_json(&tr, &results)).expect("write results");
    println!("{} cases on {:.1} s of path -> {out}", results.len(), tr.seconds());
}
