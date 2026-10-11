//! Capability-owned fixtures for subprocess tests of the actual build-tool scripts.

use std::process::{Command, Output};

use cap_std::{
    ambient_authority,
    fs_utf8::{
        Dir,
        Permissions,
        PermissionsExt,
        camino::{Utf8Path, Utf8PathBuf},
    },
};

/// The two build-tool entrypoints copied into the hermetic repository.
#[derive(Clone, Copy, Debug)]
pub enum ScriptName {
    /// Installs the pinned Rust components and verified linker archive.
    Install,
    /// Checks toolchain, development linker, and optional coverage prerequisites.
    Check,
}

impl ScriptName {
    /// Resolves the entrypoint without permitting arbitrary executable paths.
    const fn filename(self) -> &'static str {
        match self {
            Self::Install => "install-build-tools.sh",
            Self::Check => "check-build-tools.sh",
        }
    }
}

/// The shims whose absence has meaning in the prerequisite contracts.
#[derive(Clone, Copy, Debug)]
pub enum ToolName {
    /// Linux compiler driver used by the configured linker route.
    Clang,
    /// The project-pinned development linker.
    PinnedLinker,
    /// The additional LLVM coverage linker.
    Lld,
    /// Download command that must be skipped on non-Linux hosts.
    Curl,
    /// Archive extraction command that must be skipped on non-Linux hosts.
    Tar,
}

impl ToolName {
    /// Resolves a removable shim without accepting filesystem traversal.
    const fn executable(self) -> &'static str {
        match self {
            Self::Clang => "clang",
            Self::PinnedLinker => "mold",
            Self::Lld => "ld.lld",
            Self::Curl => "curl",
            Self::Tar => "tar",
        }
    }
}

/// A minimal copied repository and isolated tool search path, removed by RAII.
pub struct ScriptFixture {
    /// Capability used for all fixture reads and writes.
    directory: Dir,
    /// Absolute path supplied only to subprocesses.
    path: Utf8PathBuf,
    /// Owns allocation and cleanup without clock-derived names.
    _owner: tempfile::TempDir,
}

impl ScriptFixture {
    /// Copies the three real scripts and installs controlled tool shims.
    ///
    /// For example, `ScriptFixture::new()?.run(ScriptName::Check, &[], &[])`
    /// checks the fixture toolchain without consulting the host's tools.
    pub fn new() -> Result<Self, String> {
        let owner = tempfile::Builder::new()
            .prefix("mornington-build-tools-")
            .tempdir()
            .map_err(|error| format!("allocating build-tool fixture: {error}"))?;
        let path = Utf8PathBuf::from_path_buf(owner.path().to_path_buf())
            .map_err(|value| format!("fixture path {} is not UTF-8", value.display()))?;
        let directory = Dir::open_ambient_dir(&path, ambient_authority())
            .map_err(|error| format!("opening fixture {path}: {error}"))?;
        let fixture = Self {
            directory,
            path,
            _owner: owner,
        };
        fixture.populate()?;
        Ok(fixture)
    }

    /// Builds the minimal repository and an explicit allowlist of inert tools.
    fn populate(&self) -> Result<(), String> {
        for name in ["scripts", "tools/mold", "bin", "payload/bin", "scratch"] {
            self.directory
                .create_dir_all(name)
                .map_err(|error| format!("creating fixture {name}: {error}"))?;
        }
        self.copy_sources()?;
        self.install_shims()?;
        self.write(Utf8Path::new("commands.log"), "")?;
        self.write(Utf8Path::new("tools/mold/VERSION"), "2.41.0\n")?;
        self.write(
            Utf8Path::new("payload/bin/mold"),
            "verified fixture linker artefact\n",
        )?;
        self.create_archive()
    }

    /// Copies source scripts and the toolchain instead of reimplementing their logic.
    fn copy_sources(&self) -> Result<(), String> {
        let source = Dir::open_ambient_dir(env!("CARGO_MANIFEST_DIR"), ambient_authority())
            .map_err(|error| format!("opening source repository: {error}"))?;
        for name in [
            "scripts/install-build-tools.sh",
            "scripts/check-build-tools.sh",
            "scripts/build-tools-common.sh",
            "rust-toolchain.toml",
        ] {
            let contents = source
                .read_to_string(name)
                .map_err(|error| format!("reading source {name}: {error}"))?;
            self.write(Utf8Path::new(name), &contents)?;
        }
        Ok(())
    }

