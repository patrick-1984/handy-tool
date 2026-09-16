import {
  copyFile,
  mkdir,
  readFile,
  readdir,
  writeFile,
} from "node:fs/promises";
import { existsSync } from "node:fs";
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
  // src-tauri/.cargo/config.toml sets target-dir = "C:/tmp/hb", so the default
  // cargo path does not exist on this repo and this used to fail with ENOENT.
  // Honour CARGO_TARGET_DIR, fall back to the configured dir, then to cargo's default.
  const targetDir =
    process.env.CARGO_TARGET_DIR ??
    (existsSync("C:/tmp/hb") ? "C:/tmp/hb" : "src-tauri/target");
  const directory = resolve(targetDir, "release/bundle/nsis");
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
  throw new Error(
    `${signaturePath} does not contain a complete minisign signature`,
  );
}

// The installer's filename must carry the version we are about to publish. Tauri
// signs BYTES, not names - so a stale installer copied to the new asset name yields
// a VALID signature on the wrong binary, and every client silently "updates" to it
// and then re-offers the same update forever. The build dir legitimately holds every
// past release, so this is one bad --installer away at all times.
// Exact token match, not `includes`: a substring test passes "11.6.0" for version
// "1.6.0", and also passes any file that merely mentions the version anywhere in its
// name. The installer name is `Handy Tool_<version>_x64-setup.exe`, so pull the
// version out and compare it whole.
const installerVersion = basename(installer).match(/_(\d+\.\d+\.\d+)_/)?.[1];
if (installerVersion !== config.version) {
  throw new Error(
    `Installer ${basename(installer)} carries version ${installerVersion ?? "<unparseable>"}, not ${config.version} from tauri.conf.json. ` +
      `Refusing to publish a manifest that would point at the wrong binary. ` +
      `Pass the right --installer, or bump the version first.`,
  );
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

console.log(
  `Prepared updater assets from ${basename(installer)} in ${outputDirectory}`,
);
console.log(
  `Upload ${assetName}, ${assetName}.sig, and latest.json to release v${config.version}.`,
);
