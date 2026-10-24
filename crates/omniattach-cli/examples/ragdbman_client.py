"""Minimal subprocess integration; the actual facade-enabled omniattach binary must exist."""
import json
import subprocess
from pathlib import Path


def convert(source, output, goal="extract", binary="omniattach"):
    process = subprocess.run(
        [binary, "convert", str(source), "--goal", goal, "--output", str(output)],
        capture_output=True, text=True, check=False,
    )
    if process.returncode:
        records = [json.loads(line) for line in process.stderr.splitlines() if line.strip()]
        raise RuntimeError(next((record for record in reversed(records) if "error" in record), records))
    result = json.loads(process.stdout)
    root = Path(result["output_dir"])
    return result["manifest"], root


if __name__ == "__main__":
    import sys
    manifest, output = convert(sys.argv[1], sys.argv[2], sys.argv[3] if len(sys.argv) > 3 else "extract")
    print(json.dumps({"output_dir": str(output), "manifest": manifest}, ensure_ascii=False))
