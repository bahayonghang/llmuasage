#!/usr/bin/env python3
"""Run the Rust checks shared by local `just ci` and GitHub Actions."""

import os
import subprocess
from typing import Dict, List, Optional


def run(args: List[str], *, env: Optional[Dict[str, str]] = None) -> None:
    print(f"+ {' '.join(args)}", flush=True)
    subprocess.run(args, check=True, env=env)


def main() -> None:
    run(["cargo", "fmt", "--check"])
    run(
        [
            "cargo",
            "clippy",
            "--locked",
            "--all-targets",
            "--all-features",
            "--",
            "-D",
            "warnings",
        ]
    )
    run(
        [
            "cargo",
            "test",
            "--locked",
            "--all-features",
            "--",
            "--test-threads=1",
        ]
    )
    rustdoc_env = os.environ.copy()
    rustdoc_env["RUSTDOCFLAGS"] = "-D warnings"
    run(["cargo", "doc", "--locked", "--no-deps"], env=rustdoc_env)


if __name__ == "__main__":
    main()
