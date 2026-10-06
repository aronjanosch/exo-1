//! Variant C: plain Rust benchmark. `gen_bench_cli bench out.json | probe out.csv | compare a.csv b.csv`
use gen_core::*;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Instant;

fn stats(mut us: Vec<f64>) -> (f64, f64, f64) {
    us.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let mean = us.iter().sum::<f64>() / us.len() as f64;
    let p95 = us[((us.len() as f64) * 0.95).ceil() as usize - 1];
    (mean, p95, *us.last().unwrap())
}

fn throughput(g: &Gen, list: &[(usize, usize, usize)], threads: usize, passes: usize) -> f64 {
    let total = list.len() * passes;
    let next = AtomicUsize::new(0);
    let t0 = Instant::now();
    std::thread::scope(|s| {
        for _ in 0..threads {
            s.spawn(|| {
                let mut sink = 0usize;
                loop {
                    let i = next.fetch_add(1, Ordering::Relaxed);
                    if i >= total {
                        break;
                    }
                    let (f, x, y) = list[i % list.len()];
                    sink += g.build_chunk(f, x, y).canopy.len();
                }
                std::hint::black_box(sink);
            });
        }
    });
    total as f64 / t0.elapsed().as_secs_f64()
}

fn bench(out: &str) {
    let ncpu = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(1);
    let mut g = Gen::new(SEED);
    let mut j = String::from("{\n");
    let mut log = String::new();
    let mut say = |s: String, log: &mut String| {
        println!("{s}");
        log.push_str(&s);
        log.push('\n');
    };

    // bake: 3 reps each, report each
    let mut bake1 = vec![];
    let mut baken = vec![];
    for _ in 0..3 {
        let t = Instant::now();
        g.bake(1);
        bake1.push(t.elapsed().as_secs_f64() * 1000.0);
        let t = Instant::now();
        g.bake(ncpu);
        baken.push(t.elapsed().as_secs_f64() * 1000.0);
    }
    say(format!("bake 1 thread ms: {:?}", bake1), &mut log);
    say(format!("bake {ncpu} threads ms: {:?}", baken), &mut log);
    j += &format!("  \"threads_logical\": {ncpu},\n  \"bake_1thread_ms\": {:?},\n  \"bake_nthreads_ms\": {:?},\n", bake1, baken);

    let list = chunk_list();
    for &(f, x, y) in list.iter().cycle().take(20) {
        std::hint::black_box(g.build_chunk(f, x, y));
    }
    let mut us = vec![];
    let reps = 10;
    for _ in 0..reps {
        for &(f, x, y) in &list {
            let t = Instant::now();
            let c = g.build_chunk(f, x, y);
            us.push(t.elapsed().as_secs_f64() * 1e6);
            std::hint::black_box(c);
        }
    }
    let (mean, p95, max) = stats(us);
    say(format!("single thread per chunk ({} samples): mean {mean:.1} us, p95 {p95:.1} us, max {max:.1} us", reps * list.len()), &mut log);
    j += &format!("  \"chunk_1thread_us\": {{\"mean\": {mean:.2}, \"p95\": {p95:.2}, \"max\": {max:.2}, \"samples\": {}}},\n", reps * list.len());

    j += "  \"throughput_chunks_per_s\": {";
    let mut first = true;
    for n in [1usize, 4, 8, ncpu] {
        let cps = throughput(&g, &list, n, 25);
        say(format!("throughput {n} threads: {cps:.0} chunks/s"), &mut log);
        if !first {
            j += ", ";
        }
        first = false;
        j += &format!("\"{n}\": {cps:.1}");
    }
    j += "},\n";

    let (mut hsum, mut can, mut rocks, mut bio) = (0.0f64, 0usize, 0usize, [0i64; 4]);
    for &(f, x, y) in &list {
        let c = g.build_chunk(f, x, y);
        hsum += c.height_sum;
        can += c.canopy.len() / 12;
        rocks += c.rocks.len() / 12;
        for k in 0..4 {
            bio[k] += c.biomes[k] as i64;
        }
    }
    say(format!("checksum: height_sum {hsum:.3}, canopy {can}, rocks {rocks}, biomes {:?}", bio), &mut log);
    j += &format!("  \"checksum\": {{\"height_sum\": {hsum:.4}, \"canopy\": {can}, \"rocks\": {rocks}, \"biomes\": {:?}}}\n}}\n", bio);
    std::fs::write(out, j).unwrap();
}

fn probe_points() -> Vec<V3> {
    (0..1000i64)
        .map(|k| {
            v3(
                ((k * 7919 % 6001) - 3000) as f32,
                ((k * 104729 % 6001) - 3000) as f32,
                ((k * 1299709 % 6001) - 3000) as f32,
            )
        })
        .collect()
}

fn probe(out: &str) {
    let n = Noises::new(SEED);
    let mut s = String::new();
    s += "k";
    for c in NOISE_CFGS.iter() {
        s += &format!(",{}", c.name);
    }
    s += "\n";
    for (k, p) in probe_points().into_iter().enumerate() {
        s += &k.to_string();
        for v in n.all_at(p) {
            s += &format!(",{v:.12}");
        }
        s += "\n";
    }
    std::fs::write(out, s).unwrap();
}

fn parse(path: &str) -> (Vec<String>, Vec<Vec<f64>>) {
    let t = std::fs::read_to_string(path).unwrap();
    let mut l = t.lines();
    let head: Vec<String> = l.next().unwrap().split(',').map(String::from).collect();
    let rows = l
        .filter(|x| !x.is_empty())
        .map(|x| x.split(',').map(|v| v.parse().unwrap()).collect())
        .collect();
    (head, rows)
}

fn compare(a: &str, b: &str) {
    let (h, ra) = parse(a);
    let (_, rb) = parse(b);
    println!("rows {} / {}", ra.len(), rb.len());
    for c in 1..h.len() {
        let (mut mx, mut at, mut sum) = (0.0f64, 0usize, 0.0f64);
        for k in 0..ra.len().min(rb.len()) {
            let d = (ra[k][c] - rb[k][c]).abs();
            sum += d;
            if d > mx {
                mx = d;
                at = k;
            }
        }
        println!("{:8} max abs diff {:.3e} (k={at}), mean abs diff {:.3e}", h[c], mx, sum / ra.len() as f64);
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    match args.get(1).map(String::as_str) {
        Some("bench") => bench(args.get(2).map(String::as_str).unwrap_or("bench_c.json")),
        Some("probe") => probe(&args[2]),
        Some("compare") => compare(&args[2], &args[3]),
        _ => eprintln!("usage: gen_bench_cli bench <out.json> | probe <out.csv> | compare <a.csv> <b.csv>"),
    }
}
