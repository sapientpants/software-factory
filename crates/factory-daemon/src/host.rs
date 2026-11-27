//! What the host has installed: the toolchain that a repository names and the
//! check tools that its commands need, as directories on the host's own disk
//! (SPEC WS-7, WS-7a, 19.1).
//!
//! `Ecosystem` says what to look for, and this module looks. It runs no
//! program, neither rustup nor one of the toolchain nor a check tool: the
//! version of each is what its path says. What it finds is the set of paths
//! that a `SandboxPolicy` is given, the directories that a sandbox shows
//! read-only and the `bin` directories of the worker `PATH`. No other part of
//! the toolchains directory or of the tools directory is among them, and so
//! no shim of rustup or of mise.

use std::collections::BTreeSet;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use factory_core::check_tool::CheckTool;
use factory_core::ecosystem::{
    Ecosystem, HostTriple, PartKind, ToolchainFileError, ToolchainLookup,
};

/// The two directories of the operator configuration that the lookup reads
/// (16.4): `[ecosystem.<name>] toolchains_dir` and `tools_dir`. Where the
/// configuration leaves one out, nothing is present below it.
#[derive(Debug, Clone, Copy, Default)]
pub struct Directories<'a> {
    /// The toolchains directory of the repository's ecosystem (WS-7).
    pub toolchains: Option<&'a Path>,
    /// The tools directory (WS-7a).
    pub tools: Option<&'a Path>,
}

/// A toolchain that is present.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Toolchain {
    /// Its directory, where a symbolic link of the toolchains directory leads
    /// (WS-7). The sandbox shows it read-only.
    pub directory: PathBuf,
    /// The directory that goes first on the worker `PATH`.
    pub bin: PathBuf,
}

/// A check tool that is present.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FoundCheckTool {
    /// The tool.
    pub tool: CheckTool,
    /// `<tools directory>/<name>/<version>`, which the sandbox shows
    /// read-only (WS-7a).
    pub directory: PathBuf,
    /// Its `bin`, which joins the worker `PATH`.
    pub bin: PathBuf,
}

/// The toolchain and the check tools that a sandbox is given (WS-7, WS-7a).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HostPaths {
    /// The toolchain.
    pub toolchain: Toolchain,
    /// The check tools, in order of name.
    pub tools: Vec<FoundCheckTool>,
}

impl HostPaths {
    /// The worker `PATH`, as far as the host's installations make it: the
    /// toolchain's `bin` first, then each check tool's in order of name
    /// (WS-7a).
    #[must_use]
    pub fn worker_path(&self) -> Vec<PathBuf> {
        std::iter::once(&self.toolchain.bin)
            .chain(self.tools.iter().map(|tool| &tool.bin))
            .cloned()
            .collect()
    }

    /// The directories that the sandbox shows read-only (SEC-6): the
    /// toolchain's and that of each check tool.
    #[must_use]
    pub fn read_only(&self) -> Vec<PathBuf> {
        std::iter::once(&self.toolchain.directory)
            .chain(self.tools.iter().map(|tool| &tool.directory))
            .cloned()
            .collect()
    }
}

/// One thing that the host lacks, by name (WS-7, WS-7a).
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum Missing {
    /// The tree holds no file that names a toolchain.
    #[error("the toolchain is missing: the tree holds no `{file}`, so it names none")]
    NoToolchainFile {
        /// The file, as the ecosystem names it.
        file: &'static str,
    },
    /// The file names no toolchain that the host can look for.
    #[error("the toolchain is missing: {0}")]
    ToolchainFile(ToolchainFileError),
    /// The toolchain, with what is wrong with its directory.
    #[error("the toolchain {toolchain} is missing: {problem}")]
    Toolchain {
        /// The toolchain, as the file names it.
        toolchain: String,
        /// What is wrong.
        problem: String,
    },
    /// A component that the file names.
    #[error("the component `{component}` of the toolchain {toolchain} is missing")]
    Component {
        /// The toolchain.
        toolchain: String,
        /// The component.
        component: String,
    },
    /// A target that the file names.
    #[error("the target `{target}` of the toolchain {toolchain} is missing")]
    Target {
        /// The toolchain.
        toolchain: String,
        /// The target.
        target: String,
    },
    /// A check tool, with what is wrong with its directory.
    #[error("the check tool {tool} is missing: {problem}")]
    Tool {
        /// The tool.
        tool: CheckTool,
        /// What is wrong.
        problem: String,
    },
}

/// Looks up the toolchain that `toolchain_file` names, the bytes of the
/// ecosystem's file at the root of the tree or `None` where the tree has
/// none, and each check tool of `tools` (WS-7, WS-7a).
///
/// # Errors
///
/// Every thing that is missing, the toolchain's first and then the check
/// tools' in order of name.
pub fn resolve(
    ecosystem: &dyn Ecosystem,
    host: HostTriple,
    directories: Directories<'_>,
    toolchain_file: Option<&[u8]>,
    tools: &[CheckTool],
) -> Result<HostPaths, Vec<Missing>> {
    let toolchain = match toolchain_file {
        None => Err(vec![Missing::NoToolchainFile {
            file: ecosystem.toolchain_file(),
        }]),
        Some(file) => match ecosystem.toolchain(file, host) {
            Err(error) => Err(vec![Missing::ToolchainFile(error)]),
            Ok(lookup) => find_toolchain(directories.toolchains, &lookup),
        },
    };
    let tools = find_check_tools(directories.tools, tools);
    match (toolchain, tools) {
        (Ok(toolchain), Ok(tools)) => Ok(HostPaths { toolchain, tools }),
        (toolchain, tools) => Err(toolchain
            .err()
            .unwrap_or_default()
            .into_iter()
            .chain(tools.err().unwrap_or_default())
            .collect()),
    }
}

