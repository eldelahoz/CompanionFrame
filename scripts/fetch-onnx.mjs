// Downloads onnxruntime.dll and the OCR models into src-tauri/resources (bundled by tauri.conf.json),
// verifying pinned SHA-256s. Nothing binary is committed; re-runs are no-ops once the files match.
import { execFileSync } from "node:child_process";
import { createHash } from "node:crypto";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";

const root = path.join(path.dirname(fileURLToPath(import.meta.url)), "..");
const res = path.join(root, "src-tauri", "resources");

// Must satisfy the `api-NN` feature of the `ort` crate in Cargo.toml (ort rc.15 -> ORT 1.31).
const ORT = "1.31.0";
const ORT_ZIP_SHA = "3958d8a44984160692c1b4ffd0de3cb48f8c39f9d34bb31e433db06e30b82195";
const WFH = "06fa689715b0488eb6345b3b1649425cd5a268c8"; // WFHelper main when the models were pinned
// RapidOCR's ONNX exports of PaddleOCR (Apache-2.0), release tag v3.4.0: the multilingual Latin recognizer.
const RAPID = "https://www.modelscope.cn/models/RapidAI/RapidOCR/resolve/v3.4.0";

const files = [
  {
    out: "onnxruntime/onnxruntime.dll",
    sha: "0c42dc4e278f0968fdf9ebf1b1e54a74c657a9942e70803e453c91993451cf4c",
    zip: { url: `https://github.com/microsoft/onnxruntime/releases/download/v${ORT}/onnxruntime-win-x64-${ORT}.zip`, sha: ORT_ZIP_SHA, entry: `onnxruntime-win-x64-${ORT}/lib/onnxruntime.dll` },
  },
  ...[
    ["paddle/ch_PP-OCRv3_rec_infer.onnx", "897a3ededb38fee0dae2c1ccee38241f37df202c9509e3abca02e9217c5ee615"],
    ["paddle/ch_dict.txt", "c084fa990ecf0ce7fcb6bd5a3b689382645ec454e6f6af712b3d6be0addfdaf5"],
  ].map(([f, sha]) => ({
    out: `riven-ocr/${f}`,
    sha,
    url: `https://raw.githubusercontent.com/WFHelper/WFHelper/${WFH}/resources/riven-ocr/${f}`,
  })),
  {
    // PP-OCRv5 mobile text detector (DBNet): finds the riven stat lines.
    out: "riven-ocr/paddle/ch_PP-OCRv5_mobile_det.onnx",
    sha: "4d97c44a20d30a81aad087d6a396b08f786c4635742afc391f6621f5c6ae78ae",
    url: `${RAPID}/onnx/PP-OCRv5/det/ch_PP-OCRv5_mobile_det.onnx`,
  },
  {
    out: "riven-ocr/paddle/latin_PP-OCRv5_rec_mobile_infer.onnx",
    sha: "b20bd37c168a570f583afbc8cd7925603890efbcdc000a59e22c269d160b5f5a",
    url: `${RAPID}/onnx/PP-OCRv5/rec/latin_PP-OCRv5_rec_mobile_infer.onnx`,
  },
  {
    out: "riven-ocr/paddle/ppocrv5_latin_dict.txt",
    sha: "3c0a8a79b612653c25f765271714f71281e4e955962c153e272b7b8c1d2b13ff",
    url: `${RAPID}/paddle/PP-OCRv5/rec/latin_PP-OCRv5_rec_mobile_infer/ppocrv5_latin_dict.txt`,
  },
];

const sha256 = (buf) => createHash("sha256").update(buf).digest("hex");
const download = async (url, sha) => {
  const r = await fetch(url);
  if (!r.ok) throw new Error(`${url}: HTTP ${r.status}`);
  const buf = Buffer.from(await r.arrayBuffer());
  if (sha256(buf) !== sha) throw new Error(`${url}: SHA-256 mismatch`);
  return buf;
};

for (const f of files) {
  const dest = path.join(res, f.out);
  if (fs.existsSync(dest) && sha256(fs.readFileSync(dest)) === f.sha) continue;
  fs.mkdirSync(path.dirname(dest), { recursive: true });
  if (f.zip) {
    const tmp = fs.mkdtempSync(path.join(os.tmpdir(), "ort-"));
    try {
      const zip = path.join(tmp, "ort.zip");
      fs.writeFileSync(zip, await download(f.zip.url, f.zip.sha));
      // System32 bsdtar reads zips (a GNU tar earlier on PATH does not).
      execFileSync(path.join(process.env.SystemRoot ?? "C:\\Windows", "System32", "tar.exe"), ["-xf", zip, "-C", tmp, f.zip.entry]);
      fs.copyFileSync(path.join(tmp, f.zip.entry), dest);
    } finally {
      fs.rmSync(tmp, { recursive: true, force: true });
    }
  } else {
    fs.writeFileSync(dest, await download(f.url, f.sha));
  }
  if (sha256(fs.readFileSync(dest)) !== f.sha) throw new Error(`${f.out}: SHA-256 mismatch after extract`);
  console.log(`fetched ${f.out}`);
}
