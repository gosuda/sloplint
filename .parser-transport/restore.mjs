import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { readFileSync, writeFileSync } from "node:fs";
import { gunzipSync } from "node:zlib";

const files = [
  {
    "id": "cpp",
    "path": "vendor/tree-sitter-cpp/src/parser.c",
    "size": 17990766,
    "git_blob_sha": "28665d3f39c95cd18a41e1b988532ce89f84b830",
    "sha256": "b76c4adc9ed73ff225d57bee19ccddee357e339ba29fe8ac97e6b677fb030bf7",
    "payload_size": 1318300
  },
  {
    "id": "kotlin",
    "path": "vendor/tree-sitter-kotlin-ng/src/parser.c",
    "size": 25270889,
    "git_blob_sha": "6340b7e2856e923f016e54239f49261d3bdd3285",
    "sha256": "4b1912b7c1bd0fcbcc55e6700f9d40dec4e61f7e278b95178b38ae56c786547c",
    "payload_size": 1631072
  }
];
for (const file of files) {
  const encoded = readFileSync(`.parser-transport/${file.id}.gz.b64`, "ascii");
  assert.equal(encoded.length, file.payload_size);
  const bytes = gunzipSync(Buffer.from(encoded, "base64"), { maxOutputLength: file.size });
  assert.equal(bytes.length, file.size);
  assert.equal(createHash("sha256").update(bytes).digest("hex"), file.sha256);
  const blob = createHash("sha1").update(Buffer.from(`blob ${bytes.length}\0`)).update(bytes).digest("hex");
  assert.equal(blob, file.git_blob_sha);
  writeFileSync(file.path, bytes);
  process.stdout.write(`${file.path}: verified ${blob}\n`);
}