    /// Exposes only shims and explicitly selected ordinary Unix utilities.
    fn install_shims(&self) -> Result<(), String> {
        for (name, contents) in [
            ("rustup", include_str!("../fixtures/build_tools/rustup.sh")),
            ("uname", include_str!("../fixtures/build_tools/uname.sh")),
            ("curl", include_str!("../fixtures/build_tools/curl.sh")),
            ("tar", include_str!("../fixtures/build_tools/tar.sh")),
            ("clang", "#!/bin/bash\nexit 0\n"),
            ("ld.lld", "#!/bin/bash\nexit 0\n"),
            (
                "mold",
                "#!/bin/bash\nprintf 'mold %s\\n' \"$LINKER_VERSION\"\n",
            ),
        ] {
            let executable = format!("bin/{name}");
            self.write(Utf8Path::new(&executable), contents)?;
            self.directory
                .set_permissions(&executable, Permissions::from_mode(0o755))
                .map_err(|error| format!("making {name} executable: {error}"))?;
        }
        for name in [
            "dirname",
            "sed",
            "grep",
            "tr",
            "awk",
            "sha256sum",
            "mktemp",
            "rm",
            "mkdir",
            "gzip",
        ] {
            self.directory
                .symlink_contents(format!("/usr/bin/{name}"), format!("bin/{name}"))
                .map_err(|error| format!("linking fixture utility {name}: {error}"))?;
        }
        Ok(())
    }

    /// Creates a real local archive and records its actual checksum.
    fn create_archive(&self) -> Result<(), String> {
        let status = Command::new("/usr/bin/tar")
            .args(["--create", "--gzip", "--file", "archive.tar.gz", "payload"])
            .current_dir(&self.path)
            .env_clear()
            .env("PATH", "/usr/bin:/bin")
            .status()
            .map_err(|error| format!("creating fixture archive: {error}"))?;
        if !status.success() {
            return Err(format!("fixture archive creation failed: {status}"));
        }
        let output = Command::new("/usr/bin/sha256sum")
            .arg(self.path.join("archive.tar.gz"))
            .env_clear()
            .output()
            .map_err(|error| format!("hashing fixture archive: {error}"))?;
        if !output.status.success() {
            return Err("fixture checksum calculation failed".to_owned());
        }
        let hash_output = String::from_utf8(output.stdout)
            .map_err(|error| format!("decoding fixture checksum: {error}"))?;
        let hash = hash_output
            .split_whitespace()
            .next()
            .ok_or_else(|| "fixture checksum output was empty".to_owned())?;
        self.write(
            Utf8Path::new("tools/mold/SHA256SUMS"),
            &format!(
                "{hash}  mold-2.41.0-x86_64-linux.tar.gz\n{hash}  \
                 mold-2.41.0-aarch64-linux.tar.gz\n"
            ),
        )
    }

    /// Executes an actual copied script with a child-only controlled environment.
    ///
    /// Example: `fixture.run(ScriptName::Check, &["--coverage"], &[])`.
    pub fn run(
        &self,
        script: ScriptName,
        arguments: &[&str],
        overrides: &[(&str, &str)],
    ) -> Result<Output, String> {
        Command::new("/bin/bash")
            .arg(self.path.join("scripts").join(script.filename()))
            .args(arguments)
            .current_dir(&self.path)
            .env_clear()
            .env("PATH", self.path.join("bin"))
            .env("HOME", &self.path)
            .env("TMPDIR", self.path.join("scratch"))
            .env("BUILD_TOOLS_PREFIX", self.path.join("prefix"))
            .env("COMMAND_LOG", self.path.join("commands.log"))
            .env("FIXTURE_ARCHIVE", self.path.join("archive.tar.gz"))
            .env("HOST_OS", "Linux")
            .env("HOST_ARCH", "x86_64")
            .env("LINKER_VERSION", "2.41.0")
            .envs(overrides.iter().copied())
            .output()
            .map_err(|error| format!("running fixture {script:?}: {error}"))
    }

    /// Reads fixture output through the owned capability.
    ///
    /// Example: `fixture.read(Utf8Path::new("commands.log"))` returns recorded shim calls.
    pub fn read(&self, name: &Utf8Path) -> Result<String, String> {
        self.directory
            .read_to_string(name)
            .map_err(|error| format!("reading fixture {name}: {error}"))
    }

    /// Replaces one fixture input without touching the source repository.
    ///
    /// Example: `fixture.write(Utf8Path::new("tools/mold/VERSION"), "2.41.0\n")`.
    pub fn write(&self, name: &Utf8Path, contents: &str) -> Result<(), String> {
        self.directory
            .write(name, contents)
            .map_err(|error| format!("writing fixture {name}: {error}"))
    }

    /// Removes a shim to exercise `command -v` without a host-PATH fallback.
    ///
    /// Example: `fixture.remove_tool(ToolName::Lld)` makes coverage refuse the route.
    pub fn remove_tool(&self, tool: ToolName) -> Result<(), String> {
        self.directory
            .remove_file(Utf8Path::new("bin").join(tool.executable()))
            .map_err(|error| format!("removing fixture tool {tool:?}: {error}"))
    }
}
