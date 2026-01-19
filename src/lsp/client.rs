//! Non-blocking LSP client for diagnostics.
//!
//! Uses a background worker thread to communicate with the LSP server,
//! so the main editor thread is never blocked by slow LSP responses.

use std::{
    io::{BufRead, BufReader, BufWriter, Write},
    process::{Child, ChildStdin, ChildStdout, Command, Stdio},
    thread,
    time::{Duration, Instant},
};

use crossbeam::channel::{bounded, Receiver, Sender};
use lsp_types::{
    CodeAction, Diagnostic, DidChangeTextDocumentParams, DidOpenTextDocumentParams,
    InitializeParams, InitializedParams, PublishDiagnosticsParams, TextDocumentContentChangeEvent,
    TextDocumentItem, VersionedTextDocumentIdentifier,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::{actions::request_code_actions, diagnostic::Span};

/// LSP server configuration.
#[derive(Debug, Clone)]
pub struct LspConfig {
    /// Full command to start the LSP server (e.g., "nu-lint --lsp")
    pub command: String,
    /// Response timeout in milliseconds
    pub timeout_ms: u64,
    /// URI scheme (default: "repl")
    pub uri_scheme: String,
}

// Channel capacity for commands and responses
const CHANNEL_CAPACITY: usize = 32;

/// Commands sent from main thread to worker.
enum LspCommand {
    UpdateContent(String),
    RequestCodeActions { content: String, span: Span },
    Shutdown,
}

/// Responses sent from worker to main thread.
enum LspResponse {
    Diagnostics(Vec<Diagnostic>),
    CodeActions(Vec<CodeAction>),
}

/// LSP diagnostics provider (main thread interface).
///
/// Provides a non-blocking interface to LSP diagnostics.
/// All communication with the LSP server happens in a background thread.
pub struct LspDiagnosticsProvider {
    command_tx: Sender<LspCommand>,
    response_rx: Receiver<LspResponse>,
    wake_rx: Receiver<()>,
    diagnostics: Vec<Diagnostic>,
    last_content_hash: u64,
}

/// Background worker that owns the LSP connection.
struct LspWorker {
    config: LspConfig,
    conn: Option<Connection>,
    uri: String,
    version: i32,
    command_rx: Receiver<LspCommand>,
    response_tx: Sender<LspResponse>,
    wake_tx: Sender<()>,
}

struct Connection {
    #[allow(dead_code)]
    child: Child,
    writer: BufWriter<ChildStdin>,
    reader: BufReader<ChildStdout>,
    next_id: i32,
}

impl LspDiagnosticsProvider {
    /// Create new provider and spawn worker thread.
    #[must_use]
    pub fn new(config: LspConfig) -> Self {
        let (command_tx, command_rx) = bounded(CHANNEL_CAPACITY);
        let (response_tx, response_rx) = bounded(CHANNEL_CAPACITY);
        let (wake_tx, wake_rx) = bounded(1);

        let worker = LspWorker {
            uri: format!("{}:/session/repl", config.uri_scheme),
            config,
            conn: None,
            version: 0,
            command_rx,
            response_tx,
            wake_tx,
        };

        thread::spawn(move || worker.run());

        Self {
            command_tx,
            response_rx,
            wake_rx,
            diagnostics: Vec::new(),
            last_content_hash: 0,
        }
    }

    /// Update content (non-blocking). Sends to worker if content changed.
    pub fn update_content(&mut self, content: &str) {
        if content.is_empty() {
            self.diagnostics.clear();
            return;
        }

        // Only send if content changed to avoid flooding the worker
        let hash = hash_str(content);
        if hash != self.last_content_hash {
            self.last_content_hash = hash;
            let _ = self
                .command_tx
                .try_send(LspCommand::UpdateContent(content.to_string()));
        }
    }

    /// Get current diagnostics, polling for any new responses first.
    pub fn diagnostics(&mut self) -> &[Diagnostic] {
        self.poll_responses();
        &self.diagnostics
    }

    /// Get code actions for a given span.
    pub fn code_actions(&mut self, content: &str, span: Span) -> Vec<CodeAction> {
        let _ = self.command_tx.try_send(LspCommand::RequestCodeActions {
            content: content.to_string(),
            span,
        });

        // Brief wait for response
        let start = Instant::now();
        while start.elapsed() < Duration::from_millis(100) {
            match self.response_rx.recv_timeout(Duration::from_millis(10)) {
                Ok(LspResponse::CodeActions(actions)) => return actions,
                Ok(LspResponse::Diagnostics(diags)) => self.diagnostics = diags,
                Err(_) => {}
            }
        }
        Vec::new()
    }

    /// Poll for responses from worker (non-blocking).
    fn poll_responses(&mut self) {
        while let Ok(response) = self.response_rx.try_recv() {
            match response {
                LspResponse::Diagnostics(diags) => self.diagnostics = diags,
                LspResponse::CodeActions(_) => {} // Ignore stale code actions
            }
        }
    }

    /// Check if worker has signaled new diagnostics are available.
    /// If so, polls responses and returns true.
    pub fn check_wake(&mut self) -> bool {
        if self.wake_rx.try_recv().is_ok() {
            self.poll_responses();
            true
        } else {
            false
        }
    }
}

impl Drop for LspDiagnosticsProvider {
    fn drop(&mut self) {
        let _ = self.command_tx.try_send(LspCommand::Shutdown);
        // Worker will exit when channel disconnects
    }
}

fn hash_str(s: &str) -> u64 {
    use std::{
        collections::hash_map::DefaultHasher,
        hash::{Hash, Hasher},
    };
    let mut h = DefaultHasher::new();
    s.hash(&mut h);
    h.finish()
}

// Worker implementation

impl LspWorker {
    fn run(mut self) {
        loop {
            // Block waiting for commands (with timeout to allow graceful shutdown)
            match self.command_rx.recv_timeout(Duration::from_millis(50)) {
                Ok(LspCommand::Shutdown) => {
                    self.shutdown();
                    return;
                }
                Ok(LspCommand::UpdateContent(content)) => {
                    self.handle_update_content(&content);
                }
                Ok(LspCommand::RequestCodeActions { content, span }) => {
                    self.handle_code_actions_request(&content, span);
                }
                Err(crossbeam::channel::RecvTimeoutError::Disconnected) => {
                    self.shutdown();
                    return;
                }
                Err(crossbeam::channel::RecvTimeoutError::Timeout) => {
                    // No commands, continue loop
                }
            }
        }
    }

    fn handle_update_content(&mut self, content: &str) {
        if content.is_empty() {
            self.send_diagnostics(Vec::new());
            return;
        }

        if !self.ensure_init() {
            return;
        }

        self.version += 1;
        let Some(conn) = self.conn.as_mut() else {
            return;
        };
        let Some(uri) = self.uri.parse().ok() else {
            return;
        };

        let params = DidChangeTextDocumentParams {
            text_document: VersionedTextDocumentIdentifier {
                uri,
                version: self.version,
            },
            content_changes: vec![TextDocumentContentChangeEvent {
                range: None,
                range_length: None,
                text: content.into(),
            }],
        };
        let _ = notify(conn, "textDocument/didChange", &params);

        self.poll_for_diagnostics();
    }

    fn send_diagnostics(&self, diagnostics: Vec<Diagnostic>) {
        let _ = self
            .response_tx
            .try_send(LspResponse::Diagnostics(diagnostics));
        let _ = self.wake_tx.try_send(());
    }

    fn handle_code_actions_request(&mut self, content: &str, span: Span) {
        let actions = self
            .conn
            .as_mut()
            .map(|conn| {
                request_code_actions(
                    &self.uri,
                    content,
                    span,
                    self.config.timeout_ms,
                    |method, params, timeout| request(conn, method, params, timeout),
                )
            })
            .unwrap_or_default();

        let _ = self.response_tx.try_send(LspResponse::CodeActions(actions));
    }

    fn poll_for_diagnostics(&mut self) {
        let Some(conn) = &mut self.conn else { return };

        let timeout = Duration::from_millis(self.config.timeout_ms);
        let start = Instant::now();

        let diagnostics =
            std::iter::from_fn(|| read_msg(&mut conn.reader, Duration::from_millis(5)))
                .take_while(|_| start.elapsed() < timeout)
                .filter(|msg| msg.method.as_deref() == Some("textDocument/publishDiagnostics"))
                .filter_map(|msg| msg.params)
                .filter_map(|params| {
                    serde_json::from_value::<PublishDiagnosticsParams>(params).ok()
                })
                .next()
                .map(|p| p.diagnostics);

        if let Some(diagnostics) = diagnostics {
            self.send_diagnostics(diagnostics);
        }
    }

    fn ensure_init(&mut self) -> bool {
        if self.conn.is_some() {
            return true;
        }
        self.conn = self.try_init();
        self.conn.is_some()
    }

    fn try_init(&self) -> Option<Connection> {
        let mut parts = self.config.command.split_whitespace();
        let bin = parts.next()?;
        let args: Vec<&str> = parts.collect();

        let mut child = Command::new(bin)
            .args(&args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .ok()?;

        let mut conn = Connection {
            writer: BufWriter::new(child.stdin.take()?),
            reader: BufReader::new(child.stdout.take()?),
            child,
            next_id: 1,
        };

        let init_params = InitializeParams {
            process_id: Some(std::process::id()),
            client_info: Some(lsp_types::ClientInfo {
                name: "reedline".into(),
                version: Some(env!("CARGO_PKG_VERSION").into()),
            }),
            ..Default::default()
        };

        request(
            &mut conn,
            "initialize",
            &init_params,
            self.config.timeout_ms * 5,
        )?;
        notify(&mut conn, "initialized", &InitializedParams {})?;
        notify(
            &mut conn,
            "textDocument/didOpen",
            &DidOpenTextDocumentParams {
                text_document: TextDocumentItem {
                    uri: self.uri.parse().ok()?,
                    language_id: "nushell".into(),
                    version: 0,
                    text: String::new(),
                },
            },
        )?;

        Some(conn)
    }

    fn shutdown(&mut self) {
        if let Some(mut conn) = self.conn.take() {
            let _ = request(&mut conn, "shutdown", &(), 100);
            let _ = notify(&mut conn, "exit", &());
            thread::sleep(Duration::from_millis(20));
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

fn request<T: Serialize>(
    conn: &mut Connection,
    method: &str,
    params: &T,
    timeout_ms: u64,
) -> Option<Value> {
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
