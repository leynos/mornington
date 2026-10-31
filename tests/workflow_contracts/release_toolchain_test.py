"""The release workflow takes its toolchain from ``rust-toolchain.toml``.

Dependabot bumps the pinned nightly in ``rust-toolchain.toml`` only. A release
build that restated the date would lag the manifest after every bump, so the
workflow reads the channel once and both ``setup-rust`` and ``cross`` use it.
The checks read properties of the text (a dated nightly anywhere, an expression
in a given position), not exact lines, so a reformat of the workflow changes
nothing.
"""

import re
import subprocess
import sys
import tempfile
import textwrap
import tomllib
import unittest
from pathlib import Path

REPOSITORY = Path(__file__).resolve().parents[2]
RELEASE = (REPOSITORY / ".github" / "workflows" / "release.yml").read_text(
    encoding="utf-8"
)
TOOLCHAIN = REPOSITORY / "rust-toolchain.toml"
CHANNEL = r"\$\{\{\s*steps\.toolchain\.outputs\.channel\s*\}\}"
DATED_NIGHTLY = re.compile(r"nightly-\d{4}-\d{2}-\d{2}")


RUN_HEADER = re.compile(r"^(?P<indent>[ ]*)run:[ ]*\|[-+]?[ ]*$", re.MULTILINE)


def read_step_script() -> str:
    """Return the shell script of the step that reads the manifest channel."""
    start = RELEASE.find("id: toolchain")
    header = RUN_HEADER.search(RELEASE, start)
    if start < 0 or header is None:
        raise AssertionError("release.yml has no toolchain read step")
    deeper = header.group("indent") + " "
    body = []
    for line in RELEASE[header.end() :].lstrip("\n").splitlines():
        if line.strip() and not line.startswith(deeper):
            break
        body.append(line)
    return textwrap.dedent("\n".join(body)) + "\n"


def run_reader(manifest: str | None) -> tuple[int, str]:
    """Run the workflow's reader on a manifest; return its status and output."""
    with tempfile.TemporaryDirectory() as directory:
        root = Path(directory)
        if manifest is not None:
            (root / "rust-toolchain.toml").write_text(manifest, encoding="utf-8")
        bin_dir = root / "bin"
        bin_dir.mkdir()
        (bin_dir / "python3").symlink_to(sys.executable)
        output = root / "github-output"
        output.touch()
        env = {"PATH": f"{bin_dir}:/usr/bin:/bin", "GITHUB_OUTPUT": str(output)}
        done = subprocess.run(
            ["/bin/bash", "-c", read_step_script()],
            cwd=root,
            env=env,
            capture_output=True,
            text=True,
            check=False,
            timeout=60,
        )
        return done.returncode, output.read_text(encoding="utf-8")


class ReleaseToolchain(unittest.TestCase):
    """Hold the release build to the manifest's toolchain."""

    def test_the_workflow_names_no_dated_nightly(self):
        """A date anywhere in the workflow would lag the manifest after a bump."""
        found = DATED_NIGHTLY.findall(RELEASE)
        self.assertEqual(found, [], f"release.yml restates a dated nightly: {found}")

    def test_a_step_reads_the_manifest_channel_before_setup(self):
        """The channel is read from the manifest, ahead of setup-rust."""
        read = re.search(
            r"id:\s*toolchain\b.*?rust-toolchain\.toml", RELEASE, re.DOTALL
        )
        setup = RELEASE.find("setup-rust@")
        if read is None:
            self.fail("release.yml must read rust-toolchain.toml")
        self.assertLess(read.start(), setup, "the read must precede setup-rust")

    def test_setup_rust_uses_the_channel(self):
        """The setup-rust toolchain input is the manifest channel."""
        pattern = rf"setup-rust@[^\n]*\n(?:[^\n]*\n)*?\s*toolchain:\s*{CHANNEL}"
        self.assertRegex(RELEASE, pattern, "setup-rust must use the read channel")

    def test_cross_uses_the_channel(self):
        """The cross build selects the channel the manifest names."""
        self.assertRegex(
            RELEASE, rf"cross\s+\+{CHANNEL}\s+build", "cross must use the channel"
        )

    def test_the_reader_writes_the_channel_of_each_manifest(self):
        """Two distinct channels give two distinct outputs."""
        for channel in ("nightly-2030-01-02", "nightly-2031-03-04"):
            with self.subTest(channel=channel):
                status, output = run_reader(f'[toolchain]\nchannel = "{channel}"\n')
                self.assertEqual(status, 0, "the reader must succeed")
                self.assertEqual(output, f"channel={channel}\n")

    def test_the_reader_fails_on_a_missing_or_malformed_channel(self):
        """No channel means no output, and the step fails the job."""
        cases = {
            "no manifest": None,
            "no channel": "[toolchain]\ncomponents = []\n",
            "malformed": "[toolchain\nchannel = 1\n",
        }
        for label, manifest in cases.items():
            with self.subTest(case=label):
                status, output = run_reader(manifest)
                self.assertNotEqual(status, 0, "the reader must fail")
                self.assertEqual(output, "", "a failing read must write no output")

    def test_the_read_precedes_the_cross_build(self):
        """The cross command runs after the channel is read."""
        read = RELEASE.find("id: toolchain")
        cross = RELEASE.find("cross +")
        self.assertTrue(0 <= read < cross, "the read must precede the cross build")

    def test_the_manifest_channel_is_a_dated_nightly(self):
        """The value the workflow reads is the pin Dependabot moves."""
        manifest = tomllib.loads(TOOLCHAIN.read_text(encoding="utf-8"))
        self.assertRegex(manifest["toolchain"]["channel"], DATED_NIGHTLY)


if __name__ == "__main__":
    unittest.main()
