"""Contracts for the repository-wide Python 3.14 quality gateway."""

import re
import tomllib
import unittest
from pathlib import Path

REPOSITORY = Path(__file__).resolve().parents[2]
MAKEFILE = REPOSITORY / "Makefile"
PYPROJECT = REPOSITORY / "pyproject.toml"
SOURCE_ROOTS = {".github", "tests", "scripts", "benches", "benchmarks"}
PRUNED_PARTS = {
    ".git",
    ".mypy_cache",
    ".pytest_cache",
    ".ruff_cache",
    ".uv-cache",
    ".uv-tools",
    ".venv",
    "__pycache__",
    "node_modules",
    "target",
    "vendor",
    "venv",
}
DF12_MESSAGES = {
    "R9101",
    "C9102",
    "R9103",
    "R9104",
    "C9105",
    "C9106",
    "C9107",
    "R9108",
    "R9109",
    "R9110",
    "R9111",
    "R9112",
    "C9112",
}


def read_text(path: Path) -> str:
    """Read a repository file as UTF-8 text."""
    return path.read_text(encoding="utf-8")


def make_value(name: str) -> str:
    """Return a Make assignment after joining continuation lines."""
    assignment = re.compile(rf"{re.escape(name)}\s*[:?]?=\s*(?P<value>.+?)\s*")
    continued = ""
    for physical_line in read_text(MAKEFILE).splitlines():
        line = continued + physical_line.lstrip()
        if line.endswith("\\"):
            continued = line[:-1] + " "
            continue
        continued = ""
        match = assignment.fullmatch(line)
        if match is not None:
            return match["value"]
    raise AssertionError(f"Makefile does not assign {name}")


def python_sources() -> list[Path]:
    """List repository-owned Python while excluding generated directories."""
    found: list[Path] = []
    for directory, subdirectories, files in REPOSITORY.walk():
        subdirectories[:] = [
            name for name in subdirectories if name not in PRUNED_PARTS
        ]
        found.extend(directory / name for name in files if name.endswith(".py"))
    return sorted(found)


