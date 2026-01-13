//! Minimal synchronous LSP client for diagnostics.

use super::diagnostic::{CodeAction, Diagnostic, DiagnosticSeverity, Span};
use lsp_types::{
    DidChangeTextDocumentParams, DidOpenTextDocumentParams, InitializeParams, InitializedParams,
    NumberOrString, Position, PublishDiagnosticsParams, Range, TextDocumentContentChangeEvent,
    TextDocumentItem, VersionedTextDocumentIdentifier,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    io::{BufRead, BufReader, BufWriter, Write},
    process::{Child, ChildStdin, ChildStdout, Command, Stdio},
    time::{Duration, Instant},
};

/// LSP server configuration.
#[derive(Debug, Clone)]
pub struct LspConfig {
    /// Server command
    pub server_cmd: String,
    /// Server arguments
    pub server_args: Vec<String>,
    /// Response timeout (ms)
    pub timeout_ms: u64,
    /// URI scheme (default: "repl")
    pub uri_scheme: String,
}

impl LspConfig {
    /// Create configuration for server command.
    pub fn new(cmd: impl Into<String>) -> Self {
        Self {
            server_cmd: cmd.into(),
            server_args: Vec::new(),
            timeout_ms: 200,
            uri_scheme: "repl".into(),
        }
    }

    /// Set server arguments.
    #[must_use]
    pub fn with_args(mut self, args: Vec<String>) -> Self {
        self.server_args = args;
        self
    }

    /// Set timeout.
    #[must_use]
    pub fn with_timeout_ms(mut self, ms: u64) -> Self {
        self.timeout_ms = ms;
        self
    }
}

/// LSP diagnostics provider.
pub struct LspDiagnosticsProvider {
    config: LspConfig,
    conn: Option<Connection>,
    uri: String,
    version: i32,
    diagnostics: Vec<Diagnostic>,
}

struct Connection {
    #[allow(dead_code)]
    child: Child,
    writer: BufWriter<ChildStdin>,
    reader: BufReader<ChildStdout>,
    next_id: i32,
}

impl LspDiagnosticsProvider {
    /// Create new provider.
    pub fn new(config: LspConfig) -> Self {
        let uri = format!("{}:/session/repl", config.uri_scheme);
        Self {
            config,
            conn: None,
            uri,
            version: 0,
            diagnostics: Vec::new(),
        }
    }

    /// Update content and poll for diagnostics.
    pub fn update_content(&mut self, content: &str) {
        if content.is_empty() {
            self.diagnostics.clear();
            return;
        }
        if !self.ensure_init() {
            return;
        }

        self.version += 1;
        let conn = self.conn.as_mut().unwrap();

        let params = DidChangeTextDocumentParams {
            text_document: VersionedTextDocumentIdentifier {
                uri: self.uri.parse().unwrap(),
                version: self.version,
            },
            content_changes: vec![TextDocumentContentChangeEvent {
                range: None,
                range_length: None,
                text: content.into(),
            }],
        };
        let _ = notify(conn, "textDocument/didChange", &params);

        self.poll(content);
    }

    /// Get current diagnostics.
    pub fn diagnostics(&self) -> &[Diagnostic] {
        &self.diagnostics
    }

    /// Get code actions (stub).
    pub fn code_actions(&mut self, _content: &str, _span: Span) -> Vec<CodeAction> {
        Vec::new()
    }

