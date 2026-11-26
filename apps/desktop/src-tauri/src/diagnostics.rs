use crate::{development_root, Issue, CREATE_NO_WINDOW};
use serde::Serialize;
use serde_json::{json, Value};
use std::{env, io, process::Stdio, time::Duration};
use tokio::{
    io::{AsyncBufRead, AsyncBufReadExt, AsyncReadExt, AsyncWrite, AsyncWriteExt, BufReader},
    process::Command,
    time::timeout,
};

const PROTOCOL: &str = "2025-11-25";
const MAX_RESPONSE: u64 = 1_048_576;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct McpStatus {
    protocol: String,
    tool_count: usize,
}

pub(crate) async fn check_mcp() -> Result<McpStatus, Issue> {
    timeout(Duration::from_secs(10), query_mcp())
        .await
        .map_err(|_| Issue::new("mcp_probe_timeout", "Local MCP probe timed out"))?
}

async fn query_mcp() -> Result<McpStatus, Issue> {
    let path = env::current_exe()
        .map_err(failed)?
        .with_file_name("ftw-mcp.exe");
    let mut command = Command::new(path);
    if let Some(profile) = development_root() {
        command.arg("--dev-root").arg(profile);
    }
    let mut child = command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .env_remove("FTW_AGENT_TOKEN")
        .creation_flags(CREATE_NO_WINDOW)
        .kill_on_drop(true)
        .spawn()
        .map_err(|error| {
            if error.kind() == io::ErrorKind::NotFound {
                Issue::new("mcp_missing", "ftw-mcp.exe is unavailable")
            } else {
                failed(error)
            }
        })?;
    let mut input = child
        .stdin
        .take()
        .ok_or_else(|| failed("Unable to open MCP input"))?;
    let mut output = BufReader::new(
        child
            .stdout
            .take()
            .ok_or_else(|| failed("Unable to open MCP output"))?,
    );

    send(&mut input, json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{
        "protocolVersion":PROTOCOL,"capabilities":{},"clientInfo":{"name":"FTW Workbench","version":env!("CARGO_PKG_VERSION")}
    }})).await?;
    let initialize = receive(&mut output, 1).await?;
    let protocol = initialize["protocolVersion"]
        .as_str()
        .filter(|value| *value == PROTOCOL)
        .ok_or_else(|| failed("Unexpected MCP protocol version"))?;

    send(
        &mut input,
        json!({"jsonrpc":"2.0","method":"notifications/initialized"}),
    )
    .await?;
    send(
        &mut input,
        json!({"jsonrpc":"2.0","id":2,"method":"tools/list","params":{}}),
    )
    .await?;
    let tools = receive(&mut output, 2).await?;
    let tools = tools["tools"]
        .as_array()
        .filter(|tools| tools.iter().any(|tool| tool["name"] == "query_customers"))
        .ok_or_else(|| failed("MCP customer query tool is unavailable"))?;

    send(
        &mut input,
        json!({"jsonrpc":"2.0","id":3,"method":"tools/call","params":{
            "name":"query_customers","arguments":{"keyword":"__FTW_LOCAL_HEALTH_PROBE__"}
        }}),
    )
    .await?;
    let result = receive(&mut output, 3).await?;
    let crm = result["content"][0]["text"]
        .as_str()
        .and_then(|value| serde_json::from_str::<Value>(value).ok());
    if result["isError"] == true || !crm.is_some_and(|value| value["kind"] == "contacts") {
        return Err(failed(
            "MCP read-only query did not return customer results",
        ));
    }
    Ok(McpStatus {
        protocol: protocol.to_owned(),
        tool_count: tools.len(),
    })
}

async fn send(output: &mut (impl AsyncWrite + Unpin), message: Value) -> Result<(), Issue> {
    let mut bytes = serde_json::to_vec(&message).map_err(failed)?;
    bytes.push(b'\n');
    output.write_all(&bytes).await.map_err(failed)?;
    output.flush().await.map_err(failed)
}

async fn receive(input: &mut (impl AsyncBufRead + Unpin), id: u64) -> Result<Value, Issue> {
    for _ in 0..8 {
        let mut line = String::new();
        (&mut *input)
            .take(MAX_RESPONSE + 1)
            .read_line(&mut line)
            .await
            .map_err(failed)?;
        if line.len() as u64 > MAX_RESPONSE || !line.ends_with('\n') {
            return Err(failed("Incomplete MCP response"));
        }
        let message: Value =
            serde_json::from_str(&line).map_err(|_| failed("Invalid MCP response"))?;
        if message["id"].as_u64() == Some(id) {
            if let Some(result) = message.get("result") {
                return Ok(result.clone());
            }
            return Err(failed("MCP returned a protocol error"));
        }
    }
    Err(failed("MCP response ID was not found"))
}

fn failed(detail: impl ToString) -> Issue {
    Issue::new("mcp_probe_failed", detail)
}
