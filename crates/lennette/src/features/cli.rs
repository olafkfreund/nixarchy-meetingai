//! Local AI CLIs.
//!
//! Detects installed command-line assistants and streams their output. Each CLI
//! has its own invocation shape, so the catalog below owns the arguments while a
//! single runner handles spawning, line streaming and cancellation.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::process::Stdio;

use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;

/// Errors from running a local CLI.
#[derive(Debug, thiserror::Error)]
pub enum CliError {
    #[error("{name} is not installed or not on PATH")]
    NotFound { name: &'static str },
    #[error("could not start {name}: {error}")]
    Spawn { name: &'static str, error: String },
    #[error("could not read {name} output: {error}")]
    Io { name: &'static str, error: String },
}

/// A supported local CLI.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CliSpec {
    pub id: &'static str,
    pub name: &'static str,
    /// Binary looked up on PATH.
    pub binary: &'static str,
}

/// Every CLI lennette knows how to drive.
pub const CLIS: &[CliSpec] = &[
    CliSpec {
        id: "claude",
        name: "Claude Code",
        binary: "claude",
    },
    CliSpec {
        id: "codex",
        name: "OpenAI Codex",
        binary: "codex",
    },
    CliSpec {
        id: "gemini",
        name: "Gemini CLI",
        binary: "gemini",
    },
    CliSpec {
        id: "ollama",
        name: "Ollama",
        binary: "ollama",
    },
];

impl CliSpec {
    pub fn by_id(id: &str) -> Option<&'static CliSpec> {
        CLIS.iter().find(|spec| spec.id == id)
    }

    /// Arguments for a one-shot prompt.
    ///
    /// Shapes follow each tool's documented non-interactive mode.
    pub fn args(self, prompt: &str) -> Vec<String> {
        let prompt = prompt.to_owned();
        match self.id {
            "claude" => vec!["-p".into(), prompt, "--output-format".into(), "text".into()],
            "codex" => vec!["exec".into(), prompt],
            "gemini" => vec!["-p".into(), prompt],
            "ollama" => vec!["run".into(), "llama3.2".into(), prompt],
            _ => vec![prompt],
        }
    }
}

/// Detect which CLIs are installed, with their resolved paths.
pub fn detect() -> BTreeMap<&'static str, PathBuf> {
    let mut found = BTreeMap::new();
    for spec in CLIS {
        if let Some(path) = which(spec.binary) {
            found.insert(spec.id, path);
        }
    }
    found
}

/// Resolve a binary on `PATH`.
pub fn which(binary: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path)
        .map(|dir| dir.join(binary))
        .find(|candidate| candidate.is_file())
}

/// A running CLI invocation.
pub struct CliStream {
    child: tokio::process::Child,
    lines: tokio::io::Lines<BufReader<tokio::process::ChildStdout>>,
    name: &'static str,
}

impl CliStream {
    /// Spawn a CLI with a prompt.
    pub fn spawn(spec: &CliSpec, prompt: &str, path: Option<PathBuf>) -> Result<Self, CliError> {
        let binary = path.unwrap_or_else(|| PathBuf::from(spec.binary));
        if !binary.is_file() && which(spec.binary).is_none() {
            return Err(CliError::NotFound { name: spec.name });
        }
        let mut child = Command::new(&binary)
            .args(spec.args(prompt))
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true)
            .spawn()
            .map_err(|error| CliError::Spawn {
                name: spec.name,
                error: error.to_string(),
            })?;
        let stdout = child.stdout.take().ok_or_else(|| CliError::Io {
            name: spec.name,
            error: "no stdout".into(),
        })?;
        Ok(Self {
            child,
            lines: BufReader::new(stdout).lines(),
            name: spec.name,
        })
    }

    /// Next line of output, trimmed. `None` at end of stream.
    pub async fn next_line(&mut self) -> Result<Option<String>, CliError> {
        match self.lines.next_line().await {
            Ok(Some(line)) => Ok(Some(line)),
            Ok(None) => Ok(None),
            Err(error) => Err(CliError::Io {
                name: self.name,
                error: error.to_string(),
            }),
        }
    }

    /// Wait for exit, returning the status code.
    pub async fn finish(mut self) -> Result<Option<i32>, CliError> {
        let status = self.child.wait().await.map_err(|error| CliError::Io {
            name: self.name,
            error: error.to_string(),
        })?;
        Ok(status.code())
    }

    /// Stop the process immediately.
    pub async fn cancel(mut self) -> Result<(), CliError> {
        self.child.kill().await.map_err(|error| CliError::Io {
            name: self.name,
            error: error.to_string(),
        })
    }
}

/// Parse `ollama list` output into model names.
pub fn parse_ollama_models(output: &str) -> Vec<String> {
    output
        .lines()
        .skip(1)
        .filter_map(|line| line.split_whitespace().next())
        .filter(|name| !name.is_empty() && *name != "NAME")
        .map(str::to_owned)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cli_ids_are_unique() {
        let mut seen = std::collections::HashSet::new();
        for spec in CLIS {
            assert!(seen.insert(spec.id), "duplicate {}", spec.id);
        }
    }

    #[test]
    fn argument_shapes_are_sane() {
        let claude = CliSpec::by_id("claude").unwrap();
        assert_eq!(
            claude.args("hi"),
            vec!["-p", "hi", "--output-format", "text"]
        );

        let codex = CliSpec::by_id("codex").unwrap();
        assert_eq!(codex.args("hi"), vec!["exec", "hi"]);

        let ollama = CliSpec::by_id("ollama").unwrap();
        assert_eq!(ollama.args("hi")[..2], ["run", "llama3.2"]);
    }

    #[test]
    fn unknown_cli_is_none() {
        assert!(CliSpec::by_id("nope").is_none());
    }

    #[test]
    fn ollama_list_parsing_skips_header() {
        let output = "NAME             ID              SIZE      MODIFIED\n\
                      llama3.2:latest  a80c4f17acd5    2.0 GB    2 days ago\n\
                      qwen2.5:7b       abc123def456    4.7 GB    1 week ago\n";
        assert_eq!(
            parse_ollama_models(output),
            vec!["llama3.2:latest", "qwen2.5:7b"]
        );
        assert!(parse_ollama_models("NAME ID\n").is_empty());
    }

    #[test]
    fn missing_binary_reports_clearly() {
        let spec = CliSpec {
            id: "missing",
            name: "Missing CLI",
            binary: "lennette-definitely-not-installed-xyz",
        };
        let error = match CliStream::spawn(&spec, "hi", None) {
            Err(error) => error,
            Ok(_) => panic!("a missing binary must not spawn"),
        };
        assert!(matches!(error, CliError::NotFound { .. }));
        assert_eq!(
            error.to_string(),
            "Missing CLI is not installed or not on PATH"
        );
    }
}
