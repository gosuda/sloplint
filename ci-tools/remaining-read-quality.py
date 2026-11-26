from __future__ import annotations

import hashlib
import io
import json
import os
import subprocess
import zipfile
from pathlib import Path


def main() -> None:
    evidence = Path(os.environ["GITHUB_WORKSPACE"]) / "evidence"
    evidence.mkdir(parents=True, exist_ok=True)
    archive = subprocess.check_output(["gh", "api", "/repos/gosuda/sloplint/actions/artifacts/15110946601/zip"], timeout=120)
    assert hashlib.sha256(archive).hexdigest() == "b76520c59921825d1bb200bf1928183ec02a620a793a668b820fe88c6ba9b990"
    with zipfile.ZipFile(io.BytesIO(archive)) as zipped:
        quality = zipped.read("quality.log").decode()
        identity = json.loads(zipped.read("identity.json"))
    rows = quality.splitlines()
    metrics = sorted(row for row in rows if row.startswith("corpus/") and " ours=" in row)
    summary = {"head": identity["candidate"]["head"], "tree": identity["candidate"]["tree"], "fixtures": len(metrics), "exceptions": sum("exception=true" in row for row in metrics), "metrics_sha256": hashlib.sha256(("\n".join(metrics) + "\n").encode()).hexdigest(), "gates": [row for row in rows if row.startswith("gate ")], "diagnostics": [row for row in rows if "differs from computed" in row]}
    (evidence / "candidate-quality-summary.json").write_text(json.dumps(summary, indent=2) + "\n")
    print("QUALITY_SUMMARY " + json.dumps(summary), flush=True)


if __name__ == "__main__":
    main()
