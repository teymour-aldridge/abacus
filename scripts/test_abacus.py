"""Wrapper regression tests: python3 -m unittest discover -s scripts."""

import json
import os
from pathlib import Path
import subprocess
import tempfile
import unittest


WRAPPER = Path(__file__).with_name("abacus").resolve()


class TestAbacus(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.log = self.root / "calls.jsonl"
        for name in ("cargo", "npm"):
            command = self.root / name
            command.write_text(
                "#!/usr/bin/env python3\n"
                "import json, os, sys\n"
                "with open(os.environ['CALL_LOG'], 'a') as log:\n"
                "    log.write(json.dumps(sys.argv) + '\\n')\n"
                "sys.exit(int(os.environ.get('MOCK_EXIT', '0')))\n"
            )
            command.chmod(0o755)
        self.env = {
            **os.environ,
            "PATH": f"{self.root}{os.pathsep}{os.environ['PATH']}",
            "CALL_LOG": str(self.log),
            "MOCK_EXIT": "0",
        }

    def run_wrapper(self, *args):
        # Run without a frontend directory (or even a Cargo project).
        return subprocess.run(
            ["bash", str(WRAPPER), "test", *args],
            cwd=self.root,
            env=self.env,
            capture_output=True,
            text=True,
        )

    def calls(self):
        if not self.log.exists():
            return []
        return [
            [Path(call[0]).name, *call[1:]]
            for call in map(json.loads, self.log.read_text().splitlines())
        ]

    def test_default_runs_only_cargo(self):
        self.assertEqual(self.run_wrapper().returncode, 0)
        self.assertEqual(self.calls(), [["cargo", "test", "--locked", "--all-targets"]])

    def test_rust_only_is_compatible(self):
        self.assertEqual(self.run_wrapper("--rust-only").returncode, 0)
        self.assertEqual(self.calls(), [["cargo", "test", "--locked", "--all-targets"]])

    def test_arguments_are_forwarded(self):
        args = ["--release", "--", "filter with spaces", "--nocapture"]
        self.assertEqual(self.run_wrapper("--", *args).returncode, 0)
        self.assertEqual(
            self.calls(), [["cargo", "test", "--locked", "--all-targets", *args]]
        )

    def test_frontend_only_is_rejected(self):
        result = self.run_wrapper("--frontend-only")
        self.assertEqual(result.returncode, 2)
        self.assertIn("there is no frontend test suite", result.stderr)
        self.assertEqual(self.calls(), [])

    def test_cargo_failure_is_propagated(self):
        self.env["MOCK_EXIT"] = "42"
        self.assertEqual(self.run_wrapper().returncode, 42)
