//! #117: two runs of the same test at once (two lanes) must not share an output folder.
mod common;

const CHILD: &str = "EXO_OUT_DIRS_CHILD";

/// Child mode: write the folder this process would use to the file named in the env var.
#[test]
fn child_reports_its_out_dir() {
    let Ok(report) = std::env::var(CHILD) else { return };
    std::fs::write(report, common::out_dir("full").to_string_lossy().as_bytes()).unwrap();
}

#[test]
fn two_processes_get_different_out_dirs_under_the_target_tmpdir() {
    let tmp = std::path::PathBuf::from(env!("CARGO_TARGET_TMPDIR"));
    let run = |tag: &str| {
        let report = tmp.join(format!("out-dirs-{}-{tag}.txt", std::process::id()));
        let st = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["child_reports_its_out_dir", "--exact", "--nocapture"])
            .env(CHILD, &report)
            .status()
            .unwrap();
        assert!(st.success());
        std::fs::read_to_string(report).unwrap()
    };
    let (a, b) = (run("a"), run("b"));
    assert_ne!(a, b, "two runs share {a}");
    for d in [&a, &b] {
        assert!(std::path::Path::new(d).starts_with(&tmp), "{d} is outside {}", tmp.display());
    }
}
