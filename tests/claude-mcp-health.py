"""Verify Claude Code can connect to KeyHarbor using a temporary configuration."""

import os
from pathlib import Path
import subprocess
import tempfile


binary = os.environ.get("KEYHARBOR_BINARY")
if not binary or not Path(binary).is_file():
    raise SystemExit("Set KEYHARBOR_BINARY to the built KeyHarbor executable")

with tempfile.TemporaryDirectory(prefix="keyharbor-claude-health-") as temporary:
    root = Path(temporary)
    config = root / "config"
    project = root / "project"
    config.mkdir()
    project.mkdir()
    env = os.environ.copy()
    env["CLAUDE_CONFIG_DIR"] = str(config)

    def run(*args):
        result = subprocess.run(
            ["claude", *args], cwd=project, env=env, capture_output=True, text=True, timeout=30
        )
        if result.returncode:
            raise RuntimeError(f"Claude command failed (exit {result.returncode})")
        return result.stdout

    run("mcp", "add", "--scope", "user", "--transport", "stdio", "keyharbor", "--", binary,
        "--data-dir", str(root / "data"), "mcp")
    status = run("mcp", "get", "keyharbor")
    if "Connected" not in status:
        raise RuntimeError("Claude did not report an MCP connection")
    print("PASS: Claude Code MCP health check with isolated configuration")
