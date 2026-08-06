import { copyFile, mkdir, readFile, readdir, writeFile } from "node:fs/promises";
import { basename, join, resolve } from "node:path";

interface TauriConfig {
  version: string;
}

const arg = (name: string): string | undefined => {
  const index = process.argv.indexOf(name);
  return index >= 0 ? process.argv[index + 1] : undefined;
};

const findInstaller = async (): Promise<string> => {
  const explicit = arg("--installer");
  if (explicit) return resolve(explicit);
  const directory = resolve("src-tauri/target/release/bundle/nsis");
  const entries = await readdir(directory);
  const installers = entries.filter((entry) => entry.endsWith("-setup.exe"));
  if (installers.length !== 1) {
    throw new Error(
      `Expected exactly one NSIS installer in ${directory}; found ${installers.length}. Pass --installer <path>.`,
    );
  }
  return join(directory, installers[0]);
};

const config = JSON.parse(
  await readFile("src-tauri/tauri.conf.json", "utf8"),
) as TauriConfig;
const installer = await findInstaller();
const signaturePath = `${installer}.sig`;
const signature = (await readFile(signaturePath, "utf8")).trimEnd();
// Tauri writes the .sig BASE64-ENCODED; the minisign text lives inside it, and
// latest.json expects that base64 string verbatim. Validate the DECODED form.
let decodedSignature: string;
try {
  decodedSignature = Buffer.from(signature, "base64").toString("utf8");
} catch {
  decodedSignature = "";
}
if (
  !decodedSignature.includes("untrusted comment:") ||
  !decodedSignature.includes("trusted comment:")
) {
  throw new Error(`${signaturePath} does not contain a complete minisign signature`);
}

const outputDirectory = resolve(
  arg("--output") ?? "src-tauri/target/release-artifacts",
);
const assetName = `Handy.Tool_${config.version}_x64-setup.exe`;
const assetUrl = `https://github.com/patrick-1984/handy-tool/releases/download/v${config.version}/${assetName}`;
const platform = { url: assetUrl, signature };
const manifest = {
  version: config.version,
  notes: arg("--notes") ?? `Handy Tool ${config.version}`,
  pub_date: new Date().toISOString(),
  platforms: {
    "windows-x86_64-nsis": platform,
    "windows-x86_64": platform,
  },
};

await mkdir(outputDirectory, { recursive: true });
await copyFile(installer, join(outputDirectory, assetName));
await copyFile(signaturePath, join(outputDirectory, `${assetName}.sig`));
await writeFile(
  join(outputDirectory, "latest.json"),
  `${JSON.stringify(manifest, null, 2)}\n`,
  "utf8",
);

console.log(`Prepared updater assets from ${basename(installer)} in ${outputDirectory}`);
console.log(`Upload ${assetName}, ${assetName}.sig, and latest.json to release v${config.version}.`);
