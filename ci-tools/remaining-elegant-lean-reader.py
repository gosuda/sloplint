from __future__ import annotations

import hashlib
import io
import json
import os
from pathlib import Path
import re
from typing import BinaryIO
from email.message import Message
from urllib.error import HTTPError
from urllib.parse import urljoin, urlsplit
from urllib.request import HTTPRedirectHandler, Request, build_opener, urlopen
from zipfile import ZipFile

ARTIFACT_ID = 15119192886
EXPECTED_SHA256 = "47e8353fcd8e2ca7875fb06c9c469ae4e42b39a760849ada877f7bbf0e8dc7e4"


class NoRedirect(HTTPRedirectHandler):
    def redirect_request(self, req: Request, fp: BinaryIO, code: int, msg: str, headers: Message, newurl: str) -> None:
        return None


def main() -> None:
    url = f"https://api.github.com/repos/gosuda/sloplint/actions/artifacts/{ARTIFACT_ID}/zip"
    request = Request(url, headers={"Authorization": f"Bearer {os.environ['GH_TOKEN']}", "Accept": "application/vnd.github+json"})
    try:
        with build_opener(NoRedirect()).open(request, timeout=30) as response:
            payload = response.read()
    except HTTPError as redirect:
        assert redirect.code in (302, 303, 307, 308), redirect.code
        target = urljoin(url, redirect.headers["Location"])
        assert urlsplit(target).scheme == "https"
        # The temporary artifact URL is fetched without the GitHub credential.
        with urlopen(target, timeout=60) as response:
            payload = response.read()
    assert hashlib.sha256(payload).hexdigest() == EXPECTED_SHA256
    output = Path(os.environ["GITHUB_WORKSPACE"]) / "evidence"
    output.mkdir(exist_ok=True)
    with ZipFile(io.BytesIO(payload)) as archive:
        names = [name for name in archive.namelist() if name.rsplit("/", 1)[-1] == "verify.log"]
        assert len(names) == 1, names
        raw = archive.read(names[0]).decode("utf-8")
        rows = re.sub(r"\x1b\[[0-9;]*m", "", raw).splitlines()
        selected: set[int] = set()
        for index, row in enumerate(rows):
            if row.startswith("error: ") or row.startswith("Some required targets"):
                selected.update(range(max(0, index - 1), min(len(rows), index + 12)))
        excerpt = "\n".join(f"{index + 1}: {rows[index]}" for index in sorted(selected))
        (output / "lean-failure-excerpts.txt").write_text(excerpt + "\n")
        identity = {"artifact_id": ARTIFACT_ID, "artifact_sha256": EXPECTED_SHA256, "member": names[0], "log_sha256": hashlib.sha256(archive.read(names[0])).hexdigest(), "matching_lines": len(selected)}
        (output / "reader-identity.json").write_text(json.dumps(identity, indent=2) + "\n")
        print("READER_IDENTITY " + json.dumps(identity), flush=True)
        print(excerpt[:18000], flush=True)


if __name__ == "__main__":
    main()
