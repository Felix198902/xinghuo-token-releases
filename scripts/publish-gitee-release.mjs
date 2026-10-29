#!/usr/bin/env node

import { readdirSync, readFileSync, writeFileSync } from "node:fs";
import path from "node:path";

const [assetDirectory, tag] = process.argv.slice(2);
const token = process.env.GITEE_TOKEN;
const owner = process.env.GITEE_OWNER;
const repo = process.env.GITEE_REPO;

if (!assetDirectory || !tag || !token || !owner || !repo) {
  console.error(
    "Usage: GITEE_TOKEN=... GITEE_OWNER=... GITEE_REPO=... node scripts/publish-gitee-release.mjs <asset-directory> <tag>",
  );
  process.exit(1);
}

const apiRoot = `https://gitee.com/api/v5/repos/${owner}/${repo}`;
const downloadBase = `https://gitee.com/${owner}/${repo}/releases/download/${tag}`;

async function request(endpoint, options = {}) {
  const separator = endpoint.includes("?") ? "&" : "?";
  const response = await fetch(
    `${apiRoot}${endpoint}${separator}access_token=${encodeURIComponent(token)}`,
    options,
  );
  if (!response.ok) {
    const detail = await response.text();
    throw new Error(
      `Gitee API ${options.method ?? "GET"} failed (${response.status}): ${detail}`,
    );
  }
  return response.status === 204 ? null : response.json();
}

async function findOrCreateRelease() {
  const response = await fetch(
    `${apiRoot}/releases/tags/${encodeURIComponent(tag)}?access_token=${encodeURIComponent(token)}`,
  );
  if (response.ok) return response.json();
  if (response.status !== 404) {
    throw new Error(`Unable to read Gitee release (${response.status})`);
  }
  return request("/releases", {
    method: "POST",
    headers: { "Content-Type": "application/x-www-form-urlencoded" },
    body: new URLSearchParams({
      tag_name: tag,
      name: `星火无限token ${tag}`,
      body: `星火无限token ${tag}`,
      target_commitish: process.env.GITHUB_SHA || "custom",
      prerelease: "true",
    }),
  });
}

async function uploadFile(releaseId, filePath, existingNames, uploadName) {
  const name = uploadName ?? path.basename(filePath);
  if (existingNames.has(name)) {
    console.log(`Already present on Gitee: ${name}`);
    return;
  }
  const form = new FormData();
  form.append("file", new Blob([readFileSync(filePath)]), name);
  await request(`/releases/${releaseId}/attach_files`, {
    method: "POST",
    body: form,
  });
  existingNames.add(name);
  console.log(`Uploaded to Gitee: ${name}`);
}

const manifestPath = path.join(assetDirectory, "latest.json");
const manifest = JSON.parse(readFileSync(manifestPath, "utf8"));
const platforms = Object.entries(manifest.platforms ?? {});
if (platforms.length === 0)
  throw new Error("Updater manifest has no platforms");

for (const [platform, entry] of platforms) {
  if (typeof entry.url !== "string" || typeof entry.signature !== "string") {
    throw new Error(`${platform} is missing its updater URL or signature`);
  }
  const name = entry.url.split("/").pop();
  if (!name) throw new Error(`${platform} has an invalid updater URL`);
  entry.url = `${downloadBase}/${name}`;
}

const release = await findOrCreateRelease();
const existingNames = new Set(
  (release.assets ?? []).map((asset) => asset.name),
);
const artifactNames = readdirSync(assetDirectory).filter(
  (name) => !name.endsWith(".sig") && !name.endsWith(".json"),
);
for (const fileName of artifactNames) {
  await uploadFile(
    release.id,
    path.join(assetDirectory, fileName),
    existingNames,
  );
}

const giteeManifestPath = path.join(assetDirectory, "latest.gitee.json");
writeFileSync(giteeManifestPath, `${JSON.stringify(manifest, null, 2)}\n`);
await uploadFile(release.id, giteeManifestPath, existingNames, "latest.json");
console.log(
  `Gitee release ${tag} is ready with ${platforms.length} updater platforms`,
);
