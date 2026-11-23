use ftw_ipc::crm::{Command, Result as Reply};
use schemars::schema_for;
use std::{env, process::ExitCode};

fn main() -> ExitCode {
    let value = match env::args().nth(1).as_deref() {
        Some("command") => schema_for!(Command),
        Some("result") => schema_for!(Reply),
        _ => {
            eprintln!("usage: crm_schema <command|result>");
            return ExitCode::from(2);
        }
    };
    println!(
        "{}",
        serde_json::to_string_pretty(&value).expect("serialize CRM schema")
    );
    ExitCode::SUCCESS
}
