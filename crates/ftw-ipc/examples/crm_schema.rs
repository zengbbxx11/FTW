use ftw_ipc::crm::{Command, Result as Reply};
use schemars::generate::SchemaSettings;
use std::{env, process::ExitCode};

fn main() -> ExitCode {
    let generator = SchemaSettings::draft07().into_generator();
    let value = match env::args().nth(1).as_deref() {
        Some("command") => generator.into_root_schema_for::<Command>(),
        Some("result") => generator.into_root_schema_for::<Reply>(),
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