    fn ensure_init(&mut self) -> bool {
        if self.conn.is_some() {
            return true;
        }

        let mut child = match Command::new(&self.config.server_cmd)
            .args(&self.config.server_args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
        {
            Ok(c) => c,
            Err(_) => return false,
        };

        let stdin = match child.stdin.take() {
            Some(s) => s,
            None => return false,
        };
        let stdout = match child.stdout.take() {
            Some(s) => s,
            None => return false,
        };

        let mut conn = Connection {
            child,
            writer: BufWriter::new(stdin),
            reader: BufReader::new(stdout),
            next_id: 1,
        };

        // Initialize
        let params = InitializeParams {
            process_id: Some(std::process::id()),
            root_uri: None,
            root_path: None,
            initialization_options: None,
            capabilities: Default::default(),
            trace: None,
            workspace_folders: None,
            client_info: Some(lsp_types::ClientInfo {
                name: "reedline".into(),
                version: Some(env!("CARGO_PKG_VERSION").into()),
            }),
            locale: None,
            work_done_progress_params: Default::default(),
        };

        if request(&mut conn, "initialize", &params, self.config.timeout_ms * 5).is_none() {
            return false;
        }
        let _ = notify(&mut conn, "initialized", &InitializedParams {});

        // Open document
        let _ = notify(
            &mut conn,
            "textDocument/didOpen",
            &DidOpenTextDocumentParams {
                text_document: TextDocumentItem {
                    uri: self.uri.parse().unwrap(),
                    language_id: "nushell".into(),
                    version: 0,
                    text: String::new(),
                },
            },
        );

        self.conn = Some(conn);
        true
    }

    fn poll(&mut self, content: &str) {
        let conn = match &mut self.conn {
            Some(c) => c,
            None => return,
        };
        let timeout = Duration::from_millis(self.config.timeout_ms);
        let start = Instant::now();

        while start.elapsed() < timeout {
            if let Some(msg) = read_msg(&mut conn.reader, Duration::from_millis(5)) {
                if msg.method.as_deref() == Some("textDocument/publishDiagnostics") {
                    if let Some(params) = msg.params {
                        if let Ok(p) = serde_json::from_value::<PublishDiagnosticsParams>(params) {
                            self.diagnostics = p.diagnostics.iter().map(|d| convert(d, content)).collect();
                            return;
                        }
                    }
                }
            }
        }
    }
}

impl Drop for LspDiagnosticsProvider {
    fn drop(&mut self) {
        if let Some(mut conn) = self.conn.take() {
            let _ = request(&mut conn, "shutdown", &(), 100);
            let _ = notify(&mut conn, "exit", &());
            std::thread::sleep(Duration::from_millis(20));
            let _ = conn.child.kill();
        }
    }
}

// JSON-RPC helpers

#[derive(Serialize, Deserialize)]
struct Msg {
    jsonrpc: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    id: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    method: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    params: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<Value>,
}

fn request<T: Serialize>(conn: &mut Connection, method: &str, params: &T, timeout_ms: u64) -> Option<Value> {
    let id = conn.next_id;
    conn.next_id += 1;

    let msg = Msg {
        jsonrpc: "2.0".into(),
        id: Some(id),
        method: Some(method.into()),
        params: serde_json::to_value(params).ok(),
        result: None,
        error: None,
    };
    write_msg(&mut conn.writer, &msg).ok()?;

    let timeout = Duration::from_millis(timeout_ms);
    let start = Instant::now();
    while start.elapsed() < timeout {
        if let Some(resp) = read_msg(&mut conn.reader, Duration::from_millis(10)) {
            if resp.id == Some(id) {
                return resp.result;
            }
        }
    }
    None
}

fn notify<T: Serialize>(conn: &mut Connection, method: &str, params: &T) -> Option<()> {
    let msg = Msg {
        jsonrpc: "2.0".into(),
        id: None,
        method: Some(method.into()),
        params: serde_json::to_value(params).ok(),
        result: None,
        error: None,
    };
    write_msg(&mut conn.writer, &msg).ok()
}

fn write_msg<W: Write>(w: &mut W, msg: &Msg) -> std::io::Result<()> {
    let json = serde_json::to_string(msg)?;
    write!(w, "Content-Length: {}\r\n\r\n{}", json.len(), json)?;
    w.flush()
}

fn read_msg<R: BufRead>(r: &mut R, timeout: Duration) -> Option<Msg> {
    let start = Instant::now();
    let mut header = String::new();

    while start.elapsed() < timeout {
        header.clear();
        if r.read_line(&mut header).ok()? == 0 {
            return None;
        }
        if let Some(len) = header.strip_prefix("Content-Length:") {
            let len: usize = len.trim().parse().ok()?;
            let mut empty = String::new();
            r.read_line(&mut empty).ok()?;
            let mut buf = vec![0u8; len];
            r.read_exact(&mut buf).ok()?;
            return serde_json::from_slice(&buf).ok();
        }
    }
    None
}

// Conversion

fn convert(d: &lsp_types::Diagnostic, content: &str) -> Diagnostic {
    let severity = match d.severity {
        Some(lsp_types::DiagnosticSeverity::ERROR) => DiagnosticSeverity::Error,
        Some(lsp_types::DiagnosticSeverity::WARNING) => DiagnosticSeverity::Warning,
        Some(lsp_types::DiagnosticSeverity::INFORMATION) => DiagnosticSeverity::Info,
        Some(lsp_types::DiagnosticSeverity::HINT) => DiagnosticSeverity::Hint,
        _ => DiagnosticSeverity::Warning,
    };

    let span = range_to_span(content, &d.range);
    let mut diag = Diagnostic::new(severity, span, &d.message);

    if let Some(code) = &d.code {
        diag = diag.with_rule_id(match code {
            NumberOrString::Number(n) => n.to_string(),
            NumberOrString::String(s) => s.clone(),
        });
    }
    diag
}

fn range_to_span(content: &str, range: &Range) -> Span {
    Span::new(pos_to_offset(content, &range.start), pos_to_offset(content, &range.end))
}

fn pos_to_offset(content: &str, pos: &Position) -> usize {
    let mut off = 0;
    for (i, line) in content.lines().enumerate() {
        if i == pos.line as usize {
            return off + (pos.character as usize).min(line.len());
        }
        off += line.len() + 1;
    }
    content.len()
}
