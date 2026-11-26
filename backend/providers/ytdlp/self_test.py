from __future__ import annotations

import contextlib
import importlib.metadata
import json
import os
import shutil
import subprocess
import tempfile
from pathlib import Path
from typing import Any
from unittest.mock import patch

from providers.ytdlp import options, runtime


def _solver_command(mode: str, runtime_path: str, script_path: str) -> list[str]:
    if mode.endswith("quickjs"):
        return [runtime_path, "--script", script_path]
    if mode.endswith("deno"):
        return [runtime_path, "run", "--no-prompt", script_path]
    if mode.endswith("bun"):
        return [runtime_path, "run", script_path]
    return [runtime_path, script_path]


def _run_solver_fixture(mode: str, runtime_path: str) -> str:
    import yt_dlp_ejs.yt.solver as solver

    lib_js = solver.lib()
    core_js = solver.core()
    if not lib_js or not core_js:
        raise RuntimeError("yt_dlp_ejs solver scripts are empty")

    program = (
        f"{lib_js}\n"
        "Object.assign(globalThis, lib);\n"
        f"{core_js}\n"
        "console.log(JSON.stringify(jsc({\n"
        '    type: "preprocessed",\n'
        '    preprocessed_player: "_result.n = (x) => x + \'_ok\';",\n'
        '    requests: [{ type: "n", challenges: ["abc123"] }]\n'
        "})));\n"
    )

    with tempfile.NamedTemporaryFile("w", suffix=".js", delete=False, encoding="utf-8") as handle:
        handle.write(program)
        script_path = handle.name

    try:
        proc = subprocess.run(
            _solver_command(mode, runtime_path, script_path),
            capture_output=True,
            text=True,
            timeout=15,
            check=False,
        )
    finally:
        with contextlib.suppress(OSError):
            os.unlink(script_path)

    if proc.returncode != 0:
        err = (proc.stderr or proc.stdout).strip()
        raise RuntimeError(f"JS solver execution failed ({mode}): {err}")

    output = proc.stdout.strip().splitlines()
    if not output:
        raise RuntimeError(f"JS solver returned empty output ({mode})")

    parsed = json.loads(output[-1])
    responses = parsed.get("responses") or []
    data = responses[0].get("data") if responses else None
    if not isinstance(data, dict) or data.get("abc123") != "abc123_ok":
        raise RuntimeError(f"Unexpected JS solver output ({mode}): {parsed}")
    return data["abc123"]


def run_self_test(data_dir: str | Path | None = None) -> dict[str, Any]:
    """
    Runs offline deterministic self-checks on embedded yt-dlp, yt-dlp-ejs,
    and the JS challenge runtime without requiring a yt-dlp binary on PATH.
    """
    import yt_dlp
    import yt_dlp_ejs

    options.apply_hermetic_globals()

    ytdlp_version = yt_dlp.version.__version__
    try:
        ejs_version = importlib.metadata.version("yt-dlp-ejs")
    except importlib.metadata.PackageNotFoundError:
        ejs_version = str(getattr(yt_dlp_ejs, "version", "unknown"))

    _, mode, runtime_path = runtime.detect_runtime(data_dir)
    if mode == "none" or not runtime_path:
        raise RuntimeError("No JS runtime detected (expected bundled QuickJS or deno/node)")

    solver_result = _run_solver_fixture(mode, runtime_path)

    # Confirm YoutubeDL constructs hermetically even when yt-dlp is absent from PATH
    orig_which = shutil.which

    def _which_without_ytdlp(cmd: str, *args: Any, **kwargs: Any) -> str | None:
        if cmd == "yt-dlp" or cmd == "yt-dlp.exe":
            return None
        return orig_which(cmd, *args, **kwargs)

    with patch("shutil.which", side_effect=_which_without_ytdlp):
        params = options.build_params(data_dir=str(data_dir) if data_dir else None)
        _ = yt_dlp.YoutubeDL(params)

    return {
        "ytdlp_version": ytdlp_version,
        "ejs_version": ejs_version,
        "js_runtime": mode,
        "js_runtime_path": runtime_path,
        "solver_output": solver_result,
        "ytdlp_binary_needed": False,
    }

