//! Loader test for text keys (#131): every key used by the shipped loop content exists in
//! the English table, and every pool a player sees often has at least 3 lines.
use std::path::Path;

use gameplay_core::text::TextTable;
use gameplay_core::{Content, File};
use jobs_core::JobContent;
use customers_core::CustomerContent;

fn files_from_dir(dir: &Path) -> Vec<File> {
    let mut out = Vec::new();
    if !dir.exists() {
        return out;
    }
    for d in std::fs::read_dir(dir).unwrap() {
        let d = d.unwrap().path();
        if d.is_dir() {
            for f in std::fs::read_dir(&d).unwrap() {
                let f = f.unwrap().path();
                if f.is_file() {
                    out.push(File::new(f.strip_prefix(dir).unwrap().to_str().unwrap(), std::fs::read_to_string(&f).unwrap()));
                }
            }
        }
    }
    out
}

#[test]
fn texts_all_keys_exist() {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let workspace_root = manifest_dir.parent().unwrap().parent().unwrap();
    let content_root = workspace_root.join("content/gameplay");

    // Load all content from the shipped directory
    let all_files = files_from_dir(&content_root);

    // Find the text file
    let text_file = all_files.iter().find(|f| f.path == "text/en.json").expect("en.json found");
    let table = TextTable::from_json(&text_file.path, &text_file.text).expect("text table load");

    // Load kernel from shipped content (commodities, locations, tracks, unlocks)
    let kernel_files: Vec<_> = all_files.iter().filter(|f| {
        f.path.starts_with("commodity/") ||
        f.path.starts_with("location/") ||
        f.path.starts_with("progress_track/") ||
        f.path.starts_with("unlock/")
    }).cloned().collect();
    let kernel = Content::load(&kernel_files, &["small", "medium", "large"]).expect("kernel load");

    // Filter out text files for content loading
    let content_files: Vec<_> = all_files.iter().filter(|f| !f.path.starts_with("text/")).cloned().collect();

    // Check job content texts
    let job_content = JobContent::load(&content_files, &kernel).expect("job content load");
    let job_errors = job_content.check_texts(&table);

    // Check customer content texts
    let customer_content = CustomerContent::load(&content_files, &kernel).expect("customer content load");
    let customer_errors = customer_content.check_texts(&table);

    // Check glue keys
    let glue_errors = jobs_core::check_glue_keys(&table);

    let all_errors = [job_errors, customer_errors, glue_errors].concat();
    if !all_errors.is_empty() {
        panic!("Missing text keys:\n{}", all_errors.join("\n"));
    }
}

#[test]
fn texts_pools_have_minimum_lines() {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let workspace_root = manifest_dir.parent().unwrap().parent().unwrap();
    let content_root = workspace_root.join("content/gameplay");

    // Load all content from the shipped directory
    let all_files = files_from_dir(&content_root);

    // Find the text file
    let text_file = all_files.iter().find(|f| f.path == "text/en.json").expect("en.json found");
    let table = TextTable::from_json(&text_file.path, &text_file.text).expect("text table load");

    // Load kernel from shipped content
    let kernel_files: Vec<_> = all_files.iter().filter(|f| {
        f.path.starts_with("commodity/") ||
        f.path.starts_with("location/") ||
        f.path.starts_with("progress_track/") ||
        f.path.starts_with("unlock/")
    }).cloned().collect();
    let kernel = Content::load(&kernel_files, &["small", "medium", "large"]).expect("kernel load");

    let content_files: Vec<_> = all_files.iter().filter(|f| !f.path.starts_with("text/")).cloned().collect();
    let job_content = JobContent::load(&content_files, &kernel).expect("job content load");
    let customer_content = CustomerContent::load(&content_files, &kernel).expect("customer content load");

    // Check pool minimum sizes in job content
    let job_pool_errors = job_content.check_pool_sizes(&table);

    // Check pool minimum sizes in customer content
    let customer_pool_errors = customer_content.check_pool_sizes(&table);

    let all_errors = [job_pool_errors, customer_pool_errors].concat();
    if !all_errors.is_empty() {
        panic!("Pools with fewer than 3 lines:\n{}", all_errors.join("\n"));
    }
}
