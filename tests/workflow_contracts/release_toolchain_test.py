"""The release workflow takes its toolchain from ``rust-toolchain.toml``.

Dependabot bumps the pinned nightly in ``rust-toolchain.toml`` only. A release
build that restated the date would lag the manifest after every bump, so the
workflow reads the channel once and both ``setup-rust`` and ``cross`` use it.
"""

import re
import tomllib
import unittest
from pathlib import Path

import yaml

REPOSITORY = Path(__file__).resolve().parents[2]
RELEASE = REPOSITORY / ".github" / "workflows" / "release.yml"
TOOLCHAIN = REPOSITORY / "rust-toolchain.toml"
READ_STEP = "Read the pinned toolchain"
CHANNEL_EXPRESSION = "${{ steps.toolchain.outputs.channel }}"
DATED_NIGHTLY = re.compile(r"nightly-\d{4}-\d{2}-\d{2}")


def build_steps() -> list[dict]:
    """Return the release build job's steps."""
    document = yaml.safe_load(RELEASE.read_text(encoding="utf-8"))
    return document["jobs"]["build"]["steps"]


class ReleaseToolchain(unittest.TestCase):
    """Hold the release build to the manifest's toolchain."""

    def test_no_step_restates_a_dated_nightly(self):
        """A date in any step would lag the manifest after a Dependabot bump."""
        for step in build_steps():
            with self.subTest(step=step.get("name") or step.get("uses")):
                text = repr(step)
                self.assertIsNone(
                    DATED_NIGHTLY.search(text),
                    f"a release step names a dated nightly: {text}",
                )

    def test_the_manifest_is_read_before_the_toolchain_is_used(self):
        """The channel step precedes setup-rust and reads the manifest."""
        steps = build_steps()
        names = [step.get("name") for step in steps]
        self.assertIn(READ_STEP, names, "the release build must read the manifest")
        read = steps[names.index(READ_STEP)]
        self.assertEqual(read.get("id"), "toolchain")
        self.assertIn("rust-toolchain.toml", read["run"])
        setup = next(s for s in steps if "setup-rust@" in str(s.get("uses", "")))
        self.assertLess(steps.index(read), steps.index(setup))
        self.assertEqual(setup["with"]["toolchain"], CHANNEL_EXPRESSION)

    def test_cross_uses_the_same_channel(self):
        """The cross build selects the channel the manifest names."""
        steps = build_steps()
        build = next(s for s in steps if s.get("name") == "Build release binary")
        self.assertIn(f"cross +{CHANNEL_EXPRESSION} build", build["run"])

    def test_the_manifest_channel_is_a_dated_nightly(self):
        """The value the workflow reads is the pin Dependabot moves."""
        manifest = tomllib.loads(TOOLCHAIN.read_text(encoding="utf-8"))
        channel = manifest["toolchain"]["channel"]
        self.assertRegex(channel, DATED_NIGHTLY)


if __name__ == "__main__":
    unittest.main()
