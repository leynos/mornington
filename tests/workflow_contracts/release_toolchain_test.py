"""The release workflow takes its toolchain from ``rust-toolchain.toml``.

Dependabot bumps the pinned nightly in ``rust-toolchain.toml`` only. A release
build that restated the date would lag the manifest after every bump, so the
workflow reads the channel once and both ``setup-rust`` and ``cross`` use it.
The checks read properties of the text (a dated nightly anywhere, an expression
in a given position), not exact lines, so a reformat of the workflow changes
nothing.
"""

import re
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
        self.assertIsNotNone(read, "release.yml must read rust-toolchain.toml")
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

    def test_the_manifest_channel_is_a_dated_nightly(self):
        """The value the workflow reads is the pin Dependabot moves."""
        manifest = tomllib.loads(TOOLCHAIN.read_text(encoding="utf-8"))
        self.assertRegex(manifest["toolchain"]["channel"], DATED_NIGHTLY)


if __name__ == "__main__":
    unittest.main()