/// Looks for the toolchain of `lookup` below `toolchains`, and follows a
/// symbolic link there, at the toolchain's directory or below it (WS-7).
///
/// # Errors
///
/// The toolchain with each thing wrong with its directory, or each component
/// and target that its record lacks.
fn find_toolchain(
    toolchains: Option<&Path>,
    lookup: &ToolchainLookup,
) -> Result<Toolchain, Vec<Missing>> {
    let missing = |problem: String| Missing::Toolchain {
        toolchain: lookup.name.clone(),
        problem,
    };
    let Some(toolchains) = toolchains else {
        return Err(vec![missing(
            "the operator configuration names no toolchains directory".to_owned(),
        )]);
    };
    let named = toolchains.join(&lookup.directory);
    let directory = fs::canonicalize(&named).map_err(|error| {
        vec![missing(format!(
            "`{}` cannot be found: {error}",
            named.display()
        ))]
    })?;
    if !directory.is_dir() {
        return Err(vec![missing(format!(
            "`{}` is no directory",
            named.display()
        ))]);
    }

    let mut problems: Vec<Missing> = lookup
        .programs
        .iter()
        .filter(|program| !is_executable_within(&directory.join(program), &directory))
        .map(|program| {
            missing(format!(
                "`{program}` is no regular file in the toolchain's directory that the factory's user may execute"
            ))
        })
        .collect();
    match fs::read_to_string(directory.join(&lookup.record)) {
        Err(error) => problems.push(missing(format!(
            "`{}` cannot be read: {error}",
            lookup.record
        ))),
        Ok(record) => {
            let lines: BTreeSet<&str> = record.lines().collect();
            for part in &lookup.parts {
                if part.lines.iter().any(|line| lines.contains(line.as_str())) {
                    continue;
                }
                let toolchain = lookup.name.clone();
                problems.push(match part.kind {
                    PartKind::Toolchain => missing(format!(
                        "`{}` lacks the line `{}`",
                        lookup.record, part.name
                    )),
                    PartKind::Component => Missing::Component {
                        toolchain,
                        component: part.name.clone(),
                    },
                    PartKind::Target => Missing::Target {
                        toolchain,
                        target: part.name.clone(),
                    },
                });
            }
        }
    }
    if problems.is_empty() {
        Ok(Toolchain {
            bin: directory.join(&lookup.bin),
            directory,
        })
    } else {
        Err(problems)
    }
}

/// Tests the directory of each check tool of `tools` below `tools_dir`: a
/// tool is present when `<name>/<version>/bin` exists and holds at least one
/// executable file (WS-7a). A symbolic link there is followed as in the
/// toolchains directory (WS-7).
///
/// # Errors
///
/// Each check tool that is missing, in order of name.
fn find_check_tools(
    tools_dir: Option<&Path>,
    tools: &[CheckTool],
) -> Result<Vec<FoundCheckTool>, Vec<Missing>> {
    let mut tools = tools.to_vec();
    tools.sort();
    let mut found = Vec::new();
    let mut missing = Vec::new();
    for tool in tools {
        match find_check_tool(tools_dir, &tool) {
            Ok((directory, bin)) => found.push(FoundCheckTool {
                tool,
                directory,
                bin,
            }),
            Err(problem) => missing.push(Missing::Tool { tool, problem }),
        }
    }
    if missing.is_empty() {
        Ok(found)
    } else {
        Err(missing)
    }
}

/// The directory and the `bin` of one check tool, or what is wrong.
fn find_check_tool(
    tools_dir: Option<&Path>,
    tool: &CheckTool,
) -> Result<(PathBuf, PathBuf), String> {
    let tools_dir = tools_dir.ok_or("the operator configuration names no tools directory")?;
    let named = tools_dir.join(tool.name()).join(tool.version());
    let directory = fs::canonicalize(&named)
        .map_err(|error| format!("`{}` cannot be found: {error}", named.display()))?;
    let bin = directory.join("bin");
    let entries = fs::read_dir(&bin)
        .map_err(|error| format!("`{}` cannot be read: {error}", bin.display()))?;
    if entries
        .filter_map(Result::ok)
        .any(|entry| is_executable_within(&entry.path(), &directory))
    {
        Ok((directory, bin))
    } else {
        Err(format!(
            "`{}` holds no executable file that lies in `{}`",
            bin.display(),
            directory.display()
        ))
    }
}

/// Whether `path`, after every symbolic link, is a regular file with an
/// execute bit set that lies below `directory`, which is canonical. The
/// sandbox shows `directory` alone, so a link that leads out of it would
/// lead nowhere there (WS-7).
///
/// The mode is read and the user is not: whether the factory's user may
/// execute a file that only its owner or group may execute is not tested
/// here, since that needs the user's ids and std gives none.
fn is_executable_within(path: &Path, directory: &Path) -> bool {
    fs::canonicalize(path).is_ok_and(|real| {
        real.starts_with(directory)
            && fs::metadata(&real).is_ok_and(|metadata| {
                metadata.is_file() && metadata.permissions().mode() & 0o111 != 0
            })
    })
}