class PythonGatewayContract(unittest.TestCase):
    """Keep every Python entrypoint on one strict, reproducible baseline."""

    def test_baseline_agrees_across_make_and_tool_configuration(self) -> None:
        """Make, Ruff, and Pylint must all select CPython 3.14."""
        configuration = tomllib.loads(read_text(PYPROJECT))
        self.assertEqual(
            make_value("PYTHON_BASELINE"),
            "3.14",
            "Make must select CPython 3.14",
        )
        self.assertEqual(
            configuration["tool"]["ruff"]["target-version"],
            "py314",
            "Ruff must parse Python 3.14 syntax",
        )
        self.assertEqual(
            configuration["tool"]["pylint"]["main"]["py-version"],
            "3.14",
            "Pylint must apply the Python 3.14 rule baseline",
        )

    def test_inventory_covers_every_repository_python_file(self) -> None:
        """Every owned Python file must live below a linted source root."""
        for root in SOURCE_ROOTS:
            self.assertIn(
                root,
                make_value("PYTHON_SOURCE_ROOTS"),
                f"Python source inventory omits {root}",
            )
        sources = python_sources()
        self.assertTrue(sources, "the gateway contract must discover itself")
        for source in sources:
            relative = source.relative_to(REPOSITORY)
            self.assertIn(
                relative.parts[0],
                SOURCE_ROOTS,
                f"Python source lies outside the gateway roots: {relative}",
            )

    def test_pylint_defaults_and_all_df12_messages_share_one_process(self) -> None:
        """The combined Pylint invocation must retain both policy layers."""
        pylint_command = make_value("PYLINT")
        configuration = tomllib.loads(read_text(PYPROJECT))
        message_configuration = configuration["tool"]["pylint"].get(
            "messages control", {}
        )
        self.assertNotIn(
            "--disable",
            pylint_command,
            "the Pylint command must retain its default messages",
        )
        self.assertNotIn(
            "disable",
            message_configuration,
            "pyproject.toml must not disable Pylint messages",
        )
        self.assertIn(
            "--load-plugins=df12_python_lints",
            pylint_command,
            "Pylint must load the df12 plugin",
        )
        messages = set(make_value("DF12_PYLINT_MESSAGES").split(","))
        self.assertEqual(
            messages,
            DF12_MESSAGES,
            "the gateway must enable every message from df12-python-lints v0.3.0",
        )

    def test_gateway_tools_and_policy_source_are_pinned(self) -> None:
        """Changing a quality rule must require an explicit repository edit."""
        expected = {
            "RUFF_VERSION": "0.16.4",
            "PYLINT_VERSION": "4.0.9",
            "INTERROGATE_VERSION": "1.7.0",
            "TY_VERSION": "0.0.74",
        }
        for name, value in expected.items():
            self.assertEqual(
                make_value(name),
                value,
                f"{name} must stay pinned until its findings are addressed",
            )
        df12_reference = make_value("DF12_PYTHON_LINTS_REF")
        self.assertIsNotNone(
            re.fullmatch(r"[0-9a-f]{40}", df12_reference),
            "df12-python-lints must be pinned to an immutable full commit",
        )
        self.assertIn(
            "df12-python-lints.git@$(DF12_PYTHON_LINTS_REF)",
            make_value("DF12_PYTHON_LINTS"),
            "the df12 package URL must consume the validated revision",
        )

    def test_public_gates_include_python(self) -> None:
        """Formatting, lint, typecheck, and test must route through Python."""
        makefile = read_text(MAKEFILE)
        required_fragments = {
            "all": "+$(MAKE) typecheck",
            "check-fmt": "$(RUFF) format --check",
            "fmt": "$(RUFF) format",
            "lint": "+$(MAKE) lint-python",
            "test": "test: check-build-tools test-python",
            "test-workflow-contracts": "test-workflow-contracts: test-python",
            "typecheck": "typecheck: check-build-tools typecheck-rust typecheck-python",
        }
        for target, fragment in required_fragments.items():
            self.assertIn(
                fragment,
                makefile,
                f"make {target} does not include its Python gateway",
            )

    def test_lint_gateway_invokes_every_required_tool(self) -> None:
        """Lint must run Ruff, Pylint, df12, ambrleaks, and Interrogate."""
        makefile = read_text(MAKEFILE)
        self.assertIn(
            "set -e;",
            makefile,
            "lint-python must stop when any gateway tool reports a finding",
        )
        for invocation in (
            "$(RUFF) check $(PYTHON_SOURCES)",
            "$(PYLINT) $(PYTHON_SOURCES)",
            "$(AMBRLEAKS) $(PYTHON_SOURCES)",
            "$(INTERROGATE) $(PYTHON_SOURCES)",
        ):
            self.assertIn(invocation, makefile, f"lint gateway omits {invocation}")

    def test_python_commands_request_managed_baseline(self) -> None:
        """Each tool must run on uv's managed baseline interpreter."""
        makefile = read_text(MAKEFILE)
        self.assertGreaterEqual(
            makefile.count("--managed-python"),
            7,
            "every Python tool route must request a managed interpreter",
        )
        self.assertGreaterEqual(
            makefile.count("--python $(PYTHON_BASELINE)"),
            7,
            "every Python tool route must request the shared baseline",
        )

    def test_python_sources_contain_no_inline_suppressions(self) -> None:
        """Findings must be fixed in code rather than hidden at call sites."""
        forbidden = (
            "# no" + "qa",
            "# pylint:" + " disable",
            "# type:" + " ignore",
            "# ty:" + " ignore",
        )
        for source in python_sources():
            text = read_text(source)
            for marker in forbidden:
                self.assertNotIn(
                    marker,
                    text,
                    f"{source.relative_to(REPOSITORY)} contains {marker}",
                )

    def test_workflows_pin_python_314(self) -> None:
        """Workflow Python setup must not float away from the local baseline."""
        for relative in (".github/workflows/ci.yml", ".github/workflows/audit.yml"):
            workflow = read_text(REPOSITORY / relative)
            self.assertIn(
                "python-version: '3.14'",
                workflow,
                f"{relative} must install Python 3.14",
            )
        self.assertIn(
            "run: make typecheck",
            read_text(REPOSITORY / ".github/workflows/ci.yml"),
            "pull-request CI must run the combined typecheck gateway",
        )


if __name__ == "__main__":
    unittest.main()
