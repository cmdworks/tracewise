//! The JSON that `--json` prints is a contract (docs/json.md): editors and the desktop app read it. These tests pin the keys each
//! command must keep. New keys may appear; a key listed here must not disappear or change its type.
mod common;
use common::Repo;
use serde_json::Value;

/// A repository with one file recorded twice: a baseline-era line and a line added later.
fn repo() -> Repo {
    let repo = Repo::new("");
    repo.edit_as("human:me", "f.txt", "a\nb\n");
    repo.later();
    repo.edit_as("human:me", "f.txt", "a\nb\nc\n");
    repo
}

fn has(v: &Value, what: &str, keys: &[(&str, &str)]) {
    let obj = v.as_object().unwrap_or_else(|| panic!("{what}: not an object: {v}"));
    for (key, ty) in keys {
        let got = obj.get(*key).unwrap_or_else(|| panic!("{what}: key `{key}` is missing in {v}"));
        let ok = match *ty {
            "string" => got.is_string(),
            "number" => got.is_number(),
            "bool" => got.is_boolean(),
            "array" => got.is_array(),
            "object" => got.is_object(),
            "string|null" => got.is_string() || got.is_null(),
            "any" => true,
            other => panic!("unknown type {other}"),
        };
        assert!(ok, "{what}: `{key}` should be {ty}, is {got}");
    }
}

#[test]
fn log_rows() {
    let repo = repo();
    let rows = repo.me(&["log", "--json"]).json();
    let row = &rows.as_array().unwrap()[0];
    has(
        row,
        "log row",
        &[
            ("id", "string"),
            ("path", "string"),
            ("op", "string"),
            ("actor", "object"),
            ("time", "object"),
            ("source", "string"),
            ("confidence", "string"),
            ("hunks", "array"),
            ("hash_only", "bool"),
            ("before", "string|null"),
            ("after", "string|null"),
            ("candidates", "array"),
            ("old_path", "string|null"),
            ("why", "any"),
        ],
    );
    has(&row["actor"], "actor", &[("kind", "string")]);
}

#[test]
fn blame_rows_one_per_line() {
    let repo = repo();
    let rows = repo.me(&["blame", "f.txt", "--json"]).json();
    let rows = rows.as_array().unwrap();
    assert_eq!(rows.len(), 3);
    for row in rows {
        has(
            row,
            "blame row",
            &[
                ("line", "number"),
                ("text", "string"),
                ("change", "string"),
                ("actor", "object"),
                ("time", "string"),
                ("why", "string|null"),
            ],
        );
    }
}

#[test]
fn why_of_a_line() {
    let repo = repo();
    let why = repo.me(&["why", "f.txt:3", "--json"]).json();
    has(&why, "why", &[("change", "object"), ("history", "array"), ("intent", "any"), ("line", "number")]);
    has(&why["change"], "why.change", &[("id", "string"), ("path", "string"), ("op", "string"), ("actor", "object"), ("time", "object")]);
}

#[test]
fn status() {
    let repo = repo();
    let status = repo.me(&["status", "--json"]).json();
    has(
        &status,
        "status",
        &[
            ("repo", "string"),
            ("data_dir", "string"),
            ("changes", "number"),
            ("locks", "number"),
            ("daemon", "string"),
            ("repo_id", "string"),
            ("share", "any"),
            ("profile", "object"),
            ("capture", "object"),
        ],
    );
    has(&status["capture"], "status.capture", &[("mode", "string")]);
}

#[test]
fn doctor() {
    let repo = repo();
    let doctor = repo.me(&["doctor", "--json"]).json();
    has(&doctor, "doctor", &[("ok", "bool"), ("problems", "any"), ("coverage", "object"), ("attribution", "object"), ("profile", "any")]);
    has(
        &doctor["coverage"],
        "doctor.coverage",
        &[("agents", "object"), ("shell", "bool"), ("shims", "bool"), ("git_wrapper", "bool"), ("daemon", "bool"), ("capture", "object")],
    );
    has(&doctor["coverage"]["capture"], "doctor.coverage.capture", &[("mode", "string"), ("live", "array"), ("caught_up", "array")]);
}

#[test]
fn pending_rows() {
    let repo = Repo::new("");
    repo.edit_as("agent:claude:s1", "a.txt", "x\n");
    let rows = repo.me(&["pending", "--json"]).json();
    let row = &rows.as_array().unwrap()[0];
    has(row, "pending row", &[("id", "string"), ("path", "string"), ("actor", "object"), ("time", "object")]);
}

#[test]
fn file_view() {
    let repo = repo();
    let file = repo.me(&["status", "f.txt", "--json"]).json();
    has(&file, "file", &[("path", "string"), ("changes", "array"), ("lock", "any")]);
}

#[test]
fn scan_and_record_answer_with_the_ids_they_recorded() {
    let repo = Repo::new("");
    repo.write("a.txt", "one\n");
    has(&repo.me(&["record", "a.txt", "--json"]).json(), "record", &[("changes", "array")]);
    repo.write("b.txt", "two\n");
    has(&repo.me(&["scan", "--json"]).json(), "scan", &[("changes", "array")]);
}

#[test]
fn the_companion_names_what_this_program_can_do() {
    let repo = Repo::new("");
    let info = repo.me(&["companion", "--json"]).json();
    has(&info, "companion", &[("api", "number"), ("name", "string"), ("version", "string"), ("features", "array")]);
    let features: Vec<&str> = info["features"].as_array().unwrap().iter().map(|f| f.as_str().unwrap()).collect();
    for wanted in ["json-contract", "record"] {
        assert!(features.contains(&wanted), "feature `{wanted}` is missing: {features:?}");
    }
}
